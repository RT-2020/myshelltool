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
///
/// **为什么是 `env LC_ALL=C` 而不是 `LC_ALL=C df …`**：sshd 用 `$SHELL -c '<cmd>'`
/// 执行 exec 串，远端登录 shell 未必是 POSIX sh——FreeBSD 的 root 默认就是
/// csh/tcsh，而 csh/fish 里 `VAR=value cmd` **不是赋值语法**，会被当命令名
/// （`LC_ALL=C: Command not found.`），本条命令整条不执行、工具完全取不到数据。
/// `env` 是 POSIX 且 BusyBox/BSD 均自带，两种 shell 下语义一致。
pub const CMD_DISK_USAGE: &str = "env LC_ALL=C df -h; echo rc_df=$?";
/// system_status 工具实际执行的命令。
/// top 段先落临时文件再截断：`top … | head` 的 `$?` 是 **head** 的退出码（恒 0），
/// 会把「top 不存在」伪装成该段成功（rc_top 语义失真）。`mktemp` 在 GNU/BSD
/// 与 BusyBox 上均可用；失败时退化为空文件（rc_top 反映真实失败）。
pub const CMD_SYSTEM_STATUS: &str = "env LC_ALL=C uptime; echo rc_uptime=$?; echo '---'; env LC_ALL=C free -h; echo rc_free=$?; echo '---'; __t=$(mktemp 2>/dev/null || echo /tmp/.myshelltool-top.$$); env LC_ALL=C top -bn1 >\"$__t\" 2>&1; echo rc_top=$?; head -20 \"$__t\"; rm -f \"$__t\"";
/// service_status 工具实际执行的命令（按服务名拼接）。
pub fn service_status_command(service: &str) -> String {
    format!("env LC_ALL=C systemctl status {service}; echo rc_systemctl=$?")
}

/// resource_monitor_snapshot 工具实际执行的命令。
/// 全程 env 前缀（不用 `export`）：csh/tcsh/fish 无 POSIX `export`，
/// 该赋值会以 `export: Command not found.` 失败并使 locale 未锁定。
pub const CMD_RESOURCE_MONITOR_SNAPSHOT: &str = "env LC_ALL=C uptime; echo rc_uptime=$?; echo '--- Memory ---'; env LC_ALL=C free -m; echo rc_free=$?; echo '--- Disk ---'; env LC_ALL=C df -h; echo rc_df=$?";

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
    /// GUI 弹窗审批的 pending 表（与 AppState 共享同一份 Arc clone）。
    pub approval_pending: ApprovalPending,
    /// GUI 句柄，用于 emit 审批事件给前端弹窗。
    /// None = headless/测试/probe 模式（无 GUI → 退回 fail-secure 拒绝）。
    pub app_handle: Option<AppHandle>,
    /// v2：拦截等级配置（与 AppState 共享同一份 Arc）。每次 call_tool 现读快照，
    /// mcp_set_config 后已建 HTTP 会话下次调用即生效。
    /// 选 tokio RwLock 而非 std：与 approval_pending 的 tokio::sync::Mutex
    /// 同族，读端可在 async 上下文无阻塞并发读。
    pub config: Arc<RwLock<McpConfig>>,
    /// v2：MCP 数据目录（mcp-config.json / mcp-execution-log.json 落盘位置）。
    /// lib.rs setup 统一从 mcp_data_dir() 取（环境变量优先），与 endpoint 一致。
    pub data_dir: PathBuf,
    /// v0.20（B3）：GUI 会话管理器（与 AppState 共享同一 Arc）——exec 工具
    /// 复用优先级的第一层「GUI 已连接会话」。None = headless/测试（跳过该层）。
    pub ssh_sessions: Option<Arc<tokio::sync::Mutex<crate::ssh::SshSessionManager>>>,
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
            ssh_sessions: None,
        }
    }

    /// GUI 构造：持有 AppHandle + 共享 pending 表 + 共享配置。lib.rs setup 用此路径。
    /// approval_pending / config / ssh_sessions 由调用方传入，确保与 AppState 持有同一份 Arc。
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_gui(
        app_handle: AppHandle,
        approval_pending: ApprovalPending,
        asset_store_path: PathBuf,
        secret_store_dir: PathBuf,
        known_hosts_path: PathBuf,
        config: Arc<RwLock<McpConfig>>,
        data_dir: PathBuf,
        ssh_sessions: Arc<tokio::sync::Mutex<crate::ssh::SshSessionManager>>,
    ) -> Self {
        Self {
            asset_store_path,
            secret_store_dir,
            known_hosts_path,
            approval_pending,
            app_handle: Some(app_handle),
            config,
            data_dir,
            ssh_sessions: Some(ssh_sessions),
        }
    }
}

/// 返回全部工具 schema。
/// v0.20（A2）：工具定义已收敛到 `registry.rs`（单一事实源），本函数保留为
/// 兼容转发（lib.rs mcp_status 等旧调用点不改路径）。
pub fn list_all_tools() -> Vec<Tool> {
    super::registry::all_tools()
}

/// 把工具参数表序列化成**可安全落日志**的摘要。
///
/// `command` 字段走 `redact_command`（值里常带 `-p<口令>`）；名字像凭据的字段
/// （password/token/secret…）整体遮蔽。其余字段原样保留——日志的排查价值在于
/// 「谁对哪台机器做了什么」，不依赖口令明文。
fn redacted_args_summary(args: &Map<String, serde_json::Value>) -> String {
    let mut safe = args.clone();
    for (key, value) in safe.iter_mut() {
        let lower = key.to_ascii_lowercase();
        if lower == "command" {
            if let Some(text) = value.as_str() {
                *value = serde_json::Value::String(myshelltool_core::redact_command(text));
            }
        } else if lower.contains("password")
            || lower.contains("secret")
            || lower.contains("token")
            || lower.contains("passphrase")
            || lower.contains("credential")
        {
            *value = serde_json::Value::String("<redacted>".to_string());
        }
    }
    serde_json::to_string(&safe).unwrap_or_else(|_| "<unserializable args>".to_string())
}

/// 分发工具调用。返回 CallToolResult（成功用 text content，失败用 is_error）。
pub async fn call_tool(
    name: &str,
    params: CallToolRequestParams,
    ctx: &McpToolContext,
) -> Result<CallToolResult, String> {
    let arguments: Map<String, serde_json::Value> = params.arguments.unwrap_or_default();

    // 参数表里可能含 `command`（其值常带 `-p<口令>`）与 `intent` 文本——整表序列化
    // 落日志等于把凭据写进 myshelltool.log（§8 红线）。故：
    // 命令值单独脱敏；其余字段照打（排查需要：哪个工具、哪个资产、哪个路径）。
    log::info!("MCP call_tool: {} args={}", name, redacted_args_summary(&arguments));

    // v0.20（A2）：分发走注册表——工具的 handler 在 registry.rs 的 ToolSpec 里
    // 声明，新增工具不再需要改这里的 match。未知工具报错口径与旧版一致。
    match super::registry::find(name) {
        Some(spec) => (spec.handler)(ctx, &arguments).await,
        None => Ok(error_result(&format!("未知工具: {}", name))),
    }
}

/// service_status handler（A2 从旧 call_tool 的 match 分支提取，行为逐字保留）。
pub(crate) async fn tool_service_status(
    ctx: &McpToolContext,
    arguments: &Map<String, serde_json::Value>,
) -> Result<CallToolResult, String> {
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
    exec_on_asset(ctx, arguments, &service_status_command(service)).await
}

/// v0.20（C3）：journalctl 结构化查询——跨发行版口径固化在服务端（AI 不再现场编命令）。
///
/// 跨发行版口径（诚实边界）：
/// - journalctl 仅存在于 systemd 发行版（Debian 8+/CentOS 7+/Ubuntu 16+）；
///   sysvinit/老系统上命令头部的 command -v 检查会以 rc=127 退出并给出降级指引
///   （查 /var/log/messages 或 /var/log/syslog）。
/// - --no-pager 必须显式：exec 通道无 TTY 时部分版本仍尝试分页；
/// - -o short-iso：时间戳稳定 ISO 格式（locale 无关）；
/// - 不用 -g（journalctl 自带 grep，systemd>=237 才有——CentOS 7 的 219 没有），
///   统一走管道 env LC_ALL=C grep -F（POSIX grep 恒有；rc 反映 grep 语义：无匹配=1）。
/// - env 前缀锁 locale（指南 §7 形态 B：输出不随服务器 locale 变化）。
///
/// 注入防护：unit 白名单（字母数字-_@:.，同 service_status）；since/until/grep

/// v0.20（C3 第二件）：监听端口结构化查询——ss → netstat → lsof 三级降级链
/// 固化在服务端（此前口径只在 audit_security prompt 文案里，AI 每次现场编命令
/// 正是「猜环境」；现在服务端固化）。
///
/// 降级链与口径（诚实边界，全部 POSIX/最低公约）：
/// - 首选 `ss`（iproute2，现代发行版标配）：`-tunlp` = TCP+UDP/数值端口与
///   地址/监听态/进程信息；`--no-header` 去表头稳化 AI 解析（iproute2 >= 4.9；
///   老版本无此选项会报错 → 由降级链接住）。
/// - 次选 `netstat`（老 CentOS 6/部分 BSD）：POSIX netstat 无 -p 的可移植性
///   差（Linux 有、BSD 形态不同），用 `-tunp`（Linux net-tools 口径）——注释
///   声明假设：非 Linux 的 BSD netstat 输出列序不同，AI 需按表头读。
/// - 兜底 `lsof`（最小公约）：`-nP -i` 列全部网络端点（含非 LISTEN），
///   后置 `grep LISTEN`（Linux/共用 lsof 口径；rc 反映 grep：无监听=1）。
/// - `env LC_ALL=C` 锁三段命令的输出 locale（指南 §7 形态 B）。
/// - tcp_only 参数：ss/netstat 加 -t 前已含；lsof 段用 grep -E 'TCP.*LISTEN'
///   粗滤（诚实口径：lsof 兜底段不做精细协议过滤，输出含表头供 AI 自辨）。
///

/// v0.20（C3 第三件）：进程列表结构化——ps POSIX 形态 + 排序固化在服务端。
///
/// 跨发行版口径（诚实边界）：
/// - 列集 `pid,ppid,user,%cpu,%mem,rss,stat,etime,comm` 在 procps（Linux）与
///   BSD ps（macOS/FreeBSD）的 `-eo` 形态都可用。
/// - `--no-headers` 是 procps 扩展（BSD 无）——用 `(cmd1 --no-headers 2>/dev/null || cmd2)`
///   子 shell 包裹降级：BSD 走 cmd2（带表头，但表头行在管道排序中非数值会沉底），
///   故 cmd2 首行 echo 固定表头（AI 解析始终有稳定列名）。
/// - 排序不依赖 ps --sort / sort -g 的 GNU 差异：**POSIX sort -k<N> -nr**——
///   %cpu/%mem 列是数值（含小数点，sort -n 前缀数值比较语义正确）。
/// - `head -<limit>`（POSIX）截前 N。子 shell 包裹保证两条 ps 路径都进同一
///   sort|head 管道（shell 里 `|` 优先于 `||`，不包裹会让首选路径绕过排序）。
/// - env LC_ALL=C 三段锁定；rc_chain 反映整段。
///

/// v0.20（C3 收官件）：按条件查找文件——find POSIX 形态 + 结果上限，固化在服务端。
///
/// 跨发行版口径（诚实边界）：
/// - **不用 `-printf`**（GNU 扩展，BusyBox/BSD 上整条失败——项目真实事故，
///   fact-guards 的 no-gnu-only-flags 规则即由此而来）：输出格式用默认路径列表
///   （POSIX find 恒有），需要元数据时 AI 对少量结果逐个 sftp_stat。
/// - `-maxdepth` 同为 GNU 起源但 **BusyBox/BSD find 均已支持**（busybox 1.x、
///   FreeBSD find 皆有），作为唯一深限手段保留；若目标系统极老报错，
///   rc_chain 非零——AI 可改用 ssh_exec 自行降级。
/// - `-size`/`-mtime`/`-name` 是 POSIX 标准。
/// - **结果上限在服务端拼进命令**（`head -N` 管道）：不依赖 AI 记得收窄——
///   深目录 find 输出几十万行会直接炸上下文（这正是加本工具的动机）。
/// - `env LC_ALL=C` 锁 find 的错误信息 locale。
/// - rc_chain 反映整段；head 截断时 find 侧 SIGPIPE 的 rc 噪音由管道语义吸收。
///
/// 注入防护：path 禁单引号/反斜杠/分号/空格（拼入单引号字面量）；
/// name_pattern 同校验后进 `-name '<pattern>'`（通配符是 find 语义、允许）；
/// size/mtime 数字；sort 枚举白名单（name/mtime/size——mtime/size 需要 GNU

/// list_sessions：列出客户端活动的 SSH 会话。
pub(crate) async fn tool_list_sessions(ctx: &McpToolContext) -> Result<CallToolResult, String> {
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
/// v0.20（B1）：scope 生效时只返回范围内的资产，并带 scopeApplied/hiddenCount
/// ——让 AI 知道「世界比它看到的大」，而不是得出「只有这几台机器」的错误结论。
/// v0.20（B4）：视图组装收敛到 resources::assets_view（与 ://assets 资源同一实现）。
pub(crate) async fn tool_list_assets(ctx: &McpToolContext) -> Result<CallToolResult, String> {
    let store = myshelltool_core::load_connection_asset_store(&ctx.asset_store_path)
        .map_err(|e| format!("加载资产库失败: {e}"))?;
    let scope = ctx.config.read().await.scope.clone();
    let mut view = super::resources::assets_view(&store, &scope);
    if scope.is_restricted() {
        let hidden = view["hiddenCount"].as_u64().unwrap_or(0);
        if hidden > 0 {
            view["scopeNote"] = json!(
                "授权范围已生效：以上为 MCP 可访问的资产；另有 hiddenCount 台资产在范围外（不可访问）"
            );
        }
    }
    Ok(text_result(&serde_json::to_string_pretty(&view).unwrap_or_default()))
}

/// ssh_exec：高危工具，经 approval.rs 审批后执行。
///
/// 审批（D9 + v2.1 拦截等级 + catastrophic 硬拦）：
/// - 毁灭性命令（detect_catastrophic_command）→ HardBlock 直接拒绝（两档同）
/// - command 命中白名单（READONLY_WHITELIST）→ 自动执行（两档同）
/// - command 命中非毁灭黑名单（rm -rf 子路径等）→ Minimal 放行 / Strict 需确认
/// - command 未知/黄名单 → Minimal 放行 / Strict 需确认（等级用户可配置）
/// v2：审批已移至 server.rs call_tool 拦截层并记执行日志。
pub(crate) async fn tool_ssh_exec(
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

    // 命令文本在应用日志里脱敏（§8 凭据红线）：`mysql -pP@ss` 这类明文不该进
    // myshelltool.log。审批弹窗仍展示原命令（透明性优先），见 server.rs。
    log::info!("ssh_exec: asset={} intent={:?} command={:?}", asset_id, intent, myshelltool_core::redact_command(command));

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
pub(crate) async fn exec_on_asset(
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

    // v0.20（B1）scope 收口点①：全部 exec 类工具的必经之路。判定内核在
    // core::mcp_scope（组件级分组匹配/优先级/fail-closed 有单测）；server.rs
    // 分发前还有一次预判（记 scopeResult 日志），这里是兜底防线——任何未来
    // 旁路（新增 handler 绕过预判直连建连）都会在此被拦下。
    let scope_snapshot = ctx.config.read().await.scope.clone();
    let verdict = myshelltool_core::mcp_scope::evaluate(&scope_snapshot, asset);
    if !verdict.is_allowed() {
        log::warn!(
            "exec_on_asset: asset {} denied by scope ({})",
            asset_id,
            verdict.as_str()
        );
        return Err(myshelltool_core::mcp_scope::denied_message(verdict));
    }

    log::info!("exec_on_asset: {}@{}:{} cmd={}", asset.username, asset.host, asset.port, myshelltool_core::redact_command(command));

    // v0.20（B3）三层复用：GUI 已连接会话 → headless 池 → 新建（用完回池）。
    // 返回 (输出, 会话来源)，来源记入执行日志 sessionSource（gui/pool/new）。
    let (output, source) = match exec_with_reuse(ctx, asset, asset_id, command).await {
        Ok(v) => v,
        Err(e) => {
            log::warn!("exec_on_asset execute failed for {}: {}", asset_id, e);
            return Err(e);
        }
    };

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

    // 截断保护（v0.20/B2）：正文超 MAX_RETURN_CHARS 头尾保留，完整输出存入
    // output_cache 并在提示里给出 cursor——AI 可用 read_output 工具分页取回
    // 原文（不再「截断即丢失」）；「收窄命令重取」仍保留（多数场景更省）。
    let total_chars = body.chars().count();
    if total_chars > MAX_RETURN_CHARS {
        let cursor = super::output_cache::store(body.clone().into_bytes()).await;
        let truncated =
            super::execution_log::truncate_middle(&body, TRUNCATE_HEAD, TRUNCATE_TAIL);
        body = format!(
            "[输出已截断：完整 {total_chars} 字符。原文可用 read_output 工具取回（cursor=\"{cursor}\"，10 分钟内有效），或用 grep / tail -n / head -n 收窄命令重取]\n{truncated}"
        );
    }

    LAST_EXEC_SESSION_SOURCE.with(|c| c.borrow_mut().replace(source));
    Ok(text_result(&format!("{exit_line}\n{body}")))
}

thread_local! {
    /// 本次 exec_on_asset 实际使用的会话来源（server.rs 组装执行日志 sessionSource
    /// 时读取）。async 下 thread_local 不完美（跨 .await 的任务可能被调度到别的
    /// worker），但 exec_on_asset 在返回前写入、server.rs 紧随其后读取，中间无
    /// 挂起点的错位概率极低；即便读到过期值也只是日志字段偏差，不影响安全判定。
    /// B3 若后续要求严格准确，可改为在 McpToolContext 加 per-call 通道。
    static LAST_EXEC_SESSION_SOURCE: std::cell::RefCell<Option<&'static str>> =
        const { std::cell::RefCell::new(None) };
}

/// server.rs 组装日志时读取最近一次 exec 的会话来源。
pub(crate) fn last_exec_session_source() -> Option<&'static str> {
    LAST_EXEC_SESSION_SOURCE.with(|c| *c.borrow())
}

/// B3 三层复用的执行内核。返回 (输出, 来源标识)。
///
/// 层① GUI 会话（信任增强：用户在终端里正开着同主机会话时复用它——
/// 用户能实时看到 AI 的流量；exec_command_once 已泛型化，GUI/Headless
/// 两种 Handle 共用同一输出组装，退出码语义不漂移）。
/// 层② headless 池（凭据快照比对 + is_closed 探死；失败即失效）。
/// 层③ 新建（用完 put 回池，密码为空从凭据存储读；host key 未信任会被拒绝）。
pub(crate) async fn exec_with_reuse(
    ctx: &McpToolContext,
    asset: &myshelltool_core::ConnectionAsset,
    asset_id: &str,
    command: &str,
) -> Result<(ssh::ExecOnceOutput, &'static str), String> {
    // 层①：GUI 已连接会话
    if let Some(manager) = &ctx.ssh_sessions {
        let gui_session_id = {
            let guard = manager.lock().await;
            guard.find_session_by_host(&asset.host, asset.port, &asset.username)
        };
        if let Some(session_id) = gui_session_id {
            let handle = {
                let guard = manager.lock().await;
                guard.session_handle(&session_id)
            };
            if let Some(handle) = handle {
                match ssh::exec_command_once(&handle, command).await {
                    Ok(output) => {
                        log::info!("exec_on_asset: reusing GUI session {session_id}");
                        return Ok((output, "gui"));
                    }
                    Err(e) => {
                        // GUI 会话半开/刚断：不视为失败，落到下层（warn 留痕）
                        log::warn!("exec_on_asset: GUI session {session_id} unusable ({e}), falling back to headless");
                    }
                }
            }
        }
    }

    // 层②：headless 池
    let auth = super::session_pool::AuthSnapshot::from_asset(asset);
    if let Some(handle) = super::session_pool::get(asset_id, &auth).await {
        match ssh::exec_command_once(&handle, command).await {
            Ok(output) => return Ok((output, "pool")),
            Err(e) => {
                // 服务器侧掐断的兜底：失效重建（不静默——warn 留痕后新建重试一次）
                log::warn!("exec_on_asset: pooled connection for {asset_id} unusable ({e}), invalidating and reconnecting");
                super::session_pool::invalidate_asset(asset_id).await;
            }
        }
    }

    // 层③：新建 + 回池
    let params = HeadlessConnectParams {
        host: asset.host.clone(),
        port: asset.port,
        username: asset.username.clone(),
        password: String::new(), // 从 credential_id 读
        credential_id: asset.credential_id.clone(),
        auth_method: Some(format!("{:?}", asset.auth_method)),
        private_key_path: asset.private_key_path.clone(),
        // v0.20（B0）：凭据库托管私钥内容（与 GUI 同优先级：SecretStore 优先、文件兜底）
        private_key_credential_id: asset.private_key_credential_id.clone(),
        passphrase: None,
        passphrase_credential_id: asset.passphrase_credential_id.clone(),
        secret_store_dir: ctx.secret_store_dir.clone(),
        known_hosts_path: ctx.known_hosts_path.clone(),        // v0.20（SSH P0-2）：跳板透传（资产配置了 jump_host 即经跳板连接）
        jump_host: crate::ssh::jump_host_of(asset).map(str::to_string),
        connect_timeout_secs: asset.connect_timeout_secs,
        keepalive_interval_secs: asset.keepalive_interval_secs,
        asset_store_path: Some(ctx.asset_store_path.clone()),
    };
    let handle = ssh::connect_headless(&params).await?;
    let output = ssh::exec_command_once(&handle, command).await;
    match output {
        Ok(o) => {
            // 连接健康才回池（执行失败可能是连接问题，不缓存病连接）
            super::session_pool::put(asset_id, auth, std::sync::Arc::new(handle)).await;
            Ok((o, "new"))
        }
        Err(e) => Err(format!("命令执行失败: {e}")),
    }
}

/// read_output（v0.20/B2）：按 cursor 分页取回 ssh_exec 被截断的完整输出。
/// 只读缓存，不做文件读取（sftp_read_file 的续读走它自身的 offset 参数，
/// 每次照常过敏感路径审批——本工具绝不能成为绕过审批的第二通道）。
pub(crate) async fn tool_read_output(
    _ctx: &McpToolContext,
    args: &Map<String, serde_json::Value>,
) -> Result<CallToolResult, String> {
    let cursor = args
        .get("cursor")
        .and_then(|v| v.as_str())
        .ok_or("缺少 cursor 参数")?;
    let offset = args
        .get("offset")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(64 * 1024)
        .min(1024 * 1024) as usize; // 单页上限 1 MiB

    let (chunk, total, eof, dropped_head) =
        super::output_cache::read(cursor, offset, limit).await?;
    let head_note = if dropped_head > 0 {
        format!(
            "\n[注：该输出开头 {dropped_head} 字节因超出单条缓存上限（8 MiB）未保留]"
        )
    } else {
        String::new()
    };
    let status = if eof { "已到末尾（EOF）" } else { "未完，继续传 offset 取下一段" };
    let next = if eof {
        String::new()
    } else {
        format!("，下一段 offset={}", (offset as usize + chunk.len()) as u64)
    };
    Ok(text_result(&format!(
        "[read_output cursor={cursor}：总 {total} 字节，本段 offset={offset} 起 {len} 字节，{status}{next}]{head_note}\n{chunk}",
        len = chunk.len()
    )))
}

// ─── 辅助：schema / 结果构造 ───

pub(crate) fn empty_object_schema() -> Map<String, serde_json::Value> {
    json!({ "type": "object", "properties": {} })
        .as_object()
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn schema_with_required_session() -> Map<String, serde_json::Value> {
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
