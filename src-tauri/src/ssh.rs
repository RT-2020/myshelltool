use async_trait::async_trait;
use log::{error, info, warn};
use russh::client;
use russh::keys::PublicKeyBase64;
use russh::ChannelMsg;
use russh::Pty;
use russh_sftp::client::SftpSession;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{oneshot, Mutex};

use crate::AppState;
type PendingDecisions = Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>;

type PendingKeyboardResponses = Arc<Mutex<HashMap<String, oneshot::Sender<Vec<String>>>>>;
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

/// 会话连接元数据（host:port:username）。
///
/// v1.1 引入：用于 MCP named pipe 会话映射——MCP 工具拿到的标识是
/// `asset_id`，而 GUI 的会话池用 `session_id`（UUID）。要复用 GUI
/// 会话，必须把 `session_id` 反查到其连接的 `host:port:username`，
/// 再与资产库里的 `ConnectionAsset` 匹配。`ssh_connect` 时写入，
/// `ssh_disconnect` 时清理。
#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub host: String,
    pub port: u16,
    pub username: String,
}
pub struct SshSessionManager {
    sessions: HashMap<String, SshSession>,
    ssh_handles: HashMap<String, Arc<client::Handle<SshClient>>>,
    /// v1.1：session_id → 连接元数据，用于 MCP pipe 会话映射（asset_id → session_id）。
    session_meta: HashMap<String, SessionMeta>,
    sftp_cache: HashMap<String, Arc<Mutex<SftpSession>>>,
    tunnels: HashMap<String, TunnelStatus>,
    tunnel_handles: HashMap<String, tokio::task::JoinHandle<()>>,
    /// transfer_id → 上传取消旗标。
    ///
    /// 流式上传（sftp_upload_from_file）是单条 invoke 跑全程，没有独立 writer
    /// 任务可 abort：取消靠这面共享旗标（循环逐块检查）。条目带 session_id，
    /// cleanup_session_tables 据此把会话的在途上传全部置旗——否则断连后
    /// 上传循环要等产品级写超时（120s）才停。
    upload_cancels: Arc<Mutex<HashMap<String, UploadCancelEntry>>>,
    app: AppHandle,
    secret_store_dir: PathBuf,
    known_hosts_path: PathBuf,
    pending_host_decisions: PendingDecisions,
    pending_keyboard: PendingKeyboardResponses,
}

impl SshSessionManager {
    /// GUI 模式构造（持有 AppHandle，host key/keyboard 可经事件弹窗）。
    pub fn new(app: AppHandle, secret_store_dir: PathBuf, known_hosts_path: PathBuf) -> Self {
        Self {
            sessions: HashMap::new(),
            ssh_handles: HashMap::new(),
            session_meta: HashMap::new(),
            sftp_cache: HashMap::new(),
            tunnels: HashMap::new(),
            tunnel_handles: HashMap::new(),
            upload_cancels: Arc::new(Mutex::new(HashMap::new())),
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

    /// 当前活跃会话 ID 列表（MCP `list_sessions` 工具用）。
    pub fn list_session_ids(&self) -> Vec<String> {
        self.sessions.keys().cloned().collect()
    }

    /// 会话是否存在（MCP 工具参数校验用）。
    pub fn has_session(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    /// v1.1：在已建立的会话上执行一次性命令（开新 channel，不干扰交互 PTY）。
    ///
    /// 这是 MCP named pipe 复用 GUI 会话的核心方法：MCP 工具的 `disk_usage`/
    /// `system_status`/`ssh_exec` 等经 pipe 调用本方法，免去 headless 重连开销，
    /// 也免去二次 host key 验证。命令执行完通道即关闭，不影响终端 PTY。
    ///
    /// 失败场景：
    /// - 会话不存在（已断开/从未连接）→ `session {id} not found`
    /// - channel 打不开/exec 失败 → 底层 russh 错误
    pub async fn exec_on_session(
        &self,
        session_id: &str,
        command: &str,
    ) -> Result<String, String> {
        let handle = self
            .ssh_handles
            .get(session_id)
            .ok_or_else(|| format!("session {} not found", session_id))?
            .clone();

        let mut channel = handle
            .channel_open_session()
            .await
            .map_err(|e| format!("Channel open failed: {e}"))?;
        channel
            .exec(true, command.to_string())
            .await
            .map_err(|e| format!("Command exec failed: {e}"))?;

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

        let mut result = String::from_utf8_lossy(&output).to_string();
        if !errors.is_empty() {
            let err_str = String::from_utf8_lossy(&errors).trim().to_string();
            if !err_str.is_empty() {
                result.push('\n');
                result.push_str("[stderr] ");
                result.push_str(&err_str);
            }
        }
        Ok(result)
    }

    /// v1.1：按 `host:port:username` 反查 session_id（MCP pipe 会话映射）。
    ///
    /// MCP 工具持有 `asset_id`，经资产库解析出 `host:port:username`，
    /// 再调本方法找到 GUI 已建立的会话。username 不匹配时仍按 host:port
    /// 返回（容错：同主机多账号场景以 host:port 为主键）。
    pub fn find_session_by_host(&self, host: &str, port: u16, username: &str) -> Option<String> {
        // 优先全匹配
        for (sid, meta) in &self.session_meta {
            if meta.host == host && meta.port == port && meta.username == username {
                return Some(sid.clone());
            }
        }
        // 退化：仅 host:port 匹配（同主机账号容错）
        for (sid, meta) in &self.session_meta {
            if meta.host == host && meta.port == port {
                return Some(sid.clone());
            }
        }
        None
    }

    /// v1.1：列出所有会话及其连接元数据（MCP `list_sessions` 工具用，供 AI 判断可复用哪个会话）。
    pub fn list_sessions_with_meta(&self) -> Vec<(String, SessionMeta)> {
        self.session_meta
            .iter()
            .map(|(sid, meta)| (sid.clone(), meta.clone()))
            .collect()
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
    /// 权限八进制串（如 "755"）。来自 SFTP FileAttributes.permissions。
    /// None 表示后端未提供（老 SFTP server）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
    /// 属主用户名（如 "root"）。None 表示未提供。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// 属主组名（如 "root"）。None 表示未提供。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteDirectoryList {
    pub host: String,
    pub path: String,
    pub entries: Vec<RemoteFileEntry>,
}

// ─── 按域拆出的子模块（architecture-log Target 1；命令名不变，前端零感知）───
mod headless;
mod known_hosts;
mod monitor;
mod session;
mod sftp;
mod tunnel;

// 子模块公开符号经本模块路径再导出（lib.rs generate_handler / mcp 调用点不变）。
// 必须用 glob 而非逐项列表：#[tauri::command] 生成的隐藏宏（__cmd__X 等）
// 随 glob 一并可见，generate_handler 按 `ssh::命令名` 解析时才找得到。
pub use known_hosts::*;
pub use monitor::*;
pub use headless::*;
pub use session::*;
pub use sftp::*;
pub use tunnel::*;

/// 会话级清理（对外入口）：摘掉全局表项 + 停掉该会话的隧道与资源监控。
///
/// 为什么需要独立函数：清理原先只存在于 `ssh_disconnect` 里，而**远端断开并不经
/// 过 `ssh_disconnect`**——PTY 任务收到 EOF 只会 emit 事件然后 break。结果是
/// `ssh_handles` / `session_meta` / `sftp_cache` / 隧道全部残留，后续 SFTP 请求
/// 一直命中已经不存在的会话，用户必须手动点一次「断开」才能恢复。
///
/// **可重入**：键不存在时全是 no-op，因此 `ssh_disconnect` 与 PTY 任务退出两侧
/// 可以都调用它而不会互相干扰。
///
/// 注意：它**不**负责向 PTY 任务发送 `SshCommand::Disconnect`——那需要先从
/// `sessions` 表里取出 `cmd_tx`，属于调用方（`ssh_disconnect`）的时序职责。
pub(crate) async fn cleanup_session(state: &AppState, session_id: &str) {
    cleanup_session_tables(&state.ssh_sessions, session_id).await;
    stop_session_monitor(&state.resource_monitors, session_id);
}

/// 摘掉某 session 在 `SshSessionManager` 里的全部表项（会话、句柄、元数据、
/// SFTP 缓存、在途上传）并 abort 它的隧道 / 上传任务。
///
/// 只依赖 `ssh_sessions` 这一个 Arc，因此会话 PTY 任务（拿不到 `AppState`）也能
/// 直接调用。锁边界（关键）：持锁时**只做纯内存删除**，绝不 await IO、绝不 emit。
pub(crate) async fn cleanup_session_tables(
    ssh_sessions: &Arc<tokio::sync::Mutex<SshSessionManager>>,
    session_id: &str,
) {
    let mut mgr = ssh_sessions.lock().await;
    mgr.sessions.remove(session_id);
    mgr.ssh_handles.remove(session_id);
    mgr.session_meta.remove(session_id);
    // 仅移除 sftp_cache 是「假清理」：在途上传的 writer 任务自己持有 File，
    // 而 File 握着一个远端 SFTP channel，会一直占 OpenSSH MaxSessions 配额。
    mgr.sftp_cache.remove(session_id);
    // 该会话的隧道：取出 JoinHandle 后 abort，避免隧道任务在会话消失后继续跑
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
    // 该会话的在途上传：置取消旗标并摘条目。流式上传没有 writer 任务可 abort，
    // 循环在下一个块边界看到旗标即停（随后的 SFTP 写多半也会因会话已死而失败，
    // 两条路径殊途同归）。找不到该会话的传输时是纯 no-op（本函数可重入）。
    {
        let mut cancels = mgr.upload_cancels.lock().await;
        let transfer_ids: Vec<String> = cancels
            .iter()
            .filter(|(_, entry)| entry.session_id == session_id)
            .map(|(id, _)| id.clone())
            .collect();
        for tid in transfer_ids {
            if let Some(entry) = cancels.remove(&tid) {
                entry.flag.store(true, Ordering::SeqCst);
            }
        }
    }
}

/// 停掉某 session 的资源监控轮询任务。
///
/// 与 `SshSessionManager` 是两把独立的锁（这里是 std Mutex，另一个是 tokio
/// 异步 Mutex），因此**必须分开调用**：先结束 `ssh_sessions` 的作用域、再取这把
/// 锁，避免两把锁嵌套。清理阶段不因锁中毒而失败（中毒时跳过即可）。
pub(crate) fn stop_session_monitor(
    resource_monitors: &Arc<std::sync::Mutex<crate::resource_monitor::ResourceMonitorState>>,
    session_id: &str,
) {
    if let Ok(mut monitors) = resource_monitors.lock() {
        if let Some(handle) = monitors.handles.remove(session_id) {
            let _ = handle.cancel.send(());
            info!("resource_monitor: stopped monitor for {session_id} (via cleanup_session)");
        }
    }
}

#[tauri::command]
pub async fn ssh_disconnect(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    // 时序不能变：必须先发 Disconnect（要取出 cmd_tx），再做表项清理。
    {
        let mut mgr = state.ssh_sessions.lock().await;
        if let Some(session) = mgr.sessions.remove(&session_id) {
            let _ = session.cmd_tx.send(SshCommand::Disconnect);
            info!("SSH session {session_id} disconnected");
        }
    }
    cleanup_session(&state, &session_id).await;
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
