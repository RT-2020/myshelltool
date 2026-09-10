//! MCP Tools 实现（只读、执行与文件传输全能力）。
//!
//! 见 docs/plans/MCP文件传输-实施计划.md。
//!
//! - list_assets / list_sessions / disk_usage / system_status / service_status
//! - resource_monitor_snapshot
//! - ssh_exec（命令审批与结构化输出）
//! - 文件传输体系（sftp_list / sftp_read_file / sftp_write_file / sftp_upload / sftp_download / sftp_remove）委托给 file_tools.rs

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, CallToolResult, Content, Tool};
use serde_json::{json, Map};
use tauri::AppHandle;
use tokio::sync::{oneshot, Mutex, RwLock};

use crate::ssh::{self, HeadlessConnectParams};

use super::config::McpConfig;

/// GUI 弹窗审批的 pending 表类型：request_id → oneshot::Sender<bool>。
///
/// server.rs 等待审批结果时注册 sender，mcp_confirm_tool 命令回传时 send。
/// 用 Arc 包裹让 McpToolContext 保持 Clone——lib.rs setup 时把它 clone 进
/// AppState，使 McpToolContext（server.rs 等待）与 AppState（命令 resolve）
/// 共享同一份表。模式照 ssh.rs:40/196 的 PendingDecisions。
pub type ApprovalPending = Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>;

// ─── 只读工具内部拼接的固定命令（单一事实源）───
//
// v2：server.rs 执行日志需要记录「真实执行的命令文本」，抽成 pub 常量/函数
// 避免两处硬编码漂移。tools.rs 分发与 server.rs 日志共用同一份。
//
// v2.5 逐段 rc 回声：复合命令的 shell 退出码只反映**最后一条**（echo/head
// 恒 0），macOS 无 free、BSD top 无 -b 时前面的 `command not found` 会被
// exit_code=0 掩盖成噪音。因此每个子命令后紧跟 `echo rc_<名>=$?`，AI 可
// 按段定位真实失败。已知局限：`top | head` 管道段的 rc_top 反映管道末端
// （head）的退出码，top 自身失败经 stderr 带出（POSIX shell 无可移植的
// PIPESTATUS 用法，dash/zsh 对 `${PIPESTATUS[0]}` 报 Bad substitution）。

/// disk_usage 工具实际执行的命令。
/// MCP 命令统一锁 C locale：输出（含列头）不随服务器 locale 变化，
/// 保证 AI 消费与执行日志稳定可解析（指南 §7 形态 B）。
pub const CMD_DISK_USAGE: &str = "LC_ALL=C df -h; echo rc_df=$?";
/// system_status 工具实际执行的命令。
pub const CMD_SYSTEM_STATUS: &str = "export LC_ALL=C; uptime; echo rc_uptime=$?; echo '---'; free -h; echo rc_free=$?; echo '---'; top -bn1 | head -20; echo rc_top=$?";
/// service_status 工具实际执行的命令（按服务名拼接）。
pub fn service_status_command(service: &str) -> String {
    format!("LC_ALL=C systemctl status {service}; echo rc_systemctl=$?")
}

// ─── v2.1 ssh_exec 返回截断保护（exec_on_asset 组装结构化返回时用）───

/// 返回正文（不含 exit_code 行）的字符上限，超出触发头尾截断。
const MAX_RETURN_CHARS: usize = 16000;
/// 截断保留的头部字符数。
const TRUNCATE_HEAD: usize = 8000;
/// 截断保留的尾部字符数。
const TRUNCATE_TAIL: usize = 8000;

/// MCP 工具上下文：持有资产库路径 + 凭据/known_hosts 路径。
///
/// v1.0（独立会话）：每次命令调用时按资产参数临时建连，exec 完即断。
/// v1.5：加 GUI 弹窗审批降级（方案 A）。elicitation 仍是主路径，但客户端
/// 不支持 elicitation 时（如 ZCode），若 `app_handle` 存在则 emit 事件给
/// GUI 弹窗让用户确认，替代 v1.4 的 fail-secure 拒绝。
/// v2：加 `config`（拦截等级共享配置）与 `data_dir`（执行日志/配置落盘目录）。
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
    /// v2：拦截等级配置（与 AppState 共享同一 Arc）。每次 call_tool 现读快照，
    /// mcp_set_config 后已建 HTTP 会话下次调用即生效。
    /// 选 tokio RwLock 而非 std：与 approval_pending 的 tokio::sync::Mutex
    /// 同族，读端可在 async 上下文无阻塞并发读。
    pub config: Arc<RwLock<McpConfig>>,
    /// v2：MCP 数据目录（mcp-config.json / mcp-execution-log.json 落盘位置）。
    /// lib.rs setup 统一从 mcp_data_dir() 取（环境变量优先），与 endpoint 一致。
    pub data_dir: PathBuf,
    /// 缓存最近一次 headless 连接的资产 id → handle，避免只读查询每次重连。
    /// v1.0 简化：M3 阶段先不缓存，每次按需建连。
    _session_cache: Arc<Mutex<()>>,
}

impl McpToolContext {
    /// 测试/headless 构造：无 GUI 句柄，高危命令无法弹窗（退回 fail-secure 拒绝）。
    /// approval_pending 仍是有效空表，config 用默认等级（Minimal），保持 struct 完整性。
    ///
    /// 当前生产路径走 `new_with_gui`，本构造器为 headless/测试保留（未被调用），
    /// 故 allow(dead_code)——删除会导致测试或未来 headless bin 无可用构造路径。
    #[allow(dead_code)]
    pub fn new(
        asset_store_path: PathBuf,
        secret_store_dir: PathBuf,
        known_hosts_path: PathBuf,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            asset_store_path,
            secret_store_dir,
            known_hosts_path,
            approval_pending: Arc::new(Mutex::new(HashMap::new())),
            app_handle: None,
            config: Arc::new(RwLock::new(McpConfig::default())),
            data_dir,
            _session_cache: Arc::new(Mutex::new(())),
        }
    }

    /// GUI 构造：持有 AppHandle + 共享 pending 表 + 共享配置。lib.rs setup 用此路径。
    /// approval_pending / config 由调用方传入，确保与 AppState 持有同一份 Arc。
    pub fn new_with_gui(
        app_handle: AppHandle,
        approval_pending: ApprovalPending,
        asset_store_path: PathBuf,
        secret_store_dir: PathBuf,
        known_hosts_path: PathBuf,
        config: Arc<RwLock<McpConfig>>,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            asset_store_path,
            secret_store_dir,
            known_hosts_path,
            approval_pending,
            app_handle: Some(app_handle),
            config,
            data_dir,
            _session_cache: Arc::new(Mutex::new(())),
        }
    }
}

/// 返回 M4 阶段的全部工具 schema（7 只读 + 2 高危 = 9 个）。
pub fn list_all_tools() -> Vec<Tool> {
    let mut tools = vec![
        Tool::new(
            "list_assets",
            "列出所有已配置的 SSH 连接资产（不含密码/凭据，仅元数据：name/host/port/username/group/status）",
            empty_object_schema(),
        ),
        Tool::new(
            "list_sessions",
            "列出当前客户端活跃的 SSH 会话清单（session_id 列表）。",
            empty_object_schema(),
        ),
        Tool::new(
            "disk_usage",
            "查询指定资产的磁盘使用情况（执行 df -h）。stdout 含 rc_df=N 行标注 df 的真实退出码（复合命令回声，勿只看首行 exit_code）",
            schema_with_required_session(),
        ),
        Tool::new(
            "system_status",
            "查询指定资产的系统状态：uptime / 内存 / 负载 / top 进程。仅支持 Linux（依赖 /proc 与 systemd）主机，其他平台命令会失败。stdout 含 rc_uptime/rc_free/rc_top=N 行标注各子命令真实退出码（勿只看首行 exit_code，它只反映最后的 echo）",
            schema_with_required_session(),
        ),
        Tool::new(
            "service_status",
            "查询指定资产上某 systemd 服务的状态（systemctl status <service>）。仅支持 Linux（依赖 /proc 与 systemd）主机，其他平台命令会失败。stdout 含 rc_systemctl=N 行标注真实退出码",
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
            "resource_monitor_snapshot",
            "获取指定资产的资源监控快照（CPU负载、内存占用、磁盘空间概览）。仅支持 Linux（依赖 /proc 与 systemd）主机，其他平台命令会失败。stdout 含 rc_uptime/rc_free/rc_df=N 行标注各子命令真实退出码",
            schema_with_required_session(),
        ),
        // ─── 高危 Shell 执行工具（经 Layer 6 审批）───
        Tool::new(
            "ssh_exec",
            "在指定资产上执行任意 Shell 命令。返回结构化文本：首行 exit_code=<n>（无退出码时 exit_code=unknown），随后为 stdout（无输出时给出提示），stderr 非空时以 --- stderr --- 分隔行附后，超长自动头尾截断。拦截语义：毁灭性命令（mkfs/dd 写块设备/rm 根级删除等）直接拒绝；其余按当前拦截等级放行或需确认（等级可在 myshelltool GUI 的 MCP 面板调整）。调用时必须如实声明 intent 意图。",
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
    ];

    // ─── 文件传输类工具（Layer 3 + 4，详见 file_tools.rs）───
    tools.extend(super::file_tools::list_file_tools());
    tools
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
        "list_sessions" => tool_list_sessions(ctx).await,
        "disk_usage" => exec_on_asset(ctx, &arguments, CMD_DISK_USAGE).await,
        "system_status" => exec_on_asset(ctx, &arguments, CMD_SYSTEM_STATUS).await,
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
            exec_on_asset(ctx, &arguments, &service_status_command(service)).await
        }
        "resource_monitor_snapshot" => {
            exec_on_asset(
                ctx,
                &arguments,
                "export LC_ALL=C; uptime; echo rc_uptime=$?; echo '--- Memory ---'; free -m; echo rc_free=$?; echo '--- Disk ---'; df -h; echo rc_df=$?",
            )
            .await
        }
        // ─── 高危工具（审批在 server.rs call_tool 拦截层做）───
        "ssh_exec" => tool_ssh_exec(ctx, &arguments).await,
        _ => {
            // 尝试文件工具分发（sftp_list/sftp_read_file/sftp_write_file/sftp_upload/sftp_download/sftp_remove）
            if let Some(res) = super::file_tools::dispatch_file_tool(ctx, name, &arguments).await? {
                Ok(res)
            } else {
                Ok(error_result(&format!("未知工具: {}", name)))
            }
        }
    }
}

/// list_sessions：列出客户端活动的 SSH 会话。
async fn tool_list_sessions(ctx: &McpToolContext) -> Result<CallToolResult, String> {
    if let Some(app) = &ctx.app_handle {
        use tauri::Manager;
        if let Some(state) = app.try_state::<crate::AppState>() {
            let sessions_lock = state.ssh_sessions.lock().await;
            let ids = sessions_lock.list_session_ids();
            if ids.is_empty() {
                return Ok(text_result(
                    "当前客户端暂无活动 SSH 会话（可在 GUI 中连接主机，或直接使用基于 asset_id 的各项远程工具）。",
                ));
            }
            let list_str = ids
                .iter()
                .map(|id| format!("- session_id: {id}"))
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(text_result(&format!(
                "当前活动 SSH 会话列表（共 {} 个）：\n{}",
                ids.len(),
                list_str
            )));
        }
    }
    Ok(text_result(
        "当前处于 Headless/独立运行模式。建议直接使用基于 asset_id 的远程工具执行操作。",
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

/// ssh_exec：高危工具，经 approval.rs 审批后执行。
///
/// 审批（D9 + v2.1 拦截等级 + catastrophic 硬拦）：
/// - 毁灭性命令（detect_catastrophic_command）→ HardBlock 直接拒绝（两档同）
/// - command 命中白名单（READONLY_WHITELIST）→ 自动执行（两档同）
/// - command 命中非毁灭黑名单（rm -rf 子路径等）→ Minimal 放行 / Strict 需确认
/// - command 未知/黄名单 → Minimal 放行 / Strict 需确认（等级用户可配置）
/// v2：审批已移至 server.rs call_tool 拦截层并记执行日志。
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

/// 在指定资产上执行一次性命令（v2.1 返回结构化文本）。
///
/// v1.4：删掉 v1.1 的 pipe 复用分支（内嵌后无独立 MCP 进程，不再需要 pipe 桥）。
/// 当前直接走 headless 建连（connect_headless + exec_command_once），并把
/// ExecOnceOutput 组装为「exit_code 行 + stdout + stderr 段」结构化文本，
/// 超长按 MAX_RETURN_CHARS 头尾截断（见下方常量注释）。
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

    // v2.1 结构化返回：exit_code 行 + stdout + stderr 段，MCP 主代理可据此
    // 判断执行情况（旧版丢弃退出码且无输出命令返回空串，无法判断）。
    let exit_line = match output.exit_code {
        Some(code) => format!("exit_code={code}"),
        None => "exit_code=unknown".to_string(),
    };

    let mut body = if output.stdout.is_empty() {
        "（命令无标准输出）".to_string()
    } else {
        output.stdout
    };
    if !output.stderr.is_empty() {
        body.push_str("\n--- stderr ---\n");
        body.push_str(&output.stderr);
    }

    // 截断保护：正文（不含 exit_code 行）超 MAX_RETURN_CHARS 时头尾保留，
    // 头部插入截断提示（N 为截断前总字符数），引导收窄命令重取。
    let total_chars = body.chars().count();
    if total_chars > MAX_RETURN_CHARS {
        let truncated =
            super::execution_log::truncate_middle(&body, TRUNCATE_HEAD, TRUNCATE_TAIL);
        body = format!(
            "[输出已截断：完整 {total_chars} 字符，请用 grep / tail -n / head -n 收窄命令重取]\n{truncated}"
        );
    }

    Ok(text_result(&format!("{exit_line}\n{body}")))
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
