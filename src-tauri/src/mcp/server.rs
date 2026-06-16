//! MCP stdio server 主循环 + ServerHandler 实现（Layer 2）。
//!
//! M2 阶段：建立 rmcp stdio server，响应 `initialize`（返回 serverInfo +
//! capabilities）与 `tools/list`（返回空列表，Layer 3 填充）。
//!
//! 见 docs/plans/MCP服务接入-实施计划.md §4（Layer 2）。
//!
//! API 依据：rmcp 1.7.0（本地 cargo 缓存源码核查，2026-06-16）
//! - `ServerHandler::get_info(&self) -> ServerInfo`（= `InitializeResult`）
//! - `ServerInfo` 经 `InitializeResult::new(caps).with_server_info(impl_info)` 构造
//! - `serve` 在 `ServiceExt` trait 上，需 `use rmcp::ServiceExt`
//! - transport 用 `rmcp::transport::stdio()`（返回 `(Stdin, Stdout)` 元组，
//!   实现 `IntoTransport`）

use rmcp::{
    ServerHandler, ServiceExt,
    model::{Implementation, InitializeResult, ServerCapabilities, ServerInfo},
};

/// MCP server handler。
///
/// M2 阶段为最小骨架：仅实现 `get_info` 返回服务元数据。
/// Layer 3/4/5 在此 struct 上追加 list_tools / call_tool /
/// list_resources / list_prompts 等方法。
#[derive(Clone, Debug)]
pub struct MyshellToolMcpServer;

impl MyshellToolMcpServer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MyshellToolMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerHandler for MyshellToolMcpServer {
    /// 返回服务元数据 + 能力声明。
    ///
    /// 声明支持 tools（Layer 3）、resources（Layer 4）、prompts（Layer 5）。
    /// M2 阶段三原语实现为空，但 capabilities 先声明，避免客户端误判不支持。
    fn get_info(&self) -> ServerInfo {
        // ServerCapabilities 是 #[non_exhaustive]，必须用 builder 构造。
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_prompts()
            .build();
        InitializeResult::new(capabilities)
            .with_server_info(Implementation::new(
                "myshelltool",
                env!("CARGO_PKG_VERSION"),
            ))
    }
}

/// MCP stdio server 主入口（被 `lib::run_mcp_stdio` 调用）。
///
/// Layer 2：落地 rmcp stdio serve 主循环，替换 M1 的桩。
/// - 用 `rmcp::transport::stdio()` 绑定 stdin/stdout 作 JSON-RPC 通道
/// - rmcp 内部独占 stdout（仅写协议帧），我们的日志必须只写 stderr
/// - Layer 7（降级）会在此函数开头加 GUI 检测 + 只读降级分支
pub async fn serve_stdio() -> Result<(), Box<dyn std::error::Error>> {
    log::info!("myshelltool-mcp stdio server starting");

    let handler = MyshellToolMcpServer::new();
    // transport::stdio() 返回 (Stdin, Stdout) 元组，实现 IntoTransport。
    let transport = rmcp::transport::stdio();
    let service = handler.serve(transport).await?;

    log::info!("myshelltool-mcp stdio server serving");
    service.waiting().await?;

    log::info!("myshelltool-mcp stdio server stopped");
    Ok(())
}
