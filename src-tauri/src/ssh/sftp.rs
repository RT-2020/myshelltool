//! SFTP 文件操作（列目录/读写/分块上传/流式下载/建删改名/stat）。
//! 从 ssh.rs 按域拆出（architecture-log Target 1），零逻辑变更。

use super::*;

/// 上传 writer 任务的帧：数据块，或「收尾并回传 flush 结果」的控制帧。
///
/// 为什么不能只用 `Vec<u8>`：收尾必须能把 `shutdown()` 的**真实**错误回传给调用方
/// ——`send()` 成功只证明帧进了队列，不代表数据已落盘（写失败发生在任务侧），
/// 所以控制帧需要一个一次性回传通道把 `Result` 送回 `sftp_upload_finalize`。
enum UploadFrame {
    Data(Vec<u8>),
    /// 收尾：writer 收到后 `shutdown()`，把结果经该 sender 回传，然后退出循环。
    Finish(oneshot::Sender<Result<(), String>>),
}

/// 上传传输表里的一条：发帧用的 sender + 该任务的身份/终止手段。
///
/// 表里只存 sender 是不够的：任务若正阻塞在 `write_all`（远端不应答），
/// drop sender 并不能打断它（任务自持一份 sender，通道不会关闭），远端 SFTP
/// channel 会一直被占着（每条吃一个 OpenSSH `MaxSessions` 配额）。因此同时留下
/// `session_id`（供清理时筛出本会话）与 `JoinHandle`（供 `abort()` 真正终止任务
/// → drop `File` → 关闭 channel）。
///
/// **为何句柄是必填而非可选槽位**：任务一旦存在，就必须能被打扫——所以句柄在
/// 「插入表项」这一个临界区内就随条目一起写入，不存在「任务已 spawn 但句柄还没
/// 登记」的中间态。为此 `sftp_upload_start` 的次序是「先 spawn 拿句柄 → 再加锁
/// 查重并插入」；命中重复 id 时 abort 掉刚 spawn 的任务，而不是留下孤儿。
pub struct UploadEntry {
    /// 该传输所属的 SSH 会话，`cleanup_session_tables` 据此终止本会话的在途上传。
    pub session_id: String,
    tx: tokio::sync::mpsc::Sender<UploadFrame>,
    /// writer 任务的句柄，供 `cleanup_session_tables` abort（见上）。
    pub handle: tokio::task::JoinHandle<()>,
}

/// SFTP DirEntry → RemoteFileEntry 共享映射（sftp_list_dir 与
/// ssh_list_directory 两处复用，保持字段语义一致）。
pub fn dir_entry_to_remote_file_entry(entry: russh_sftp::client::fs::DirEntry) -> RemoteFileEntry {
    let meta = entry.metadata();
    let kind = if meta.is_dir() {
        "directory"
    } else if meta.is_symlink() {
        "symlink"
    } else {
        "file"
    }
    .to_string();
    // 权限 u32 → 八进制串（过滤文件类型位，只留权限位，如 "0755"）。
    // user/group 来自 SFTP 长名解析，部分 server 不提供（None）。
    let permissions = meta.permissions.map(|p| format!("{:04o}", p & 0o7777));
    RemoteFileEntry {
        name: entry.file_name(),
        path: entry.path(),
        kind,
        size: meta.len(),
        // Unix 秒字符串（复用 fs_local::format_modified）：Debug 格式会输出
        // "Ok(SystemTime { .. })" 直接透给前端，两条路径必须一致。
        modified: crate::fs_local::format_modified(meta.modified()),
        permissions,
        user: meta.user.clone(),
        group: meta.group.clone(),
    }
}

// --- SFTP operations ---

async fn get_or_create_sftp(
    state: &State<'_, AppState>,
    session_id: &str,
) -> Result<Arc<Mutex<SftpSession>>, String> {
    {
        let mgr = state.ssh_sessions.lock().await;
        if let Some(sftp) = mgr.sftp_cache.get(session_id) {
            return Ok(sftp.clone());
        }
    }

    let handle = {
        let mgr = state.ssh_sessions.lock().await;
        mgr.ssh_handles
            .get(session_id)
            .ok_or_else(|| format!("SSH handle for session {session_id} not found"))?
            .clone() // Arc::clone
    };

    // SFTP 建链（channel open → subsystem → 协议握手）全程加超时：
    // 这三步都没有任何内建超时，半开 TCP 上会永久挂起，前端 invoke 永不返回。
    // 一次性 15s 上限覆盖整段建链（而非每步各 15s），避免最坏情况下累积成 45s。
    const SFTP_INIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
    let init = async {
        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| format!("SFTP channel open failed: {e}"))
            .inspect_err(|m| error!("sftp init (session {session_id}): {m}"))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| format!("SFTP subsystem request failed: {e}"))
            .inspect_err(|m| error!("sftp init (session {session_id}): {m}"))?;
        SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| format!("SFTP session init failed: {e}"))
            .inspect_err(|m| error!("sftp init (session {session_id}): {m}"))
    };

    let sftp = match tokio::time::timeout(SFTP_INIT_TIMEOUT, init).await {
        Ok(Ok(sftp)) => sftp,
        Ok(Err(e)) => {
            evict_sftp_cache(state, session_id).await;
            return Err(e);
        }
        Err(_) => {
            // 建链超时（半开连接最典型）。淘汰缓存：不淘汰的话该 session 之后每次
            // SFTP 请求都会命中这个死条目，一路失败到用户手动断开为止。
            let m = format!(
                "SFTP init timeout ({}s): session {session_id}（连接可能已半开，请重连）",
                SFTP_INIT_TIMEOUT.as_secs()
            );
            error!("sftp init (session {session_id}): {m}");
            evict_sftp_cache(state, session_id).await;
            return Err(m);
        }
    };

    info!("SFTP session initialized for session {session_id}");

    let arc = Arc::new(Mutex::new(sftp));
    {
        let mut mgr = state.ssh_sessions.lock().await;
        mgr.sftp_cache.insert(session_id.to_string(), arc.clone());
    }
    Ok(arc)
}

/// 淘汰某 session 的 SFTP 缓存条目（建链失败 / 超时后调用）。
///
/// 独立性：`sftp_cache` 只在 `ssh_disconnect` 里被清理，而远端掉线不必然触发
/// `ssh_disconnect`。死条目留在缓存里会让后续每个请求都命中同一具尸体，
/// 用户必须手动断开才能恢复。
async fn evict_sftp_cache(state: &State<'_, AppState>, session_id: &str) {
    let mut mgr = state.ssh_sessions.lock().await;
    mgr.sftp_cache.remove(session_id);
}

#[tauri::command]
pub async fn sftp_list_dir(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<RemoteDirectoryList, String> {
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;

    // 空 path = 服务器默认目录：SFTP 服务进程 cwd 起始于登录用户家目录，
    // canonicalize(".") 解析出真实绝对路径。此前前端猜 /home/<username>，
    // 对 root（家在 /root）等非标准布局必错，报 "SFTP read_dir failed: No such file"。
    let requested_path = if path.trim().is_empty() {
        match sftp.canonicalize(".").await {
            Ok(resolved) => resolved,
            Err(e) => {
                let m = format!("SFTP canonicalize failed: {e}");
                error!("sftp_list_dir (session {session_id}): {m}");
                return Err(m);
            }
        }
    } else {
        path
    };

    let raw_entries = sftp
        .read_dir(&requested_path)
        .await
        .map_err(|e| format!("SFTP read_dir failed: {e}"))
        .inspect_err(|m| error!("sftp_list_dir (session {session_id}, path {requested_path}): {m}"))?;

    let mut entries: Vec<RemoteFileEntry> = raw_entries
        .into_iter()
        .map(dir_entry_to_remote_file_entry)
        .collect();

    entries.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));

    Ok(RemoteDirectoryList {
        host: String::new(),
        path: requested_path,
        entries,
    })
}

#[tauri::command]
pub async fn sftp_read_file(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<String, String> {
    // lossy 文件名守卫：见 core::remote_text::is_lossy_remote_path（按名寻址失真
    // 名可能 No such file，也可能命中字面含 U+FFFD 的另一个真实文件）
    if myshelltool_core::is_lossy_remote_path(&path) {
        return Err(myshelltool_core::lossy_remote_path_error(&path));
    }
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;

    let mut file = sftp
        .open(&path)
        .await
        .map_err(|e| format!("SFTP open failed: {e}"))?;

    use tokio::io::AsyncReadExt;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .await
        .map_err(|e| format!("SFTP read failed: {e}"))?;

    Ok(contents)
}

#[tauri::command]
pub async fn sftp_write_file(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    content: String,
) -> Result<(), String> {
    // lossy 守卫（create = O_CREAT|O_TRUNC 覆盖语义，分不清新建与覆盖——
    // 失真名可能静默覆盖字面含 U+FFFD 的另一个真实文件，多角色审查 Issue 3）
    if myshelltool_core::is_lossy_remote_path(&path) {
        return Err(myshelltool_core::lossy_remote_path_error(&path));
    }
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;

    let mut file = sftp
        .create(&path)
        .await
        .map_err(|e| format!("SFTP create failed: {e}"))?;

    use tokio::io::AsyncWriteExt;
    file.write_all(content.as_bytes())
        .await
        .map_err(|e| format!("SFTP write failed: {e}"))?;
    file.shutdown()
        .await
        .map_err(|e| format!("SFTP flush failed: {e}"))?;

    Ok(())
}

#[derive(Clone, Serialize)]
struct TransferProgressEvent {
    transfer_id: String,
    bytes_transferred: u64,
    total_bytes: u64,
}

#[tauri::command]
pub async fn sftp_upload_start(
    state: State<'_, AppState>,
    session_id: String,
    remote_path: String,
    transfer_id: String,
) -> Result<(), String> {
    // lossy 守卫（同 sftp_write_file：create 的覆盖语义分不清新建与覆盖）
    if myshelltool_core::is_lossy_remote_path(&remote_path) {
        return Err(myshelltool_core::lossy_remote_path_error(&remote_path));
    }
    // 时序不能变：先在 sftp_arc 锁内建好远端文件，**之后**才取 ssh_sessions 锁。
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;
    let file = sftp
        .create(&remote_path)
        .await
        .map_err(|e| format!("SFTP create failed: {e}"))?;
    drop(sftp);

    // 任务里拿不到 AppState，所以预先 clone 出 Arc 与 id。
    let sessions_arc = state.ssh_sessions.clone();
    let task_transfer_id = transfer_id.clone();

    // 有界通道（容量 4）形成背压：慢链路上 chunk 会在 send 处等待，而不是在内存里
    // 无上限堆积成 OOM。
    let (tx, mut rx) = tokio::sync::mpsc::channel::<UploadFrame>(4);
    // 任务自持一份 sender（用于退出时比对「表项还是不是自己这一条」）。
    let task_tx = tx.clone();

    // **先 spawn、再在同一临界区里查重并插入**，理由：
    // - 句柄只能由 spawn 产生，而表项必须从一开始就带着句柄 —— 否则会出现「任务已
    //   存在但清理侧拿不到句柄」的窗口（历史实现用空槽位缓解，但「复核归属」与
    //   「填槽」仍在两次加锁之间，中间照样能被 cleanup 插进来，留下永不退出的孤儿）。
    // - 先 spawn 的唯一代价是「命中重复 id 时要自己收尾」：所以下面那个分支会
    //   abort 掉刚 spawn 的任务，而不是像最初版本那样直接 return 泄漏一个孤儿。
    let writer_handle = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        let mut file = file;
        // 单次写入 120s 上限：死连接（半开 TCP）上 write_all 永不返回，没有这个
        // 时限任务会永久挂住并一直占着远端 channel。
        const WRITE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
        // 收尾上限（更短）：`shutdown()` 要把排队的 write ack 全部排空，半开 TCP 下
        // 这个等待理论无界。它由 `finalize` 的 oneshot 直接等（见 sftp_upload_finalize
        // 的 120s 上限），若不在这里收口，任务会在 finalize 超时之后继续占着 File 与
        // 远端 channel。60s 给「慢链路但活着」的连接留足余量。
        const FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
        while let Some(frame) = rx.recv().await {
            match frame {
                UploadFrame::Data(bytes) => {
                    let write = file.write_all(&bytes);
                    match tokio::time::timeout(WRITE_TIMEOUT, write).await {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => {
                            // 写失败后这条传输无法继续：结束任务（下面的自移除会清掉表项），
                            // 由前端收到错误后重新发起。
                            error!(
                                "sftp upload {task_transfer_id}: write failed: {e}; aborting writer task"
                            );
                            break;
                        }
                        Err(_) => {
                            error!(
                                "sftp upload {task_transfer_id}: write timed out after {}s; aborting writer task",
                                WRITE_TIMEOUT.as_secs()
                            );
                            break;
                        }
                    }
                }
                UploadFrame::Finish(resp) => {
                    // 收尾必须回传 shutdown() 的真实结果：send() 成功只代表入队。
                    // 外面再包一层超时：半开连接上排空 write ack 可能永不完成，
                    // 没有它这个任务会一直占着 File 与远端 channel（见 FLUSH_TIMEOUT）。
                    let result = match tokio::time::timeout(FLUSH_TIMEOUT, file.shutdown()).await {
                        Ok(Ok(())) => Ok(()),
                        Ok(Err(e)) => Err(format!("SFTP flush failed: {e}")),
                        Err(_) => Err(format!(
                            "SFTP flush timeout ({}s): remote not draining write acks",
                            FLUSH_TIMEOUT.as_secs()
                        )),
                    };
                    let _ = resp.send(result);
                    break;
                }
            }
        }
        // 已经 shutdown 过的 File 再 drop 是幂等的。走到这里有两种情况：
        // 1) 正常的 Finish 收尾；2) 通道关闭（`rx.recv()` 返回 None）—— 只会发生在
        //    表项被移除（finalize 取走）或被 abort 之后，此时 drop(file) 关闭远端 channel。
        drop(file);

        // 自移除：任务退出时必须把表项摘掉，否则 upload_files 会随每次上传泄漏条目
        // （写失败的任务、以及前端忘了 finalize 的传输都会留下垃圾）。
        // 这条路径只取内层 upload_files 锁（不经全局 ssh_sessions 锁的方式见下）：
        // 顺序与全仓一致（先 ssh_sessions 后 upload_files），不构成死锁环。
        let mgr = sessions_arc.lock().await;
        let mut uploads = mgr.upload_files.lock().await;
        // 只摘「还是自己这一条」的：同一个 transfer_id 可能已被后续上传重新占用，
        // 直接 remove 会把新传输的 sender 删掉。
        // （tokio::sync::mpsc::Sender::same_channel 自 tokio 1.36 起可用，本仓库锁定 1.52。）
        if uploads
            .get(&task_transfer_id)
            .is_some_and(|entry| entry.tx.same_channel(&task_tx))
        {
            uploads.remove(&task_transfer_id);
        }
    });

    // 查重、**会话存活校验**、插入在**同一次持锁期间**完成（中间无 await），
    // 因此「插入成功」才是这条传输真正的提交点：在此之前它不属于任何会话表，
    // 在此之后它必定带着可 abort 的句柄、且所属会话仍然存在。
    //
    // 为什么必须校验会话存活：`create()` 那一步要跑一次远端往返（可能数百毫秒到
    // 数秒），这期间 `cleanup_session_tables` 完全可能把整个会话拆掉（远端 EOF 的
    // PTY 自清理、或用户点断开）。若此处只查 transfer_id 重复就插入，条目会挂在
    // 一个**已被销毁**的会话上：session_id 是 uuid v4 不会复用，此后没有任何一次
    // cleanup 会再匹配到它（该会话的清理早已跑完，而全局清理永不遍历旧 id），
    // writer 又自持 sender（`rx.recv()` 永不返回 None）→ 永久阻塞并占住远端 channel。
    {
        let ssh_mgr = state.ssh_sessions.lock().await;
        let session_alive = ssh_mgr.sessions.contains_key(&session_id);
        let mut uploads = ssh_mgr.upload_files.lock().await;
        // 重复 id（前端在「重试」弹出后抢跑）：收掉刚 spawn 的任务再报错。
        // abort → drop `file` → 关闭刚 create 的远端文件句柄；本地的 tx/rx 也随栈释放。
        // 注意重复调用**已经**在更早处 `create` 过一次（truncate），属既存行为。
        if uploads.contains_key(&transfer_id) {
            writer_handle.abort();
            return Err(format!("transfer_id {transfer_id} already in progress"));
        }
        if !session_alive {
            writer_handle.abort();
            return Err(format!("session {session_id} closed during upload start"));
        }
        uploads.insert(
            transfer_id,
            UploadEntry {
                session_id,
                tx,
                handle: writer_handle,
            },
        );
        // 作用域到此结束：两把锁都在这里释放，绝不跨越任何 await。
    }
    Ok(())
}

#[tauri::command]
pub async fn sftp_upload_chunk(
    state: State<'_, AppState>,
    _session_id: String,
    chunk: Vec<u8>,
    transfer_id: String,
    bytes_transferred: u64,
    total_bytes: u64,
) -> Result<(), String> {
    // 锁内只 clone 出 sender 与 app，**不做任何 IO**。
    let (tx, app) = {
        let ssh_mgr = state.ssh_sessions.lock().await;
        let app = ssh_mgr.app.clone();
        let uploads = ssh_mgr.upload_files.lock().await;
        let entry = uploads
            .get(&transfer_id)
            .ok_or_else(|| format!("transfer_id {transfer_id} not started"))?;
        (entry.tx.clone(), app)
        // 两把锁在此 drop；send 在锁外 await。
    };

    tx.send(UploadFrame::Data(chunk))
        .await
        .map_err(|_| format!("SFTP write failed: transfer task for {transfer_id} is gone"))?;

    let _ = app.emit(
        "sftp-transfer-progress",
        TransferProgressEvent {
            transfer_id,
            bytes_transferred,
            total_bytes,
        },
    );
    Ok(())
}

#[tauri::command]
pub async fn sftp_upload_finalize(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    // 取出即从表里摘掉：之后该 id 可以立刻被新上传复用，所以 writer 任务退出时的
    // 自移除必须容忍「键已不存在」（见 sftp_upload_start 里的 same_channel 判断）。
    let tx = {
        let ssh_mgr = state.ssh_sessions.lock().await;
        let mut uploads = ssh_mgr.upload_files.lock().await;
        uploads
            .remove(&transfer_id)
            .ok_or_else(|| format!("transfer_id {transfer_id} not started"))?
            .tx
    };

    // 收尾等真实结果，同样加超时：远端不应答时不能让这个 invoke 永久挂住。
    const FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
    let (resp_tx, resp_rx) = oneshot::channel::<Result<(), String>>();
    // send 失败 = writer 任务已退出（例如写失败后自杀），此时没人会回传结果。
    if tx.send(UploadFrame::Finish(resp_tx)).await.is_err() {
        return Err(format!(
            "SFTP flush failed: upload task for {transfer_id} is gone"
        ));
    }
    match tokio::time::timeout(FLUSH_TIMEOUT, resp_rx).await {
        Ok(Ok(Ok(()))) => Ok(()),
        Ok(Ok(Err(e))) => Err(e),
        // 回传通道被 drop = 任务异常结束（abort / panic），没拿到真实结论。
        Ok(Err(_)) => Err(format!(
            "SFTP flush failed: upload task for {transfer_id} ended unexpectedly"
        )),
        Err(_) => Err(format!(
            "SFTP flush failed: upload task for {transfer_id} timed out after {}s",
            FLUSH_TIMEOUT.as_secs()
        )),
    }
}

/// 分块读取远端文件并直接流式落盘到本地，**不再把文件内容经 IPC 返回前端**。
///
/// 为什么改名（旧名 `sftp_download_with_progress`）：旧实现的返回值是整份 `Vec<u8>`，
/// 经 Tauri IPC 以 JSON 数字数组序列化——1 GiB 文件膨胀成约 4 GiB 文本，前后端各驻留
/// 一份，必然 OOM 或长时间卡死；且 `_with_progress` 这个名字描述的是「顺带发进度」，
/// 会掩盖「返回值从内容变成 ()」这一破坏性变更。改名让旧调用方在编译/调用期立刻暴露。
///
/// 进度事件节流（见下方常量）：逐个 64 KiB 块 emit 会让 1 GiB 产生约 1.6 万条事件，
/// 每条都要跨 IPC 广播并 JSON 序列化，进度条既不更准、又把运行时拖卡。
const DOWNLOAD_CHUNK_SIZE: usize = 64 * 1024;
/// 距上次 emit 至少这么多字节才允许再发一次（小文件只发最终一条）。
const PROGRESS_EMIT_MIN_BYTES: u64 = 1024 * 1024;
/// 距上次 emit 至少这么久才允许再发一次（大文件按时间给反馈，避免块数决定事件数）。
const PROGRESS_EMIT_MIN_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

#[tauri::command]
pub async fn sftp_download_to_file(
    state: State<'_, AppState>,
    session_id: String,
    remote_path: String,
    local_path: String,
    transfer_id: String,
) -> Result<(), String> {
    // lossy 文件名守卫（同 sftp_read_file）：失真名要么 No such file，要么落到
    // 字面含 U+FFFD 的另一个真实文件——静默下到错内容比报错危险。
    if myshelltool_core::is_lossy_remote_path(&remote_path) {
        return Err(myshelltool_core::lossy_remote_path_error(&remote_path));
    }
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;

    // sftp_arc 的 Mutex 只覆盖「打开 + 取长度」这两次 DISPATCH 级请求，循环开始前就
    // 释放。SftpSession 的方法都取 &self，内部是 Arc<RawSftpSession>（并发请求由
    // 内部 req_id 表 + oneshot 支撑），而 open() 返回的 File 自己持有 Arc<RawSftpSession>
    // 而不借用 session。因此本函数读文件期间，列目录/上传/stat 等其它 SFTP 操作不会被
    // 这把锁排队（修复前整段下载期间持锁，前端表现为「列目录卡死」）。
    //
    // 顺序注意：先 drop 锁再做本地 IO（创建文件可能因磁盘慢/网络盘阻塞），锁的持有
    // 时间与本地磁盘性能解耦。metadata 与 open 的先后顺序不可倒置（下方注释说明）。
    let (mut file, total) = {
        let sftp = sftp_arc.lock().await;
        let file = sftp
            .open(&remote_path)
            .await
            .map_err(|e| format!("SFTP open failed: {e}"))?;
        // 用 fstat（文件句柄）而不是 sftp.metadata(路径)：句柄已经打开后再按路径 stat，
        // 若远端在两步之间把该路径替换成另一个文件（或 symlink 指向别处），
        // total_bytes 会描述一个与正在读的句柄无关的文件。
        let attrs = file
            .metadata()
            .await
            .map_err(|e| format!("SFTP stat failed: {e}"))?;
        (file, attrs.len())
        // sftp guard 在此 drop；file 不借用它，后续 read 循环不再持锁。
    };

    let app = { state.ssh_sessions.lock().await.app.clone() };
    let local = std::path::PathBuf::from(&local_path);

    // 主体放进 async 块：失败时统一走下面的清理分支，避免每一处 `?` 都复制一遍删除逻辑。
    let transfer = async {
        // 本地文件在循环外打开一次（create + truncate），全程持一个写句柄顺序追加。
        // 不每块都 open/seek/close：整份下载由本函数独占该文件，逐块重开既慢，
        // 也让「半截文件」的清理时机变得难以界定。
        let mut local_file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&local)
            .await
            .map_err(|e| format!("open local file failed for {}: {e}", local.display()))?;

        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut buf = vec![0u8; DOWNLOAD_CHUNK_SIZE];
        let mut written: u64 = 0;
        let mut last_emit = tokio::time::Instant::now();
        let mut last_emit_bytes: u64 = 0;

        loop {
            let n = file
                .read(&mut buf)
                .await
                .map_err(|e| format!("SFTP read chunk failed: {e}"))?;
            if n == 0 {
                break; // 真 EOF
            }
            local_file
                .write_all(&buf[..n])
                .await
                .map_err(|e| format!("write local file failed for {}: {e}", local.display()))?;
            written += n as u64;

            // 节流：距上次 emit 满 1 MiB**且**满 200ms 才发；读完必发（见下方收尾）。
            // 首个块不 emit（last_emit_bytes 初值 0 且 last_emit 为刚取的时间戳，
            // 两个条件都不满足），进度从第一次越过 1 MiB 才开始上报。
            let due_by_bytes = written - last_emit_bytes >= PROGRESS_EMIT_MIN_BYTES;
            let due_by_time = last_emit.elapsed() >= PROGRESS_EMIT_MIN_INTERVAL;
            if due_by_bytes && due_by_time {
                let _ = app.emit(
                    "sftp-transfer-progress",
                    TransferProgressEvent {
                        transfer_id: transfer_id.clone(),
                        bytes_transferred: written,
                        total_bytes: total,
                    },
                );
                last_emit = tokio::time::Instant::now();
                last_emit_bytes = written;
            }
        }

        // 收尾必发一次：否则最后不足 1 MiB / 不足 200ms 的尾巴永远收不到，
        // 前端进度条会停在 99% 直到 invoke resolve。
        let _ = app.emit(
            "sftp-transfer-progress",
            TransferProgressEvent {
                transfer_id: transfer_id.clone(),
                bytes_transferred: written,
                total_bytes: total,
            },
        );

        // 显式 flush + close：File 的 Drop 也会关句柄，但不等待对端应答，
        // 关失败会被静默丢弃——这里要把「落盘失败」当成下载失败报上去。
        local_file
            .flush()
            .await
            .map_err(|e| format!("flush local file failed for {}: {e}", local.display()))?;
        drop(local_file);
        Ok(())
    }
    .await;

    if let Err(e) = transfer {
        // 尽力删除半截文件：不删的话用户磁盘上会留下一个**看起来完整**的坏文件
        // （前端进度条可能已到 100%、文件名也对），下次打开才发现内容被截断，
        // 而用户根本无从判断是下载失败还是远端文件本身损坏。删除失败只记日志
        // 不上报——真正的失败原因（e）比清理失败更值得占用错误串。
        if let Err(cleanup_err) = tokio::fs::remove_file(&local).await {
            warn!(
                "sftp download cleanup failed for {}: {cleanup_err}",
                local.display()
            );
        }
        return Err(e);
    }

    Ok(())
}

#[tauri::command]
pub async fn sftp_mkdir(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<(), String> {
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;
    sftp.create_dir(&path)
        .await
        .map_err(|e| format!("SFTP mkdir failed: {e}"))
}

#[tauri::command]
pub async fn sftp_rename(
    state: State<'_, AppState>,
    session_id: String,
    old_path: String,
    new_path: String,
) -> Result<(), String> {
    // 只拦「寻址既有文件」的 old_path；new_path 是用户显式输入的新名字（v2.6 backlog #2）
    if myshelltool_core::is_lossy_remote_path(&old_path) {
        return Err(myshelltool_core::lossy_remote_path_error(&old_path));
    }
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;
    sftp.rename(&old_path, &new_path)
        .await
        .map_err(|e| format!("SFTP rename failed: {e}"))
}

#[tauri::command]
pub async fn sftp_remove(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    kind: String,
) -> Result<(), String> {
    // 删除是最高危操作：失真名可能误删「字面含 U+FFFD 的另一个真实文件」，fail-closed
    if myshelltool_core::is_lossy_remote_path(&path) {
        return Err(myshelltool_core::lossy_remote_path_error(&path));
    }
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;
    if kind == "directory" {
        sftp.remove_dir(&path).await
    } else {
        sftp.remove_file(&path).await
    }
    .map_err(|e| format!("SFTP remove failed: {e}"))
}

#[tauri::command]
pub async fn sftp_stat(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<RemoteFileEntry, String> {
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;
    let meta = sftp
        .metadata(&path)
        .await
        .map_err(|e| format!("SFTP stat failed: {e}"))?;

    let name = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&path)
        .to_string();

    Ok(RemoteFileEntry {
        name,
        path,
        kind: if meta.is_dir() {
            "directory"
        } else if meta.is_symlink() {
            "symlink"
        } else {
            "file"
        }
        .to_string(),
        size: meta.len(),
        // Unix 秒字符串（复用 fs_local::format_modified），与 sftp_list_dir 对齐
        modified: crate::fs_local::format_modified(meta.modified()),
        permissions: meta.permissions.map(|p| format!("{:04o}", p & 0o7777)),
        user: meta.user.clone(),
        group: meta.group.clone(),
    })
}
