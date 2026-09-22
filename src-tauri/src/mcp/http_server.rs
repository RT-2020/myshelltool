//! MCP Streamable HTTP server（v1.4：MCP 内嵌 GUI）。
//!
//! 取代 v1.0-v1.3 的「独立 myshelltool-mcp.exe + stdio + named pipe 桥」架构。
//! MCP server 直接跑在 GUI 进程内，用 Streamable HTTP transport 对外暴露，
//! 任何合规 MCP host（Claude Code / Cursor 等）经 http://127.0.0.1:<port>/mcp 连入。
//!
//! ## 为什么内嵌（取代双进程）
//!
//! - **单 exe 单安装包**：不再需要打包独立 mcp exe，根治 v1.3 的打包缺口。
//! - **根治僵尸进程**：不再 spawn 子进程做探测，从源头消除 os error 32。
//! - **消除 pipe 桥**：MCP server 与 SSH 会话/资产/审批同进程，直接内存访问。
//! - **绕开 windows_subsystem 冲突**：HTTP transport 不依赖 stdin/stdout，
//!   GUI 用 windows 子系统不受影响（stdio 才需要 console 子系统）。
//!
//! ## 端口策略
//!
//! 起始端口 = `MYSHELLTOOL_MCP_PORT` 环境变量（显式覆盖，三态解析见
//! `myshelltool_core::port_env`）> 按构建形态取默认：release/正式版 41235，
//! debug（`tauri:dev`）41500——开发实例与已安装正式版并行时错开起始端口，
//! 防止先启动的开发实例抢走 41235（MCP host 配置的 URL 固定指向正式版端口，
//! 被开发实例占了 host 就连错进程：审批弹窗弹在开发窗口、dev 重编译时连接反复断）。
//! 被占用则 +1 重试，最多重试 10 次。**只监听 localhost，绝不监听 0.0.0.0**
//! （AGENTS.md §8 安全红线）。实际监听地址写入 `<data_dir>/mcp-endpoint.json`，
//! 供前端展示 + 用户配置 host。
//!
//! ## 鉴权（v0.20，A1：URL 内嵌 token）
//!
//! 「只监听 127.0.0.1」在多进程桌面上不是安全边界：本机任意进程（含同机其他
//! Windows 用户）都能连回环端口，而工具面含 ssh_exec。因此入口加 token：
//! 启动时生成 256-bit CSPRNG token，路由形态 `/mcp/<token>`（同时兼容
//! `Authorization: Bearer` header），无 token 一律 401——不区分「路径错」与
//! 「token 错」，不做探测预言机。判定逻辑在 `myshelltool_core::mcp_auth`
//! （安全判据 core 真跑）。token 只写进 mcp-endpoint.json（authToken 字段）
//! 与内存（AppState 共享 Arc<RwLock>），**绝不进日志**（凭据红线延伸）；
//! 拒绝日志只记脱敏后的路径摘要（redacted_path_summary）。
//!
//! ## 生命周期
//!
//! 由 lib.rs setup 经 `tauri::async_runtime::spawn` 拉起，持有 CancellationToken；
//! GUI 退出时取消 token 触发 axum graceful shutdown。无独立子进程，无孤儿风险。

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, RwLock};

use axum::extract::{ConnectInfo, Request};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService,
    session::local::LocalSessionManager,
};
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use super::server::MyshellToolMcpServer;
use super::tools::McpToolContext;

/// MCP HTTP server 默认监听地址（127.0.0.1，仅本机）。
pub const DEFAULT_BIND_HOST: &str = "127.0.0.1";
/// release/正式版默认端口（与 vite dev 41234 区分）。
#[cfg(not(debug_assertions))]
pub const DEFAULT_BIND_PORT: u16 = 41235;
/// debug（`tauri:dev` / debug 构建）默认端口：错开正式版的 41235——开发实例
/// 与已安装正式版并行时不再抢占 host 固定指向正式版的连接（见模块头「端口策略」）。
/// 要在 debug 构建下复现正式端口行为，设 `MYSHELLTOOL_MCP_PORT=41235` 覆盖。
#[cfg(debug_assertions)]
pub const DEFAULT_BIND_PORT: u16 = 41500;
/// 端口被占用时的最大重试次数。
const MAX_PORT_RETRIES: u16 = 10;

/// 实际监听地址（写入 mcp-endpoint.json + 返回前端）。
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct McpEndpoint {
    /// 基础 URL（**不含 token**，形如 http://127.0.0.1:41235/mcp）——
    /// 日志/状态展示用它；含 token 的完整 URL 由调用方现场拼接，避免 token 扩散。
    pub url: String,
    pub host: String,
    pub port: u16,
    /// v0.20（A1）：入口鉴权 token。`#[serde(default)]` 兼容旧版文件（缺字段读出
    /// 空串 → core::mcp_auth fail-closed 全拒；server 启动后重写本文件补齐）。
    #[serde(default, rename = "authToken")]
    pub auth_token: String,
}

/// 生成 256-bit 入口鉴权 token（CSPRNG → base64url 无 padding，43 字符）。
pub fn generate_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    myshelltool_core::mcp_auth::base64url_nopad(&bytes)
}

/// 把基础 URL 与 token 拼成含 token 的完整接入 URL（面板复制 / 探测用）。
pub fn with_token(base_url: &str, token: &str) -> String {
    if token.is_empty() {
        return base_url.to_string();
    }
    format!("{}/{}", base_url.trim_end_matches('/'), token)
}

/// 解析起始监听端口：`MYSHELLTOOL_MCP_PORT` 优先，其次按构建形态取默认。
/// 三态口径与 `MYSHELLTOOL_DATA_DIR`（lib.rs::mcp_data_dir）一致：未设置 = 默认；
/// 设置了但不可用（空/非数字/0/超范围）= warn 让用户看见后回退默认，
/// 不静默折叠成「未配置」——否则用户以为端口改了，实际还监听在老端口。
fn resolve_base_port() -> u16 {
    match myshelltool_core::parse_port(
        std::env::var_os("MYSHELLTOOL_MCP_PORT").as_deref(),
    ) {
        myshelltool_core::PortOverride::Port(p) => p,
        myshelltool_core::PortOverride::NotSet => DEFAULT_BIND_PORT,
        myshelltool_core::PortOverride::Invalid(raw) => {
            log::warn!(
                "MYSHELLTOOL_MCP_PORT 存在但不是合法端口（{raw:?}），已忽略并回退默认端口 {DEFAULT_BIND_PORT}"
            );
            DEFAULT_BIND_PORT
        }
    }
}

/// 绑定 TCP 端口：从起始端口（`resolve_base_port`）开始，被占用则 +1，
/// 最多重试 MAX_PORT_RETRIES 次。
///
/// 返回 (listener, 实际绑定的 port)。失败返回最后一个错误。
async fn bind_with_port_fallback() -> Result<(tokio::net::TcpListener, u16), std::io::Error> {
    let base_port = resolve_base_port();
    let mut last_err = None;
    for offset in 0..=MAX_PORT_RETRIES {
        let port = base_port.saturating_add(offset);
        let addr = format!("{DEFAULT_BIND_HOST}:{port}");
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                log::info!("MCP HTTP server bound on {addr}");
                return Ok((listener, port));
            }
            Err(e) => {
                log::warn!("MCP HTTP bind {addr} failed ({e}), trying next port");
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::AddrInUse, "MCP HTTP bind: all ports exhausted")
    }))
}

/// 把实际监听地址写入 `<data_dir>/mcp-endpoint.json`（前端读它展示 + 用户配置 host）。
/// v0.20：返回 Result——mcp_reset_token 命令必须感知落盘失败（写不出去却宣称
/// 重置成功 = 用户拿旧 token 的 host 全部失灵且无从归因）；启动路径仍是 warn 不致命。
pub fn persist_endpoint(data_dir: &Path, endpoint: &McpEndpoint) -> Result<(), String> {
    let path = data_dir.join("mcp-endpoint.json");
    let json = serde_json::to_string_pretty(endpoint)
        .map_err(|e| format!("序列化 endpoint 失败: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("写入 {} 失败: {e}", path.display()))
}

/// 鉴权中间件（A1）：URL 路径内嵌 token 或 Bearer header 二选一，统一 401。
/// 判定委托 core::mcp_auth（安全判据单测在 core）；这里只做 HTTP 形态适配：
/// AllowRewrite 时把 URI 的 token 段剥掉再进路由（query 保留）。
async fn mcp_auth_gate(
    axum::extract::State(auth): axum::extract::State<Arc<RwLock<String>>>,
    req: Request,
    next: Next,
) -> Response {
    // 读锁只取快照立即释放（短临界区，std RwLock 不跨 await）；锁中毒 → 空串
    // → core decide fail-closed 全拒（宁可不可用，不裸奔）。
    let token = auth.read().map(|g| g.clone()).unwrap_or_default();
    let bearer = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let path = req.uri().path().to_string();

    match myshelltool_core::mcp_auth::decide(&path, bearer, &token) {
        myshelltool_core::mcp_auth::McpAuthDecision::Allow => next.run(req).await,
        myshelltool_core::mcp_auth::McpAuthDecision::AllowRewrite(new_path) => {
            // 剥掉 token 段再进 nest_service（query 原样保留）。重写失败（非法
            // URI 字符）按保守处理 = 拒绝，不放行形态不明的请求。
            let pq = match req.uri().query() {
                Some(q) => format!("{new_path}?{q}"),
                None => new_path,
            };
            match pq.parse::<axum::http::Uri>() {
                Ok(uri) => {
                    let mut req = req;
                    *req.uri_mut() = uri;
                    next.run(req).await
                }
                Err(_) => deny(&path, req.extensions().get::<ConnectInfo<SocketAddr>>()),
            }
        }
        myshelltool_core::mcp_auth::McpAuthDecision::Deny => {
            deny(&path, req.extensions().get::<ConnectInfo<SocketAddr>>())
        }
    }
}

/// 统一 401：不区分「路径错」与「token 错」（防探测预言机）。日志只记脱敏路径
/// 摘要 + 来源地址——请求路径可能含猜错的 token，整段不落盘。
fn deny(path: &str, peer: Option<&ConnectInfo<SocketAddr>>) -> Response {
    let summary = myshelltool_core::mcp_auth::redacted_path_summary(path);
    match peer {
        Some(ConnectInfo(addr)) => {
            log::warn!("MCP HTTP 拒绝未授权请求：{summary}，来源 {addr}")
        }
        None => log::warn!("MCP HTTP 拒绝未授权请求：{summary}"),
    }
    StatusCode::UNAUTHORIZED.into_response()
}

/// 启动 MCP Streamable HTTP server（GUI 进程内，阻塞运行直到 CancellationToken 取消）。
///
/// 由 lib.rs setup 经 `tauri::async_runtime::spawn` 调用。data_dir 用于写
/// mcp-endpoint.json；ctx 是 MCP 工具上下文（资产/凭据路径）。shutdown_token
/// 在 GUI 退出时取消，触发 axum graceful shutdown。
pub async fn run_http_server(
    ctx: McpToolContext,
    data_dir: std::path::PathBuf,
    shutdown_token: CancellationToken,
    auth_token: Arc<RwLock<String>>,
) {
    let (listener, port) = match bind_with_port_fallback().await {
        Ok(v) => v,
        Err(e) => {
            log::error!("MCP HTTP server: failed to bind any port: {e}");
            return;
        }
    };

    let endpoint = McpEndpoint {
        url: format!("http://{DEFAULT_BIND_HOST}:{port}/mcp"),
        host: DEFAULT_BIND_HOST.to_string(),
        port,
        auth_token: auth_token.read().map(|g| g.clone()).unwrap_or_default(),
    };
    // 启动期落盘失败 warn 不致命（面板会显示「server 未启动」态），但绝不日志 token。
    if let Err(e) = persist_endpoint(&data_dir, &endpoint) {
        log::warn!("MCP HTTP: {e}");
    }
    log::info!("MCP HTTP server serving at {}（已启用 token 鉴权）", endpoint.url);

    // handler factory：每个 MCP 会话独立构造一个 MyshellToolMcpServer（ctx 是 Clone 的 Arc）。
    // LocalSessionManager = 单进程内存会话管理（不做跨进程/分布式会话）。
    let service = StreamableHttpService::new(
        move || Ok(MyshellToolMcpServer::new(ctx.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_cancellation_token(shutdown_token.child_token()),
    );
    let router = axum::Router::new()
        .nest_service("/mcp", service)
        // 兜底同样 401：与鉴权拒绝同码同体，不区分「路径不存在」与「token 错」
        .fallback(|| async { StatusCode::UNAUTHORIZED })
        .layer(middleware::from_fn_with_state(auth_token, mcp_auth_gate));

    // axum::serve 在 listener 上跑直到 graceful shutdown（shutdown_token 取消时）。
    // ConnectInfo 注入让 401 日志能记来源地址。
    if let Err(e) = axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async move { shutdown_token.cancelled().await })
        .await
    {
        log::error!("MCP HTTP server exited with error: {e}");
    }
    log::info!("MCP HTTP server stopped");
}

/// 读取已持久化的 endpoint（启动时若 server 还没起来，返回 None）。
/// 供 mcp_status 命令读取实际监听地址返回前端。
pub fn read_endpoint(data_dir: &Path) -> Option<McpEndpoint> {
    let path = data_dir.join("mcp-endpoint.json");
    let json = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&json).ok()
}
