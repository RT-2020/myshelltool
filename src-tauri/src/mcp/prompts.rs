//! MCP Prompts 实现（Layer 5）。
//!
//! 见 docs/plans/MCP服务接入-实施计划.md §7（Layer 5）。
//!
//! Prompts 是 server 端 workflow：客户端发 prompts/get 带 argument 值，
//! server 动态返回 messages[]（role + content）。
//!
//! v0.20（C5）多资产化：`asset_id` → `asset_ids`（逗号分隔或 JSON 数组）——
//! 单资产旧用法保持兼容（单元素视同旧形态）。diagnose_server 输出多机对比表。
//!
//! 3 个诊断 prompt：
//! - diagnose_server：依次查 disk/CPU/内存/服务，**多机对比表**汇总异常
//! - audit_security：查最近登录/异常进程/开放端口
//! - cleanup_disk：找大文件/旧日志
//!
//! **「查不到 ≠ 无异常」纪律**（audit_security 的 77-78 行原文纪律，C5 全部
//! prompts 保留）：命令失败/工具缺失时必须如实报告「该步未取得数据」，
//! 不得据此判定无异常——假阴性结论比查不到更危险。

use rmcp::model::{
    GetPromptRequestParams, GetPromptResult, Prompt, PromptArgument, PromptMessage,
    PromptMessageContent, PromptMessageRole,
};

/// 返回 3 个 prompt schema（含 arguments 定义）。
/// v0.20（C5）：asset_id → asset_ids（多资产，逗号分隔或 JSON 数组形态）。
pub fn list_prompts() -> Vec<Prompt> {
    vec![
        Prompt::new(
            "diagnose_server",
            Some("服务器健康诊断（支持多机对比）：依次检查磁盘/CPU/内存/关键服务状态，多机时输出对比表汇总异常项。"),
            Some(vec![PromptArgument::new("asset_ids")]),
        ),
        Prompt::new(
            "audit_security",
            Some("安全审计（支持多机）：检查最近登录记录、异常进程、开放端口。"),
            Some(vec![PromptArgument::new("asset_ids")]),
        ),
        Prompt::new(
            "cleanup_disk",
            Some("磁盘清理引导（支持多机）：查找大文件和旧日志，建议清理方案。"),
            Some(vec![PromptArgument::new("asset_ids")]),
        ),
    ]
}

/// 解析 asset_ids 参数：兼容三种形态——JSON 数组（["a","b"]）、逗号分隔（a,b）、
/// 单值（a，旧 asset_id 用法的等价形态）。空/全空元素报错。
fn parse_asset_ids(request: &GetPromptRequestParams) -> Result<Vec<String>, String> {
    let raw = request
        .arguments
        .as_ref()
        .and_then(|args| args.get("asset_ids"))
        .map(|v| v.to_string())
        .unwrap_or_default();
    // 兼容旧参数名（asset_id 单值形态）
    let raw = if raw.is_empty() {
        request
            .arguments
            .as_ref()
            .and_then(|args| args.get("asset_id"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_default()
    } else {
        raw
    };
    if raw.is_empty() {
        return Err("缺少 asset_ids 参数（多资产：逗号分隔或 JSON 数组；单资产直接传 id）".to_string());
    }

    let ids: Vec<String> = if let Ok(arr) = serde_json::from_str::<Vec<String>>(&raw) {
        arr
    } else {
        raw.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    };
    if ids.is_empty() {
        return Err("asset_ids 解析后为空（全空元素）".to_string());
    }
    Ok(ids)
}

/// 多资产 prompt 的通用头部：说明逐机执行 + 对比口径。
fn multi_asset_header(ids: &[String]) -> String {
    if ids.len() == 1 {
        format!("请对资产 {} ", ids[0])
    } else {
        format!(
            "请对 {} 台资产（{}）**逐台**执行以下检查，最后输出**多机对比表**（行=检查项，列=资产，单元格=值/异常标记），\
             突出偏离基线的资产：\n\n",
            ids.len(),
            ids.join("、")
        )
    }
}

/// 通用尾部纪律：逐机模式下的「查不到 ≠ 无异常」（逐字保留原纪律语义并扩展到多机：
/// 某台某步失败 ≠ 该台无异常，也 ≠ 其他台同步骤可跳过）。
fn cannot_query_note() -> &'static str {
    "**任何一台的任何一步命令失败或工具不可用时，必须在该台的结论中如实标注「该步未取得数据」，\
     不得据此判定该台无异常，也不得因一台失败跳过其他台的同一检查**——\
     假阴性的结论比查不到更危险。"
}

/// 根据 prompt name + arguments 生成 messages。
pub fn get_prompt(request: &GetPromptRequestParams) -> Result<GetPromptResult, String> {
    let ids = parse_asset_ids(request)?;
    let header = multi_asset_header(&ids);

    let (description, message_text) = match request.name.as_ref() {
        "diagnose_server" => (
            "服务器健康诊断",
            format!(
                "{header}依次执行以下步骤并汇总：\n\n\
                 1. 对每台资产调用 disk_usage 工具查看磁盘使用情况\n\
                 2. 对每台资产调用 system_status 工具查看 CPU/内存/负载/top 进程\n\
                 3. 对每台资产调用 service_status 工具检查关键服务状态（按各机用途选：nginx/mysql/docker 等）\n\
                 4. 汇总结论：单机时明确指出异常项（磁盘 >80%、负载过高、服务未运行）；\
                 多机时输出对比表并突出偏离基线的资产，给出针对性建议\n\n\
                 注意：只使用只读工具查询，不要执行任何修改操作。\n{}",
                cannot_query_note()
            ),
        ),
        "audit_security" => (
            "安全审计",
            format!(
                "{header}依次执行以下检查：\n\n\
                 1. 用 ssh_exec 执行 `last -20` 查看最近 20 次登录记录，留意异常 IP/时间\n\
                 2. 查看监听端口：优先用 port_listen 工具（ss → netstat → lsof 降级链已固化）；\
                 或用 ssh_exec 手动：Linux `ss -tlnp`；BSD/macOS `netstat -an -p tcp | head -30`。留意非预期端口\n\
                 3. 用 process_list 工具查看 CPU 占用最高的进程（POSIX 排序已固化）；\
                 或 ssh_exec 手动 `ps aux | sort -k3 -rn | head -10`\n\
                 4. 汇总结论，标注可疑项（陌生登录 IP、异常监听端口、可疑进程）；\
                 多机时输出对比表，突出与其他机器模式不一致的资产\n\n\
                 注意：所有命令均为只读查询，ssh_exec 中只读命令会自动放行。\n{}",
                cannot_query_note()
            ),
        ),
        "cleanup_disk" => (
            "磁盘清理引导",
            format!(
                "{header}依次执行：\n\n\
                 1. 对每台资产调用 disk_usage 查看整体磁盘占用\n\
                 2. 用 file_search 工具查找 /var/log 下的大文件与旧日志（POSIX 形态已固化）；\
                 或 ssh_exec 手动：`du -sk /var/log/* 2>/dev/null | sort -rn | head -10`\n\
                 3. 用 file_search 查找 /tmp 下 7 天前的 10M+ 旧文件\n\
                 4. 汇总占用情况，给出清理建议（可安全清理的日志/临时文件），但**不要自动执行删除**——删除需用户确认；\
                 多机时按磁盘压力排序输出优先级\n\n\
                 注意：删除操作（rm）属于高危命令，会被审批拦截。仅提供分析建议。\n{}",
                cannot_query_note()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn req(name: &str, args: serde_json::Value) -> GetPromptRequestParams {
        let mut p = GetPromptRequestParams::new(name.to_string());
        p.arguments = Some(serde_json::from_value(args).unwrap());
        p
    }

    #[test]
    fn asset_ids_three_forms() {
        // JSON 数组
        let ids = parse_asset_ids(&req("diagnose_server", serde_json::json!({"asset_ids": "[\"a\",\"b\"]"}))).unwrap();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
        // 逗号分隔
        let ids = parse_asset_ids(&req("d", serde_json::json!({"asset_ids": "a, b,,c"}))).unwrap();
        assert_eq!(ids, vec!["a", "b", "c"]);
        // 旧单值（asset_id 参数名兼容）
        let ids = parse_asset_ids(&req("d", serde_json::json!({"asset_id": "single"}))).unwrap();
        assert_eq!(ids, vec!["single".to_string()]);
        // 单值 asset_ids 也是合法形态
        let ids = parse_asset_ids(&req("d", serde_json::json!({"asset_ids": "one"}))).unwrap();
        assert_eq!(ids, vec!["one".to_string()]);
    }

    #[test]
    fn missing_or_empty_rejected() {
        assert!(parse_asset_ids(&req("d", serde_json::json!({}))).is_err());
        assert!(parse_asset_ids(&req("d", serde_json::json!({"asset_ids": " , ,"}))).is_err());
    }

    fn prompt_text(result: &GetPromptResult) -> String {
        result
            .messages
            .first()
            .and_then(|m| match &m.content {
                rmcp::model::PromptMessageContent::Text { text } => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    #[test]
    fn discipline_preserved_in_all_prompts() {
        for name in ["diagnose_server", "audit_security", "cleanup_disk"] {
            let result = get_prompt(&req(name, serde_json::json!({"asset_ids": "a,b"}))).unwrap();
            let text = prompt_text(&result);
            assert!(
                text.contains("未取得数据"),
                "{name} 缺「查不到≠无异常」纪律"
            );
            assert!(text.contains("对比表") || name == "cleanup_disk", "{name} 缺多机对比口径");
        }
    }

    #[test]
    fn single_asset_has_no_multi_table_noise() {
        let result = get_prompt(&req("diagnose_server", serde_json::json!({"asset_ids": "solo"}))).unwrap();
        let text = prompt_text(&result);
        assert!(!text.contains("对比表"), "单资产不应输出多机对比噪音");
    }
}
