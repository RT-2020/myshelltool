//! MCP Resources 实现（Layer 4）。
//!
//! v0.20（B4）资源面修正（MCP服务设计-v3 §3.6 P6/P7）：
//! - `myshelltool://sessions`：真实 GUI 会话清单（旧版回答「v1.0 独立模式」
//!   是陈旧的世界模型——GUI 一直有会话池，AI 却被告知没有）；
//! - `myshelltool://sessions/{id}/log` 死模板已删（恒答「不可用」= 死面，
//!   违反死代码红线；查日志用 ssh_exec/journalctl）；
//! - `myshelltool://assets` 与 `list_assets` 工具共用 `assets_view` 单实现
//!   （消除同一概念两份序列化的漂移面），并应用 scope 过滤（B1 可见性同步）；
//! - 新增 `myshelltool://mcp/scope`：当前授权范围（AI 应知道自己的边界）。

use std::path::Path;

use rmcp::model::{
    AnnotateAble, RawResource, ReadResourceRequestParams, ReadResourceResult, ResourceContents,
};

/// 返回静态资源清单。
pub fn list_resources() -> Vec<rmcp::model::Resource> {
    vec![
        RawResource::new("myshelltool://assets", "assets").no_annotation(),
        RawResource::new("myshelltool://sessions", "sessions").no_annotation(),
        RawResource::new("myshelltool://known-hosts", "known-hosts").no_annotation(),
        RawResource::new("myshelltool://mcp/scope", "mcp-scope").no_annotation(),
    ]
}

/// 返回 resource template 清单。
/// v0.20（B4）：`sessions/{id}/log` 死模板已删——它恒答「不可用」（死面，
/// 违反死代码红线）；需要会话日志时用 ssh_exec / journalctl。
pub fn list_resource_templates() -> Vec<rmcp::model::ResourceTemplate> {
    vec![]
}

/// 资产视图的单一实现（list_assets 工具与 ://assets 资源共用，P7）。
/// scope 生效时过滤范围外资产并附 scopeApplied/hiddenCount（B1 可见性同步：
/// AI 不应从资源面看到比工具面更多的机器）。
pub fn assets_view(
    store: &myshelltool_core::ConnectionAssetStore,
    scope: &myshelltool_core::mcp_scope::McpScope,
) -> serde_json::Value {
    let scope_applied = scope.is_restricted();
    let mut assets: Vec<serde_json::Value> = Vec::new();
    let mut hidden = 0usize;
    for a in &store.assets {
        if scope_applied && !myshelltool_core::mcp_scope::evaluate(scope, a).is_allowed() {
            hidden += 1;
            continue;
        }
        assets.push(serde_json::json!({
            "id": a.id,
            "name": a.name,
            "host": a.host,
            "port": a.port,
            "username": a.username,
            "group": a.group,
            "status": format!("{:?}", a.status),
            "tags": a.tags,
        }));
    }
    let mut view = serde_json::json!({
        "source": "local",
        "count": assets.len(),
        "assets": assets,
        "groups": store.groups,
    });
    if scope_applied {
        view["scopeApplied"] = serde_json::json!(true);
        view["hiddenCount"] = serde_json::json!(hidden);
    }
    view
}

/// 读取资源。匹配 URI 返回内容，不匹配返回错误。
///
/// `known_hosts_path` / `sessions` / `scope` 由调用方从 McpToolContext 透传
/// （lib.rs setup 统一解析），不在本模块重新推导，避免两处路径来源漂移。
pub fn read_resource(
    request: &ReadResourceRequestParams,
    asset_store_path: &Path,
    known_hosts_path: &Path,
    sessions: &[(String, crate::ssh::SessionMeta)],
    scope: &myshelltool_core::mcp_scope::McpScope,
) -> Result<ReadResourceResult, String> {
    let uri = request.uri.as_ref();
    match uri {
        "myshelltool://assets" => {
            let store = myshelltool_core::load_connection_asset_store(asset_store_path)
                .map_err(|e| format!("加载资产库失败: {e}"))?;
            let body = serde_json::to_string_pretty(&assets_view(&store, scope))
                .unwrap_or_default();
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                body,
                "myshelltool://assets",
            )]))
        }
        // v0.20（B4）：真实 GUI 会话清单（取代「v1.0 独立模式」陈旧文案）。
        "myshelltool://sessions" => {
            let list: Vec<serde_json::Value> = sessions
                .iter()
                .map(|(sid, meta)| {
                    serde_json::json!({
                        "session_id": sid,
                        "host": meta.host,
                        "port": meta.port,
                        "username": meta.username,
                    })
                })
                .collect();
            let body = serde_json::to_string_pretty(&serde_json::json!({
                "count": list.len(),
                "sessions": list,
                "note": "GUI 正在保持的 SSH 会话。MCP exec 工具（ssh_exec 等）会优先复用这些会话（sessionSource=gui）。",
            }))
            .unwrap_or_default();
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                body,
                "myshelltool://sessions",
            )]))
        }
        "myshelltool://known-hosts" => read_known_hosts(known_hosts_path),
        // v0.20（B4 新增）：AI 应知道自己的授权边界（可主动告知用户「我看不到 X 分组」）。
        "myshelltool://mcp/scope" => {
            let body = serde_json::to_string_pretty(scope).unwrap_or_default();
            Ok(ReadResourceResult::new(vec![ResourceContents::text(
                body,
                "myshelltool://mcp/scope",
            )]))
        }
        _ => Err(format!("未知资源 URI: {}", uri)),
    }
}

fn read_known_hosts(known_hosts_path: &Path) -> Result<ReadResourceResult, String> {
    if !known_hosts_path.exists() {
        return Ok(ReadResourceResult::new(vec![ResourceContents::text(
            "{\"hosts\": []}".to_string(),
            "myshelltool://known-hosts",
        )]));
    }
    let raw = std::fs::read_to_string(known_hosts_path)
        .map_err(|e| format!("读取 known_hosts 失败: {e}"))?;
    Ok(ReadResourceResult::new(vec![ResourceContents::text(
        raw,
        "myshelltool://known-hosts",
    )]))
}
