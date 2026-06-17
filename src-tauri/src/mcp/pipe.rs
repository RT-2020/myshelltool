//! Layer 8：named pipe 桥接（v1.1 M8）。
//!
//! 让独立的 `myshelltool-mcp.exe` 进程复用 GUI 已建立的 SSH 会话，
//! 避免每条命令都 headless 重连（含二次 host key 验证 + 凭据读取开销）。
//!
//! ## 架构
//!
//! ```text
//! ┌─────────────────┐        named pipe           ┌──────────────────────┐
//! │ myshelltool.exe │  \\.\pipe\myshelltool-mcp    │ myshelltool-mcp.exe  │
//! │  (GUI, Windows) │ ◄════════════════════════►  │  (console, MCP)      │
//! │  pipe server    │   JSON 行协议（见下方）      │  pipe client         │
//! │  + SessionMgr   │                             │  + rmcp stdio server │
//! └─────────────────┘                             └──────────────────────┘
//! ```
//!
//! - **GUI 端**：`run_pipe_server` 在 `lib.rs` setup 中 spawn，持有
//!   `Arc<AsyncMutex<SshSessionManager>>`，循环 accept + per-client spawn。
//! - **MCP 端**：`PipeClient::connect` 一次性连接，发请求收响应。
//!
//! ## 协议（JSON 行，扁平字段）
//!
//! 每行一个完整 JSON 对象。请求/响应用 `id` 关联。扁平字段设计避免
//! serde 嵌套 flatten 的复杂性，可读性强且对客户端解析零歧义。
//!
//! - 请求 `exec`：`{id, method:"exec", session_id, command}`
//!   → 成功：`{id, ok:true, output:"..."}`
//!   → 失败：`{id, ok:false, error:"..."}`
//! - 请求 `resolve_session`：`{id, method:"resolve_session", host, port, username}`
//!   → 成功：`{id, ok:true, session_id:"..."}`
//!   → 失败（无匹配会话）：`{id, ok:false, error:"no matching session"}`
//! - 请求 `list_sessions`：`{id, method:"list_sessions"}`
//!   → 成功：`{id, ok:true, sessions:[{session_id,host,port,username}]}`
//!
//! ## 降级语义
//!
//! MCP 端连不上 pipe（GUI 未运行）→ `PipeClient::connect` 返回 `Err`，
//! `tools.rs` 的 `exec_on_asset` 据此降级到 v1.0 headless 建连。
//! 这不是错误，是预期路径之一（GUI 离线时 MCP 仍可用）。

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, Mutex};

use crate::ssh::SshSessionManager;

/// Windows named pipe 路径。
///
/// 固定路径（无 per-user 命名空间）：MCP 进程与 GUI 进程都跑在同一用户会话，
/// 用固定名最简单。多用户同主机不并发是合理假设（桌面 SSH 客户端场景）。
pub const PIPE_NAME: &str = r"\\.\pipe\myshelltool-mcp";

/// GUI 端审批 pending 表：request_id → oneshot sender。
///
/// 当 MCP 进程经 pipe 发来 `request_approval` 请求时，GUI 端生成 request_id，
/// 把 oneshot sender 存入此表，然后 `app.emit` 通知前端弹窗。前端用户点击后
/// 调 `mcp_approval_resolve` 命令，从表中取出 sender 回传 bool。
///
/// 用 `tokio::sync::Mutex`（与 ssh.rs 的 PendingDecisions 一致）。
pub type ApprovalPending = Arc<Mutex<std::collections::HashMap<String, oneshot::Sender<bool>>>>;

/// emit 到前端的审批事件 payload（照 ssh.rs 的 HostKeyVerifyEvent 结构）。
///
/// 前端收到 `mcp-approval-verify` 事件后据 payload 弹三段式确认框。
#[derive(Debug, Clone, Serialize)]
pub struct McpApprovalEvent {
    pub request_id: String,
    pub intent: String,
    pub command: String,
    pub consequence: String,
}

// ─── GUI 端：pipe server ───

/// 启动 named pipe server（GUI 进程调用，应在 setup hook 中 spawn）。
///
/// 持有 `SshSessionManager` 的 Arc + `AppHandle`（用于 emit 审批弹窗事件）+
/// 审批 pending 表，循环 accept 客户端连接，每个连接独立 spawn 一个处理任务。
/// 函数本身永不返回（直到进程退出）。
///
/// 错误处理：创建/accept 失败仅记日志后重试，不 panic——
/// pipe 不可用只是失去会话复用能力，MCP 端会自动降级 headless。
pub async fn run_pipe_server(
    ssh_sessions: Arc<Mutex<SshSessionManager>>,
    app: AppHandle,
    approval_pending: ApprovalPending,
) {
    use tokio::net::windows::named_pipe::ServerOptions;

    log::info!("MCP pipe server starting on {PIPE_NAME}");
    loop {
        // 每轮循环重新创建 server end（Windows named pipe 的使用模式：
        // 一个 server end 对应一次连接，accept 后需新建才能接下一个）。
        let server = match ServerOptions::new()
            .first_pipe_instance(false)
            .create(PIPE_NAME)
        {
            Ok(s) => s,
            Err(e) => {
                log::warn!("MCP pipe: create server end failed: {e}, retry in 1s");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };

        // 阻塞等待客户端连接。
        if let Err(e) = server.connect().await {
            log::warn!("MCP pipe: accept failed: {e}, retry in 1s");
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            continue;
        }

        let ssh = ssh_sessions.clone();
        let app = app.clone();
        let pending = approval_pending.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(server, ssh, app, pending).await {
                log::warn!("MCP pipe: client handler error: {e}");
            }
        });
    }
}

/// 处理单个客户端连接：逐行读请求，逐行写响应。
///
/// 一个连接内可发多条请求（连接复用），客户端断开时函数返回。
async fn handle_client(
    stream: tokio::net::windows::named_pipe::NamedPipeServer,
    ssh_sessions: Arc<Mutex<SshSessionManager>>,
    app: AppHandle,
    approval_pending: ApprovalPending,
) -> Result<(), String> {
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .await
            .map_err(|e| format!("read line: {e}"))?;
        if n == 0 {
            // EOF — 客户端断开
            break;
        }

        let req: PipeRequest = match serde_json::from_str(line.trim()) {
            Ok(r) => r,
            Err(e) => {
                // 协议错误：回一个无法关联的 error（id 未知）
                let resp = PipeResponse::error(String::new(), format!("bad request json: {e}"));
                write_response(&mut write_half, &resp).await?;
                continue;
            }
        };

        let resp = dispatch(&req, &ssh_sessions, &app, &approval_pending).await;
        write_response(&mut write_half, &resp).await?;
    }

    Ok(())
}

async fn write_response(
    writer: &mut tokio::io::WriteHalf<tokio::net::windows::named_pipe::NamedPipeServer>,
    resp: &PipeResponse,
) -> Result<(), String> {
    let mut json = serde_json::to_string(resp).map_err(|e| format!("serialize resp: {e}"))?;
    json.push('\n');
    writer
        .write_all(json.as_bytes())
        .await
        .map_err(|e| format!("write resp: {e}"))?;
    writer.flush().await.map_err(|e| format!("flush: {e}"))?;
    Ok(())
}

/// 分发单个请求。
///
/// `ssh_sessions` 用于 exec/resolve_session/list_sessions；
/// `app` + `approval_pending` 用于 request_approval（emit 弹窗 + oneshot 等待）。
async fn dispatch(
    req: &PipeRequest,
    ssh_sessions: &Arc<Mutex<SshSessionManager>>,
    app: &AppHandle,
    approval_pending: &ApprovalPending,
) -> PipeResponse {
    match req.method.as_str() {
        "exec" => {
            let cmd = req.command.as_deref().unwrap_or("");
            let sid = req.session_id.as_deref().unwrap_or("");
            let mgr = ssh_sessions.lock().await;
            match mgr.exec_on_session(sid, cmd).await {
                Ok(output) => PipeResponse::exec_ok(req.id.clone(), output),
                Err(e) => PipeResponse::error(req.id.clone(), e),
            }
        }
        "resolve_session" => {
            let host = req.host.as_deref().unwrap_or("");
            let port = req.port.unwrap_or(0);
            let username = req.username.as_deref().unwrap_or("");
            let mgr = ssh_sessions.lock().await;
            match mgr.find_session_by_host(host, port, username) {
                Some(sid) => PipeResponse::session_ok(req.id.clone(), sid),
                None => PipeResponse::error(req.id.clone(), "no matching session".into()),
            }
        }
        "list_sessions" => {
            let mgr = ssh_sessions.lock().await;
            let sessions = mgr
                .list_sessions_with_meta()
                .into_iter()
                .map(|(session_id, meta)| SessionInfo {
                    session_id,
                    host: meta.host,
                    port: meta.port,
                    username: meta.username,
                })
                .collect();
            PipeResponse::list_ok(req.id.clone(), sessions)
        }
        // v1.1：MCP 客户端不支持 elicitation 时，经 pipe 委托 GUI 弹原生确认框。
        // 照 ssh.rs check_server_key 的 emit + oneshot + timeout 范式。
        "request_approval" => {
            let intent = req.intent.clone().unwrap_or_default();
            let command = req.command.clone().unwrap_or_default();
            let consequence = req.consequence.clone().unwrap_or_default();

            // 1. 生成 request_id + oneshot，sender 存入 pending 表
            let request_id = uuid::Uuid::new_v4().to_string();
            let (tx, rx) = oneshot::channel();
            {
                let mut map = approval_pending.lock().await;
                map.insert(request_id.clone(), tx);
            }

            // 2. emit 到前端弹窗（失败则清理 pending 并返回 error）
            let event = McpApprovalEvent {
                request_id: request_id.clone(),
                intent,
                command,
                consequence,
            };
            if let Err(e) = app.emit("mcp-approval-verify", event) {
                let mut map = approval_pending.lock().await;
                map.remove(&request_id);
                return PipeResponse::error(req.id.clone(), format!("emit failed: {e}"));
            }

            // 3. 阻塞等待前端回传（60s 超时，照 check_server_key）
            let approved = match tokio::time::timeout(
                std::time::Duration::from_secs(60),
                rx,
            )
            .await
            {
                Ok(Ok(v)) => v,
                Ok(Err(_)) => {
                    // 前端 dropped sender（理论上不会，GUI 关闭时 pending 会清）
                    let mut map = approval_pending.lock().await;
                    map.remove(&request_id);
                    return PipeResponse::error(req.id.clone(), "approval channel closed".into());
                }
                Err(_) => {
                    // 超时：清理 pending 表
                    let mut map = approval_pending.lock().await;
                    map.remove(&request_id);
                    log::warn!("MCP pipe: approval request {request_id} timed out (60s)");
                    return PipeResponse::error(
                        req.id.clone(),
                        "审批超时（60s 内未响应）".into(),
                    );
                }
            };

            PipeResponse::approval_ok(req.id.clone(), approved)
        }
        other => PipeResponse::error(req.id.clone(), format!("unknown method: {other}")),
    }
}

// ─── MCP 端：pipe client ───

/// MCP 进程端的 pipe 客户端。
///
/// 单连接 + `tokio::io::split` 分离读写。协议是单工轮转（发一条等一条），
/// 读写不会并发，但 split 后两端可独立持有（reader 在等响应时 writer 可释放）。
///
/// 连接失败表示 GUI 未运行（或 pipe 未就绪），调用方应降级 headless。
pub struct PipeClient {
    writer: tokio::io::WriteHalf<tokio::net::windows::named_pipe::NamedPipeClient>,
    reader: BufReader<tokio::io::ReadHalf<tokio::net::windows::named_pipe::NamedPipeClient>>,
}

impl PipeClient {
    /// 尝试连接 GUI 的 pipe server。
    ///
    /// 失败（GUI 未运行 / pipe 不存在）返回 `Err`——这是预期降级路径，不是错误。
    pub fn connect() -> Result<Self, String> {
        use tokio::net::windows::named_pipe::ClientOptions;

        let stream = ClientOptions::new()
            .open(PIPE_NAME)
            .map_err(|e| format!("pipe connect failed (GUI 未运行?): {e}"))?;
        // 单连接 split 成读写两半（与 GUI 端 handle_client 的 split 对称）。
        let (read_half, write_half) = tokio::io::split(stream);
        Ok(Self {
            writer: write_half,
            reader: BufReader::new(read_half),
        })
    }

    /// 发送请求并等待响应（按 id 关联）。
    ///
    /// 单工协议：发一条→读一条。id 由调用方指定，便于将来扩展流水线。
    pub async fn request(&mut self, req: &PipeRequest) -> Result<PipeResponse, String> {
        let mut json = serde_json::to_string(req).map_err(|e| format!("serialize req: {e}"))?;
        json.push('\n');
        self.writer
            .write_all(json.as_bytes())
            .await
            .map_err(|e| format!("write req: {e}"))?;
        self.writer
            .flush()
            .await
            .map_err(|e| format!("flush req: {e}"))?;

        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .await
            .map_err(|e| format!("read resp: {e}"))?;
        if line.is_empty() {
            return Err("pipe closed by server".into());
        }
        let resp: PipeResponse =
            serde_json::from_str(line.trim()).map_err(|e| format!("parse resp: {e}"))?;
        if resp.id != req.id {
            return Err(format!(
                "pipe id mismatch: expected {}, got {}",
                req.id, resp.id
            ));
        }
        Ok(resp)
    }
}

/// 便捷方法：按 host:port:username 解析 session 后执行命令（resolve + exec 两步）。
///
/// 返回值语义：
/// - `Ok(Some(output))` — 命中 GUI 会话并执行成功
/// - `Ok(None)` — pipe 通但无匹配会话（调用方降级 headless）
/// - `Err` — pipe 通信故障（调用方降级 headless）
///
/// 两种降级情形都让调用方走 headless，区别仅在日志。
pub async fn resolve_and_exec(
    host: &str,
    port: u16,
    username: &str,
    command: &str,
) -> Result<Option<String>, String> {
    let mut client = match PipeClient::connect() {
        Ok(c) => c,
        Err(e) => {
            log::debug!("MCP pipe connect failed, will degrade to headless: {e}");
            return Ok(None);
        }
    };

    // 1. resolve_session
    let resolve_resp = client
        .request(&PipeRequest {
            id: "r1".into(),
            method: "resolve_session".into(),
            session_id: None,
            command: None,
            host: Some(host.into()),
            port: Some(port),
            username: Some(username.into()),
            intent: None,
            consequence: None,
        })
        .await?;

    let session_id = match resolve_resp {
        PipeResponse {
            ok: true,
            session_id: Some(sid),
            ..
        } => sid,
        PipeResponse { ok: false, error, .. } => {
            log::debug!(
                "MCP pipe: no matching session ({}), degrade to headless",
                error.unwrap_or_default()
            );
            return Ok(None);
        }
        _ => return Err("unexpected resolve response shape".into()),
    };

    // 2. exec
    let exec_resp = client
        .request(&PipeRequest {
            id: "r2".into(),
            method: "exec".into(),
            session_id: Some(session_id),
            command: Some(command.into()),
            host: None,
            port: None,
            username: None,
            intent: None,
            consequence: None,
        })
        .await?;

    match exec_resp {
        PipeResponse {
            ok: true,
            output: Some(out),
            ..
        } => Ok(Some(out)),
        PipeResponse { ok: false, error, .. } => {
            Err(format!("pipe exec failed: {}", error.unwrap_or_default()))
        }
        _ => Err("unexpected exec response shape".into()),
    }
}

/// 便捷方法：经 GUI pipe 委托弹窗审批高危操作（MCP 客户端不支持 elicitation 时调用）。
///
/// 返回值语义：
/// - `Ok(Some(true))` — GUI 弹窗，用户确认执行
/// - `Ok(Some(false))` — GUI 弹窗，用户拒绝
/// - `Ok(None)` — pipe 连不上（GUI 未运行），调用方走 fail-secure 拒绝
/// - `Err` — pipe 通信故障（超时/格式错误），调用方走 fail-secure 拒绝
///
/// server.rs 的 NotSupported 降级分支调用本函数，把「直接拒绝」升级为「GUI 弹窗审批」。
pub async fn request_approval_via_pipe(
    intent: &str,
    command: &str,
    consequence: &str,
) -> Result<Option<bool>, String> {
    let mut client = match PipeClient::connect() {
        Ok(c) => c,
        Err(e) => {
            log::debug!(
                "MCP approval pipe connect failed (GUI 未运行?), fail-secure: {e}"
            );
            return Ok(None);
        }
    };

    let resp = client
        .request(&PipeRequest {
            id: "a1".into(),
            method: "request_approval".into(),
            session_id: None,
            command: Some(command.into()),
            host: None,
            port: None,
            username: None,
            intent: Some(intent.into()),
            consequence: Some(consequence.into()),
        })
        .await?;

    match resp {
        PipeResponse {
            ok: true,
            approved: Some(v),
            ..
        } => {
            log::info!("MCP approval via GUI pipe: approved={v}");
            Ok(Some(v))
        }
        PipeResponse { ok: false, error, .. } => {
            Err(format!("approval pipe error: {}", error.unwrap_or_default()))
        }
        _ => Err("unexpected approval response shape".into()),
    }
}

// ─── 协议类型（扁平字段，serde 零歧义）───

#[derive(Debug, Serialize, Deserialize)]
pub struct PipeRequest {
    pub id: String,
    pub method: String, // "exec" | "resolve_session" | "list_sessions" | "request_approval"
    // exec
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    // resolve_session
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    // request_approval（三段式：intent + command 复用上面 + consequence）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consequence: Option<String>,
}

/// 扁平响应：成功时填 `output`/`session_id`/`sessions`/`approved` 之一，失败填 `error`。
///
/// 用 `ok` 布尔区分成败，而非 `#[serde(tag)]` 枚举——扁平字段对任意客户端
/// 解析都零歧义，也避免 serde 内部枚举 + flatten 的已知坑。
#[derive(Debug, Serialize, Deserialize)]
pub struct PipeResponse {
    pub id: String,
    pub ok: bool,
    // exec 成功
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    // resolve_session 成功
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    // list_sessions 成功
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sessions: Option<Vec<SessionInfo>>,
    // request_approval 成功（用户决定）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved: Option<bool>,
    // 失败
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl PipeResponse {
    pub fn exec_ok(id: String, output: String) -> Self {
        Self {
            id,
            ok: true,
            output: Some(output),
            session_id: None,
            sessions: None,
            approved: None,
            error: None,
        }
    }
    pub fn session_ok(id: String, session_id: String) -> Self {
        Self {
            id,
            ok: true,
            output: None,
            session_id: Some(session_id),
            sessions: None,
            approved: None,
            error: None,
        }
    }
    pub fn list_ok(id: String, sessions: Vec<SessionInfo>) -> Self {
        Self {
            id,
            ok: true,
            output: None,
            session_id: None,
            sessions: Some(sessions),
            approved: None,
            error: None,
        }
    }
    /// request_approval 成功响应：approved = 用户是否确认执行。
    pub fn approval_ok(id: String, approved: bool) -> Self {
        Self {
            id,
            ok: true,
            output: None,
            session_id: None,
            sessions: None,
            approved: Some(approved),
            error: None,
        }
    }
    pub fn error(id: String, message: String) -> Self {
        Self {
            id,
            ok: false,
            output: None,
            session_id: None,
            sessions: None,
            approved: None,
            error: Some(message),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
}

// ─── Tauri 命令：前端审批回传（照 ssh.rs ssh_confirm_host_key 范式）───

/// 前端用户在 GUI 确认框点击后调用此命令，把结果回传给阻塞等待的 pipe dispatch。
///
/// 从 `AppState.mcp_approval_pending` 取出 oneshot sender，send bool。
/// 失败场景：request_id 不存在（超时已被清理 / 重复提交）。
#[tauri::command]
pub async fn mcp_approval_resolve(
    state: tauri::State<'_, crate::AppState>,
    request_id: String,
    approved: bool,
) -> Result<(), String> {
    let mut map = state.mcp_approval_pending.lock().await;
    match map.remove(&request_id) {
        Some(tx) => {
            let _ = tx.send(approved);
            log::info!("mcp_approval_resolve: {request_id} -> {approved}");
            Ok(())
        }
        None => Err(format!(
            "mcp_approval_resolve: request_id {request_id} not found (可能已超时被清理)"
        )),
    }
}
