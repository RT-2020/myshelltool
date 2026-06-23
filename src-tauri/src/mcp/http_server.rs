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
//! 优先绑 `127.0.0.1:41235`（与 vite dev 41234 区分）。被占用则 +1 重试，
//! 最多重试 10 次。**只监听 localhost，绝不监听 0.0.0.0**（AGENTS.md §8 安全红线）。
//! 实际监听地址写入 `<data_dir>/mcp-endpoint.json`，供前端展示 + 用户配置 host。
//!
//! ## 生命周期
//!
//! 由 lib.rs setup 经 `tauri::async_runtime::spawn` 拉起，持有 CancellationToken；
//! GUI 退出时取消 token 触发 axum graceful shutdown。无独立子进程，无孤儿风险。

use std::path::Path;

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
/// MCP HTTP server 默认端口（与 vite dev server 41234 区分）。
pub const DEFAULT_BIND_PORT: u16 = 41235;
/// 端口被占用时的最大重试次数。
const MAX_PORT_RETRIES: u16 = 10;

/// 实际监听地址（写入 mcp-endpoint.json + 返回前端）。
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct McpEndpoint {
    pub url: String,
    pub host: String,
    pub port: u16,
}

/// 绑定 TCP 端口：从 DEFAULT_BIND_PORT 开始，被占用则 +1，最多重试 MAX_PORT_RETRIES 次。
///
/// 返回 (listener, 实际绑定的 port)。失败返回最后一个错误。
async fn bind_with_port_fallback() -> Result<(tokio::net::TcpListener, u16), std::io::Error> {
    let mut last_err = None;
    for offset in 0..=MAX_PORT_RETRIES {
        let port = DEFAULT_BIND_PORT.saturating_add(offset);
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
fn persist_endpoint(data_dir: &Path, endpoint: &McpEndpoint) {
    let path = data_dir.join("mcp-endpoint.json");
    match serde_json::to_string_pretty(endpoint) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("MCP HTTP: failed to write {}: {e}", path.display());
            }
        }
        Err(e) => log::warn!("MCP HTTP: failed to serialize endpoint: {e}"),
    }
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
    };
    persist_endpoint(&data_dir, &endpoint);
    log::info!("MCP HTTP server serving at {}", endpoint.url);

    // handler factory：每个 MCP 会话独立构造一个 MyshellToolMcpServer（ctx 是 Clone 的 Arc）。
    // LocalSessionManager = 单进程内存会话管理（不做跨进程/分布式会话）。
    let service = StreamableHttpService::new(
        move || Ok(MyshellToolMcpServer::new(ctx.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_cancellation_token(shutdown_token.child_token()),
    );
    let router = axum::Router::new().nest_service("/mcp", service);

    // axum::serve 在 listener 上跑直到 graceful shutdown（shutdown_token 取消时）。
    if let Err(e) = axum::serve(listener, router)
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
