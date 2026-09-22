//! MCP C4：service_control + 隧道管理工具（v0.20，3-6 月段）。
//!
//! service_control：systemctl start/stop/restart——**语义化风险**（比 ssh_exec
//! 更明确地被拦：action 是枚举白名单，不混入任意命令文本）。跨发行版口径：
//! 仅 systemd（服务级；sysvinit 的 service 命令语义差异大不固化，报错降级）。
//!
//! 隧道工具：tunnel_list / tunnel_create 复用 GUI 的 ssh/tunnel.rs 路径
//! （同一 SshSessionManager / tunnel_start，**不另写**——G3 决策）。
//! tunnel_create 的 Policy=RemoteWrite（建立端口转发 = 改变网络暴露面）。

use rmcp::model::{CallToolResult, Content};
use serde_json::{json, Map, Value};

use super::registry::ToolFuture;
use super::tools::McpToolContext;

fn text_result(text: String) -> CallToolResult {
    CallToolResult::success(vec![Content::text(text)])
}

fn error_result(message: &str) -> CallToolResult {
    let mut result = CallToolResult::success(vec![Content::text(message.to_string())]);
    result.is_error = Some(true);
    result
}

/// service_control：systemctl 生命周期操作。
///
/// 注入防护：action 枚举白名单（start/stop/restart/reload）；service 名
/// 白名单（字母数字-_@:.，同 service_status）。执行后自动补一次
/// `systemctl is-active` 确认生效（restart 失败时 status 会如实显示）。
pub(crate) async fn tool_service_control(
    ctx: &McpToolContext,
    args: &Map<String, Value>,
) -> Result<CallToolResult, String> {
    args.get("asset_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("缺少 asset_id 参数")?;
    let service = args
        .get("service")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("缺少 service 参数")?;
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("缺少 action 参数")?;

    if !service.chars().all(|c| c.is_alphanumeric() || "-_@:.".contains(c)) {
        return Ok(error_result("service 参数含非法字符"));
    }
    if !matches!(action, "start" | "stop" | "restart" | "reload") {
        return Ok(error_result(&format!(
            "action 仅支持 start / stop / restart / reload（收到 {action:?}）"
        )));
    }

    // 操作 + 即时确认（is-active 的输出是最终态；操作失败时 is-active 会显示
    // failed/inactive 而非报错——诚实语义）
    let cmd = format!(
        "env LC_ALL=C systemctl {action} {service} 2>&1; echo rc_ctl=$?; echo '---'; env LC_ALL=C systemctl is-active {service}; echo rc_active=$?"
    );
    super::tools::exec_on_asset(ctx, args, &cmd).await
}

/// tunnel_list：列出当前隧道（GUI 的 SshSessionManager 同一数据源）。
pub(crate) fn h_tunnel_list<'a>(ctx: &'a McpToolContext, _args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(async move {
        let Some(app) = &ctx.app_handle else {
            return Ok(text_result("隧道管理需要 GUI 运行（headless 模式无会话池）".to_string()));
        };
        use tauri::Manager;
        let Some(state) = app.try_state::<crate::AppState>() else {
            return Ok(error_result("AppState 不可用"));
        };
        let list = crate::ssh::tunnel_list(state.clone()).await?;
        let text = serde_json::to_string_pretty(&json!({
            "count": list.len(),
            "tunnels": list,
            "note": "kind: local(本地转发)/remote(远程转发)/dynamic(SOCKS5)"
        }))
        .unwrap_or_default();
        Ok(text_result(text))
    })
}

/// tunnel_create：建隧道（复用 GUI tunnel_start 路径——同一 manager，
/// 隧道生命周期与会话清理联动）。RemoteWrite 语义（网络暴露面变更）。
pub(crate) fn h_tunnel_create<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(async move {
        let asset_id = args
            .get("asset_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "缺少 asset_id 参数".to_string())?;
        let kind = args
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("local");
        let local_port = args
            .get("localPort")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "缺少 localPort 参数".to_string())? as u16;
        let remote_addr = args
            .get("remoteAddr")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "缺少 remoteAddr 参数（remote/local 转发的目标地址）".to_string())?;
        let remote_port = args
            .get("remotePort")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "缺少 remotePort 参数".to_string())? as u16;
        let bind_addr = args.get("bindAddr").and_then(|v| v.as_str()).unwrap_or("127.0.0.1");

        if !["local", "remote", "dynamic"].contains(&kind) {
            return Ok(error_result(&format!(
                "kind 仅支持 local / remote / dynamic（收到 {kind:?}）"
            )));
        }
        // remoteAddr 校验（拼入命令参数面）
        if remote_addr.contains('\'') || remote_addr.contains('\\') || remote_addr.contains(' ') {
            return Ok(error_result("remoteAddr 参数含非法字符"));
        }

        let Some(app) = &ctx.app_handle else {
            return Ok(text_result("隧道管理需要 GUI 运行（headless 模式无会话池）".to_string()));
        };
        use tauri::Manager;
        let Some(state) = app.try_state::<crate::AppState>() else {
            return Ok(error_result("AppState 不可用"));
        };

        // scope：建隧道 = 连资产（同 exec 的授权面）
        let store = myshelltool_core::load_connection_asset_store(&ctx.asset_store_path)
            .map_err(|e| format!("加载资产库失败: {e}"))?;
        let Some(asset) = store.assets.iter().find(|a| a.id == asset_id) else {
            return Ok(error_result(&format!("资产 {asset_id} 不存在")));
        };
        let scope_snapshot = ctx.config.read().await.scope.clone();
        let verdict = myshelltool_core::mcp_scope::evaluate(&scope_snapshot, asset);
        if !verdict.is_allowed() {
            return Err(myshelltool_core::mcp_scope::denied_message(verdict));
        }

        // MVP 诚实边界：remote 转发走资产专用连接（tunnel.rs 的既有路径）；
        // local/dynamic 需要活跃 GUI 会话 handle——提示经 GUI 建立，不在此伪造。
        if kind != "remote" {
            return Ok(error_result(
                "MVP 仅支持 remote 转发（走资产专用连接）；local/dynamic 需要活跃 GUI 会话，请在 GUI 隧道面板建立",
            ));
        }

        let config = crate::ssh::TunnelConfig {
            id: format!("mcp-{kind}-{local_port}-{}", chrono::Utc::now().timestamp_millis()),
            name: format!("MCP {kind} {local_port}"),
            kind: kind.to_string(),
            local_addr: bind_addr.to_string(),
            local_port,
            remote_addr: remote_addr.to_string(),
            remote_port,
            session_id: String::new(),
            auto_start: false,
            asset_id: Some(asset_id.to_string()),
        };

        // 复用 GUI 同一路径（G3：不另写）：tunnel_create 登记 tunnels 表 →
        // tunnel_start 启动（remote 走资产专用连接，session_id 空串不影响）
        let status = crate::ssh::tunnel_create(state.clone(), config.clone()).await?;
        crate::ssh::tunnel_start(state.clone(), String::new(), status.id.clone()).await?;
        let text = serde_json::to_string_pretty(&json!({
            "created": true,
            "tunnelId": status.id,
            "kind": kind,
            "listen": format!("{bind_addr}:{local_port}"),
            "target": format!("{remote_addr}:{remote_port}"),
            "note": "remote 转发已建立（专用连接）；tunnel_list 查看状态（active 字段）"
        }))
        .unwrap_or_default();
        Ok(text_result(text))
    })
}
