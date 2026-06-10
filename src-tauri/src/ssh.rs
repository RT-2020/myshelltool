use async_trait::async_trait;
use russh::client;
use russh::ChannelMsg;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

struct SshClient;

#[async_trait]
impl client::Handler for SshClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

enum SshCommand {
    Write(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Disconnect,
}

pub struct SshSession {
    cmd_tx: tokio::sync::mpsc::UnboundedSender<SshCommand>,
}

pub struct SshSessionManager {
    sessions: HashMap<String, SshSession>,
    app: AppHandle,
}

impl SshSessionManager {
    pub fn new(app: AppHandle) -> Self {
        Self {
            sessions: HashMap::new(),
            app,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SshConnectResult {
    pub session_id: String,
    pub connected: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn ssh_connect(
    state: State<'_, Arc<Mutex<SshSessionManager>>>,
    host: String,
    port: u16,
    username: String,
    password: String,
) -> Result<SshConnectResult, String> {
    let config = Arc::new(client::Config::default());

    let handler = SshClient;
    let mut handle = client::connect(config, (host.as_str(), port), handler)
        .await
        .map_err(|e| format!("SSH connect failed: {e}"))?;

    let auth_ok = handle
        .authenticate_password(username, password)
        .await
        .map_err(|e| format!("Auth failed: {e}"))?;

    if !auth_ok {
        return Ok(SshConnectResult {
            session_id: String::new(),
            connected: false,
            error: Some("Authentication failed".to_string()),
        });
    }

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
        mgr.sessions.insert(session_id.clone(), SshSession { cmd_tx });
    }

    let emit_app = {
        let mgr = state.lock().await;
        mgr.app.clone()
    };

    // This task exclusively owns the channel
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
    Ok(())
}
