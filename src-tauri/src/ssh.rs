use async_trait::async_trait;
use russh::client;
use russh::ChannelMsg;
use russh::keys::PublicKeyBase64;
use russh_sftp::client::SftpSession;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{Mutex, oneshot};

type PendingDecisions = Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>;

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
        let key_bytes = server_public_key.public_key_bytes();
        let key_hex = bytes_to_hex(&key_bytes);
        let key_type = format!("{}", server_public_key.algorithm());
        let fingerprint = format!(
            "{}",
            server_public_key.fingerprint(russh::keys::ssh_key::HashAlg::Sha256)
        );

        let known = load_known_hosts(&self.known_hosts_path);

        if let Some(entry) = known.get(&self.host_port) {
            if entry.key_hex == key_hex {
                return Ok(true);
            }
        }

        let request_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        {
            let mut map = self.pending.lock().await;
            map.insert(request_id.clone(), tx);
        }

        let is_changed = known.contains_key(&self.host_port);
        let event = HostKeyVerifyEvent {
            request_id,
            host_port: self.host_port.clone(),
            key_type: key_type.clone(),
            fingerprint,
            is_changed,
        };
        if self.app.emit("ssh-host-key-verify", event).is_err() {
            return Ok(false);
        }

        let accepted = match rx.await {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };

        if accepted {
            let mut known = known;
            known.insert(self.host_port.clone(), KnownHostEntry { key_type, key_hex });
            let _ = save_known_hosts(&self.known_hosts_path, &known);
        }

        Ok(accepted)
    }
}

enum SshCommand {
    Write(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Disconnect,
}

struct SshSession {
    cmd_tx: tokio::sync::mpsc::UnboundedSender<SshCommand>,
}

pub struct SshSessionManager {
    sessions: HashMap<String, SshSession>,
    ssh_handles: HashMap<String, Arc<client::Handle<SshClient>>>,
    sftp_cache: HashMap<String, Arc<Mutex<SftpSession>>>,
    app: AppHandle,
    secret_store_dir: PathBuf,
    known_hosts_path: PathBuf,
    pending_host_decisions: PendingDecisions,
}

impl SshSessionManager {
    pub fn new(app: AppHandle, secret_store_dir: PathBuf, known_hosts_path: PathBuf) -> Self {
        Self {
            sessions: HashMap::new(),
            ssh_handles: HashMap::new(),
            sftp_cache: HashMap::new(),
            app,
            secret_store_dir,
            known_hosts_path,
            pending_host_decisions: Arc::new(Mutex::new(HashMap::new())),
        }
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

async fn connect_authenticated(
    state: &State<'_, Arc<Mutex<SshSessionManager>>>,
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

    let (app, secret_store_dir, known_hosts_path, pending) = {
        let mgr = state.lock().await;
        (
            mgr.app.clone(),
            mgr.secret_store_dir.clone(),
            mgr.known_hosts_path.clone(),
            mgr.pending_host_decisions.clone(),
        )
    };

    let handler = SshClient {
        app,
        host_port: format!("{host}:{port}"),
        known_hosts_path,
        pending,
    };
    let mut handle = client::connect(config, (host, port), handler)
        .await
        .map_err(|e| format!("SSH connect failed: {e}"))?;

    let auth_ok = if auth_method.as_deref() == Some("PrivateKey") {
        let key_path = private_key_path.as_deref().unwrap_or("~/.ssh/id_ed25519");
        let expanded = expand_home_path(key_path);
        let resolved_passphrase = if let Some(ref cred_id) = passphrase_credential_id {
            myshelltool_core::SecretStore::new(&secret_store_dir).read(cred_id)
                .map_err(|e| format!("Failed to read passphrase: {e}"))?
        } else {
            passphrase.as_deref().and_then(|p| if p.is_empty() { None } else { Some(p.to_string()) })
        };
        let key_data = std::fs::read(&expanded)
            .map_err(|e| format!("Failed to read key file '{}': {e}", expanded))?;
        let key_str = String::from_utf8_lossy(&key_data);
        let key_pair = russh::keys::decode_secret_key(&key_str, resolved_passphrase.as_deref())
            .map_err(|e| format!("Failed to load private key: {e}"))?;
        let key_with_hash = russh::keys::key::PrivateKeyWithHashAlg::new(
            Arc::new(key_pair),
            None,
        ).map_err(|e| format!("Key wrap failed: {e}"))?;
        handle.authenticate_publickey(username, key_with_hash)
            .await
            .map_err(|e| format!("Public key auth failed: {e}"))?
    } else {
        let resolved_password = if password.is_empty() {
            if let Some(ref cred_id) = credential_id {
                myshelltool_core::SecretStore::new(&secret_store_dir).read(cred_id)
                    .map_err(|e| format!("Failed to read credential: {e}"))?
                    .ok_or_else(|| "Stored credential not found".to_string())?
            } else {
                return Err("No password provided and no stored credential".to_string());
            }
        } else {
            password
        };
        handle.authenticate_password(username, &resolved_password)
            .await
            .map_err(|e| format!("Auth failed: {e}"))?
    };

    if !auth_ok {
        return Err("Authentication failed".to_string());
    }

    Ok(handle)
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
    state: State<'_, Arc<Mutex<SshSessionManager>>>,
    host: String,
    port: u16,
    username: String,
    password: String,
    credential_id: Option<String>,
    auth_method: Option<String>,
    private_key_path: Option<String>,
    passphrase: Option<String>,
    passphrase_credential_id: Option<String>,
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
    ).await {
        Ok(handle) => handle,
        Err(error) => {
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

    channel
        .request_pty(false, "xterm-256color", 80, 24, 0, 0, &[])
        .await
        .map_err(|e| format!("PTY request failed: {e}"))?;

    channel
        .request_shell(true)
        .await
        .map_err(|e| format!("Shell request failed: {e}"))?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let event_name = format!("ssh-output-{session_id}");

    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<SshCommand>();

    {
        let mut mgr = state.lock().await;
        mgr.ssh_handles.insert(session_id.clone(), Arc::new(handle));
        mgr.sessions.insert(session_id.clone(), SshSession { cmd_tx });
    }

    let emit_app = {
        let mgr = state.lock().await;
        mgr.app.clone()
    };

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
                            let _ = emit_app.emit(&event_name, Vec::<u8>::new());
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
                        Some(SshCommand::Disconnect) | None => {
                            let _ = channel.eof().await;
                            let _ = emit_app.emit(&event_name, Vec::<u8>::new());
                            break;
                        }
                    }
                }
            }
        }
    });

    Ok(SshConnectResult {
        session_id,
        connected: true,
        error: None,
    })
}

#[tauri::command]
pub async fn ssh_list_directory(
    state: State<'_, Arc<Mutex<SshSessionManager>>>,
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
    let requested_path = if path.trim().is_empty() { ".".to_string() } else { path };
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
    ).await?;

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
    state: State<'_, Arc<Mutex<SshSessionManager>>>,
    session_id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    let mgr = state.lock().await;
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
    state: State<'_, Arc<Mutex<SshSessionManager>>>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    let mgr = state.lock().await;
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
    state: State<'_, Arc<Mutex<SshSessionManager>>>,
    session_id: String,
) -> Result<(), String> {
    let mut mgr = state.lock().await;
    if let Some(session) = mgr.sessions.remove(&session_id) {
        let _ = session.cmd_tx.send(SshCommand::Disconnect);
    }
    mgr.ssh_handles.remove(&session_id);
    mgr.sftp_cache.remove(&session_id);
    Ok(())
}

#[tauri::command]
pub async fn ssh_confirm_host_key(
    state: State<'_, Arc<Mutex<SshSessionManager>>>,
    request_id: String,
    accepted: bool,
) -> Result<(), String> {
    let mgr = state.lock().await;
    let mut pending = mgr.pending_host_decisions.lock().await;
    if let Some(tx) = pending.remove(&request_id) {
        let _ = tx.send(accepted);
    }
    Ok(())
}

// --- SFTP operations ---

async fn get_or_create_sftp(
    state: &State<'_, Arc<Mutex<SshSessionManager>>>,
    session_id: &str,
) -> Result<Arc<Mutex<SftpSession>>, String> {
    {
        let mgr = state.lock().await;
        if let Some(sftp) = mgr.sftp_cache.get(session_id) {
            return Ok(sftp.clone());
        }
    }

    let handle = {
        let mgr = state.lock().await;
        mgr.ssh_handles.get(session_id)
            .ok_or_else(|| format!("SSH handle for session {session_id} not found"))?
            .clone()  // Arc::clone
    };

    let mut channel = handle.channel_open_session().await
        .map_err(|e| format!("SFTP channel open failed: {e}"))?;
    channel.request_subsystem(true, "sftp").await
        .map_err(|e| format!("SFTP subsystem request failed: {e}"))?;

    let sftp = SftpSession::new(channel.into_stream()).await
        .map_err(|e| format!("SFTP session init failed: {e}"))?;

    let arc = Arc::new(Mutex::new(sftp));
    {
        let mut mgr = state.lock().await;
        mgr.sftp_cache.insert(session_id.to_string(), arc.clone());
    }
    Ok(arc)
}

#[tauri::command]
pub async fn sftp_list_dir(
    state: State<'_, Arc<Mutex<SshSessionManager>>>,
    session_id: String,
    path: String,
) -> Result<RemoteDirectoryList, String> {
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;

    let requested_path = if path.trim().is_empty() { ".".to_string() } else { path };

    let raw_entries = sftp.read_dir(&requested_path).await
        .map_err(|e| format!("SFTP read_dir failed: {e}"))?;

    let mut entries: Vec<RemoteFileEntry> = raw_entries.into_iter().map(|entry| {
        let meta = entry.metadata();
        let kind = if meta.is_dir() {
            "directory"
        } else if meta.is_symlink() {
            "symlink"
        } else {
            "file"
        }.to_string();
        RemoteFileEntry {
            name: entry.file_name(),
            path: entry.path(),
            kind,
            size: meta.len(),
            modified: format!("{:?}", meta.modified()),
        }
    }).collect();

    entries.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));

    Ok(RemoteDirectoryList {
        host: String::new(),
        path: requested_path,
        entries,
    })
}

#[tauri::command]
pub async fn sftp_read_file(
    state: State<'_, Arc<Mutex<SshSessionManager>>>,
    session_id: String,
    path: String,
) -> Result<String, String> {
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;

    let mut file = sftp.open(&path).await
        .map_err(|e| format!("SFTP open failed: {e}"))?;

    use tokio::io::AsyncReadExt;
    let mut contents = String::new();
    file.read_to_string(&mut contents).await
        .map_err(|e| format!("SFTP read failed: {e}"))?;

    Ok(contents)
}

#[tauri::command]
pub async fn sftp_write_file(
    state: State<'_, Arc<Mutex<SshSessionManager>>>,
    session_id: String,
    path: String,
    content: String,
) -> Result<(), String> {
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;

    let mut file = sftp.create(&path).await
        .map_err(|e| format!("SFTP create failed: {e}"))?;

    use tokio::io::AsyncWriteExt;
    file.write_all(content.as_bytes()).await
        .map_err(|e| format!("SFTP write failed: {e}"))?;
    file.shutdown().await
        .map_err(|e| format!("SFTP flush failed: {e}"))?;

    Ok(())
}

#[tauri::command]
pub async fn sftp_mkdir(
    state: State<'_, Arc<Mutex<SshSessionManager>>>,
    session_id: String,
    path: String,
) -> Result<(), String> {
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;
    sftp.create_dir(&path).await
        .map_err(|e| format!("SFTP mkdir failed: {e}"))
}

#[tauri::command]
pub async fn sftp_rename(
    state: State<'_, Arc<Mutex<SshSessionManager>>>,
    session_id: String,
    old_path: String,
    new_path: String,
) -> Result<(), String> {
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;
    sftp.rename(&old_path, &new_path).await
        .map_err(|e| format!("SFTP rename failed: {e}"))
}

#[tauri::command]
pub async fn sftp_remove(
    state: State<'_, Arc<Mutex<SshSessionManager>>>,
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
    }.map_err(|e| format!("SFTP remove failed: {e}"))
}

#[tauri::command]
pub async fn sftp_stat(
    state: State<'_, Arc<Mutex<SshSessionManager>>>,
    session_id: String,
    path: String,
) -> Result<RemoteFileEntry, String> {
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;
    let meta = sftp.metadata(&path).await
        .map_err(|e| format!("SFTP stat failed: {e}"))?;

    let name = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&path)
        .to_string();

    Ok(RemoteFileEntry {
        name,
        path,
        kind: if meta.is_dir() { "directory" } else if meta.is_symlink() { "symlink" } else { "file" }.to_string(),
        size: meta.len(),
        modified: format!("{:?}", meta.modified()),
    })
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
            }.to_string(),
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

fn save_known_hosts(
    path: &PathBuf,
    hosts: &HashMap<String, KnownHostEntry>,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(hosts).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}
