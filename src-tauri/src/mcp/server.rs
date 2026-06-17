//! MCP stdio server 主循环 + ServerHandler 实现（Layer 2 + Layer 3）。
//!
//! M2：rmcp stdio server 骨架（get_info）
//! M3/M4：接入 list_tools（9 个工具）+ call_tool（headless exec + 审批）
//! M5：接入 list_resources/read_resource/list_prompts/get_prompt
//!
//! 见 docs/plans/MCP服务接入-实施计划.md §4-§7。

use std::sync::Arc;

use rmcp::{
    ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult,
        Implementation, InitializeResult, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams,
        ReadResourceResult, ServerCapabilities, ServerInfo,
    },
    service::{Peer, RequestContext},
    ErrorData as McpError,
};

use super::approval::{self, ApprovalDecision, ElicitationInfo};
use super::tools::{self, McpToolContext};

// ── v1.1 审批辅助 ──

/// elicitation 结果。
enum ElicitOutcome {
    /// 用户确认执行。
    Accepted,
    /// 用户拒绝（decline 或 cancel）。
    Declined(String),
    /// 客户端不支持 elicitation，降级为拒绝。
    NotSupported(String),
}

/// 对高危工具（ssh_exec）做审批判定。
///
/// 返回 None 表示该工具不需要审批（只读工具），Some 表示需要审批决策。
fn check_approval_needed(
    tool_name: &str,
    arguments: &Option<serde_json::Map<String, serde_json::Value>>,
) -> Option<ApprovalDecision> {
    let args = arguments.as_ref()?;
    match tool_name {
        "ssh_exec" => {
            let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let intent = args.get("intent").and_then(|v| v.as_str()).unwrap_or("");
            if command.is_empty() {
                return None;
            }
            Some(approval::evaluate(
                command,
                intent,
                super::approval::READONLY_WHITELIST,
                &[],
            ))
        }
        // sftp_remove 始终需要确认（删除不可逆）
        "sftp_remove" => {
            let intent = args.get("intent").and_then(|v| v.as_str()).unwrap_or("");
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            Some(ApprovalDecision::RequestElicitation(ElicitationInfo {
                intent: intent.to_string(),
                command: format!("sftp_remove path={}", path),
                consequence: "将删除远程文件/目录，可能不可恢复。".to_string(),
            }))
        }
        _ => None,
    }
}

/// 尝试经 MCP elicitation 向用户确认。
///
/// 客户端支持时弹出确认框（三段式），不支持时返回 NotSupported 降级。
async fn try_elicit(peer: &Peer<rmcp::RoleServer>, info: &ElicitationInfo) -> ElicitOutcome {
    // 先检查客户端是否声明了 elicitation 能力
    let modes = peer.supported_elicitation_modes();
    if modes.is_empty() {
        return ElicitOutcome::NotSupported(info.to_rejection());
    }

    // 发起 elicitation（message = 三段式确认文本）
    // elicit::<T>() 返回 Result<Option<T>, ElicitationError>：
    //   Ok(Some(form)) = 用户确认并填表单
    //   Ok(None) = 用户没提供内容
    //   Err(UserDeclined) = 用户拒绝
    //   Err(UserCancelled) = 用户取消
    //   Err(CapabilityNotSupported) = 客户端不支持 → 降级
    match peer.elicit::<ApprovalForm>(info.to_message()).await {
        Ok(Some(form)) => {
            if form.confirmed {
                ElicitOutcome::Accepted
            } else {
                ElicitOutcome::Declined("用户在确认框中选择了不执行".to_string())
            }
        }
        Ok(None) => ElicitOutcome::Declined("用户未提供确认".to_string()),
        Err(rmcp::service::ElicitationError::UserDeclined) => {
            ElicitOutcome::Declined("用户拒绝了此操作".to_string())
        }
        Err(rmcp::service::ElicitationError::UserCancelled) => {
            ElicitOutcome::Declined("用户取消了确认".to_string())
        }
        Err(rmcp::service::ElicitationError::CapabilityNotSupported) => {
            ElicitOutcome::NotSupported(info.to_rejection())
        }
        Err(e) => {
            log::warn!("elicitation error: {:?}, degrading to reject", e);
            ElicitOutcome::NotSupported(info.to_rejection())
        }
    }
}

/// elicitation 表单 schema（客户端据此渲染确认框）。
#[derive(schemars::JsonSchema, serde::Deserialize)]
struct ApprovalForm {
    /// 确认执行此高危操作。
    confirmed: bool,
}
// 标记为 elicitation 安全类型（rmcp 要求）
rmcp::elicit_safe!(ApprovalForm);

/// 辅助：构造 error CallToolResult。
fn error_result(message: &str) -> CallToolResult {
    let mut result = CallToolResult::success(vec![rmcp::model::Content::text(message.to_string())]);
    result.is_error = Some(true);
    result
}

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

    /// 返回 9 个工具（7 只读 + 2 高危）。
    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
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
    ///
    /// v1.1：高危工具（ssh_exec/sftp_remove）在分发前做审批拦截——
    /// 命中黑名单/未知 → 经 MCP elicitation 在客户端界面内弹确认框（三段式），
    /// 用户 accept 才执行。客户端不支持 elicitation 时降级为 v1.0 的进程内拒绝。
    fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        let ctx = self.ctx.clone();
        let tool_name = request.name.clone();
        let arguments = request.arguments.clone();
        async move {
            // ── v1.1 审批拦截：高危工具先做危险判定 ──
            if let Some(approval_needed) = check_approval_needed(&tool_name, &arguments) {
                match approval_needed {
                    super::approval::ApprovalDecision::AutoExecute => {
                        // 白名单命令，直接放行
                    }
                    super::approval::ApprovalDecision::Reject(reason) => {
                        // 降级路径（不应走到这里，evaluate 对高危返回 RequestElicitation）
                        return Ok(error_result(&reason));
                    }
                    super::approval::ApprovalDecision::RequestElicitation(info) => {
                        // v1.1 核心：经 elicitation 在客户端界面内确认
                        match try_elicit(&context.peer, &info).await {
                            ElicitOutcome::Accepted => {
                                log::info!("elicitation: user accepted, proceeding");
                                // 放行执行
                            }
                            ElicitOutcome::Declined(reason) => {
                                log::info!("elicitation: user declined");
                                return Ok(error_result(&reason));
                            }
                            ElicitOutcome::NotSupported(reason) => {
                                // 客户端不支持 elicitation → 降级 v1.0 进程内拒绝
                                log::warn!("elicitation not supported, degrading to reject: {}", reason);
                                return Ok(error_result(&reason));
                            }
                        }
                    }
                }
            }

            // ── 正常分发 ──
            match tools::call_tool(tool_name.as_ref(), request, &ctx).await {
                Ok(result) => Ok(result),
                Err(e) => {
                    log::warn!("MCP call_tool error: {}", e);
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
        _request: Option<PaginatedRequestParams>,
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
        _request: Option<PaginatedRequestParams>,
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
        _request: Option<PaginatedRequestParams>,
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
        request: GetPromptRequestParams,
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
