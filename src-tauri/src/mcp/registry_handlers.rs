//! MCP 工具 handler 包装（v0.20 自 registry.rs 拆出，S2 后续刀）：把各实现
//! 函数适配为 ToolSpec 的统一签名。

use serde_json::{Map, Value};

use super::registry::ToolFuture;
pub(crate) use super::job_tools::{h_ssh_exec_async, h_job_status, h_job_output, h_job_cancel};
// journal_query 是 async fn——包一层 ToolFuture 形态（与其他 handler 统一签名）
pub(crate) fn h_journal_query<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::readonly_tools::tool_journal_query(ctx, args))
}
use super::tools::McpToolContext;

// ─── handler 包装（把各实现函数适配为统一签名）───

pub(crate) fn h_list_assets<'a>(ctx: &'a McpToolContext, _args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::tools::tool_list_assets(ctx))
}

pub(crate) fn h_list_sessions<'a>(ctx: &'a McpToolContext, _args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::tools::tool_list_sessions(ctx))
}

pub(crate) fn h_disk_usage<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::tools::exec_on_asset(ctx, args, super::tools::CMD_DISK_USAGE))
}

pub(crate) fn h_system_status<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::tools::exec_on_asset(
        ctx,
        args,
        super::tools::CMD_SYSTEM_STATUS,
    ))
}

pub(crate) fn h_service_status<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::tools::tool_service_status(ctx, args))
}

pub(crate) fn h_resource_monitor_snapshot<'a>(
    ctx: &'a McpToolContext,
    args: &'a Map<String, Value>,
) -> ToolFuture<'a> {
    Box::pin(super::tools::exec_on_asset(
        ctx,
        args,
        super::tools::CMD_RESOURCE_MONITOR_SNAPSHOT,
    ))
}

pub(crate) fn h_ssh_exec<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::tools::tool_ssh_exec(ctx, args))
}

pub(crate) fn h_sftp_list<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::file_tools::tool_sftp_list(ctx, args))
}

pub(crate) fn h_sftp_read_file<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::file_tools::tool_sftp_read_file(ctx, args))
}

pub(crate) fn h_read_output<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::tools::tool_read_output(ctx, args))
}

pub(crate) fn h_sftp_write_file<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::file_tools::tool_sftp_write_file(ctx, args))
}

pub(crate) fn h_sftp_upload<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::file_tools::tool_sftp_upload(ctx, args))
}

pub(crate) fn h_sftp_download<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::file_tools::tool_sftp_download(ctx, args))
}

pub(crate) fn h_sftp_remove<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::file_tools::tool_sftp_remove(ctx, args))
}

pub(crate) fn h_exec_many<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(async move {
        let asset_ids: Vec<String> = args
            .get("asset_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or("缺少 command 参数")?;
        let intent = args.get("intent").and_then(|v| v.as_str()).unwrap_or("");
        log::info!("exec_many: {} targets, intent={:?}, command={:?}", asset_ids.len(), intent, myshelltool_core::redact_command(command));
        let outcome = super::orchestrator::exec_many_on_assets(ctx, &asset_ids, command).await?;
        let summary = super::orchestrator::summarize(&outcome);
        Ok(rmcp::model::CallToolResult::success(vec![rmcp::model::Content::text(
            serde_json::to_string_pretty(&summary).unwrap_or_default(),
        )]))
    })
}

pub(crate) fn h_port_listen<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::readonly_tools::tool_port_listen(ctx, args))
}

pub(crate) fn h_process_list<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::readonly_tools::tool_process_list(ctx, args))
}

pub(crate) fn h_file_search<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::readonly_tools::tool_file_search(ctx, args))
}

pub(crate) use super::service_tools::{h_tunnel_list, h_tunnel_create};
pub(crate) fn h_service_control<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(super::service_tools::tool_service_control(ctx, args))
}
