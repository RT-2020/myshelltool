//! MCP 工具调用审批（Layer 6）。
//!
//! v1.0：进程内拒绝（Reject），黑名单/未知直接 isError。
//! v1.1：高危命令走 MCP elicitation（RequestElicitation），在客户端界面内
//!   弹确认框（三段式），用户 accept 才执行。客户端不支持 elicitation 时
//!   自动降级为 v1.0 的 Reject。
//!
//! 三层审批（D9）：
//! - 白名单 → AutoExecute
//! - 黄名单 → AutoExecute（+ 日志）
//! - 黑名单 → RequestElicitation（v1.1：客户端确认）/ Reject（v1.0 降级）
//! - 未知命令 → RequestElicitation（v1.1：让用户决定）/ Reject（v1.0 降级）

use crate::dangerous_commands::{self, CommandRisk, DangerousMatch};

/// 审批决策结果。
pub enum ApprovalDecision {
    /// 自动执行（白名单/黄名单）。
    AutoExecute,
    /// 拒绝执行，附带三段式理由（v1.0 降级路径：客户端不支持 elicitation 时）。
    Reject(String),
    /// 需要用户确认（v1.1：经 MCP elicitation 在客户端界面内弹确认框）。
    RequestElicitation(ElicitationInfo),
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

    /// v1.0 降级用的拒绝文本（客户端不支持 elicitation 时）。
    pub fn to_rejection(&self) -> String {
        format_rejection(&self.intent, &self.command, &self.consequence)
    }
}

/// 评估一条命令的审批决策。
///
/// `intent` 是 AI 声明的意图（来自工具调用的 intent 参数），
/// `command` 是真实要执行的命令。
/// `whitelist` / `yellow_list` 见 D9 决策（白名单内置，黄名单按资产配置）。
pub fn evaluate(
    command: &str,
    intent: &str,
    whitelist: &[&str],
    yellow_list: &[String],
) -> ApprovalDecision {
    let wl: Vec<String> = whitelist.iter().map(|s| s.to_string()).collect();
    match dangerous_commands::classify_command(command, &wl, yellow_list) {
        CommandRisk::Safe => {
            log::info!("approval: command approved (whitelist)");
            ApprovalDecision::AutoExecute
        }
        CommandRisk::Allowed => {
            log::info!("approval: command approved (yellow list)");
            ApprovalDecision::AutoExecute
        }
        CommandRisk::Dangerous(m) => {
            log::warn!(
                "approval: dangerous command, requesting elicitation (pattern={})",
                m.pattern
            );
            ApprovalDecision::RequestElicitation(ElicitationInfo {
                intent: intent.to_string(),
                command: command.to_string(),
                consequence: predict_consequence(&m),
            })
        }
        CommandRisk::Unknown => {
            log::warn!("approval: unknown command, requesting elicitation (user decides)");
            ApprovalDecision::RequestElicitation(ElicitationInfo {
                intent: intent.to_string(),
                command: command.to_string(),
                consequence: "此命令不在已知安全名单内，需要用户确认。".to_string(),
            })
        }
    }
}

/// 三段式拒绝信息格式（D5+D9：AI意图 + 真实命令 + 后果预测）。
///
/// 这是 v1.0 的核心安全呈现：即使 AI 在 intent 里声称「查看日志」，
/// 但 command 是 `rm -rf /var/log`，用户（或读 error 的 LLM）能通过
/// 三段对照识破伪装。
fn format_rejection(intent: &str, command: &str, consequence: &str) -> String {
    format!(
        "【操作已被 MCP 审批拦截】\n\n\
         【AI 声明意图】{}\n\n\
         【真实命令】{}\n\n\
         【后果预测】{}\n\n\
         此命令在 v1.0 模式下被拒绝。如确需执行，请在 myshelltool GUI 中手动操作。",
        if intent.is_empty() { "(AI 未声明意图)" } else { intent },
        command,
        consequence,
    )
}

/// 基于命中的危险模式给出固定后果预测文案。
///
/// 对应 dangerous_commands.rs 的 16 条正则，每类给出人话后果说明，
/// 帮助用户判断是否真的要执行。
fn predict_consequence(m: &DangerousMatch) -> String {
    let p = m.pattern.as_str();
    // 按正则特征匹配文案（顺序对应 dangerous_commands.rs 的 raw 数组）
    if p.contains("rm\\s+") {
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
    fn whitelist_command_auto_approved() {
        let d = evaluate("df -h", "查询磁盘", READONLY_WHITELIST, &[]);
        assert!(matches!(d, ApprovalDecision::AutoExecute));
    }

    #[test]
    fn dangerous_command_rejected_with_three_sections() {
        let d = evaluate("rm -rf /var/log", "清理日志", READONLY_WHITELIST, &[]);
        match d {
            ApprovalDecision::Reject(msg) => {
                assert!(msg.contains("【AI 声明意图】清理日志"));
                assert!(msg.contains("【真实命令】rm -rf /var/log"));
                assert!(msg.contains("【后果预测】"));
                assert!(msg.contains("递归"));
            }
            _ => panic!("expected Reject"),
        }
    }

    #[test]
    fn unknown_command_rejected_fail_secure() {
        let d = evaluate("echo hello", "", READONLY_WHITELIST, &[]);
        match d {
            ApprovalDecision::Reject(msg) => {
                assert!(msg.contains("fail-secure"));
                assert!(msg.contains("AI 未声明意图"));
            }
            _ => panic!("expected Reject"),
        }
    }

    #[test]
    fn yellow_list_allows_command() {
        let yellow = vec!["nginx -t".to_string()];
        let d = evaluate("nginx -t", "测试配置", &[], &yellow);
        assert!(matches!(d, ApprovalDecision::AutoExecute));
    }

    #[test]
    fn mkfs_consequence_mentioned() {
        let d = evaluate("mkfs.ext4 /dev/sda1", "格式化", &[], &[]);
        match d {
            ApprovalDecision::Reject(msg) => {
                assert!(msg.contains("格式化"));
                assert!(msg.contains("数据"));
            }
            _ => panic!("expected Reject"),
        }
    }
}
