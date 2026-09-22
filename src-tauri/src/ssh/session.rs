//! SSH 连接与终端会话（握手/认证/host-key/keyboard-interactive/PTY 命令）。
//! 从 ssh.rs 按域拆出（architecture-log Target 1），零逻辑变更。

use super::*;

pub struct SshClient {
    app: AppHandle,
    host_port: String,
    known_hosts_path: PathBuf,
    pending: PendingDecisions,
}

#[async_trait]
impl client::Handler for SshClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        info!(
            "check_server_key: enter for {}, known_hosts_path={}",
            self.host_port,
            self.known_hosts_path.display()
        );
        let key_bytes = server_public_key.public_key_bytes();
        let key_hex = bytes_to_hex(&key_bytes);
        let key_type = format!("{}", server_public_key.algorithm());
        let fingerprint = format!(
            "{}",
            server_public_key.fingerprint(russh::keys::ssh_key::HashAlg::Sha256)
        );
        info!(
            "check_server_key: {} presented key type={}, fingerprint={}",
            self.host_port, key_type, fingerprint
        );

        let known = load_known_hosts(&self.known_hosts_path);

        if let Some(entry) = known.get(&self.host_port) {
            if entry.key_hex == key_hex {
                info!("check_server_key: {} matched known_hosts, accepting", self.host_port);
                return Ok(true);
            }
            warn!(
                "check_server_key: {} key_hex mismatch (expected {}, got {})",
                self.host_port, entry.key_hex, key_hex
            );
        } else {
            info!("check_server_key: {} not in known_hosts, prompting user", self.host_port);
        }

        let request_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        {
            let mut map = self.pending.lock().await;
            map.insert(request_id.clone(), tx);
        }
        info!(
            "check_server_key: {} registered request_id={}, waiting for user",
            self.host_port, request_id
        );

        let is_changed = known.contains_key(&self.host_port);
        let event = HostKeyVerifyEvent {
            request_id: request_id.clone(),
            host_port: self.host_port.clone(),
            key_type: key_type.clone(),
            fingerprint,
            is_changed,
        };
        if let Err(e) = self.app.emit("ssh-host-key-verify", event) {
            warn!(
                "check_server_key: {} emit ssh-host-key-verify failed: {}",
                self.host_port, e
            );
            return Ok(false);
        }

        let accepted = match tokio::time::timeout(std::time::Duration::from_secs(60), rx).await {
            Ok(Ok(v)) => v,
            Ok(Err(_)) => {
                warn!(
                    "check_server_key: {} oneshot channel closed (frontend dropped)",
                    self.host_port
                );
                return Ok(false);
            }
            Err(_) => {
                warn!(
                    "check_server_key: {} timeout (60s) waiting for user response",
                    self.host_port
                );
                let mut map = self.pending.lock().await;
                map.remove(&request_id);
                return Ok(false);
            }
        };

        if accepted {
            info!("check_server_key: {} user accepted, saving to known_hosts", self.host_port);
            let mut known = known;
            known.insert(self.host_port.clone(), KnownHostEntry { key_type, key_hex });
            if let Err(e) = save_known_hosts(&self.known_hosts_path, &known) {
                warn!(
                    "check_server_key: {} save_known_hosts failed: {}",
                    self.host_port, e
                );
            }
        } else {
            info!("check_server_key: {} user rejected", self.host_port);
        }

        Ok(accepted)
    }
}

#[derive(Debug, Clone, Serialize)]
struct HostKeyVerifyEvent {
    request_id: String,
    host_port: String,
    key_type: String,
    fingerprint: String,
    is_changed: bool,
}

#[derive(Clone, Serialize)]
pub(crate) struct KeyboardInteractiveEvent {
    pub(crate) request_id: String,
    pub(crate) name: String,
    pub(crate) instructions: String,
    pub(crate) prompts: Vec<String>,
}

// Unified session lifecycle status event. Emitted at connect-success,
// remote-close, and user-disconnect so the frontend can keep a single
// authoritative `session.status` and derive all status UI from it.
// (The existing per-session `ssh-closed-{id}` event is preserved for
// backward compatibility / existing listeners; this is additive.)
const SESSION_STATUS_EVENT: &str = "ssh-session-status";

#[derive(Clone, Serialize)]
struct SessionStatusEvent {
    session_id: String,
    // "connected" | "disconnected"
    status: String,
    reason: Option<String>,
}

/// 统一的 russh 客户端配置构建点（GUI 交互式连接与 headless 连接共用）。
///
/// 为什么必须显式设置：russh 0.49 的 `Config::default()` 里 `keepalive_interval`
/// 与 `inactivity_timeout` **都是 `None`**（见 russh-0.49.2 `client/mod.rs:1514-1516`）。
/// 于是半开 TCP（拔网线、休眠唤醒、NAT 表项超时）上会话永不判死：UI 永远显示
/// 「已连接」，而该连接上任何新操作都只能靠各自超时兜底，用户看到的是「卡住」。
///
/// 两个字段的分工（russh `client/mod.rs:1038-1045`）：
/// - `keepalive_interval`：静默达到该时长就发一个 keepalive 探测；
/// - `inactivity_timeout`：**没发 keepalive 时**的静默上限，发送 keepalive 会重置它。
/// 因此 inactivity 必须 **大于** keepalive_interval，否则会在探测有机会失败重试前
/// 就先把连接掐掉。
///
/// 断开时机（russh `client/mod.rs:955-963`）：判据是 `alive_timeouts > keepalive_max`
/// 且 `keepalive_max` 默认为 3（`mod.rs:1516`）；`alive_timeouts` 在**判据之后**才自增，
/// 因此 0→1→2→3→4 要走到第 5 个 30s tick（约 150s）判据才成立。再叠加 300s 的
/// inactivity 上限，半开会话最迟约 5 分钟被回收。
/// v0.20（SSH P1）参数化版本：按资产配置覆盖 keepalive 间隔。
/// `keepalive_secs = None` → 默认 30s（与旧 build_client_config 完全一致）；
/// inactivity 固定 = keepalive × 10（保持「inactivity 必须大于 keepalive」的
/// 分工约束——间隔调小检测更快，但 inactivity 也随之等比收紧，不会出现


/// 包裹私钥用于 publickey 认证。RSA 密钥必须指定 rsa-sha2-256：传 None 会
/// 退回 SHA-1 的 ssh-rsa 算法，OpenSSH 8.8+ 服务器默认禁用该算法，导致
/// RSA 私钥（云厂商下发的 PEM 几乎都是）对现代服务器必然认证失败。
/// 非 RSA 密钥必须传 None（russh 对非 RSA 传 Some 会报 InvalidParameters）。
pub fn wrap_key_with_preferred_hash(
    key_pair: russh::keys::PrivateKey,
) -> Result<russh::keys::key::PrivateKeyWithHashAlg, String> {
    let hash_alg = if key_pair.algorithm().is_rsa() {
        Some(russh::keys::HashAlg::Sha256)
    } else {
        None
    };
    russh::keys::key::PrivateKeyWithHashAlg::new(Arc::new(key_pair), hash_alg)
        .map_err(|e| format!("Key wrap failed: {e}"))
}

/// 展开 `~/` 前缀为本机家目录。
///
/// v2.5：USERPROFILE/HOME 均缺失时返回显式 Err（不再静默回退 `"."`——
/// 那会把私钥读定向到进程 CWD，报错信息误导排查方向）。
pub fn expand_home_path(path: &str) -> Result<String, String> {
    if path.starts_with("~/") {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(|h| h.to_string_lossy().into_owned())
            .ok_or_else(|| {
                "无法确定本机用户主目录（USERPROFILE/HOME 均未设置），无法展开私钥路径 '~'，请改用绝对路径".to_string()
            })?;
        Ok(format!("{}{}", home, &path[1..]))
    } else {
        Ok(path.to_string())
    }
}

#[tauri::command]
pub async fn ssh_connect(
    state: State<'_, AppState>,
    host: String,
    port: u16,
    username: String,
    password: String,
    credential_id: Option<String>,
    auth_method: Option<String>,
    private_key_path: Option<String>,
    passphrase: Option<String>,
    passphrase_credential_id: Option<String>,
    private_key_credential_id: Option<String>,
    jump_host: Option<String>,
    connect_timeout_secs: Option<u32>,
    keepalive_interval_secs: Option<u32>,
    cols: u32,
    rows: u32,
) -> Result<SshConnectResult, String> {
    let handle = match connect_authenticated(
        &state,
        &host,
        port,
        &username,
        password,
        credential_id,
        auth_method,
        private_key_path,
        passphrase,
        passphrase_credential_id,
        private_key_credential_id,
        jump_host,
        connect_timeout_secs,
        keepalive_interval_secs,
    )
    .await
    {
        Ok(handle) => handle,
        Err(error) => {
            error!("SSH connect failed for {username}@{host}:{port}: {error}");
            return Ok(SshConnectResult {
                session_id: String::new(),
                connected: false,
                error: Some(error),
            });
        }
    };

    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|e| format!("Channel open failed: {e}"))?;

    let pty_cols = if cols > 0 { cols } else { 80 };
    let pty_rows = if rows > 0 { rows } else { 24 };
    // pty 以 ECHO=0 打开（SSH_TTY_OP_ECHO=53）：前端 OSC 7 cwd 上报注入
    // （injectShellCwdIntegration）依赖远端回显从建连起关闭才能无痕——注入行在远端
    // .bashrc 执行期就到达输入队列，回显开着会被 tty 提前回显成可见乱码（MOTD 横幅
    // 是 sshd 在 shell 启动前打印的，前端无法用输出事件判断 shell 是否就绪）。
    // 回显恢复由注入行内的 `stty echo` 完成（shell 就绪后随首条注入命令执行）。
    channel
        .request_pty(false, "xterm-256color", pty_cols, pty_rows, 0, 0, &[(Pty::ECHO, 0)])
        .await
        .map_err(|e| format!("PTY request failed: {e}"))?;

    channel
        .request_shell(true)
        .await
        .map_err(|e| format!("Shell request failed: {e}"))?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let event_name = format!("ssh-output-{session_id}");
    let closed_event_name = format!("ssh-closed-{session_id}");

    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<SshCommand>();

    let ssh_handle: Arc<client::Handle<SshClient>> = Arc::new(handle);
    {
        let mut mgr = state.ssh_sessions.lock().await;
        mgr.ssh_handles.insert(session_id.clone(), ssh_handle.clone());
        mgr.session_meta.insert(
            session_id.clone(),
            SessionMeta {
                host: host.clone(),
                port,
                username: username.clone(),
            },
        );
        mgr.sessions
            .insert(session_id.clone(), SshSession { cmd_tx });
    }

    let emit_app = {
        let mgr = state.ssh_sessions.lock().await;
        mgr.app.clone()
    };

    // Clone the AppHandle so the spawned task can resolve AppState later
    // (for resource_monitor's MonitorExec path: app.state::<AppState>()) without
    // borrowing the Tauri State<'_, AppState>.
    let app_handle_for_task = emit_app.clone();
    // Separate clone for the connect-success SESSION_STATUS_EVENT emit below:
    // both emit_app and app_handle_for_task are moved into the spawned task, so
    // we need an AppHandle that stays owned here after the spawn.
    let app_handle_for_status_emit = app_handle_for_task.clone();

    // Clone the session id into a task-local binding so the outer `session_id`
    // stays owned and valid for the log + return value below.
    let session_id_task = session_id.clone();

    // 会话级「在途 monitor exec」守卫：资源监控按固定间隔（默认 2s）轮询，每个 tick
    // 都发一条 SshCommand::MonitorExec。若远端命令挂住（stale NFS 挂载上的 `df` 会
    // 无限期阻塞），上一个 exec 任务未结束就绝不能再 spawn 下一个 —— 否则挂死任务
    // 累积、不断吃满远端 MaxSessions 配额（OpenSSH 默认 10）。用 AtomicBool 交换标记
    // 判定，纯原子操作，不会阻塞 select 循环（不能用会阻塞的锁做在途判断）。
    let monitor_exec_inflight = Arc::new(AtomicBool::new(false));

    // PTY 任务退出时自清理所需的句柄。稀疏但必要：远端断开（EOF / None）与用户
    // 断开两条路径都可能不经 `ssh_disconnect`（例如服务器重启、拔网线导致的对端
    // 关闭——后端只 emit 事件），若不自清理，ssh_handles / session_meta /
    // sftp_cache / 隧道会一直残留到用户手动断开为止。
    //
    // 只 clone 两个 Arc 而不是整个 AppState（后者含非 Clone 的 Mutex 字段，且
    // 本任务只需要这两把锁）；session id 也必须再 clone 一份，因为
    // session_id_task 已被上面的 select 分支各处借用。
    let ssh_sessions_for_task = state.ssh_sessions.clone();
    let resource_monitors_for_task = state.resource_monitors.clone();
    let session_id_for_cleanup = session_id.clone();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                msg = channel.wait() => {
                    match msg {
                        Some(ChannelMsg::Data { data }) => {
                            let _ = emit_app.emit(&event_name, data.to_vec());
                        }
                        Some(ChannelMsg::ExtendedData { data, ext: _ }) => {
                            let _ = emit_app.emit(&event_name, data.to_vec());
                        }
                        Some(ChannelMsg::Eof) | None => {
                            let _ = emit_app.emit(&closed_event_name, "remote-closed".to_string());
                            let _ = emit_app.emit(
                                SESSION_STATUS_EVENT,
                                SessionStatusEvent {
                                    session_id: session_id_task.clone(),
                                    status: "disconnected".to_string(),
                                    reason: Some("remote-closed".to_string()),
                                },
                            );
                            break;
                        }
                        _ => {}
                    }
                }
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(SshCommand::Write(data)) => {
                            let _ = channel.data(Cursor::new(data)).await;
                        }
                        Some(SshCommand::Resize { cols, rows }) => {
                            let _ = channel.window_change(cols, rows, 0, 0).await;
                        }
                        Some(SshCommand::MonitorExec(command)) => {
                            // Run a one-shot exec on a fresh channel — does NOT
                            // touch the interactive PTY. Parse /proc/* output and
                            // emit resource-monitor-snapshot.
                            //
                            // 必须 spawn 出去、绝不在这里 .await：handle_monitor_exec
                            // 会一直等 exec channel 读到 EOF，远端命令挂住时（经典场景：
                            // stale NFS 挂载让 `df` 无限期阻塞）这个 .await 永不返回，
                            // 本 select 循环被占死 —— 终端输出不再转发、键盘输入不再
                            // 处理，连用户点「断开连接」的 SshCommand::Disconnect 都取
                            // 不出来，会话变僵尸只能重启应用。spawn 后循环立刻继续服务
                            // 终端与其它 SshCommand。
                            if monitor_exec_inflight
                                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                                .is_err()
                            {
                                // 已在途：跳过本次 tick（不排队），避免累积挂死任务。
                                warn!(
                                    "resource_monitor: session {session_id_task} monitor exec still in flight, skipping this tick"
                                );
                            } else {
                                // spawn 的任务要求 'static：clone Arc<Handle> 与 AppHandle
                                // （都是 cheap clone）后 move 进去，任务内再以引用调用。
                                let ssh_handle_for_exec = ssh_handle.clone();
                                let app_handle_for_exec = app_handle_for_task.clone();
                                let session_id_for_exec = session_id_task.clone();
                                let inflight_for_exec = monitor_exec_inflight.clone();
                                tokio::spawn(async move {
                                    // RAII 守卫：超时、错误、正常结束任何返回路径都会清标记。
                                    let _inflight_guard = MonitorExecInFlightGuard(inflight_for_exec);
                                    handle_monitor_exec(
                                        session_id_for_exec,
                                        command,
                                        &ssh_handle_for_exec,
                                        &app_handle_for_exec,
                                    )
                                    .await;
                                });
                            }
                        }
                        Some(SshCommand::Disconnect) | None => {
                            let _ = channel.eof().await;
                            let _ = emit_app.emit(&closed_event_name, "disconnected-by-user".to_string());
                            let _ = emit_app.emit(
                                SESSION_STATUS_EVENT,
                                SessionStatusEvent {
                                    session_id: session_id_task.clone(),
                                    status: "disconnected".to_string(),
                                    reason: Some("disconnected-by-user".to_string()),
                                },
                            );
                            break;
                        }
                    }
                }
            }
        }
        // PTY 任务退出（远端 EOF 或用户断开）→ 自清理。
        // 这条路径原先只 emit 事件就结束，全局表项全部残留：远端重启/掉线后
        // 用户看到的「已连接」是假的，且后续 SFTP 一直命中死会话，必须手动断开
        // 才能恢复。两个清理步骤都可重入，因此与 ssh_disconnect 并发调用安全
        // （后者已经移除过的键在这里是 no-op）。
        cleanup_session_tables(&ssh_sessions_for_task, &session_id_for_cleanup).await;
        stop_session_monitor(&resource_monitors_for_task, &session_id_for_cleanup);
    });

    info!("SSH session {session_id} established for {username}@{host}:{port}");

    // Emit unified session-status "connected" so the frontend can update the
    // authoritative `session.status` and all derived UI (sidebar dot, etc.).
    // The invoke return value (SshConnectResult.connected) also signals this,
    // but the event lets a single listener own the status source of truth.
    let _ = app_handle_for_status_emit.emit(
        SESSION_STATUS_EVENT,
        SessionStatusEvent {
            session_id: session_id.clone(),
            status: "connected".to_string(),
            reason: None,
        },
    );

    Ok(SshConnectResult {
        session_id,
        connected: true,
        error: None,
    })
}

#[tauri::command]
pub async fn ssh_list_directory(
    state: State<'_, AppState>,
    host: String,
    port: u16,
    username: String,
    password: String,
    credential_id: Option<String>,
    auth_method: Option<String>,
    private_key_path: Option<String>,
    passphrase: Option<String>,
    passphrase_credential_id: Option<String>,
    private_key_credential_id: Option<String>,
    jump_host: Option<String>,
    connect_timeout_secs: Option<u32>,
    keepalive_interval_secs: Option<u32>,
    path: String,
) -> Result<RemoteDirectoryList, String> {
    let handle = connect_authenticated(
        &state,
        &host,
        port,
        &username,
        password,
        credential_id,
        auth_method,
        private_key_path,
        passphrase,
        passphrase_credential_id,
        private_key_credential_id,
        jump_host,
        connect_timeout_secs,
        keepalive_interval_secs,
    )
    .await?;

    // 走 SFTP 子系统列目录（对齐 sftp_list_dir 通道），替代旧 exec
    // `find -printf`：-printf 是 GNU 扩展，BusyBox/Alpine/BSD/macOS 上整个
    // 目录列表直接失败；且 %TY-%Tm-%Td 时间格式与 SFTP 路径的 epoch 秒
    // 不一致（前端按数字排序）。该连接本为一次性，SFTP 会话随连接丢弃。
    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| format!("SFTP channel open failed: {e}"))
        .inspect_err(|m| error!("ssh_list_directory (host {host}): {m}"))?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| format!("SFTP subsystem request failed: {e}"))
        .inspect_err(|m| error!("ssh_list_directory (host {host}): {m}"))?;
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| format!("SFTP session init failed: {e}"))
        .inspect_err(|m| error!("ssh_list_directory (host {host}): {m}"))?;

    // 空 path = 服务器默认目录：canonicalize(".") 解析出真实绝对路径
    //（SFTP 服务进程 cwd 起始于登录用户家目录，与 sftp_list_dir 语义一致）。
    let requested_path = if path.trim().is_empty() {
        sftp.canonicalize(".")
            .await
            .map_err(|e| format!("SFTP canonicalize failed: {e}"))
            .inspect_err(|m| error!("ssh_list_directory (host {host}): {m}"))?
    } else {
        path
    };

    let mut entries: Vec<RemoteFileEntry> = sftp
        .read_dir(&requested_path)
        .await
        .map_err(|e| format!("SFTP read_dir failed: {e}"))
        .inspect_err(|m| {
            error!("ssh_list_directory (host {host}, path {requested_path}): {m}")
        })?
        .into_iter()
        .map(dir_entry_to_remote_file_entry)
        .collect();

    // 排序与 sftp_list_dir 一致：目录优先，同类型按名称字母序。
    entries.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));

    Ok(RemoteDirectoryList {
        host,
        path: requested_path,
        entries,
    })
}

/// 「keepalive 5s + inactivity 300s」这类探测重试窗口不足的失衡组合）。
pub fn build_client_config_with(keepalive_secs: Option<u32>) -> Arc<client::Config> {
    let keepalive = keepalive_secs.unwrap_or(30).max(1);
    Arc::new(client::Config {
        keepalive_interval: Some(std::time::Duration::from_secs(keepalive as u64)),
        inactivity_timeout: Some(std::time::Duration::from_secs(keepalive as u64 * 10)),
        ..client::Config::default()
    })
}

async fn connect_authenticated(
    state: &State<'_, AppState>,
    host: &str,
    port: u16,
    username: &str,
    password: String,
    credential_id: Option<String>,
    auth_method: Option<String>,
    private_key_path: Option<String>,
    passphrase: Option<String>,
    passphrase_credential_id: Option<String>,
    private_key_credential_id: Option<String>,
    jump_host: Option<String>,
    // v0.20（SSH P1）：TCP 建连超时秒（None = 不设，现状）。
    connect_timeout_secs: Option<u32>,
    // v0.20（SSH P1）：keepalive 间隔秒（None = 默认 30）。
    keepalive_interval_secs: Option<u32>,
) -> Result<client::Handle<SshClient>, String> {
    let config = build_client_config_with(keepalive_interval_secs);

    let (app, secret_store_dir, known_hosts_path, pending, pending_keyboard) = {
        let mgr = state.ssh_sessions.lock().await;
        (
            mgr.app.clone(),
            mgr.secret_store_dir.clone(),
            mgr.known_hosts_path.clone(),
            mgr.pending_host_decisions.clone(),
            mgr.pending_keyboard.clone(),
        )
    };

    let known_hosts_for_jump = known_hosts_path.clone();
    let handler = SshClient {
        app: app.clone(),
        host_port: format!("{host}:{port}"),
        known_hosts_path,
        pending,
    };
    // v0.20（SSH P0-2）：带跳板时目标握手跑在跳板的 direct-tcpip 流上（同 headless 路径）
    let mut handle = match jump_host.as_deref().map(str::trim).filter(|j| !j.is_empty()) {
        Some(jump) => {
            let stream = super::jump::open_direct_stream(
                jump, host, port, &state.asset_store_path, &secret_store_dir, &known_hosts_for_jump,
            )
            .await?;
            client::connect_stream(config, stream, handler)
                .await
                .map_err(|e| format!("SSH connect via jump failed: {e}"))?
        }
        None => {
            // v0.20（SSH P1）：可选 TCP+握手超时（russh connect 无内建超时，外部包裹）
            let connect_fut = client::connect(config, (host, port), handler);
            match connect_timeout_secs {
                Some(secs) => tokio::time::timeout(
                    std::time::Duration::from_secs(secs as u64),
                    connect_fut,
                )
                .await
                .map_err(|_| format!("SSH connect 超时（{secs}s）：{host}:{port}"))?
                .map_err(|e| format!("SSH connect failed: {e}"))?,
                None => connect_fut
                    .await
                    .map_err(|e| format!("SSH connect failed: {e}"))?,
            }
        }
    };

    info!("SSH TCP connected to {host}:{port}");

    let resolved_password: Option<String> = if auth_method.as_deref() == Some("PrivateKey") {
        None
    } else if password.is_empty() {
        if let Some(ref cred_id) = credential_id {
            Some(
                myshelltool_core::SecretStore::new(&secret_store_dir, Box::new(crate::dpapi_codec::DpapiCodec))
                    .read(cred_id)
                    .map_err(|e| format!("Failed to read credential: {e}"))?
                    .ok_or_else(|| "Stored credential not found".to_string())?,
            )
        } else {
            return Err("No password provided and no stored credential".to_string());
        }
    } else {
        Some(password)
    };

    // v0.20（SSH P2）：第二因子素材预读——Password 主方式失败时尝试 publickey
    // 需要（key 内容 + passphrase）。仅 Password 模式且有私钥配置时读取（惰性：
    // 不配置就 None，不产生额外 IO）。
    let (second_factor_key, second_factor_passphrase): (Option<String>, Option<String>) =
        if auth_method.as_deref() == Some("Password") {
            let key_str = if let Some(ref cred_id) = private_key_credential_id {
                let store = myshelltool_core::SecretStore::new(&secret_store_dir, Box::new(crate::dpapi_codec::DpapiCodec));
                store.read(cred_id).ok().flatten().filter(|c| !c.trim().is_empty())
            } else {
                private_key_path.as_deref().map(|kp| {
                    expand_home_path(kp)
                        .and_then(|p| std::fs::read(&p).map_err(|e| format!("{e}").to_string()))
                        .map(|d| String::from_utf8_lossy(&d).to_string())
                        .ok()
                })
                .flatten()
            };
            let passphrase = if let Some(ref cred_id) = passphrase_credential_id {
                myshelltool_core::SecretStore::new(&secret_store_dir, Box::new(crate::dpapi_codec::DpapiCodec))
                    .read(cred_id)
                    .ok()
                    .flatten()
            } else {
                passphrase.as_deref().filter(|p| !p.is_empty()).map(str::to_string)
            };
            (key_str, passphrase)
        } else {
            (None, None)
        };

    let auth_ok = if auth_method.as_deref() == Some("Agent") {
        // v0.20（SSH P1）：agent 认证——named pipe → Pageant，逐 key 尝试；
        // 私钥留在 agent 进程（密钥零复制）。成功直接返回（不进 keyboard-interactive
        // 兜底——agent 模式下没有密码可用）。
        return match super::agent::authenticate_with_agent(&mut handle, username).await {
            super::agent::AgentAuthOutcome::Authenticated => {
                info!("agent auth succeeded for {username}@{host}:{port}");
                Ok(handle)
            }
            super::agent::AgentAuthOutcome::Failed(msg) => Err(msg),
        };
    } else if auth_method.as_deref() == Some("PrivateKey") {
        let resolved_passphrase = if let Some(ref cred_id) = passphrase_credential_id {
            myshelltool_core::SecretStore::new(&secret_store_dir, Box::new(crate::dpapi_codec::DpapiCodec))
                .read(cred_id)
                .map_err(|e| format!("Failed to read passphrase: {e}"))?
        } else {
            passphrase.as_deref().and_then(|p| {
                if p.is_empty() {
                    None
                } else {
                    Some(p.to_string())
                }
            })
        };

        // 优先从 SecretStore 读取托管在本地安全保管箱中的私钥内容
        let key_str = if let Some(ref cred_id) = private_key_credential_id {
            let store = myshelltool_core::SecretStore::new(&secret_store_dir, Box::new(crate::dpapi_codec::DpapiCodec));
            match store.read(cred_id) {
                Ok(Some(content)) if !content.trim().is_empty() => content,
                _ => {
                    let key_path = private_key_path.as_deref().unwrap_or("~/.ssh/id_ed25519");
                    let expanded = expand_home_path(key_path)?;
                    let key_data = std::fs::read(&expanded)
                        .map_err(|e| format!("Failed to read key file '{}': {e}", expanded))?;
                    String::from_utf8_lossy(&key_data).to_string()
                }
            }
        } else {
            let key_path = private_key_path.as_deref().unwrap_or("~/.ssh/id_ed25519");
            let expanded = expand_home_path(key_path)?;
            let key_data = std::fs::read(&expanded)
                .map_err(|e| format!("Failed to read key file '{}': {e}", expanded))?;
            String::from_utf8_lossy(&key_data).to_string()
        };

        let key_pair = russh::keys::decode_secret_key(&key_str, resolved_passphrase.as_deref())
            .map_err(|e| format!("Failed to load private key: {e}"))?;
        let key_with_hash = wrap_key_with_preferred_hash(key_pair)?;
        handle
            .authenticate_publickey(username, key_with_hash)
            .await
            .map_err(|e| format!("Public key auth failed: {e}"))?
    } else {
        handle
            .authenticate_password(username, resolved_password.as_deref().unwrap_or(""))
            .await
            .map_err(|e| format!("Auth failed: {e}"))?
    };

    if !auth_ok {
        // v0.20（SSH P2）：partial-success 认证链——主方式失败后先尝试**另一因子**
        // 再进 keyboard-interactive。背景：服务器 `AuthenticationMethods publickey,password`
        // 时，publickey 返回 partial success（还需第二因子），但 russh 0.49 把
        // USERAUTH_FAILURE 的 partial 标志折叠成 false（encrypted.rs:246 只读
        // remaining_methods 不外露），调用方无法区分「方法被拒」与「还需第二因子」。
        // 故采用顺序因子策略：PrivateKey 失败且有存密码 → 先试 password；
        // Password 失败且有私钥 → 先试 publickey。对单因子服务器语义不变
        // （第二因子同样被拒，最终落到原有 kbd-interactive 兜底）。
        let second_factor_ok = if auth_method.as_deref() == Some("PrivateKey") {
            if let Some(pwd) = resolved_password.as_deref() {
                if !pwd.is_empty() {
                    info!(
                        "auth: primary method rejected for {username}@{host}:{port}, trying second factor (password)"
                    );
                    handle.authenticate_password(username, pwd).await
                        .map_err(|e| format!("Second-factor password auth failed: {e}"))?
                } else { false }
            } else { false }
        } else if auth_method.as_deref() == Some("Password") {
            // Password 主方式失败 + 配有私钥：尝试 publickey 第二因子
            if let (Some(key_str), Some(passphrase)) = (second_factor_key.as_deref(), second_factor_passphrase.as_deref()) {
                match russh::keys::decode_secret_key(key_str, Some(passphrase)) {
                    Ok(key_pair) => {
                        info!(
                            "auth: primary method rejected for {username}@{host}:{port}, trying second factor (publickey)"
                        );
                        let key_with_hash = wrap_key_with_preferred_hash(key_pair)?;
                        handle.authenticate_publickey(username, key_with_hash).await
                            .map_err(|e| format!("Second-factor publickey auth failed: {e}"))?
                    }
                    Err(e) => {
                        // 第二因子私钥解析失败：如实记录后继续兜底链（不静默吞）
                        warn!("second-factor private key parse failed: {e}");
                        false
                    }
                }
            } else { false }
        } else { false };

        if second_factor_ok {
            info!("auth succeeded via second factor for {username}@{host}:{port}");
            return Ok(handle);
        }

        info!(
            "auth: password/key rejected for {username}@{host}:{port}, trying keyboard-interactive"
        );
        let resp = handle
            .authenticate_keyboard_interactive_start(username, None::<String>)
            .await
            .map_err(|e| format!("Keyboard-interactive start failed: {e}"))?;
        // 自动用已保存的密码响应 keyboard-interactive 的密码 prompt（debian 等服务器
        // 默认禁用 PasswordAuthentication 但启用 KbdInteractiveAuthentication，导致
        // password auth 失败但 keyboard-interactive 实际就是同一密码）
        let auto_password = resolved_password.as_deref();
        let (h, authenticated) =
            super::keyboard::keyboard_interactive_loop(handle, resp, &pending_keyboard, &app, auto_password).await?;
        if !authenticated {
            warn!("auth failed (all methods) for {username}@{host}:{port}");
            return Err("Authentication failed (password + keyboard-interactive)".to_string());
        }
        return Ok(h);
    }

    info!("auth succeeded for {username}@{host}:{port}");
    Ok(handle)
}
