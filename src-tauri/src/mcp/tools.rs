//! MCP Tools 实现（Layer 3，M3 阶段：7 个只读工具）。
//!
//! 见 docs/plans/MCP服务接入-实施计划.md §5（Layer 3）。
//!
//! M3 范围（只读，全部自动执行无需审批）：
//! - list_assets：资产清单（无需会话）
//! - list_sessions：（M3 桩，返回空——v1.0 独立会话无持久 session 池）
//! - disk_usage / system_status / service_status：经 headless 一次性 exec
//! - sftp_list：SFTP 目录列表（M3 桩，留 M4 完善）
//! - resource_monitor_snapshot：（M3 桩，v1.0 不走 GUI 的监控轮询）
//!
//! 高危工具（ssh_exec/sftp_upload/sftp_remove/tunnel_*）在 M4 落地 + Layer 6 审批。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, CallToolResult, Content, Tool};
use serde_json::{json, Map};
use tauri::AppHandle;
use tokio::sync::{oneshot, Mutex};

use crate::ssh::{self, HeadlessConnectParams};

/// GUI 弹窗审批的 pending 表类型：request_id → oneshot::Sender<bool>。
///
/// server.rs 等待审批结果时注册 sender，mcp_confirm_tool 命令回传时 send。
/// 用 Arc 包裹让 McpToolContext 保持 Clone——lib.rs setup 时把它 clone 进
/// AppState，使 McpToolContext（server.rs 等待）与 AppState（命令 resolve）
/// 共享同一份表。模式照 ssh.rs:40/196 的 PendingDecisions。
pub type ApprovalPending = Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>;

/// MCP 工具上下文：持有资产库路径 + 凭据/known_hosts 路径。
///
/// v1.0（独立会话）：每次命令调用时按资产参数临时建连，exec 完即断。
/// v1.1（named pipe 桥接）：会改为持有 GUI 的 SshSessionManager Arc。
///
/// v1.5：加 GUI 弹窗审批降级（方案 A）。elicitation 仍是主路径，但客户端
/// 不支持 elicitation 时（如 ZCode），若 `app_handle` 存在则 emit 事件给
/// GUI 弹窗让用户确认，替代 v1.4 的 fail-secure 拒绝。
#[derive(Clone)]
pub struct McpToolContext {
    pub asset_store_path: PathBuf,
    pub secret_store_dir: PathBuf,
    pub known_hosts_path: PathBuf,
    /// GUI 弹窗审批的 pending 表（与 AppState 共享同一 Arc clone）。
    pub approval_pending: ApprovalPending,
    /// GUI 句柄，用于 emit 审批事件给前端弹窗。
    /// None = headless/测试/probe 模式（无 GUI → 退回 fail-secure 拒绝）。
    pub app_handle: Option<AppHandle>,
    /// 缓存最近一次 headless 连接的资产 id → handle，避免只读查询每次重连。
    /// v1.0 简化：M3 阶段先不缓存，每次按需建连。
    _session_cache: Arc<Mutex<()>>,
}

impl McpToolContext {
    /// 测试/headless 构造：无 GUI 句柄，高危命令无法弹窗（退回 fail-secure 拒绝）。
    /// approval_pending 仍是有效空表，保持 struct 完整性。
    ///
    /// 当前生产路径走 `new_with_gui`，本构造器为 headless/测试保留（未被调用），
    /// 故 allow(dead_code)——删除会导致测试或未来 headless bin 无可用构造路径。
    #[allow(dead_code)]
    pub fn new(
        asset_store_path: PathBuf,
        secret_store_dir: PathBuf,
        known_hosts_path: PathBuf,
    ) -> Self {
        Self {
            asset_store_path,
            secret_store_dir,
            known_hosts_path,
            approval_pending: Arc::new(Mutex::new(HashMap::new())),
            app_handle: None,
            _session_cache: Arc::new(Mutex::new(())),
        }
    }

    /// GUI 构造：持有 AppHandle + 共享 pending 表。lib.rs setup 用此路径。
    /// approval_pending 由调用方传入，确保与 AppState 持有同一份 Arc。
    pub fn new_with_gui(
        app_handle: AppHandle,
        approval_pending: ApprovalPending,
        asset_store_path: PathBuf,
        secret_store_dir: PathBuf,
        known_hosts_path: PathBuf,
    ) -> Self {
        Self {
            asset_store_path,
            secret_store_dir,
            known_hosts_path,
            approval_pending,
            app_handle: Some(app_handle),
            _session_cache: Arc::new(Mutex::new(())),
        }
    }
}

/// 返回 M4 阶段的全部工具 schema（7 只读 + 2 高危 = 9 个）。
pub fn list_all_tools() -> Vec<Tool> {
    vec![
        Tool::new(
            "list_assets",
            "列出所有已配置的 SSH 连接资产（不含密码/凭据，仅元数据：name/host/port/username/group/status）",
            empty_object_schema(),
        ),
        Tool::new(
            "list_sessions",
            "列出当前活跃的 SSH 会话（v1.0 独立模式无持久会话池，返回空列表）",
            empty_object_schema(),
        ),
        Tool::new(
            "disk_usage",
            "查询指定资产的磁盘使用情况（执行 df -h）",
            schema_with_required_session(),
        ),
        Tool::new(
            "system_status",
            "查询指定资产的系统状态：uptime / 内存 / 负载 / top 进程",
            schema_with_required_session(),
        ),
        Tool::new(
            "service_status",
            "查询指定资产上某 systemd 服务的状态（systemctl status <service>）",
            json!({
                "type": "object",
                "properties": {
                    "asset_id": { "type": "string", "description": "资产 ID" },
                    "service": { "type": "string", "description": "服务名，如 nginx / mysql / docker" }
                },
                "required": ["asset_id", "service"]
            })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        ),
        Tool::new(
            "sftp_list",
            "列出指定资产远程目录的文件/子目录（M3 桩，M4 完善 SFTP 路径）",
            json!({
                "type": "object",
                "properties": {
                    "asset_id": { "type": "string" },
                    "path": { "type": "string", "description": "远程路径，默认 /" }
                },
                "required": ["asset_id"]
            })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        ),
        Tool::new(
            "resource_monitor_snapshot",
            "获取指定资产的资源监控快照（CPU/内存/网络/磁盘，M3 桩，M4 接入完整轮询）",
            schema_with_required_session(),
        ),
        // ─── 高危工具（M4，经 Layer 6 审批）───
        Tool::new(
            "ssh_exec",
            "在指定资产上执行任意 Shell 命令。高危：命令经三层审批——白名单自动执行，黑名单/未知命令被拒绝并返回三段式说明（AI意图+真实命令+后果）。调用时必须如实声明 intent 意图。",
            json!({
                "type": "object",
                "properties": {
                    "asset_id": { "type": "string", "description": "资产 ID（先用 list_assets 查看）" },
                    "command": { "type": "string", "description": "要执行的 Shell 命令" },
                    "intent": { "type": "string", "description": "AI 对此命令的真实意图说明（用于审批对照识破伪装）" }
                },
                "required": ["asset_id", "command", "intent"]
            })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        ),
        Tool::new(
            "sftp_remove",
            "删除指定资产上的远程文件或目录。高危：始终需要审批，v1.0 模式下默认拒绝（需 GUI 手动操作）。",
            json!({
                "type": "object",
                "properties": {
                    "asset_id": { "type": "string", "description": "资产 ID" },
                    "path": { "type": "string", "description": "要删除的远程路径" },
                    "intent": { "type": "string", "description": "AI 对此删除操作的真实意图说明" }
                },
                "required": ["asset_id", "path", "intent"]
            })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        ),
    ]
}

/// 分发工具调用。返回 CallToolResult（成功用 text content，失败用 is_error）。
pub async fn call_tool(
    name: &str,
    params: CallToolRequestParams,
    ctx: &McpToolContext,
) -> Result<CallToolResult, String> {
    let arguments: Map<String, serde_json::Value> = params.arguments.unwrap_or_default();

    log::info!("MCP call_tool: {} args={}", name, serde_json::to_string(&arguments).unwrap_or_default());

    match name {
        "list_assets" => tool_list_assets(ctx).await,
        "list_sessions" => tool_list_sessions().await,
        "disk_usage" => exec_on_asset(ctx, &arguments, "df -h").await,
        "system_status" => exec_on_asset(ctx, &arguments, "uptime; echo '---'; free -h; echo '---'; top -bn1 | head -20").await,
        "service_status" => {
            let service = arguments
                .get("service")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if service.is_empty() {
                return Ok(error_result("缺少 service 参数"));
            }
            // 服务名做基础校验，防注入（仅允许字母数字-_@:.）
            if !service.chars().all(|c| c.is_alphanumeric() || "-_@:.".contains(c)) {
                return Ok(error_result("service 参数含非法字符"));
            }
            exec_on_asset(ctx, &arguments, &format!("systemctl status {}", service)).await
        }
        "sftp_list" => Ok(error_result("sftp_list 在 M3 阶段为桩，将在 M4 完善")),
        "resource_monitor_snapshot" => Ok(error_result("resource_monitor_snapshot 在 M3 阶段为桩，将在 M4 完善")),
        // ─── 高危工具（审批在 server.rs call_tool 拦截层做）───
        "ssh_exec" => tool_ssh_exec(ctx, &arguments).await,
        "sftp_remove" => {
            // v1.1：审批已移至 server.rs（elicitation 确认）。
            // 能走到这里说明用户已确认删除。实际删除执行待后续完善。
            let path = arguments.get("path").and_then(|v| v.as_str()).unwrap_or("");
            Ok(error_result(&format!(
                "用户已确认删除 {}，但 sftp_remove 的执行逻辑尚未实现（v1.1 专注 elicitation 审批 + 会话复用，删除执行留后续版本）",
                path
            )))
        }
        _ => Ok(error_result(&format!("未知工具: {}", name))),
    }
}

/// list_sessions：列出 GUI 已建立的 SSH 会话。
///
/// v1.4：删掉 v1.1 的 pipe 查询（内嵌后无独立 MCP 进程）。当前返回空桩 +
/// 说明（与 v1.0 一致）—— AI 仍可用 headless 工具（disk_usage/system_status/ssh_exec）。
///
/// TODO(follow-up v1.4+)：注入 GUI 的 `Arc<AsyncMutex<SshSessionManager>>` 到
/// McpToolContext，直接读会话池返回真实列表（同进程访问，无需 IPC）。
async fn tool_list_sessions() -> Result<CallToolResult, String> {
    Ok(text_result(
        "当前会话查询暂未接入 GUI 会话池（v1.4 重构中）。\
         可先用 list_assets 查看可用资产，disk_usage/system_status/ssh_exec \
         等工具会自动 headless 建连。",
    ))
}

/// list_assets：读资产库，返回脱敏元数据（去除 credential_id）。
async fn tool_list_assets(ctx: &McpToolContext) -> Result<CallToolResult, String> {
    let store = myshelltool_core::load_connection_asset_store(&ctx.asset_store_path)
        .map_err(|e| format!("加载资产库失败: {e}"))?;

    let assets: Vec<serde_json::Value> = store
        .assets
        .iter()
        .map(|a| {
            json!({
                "id": a.id,
                "name": a.name,
                "host": a.host,
                "port": a.port,
                "username": a.username,
                "group": a.group,
                "status": format!("{:?}", a.status),
                "tags": a.tags,
            })
        })
        .collect();

    let result = json!({
        "source": "local",
        "count": assets.len(),
        "assets": assets,
        "groups": store.groups,
    });
    Ok(text_result(&serde_json::to_string_pretty(&result).unwrap_or_default()))
}

/// ssh_exec：高危工具，经 Layer 6 审批后执行。
///
/// 三层审批（D9 + approval.rs）：
/// - command 命中白名单（READONLY_WHITELIST）→ 自动执行
/// - command 命中黄名单（v1.0 暂无，v1.1 按资产配置）→ 自动执行
/// - command 命中黑名单（dangerous_commands 16 条）→ 拒绝 + 三段式
/// - command 未知（不在任何名单）→ 拒绝（fail-secure 默认拒）
async fn tool_ssh_exec(
    ctx: &McpToolContext,
    args: &Map<String, serde_json::Value>,
) -> Result<CallToolResult, String> {
    let asset_id = args
        .get("asset_id")
        .and_then(|v| v.as_str())
        .ok_or("缺少 asset_id 参数")?;
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or("缺少 command 参数")?;
    let intent = args
        .get("intent")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    log::info!(
        "ssh_exec: asset={} intent={:?} command={:?}",
        asset_id,
        intent,
        command
    );

    // v1.1：审批已移至 server.rs call_tool 的 elicitation 拦截层。
    // 能走到这里说明命令已被审批通过（白名单自动放行 / 用户 elicitation 确认）。
    exec_on_asset(ctx, args, command).await
}

/// 在指定资产上执行一次性命令。
///
/// v1.4：删掉 v1.1 的 pipe 复用分支（内嵌后无独立 MCP 进程，不再需要 pipe 桥）。
/// 当前直接走 headless 建连（connect_headless + exec_command_once）。
///
/// TODO(follow-up v1.4+)：注入 GUI 的 `Arc<AsyncMutex<SshSessionManager>>` 到
/// McpToolContext，命中 GUI 已建立会话时直接复用（避免重连 + 二次 host key 验证）。
/// 这是 v1.1 pipe 复用的等价能力，内嵌后实现更简单（同进程直接访问，无 IPC）。
async fn exec_on_asset(
    ctx: &McpToolContext,
    args: &Map<String, serde_json::Value>,
    command: &str,
) -> Result<CallToolResult, String> {
    let asset_id = args
        .get("asset_id")
        .and_then(|v| v.as_str())
        .ok_or("缺少 asset_id 参数")?;

    // 从资产库找到对应资产（B5：asset_id → host:port:username）
    let store = myshelltool_core::load_connection_asset_store(&ctx.asset_store_path)
        .map_err(|e| format!("加载资产库失败: {e}"))?;
    let asset = store
        .assets
        .iter()
        .find(|a| a.id == asset_id)
        .ok_or_else(|| format!("资产 {} 不存在", asset_id))?;

    log::info!(
        "exec_on_asset: {}@{}:{} cmd={}",
        asset.username,
        asset.host,
        asset.port,
        command
    );

    // headless 建连（密码为空，从凭据存储读；host key 未信任会被拒绝）
    let params = HeadlessConnectParams {
        host: asset.host.clone(),
        port: asset.port,
        username: asset.username.clone(),
        password: String::new(), // 从 credential_id 读
        credential_id: asset.credential_id.clone(),
        auth_method: Some(format!("{:?}", asset.auth_method)),
        private_key_path: asset.private_key_path.clone(),
        passphrase: None,
        passphrase_credential_id: asset.passphrase_credential_id.clone(),
        secret_store_dir: ctx.secret_store_dir.clone(),
        known_hosts_path: ctx.known_hosts_path.clone(),
    };

    let handle = ssh::connect_headless(&params)
        .await
        .map_err(|e| {
            log::warn!("exec_on_asset connect failed for {}: {}", asset_id, e);
            e
        })?;

    let output = ssh::exec_command_once(&handle, command)
        .await
        .map_err(|e| format!("命令执行失败: {e}"))?;

    Ok(text_result(&output))
}

// ─── 辅助：schema / 结果构造 ───

fn empty_object_schema() -> Map<String, serde_json::Value> {
    json!({ "type": "object", "properties": {} })
        .as_object()
        .cloned()
        .unwrap_or_default()
}

fn schema_with_required_session() -> Map<String, serde_json::Value> {
    json!({
        "type": "object",
        "properties": {
            "asset_id": { "type": "string", "description": "资产 ID（先用 list_assets 查看）" }
        },
        "required": ["asset_id"]
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

fn text_result(text: &str) -> CallToolResult {
    CallToolResult::success(vec![Content::text(text.to_string())])
}

fn error_result(message: &str) -> CallToolResult {
    let mut result = CallToolResult::success(vec![Content::text(message.to_string())]);
    result.is_error = Some(true);
    result
}
