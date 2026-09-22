//! SFTP 文件操作（列目录/读写/流式上传/流式下载/建删改名/stat）。
//! 从 ssh.rs 按域拆出（architecture-log Target 1），零逻辑变更。

use super::*;

/// 上传取消表条目：所属会话 + 取消旗标。
///
/// 流式上传（sftp_upload_from_file）是单条 invoke 跑全程，没有旧三件套的 writer
/// 任务可 abort；取消靠这面共享旗标——循环在块边界（≤1 MiB）看到即停。
/// 条目必须带 session_id：cleanup_session_tables 据此把「本会话的在途上传」
/// 全部置旗并摘除，避免会话销毁后旗标条目泄漏（transfer_id 不复用时无人再删）。
pub struct TransferCancelEntry {
    /// 该传输所属的 SSH 会话，`cleanup_session_tables` 据此筛出本会话的在途上传。
    pub session_id: String,
    /// 取消旗标：sftp_upload_cancel / 会话清理置位，上传循环逐块检查。
    pub flag: Arc<AtomicBool>,
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

// pub(crate)：编辑器文本链路（ssh::text_file）复用同一 SFTP 通道缓存
pub(crate) async fn get_or_create_sftp(
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

/// 流式上传：前端只传本机路径，后端读盘直写 SFTP，**字节不再经过 IPC**。
///
/// 替代旧三件套（`sftp_upload_start/chunk/finalize` + 前端 8 MiB 分块 invoke）：
/// 旧链路每块经 JSON 数字数组序列化约 4 倍膨胀（8 MiB 块 ≈ 32 MB 文本），
/// 前端 await 逐块串行还把所有传输压在一次事件循环里。本命令与
/// `sftp_download_to_file` 对称：单条 invoke 跑全程，进度经
/// `sftp-transfer-progress` 事件节流上报（常量与下载共用）。
///
/// 取消：`sftp_upload_cancel` 置共享旗标，循环在块边界（≤ UPLOAD_CHUNK_SIZE）
/// 看到即停。取消与中途失败都会**尽力删除远端半截文件**（对称下载侧的半截
/// 清理：一个「看起来完整」的截断文件比没有文件更危险）。
#[tauri::command]
pub async fn sftp_upload_from_file(
    state: State<'_, AppState>,
    session_id: String,
    local_path: String,
    remote_path: String,
    transfer_id: String,
) -> Result<(), String> {
    // lossy 守卫（create = O_CREAT|O_TRUNC 覆盖语义，分不清新建与覆盖——
    // 失真名可能静默覆盖字面含 U+FFFD 的另一个真实文件）
    if myshelltool_core::is_lossy_remote_path(&remote_path) {
        return Err(myshelltool_core::lossy_remote_path_error(&remote_path));
    }
    // 本机路径走 fs_local 同一入口：系统目录黑名单（is_sensitive_path 单点事实源）
    // 与 ~ 展开/规范化语义和文件面板读取完全一致。
    let local = crate::fs_local::resolve_input_path(&local_path)?;
    let meta = std::fs::symlink_metadata(&local)
        .map_err(|e| format!("stat local file failed for {}: {e}", local.display()))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(format!("not a regular file: {}", local.display()));
    }
    let mut local_file = tokio::fs::File::open(&local)
        .await
        .map_err(|e| format!("open local file failed for {}: {e}", local.display()))?;
    // total 取打开时刻的长度，仅作进度条分母：文件在传输期间增长不截断（读到 EOF
    // 为止），与旧分块契约（短块 = EOF，不信任既有 size）一致。
    let total = local_file
        .metadata()
        .await
        .map(|m| m.len())
        .unwrap_or(meta.len());

    // 时序不能变：先在 sftp_arc 锁内建好远端文件（create 有一次远端往返），
    // **之后**才取 ssh_sessions 锁登记取消条目。
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let mut remote_file = {
        let sftp = sftp_arc.lock().await;
        sftp.create(&remote_path)
            .await
            .map_err(|e| format!("SFTP create failed: {e}"))?
    };

    // 取消登记 + 重复 id 查重 + 会话存活校验在**同一临界区**（中间无 await）。
    // 必须校验会话存活：上面 create 的远端往返期间 cleanup_session_tables 完全可能
    // 已把整个会话拆掉；若登记到一个已销毁的会话上，该会话的清理早已跑完，
    // 这条取消条目将永远不会被任何清理碰到（泄漏到进程结束）。
    let flag = Arc::new(AtomicBool::new(false));
    {
        let ssh_mgr = state.ssh_sessions.lock().await;
        let mut cancels = ssh_mgr.transfer_cancels.lock().await;
        if cancels.contains_key(&transfer_id) {
            return Err(format!("transfer_id {transfer_id} already in progress"));
        }
        if !ssh_mgr.sessions.contains_key(&session_id) {
            return Err(format!("session {session_id} closed during upload start"));
        }
        cancels.insert(
            transfer_id.clone(),
            TransferCancelEntry {
                session_id: session_id.clone(),
                flag: flag.clone(),
            },
        );
    }

    let app = { state.ssh_sessions.lock().await.app.clone() };

    // 主体放进 async 块：取消/失败统一走下面的清理分支（删远端半截文件）。
    let transfer = async {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut buf = vec![0u8; UPLOAD_CHUNK_SIZE];
        let mut written: u64 = 0;
        let mut last_emit = tokio::time::Instant::now();
        let mut last_emit_bytes: u64 = 0;

        loop {
            // 取消检查在块边界：一个块最多 UPLOAD_CHUNK_SIZE，取消延迟有上界。
            if flag.load(Ordering::SeqCst) {
                return Err("upload cancelled by user".to_string());
            }
            let n = local_file
                .read(&mut buf)
                .await
                .map_err(|e| format!("read local file failed for {}: {e}", local.display()))?;
            if n == 0 {
                break; // 真 EOF
            }
            // 单次写入 120s 上限：死连接（半开 TCP）上 write_all 永不返回
            // （旧 writer 任务同一契约，随流式化平移）。
            match tokio::time::timeout(WRITE_TIMEOUT, remote_file.write_all(&buf[..n])).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(format!("SFTP write failed: {e}")),
                Err(_) => {
                    return Err(format!(
                        "SFTP write timeout ({}s): remote not responding",
                        WRITE_TIMEOUT.as_secs()
                    ))
                }
            }
            written += n as u64;

            // 节流（与下载同口径）：满 1 MiB 且满 200ms 才发；写完必发（收尾）。
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

        // 收尾必发一次（尾部不足阈值时的最终进度）。
        let _ = app.emit(
            "sftp-transfer-progress",
            TransferProgressEvent {
                transfer_id: transfer_id.clone(),
                bytes_transferred: written,
                total_bytes: total,
            },
        );

        // 显式 shutdown 并回传真实结果（旧 finalize 的 120s 等 ack 契约随流式化
        // 平移为本地的 60s 上限）：远端不应答时不能让这条 invoke 永久挂住。
        match tokio::time::timeout(FLUSH_TIMEOUT, remote_file.shutdown()).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(format!("SFTP flush failed: {e}")),
            Err(_) => Err(format!(
                "SFTP flush timeout ({}s): remote not draining write acks",
                FLUSH_TIMEOUT.as_secs()
            )),
        }
    }
    .await;

    // 摘取消条目：只摘「还是自己这一条」的（Arc 同一性比对）——取消已在
    // cleanup_session_tables 摘除（会话断开）时不误删后续同 id 的新传输。
    {
        let ssh_mgr = state.ssh_sessions.lock().await;
        let mut cancels = ssh_mgr.transfer_cancels.lock().await;
        if cancels
            .get(&transfer_id)
            .is_some_and(|entry| Arc::ptr_eq(&entry.flag, &flag))
        {
            cancels.remove(&transfer_id);
        }
    }

    if let Err(e) = transfer {
        // 先释放远端文件句柄再删半截文件（drop 不向远端等待应答，顺序仅为清晰）。
        drop(remote_file);
        // 尽力删除远端半截文件：不删的话远端会留下一个**看起来完整**的截断文件，
        // 下次有人打开才发现内容不全，且无从分辨是上传失败还是源文件本身损坏。
        // 删除失败只记日志不上报——真正的失败原因（e）比清理失败更值得占用错误串。
        let sftp = sftp_arc.lock().await;
        if let Err(cleanup_err) = sftp.remove_file(&remote_path).await {
            warn!("sftp upload cleanup failed for {remote_path}: {cleanup_err}");
        }
        return Err(e);
    }

    Ok(())
}

/// 取消在途上传：置共享旗标，上传循环在下一个块边界停止并清理远端半截文件。
///
/// 找不到条目**不报错**：传输可能刚好已完成/已被会话清理摘掉，取消是幂等的
/// 尽力而为——前端队列状态由 runPathUpload 按 invoke 的最终结果收敛。
#[tauri::command]
pub async fn sftp_upload_cancel(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    let ssh_mgr = state.ssh_sessions.lock().await;
    let cancels = ssh_mgr.transfer_cancels.lock().await;
    if let Some(entry) = cancels.get(&transfer_id) {
        entry.flag.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// v0.20（S9）：下载取消——与上传共用 transfer_cancels 表（同一旗标通道，
/// 前端只需记一个命令名）。幂等：条目不存在不报错（传输已结束/已被清理）。
#[tauri::command]
pub async fn sftp_download_cancel(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    sftp_upload_cancel(state, transfer_id).await
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
/// 流式上传的本地读块大小：russh-sftp 的 write_all 内部按 max_concurrent_writes
/// 窗口流水线发 SSH_FXP_WRITE，1 MiB 块兼顾吞吐与取消响应（旗标在块边界检查）。
const UPLOAD_CHUNK_SIZE: usize = 1024 * 1024;
/// 单次 SFTP 写入上限：死连接（半开 TCP）上 write_all 永不返回。
const WRITE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
/// 上传收尾（shutdown 排空 write ack）上限：半开连接上这个等待理论无界。
const FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

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

    // v0.20（S9）：下载取消旗标——与上传共用 transfer_cancels 表（块级检查点
    // 在循环内 read 之后，≤ DOWNLOAD_CHUNK_SIZE=64KiB 粒度）。登记/退出/清理
    // 语义与上传完全同构：重复 transfer_id 拒绝（防串号）、finally 移除条目、
    // 会话清理置旗（cleanup_session_tables 按 session_id 筛）。
    let cancel = {
        let ssh_mgr = state.ssh_sessions.lock().await;
        let mut cancels = ssh_mgr.transfer_cancels.lock().await;
        if cancels.contains_key(&transfer_id) {
            return Err(format!("transfer {transfer_id} already in flight"));
        }
        let flag = Arc::new(AtomicBool::new(false));
        cancels.insert(
            transfer_id.clone(),
            TransferCancelEntry {
                session_id: session_id.clone(),
                flag: flag.clone(),
            },
        );
        flag
    };

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
            // v0.20（S9）：取消检查点（块边界 ≤ 64 KiB，与上传同粒度）
            if cancel
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                return Err("download cancelled".to_string()); // 走统一清理分支删半截文件
            }
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
        // v0.20（S9）：取消路径的条目移除 + 错误语义——「取消」对前端不是 error
        // （上传侧由 runPathUpload 按 cancelled 标记收敛，此处对齐）
        {
            let ssh_mgr = state.ssh_sessions.lock().await;
            let mut cancels = ssh_mgr.transfer_cancels.lock().await;
            cancels.remove(&transfer_id);
        }
        if e == "download cancelled" {
            return Err("[download:cancelled]".to_string()); // 前端按前缀识别取消态
        }
        return Err(e);
    }

    // 成功路径同样移除条目（finally 语义）
    {
        let ssh_mgr = state.ssh_sessions.lock().await;
        let mut cancels = ssh_mgr.transfer_cancels.lock().await;
        cancels.remove(&transfer_id);
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

/// v0.20（SSH P2）：chmod——russh-sftp setstat 只改 permissions 位。
/// mode 输入是八进制字符串（如 "644"/"0755"）——前置解析校验（仅 0-7 数字，
/// 3-4 位；4 位时首位须 0——不静默截断错误输入）。高危：Strict 审批之外还应有
/// GUI 确认（远程列表面板右键入口带确认弹窗）。
#[tauri::command]
pub async fn sftp_chmod(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    mode: String,
) -> Result<(), String> {
    if myshelltool_core::is_lossy_remote_path(&path) {
        return Err(myshelltool_core::lossy_remote_path_error(&path));
    }
    let trimmed = mode.trim();
    if trimmed.len() < 3 || trimmed.len() > 4 || !trimmed.chars().all(|c| ('0'..='7').contains(&c)) {
        return Err(format!("权限格式无效：{mode:?}（期望 3-4 位八进制，如 644 或 0755）"));
    }
    if trimmed.len() == 4 && !trimmed.starts_with('0') {
        return Err(format!("权限格式无效：{mode:?}（4 位时首位须 0，setuid/setgid/sticky 位暂不支持）"));
    }
    let perms = u32::from_str_radix(trimmed, 8)
        .map_err(|e| format!("权限解析失败: {e}"))?;
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;
    let attrs = russh_sftp::protocol::FileAttributes {
        permissions: Some(perms),
        ..Default::default()
    };
    sftp.set_metadata(&path, attrs)
        .await
        .map_err(|e| format!("SFTP chmod 失败: {e}"))
}

/// v0.20（SSH P2）：readlink——返回符号链接的目标路径（协议 raw Name）。
#[tauri::command]
pub async fn sftp_readlink(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<String, String> {
    if myshelltool_core::is_lossy_remote_path(&path) {
        return Err(myshelltool_core::lossy_remote_path_error(&path));
    }
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;
    sftp.read_link(&path).await.map_err(|e| format!("SFTP readlink 失败: {e}"))
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
