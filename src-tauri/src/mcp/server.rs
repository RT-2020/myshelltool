//! MCP stdio server 主循环 + ServerHandler 实现（Layer 2 + Layer 3）。
//!
//! M2：rmcp stdio server 骨架（get_info）
//! M3：接入 list_tools（7 个只读工具）+ call_tool（headless exec）
//!
//! 见 docs/plans/MCP服务接入-实施计划.md §4-§5。

use std::sync::Arc;

use rmcp::{
    ServerHandler, ServiceExt,
    model::{
        CallToolRequestParam, CallToolResult, GetPromptRequestParam, GetPromptResult,
        Implementation, InitializeResult, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParam, ReadResourceRequestParams,
        ReadResourceResult, ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    ErrorData as McpError,
};

use super::tools::{self, McpToolContext};

/// MCP server handler。持有工具上下文（资产库/凭据路径）。
#[derive(Clone)]
pub struct MyshellToolMcpServer {
    ctx: Arc<McpToolContext>,
}

impl MyshellToolMcpServer {
    pub fn new(ctx: McpToolContext) -> Self {
        Self {
            ctx: Arc::new(ctx),
        }
    }
}

impl ServerHandler for MyshellToolMcpServer {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_prompts()
            .build();
        InitializeResult::new(capabilities).with_server_info(Implementation::new(
            "myshelltool",
            env!("CARGO_PKG_VERSION"),
        ))
    }

    /// 返回 M3 阶段的 7 个只读工具。
    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        let tools = tools::list_all_tools();
        std::future::ready(Ok(ListToolsResult {
            next_cursor: None,
            tools,
            meta: None,
        }))
    }

    /// 分发工具调用到 tools::call_tool。
    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        let ctx = self.ctx.clone();
        let name = request.name.clone();
        async move {
            match tools::call_tool(name.as_ref(), request, &ctx).await {
                Ok(result) => Ok(result),
                Err(e) => {
                    log::warn!("MCP call_tool error: {}", e);
                    // 用 is_error 的 CallToolResult 返回，而非 McpError
                    // （MCP 规范建议工具失败用 isError 标记，不抛协议错误）
                    let mut result = CallToolResult::success(vec![rmcp::model::Content::text(e)]);
                    result.is_error = Some(true);
                    Ok(result)
                }
            }
        }
    }

    /// 列出 3 个静态资源（Layer 4）。
    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, McpError>> + Send + '_ {
        let resources = super::resources::list_resources();
        std::future::ready(Ok(ListResourcesResult {
            next_cursor: None,
            resources,
            meta: None,
        }))
    }

    /// 读取资源内容（Layer 4）。
    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, McpError>> + Send + '_ {
        let asset_path = self.ctx.asset_store_path.clone();
        std::future::ready(match super::resources::read_resource(&request, &asset_path) {
            Ok(result) => Ok(result),
            Err(e) => {
                log::warn!("MCP read_resource error: {}", e);
                Err(McpError::invalid_request(e, None))
            }
        })
    }

    /// 列出 resource template（Layer 4）。
    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, McpError>> + Send + '_
    {
        let templates = super::resources::list_resource_templates();
        std::future::ready(Ok(ListResourceTemplatesResult {
            next_cursor: None,
            resource_templates: templates,
            meta: None,
        }))
    }

    /// 列出 3 个诊断 prompt（Layer 5）。
    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, McpError>> + Send + '_ {
        let prompts = super::prompts::list_prompts();
        std::future::ready(Ok(ListPromptsResult {
            next_cursor: None,
            prompts,
            meta: None,
        }))
    }

    /// 生成 prompt messages（Layer 5）。
    fn get_prompt(
        &self,
        request: GetPromptRequestParam,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<GetPromptResult, McpError>> + Send + '_ {
        std::future::ready(match super::prompts::get_prompt(&request) {
            Ok(result) => Ok(result),
            Err(e) => {
                log::warn!("MCP get_prompt error: {}", e);
                Err(McpError::invalid_request(e, None))
            }
        })
    }
}

/// MCP stdio server 主入口（被 `lib::run_mcp_stdio` 调用）。
///
/// Layer 2/3：加载资产/凭据路径 → 构造 McpToolContext → rmcp stdio serve。
pub async fn serve_stdio(
    asset_store_path: std::path::PathBuf,
    secret_store_dir: std::path::PathBuf,
    known_hosts_path: std::path::PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("myshelltool-mcp stdio server starting");
    log::info!(
        "assets: {} | secrets: {} | known_hosts: {}",
        asset_store_path.display(),
        secret_store_dir.display(),
        known_hosts_path.display()
    );

    let ctx = McpToolContext::new(asset_store_path, secret_store_dir, known_hosts_path);
    let handler = MyshellToolMcpServer::new(ctx);
    let transport = rmcp::transport::stdio();
    let service = handler.serve(transport).await?;

    log::info!("myshelltool-mcp stdio server serving");
    service.waiting().await?;

    log::info!("myshelltool-mcp stdio server stopped");
    Ok(())
}
