//! GUI 会话的轻量通道命令（ssh_write/ssh_resize）——v0.20 自 session.rs 拆出
//!（session.rs 贴 800 行 Rust 硬限；SSH P1 连接参数字段所致）。

use tauri::State;

use crate::AppState;
use super::SshCommand;

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
