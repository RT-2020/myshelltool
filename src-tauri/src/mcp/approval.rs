//! MCP 工具调用审批（Layer 6）。
//!
//! v1.0：进程内拒绝（黑名单/未知直接 isError）。
//! v1.1：高危命令走 MCP elicitation（RequestElicitation），在客户端界面内
//!   弹确认框（三段式），用户 accept 才执行。客户端不支持 elicitation 时，
//!   降级逻辑由 server.rs 的 NotSupported 分支处理（返回错误结果）。
//! v1.5：elicitation 仍是主路径，但客户端不支持时（如 ZCode）改为 GUI 弹窗
//!   降级（方案 A）——若 McpToolContext 持有 AppHandle，则 emit 审批事件给
//!   前端 GlobalModals 弹窗，用户在 GUI 内确认。无 GUI（headless/测试）才
//!   退回 fail-secure 拒绝。这是 MCP 官方博客定位的 deterministic runtime
//!   control：安全保证落在 runtime 层，不依赖 elicitation（UX hint）。
//! v2：拦截等级用户可配置（config.rs::McpInterceptLevel）。默认 Minimal
//!   （仅黑名单/超高危需确认，Unknown/黄名单放行）——这是**用户明确选择的
//!   低摩擦默认**，放行记入执行日志（minimal_allowed）供事后审计，产品决策
//!   上取代了 v1.x「未知命令 fail-secure 默认拒」。Strict 档保留原 fail-secure
//!   语义（非白名单一律确认）。
//! v2.1：等级语义重构——**Minimal = 零审批交互**：仅「机器报废级」毁灭性
//!   命令（catastrophic 层，见 dangerous_commands.rs）HardBlock 直接拒绝
//!   （不弹窗不等超时，修复 ZCode 等不支持 elicitation 的 host 下白等 60s），
//!   其余（含**非毁灭黑名单**：rm -rf 子路径 / shutdown / reboot / ...）直接
//!   放行记 minimal_allowed 日志。**口径变化：黑名单在 Minimal 下不再恒拦**
//!   ——毁灭性判定上移到 catastrophic 层，且必须**先于** classify_command
//!   执行（顺序是安全前提，见 dangerous_commands.rs 模块注释）。Strict 档
//!   保留「非白名单一律审批」，毁灭性命令同样 HardBlock。
//!
//! 审批分层（D9 + v2.1 等级）：
//! - 毁灭性（catastrophic）→ HardBlock（两档同，不进审批链）
//! - 白名单 → AutoExecute（两档同）
//! - 黄名单（恒空）/ 未知 / 非毁灭黑名单 → Minimal 放行 / Strict 审批

use serde::Serialize;

use crate::dangerous_commands::{self, CommandRisk, DangerousMatch};

use super::config::McpInterceptLevel;
// ApprovalPending 在 tools.rs 定义（Arc<Mutex<HashMap<id, oneshot::Sender<bool>>>> 别名）。
use super::tools::ApprovalPending;

/// 审批决策结果。
pub enum ApprovalDecision {
    /// 自动执行（白名单命中，或 Minimal 档对未知/黄名单/非毁灭黑名单的放行）。
    /// 携带原因：server.rs 执行日志据此区分 auto_approved / minimal_allowed。
    AutoExecute(AutoApproveReason),
    /// 毁灭性（catastrophic）命令硬拦（v2.1）：两档等级一致直接拒绝，
    /// **不进审批链**（不发 elicitation / 不弹 GUI 窗，避免不支持
    /// elicitation 的 host 下白等 60s 超时）。reason 是给 MCP 主代理看的
    /// 完整拒绝文本（server.rs 直接作为 error_result 返回）。
    HardBlock { reason: String },
    /// 需要用户确认（v1.5 三级降级：elicitation → GUI 弹窗 → fail-secure 拒）。
    RequestElicitation(ElicitationInfo),
}

/// 自动放行原因（v2 执行日志 decision 字段的判定依据）。
pub enum AutoApproveReason {
    /// 命中只读白名单（两档均放行）→ 日志 decision=auto_approved。
    Whitelist,
    /// Minimal 档对未知/黄名单及**非毁灭黑名单**命令的放行（v2.1 口径）
    /// → 日志 decision=minimal_allowed。
    MinimalFallback,
}

/// elicitation 请求信息（三段式：AI意图 + 真实命令 + 后果预测）。
#[derive(Debug, Clone)]
pub struct ElicitationInfo {
    /// AI 声明的意图（来自工具调用的 intent 参数）。
    pub intent: String,
    /// 真实要执行的命令。
    pub command: String,
    /// 后果预测（基于命中的危险模式）。
    pub consequence: String,
}

impl ElicitationInfo {
    /// 格式化为 elicitation 的 message 文本（用户看到的确认框内容）。
    pub fn to_message(&self) -> String {
        let intent_display = if self.intent.is_empty() {
            "(AI 未声明意图)"
        } else {
            &self.intent
        };
        format!(
            "⚠️ 高危操作审批\n\n\
             【AI 声明意图】{}\n\n\
             【真实命令】{}\n\n\
             【后果预测】{}\n\n\
             确认要执行此操作吗？",
            intent_display, self.command, self.consequence
        )
    }

/// 降级用的拒绝文本（GUI 不可用时 server.rs NotSupported 分支调用）。
///
/// fail-secure 拒绝：headless/emit 失败等 GUI 审批通道不可用的场景无法完成
/// 高危操作的安全审批，直接拒绝。只读命令不受影响（走白名单自动放行）。
/// v2 说明：等级配置（Minimal/Strict）不影响本路径——Minimal 是对「未知命令
/// 是否需要审批」的产品决策（用户明确选择的低摩擦默认），而这里是「审批已
/// 触发但无通道可用」的安全兜底，两档行为一致。
pub fn to_rejection(&self) -> String {
    format_rejection(&self.intent, &self.command, &self.consequence)
}
}

/// 评估一条命令的审批决策。
///
/// `intent` 是 AI 声明的意图（来自工具调用的 intent 参数），
/// `command` 是真实要执行的命令。
/// `whitelist` / `yellow_list` 见 D9 决策（白名单内置，黄名单按资产配置）。
/// `level` 是当前拦截等级（server.rs 每次 call_tool 现读共享配置快照）。
///
/// 等级映射（v2.1，详见模块头注释）：
/// - 毁灭性（catastrophic）→ HardBlock，两档同（**必须先于 classify 判定**，
///   顺序是安全前提——classify 的黑名单对 rm 分离/逆序形态有 MISS）。
/// - Safe → AutoExecute(Whitelist)，两档同。
/// - Dangerous（非毁灭黑名单）→ Minimal 放行 / Strict 审批。
/// - Allowed（黄名单，恒空）/ Unknown → Minimal 放行 / Strict 审批。
pub fn evaluate(
    command: &str,
    intent: &str,
    whitelist: &[&str],
    yellow_list: &[String],
    level: McpInterceptLevel,
) -> ApprovalDecision {
    // 1. 毁灭性命令先行硬拦（两档一致，level 不影响此路径；不进审批链）。
    if let Some(m) = dangerous_commands::detect_catastrophic_command(command) {
        log::warn!(
            "approval: catastrophic command hard-blocked (pattern={})",
            m.pattern
        );
        return ApprovalDecision::HardBlock {
            reason: format_hard_block_rejection(command, &predict_consequence(&m)),
        };
    }

    let wl: Vec<String> = whitelist.iter().map(|s| s.to_string()).collect();
    match dangerous_commands::classify_command(command, &wl, yellow_list) {
        CommandRisk::Safe => {
            log::info!("approval: command approved (whitelist)"); // fact-guard:allow no-unredacted-command-in-log 本条只记白名单放行事实，不含命令文本
            ApprovalDecision::AutoExecute(AutoApproveReason::Whitelist)
        }
        CommandRisk::Allowed | CommandRisk::Unknown => match level {
            McpInterceptLevel::Minimal => {
                // v2 用户明确选择的低摩擦默认：放行 + 执行日志留痕（minimal_allowed）。
                // 命令文本先脱敏再进应用日志（§8 凭据红线；`mysql -pP@ss` 明文落盘事故）。
                log::info!(
                    "approval: command allowed under minimal level (yellow/unknown): {:?}",
                    myshelltool_core::redact_command(command)
                );
                ApprovalDecision::AutoExecute(AutoApproveReason::MinimalFallback)
            }
            McpInterceptLevel::Strict => {
                log::warn!("approval: unknown/yellow command, requesting elicitation under strict level (user decides)"); // fact-guard:allow no-unredacted-command-in-log 本条只记转人工确认事实，不含命令文本
                ApprovalDecision::RequestElicitation(ElicitationInfo {
                    intent: intent.to_string(),
                    command: command.to_string(),
                    consequence: "此命令不在已知安全名单内，需要用户确认。".to_string(),
                })
            }
        },
        // v2.1：非毁灭黑名单（rm -rf 子路径 / shutdown / reboot / chown -R /
        // curl|bash 等）——Minimal 放行记 minimal_allowed（零审批交互），
        // Strict 仍走审批链（沿用原 consequence 文案）。
        CommandRisk::Dangerous(m) => match level {
            McpInterceptLevel::Minimal => {
                log::warn!(
                    "approval: dangerous (non-catastrophic) command allowed under minimal level: {:?}",
                    myshelltool_core::redact_command(command)
                );
                ApprovalDecision::AutoExecute(AutoApproveReason::MinimalFallback)
            }
            McpInterceptLevel::Strict => {
                log::warn!(
                    "approval: dangerous command, requesting elicitation under strict level (pattern={})",
                    m.pattern
                );
                ApprovalDecision::RequestElicitation(ElicitationInfo {
                    intent: intent.to_string(),
                    command: command.to_string(),
                    consequence: predict_consequence(&m),
                })
            }
        },
    }
}

/// 三段式确认信息格式（D5+D9：AI意图 + 真实命令 + 后果预测）。
///
/// 用于审批降级链全部不可用时（headless 无 GUI / emit 失败）的拒绝路径。
/// 语义为 fail-secure 拒绝：无法完成高危操作的安全确认，直接拒绝（两档
/// 等级下行为一致——Minimal 是「未知命令是否需审批」的产品决策，这里是
/// 「审批无通道可用」的安全兜底）。即使用户在 intent 里声称「查看日志」，
/// 但 command 是 `rm -rf /var/log`，三段对照也能让用户（或读 error 的 LLM）
/// 识破伪装。若确需执行，用户应在支持 elicitation 的客户端（Claude Desktop）
/// 中操作，或在 myshelltool GUI 手动执行。
fn format_rejection(intent: &str, command: &str, consequence: &str) -> String {
    format!(
        "【高危操作已被拒绝】当前客户端不支持 MCP elicitation 确认框，无法完成安全审批。\n\n\
         【AI 声明意图】{}\n\n\
         【真实命令】{}\n\n\
         【后果预测】{}\n\n\
         如确需执行此高危操作，请改用支持 elicitation 的客户端（如 Claude Desktop），\
         或在 myshelltool GUI 中手动执行。只读命令（df/uptime/systemctl status 等）不受此限制。",
        if intent.is_empty() { "(AI 未声明意图)" } else { intent },
        command,
        consequence,
    )
}

/// 毁灭性命令的硬拦拒绝文本（v2.1，给 MCP 主代理看，server.rs 直接返回）。
///
/// 与 format_rejection 的区别：毁灭性操作**不提供审批通道**（elicitation /
/// GUI 弹窗都不给——机器报废级后果不值得占用一次人工确认，且避免不支持
/// elicitation 的 host 下白等 60s 超时），两档拦截等级行为一致。
fn format_hard_block_rejection(command: &str, consequence: &str) -> String {
    format!(
        "【命令已被拦截】毁灭性命令，未执行。\n\n\
         【真实命令】{}\n\n\
         【后果预测】{}\n\n\
         说明：毁灭性操作在任何拦截等级下均直接拒绝，不提供审批通道；\
         检索/查看含此类关键词的文档或日志的命令同样会被拦截。\n\n\
         如确需执行，请在 myshelltool GUI 终端手动执行；\
         被误拦的检索类命令可改由 AI 读取文件内容（cat / grep 等）自行判断。",
        command, consequence,
    )
}

/// 基于命中的危险模式给出固定后果预测文案。
///
/// 对应 dangerous_commands.rs 的 16 条正则 + catastrophic 层的 rm 根级
/// 正则，每类给出人话后果说明，帮助用户判断是否真的要执行。
fn predict_consequence(m: &DangerousMatch) -> String {
    let p = m.pattern.as_str();
    // 按正则特征匹配文案（顺序对应 dangerous_commands.rs 的 raw 数组）
    if p.contains("\\brm") {
        // rm 根级删除（catastrophic 专属正则，见 dangerous_commands.rs RM_ROOT_SRC）
        "将递归强制删除文件系统根/家/当前目录，系统即刻报废，数据不可恢复。".to_string()
    } else if p.contains("rm\\s+") {
        "将递归强制删除文件或目录，且不可恢复。".to_string()
    } else if p.contains("mkfs") {
        "将格式化文件系统，磁盘上所有数据将被彻底销毁。".to_string()
    } else if p.contains("dd") && p.contains("/dev/") {
        "将向块设备直接写入数据，可能永久破坏磁盘分区与数据。".to_string()
    } else if p.contains("\\(\\)") || p.contains(":\\(\\)") {
        "Fork 炸弹：将瞬间耗尽系统进程资源，导致系统完全无响应。".to_string()
    } else if p.contains("/dev/sd") {
        "将数据重定向写入块设备，可能覆盖并破坏磁盘。".to_string()
    } else if p.contains("shutdown") {
        "将关闭服务器。".to_string()
    } else if p.contains("reboot") {
        "将重启服务器，正在运行的进程会被中断。".to_string()
    } else if p.contains("halt") {
        "将停机（halt）。".to_string()
    } else if p.contains("poweroff") {
        "将关机（poweroff）。".to_string()
    } else if p.contains("init\\s+0") {
        "将切换到运行级别 0（关机）。".to_string()
    } else if p.contains("chmod") {
        "将递归修改文件/目录权限，可能导致系统服务因权限错误而无法启动。".to_string()
    } else if p.contains("chown") {
        "将递归修改文件/目录属主，可能导致服务因属主错误而异常。".to_string()
    } else if p.contains("iptables") {
        "将清空防火墙规则（iptables -F），可能暴露服务端口或切断现有连接。".to_string()
    } else if p.contains("curl") || p.contains("wget") {
        "将从网络下载脚本并直接用 shell 执行，存在执行恶意代码的高风险。".to_string()
    } else {
        "此命令被判定为危险操作。".to_string()
    }
}

/// GUI 审批事件 payload（emit 给前端 GlobalModals 弹窗）。
///
/// v1.5：当客户端不支持 elicitation 但 GUI 在线时，server.rs 用此结构 emit
/// `mcp-tool-approval` 事件，前端 mcp.js store 监听后弹 modal。字段对应
/// ElicitationInfo 三段式，前端直接渲染。
#[derive(Debug, Clone, Serialize)]
pub struct McpApprovalEvent {
    /// 审批请求 id（前端回传 mcp_confirm_tool 时用，对应 pending 表 key）。
    pub request_id: String,
    /// AI 声明意图（来自工具调用 intent 参数）。
    pub intent: String,
    /// 真实要执行的命令。
    pub command: String,
    /// 后果预测（基于命中的危险模式）。
    pub consequence: String,
}

/// 命令层 resolve：由 mcp_confirm_tool 命令调用，从 pending 表取出 sender
/// 回传用户决定。
///
/// server.rs 在 emit 事件后注册 oneshot::Sender 到 pending 表并等待；
/// 前端用户点确认/拒绝后调 mcp_confirm_tool → 本函数取 sender send。
/// request_id 不存在（已被超时清理或前端重复点击）→ 返回 Err。
///
/// 模式照 ssh.rs:1003-1014 ssh_confirm_host_key。
pub async fn resolve_approval(
    pending: &ApprovalPending,
    request_id: &str,
    accepted: bool,
) -> Result<(), String> {
    let mut map = pending.lock().await;
    match map.remove(request_id) {
        Some(tx) => {
            let _ = tx.send(accepted);
            Ok(())
        }
        None => Err(format!(
            "mcp_confirm_tool: request_id {request_id} 不存在（可能已超时或不存在）"
        )),
    }
}

/// M3 只读工具内置的白名单命令前缀（D9）。
///
/// 这些是 disk_usage/system_status/service_status 等只读工具实际执行
/// 的命令前缀。ssh_exec 工具不应使用此白名单——ssh_exec 的命令由 AI
/// 提供，必须经过 evaluate() 完整审批。
pub const READONLY_WHITELIST: &[&str] = &[
    "df",
    "uptime",
    "free",
    "top",
    "systemctl status",
    "cat /proc",
    "cat /etc/os-release",
    "uname",
    "who",
    "w",
    "last",
    "netstat",
    "ss",
    "docker ps",
    "docker logs",
    "journalctl",
    "ps",
    "ls",
    "find",
    "head",
    "tail",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_command_auto_approved_under_both_levels() {
        for level in [McpInterceptLevel::Minimal, McpInterceptLevel::Strict] {
            let d = evaluate("df -h", "查询磁盘", READONLY_WHITELIST, &[], level); // fact-guard:allow locale-pinned-df 单测样例数据（非实际执行）
            match d {
                ApprovalDecision::AutoExecute(reason) => {
                    // 白名单命中两档均放行，且 reason 是 Whitelist（→ auto_approved）
                    assert!(matches!(reason, AutoApproveReason::Whitelist));
                }
                _ => panic!("expected AutoExecute for level={level:?}"),
            }
        }
    }

    #[test]
    fn noncatastrophic_dangerous_minimal_allows_strict_elicits() {
        // v2.1：非毁灭黑名单（rm -rf 子路径）Minimal 放行（MinimalFallback → 日志 minimal_allowed）
        let d = evaluate("rm -rf /var/log", "清理日志", READONLY_WHITELIST, &[], McpInterceptLevel::Minimal);
        assert!(matches!(
            d,
            ApprovalDecision::AutoExecute(AutoApproveReason::MinimalFallback)
        ));

        // Strict：仍走审批链（三段式信息完整）
        let d = evaluate("rm -rf /var/log", "清理日志", READONLY_WHITELIST, &[], McpInterceptLevel::Strict);
        match d {
            ApprovalDecision::RequestElicitation(info) => {
                // 三段式信息完整
                assert_eq!(info.intent, "清理日志");
                assert_eq!(info.command, "rm -rf /var/log");
                assert!(info.consequence.contains("递归"));
                // to_message 包含三段
                let msg = info.to_message();
                assert!(msg.contains("【AI 声明意图】清理日志"));
                assert!(msg.contains("【真实命令】rm -rf /var/log"));
                assert!(msg.contains("【后果预测】"));
            }
            _ => panic!("expected RequestElicitation under strict"),
        }
    }

    #[test]
    fn catastrophic_commands_hard_blocked_under_both_levels() {
        // rm -rf / + mkfs：毁灭性命令两档均 HardBlock（不进审批链）
        for cmd in ["rm -rf /", "mkfs.ext4 /dev/sda1"] {
            for level in [McpInterceptLevel::Minimal, McpInterceptLevel::Strict] {
                let d = evaluate(cmd, "测试", &[], &[], level);
                match d {
                    ApprovalDecision::HardBlock { reason } => {
                        assert!(reason.contains("拦截"), "cmd={cmd}");
                        assert!(reason.contains(cmd), "cmd={cmd}");
                    }
                    _ => panic!("expected HardBlock for cmd={cmd} level={level:?}"),
                }
            }
        }
    }

    #[test]
    fn rm_root_split_and_reversed_flags_hard_blocked() {
        // 防回归：rm -fr /、rm -r -f / 是现行黑名单 pattern 1 的 MISS 活洞
        // （要求 r/f 在同一短选项 token 内），两档均必须 HardBlock。
        for cmd in ["rm -fr /", "rm -r -f /"] {
            for level in [McpInterceptLevel::Minimal, McpInterceptLevel::Strict] {
                let d = evaluate(cmd, "清理", &[], &[], level);
                assert!(
                    matches!(d, ApprovalDecision::HardBlock { .. }),
                    "expected HardBlock for cmd={cmd} level={level:?}"
                );
            }
        }
    }

    #[test]
    fn reboot_allowed_under_minimal() {
        // v2.1：非毁灭黑名单在 Minimal 下零审批交互放行
        let d = evaluate("reboot", "重启", &[], &[], McpInterceptLevel::Minimal);
        assert!(matches!(
            d,
            ApprovalDecision::AutoExecute(AutoApproveReason::MinimalFallback)
        ));
        // Strict 下仍审批（原 fail-secure 语义保留）
        let d = evaluate("reboot", "重启", &[], &[], McpInterceptLevel::Strict);
        assert!(matches!(d, ApprovalDecision::RequestElicitation(_)));
    }

    #[test]
    fn unknown_command_minimal_allows_strict_elicits() {
        // minimal：未知命令放行（MinimalFallback → 日志 minimal_allowed）
        let d = evaluate("echo hello", "", READONLY_WHITELIST, &[], McpInterceptLevel::Minimal);
        match d {
            ApprovalDecision::AutoExecute(reason) => {
                assert!(matches!(reason, AutoApproveReason::MinimalFallback));
            }
            _ => panic!("expected AutoExecute under minimal"),
        }

        // strict：未知命令审批（fail-secure 语义保留）
        let d = evaluate("echo hello", "", READONLY_WHITELIST, &[], McpInterceptLevel::Strict);
        match d {
            ApprovalDecision::RequestElicitation(info) => {
                assert!(info.intent.is_empty());
                let msg = info.to_message();
                assert!(msg.contains("AI 未声明意图"));
            }
            _ => panic!("expected RequestElicitation under strict"),
        }
    }

    #[test]
    fn yellow_list_minimal_allows_strict_elicits() {
        let yellow = vec!["nginx -t".to_string()];
        // minimal：黄名单放行（MinimalFallback）
        let d = evaluate("nginx -t", "测试配置", &[], &yellow, McpInterceptLevel::Minimal);
        assert!(matches!(
            d,
            ApprovalDecision::AutoExecute(AutoApproveReason::MinimalFallback)
        ));
        // strict：黄名单同样审批（黄名单当前恒空，语义按 v2 映射）
        let d = evaluate("nginx -t", "测试配置", &[], &yellow, McpInterceptLevel::Strict);
        assert!(matches!(d, ApprovalDecision::RequestElicitation(_)));
    }

    #[test]
    fn mkfs_hard_block_reason_mentions_consequence() {
        // v2.1：mkfs 属毁灭层，两档均 HardBlock（不再走 elicitation）
        for level in [McpInterceptLevel::Minimal, McpInterceptLevel::Strict] {
            let d = evaluate("mkfs.ext4 /dev/sda1", "格式化", &[], &[], level);
            match d {
                ApprovalDecision::HardBlock { reason } => {
                    assert!(reason.contains("格式化"));
                    assert!(reason.contains("数据"));
                }
                _ => panic!("expected HardBlock for level={level:?}"),
            }
        }
    }
}
