//! MCP Resources 实现（Layer 4）。
//!
//! 见 docs/plans/MCP服务接入-实施计划.md §6（Layer 4）。
//!
//! 3 静态资源 + 1 resource template：
//! - myshelltool://assets：资产清单（脱敏，无 credential_id）
//! - myshelltool://sessions：活跃会话（v1.0 独立模式返回空）
//! - myshelltool://known-hosts：已信任主机指纹
//! - myshelltool://sessions/{id}/log：会话日志（template，v1.0 返回「不可用」）

use std::path::Path;

use rmcp::model::{
    AnnotateAble, RawResource, RawResourceTemplate, ReadResourceRequestParams,
    ReadResourceResult, ResourceContents,
};

/// 返回 3 个静态资源。
pub fn list_resources() -> Vec<rmcp::model::Resource> {
    vec![
        RawResource::new("myshelltool://assets", "assets").no_annotation(),
        RawResource::new("myshelltool://sessions", "sessions").no_annotation(),
        RawResource::new("myshelltool://known-hosts", "known-hosts").no_annotation(),
    ]
}

/// 返回 1 个 resource template。
pub fn list_resource_templates() -> Vec<rmcp::model::ResourceTemplate> {
    vec![RawResourceTemplate::new(
        "myshelltool://sessions/{id}/log",
        "session-log",
    )
    .no_annotation()]
}

/// 读取资源。匹配 URI 返回内容，不匹配返回错误。
pub fn read_resource(
    request: &ReadResourceRequestParams,
    asset_store_path: &Path,
) -> Result<ReadResourceResult, String> {
    let uri = request.uri.as_ref();
    match uri {
        "myshelltool://assets" => read_assets(asset_store_path),
        "myshelltool://sessions" => Ok(ReadResourceResult::new(vec![ResourceContents::text(
            "v1.0 独立会话模式：无持久会话池。使用 list_assets 工具查看可用资产后直接用 disk_usage/system_status 查询。",
            uri,
        )])),
        "myshelltool://known-hosts" => read_known_hosts(asset_store_path),
        other if other.starts_with("myshelltool://sessions/") && other.ends_with("/log") => {
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                "v1.0 模式下会话日志暂不可用。如需查看远程日志，请使用 ssh_exec 工具执行 tail/journalctl 命令。",
                uri,
            )]))
        }
        _ => Err(format!("未知资源 URI: {}", uri)),
    }
}

fn read_assets(asset_store_path: &Path) -> Result<ReadResourceResult, String> {
    let store = myshelltool_core::load_connection_asset_store(asset_store_path)
        .map_err(|e| format!("加载资产库失败: {e}"))?;
    let assets: Vec<serde_json::Value> = store
        .assets
        .iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id,
                "name": a.name,
                "host": a.host,
                "port": a.port,
                "username": a.username,
                "group": a.group,
                "status": format!("{:?}", a.status),
            })
        })
        .collect();
    let body = serde_json::to_string_pretty(&serde_json::json!({
        "source": "local",
        "count": assets.len(),
        "assets": assets,
    }))
    .unwrap_or_default();
    Ok(ReadResourceResult::new(vec![ResourceContents::text(
        body,
        "myshelltool://assets",
    )]))
}

fn read_known_hosts(asset_store_path: &Path) -> Result<ReadResourceResult, String> {
    let known_hosts_path = asset_store_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("known_hosts.json");
    if !known_hosts_path.exists() {
        return Ok(ReadResourceResult::new(vec![ResourceContents::text(
            "{\"hosts\": []}".to_string(),
            "myshelltool://known-hosts",
        )]));
    }
    let raw = std::fs::read_to_string(&known_hosts_path)
        .map_err(|e| format!("读取 known_hosts 失败: {e}"))?;
    Ok(ReadResourceResult::new(vec![ResourceContents::text(
        raw,
        "myshelltool://known-hosts",
    )]))
}
