//! MCP Prompts 实现（Layer 5）。
//!
//! 见 docs/plans/MCP服务接入-实施计划.md §7（Layer 5）。
//!
//! Prompts 是 server 端 workflow：客户端发 prompts/get 带 argument 值，
//! server 动态返回 messages[]（role + content）。
//!
//! 3 个诊断 prompt：
//! - diagnose_server：依次查 disk/CPU/内存/服务，汇总异常
//! - audit_security：查最近登录/异常进程/开放端口
//! - cleanup_disk：找大文件/旧日志

use rmcp::model::{
    GetPromptRequestParams, GetPromptResult, Prompt, PromptArgument, PromptMessage,
    PromptMessageContent, PromptMessageRole,
};

/// 返回 3 个 prompt schema（含 arguments 定义）。
pub fn list_prompts() -> Vec<Prompt> {
    vec![
        Prompt::new(
            "diagnose_server",
            Some("服务器健康诊断：依次检查磁盘/CPU/内存/关键服务状态，汇总结论指出异常项。"),
            Some(vec![PromptArgument::new("asset_id")]),
        ),
        Prompt::new(
            "audit_security",
            Some("安全审计：检查最近登录记录、异常进程、开放端口。"),
            Some(vec![PromptArgument::new("asset_id")]),
        ),
        Prompt::new(
            "cleanup_disk",
            Some("磁盘清理引导：查找大文件和旧日志，建议清理方案。"),
            Some(vec![PromptArgument::new("asset_id")]),
        ),
    ]
}

/// 根据 prompt name + arguments 生成 messages。
pub fn get_prompt(
    request: &GetPromptRequestParams,
) -> Result<GetPromptResult, String> {
    let asset_id = request
        .arguments
        .as_ref()
        .and_then(|args| args.get("asset_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if asset_id.is_empty() {
        return Err("缺少 asset_id 参数".to_string());
    }

    let (description, message_text) = match request.name.as_ref() {
        "diagnose_server" => (
            "服务器健康诊断",
            format!(
                "请对资产 {asset_id} 进行健康诊断，依次执行以下步骤并汇总：\n\n\
                 1. 调用 disk_usage 工具查看磁盘使用情况\n\
                 2. 调用 system_status 工具查看 CPU/内存/负载/top 进程\n\
                 3. 调用 service_status 工具检查 nginx 和 mysql 状态（如适用）\n\
                 4. 汇总结论，明确指出哪些指标异常（如磁盘 >80%、负载过高、服务未运行），并给出建议\n\n\
                 注意：只使用只读工具查询，不要执行任何修改操作。"
            ),
        ),
        "audit_security" => (
            "安全审计",
            format!(
                "请对资产 {asset_id} 进行安全审计，依次执行以下检查：\n\n\
                 1. 用 ssh_exec 执行 `last -20` 查看最近 20 次登录记录，留意异常 IP/时间\n\
                 2. 用 ssh_exec 执行 `ss -tlnp` 查看监听端口，留意非预期端口\n\
                 3. 用 ssh_exec 执行 `ps aux --sort=-%cpu | head -10` 查看 CPU 占用最高的进程\n\
                 4. 汇总结论，标注可疑项（陌生登录 IP、异常监听端口、可疑进程）\n\n\
                 注意：所有命令均为只读查询，ssh_exec 中只读命令会自动放行。"
            ),
        ),
        "cleanup_disk" => (
            "磁盘清理引导",
            format!(
                "请帮资产 {asset_id} 分析磁盘占用并提出清理建议，依次执行：\n\n\
                 1. 调用 disk_usage 查看整体磁盘占用\n\
                 2. 用 ssh_exec 执行 `du -sh /var/log/* 2>/dev/null | sort -rh | head -10` 查看日志目录占用\n\
                 3. 用 ssh_exec 执行 `find /tmp -type f -mtime +7 -size +10M 2>/dev/null | head -20` 查找旧大文件\n\
                 4. 汇总占用情况，给出清理建议（如可安全清理的日志/临时文件），但**不要自动执行删除**——删除需用户确认\n\n\
                 注意：删除操作（rm）属于高危命令，会被审批拦截。仅提供分析建议。"
            ),
        ),
        other => return Err(format!("未知 prompt: {}", other)),
    };

    Ok(
        GetPromptResult::new(vec![PromptMessage::new(
            PromptMessageRole::User,
            PromptMessageContent::text(message_text),
        )])
        .with_description(description),
    )
}
