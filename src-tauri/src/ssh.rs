use async_trait::async_trait;
use log::{error, info, warn};
use russh::client;
use russh::keys::PublicKeyBase64;
use russh::ChannelMsg;
use russh_sftp::client::SftpSession;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{oneshot, Mutex};

use crate::AppState;

// --- Tunnel / port-forwarding types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    pub id: String,
    pub name: String,
    pub kind: String, // "local", "remote", "dynamic"
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: String,
    pub remote_port: u16,
    pub session_id: String,
    pub auto_start: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TunnelStatus {
    pub id: String,
    pub config: TunnelConfig,
    pub active: bool,
    pub error: Option<String>,
}

type PendingDecisions = Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>;

type PendingKeyboardResponses = Arc<Mutex<HashMap<String, oneshot::Sender<Vec<String>>>>>;

struct SshClient {
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

pub enum SshCommand {
    Write(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Disconnect,
    /// Run a one-shot exec command on a fresh channel (no PTY).
    /// Used by resource_monitor to sample /proc/* without disturbing the
    /// interactive shell. Output is parsed + emitted as a ResourceSnapshot.
    MonitorExec(String),
}

struct SshSession {
    cmd_tx: tokio::sync::mpsc::UnboundedSender<SshCommand>,
}

pub struct SshSessionManager {
    sessions: HashMap<String, SshSession>,
    ssh_handles: HashMap<String, Arc<client::Handle<SshClient>>>,
    sftp_cache: HashMap<String, Arc<Mutex<SftpSession>>>,
    tunnels: HashMap<String, TunnelStatus>,
    tunnel_handles: HashMap<String, tokio::task::JoinHandle<()>>,
    upload_files: Arc<Mutex<HashMap<String, Box<dyn tokio::io::AsyncWrite + Send + Unpin>>>>,
    app: AppHandle,
    secret_store_dir: PathBuf,
    known_hosts_path: PathBuf,
    pending_host_decisions: PendingDecisions,
    pending_keyboard: PendingKeyboardResponses,
}

impl SshSessionManager {
    pub fn new(app: AppHandle, secret_store_dir: PathBuf, known_hosts_path: PathBuf) -> Self {
        Self {
            sessions: HashMap::new(),
            ssh_handles: HashMap::new(),
            sftp_cache: HashMap::new(),
            tunnels: HashMap::new(),
            tunnel_handles: HashMap::new(),
            upload_files: Arc::new(Mutex::new(HashMap::new())),
            app,
            secret_store_dir,
            known_hosts_path,
            pending_host_decisions: Arc::new(Mutex::new(HashMap::new())),
            pending_keyboard: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns a clone of the session's command channel sender, if the session still exists.
    pub fn get_cmd_tx(
        &self,
        session_id: &str,
    ) -> Option<tokio::sync::mpsc::UnboundedSender<SshCommand>> {
        self.sessions.get(session_id).map(|s| s.cmd_tx.clone())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SshConnectResult {
    pub session_id: String,
    pub connected: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteFileEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub modified: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteDirectoryList {
    pub host: String,
    pub path: String,
    pub entries: Vec<RemoteFileEntry>,
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
struct KeyboardInteractiveEvent {
    request_id: String,
    name: String,
    instructions: String,
    prompts: Vec<String>,
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
) -> Result<client::Handle<SshClient>, String> {
    let config = Arc::new(client::Config::default());

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

    let handler = SshClient {
        app: app.clone(),
        host_port: format!("{host}:{port}"),
        known_hosts_path,
        pending,
    };
    let mut handle = client::connect(config, (host, port), handler)
        .await
        .map_err(|e| format!("SSH connect failed: {e}"))?;

    info!("SSH TCP connected to {host}:{port}");

    let resolved_password: Option<String> = if auth_method.as_deref() == Some("PrivateKey") {
        None
    } else if password.is_empty() {
        if let Some(ref cred_id) = credential_id {
            Some(
                myshelltool_core::SecretStore::new(&secret_store_dir)
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

    let auth_ok = if auth_method.as_deref() == Some("PrivateKey") {
        let key_path = private_key_path.as_deref().unwrap_or("~/.ssh/id_ed25519");
        let expanded = expand_home_path(key_path);
        let resolved_passphrase = if let Some(ref cred_id) = passphrase_credential_id {
            myshelltool_core::SecretStore::new(&secret_store_dir)
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
        let key_data = std::fs::read(&expanded)
            .map_err(|e| format!("Failed to read key file '{}': {e}", expanded))?;
        let key_str = String::from_utf8_lossy(&key_data);
        let key_pair = russh::keys::decode_secret_key(&key_str, resolved_passphrase.as_deref())
            .map_err(|e| format!("Failed to load private key: {e}"))?;
        let key_with_hash = russh::keys::key::PrivateKeyWithHashAlg::new(Arc::new(key_pair), None)
            .map_err(|e| format!("Key wrap failed: {e}"))?;
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
            keyboard_interactive_loop(handle, resp, &pending_keyboard, &app, auto_password).await?;
        if !authenticated {
            warn!("auth failed (all methods) for {username}@{host}:{port}");
            return Err("Authentication failed (password + keyboard-interactive)".to_string());
        }
        return Ok(h);
    }

    info!("auth succeeded for {username}@{host}:{port}");
    Ok(handle)
}

async fn keyboard_interactive_loop(
    mut handle: client::Handle<SshClient>,
    mut resp: client::KeyboardInteractiveAuthResponse,
    pending_keyboard: &PendingKeyboardResponses,
    app: &AppHandle,
    auto_password: Option<&str>,
) -> Result<(client::Handle<SshClient>, bool), String> {
    loop {
        match resp {
            client::KeyboardInteractiveAuthResponse::Success => return Ok((handle, true)),
            client::KeyboardInteractiveAuthResponse::Failure => return Ok((handle, false)),
            client::KeyboardInteractiveAuthResponse::InfoRequest {
                name,
                instructions,
                prompts,
            } => {
                // 判断是否所有 prompts 都是密码类（单 prompt + 文本含 password/passphrase/密码）
                // 且 auto_password 可用 → 自动响应，不弹 modal
                let all_password_like = !prompts.is_empty()
                    && prompts.iter().all(|p| {
                        let lower = p.prompt.to_lowercase();
                        lower.contains("password") || lower.contains("passphrase") || lower.contains("密码")
                    });
                let responses = if all_password_like && auto_password.is_some() {
                    let pwd = auto_password.unwrap().to_string();
                    info!(
                        "keyboard-interactive: auto-responding {} password-like prompt(s) with saved credential",
                        prompts.len()
                    );
                    prompts.iter().map(|_| pwd.clone()).collect::<Vec<String>>()
                } else {
                    // 弹 modal 让用户手动输入（MFA、非密码 prompt 等）
                    let request_id = uuid::Uuid::new_v4().to_string();
                    let (tx, rx) = oneshot::channel();
                    {
                        let mut map = pending_keyboard.lock().await;
                        map.insert(request_id.clone(), tx);
                    }
                    let prompt_texts: Vec<String> = prompts.iter().map(|p| p.prompt.clone()).collect();
                    let event = KeyboardInteractiveEvent {
                        request_id,
                        name,
                        instructions,
                        prompts: prompt_texts,
                    };
                    if app.emit("ssh-keyboard-interactive", event).is_err() {
                        return Ok((handle, false));
                    }
                    match rx.await {
                        Ok(r) => r,
                        Err(_) => return Ok((handle, false)),
                    }
                };
                resp = handle
                    .authenticate_keyboard_interactive_respond(responses)
                    .await
                    .map_err(|e| format!("Keyboard-interactive respond failed: {e}"))?;
            }
        }
    }
}

fn expand_home_path(path: &str) -> String {
    if path.starts_with("~/") {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| ".".to_string());
        format!("{}{}", home, &path[1..])
    } else {
        path.to_string()
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
    channel
        .request_pty(false, "xterm-256color", pty_cols, pty_rows, 0, 0, &[])
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

    // Clone the session id into a task-local binding so the outer `session_id`
    // stays owned and valid for the log + return value below.
    let session_id_task = session_id.clone();

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
                            handle_monitor_exec(
                                session_id_task.clone(),
                                command,
                                &ssh_handle,
                                &app_handle_for_task,
                            )
                            .await;
                        }
                        Some(SshCommand::Disconnect) | None => {
                            let _ = channel.eof().await;
                            let _ = emit_app.emit(&closed_event_name, "disconnected-by-user".to_string());
                            break;
                        }
                    }
                }
            }
        }
    });

    info!("SSH session {session_id} established for {username}@{host}:{port}");

    Ok(SshConnectResult {
        session_id,
        connected: true,
        error: None,
    })
}

/// Run a one-shot exec command on a fresh channel and parse the /proc/* output
/// into a ResourceSnapshot. Used by the resource_monitor module's polling loop
/// (via SshCommand::MonitorExec). Does NOT touch the interactive PTY channel.
///
/// Flow:
/// 1. Open a new session channel
/// 2. exec(command) — the command is `cat /proc/stat; cat /proc/meminfo; cat /proc/net/dev; cat /proc/diskstats`
/// 3. Read all stdout into a String
/// 4. Use resource_monitor::build_snapshot to parse + build a ResourceSnapshot
///    (this requires prev_cpu delta — we read the previous snapshot from the
///    monitor handle to compute the CPU delta)
/// 5. Emit "resource-monitor-snapshot" event with the parsed ResourceSnapshot
/// 6. Record the snapshot + prev_cpu back into the monitor handle for next tick
async fn handle_monitor_exec(
    session_id: String,
    command: String,
    ssh_handle: &Arc<client::Handle<SshClient>>,
    app: &AppHandle,
) {
    // Open a fresh exec channel (separate from the interactive PTY).
    let mut exec_channel = match ssh_handle.channel_open_session().await {
        Ok(ch) => ch,
        Err(e) => {
            warn!(
                "resource_monitor: session {session_id} exec channel open failed: {e}"
            );
            return;
        }
    };

    if let Err(e) = exec_channel.exec(true, command).await {
        warn!(
            "resource_monitor: session {session_id} exec failed: {e}"
        );
        return;
    }

    // Collect stdout + stderr.
    let mut stdout = Vec::new();
    while let Some(msg) = exec_channel.wait().await {
        match msg {
            ChannelMsg::Data { data } => stdout.extend_from_slice(&data),
            ChannelMsg::ExtendedData { data, ext: _ } => {
                // stderr — log but don't fail the snapshot
                let msg = String::from_utf8_lossy(&data);
                warn!(
                    "resource_monitor: session {session_id} stderr: {}",
                    msg.trim()
                );
            }
            ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
            ChannelMsg::Close => break,
            _ => {}
        }
    }

    let combined = String::from_utf8_lossy(&stdout);

    // Resolve AppState via the AppHandle so we can read/write resource_monitors.
    let resource_state = app.state::<crate::AppState>();

    // Fetch prev_cpu so build_snapshot can compute a CPU delta.
    let prev_cpu = match resource_state.resource_monitors.lock() {
        Ok(m) => m.handles.get(&session_id).and_then(|h| h.prev_cpu.clone()),
        Err(_) => None,
    };

    // Build snapshot (parses /proc/stat, /proc/meminfo, /proc/net/dev, /proc/diskstats).
    let snapshot = crate::resource_monitor::build_snapshot(&session_id, &combined, prev_cpu);

    // Compute prev_cpu for the NEXT tick = (idle, total) from this sample.
    // We re-parse /proc/stat to get the current jiffies.
    let (idle_now, total_now, _) =
        match crate::resource_monitor::parse_proc_stat(
            &crate::resource_monitor::extract_stat_section(&combined),
        ) {
            Ok(v) => v,
            Err(_) => (0, 0, 0),
        };

    // Emit the structured snapshot to the frontend.
    if let Err(e) = app.emit("resource-monitor-snapshot", snapshot.clone()) {
        warn!(
            "resource_monitor: session {session_id} emit snapshot failed: {e}"
        );
    }

    // Record the snapshot + prev_cpu back into the monitor handle.
    crate::resource_monitor::record_snapshot(&resource_state, snapshot, (idle_now, total_now));
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
    path: String,
) -> Result<RemoteDirectoryList, String> {
    let requested_path = if path.trim().is_empty() {
        ".".to_string()
    } else {
        path
    };
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
    )
    .await?;

    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|e| format!("Channel open failed: {e}"))?;
    let command = format!(
        "LC_ALL=C find {} -maxdepth 1 -mindepth 1 -printf '%f\\t%p\\t%y\\t%s\\t%TY-%Tm-%Td %TH:%TM\\n'",
        shell_quote(&requested_path)
    );
    channel
        .exec(true, command)
        .await
        .map_err(|e| format!("Directory list failed: {e}"))?;

    let mut output = Vec::new();
    let mut errors = Vec::new();
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } => output.extend_from_slice(&data),
            ChannelMsg::ExtendedData { data, ext: _ } => errors.extend_from_slice(&data),
            ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }

    if !errors.is_empty() {
        let message = String::from_utf8_lossy(&errors).trim().to_string();
        if !message.is_empty() {
            return Err(message);
        }
    }

    Ok(RemoteDirectoryList {
        host,
        path: requested_path,
        entries: parse_remote_file_entries(&String::from_utf8_lossy(&output)),
    })
}

#[tauri::command]
pub async fn ssh_write(
    state: State<'_, AppState>,
    session_id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    let mgr = state.ssh_sessions.lock().await;
    let session = mgr
        .sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session {session_id} not found"))?;
    session
        .cmd_tx
        .send(SshCommand::Write(data))
        .map_err(|e| format!("Send failed: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn ssh_resize(
    state: State<'_, AppState>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    let mgr = state.ssh_sessions.lock().await;
    let session = mgr
        .sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session {session_id} not found"))?;
    session
        .cmd_tx
        .send(SshCommand::Resize { cols, rows })
        .map_err(|e| format!("Send failed: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn ssh_disconnect(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let mut mgr = state.ssh_sessions.lock().await;
    if let Some(session) = mgr.sessions.remove(&session_id) {
        let _ = session.cmd_tx.send(SshCommand::Disconnect);
        info!("SSH session {session_id} disconnected");
    }
    mgr.ssh_handles.remove(&session_id);
    mgr.sftp_cache.remove(&session_id);
    // Stop and remove tunnels associated with this session
    let tunnel_ids_to_remove: Vec<String> = mgr
        .tunnels
        .iter()
        .filter(|(_, status)| status.config.session_id == session_id)
        .map(|(id, _)| id.clone())
        .collect();
    for tid in tunnel_ids_to_remove {
        if let Some(jh) = mgr.tunnel_handles.remove(&tid) {
            jh.abort();
        }
        mgr.tunnels.remove(&tid);
    }
    drop(mgr);

    // Stop any active resource monitor for this session. Direct module call —
    // no event hop needed since both modules share AppState.
    {
        let mut monitors = state.resource_monitors.lock().map_err(|e| e.to_string())?;
        if let Some(handle) = monitors.handles.remove(&session_id) {
            // Signal the polling task to stop. Ignore error — task may already be done.
            let _ = handle.cancel.send(());
            info!("resource_monitor: stopped monitor for {session_id} (via ssh_disconnect)");
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn ssh_confirm_host_key(
    state: State<'_, AppState>,
    request_id: String,
    accepted: bool,
) -> Result<(), String> {
    let mgr = state.ssh_sessions.lock().await;
    let mut pending = mgr.pending_host_decisions.lock().await;
    match pending.remove(&request_id) {
        Some(tx) => {
            let _ = tx.send(accepted);
            Ok(())
        }
        None => Err(format!(
            "ssh_confirm_host_key: request_id {request_id} not found (可能已超时或不存在)"
        )),
    }
}

#[tauri::command]
pub async fn ssh_keyboard_response(
    state: State<'_, AppState>,
    request_id: String,
    responses: Vec<String>,
) -> Result<(), String> {
    let mgr = state.ssh_sessions.lock().await;
    let mut pending = mgr.pending_keyboard.lock().await;
    match pending.remove(&request_id) {
        Some(tx) => {
            let _ = tx.send(responses);
            Ok(())
        }
        None => Err(format!(
            "ssh_keyboard_response: request_id {request_id} not found (可能已超时或不存在)"
        )),
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

    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| format!("SFTP channel open failed: {e}"))?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| format!("SFTP subsystem request failed: {e}"))?;

    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| format!("SFTP session init failed: {e}"))?;

    info!("SFTP session initialized for session {session_id}");

    let arc = Arc::new(Mutex::new(sftp));
    {
        let mut mgr = state.ssh_sessions.lock().await;
        mgr.sftp_cache.insert(session_id.to_string(), arc.clone());
    }
    Ok(arc)
}

#[tauri::command]
pub async fn sftp_list_dir(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<RemoteDirectoryList, String> {
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;

    let requested_path = if path.trim().is_empty() {
        ".".to_string()
    } else {
        path
    };

    let raw_entries = sftp
        .read_dir(&requested_path)
        .await
        .map_err(|e| format!("SFTP read_dir failed: {e}"))?;

    let mut entries: Vec<RemoteFileEntry> = raw_entries
        .into_iter()
        .map(|entry| {
            let meta = entry.metadata();
            let kind = if meta.is_dir() {
                "directory"
            } else if meta.is_symlink() {
                "symlink"
            } else {
                "file"
            }
            .to_string();
            RemoteFileEntry {
                name: entry.file_name(),
                path: entry.path(),
                kind,
                size: meta.len(),
                modified: format!("{:?}", meta.modified()),
            }
        })
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
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;
    let file = sftp
        .create(&remote_path)
        .await
        .map_err(|e| format!("SFTP create failed: {e}"))?;
    let ssh_mgr = state.ssh_sessions.lock().await;
    let mut uploads = ssh_mgr.upload_files.lock().await;
    if uploads.contains_key(&transfer_id) {
        return Err(format!("transfer_id {transfer_id} already in progress"));
    }
    uploads.insert(transfer_id, Box::new(file) as Box<dyn tokio::io::AsyncWrite + Send + Unpin>);
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
    use tokio::io::AsyncWriteExt;
    let ssh_mgr = state.ssh_sessions.lock().await;
    let app = ssh_mgr.app.clone();
    let mut uploads = ssh_mgr.upload_files.lock().await;
    let file = uploads
        .get_mut(&transfer_id)
        .ok_or_else(|| format!("transfer_id {transfer_id} not started"))?;
    file.write_all(&chunk)
        .await
        .map_err(|e| format!("SFTP write chunk failed: {e}"))?;
    let _ = app.emit(
        "sftp-transfer-progress",
        TransferProgressEvent {
            transfer_id: transfer_id.clone(),
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
    use tokio::io::AsyncWriteExt;
    let ssh_mgr = state.ssh_sessions.lock().await;
    let mut uploads = ssh_mgr.upload_files.lock().await;
    let mut file = uploads
        .remove(&transfer_id)
        .ok_or_else(|| format!("transfer_id {transfer_id} not started"))?;
    file.shutdown()
        .await
        .map_err(|e| format!("SFTP flush failed: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn sftp_download_with_progress(
    state: State<'_, AppState>,
    session_id: String,
    remote_path: String,
    transfer_id: String,
) -> Result<Vec<u8>, String> {
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;
    let app = { state.ssh_sessions.lock().await.app.clone() };

    let mut file = sftp
        .open(&remote_path)
        .await
        .map_err(|e| format!("SFTP open failed: {e}"))?;

    let attrs = sftp
        .metadata(&remote_path)
        .await
        .map_err(|e| format!("SFTP stat failed: {e}"))?;
    let total = attrs.len();

    use tokio::io::AsyncReadExt;
    let mut buf = Vec::with_capacity(total as usize);
    let mut tmp = [0u8; 65536];
    loop {
        let n = file
            .read(&mut tmp)
            .await
            .map_err(|e| format!("SFTP read chunk failed: {e}"))?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        let _ = app.emit(
            "sftp-transfer-progress",
            TransferProgressEvent {
                transfer_id: transfer_id.clone(),
                bytes_transferred: buf.len() as u64,
                total_bytes: total,
            },
        );
    }

    Ok(buf)
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
        modified: format!("{:?}", meta.modified()),
    })
}

// --- Tunnel / port-forwarding commands ---

#[tauri::command]
pub async fn tunnel_create(
    state: State<'_, AppState>,
    config: TunnelConfig,
) -> Result<TunnelStatus, String> {
    let id = config.id.clone();
    let status = TunnelStatus {
        id: id.clone(),
        config,
        active: false,
        error: None,
    };
    let mut mgr = state.ssh_sessions.lock().await;
    mgr.tunnels.insert(id, status.clone());
    Ok(status)
}

#[tauri::command]
pub async fn tunnel_start(
    state: State<'_, AppState>,
    session_id: String,
    tunnel_id: String,
) -> Result<(), String> {
    let (handle, config, app) = {
        let mgr = state.ssh_sessions.lock().await;
        let handle = mgr
            .ssh_handles
            .get(&session_id)
            .ok_or_else(|| format!("SSH session {session_id} not found"))?
            .clone();
        let tunnel = mgr
            .tunnels
            .get(&tunnel_id)
            .ok_or_else(|| format!("Tunnel {tunnel_id} not found"))?;
        if tunnel.active {
            return Err("Tunnel is already active".to_string());
        }
        (handle, tunnel.config.clone(), mgr.app.clone())
    };

    let join_handle = match config.kind.as_str() {
        "local" => start_local_forward(handle, config.clone(), app).await?,
        "remote" => start_remote_forward(handle, config.clone()).await?,
        "dynamic" => start_dynamic_forward(handle, config.clone(), app).await?,
        other => return Err(format!("Unknown tunnel kind: {other}")),
    };

    {
        let mut mgr = state.ssh_sessions.lock().await;
        if let Some(status) = mgr.tunnels.get_mut(&tunnel_id) {
            status.active = true;
            status.error = None;
        }
        mgr.tunnel_handles.insert(tunnel_id.clone(), join_handle);
        info!("tunnel {tunnel_id} ({}) started", config.kind);
    }

    Ok(())
}

#[tauri::command]
pub async fn tunnel_stop(
    state: State<'_, AppState>,
    tunnel_id: String,
) -> Result<(), String> {
    let mut mgr = state.ssh_sessions.lock().await;
    if let Some(jh) = mgr.tunnel_handles.remove(&tunnel_id) {
        jh.abort();
        info!("tunnel {tunnel_id} stopped");
    }
    if let Some(status) = mgr.tunnels.get_mut(&tunnel_id) {
        status.active = false;
    }
    Ok(())
}

#[tauri::command]
pub async fn tunnel_list(
    state: State<'_, AppState>,
) -> Result<Vec<TunnelStatus>, String> {
    let mgr = state.ssh_sessions.lock().await;
    Ok(mgr.tunnels.values().cloned().collect())
}

#[tauri::command]
pub async fn tunnel_delete(
    state: State<'_, AppState>,
    tunnel_id: String,
) -> Result<(), String> {
    let mut mgr = state.ssh_sessions.lock().await;
    if let Some(jh) = mgr.tunnel_handles.remove(&tunnel_id) {
        jh.abort();
    }
    mgr.tunnels.remove(&tunnel_id);
    Ok(())
}

// --- Local port forwarding ---

async fn start_local_forward(
    handle: Arc<client::Handle<SshClient>>,
    config: TunnelConfig,
    app: AppHandle,
) -> Result<tokio::task::JoinHandle<()>, String> {
    let listen_addr = format!("{}:{}", config.local_addr, config.local_port);
    let listener = tokio::net::TcpListener::bind(&listen_addr)
        .await
        .map_err(|e| format!("Failed to bind {listen_addr}: {e}"))?;

    let remote_addr = config.remote_addr.clone();
    let remote_port = config.remote_port as u32;
    let tunnel_id = config.id.clone();

    let jh = tokio::spawn(async move {
        loop {
            let tcp_stream = match listener.accept().await {
                Ok((s, _)) => s,
                Err(_) => continue,
            };

            let ssh_handle = handle.clone();
            let remote_addr = remote_addr.clone();
            let app = app.clone();
            let tid = tunnel_id.clone();

            tokio::spawn(async move {
                let mut channel = match ssh_handle
                    .channel_open_direct_tcpip(&remote_addr, remote_port, "127.0.0.1", 0)
                    .await
                {
                    Ok(ch) => ch,
                    Err(_) => return,
                };

                let (mut tcp_read, mut tcp_write) = tokio::io::split(tcp_stream);

                // Use a single loop with select, reading from SSH channel and TCP concurrently.
                // channel.wait() borrows &mut self, so we can't have it in a separate future
                // alongside channel.data(). Instead, interleave reads and writes in one task.
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut tcp_buf = vec![0u8; 32 * 1024];
                loop {
                    tokio::select! {
                        msg = channel.wait() => {
                            match msg {
                                Some(ChannelMsg::Data { data }) => {
                                    if tcp_write.write_all(&data).await.is_err() {
                                        break;
                                    }
                                }
                                Some(ChannelMsg::Eof) | None => break,
                                _ => {}
                            }
                        }
                        n = tcp_read.read(&mut tcp_buf) => {
                            match n {
                                Ok(0) => {
                                    let _ = channel.eof().await;
                                    break;
                                }
                                Ok(n) => {
                                    if channel.data(&tcp_buf[..n]).await.is_err() {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    }
                }

                let _ = app.emit(&format!("tunnel-traffic-{tid}"), "connection-closed");
            });
        }
    });

    Ok(jh)
}

// --- Remote port forwarding ---

async fn start_remote_forward(
    _handle: Arc<client::Handle<SshClient>>,
    _config: TunnelConfig,
) -> Result<tokio::task::JoinHandle<()>, String> {
    Err(
        "Remote forwarding is not yet supported. Use local or dynamic forwarding instead."
            .to_string(),
    )
}

// --- Dynamic (SOCKS5) port forwarding ---

async fn start_dynamic_forward(
    handle: Arc<client::Handle<SshClient>>,
    config: TunnelConfig,
    app: AppHandle,
) -> Result<tokio::task::JoinHandle<()>, String> {
    let listen_addr = format!("{}:{}", config.local_addr, config.local_port);
    let listener = tokio::net::TcpListener::bind(&listen_addr)
        .await
        .map_err(|e| format!("Failed to bind {listen_addr}: {e}"))?;

    let tunnel_id = config.id.clone();

    let jh = tokio::spawn(async move {
        loop {
            let tcp_stream = match listener.accept().await {
                Ok((s, _)) => s,
                Err(_) => continue,
            };

            let ssh_handle = handle.clone();
            let app = app.clone();
            let tid = tunnel_id.clone();

            tokio::spawn(async move {
                if let Err(e) =
                    handle_socks5_connection(tcp_stream, ssh_handle, app.clone(), &tid).await
                {
                    let _ = app.emit(&format!("tunnel-error-{tid}"), format!("SOCKS5 error: {e}"));
                }
            });
        }
    });

    Ok(jh)
}

async fn handle_socks5_connection(
    tcp_stream: tokio::net::TcpStream,
    handle: Arc<client::Handle<SshClient>>,
    app: AppHandle,
    tunnel_id: &str,
) -> Result<(), String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (mut tcp_read, mut tcp_write) = tokio::io::split(tcp_stream);

    // SOCKS5 handshake: read version and auth methods
    let mut buf = [0u8; 2];
    tcp_read
        .read_exact(&mut buf)
        .await
        .map_err(|e| format!("SOCKS5 read version: {e}"))?;
    if buf[0] != 0x05 {
        return Err("Not a SOCKS5 request".to_string());
    }
    let nmethods = buf[1] as usize;
    let mut methods = vec![0u8; nmethods];
    tcp_read
        .read_exact(&mut methods)
        .await
        .map_err(|e| format!("SOCKS5 read methods: {e}"))?;

    // Reply: no auth required
    tcp_write
        .write_all(&[0x05, 0x00])
        .await
        .map_err(|e| format!("SOCKS5 write method: {e}"))?;

    // Read connect request
    let mut head = [0u8; 4];
    tcp_read
        .read_exact(&mut head)
        .await
        .map_err(|e| format!("SOCKS5 read request: {e}"))?;
    if head[0] != 0x05 || head[1] != 0x01 {
        // Only support CONNECT (0x01)
        tcp_write
            .write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .await
            .ok();
        return Err("SOCKS5 only supports CONNECT".to_string());
    }

    let target_addr = match head[3] {
        // IPv4
        0x01 => {
            let mut addr = [0u8; 4];
            tcp_read
                .read_exact(&mut addr)
                .await
                .map_err(|e| format!("SOCKS5 read IPv4: {e}"))?;
            format!("{}.{}.{}.{}", addr[0], addr[1], addr[2], addr[3])
        }
        // Domain name
        0x03 => {
            let mut len_buf = [0u8; 1];
            tcp_read
                .read_exact(&mut len_buf)
                .await
                .map_err(|e| format!("SOCKS5 read domain len: {e}"))?;
            let len = len_buf[0] as usize;
            let mut domain = vec![0u8; len];
            tcp_read
                .read_exact(&mut domain)
                .await
                .map_err(|e| format!("SOCKS5 read domain: {e}"))?;
            String::from_utf8_lossy(&domain).to_string()
        }
        // IPv6
        0x04 => {
            let mut addr = [0u8; 16];
            tcp_read
                .read_exact(&mut addr)
                .await
                .map_err(|e| format!("SOCKS5 read IPv6: {e}"))?;
            addr.iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .chunks(2)
                .map(|c| c.join(""))
                .collect::<Vec<_>>()
                .join(":")
        }
        _ => {
            tcp_write
                .write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await
                .ok();
            return Err("Unsupported SOCKS5 address type".to_string());
        }
    };

    let mut port_buf = [0u8; 2];
    tcp_read
        .read_exact(&mut port_buf)
        .await
        .map_err(|e| format!("SOCKS5 read port: {e}"))?;
    let target_port = u16::from_be_bytes(port_buf) as u32;

    // Open SSH direct TCP/IP channel to the target
    let mut channel = match handle
        .channel_open_direct_tcpip(&target_addr, target_port, "127.0.0.1", 0)
        .await
    {
        Ok(ch) => ch,
        Err(e) => {
            // SOCKS5 reply: connection refused
            tcp_write
                .write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await
                .ok();
            return Err(format!("SSH channel open failed: {e}"));
        }
    };

    // SOCKS5 success reply
    tcp_write
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
        .map_err(|e| format!("SOCKS5 write success: {e}"))?;

    let _ = app.emit(
        &format!("tunnel-traffic-{tunnel_id}"),
        format!("connected:{target_addr}:{target_port}"),
    );

    // Bidirectional copy using single select loop to avoid borrow conflicts
    // on the SSH channel (wait() takes &mut self, data() takes &self)
    let mut tcp_buf = vec![0u8; 32 * 1024];
    loop {
        tokio::select! {
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        if tcp_write.write_all(&data).await.is_err() {
                            break;
                        }
                    }
                    Some(ChannelMsg::Eof) | None => break,
                    _ => {}
                }
            }
            n = tcp_read.read(&mut tcp_buf) => {
                match n {
                    Ok(0) => {
                        let _ = channel.eof().await;
                        break;
                    }
                    Ok(n) => {
                        if channel.data(&tcp_buf[..n]).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }

    Ok(())
}

// --- known_hosts helpers ---

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KnownHostEntry {
    key_type: String,
    key_hex: String,
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn parse_remote_file_entries(output: &str) -> Vec<RemoteFileEntry> {
    let mut entries = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 5 {
            continue;
        }
        entries.push(RemoteFileEntry {
            name: parts[0].to_string(),
            path: parts[1].to_string(),
            kind: match parts[2] {
                "d" => "directory",
                "l" => "symlink",
                _ => "file",
            }
            .to_string(),
            size: parts[3].parse().unwrap_or(0),
            modified: parts[4].to_string(),
        });
    }
    entries.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
    entries
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn load_known_hosts(path: &PathBuf) -> HashMap<String, KnownHostEntry> {
    if !path.exists() {
        return HashMap::new();
    }
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_known_hosts(path: &PathBuf, hosts: &HashMap<String, KnownHostEntry>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(hosts).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}
