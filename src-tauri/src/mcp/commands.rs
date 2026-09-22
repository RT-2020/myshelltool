//! MCP 面板命令层（v0.20 自 lib.rs 按域迁出，守 800 行 Rust 硬上限）。
//! DTO（McpStatus 等）+ 面板命令（status/config/scope/token/日志/审批回传）
//! 全部在此；注册表在 lib.rs 的 generate_handler! 经 crate::mcp::commands:: 引用。

use serde::Serialize;
use tauri::State;

use crate::AppState;
// ─── v1.2：MCP 服务可观测性（前端状态栏/管理面板的聚合查询）───
//
// 把 pipe server 维护的连接状态 + MCP server 的静态能力声明（tools/resources/prompts）
// + 数据目录路径聚合成一个 DTO，供前端 mcp store 一次性拉取。
//
// 工具的「只读/高危」标记：v0.20（A2/A3）起从 registry.rs 的 RiskClass 派生
// （协议层 annotations 与本 tag 同源）；更早版本是这里手写的第三份映射，已删。
// approval.rs 的真实运行时判定逻辑不变，这里只是给 UI 展示用的静态标签。

/// MCP 工具/UI 条目（精简 DTO，避免直接序列化 rmcp 复杂类型）。
#[derive(Debug, Clone, Serialize)]
pub struct McpToolInfo {
    name: String,
    description: String,
    /// "readonly" | "dangerous"
    tag: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpResourceInfo {
    uri: String,
    name: String,
    is_template: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpPromptInfo {
    name: String,
    description: String,
    arguments: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpStatus {
    /// MCP server 协议层名字（静态声明，与 rmcp get_info 一致）。
    server_name: &'static str,
    /// GUI 自身版本（Cargo 包版本，与 tauri.conf.json 对齐）。
    server_version: &'static str,
    /// v1.4：MCP HTTP endpoint URL（供用户配置 MCP host）。
    /// v1.3 是 pipe_name（named pipe 路径），内嵌后改为 HTTP URL。
    /// v0.20（A1）：含入口鉴权 token（.../mcp/<token>）——复制即用；token 也存于
    /// mcp-endpoint.json 的 authToken 字段，供 probe/外部读取。
    endpoint: String,
    /// MCP 进程读写的数据目录（GUI 与 MCP 共享同一份资产/凭据）。
    data_dir: String,
    /// v1.4：MCP 就绪探测结果（HTTP 健康检查）。这是状态灯的唯一信号源——
    /// 向自己的 HTTP endpoint 发 initialize 握手，回答「能否正常工作」。
    /// 不再 spawn 子进程（v1.2 的一次性 spawn 已废弃）。
    probe: crate::mcp::probe::McpProbeResult,
    /// MCP 暴露的工具（26 个：11 只读（+C3 四件+tunnel_list）+ service_control/tunnel_create（RemoteWrite）+ ssh_exec + exec_many + ssh_exec_async/job_status/job_output/job_cancel + 6 文件类 + read_output；tag 判定见 tool_tag）。
    tools: Vec<McpToolInfo>,
    /// 3 静态资源 + 1 template。
    resources: Vec<McpResourceInfo>,
    /// 3 个诊断 prompt。
    prompts: Vec<McpPromptInfo>,
}

/// 工具名 → 静态标签。v0.20（A2）：从 registry 的 risk 派生（消除第三份手写映射）。
/// Write/Destructive → dangerous（写类工具标 readonly 是低估，随 A2 修正）；
/// 未知名称按旧默认 readonly（防御性——清单与判定同一张表，正常不会发生）。
pub fn tool_tag(name: &str) -> &'static str {
    match crate::mcp::registry::find(name) {
        Some(spec) => spec.risk.gui_tag(),
        None => "readonly",
    }
}

#[tauri::command]
pub async fn mcp_status(state: State<'_, AppState>) -> Result<McpStatus, String> {
    // v2.5：data_dir 用 setup 解析并托管在 AppState 的单一来源，不再独立
    // 重建（避免与 Tauri app_data_dir 分叉 / APPDATA 缺失时落到相对 CWD）。
    let data_dir = state.mcp_data_dir.clone();

    // v1.4：HTTP 健康检查。读 mcp-endpoint.json 拿实际监听地址，向它发 initialize。
    // 不再 spawn 子进程（v1.2 的 probe_mcp 已废弃）。
    // v0.20（A1）：探测 URL 必须带 token（鉴权中间件否则 401）——token 取 AppState
    // 内存里的权威值，不读文件里的 authToken 字段（启动竞态下文件可能是旧版）。
    let token = state
        .mcp_auth_token
        .read()
        .map(|g| g.clone())
        .unwrap_or_default();
    let endpoint = crate::mcp::http_server::read_endpoint(&data_dir)
        .map(|e| crate::mcp::http_server::with_token(&e.url, &token))
        .unwrap_or_default();
    let probe = if endpoint.is_empty() {
        crate::mcp::probe::fail_no_endpoint(&chrono::Utc::now().to_rfc3339())
    } else {
        crate::mcp::probe::probe_endpoint(&endpoint).await
    };

    // 静态能力声明：直接复用 mcp 模块的 schema 构造函数，保证与协议实际暴露一致。
    let tools: Vec<McpToolInfo> = crate::mcp::tools::list_all_tools()
        .into_iter()
        .map(|t| {
            // rmcp Tool 的 name/description 是 Cow<'_, str>（非 String），
            // annotations 当前无（None）。用静态 tag 表补充只读/高危标记。
            let tag = tool_tag(&t.name);
            McpToolInfo {
                name: t.name.to_string(),
                description: t.description.map(|d| d.to_string()).unwrap_or_default(),
                tag,
            }
        })
        .collect();
    let resources: Vec<McpResourceInfo> = crate::mcp::resources::list_resources()
        .into_iter()
        .map(|r| McpResourceInfo {
            uri: r.uri.to_string(),
            name: r.name.clone(),
            is_template: false,
        })
        .chain(
            crate::mcp::resources::list_resource_templates()
                .into_iter()
                .map(|t| McpResourceInfo {
                    uri: t.uri_template.to_string(),
                    name: t.name.clone(),
                    is_template: true,
                }),
        )
        .collect();
    let prompts: Vec<McpPromptInfo> = crate::mcp::prompts::list_prompts()
        .into_iter()
        .map(|p| McpPromptInfo {
            name: p.name.clone(),
            description: p.description.clone().unwrap_or_default(),
            arguments: p
                .arguments
                .as_ref()
                .map(|args| args.iter().map(|a| a.name.clone()).collect())
                .unwrap_or_default(),
        })
        .collect();

    Ok(McpStatus {
        server_name: "myshelltool",
        server_version: env!("CARGO_PKG_VERSION"),
        endpoint,
        data_dir: data_dir.to_string_lossy().to_string(),
        probe,
        tools,
        resources,
        prompts,
    })
}

/// v0.20（A1）：重置 MCP 入口鉴权 token。
///
/// 生成新 token → 更新共享 Arc（运行中的 HTTP server 立即按新值校验，旧 token
/// 即刻失效）→ 重写 mcp-endpoint.json → 返回含新 token 的完整接入 URL。
/// 调用方（面板「重置 token」按钮）负责提示用户重新复制配置到 MCP host。
/// 落盘失败返回 Err——写不出去却宣称成功 = 用户 host 全部失灵且无从归因。
#[tauri::command]
pub fn mcp_reset_token(state: State<'_, AppState>) -> Result<String, String> {
    let new_token = crate::mcp::http_server::generate_token();
    {
        let mut guard = state
            .mcp_auth_token
            .write()
            .map_err(|_| "MCP token 锁已中毒（曾有持锁 panic），请重启应用".to_string())?;
        *guard = new_token.clone();
    }
    // 读现有 endpoint 拿 port/url 再回写——文件缺失 = server 未完成首次绑定，
    // 此时内存 token 已生效但无落盘可改，如实报错让用户重启应用。
    let mut endpoint = crate::mcp::http_server::read_endpoint(&state.mcp_data_dir)
        .ok_or_else(|| "MCP endpoint 尚未生成（server 未启动），请重启应用后重试".to_string())?;
    endpoint.auth_token = new_token;
    crate::mcp::http_server::persist_endpoint(&state.mcp_data_dir, &endpoint)?;
    log::info!("MCP 入口 token 已重置（内容不入日志）");
    Ok(crate::mcp::http_server::with_token(&endpoint.url, &endpoint.auth_token))
}

/// v1.5：前端 GUI 弹窗审批的用户回传命令。
///
/// server.rs 在客户端不支持 elicitation 时，经 AppHandle emit `mcp-tool-approval`
/// 事件给前端 GlobalModals 弹窗；用户点确认/拒绝后，前端调本命令回传决定。
/// 复用 AppState 持有的 mcp_approval_pending Arc（与 McpToolContext 共享），
/// 取出 server.rs 注册的 oneshot::Sender 并 send。
///
/// 模式照 ssh.rs:998 ssh_confirm_host_key。
#[tauri::command]
pub async fn mcp_confirm_tool(
    state: State<'_, AppState>,
    request_id: String,
    accepted: bool,
) -> Result<(), String> {
    crate::mcp::approval::resolve_approval(&state.mcp_approval_pending, &request_id, accepted).await
}

// ─── v2：MCP 拦截等级配置 + 执行日志（前端 MCP 面板）───

/// 读当前拦截等级（内存 Arc，不动盘）。
#[tauri::command]
pub async fn mcp_get_config(state: State<'_, AppState>) -> Result<crate::mcp::config::McpConfig, String> {
    Ok(state.mcp_config.read().await.clone())
}

/// 切换拦截等级：解析枚举 → 更新共享 Arc（已建 MCP 会话下次调用即生效）→ 落盘。
/// 无效 level（非 minimal/strict）返回 Err。
/// v0.20（B1）：只改 level，**保留既有 scope**——重建整份配置会把用户配好的
/// 授权范围静默清空（fail-open 方向的意外重置）。
#[tauri::command]
pub async fn mcp_set_config(state: State<'_, AppState>, level: String) -> Result<crate::mcp::config::McpConfig, String> {
    let level_enum = crate::mcp::config::McpInterceptLevel::parse(&level).ok_or_else(|| {
        format!("无效的拦截等级: {level}（可选 minimal / strict）")
    })?;
    let mut new_config = state.mcp_config.read().await.clone();
    new_config.level = level_enum;
    *state.mcp_config.write().await = new_config.clone();
    crate::mcp::config::save_mcp_config(&crate::mcp::config::mcp_config_path(&state.mcp_data_dir), &new_config)?;
    Ok(new_config)
}

/// v0.20（B1 GUI 面板）：更新授权范围（scope）。
/// 只改 scope、保留 level（与 mcp_set_config 对称——两维度互不覆盖）。
/// 输入清洗：各列表 trim + 去空串去重；deny_all **不接受前端传入**（它是读盘
/// 损坏的收敛态，正常编辑永远 false——置 true 需要用户手动改坏文件，不该有 UI 通道）。
#[tauri::command]
pub async fn mcp_set_scope(
    state: State<'_, AppState>,
    scope: myshelltool_core::mcp_scope::McpScope,
) -> Result<crate::mcp::config::McpConfig, String> {
    let clean = |list: Vec<String>| -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        list.into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && seen.insert(s.clone()))
            .collect()
    };
    let cleaned = myshelltool_core::mcp_scope::McpScope {
        allowed_groups: clean(scope.allowed_groups),
        allowed_tags: clean(scope.allowed_tags),
        denied_asset_ids: clean(scope.denied_asset_ids),
        allow_local_fs: scope.allow_local_fs,
        deny_all: false, // 见上：收敛态不给 UI 通道
    };
    let mut new_config = state.mcp_config.read().await.clone();
    new_config.scope = cleaned;
    *state.mcp_config.write().await = new_config.clone();
    crate::mcp::config::save_mcp_config(&crate::mcp::config::mcp_config_path(&state.mcp_data_dir), &new_config)?;
    log::info!(
        "MCP scope 已更新（groups={} tags={} denied={} localFs={}），已建会话下次调用生效",
        new_config.scope.allowed_groups.len(),
        new_config.scope.allowed_tags.len(),
        new_config.scope.denied_asset_ids.len(),
        new_config.scope.allow_local_fs
    );
    Ok(new_config)
}

/// 读执行日志最近条目（timestampMs 倒序）。limit 缺省 200。
#[tauri::command]
pub fn mcp_list_execution_logs(
    state: State<'_, AppState>,
    limit: Option<u16>,
) -> Result<Vec<crate::mcp::execution_log::ExecutionLogEntry>, String> {
    Ok(crate::mcp::execution_log::list_entries(
        &state.mcp_data_dir,
        limit.unwrap_or(200) as usize,
    ))
}

/// 清空执行日志。
#[tauri::command]
pub async fn mcp_clear_execution_logs(state: State<'_, AppState>) -> Result<(), String> {
    crate::mcp::execution_log::clear_entries(&state.mcp_data_dir).await
}

