//! MCP 工具注册表（v0.20，A2+A3）：工具面的**单一事实源**。
//!
//! ## 为什么（MCP服务设计-v3 §3.3）
//!
//! 旧实现里一个工具散在 4 处 match：tools.rs 的 schema 表与 call_tool 分发、
//! file_tools.rs 的二次分发、server.rs 的 check_approval_needed 与
//! log_scope_for——新增工具要改 4 处，漏一处即策略错配（审批降级或日志错归）。
//! 现在：新增工具 = 在 `registry()` 表里加一行。
//!
//! ## 字段语义
//!
//! - `risk` → 协议 annotations（A3）+ GUI 能力清单 tag（lib.rs）。
//!   ⚠️ 协议明确 annotations 是 **hints**（"clients should never make tool use
//!   decisions based on annotations from untrusted servers"）——它**不替代**
//!   服务端审批链，只是让 host 的 UI 与自动放行策略有依据。
//! - `policy` → **真正的运行时审批判定**（server.rs 按它分 6 个分支，
//!   与旧 check_approval_needed 逐工具 match 行为一一对应）。
//! - `scope_req` → B1 授权层用（该工具需要哪种授权范围）。
//! - `audit_command` → 执行日志的 command 行构造；None = 不记执行日志
//!   （list_assets/list_sessions 不触发远程执行，旧行为如此）。
//! - `approval` → Strict 档审批弹窗的三段式文案（intent 默认值/命令行/后果），
//!   fn 形态以覆盖 recursive/覆盖检测等动态文案。
//!
//! ## 迁移纪律
//!
//! 本次迁移（A2）零行为变更：13 个工具的 name/description/schema 与迁移前
//! 逐字一致（scripts/mcp-tools-snapshot.mjs 快照比对为证）；各 Policy 分支的
//! 判定顺序（catastrophic 硬拦**先于**等级分支）原样保留。唯一有意变更：
//! ① 协议层补上 annotations（A3，纯增量）；② GUI tag 从第三份手写映射改为
//! risk 派生（Write/Destructive → "dangerous"，write 类工具在能力清单的
//! 展示从 readonly 修正为 dangerous——原标签低估了写操作）。

use std::future::Future;
use std::pin::Pin;

use rmcp::model::{CallToolResult, Tool, ToolAnnotations};
use serde_json::{Map, Value};

use super::approval::ElicitationInfo;
use super::tools::McpToolContext;

/// 工具 handler 的统一签名（借用 ctx/args，返回其生命周期内的 boxed future）。
pub type ToolFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CallToolResult, String>> + Send + 'a>>;
pub type ToolHandler =
    for<'a> fn(&'a McpToolContext, &'a Map<String, Value>) -> ToolFuture<'a>;

/// 风险等级：只用于协议 annotations 与 GUI 展示（决策辅助），不做运行时判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskClass {
    ReadOnly,
    Write,
    Destructive,
}

impl RiskClass {
    /// MCP 协议 annotations（A3）。映射表见 MCP服务设计-v3 §3.3：
    /// ReadOnly → readOnly+idempotent+openWorld；Write → 全 false + openWorld；
    /// Destructive → destructive。title 不填（host 用 name 即可）。
    pub fn annotations(self) -> ToolAnnotations {
        match self {
            Self::ReadOnly => {
                ToolAnnotations::from_raw(None, Some(true), None, Some(true), Some(true))
            }
            Self::Write => {
                ToolAnnotations::from_raw(None, Some(false), Some(false), Some(false), Some(true))
            }
            Self::Destructive => {
                ToolAnnotations::from_raw(None, Some(false), Some(true), Some(false), Some(true))
            }
        }
    }

    /// GUI 能力清单 tag（lib.rs McpToolInfo.tag）。
    /// Write 也算 dangerous：写/覆盖/下载落本机都改环境，标 readonly 是低估。
    pub fn gui_tag(self) -> &'static str {
        match self {
            Self::ReadOnly => "readonly",
            Self::Write | Self::Destructive => "dangerous",
        }
    }

    /// 执行日志 risk 字段值（A4）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "readonly",
            Self::Write => "write",
            Self::Destructive => "destructive",
        }
    }
}

/// 审批策略：server.rs 运行时判定的分支键（与 approval.rs 语义一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// 不进审批链（旧 match 返回 None）：纯本地查询 + 只读 exec/列举。
    NoApproval,
    /// ssh_exec：catastrophic 恒硬拦（先行）+ 白名单放行 + 其余按等级。
    ShellExec,
    /// sftp_write_file / sftp_upload：Minimal 放行记 minimal_allowed / Strict 审批。
    RemoteWrite,
    /// sftp_read_file：敏感凭据路径恒审批（凭据红线，不入等级体系），其余白名单放行。
    RemoteReadSensitive,
    /// sftp_remove：根级/核心目录恒 HardBlock（先于等级）；其余按等级。
    RemoteDelete,
    /// sftp_download：本机系统受保护目录恒 HardBlock（先于等级）；其余按等级。
    LocalPathWrite,
}

impl Policy {
    /// 执行日志 policy 字段值（A4）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoApproval => "no_approval",
            Self::ShellExec => "shell_exec",
            Self::RemoteWrite => "remote_write",
            Self::RemoteReadSensitive => "remote_read_sensitive",
            Self::RemoteDelete => "remote_delete",
            Self::LocalPathWrite => "local_path_write",
        }
    }
}

/// 该工具需要的授权范围（B1 scope 授权层消费）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeReq {
    /// 纯本地查询，不触任何资产。
    None,
    /// 需要目标资产访问权。
    Asset,
    /// 需要目标资产 + 本机文件系统触及权（upload 读本机 / download 写本机）。
    AssetAndLocalFs,
}

/// Strict 档审批弹窗的三段式文案（intent 默认/命令行/后果）。
/// 用 fn 而非静态串：recursive 标志、本机覆盖检测等动态文案由 fn 现算。
pub struct ApprovalSpec {
    pub command: fn(&Map<String, Value>) -> String,
    pub consequence: fn(&Map<String, Value>) -> String,
    /// intent 参数**缺失**时的默认文案（sftp_read_file 旧行为）。
    pub intent_default: Option<&'static str>,
}

/// 一个工具的完整声明。
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: fn() -> Map<String, Value>,
    pub risk: RiskClass,
    pub policy: Policy,
    /// B1 授权层消费（McpScope 判定时的授权需求声明）。B1 落地前暂未被读——
    /// #[allow(dead_code)] 是显式的「先声明后接线」，B1 提交时移除。
    #[allow(dead_code)]
    pub scope_req: ScopeReq,
    /// 执行日志 command 行构造；None = 不记执行日志。
    pub audit_command: Option<fn(&Map<String, Value>) -> String>,
    /// Strict 档审批文案；NoApproval / ShellExec（走 approval::evaluate）为 None。
    pub approval: Option<ApprovalSpec>,
    pub handler: ToolHandler,
    /// v0.20（C1）：长任务工具标 TaskSupport::Optional（rmcp 协议 tasks——
    /// host 支持时自动走原生任务流；不支持时走自建 job_* 轮询。两条路并存）。
    pub long_running: bool,
}

impl ToolSpec {
    /// 生成 rmcp 协议层 Tool（A3：附 annotations）。
    pub fn to_tool(&self) -> Tool {
        let mut tool = Tool::new(self.name, self.description, (self.schema)())
            .with_annotations(self.risk.annotations());
        if self.long_running {
            tool = tool.with_execution(
                rmcp::model::ToolExecution::new()
                    .with_task_support(rmcp::model::TaskSupport::Optional),
            );
        }
        tool
    }

    /// 构造 Strict 档审批的三段式信息（intent 缺失时用 intent_default——
    /// 对齐旧 sftp_read_file 的 unwrap_or 语义：缺省才用默认，显式空串不动）。
    pub fn approval_info(&self, args: &Map<String, Value>) -> ElicitationInfo {
        let spec = self
            .approval
            .as_ref()
            .expect("该 Policy 分支要求工具声明 approval 字段");
        let intent = args
            .get("intent")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| spec.intent_default.map(str::to_string))
            .unwrap_or_default();
        ElicitationInfo {
            intent,
            command: (spec.command)(args),
            consequence: (spec.consequence)(args),
        }
    }
}

// ─── 参数与文案的共用 helper（audit/approval 两组 fn 复用）───

pub(crate) fn arg_str(args: &Map<String, Value>, key: &str) -> String {
    args.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

pub(crate) fn arg_str_or(args: &Map<String, Value>, key: &str, default: &str) -> String {
    args.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}

pub(crate) fn arg_bool(args: &Map<String, Value>, key: &str) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

// v0.20（3-6 月段刀）：schema/文案/handler 三段拆出（注册表加行腾空间），
// 经 pub(crate) re-export 保持 REGISTRY 表内引用路径不变。
pub(crate) use super::registry_labels::*;
pub(crate) use super::registry_schemas::*;
use super::registry_handlers::*;
static REGISTRY: [ToolSpec; 26] = [
    ToolSpec {
        name: "list_assets",
        description: "列出所有已配置的 SSH 连接资产（不含密码/凭据，仅元数据：name/host/port/username/group/status）",
        schema: super::tools::empty_object_schema,
        risk: RiskClass::ReadOnly,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::None,
        audit_command: None,
        approval: None,
        handler: h_list_assets,
        long_running: false,
    },
    ToolSpec {
        name: "list_sessions",
        description: "列出当前客户端活跃的 SSH 会话清单（session_id 列表）。",
        schema: super::tools::empty_object_schema,
        risk: RiskClass::ReadOnly,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::None,
        audit_command: None,
        approval: None,
        handler: h_list_sessions,
        long_running: false,
    },
    ToolSpec {
        name: "disk_usage",
        description: "查询指定资产的磁盘使用情况（执行 df -h）。stdout 含 rc_df=N 行标注 df 的真实退出码（复合命令回声，勿只看首行 exit_code）",
        schema: super::tools::schema_with_required_session,
        risk: RiskClass::ReadOnly,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::Asset,
        audit_command: Some(|_| super::tools::CMD_DISK_USAGE.to_string()),
        approval: None,
        handler: h_disk_usage,
        long_running: false,
    },
    ToolSpec {
        name: "system_status",
        description: "查询指定资产的系统状态：uptime / 内存 / 负载 / top 进程。仅支持 Linux（依赖 /proc、procps 与 systemd）主机，其他平台命令会失败。stdout 含 rc_uptime/rc_free/rc_top=N 行标注各子段真实退出码（勿只看首行 exit_code，它只反映最后的 echo）；rc_top 由「先落临时文件再 head」取得，可信——top 不存在（slim 容器）时 rc_top≠0 且 stderr 带 not found",
        schema: super::tools::schema_with_required_session,
        risk: RiskClass::ReadOnly,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::Asset,
        audit_command: Some(|_| super::tools::CMD_SYSTEM_STATUS.to_string()),
        approval: None,
        handler: h_system_status,
        long_running: false,
    },
    ToolSpec {
        name: "service_status",
        description: "查询指定资产上某 systemd 服务的状态（systemctl status <service>）。仅支持 Linux（依赖 /proc 与 systemd）主机，其他平台命令会失败。stdout 含 rc_systemctl=N 行标注真实退出码",
        schema: schema_service_status,
        risk: RiskClass::ReadOnly,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::Asset,
        audit_command: Some(|args| {
            super::tools::service_status_command(&arg_str(args, "service"))
        }),
        approval: None,
        handler: h_service_status,
        long_running: false,
    },
    ToolSpec {
        name: "resource_monitor_snapshot",
        description: "获取指定资产的资源监控快照（CPU负载、内存占用、磁盘空间概览）。仅支持 Linux（依赖 /proc 与 systemd）主机，其他平台命令会失败。stdout 含 rc_uptime/rc_free/rc_df=N 行标注各子命令真实退出码",
        schema: super::tools::schema_with_required_session,
        risk: RiskClass::ReadOnly,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::Asset,
        audit_command: Some(|_| "snapshot".to_string()),
        approval: None,
        handler: h_resource_monitor_snapshot,
        long_running: false,
    },
    ToolSpec {
        name: "ssh_exec",
        description: "在指定资产上执行任意 Shell 命令。返回结构化文本：首行 exit_code=<n>（无退出码时 exit_code=unknown），随后为 stdout（无输出时给出提示），stderr 非空时以 --- stderr --- 分隔行附后，超长自动头尾截断。拦截语义：毁灭性命令（mkfs/dd 写块设备/rm 根级删除等）直接拒绝；其余按当前拦截等级放行或需确认（等级可在 myshelltool GUI 的 MCP 面板调整）。调用时必须如实声明 intent 意图。",
        schema: schema_ssh_exec,
        risk: RiskClass::Destructive,
        policy: Policy::ShellExec,
        scope_req: ScopeReq::Asset,
        audit_command: Some(audit_ssh_exec),
        approval: None, // ShellExec 的审批信息由 approval::evaluate 构造
        handler: h_ssh_exec,
        long_running: false,
    },
    ToolSpec {
        name: "exec_many",
        description: "在多台资产上执行同一 Shell 命令（批量 fan-out，单次上限 64 台，并发 8 台排队）。逐目标返回结构化结果（exitCode/output/truncated/sessionSource）；单目标失败不影响其余；范围外目标记 denied。审批按命令文本一次判定放行整批。调用时必须如实声明 intent 意图。",
        schema: schema_exec_many,
        risk: RiskClass::Destructive,
        policy: Policy::ShellExec,
        scope_req: ScopeReq::Asset,
        audit_command: Some(|args| {
            let n = args.get("asset_ids").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            format!("exec_many[{n} targets]: {}", arg_str(args, "command"))
        }),
        approval: None, // ShellExec：审批信息由 approval::evaluate 构造（与 ssh_exec 同源）
        handler: h_exec_many,
        long_running: false,
    },
    ToolSpec {
        name: "ssh_exec_async",
        description: "在指定资产上后台执行长时 Shell 命令（apt upgrade / mysqldump / rsync 等超过工具超时的命令），立即返回 job_id。用 job_status 轮询状态、job_output 分页读输出、job_cancel 取消（断开连接，远端命令收 SIGHUP）。job TTL 30 分钟，内存态不落盘。调用时必须如实声明 intent 意图。",
        schema: schema_ssh_exec_async,
        risk: RiskClass::Destructive,
        policy: Policy::ShellExec,
        scope_req: ScopeReq::Asset,
        audit_command: Some(|args| format!("ssh_exec_async: {}", arg_str(args, "command"))),
        approval: None,
        handler: h_ssh_exec_async,
        long_running: true,
    },
    ToolSpec {
        name: "job_status",
        description: "查询长任务状态（running/done/failed/cancelled + 退出码 + 输出字节数）。",
        schema: schema_job_ref,
        risk: RiskClass::ReadOnly,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::None,
        audit_command: None,
        approval: None,
        handler: h_job_status,
        long_running: false,
    },
    ToolSpec {
        name: "job_output",
        description: "分页读取长任务输出（offset/limit，单页上限 1MiB；响应带总字节数与 EOF 标记）。",
        schema: schema_job_output,
        risk: RiskClass::ReadOnly,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::None,
        audit_command: None,
        approval: None,
        handler: h_job_output,
        long_running: false,
    },
    ToolSpec {
        name: "job_cancel",
        description: "取消长任务（断开其连接，远端命令将收 SIGHUP）。状态由执行侧收敛为 cancelled，用 job_status 确认。",
        schema: schema_job_ref,
        risk: RiskClass::Write,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::None,
        audit_command: None,
        approval: None,
        handler: h_job_cancel,
        long_running: false,
    },
    ToolSpec {
        name: "journal_query",
        description: "查询指定资产的 systemd 日志（journalctl 结构化：unit/since/until/grep/limit 参数固化在服务端，跨发行版口径统一——--no-pager、-o short-iso、env LC_ALL=C；非 systemd 系统返回 127 与降级指引）。仅支持 systemd 发行版。",
        schema: super::registry_schemas::schema_journal_query,
        risk: RiskClass::ReadOnly,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::Asset,
        audit_command: Some(|args| format!("journalctl unit={} since={}", arg_str(args, "unit"), arg_str(args, "since"))),
        approval: None,
        handler: h_journal_query,
        long_running: false,
    },
    ToolSpec {
        name: "port_listen",
        description: "查询指定资产的监听端口（ss → netstat → lsof 三级降级链固化在服务端；输出首行标注实际使用的工具；env LC_ALL=C 锁 locale；无任何可用工具时返回 127）。",
        schema: super::registry_schemas::schema_port_listen,
        risk: RiskClass::ReadOnly,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::Asset,
        audit_command: Some(|args| format!("port_listen tcp_only={}", args.get("tcpOnly").and_then(|v| v.as_bool()).unwrap_or(false))),
        approval: None,
        handler: h_port_listen,
        long_running: false,
    },
    ToolSpec {
        name: "process_list",
        description: "查询指定资产的进程列表（ps POSIX 形态：pid/ppid/user/%cpu/%mem/rss/stat/etime/comm；--no-headers 是 procps 扩展，BSD 自动降级带表头形态——子 shell 包裹保证两条路径都进排序管道；POSIX sort -k -nr 数值排序 + head 截前 N；sortBy=cpu|mem 枚举白名单）。",
        schema: super::registry_schemas::schema_process_list,
        risk: RiskClass::ReadOnly,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::Asset,
        audit_command: Some(|args| format!("process_list sort={} limit={}", arg_str(args, "sortBy"), args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20))),
        approval: None,
        handler: h_process_list,
        long_running: false,
    },
    ToolSpec {
        name: "file_search",
        description: "按条件查找远端文件（find POSIX 形态：name 通配/minSizeMb/mtimeDays 过滤；**不用 GNU 专有格式化输出选项**（BusyBox/BSD 事故源，fact-guards 同类规则）；结果上限服务端拼进命令（默认 100 最大 1000，防深目录炸上下文）；需要元数据时对少量结果逐个 sftp_stat）。",
        schema: super::registry_schemas::schema_file_search,
        risk: RiskClass::ReadOnly,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::Asset,
        audit_command: Some(|args| format!("find {} name={}", arg_str(args, "path"), arg_str(args, "name"))),
        approval: None,
        handler: h_file_search,
        long_running: false,
    },
    ToolSpec {
        name: "service_control",
        description: "控制指定资产上的 systemd 服务生命周期（start/stop/restart/reload；action 枚举白名单；执行后自动补 is-active 确认最终态）。此操作改变服务运行状态，按拦截等级判定：Minimal 直接执行记日志 / Strict 弹窗确认。",
        schema: super::registry_schemas::schema_service_control,
        risk: RiskClass::Write,
        policy: Policy::RemoteWrite,
        scope_req: ScopeReq::Asset,
        audit_command: Some(|args| format!("systemctl {} {}", arg_str(args, "action"), arg_str(args, "service"))),
        approval: Some(ApprovalSpec {
            command: |args| format!("systemctl {} {}", args.get("action").and_then(|v| v.as_str()).unwrap_or(""), args.get("service").and_then(|v| v.as_str()).unwrap_or("")),
            consequence: |args| format!("将{}服务 {}。服务状态将改变，可能影响业务。", args.get("service").and_then(|v| v.as_str()).unwrap_or("该"), match args.get("action").and_then(|v| v.as_str()).unwrap_or("") { "stop" => "停止", "restart" => "重启", "reload" => "重载配置", _ => "启动" }),
            intent_default: None,
        }),
        handler: h_service_control,
        long_running: false,
    },
    ToolSpec {
        name: "tunnel_list",
        description: "列出当前隧道的清单与状态（kind: local/remote/dynamic；active/error 字段呈现异步失败）。",
        schema: super::registry_schemas::schema_tunnel_list,
        risk: RiskClass::ReadOnly,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::None,
        audit_command: None,
        approval: None,
        handler: h_tunnel_list,
        long_running: false,
    },
    ToolSpec {
        name: "tunnel_create",
        description: "建立 remote 端口转发隧道（走资产专用连接，复用 GUI 同一路径；MVP 仅 remote——local/dynamic 需活跃会话请在 GUI 建）。建立端口转发 = 网络暴露面变更，Strict 档需确认。",
        schema: super::registry_schemas::schema_tunnel_create,
        risk: RiskClass::Write,
        policy: Policy::RemoteWrite,
        scope_req: ScopeReq::Asset,
        audit_command: Some(|args| format!("tunnel {}:{}->{}", args.get("bindAddr").and_then(|v| v.as_str()).unwrap_or("127.0.0.1"), args.get("localPort").and_then(|v| v.as_u64()).unwrap_or(0), args.get("remoteAddr").and_then(|v| v.as_str()).unwrap_or(""))),
        approval: Some(ApprovalSpec {
            command: |args| format!("remote forward {}:{} -> {}:{}", args.get("bindAddr").and_then(|v| v.as_str()).unwrap_or("127.0.0.1"), args.get("localPort").and_then(|v| v.as_u64()).unwrap_or(0), args.get("remoteAddr").and_then(|v| v.as_str()).unwrap_or(""), args.get("remotePort").and_then(|v| v.as_u64()).unwrap_or(0)),
            consequence: |_| "将让远端服务器在指定端口监听并转发到目标地址（网络暴露面变更）。若绑定非回环地址则外网可访问。".to_string(),
            intent_default: None,
        }),
        handler: h_tunnel_create,
        long_running: false,
    },
    ToolSpec {
        name: "sftp_list",
        description: "列出指定资产远程目录中的文件与子目录。返回 JSON 结构列表，包含名称、路径、类型（file/directory/symlink）、字节大小、修改时间与权限。",
        schema: schema_sftp_list,
        risk: RiskClass::ReadOnly,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::Asset,
        audit_command: Some(audit_sftp_list),
        approval: None,
        handler: h_sftp_list,
        long_running: false,
    },
    ToolSpec {
        name: "sftp_read_file",
        description: "读取指定资产上的远程文本文件（v0.20 起支持分页：默认单次 256KB，超限不再报错，响应带 nextOffset 续读；单次上限可传 maxBytes 最高 4MB）。适合配置排查与代码审计（敏感路径如 /etc/shadow、SSH 私钥需在客户端确认；二进制文件请改用 sftp_download）。",
        schema: schema_sftp_read_file,
        risk: RiskClass::ReadOnly,
        policy: Policy::RemoteReadSensitive,
        scope_req: ScopeReq::Asset,
        audit_command: Some(audit_sftp_read_file),
        approval: Some(ApprovalSpec {
            command: approval_cmd_sftp_read_file,
            consequence: approval_consequence_sftp_read_file,
            intent_default: Some("读取敏感系统凭据/配置"),
        }),
        handler: h_sftp_read_file,
        long_running: false,
    },
    ToolSpec {
        name: "read_output",
        description: "分页取回 ssh_exec 被截断的完整输出（v0.20/B2）：截断提示里给出的 cursor 在 10 分钟内可按 offset/limit 分段取回原文。只读缓存，不读文件。",
        schema: schema_read_output,
        risk: RiskClass::ReadOnly,
        policy: Policy::NoApproval,
        scope_req: ScopeReq::None,
        audit_command: None,
        approval: None,
        handler: h_read_output,
        long_running: false,
    },
    ToolSpec {
        name: "sftp_write_file",
        description: "向指定资产写入远程文本文件。采用原子临时文件替换机制，单次上限 1MB。此操作涉及远程文件创建或覆盖，按拦截等级判定：Minimal 档直接执行并记执行日志，Strict 档需在客户端确认。调用时必须如实声明 intent 意图。",
        schema: schema_sftp_write_file,
        risk: RiskClass::Write,
        policy: Policy::RemoteWrite,
        scope_req: ScopeReq::Asset,
        audit_command: Some(audit_sftp_write_file),
        approval: Some(ApprovalSpec {
            command: approval_cmd_sftp_write_file,
            consequence: approval_consequence_sftp_write_file,
            intent_default: None,
        }),
        handler: h_sftp_write_file,
        long_running: false,
    },
    ToolSpec {
        name: "sftp_upload",
        description: "将本机文件流式上传至远程服务器。采用分块传输、临时文件落地原子替换与 SHA256 完整性校验。适用于大文件与二进制包传输。此操作涉及远程文件覆盖，按拦截等级判定：Minimal 档直接执行并记执行日志，Strict 档需在客户端确认。调用时必须声明 intent 意图。",
        schema: schema_sftp_upload,
        risk: RiskClass::Write,
        policy: Policy::RemoteWrite,
        scope_req: ScopeReq::AssetAndLocalFs,
        audit_command: Some(audit_sftp_upload),
        approval: Some(ApprovalSpec {
            command: approval_cmd_sftp_upload,
            consequence: approval_consequence_sftp_upload,
            intent_default: None,
        }),
        handler: h_sftp_upload,
        long_running: false,
    },
    ToolSpec {
        name: "sftp_download",
        description: "将远程服务器上的文件流式下载至本机。采用分块传输与 SHA256 完整性校验。本机系统受保护核心目录恒拒；其余路径按拦截等级判定：Minimal 档直接执行并记执行日志，Strict 档需在客户端确认。调用时必须声明 intent 意图。",
        schema: schema_sftp_download,
        risk: RiskClass::Write,
        policy: Policy::LocalPathWrite,
        scope_req: ScopeReq::AssetAndLocalFs,
        audit_command: Some(audit_sftp_download),
        approval: Some(ApprovalSpec {
            command: approval_cmd_sftp_download,
            consequence: approval_consequence_sftp_download,
            intent_default: None,
        }),
        handler: h_sftp_download,
        long_running: false,
    },
    ToolSpec {
        name: "sftp_remove",
        description: "删除指定资产上的远程文件或目录。高危破坏性操作：系统根目录与顶级核心目录删除恒拒；其余路径按拦截等级判定——Minimal 档直接执行并记执行日志，Strict 档需在客户端确认。若删除非空目录，必须显式声明 recursive=true。调用时必须如实声明 intent 意图。",
        schema: schema_sftp_remove,
        risk: RiskClass::Destructive,
        policy: Policy::RemoteDelete,
        scope_req: ScopeReq::Asset,
        audit_command: Some(audit_sftp_remove),
        approval: Some(ApprovalSpec {
            command: approval_cmd_sftp_remove,
            consequence: approval_consequence_sftp_remove,
            intent_default: None,
        }),
        handler: h_sftp_remove,
        long_running: false,
    },
];

/// 全部工具 spec（顺序即 tools/list 顺序）。
pub fn registry() -> &'static [ToolSpec] {
    &REGISTRY
}

/// 按名查找。
pub fn find(name: &str) -> Option<&'static ToolSpec> {
    REGISTRY.iter().find(|s| s.name == name)
}

/// 全部工具的协议层定义（A3：带 annotations）。
pub fn all_tools() -> Vec<Tool> {
    registry().iter().map(ToolSpec::to_tool).collect()
}
