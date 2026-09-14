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
//!
//! catastrophic 毁灭层（v2.1，MCP 侧专用，`detect_catastrophic_command`）：
//! - 覆盖「机器报废级」命令：mkfs / dd 写块设备 / fork bomb / 重定向块设备 /
//!   chmod 系统目录 / rm 根级删除。MCP 侧 `approval::evaluate` **必须先于**
//!   `classify_command` 跑本函数，命中即 HardBlock 直接拒绝（不弹窗不等
//!   超时）——**此顺序是安全前提，不可调换**。
//! - 本层是 16 条黑名单的**近似超集**：黑名单 pattern 1 要求 r/f 在同一短
//!   选项 token 内，`rm -fr /`、`rm -r -f /` 等分离/逆序形态连黑名单都
//!   MISS（Minimal 档下曾以 Unknown 零审批直接执行的活洞），仅由本层的
//!   rm 根级正则覆盖；目标带子路径的 rm（如 `rm -rf /var/log`）不属本层，
//!   仍走 classify 审批链。
//! - GUI 前端 JS 版（src/lib/dangerousCommands.js）保持 16 条全量弹窗语义
//!   不变，与 MCP 侧分层是刻意分叉（GUI 有真人盯着弹窗，MCP 面向无人值守）。

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
    /// 15 条主正则（第 11 条 chmod 除外，单独处理）。bool 标记是否属
    /// catastrophic 毁灭层（v2.1：mkfs / dd 写块设备 / fork bomb ×2 /
    /// 重定向块设备，共 5 条）。
    patterns: Vec<(&'static str, Regex, bool)>,
    /// 第 11 条：chmod -R 递归授权 + 目标路径捕获（lookahead 改写方案）。
    /// 捕获组 1 = 目标绝对路径（如 /etc、/usr）。
    chmod_recursive: Regex,
    /// rm 根级删除（catastrophic 专属）：覆盖黑名单 pattern 1 MISS 的
    /// 分离/逆序短选项形态（`rm -f -r /`），目标限根/家/当前目录。
    rm_root: Regex,
}

impl CompiledPatterns {
    const CHMOD_RECURSIVE_SRC: &'static str =
        r"(?i)\bchmod\s+-R\s+[0-7]{3,4}\s+(/\S*)";

    /// rm 根级删除正则（catastrophic 专属，规格见模块注释）：
    /// - 标志：短选项合并（-rf/-fr）、分离（-r -f 任意顺序）、长选项
    ///   （--recursive --force 任意顺序）均可；标志与目标间允许其他选项
    ///   （含 --no-preserve-root 等长选项）。
    /// - 目标：`/`、`/*`、`~`、`~/*`、裸 `*`、`./*`、`./`（长形态在前，
    ///   防短分支抢先消费导致边界校验误失败）。
    /// - 目标后必须跟边界（行尾/空白/;|&)），防误拦 `rm -rf *.log`、
    ///   `rm -rf ~/build`、`rm -rf /etc/yum.repos.d/*` 等子路径/文件形态。
    /// - 不锚定行首（覆盖 `echo hi; rm -rf /` 组合命令）。
    const RM_ROOT_SRC: &'static str = concat!(
        r"(?i)\brm\s+(?:-[^\s]+\s+)*",
        r"(?:",
        // A：r 型标志在前、f 型在后（含 --recursive ... --force 与混搭）
        r"(?:-[a-z]*r[a-z]*|--recursive)(?:\s+-[^\s]+)*\s+",
        r"(?:-[a-z]*f[a-z]*|--force)(?:\s+-[^\s]+)*\s+",
        r"|",
        // B：f 型标志在前、r 型在后（rm -f -r / 封堵现行黑名单 MISS 的活洞）
        r"(?:-[a-z]*f[a-z]*|--force)(?:\s+-[^\s]+)*\s+",
        r"(?:-[a-z]*r[a-z]*|--recursive)(?:\s+-[^\s]+)*\s+",
        r"|",
        // C：单短选项 token 兼含 r 与 f（-rf / -fr / -Rf ...）
        r"(?:-[a-z]*r[a-z]*f[a-z]*|-[a-z]*f[a-z]*r[a-z]*)(?:\s+-[^\s]+)*\s+",
        r")",
        r"(?:/\*|/|~/\*|~|\*|\./\*|\./)",
        r"(?:[\s;|&)]|$)",
    );

    fn compile() -> Self {
        // 15 条主模式（第 11 条 chmod 移出，单独用捕获组方案）。
        // 顺序与 JS 源一致，便于核对。元组第二位 = catastrophic 标记。
        let raw: &[(&str, bool)] = &[
            // 1. rm -rf / rm --recursive ... --force（子路径删除，非毁灭）
            (r"(?i)rm\s+(-[a-zA-Z]*r[a-zA-Z]*f|--recursive\b.*--force\b)", false),
            // 2. mkfs（毁灭：格式化磁盘）
            (r"(?i)\bmkfs\b", true),
            // 3. dd of=/dev/（毁灭：直写块设备）
            (r"(?i)\bdd\b[^|]*\bof=/dev/", true),
            // 4. fork bomb 变体 1（毁灭：耗尽进程资源）
            (r":\s*\(\)\s*\{[^}]*:\|:\s*&\s*\}\s*;", true),
            // 5. 重定向到块设备 >/dev/sdX（毁灭：覆盖磁盘）
            (r"(?i)>\s*/dev/sd[a-z]", true),
            // 6-10. 关机/重启类（非毁灭：可人工恢复）
            (r"(?i)\bshutdown\b", false),
            (r"(?i)\breboot\b", false),
            (r"(?i)\bhalt\b", false),
            (r"(?i)\bpoweroff\b", false),
            (r"(?i)\binit\s+0\b", false),
            // （第 11 条 chmod -R 见下方单独处理）
            // 12. chown -R（非毁灭）
            (r"(?i)\bchown\s+-R\b", false),
            // 13. iptables -F（非毁灭）
            (r"(?i)\biptables\s+-F\b", false),
            // 14. fork bomb 变体 2（毁灭，JS 的 :() 字面量转义）
            (r":\(\)\s*\{\s*:\|:&\s*\};:", true),
            // 15-16. curl/wget 管道执行 shell（非毁灭）
            (r"(?i)\bcurl\b[^|]*\|\s*(bash|sh|zsh)\b", false),
            (r"(?i)\bwget\b[^|]*\|\s*(bash|sh|zsh)\b", false),
        ];

        let patterns = raw
            .iter()
            .map(|(src, catastrophic)| {
                (
                    *src,
                    Regex::new(src).expect("危险命令正则编译失败"),
                    *catastrophic,
                )
            })
            .collect();

        let chmod_recursive = Regex::new(Self::CHMOD_RECURSIVE_SRC)
            .expect("chmod 递归正则编译失败");
        let rm_root = Regex::new(Self::RM_ROOT_SRC).expect("rm 根级正则编译失败");

        Self {
            patterns,
            chmod_recursive,
            rm_root,
        }
    }
}

/// 取全局编译后正则单例。
fn patterns() -> &'static CompiledPatterns {
    static PATTERNS: OnceLock<CompiledPatterns> = OnceLock::new();
    PATTERNS.get_or_init(CompiledPatterns::compile)
}

/// chmod -R 黑名单层的安全路径前缀（对应 JS 第 11 条 negative lookahead 的排除项）。
///
/// v2.5 变化：`/home`、`/Users` 从本名单移除——家目录递归 chmod 属破坏性操作
/// （-R 波及全部用户文件），应走黑名单审批链（Strict 人工确认 / Minimal 放行记
/// 日志），不再免拦截。仅保留临时目录。**JS 侧 negative lookahead 已同步移除
/// `/home|/Users`**，两端保持同一判定口径（GUI 与 MCP 共享单点真相）。
const SAFE_CHMOD_PREFIXES: &[&str] = &["/tmp", "/var/tmp"];

/// chmod -R 毁灭层（catastrophic）的豁免前缀。
///
/// 比 SAFE_CHMOD_PREFIXES 多 `/home`、`/Users`：家目录递归 chmod 破坏的是
/// 用户数据（通常可恢复/重装用户态即复原），够不上「机器报废级」硬拒，
/// 归黑名单审批链即可；`/etc`、`/usr` 等系统目录 chmod 仍两档恒 HardBlock。
const CHMOD_CATASTROPHIC_EXEMPT_PREFIXES: &[&str] = &["/tmp", "/var/tmp", "/home", "/Users"];

/// 边界安全前缀判定：`target` 等于 `safe` 本身，或以 `"<safe>/"` 开头。
///
/// v2.5：裸 `starts_with` 会把 `/home2`、`/tmp2`、`/homework` 误判为安全
/// 目标（Minimal 档零审批直接执行），必须补边界。
fn under_safe_prefix(target: &str, safe: &str) -> bool {
    target == safe || target.starts_with(&format!("{safe}/"))
}

/// 提取 chmod -R 的目标绝对路径（正则捕获组 1）。无匹配返回 None。
fn chmod_recursive_target(text: &str) -> Option<String> {
    patterns()
        .chmod_recursive
        .captures(text)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

/// chmod -R 递归授权是否命中系统目录（黑名单第 11 条：安全前缀之外一律 Dangerous）。
fn chmod_recursive_hits_system(text: &str) -> bool {
    chmod_recursive_target(text).is_some_and(|target| {
        !SAFE_CHMOD_PREFIXES
            .iter()
            .any(|safe| under_safe_prefix(&target, safe))
    })
}

/// chmod -R 递归授权是否命中毁灭层（系统目录 chmod = 机器报废级，HardBlock）。
///
/// 与黑名单层的差别：`/home`、`/Users` 在本层豁免（家目录数据可恢复，走
/// 审批链），其余判定（含 `/home2` 边界封堵）与黑名单层一致。
fn chmod_recursive_hits_catastrophic(text: &str) -> bool {
    chmod_recursive_target(text).is_some_and(|target| {
        !CHMOD_CATASTROPHIC_EXEMPT_PREFIXES
            .iter()
            .any(|safe| under_safe_prefix(&target, safe))
    })
}

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
    for (src, re, _catastrophic) in &compiled.patterns {
        if re.is_match(text) {
            return Some(DangerousMatch {
                pattern: (*src).to_string(),
                sample: sample(text),
            });
        }
    }

    // 第 11 条 chmod -R：命中系统目录（排除安全前缀）。
    if chmod_recursive_hits_system(text) {
        return Some(DangerousMatch {
            pattern: CompiledPatterns::CHMOD_RECURSIVE_SRC.to_string(),
            sample: sample(text),
        });
    }

    None
}

/// 检测「机器报废级」毁灭性命令（v2.1 catastrophic 层，MCP 侧专用）。
///
/// 覆盖：主正则中标记 catastrophic 的 5 条（mkfs / dd 写块设备 / fork
/// bomb ×2 / 重定向块设备）+ rm 根级删除（含黑名单 MISS 的分离/逆序短选项
/// 形态）+ chmod 系统目录。
///
/// 注意：**不做 `len < 4` 早退**——那是 JS 对齐产物，毁灭层宁多拦不早退。
/// MCP 审批（approval::evaluate）必须先于 classify_command 调用本函数
/// （顺序是安全前提，见模块注释）。
pub fn detect_catastrophic_command(text: &str) -> Option<DangerousMatch> {
    let compiled = patterns();

    for (src, re, catastrophic) in &compiled.patterns {
        if *catastrophic && re.is_match(text) {
            return Some(DangerousMatch {
                pattern: (*src).to_string(),
                sample: sample(text),
            });
        }
    }

    if compiled.rm_root.is_match(text) {
        return Some(DangerousMatch {
            pattern: CompiledPatterns::RM_ROOT_SRC.to_string(),
            sample: sample(text),
        });
    }

    if chmod_recursive_hits_catastrophic(text) {
        return Some(DangerousMatch {
            pattern: CompiledPatterns::CHMOD_RECURSIVE_SRC.to_string(),
            sample: sample(text),
        });
    }

    None
}

/// 取文本前 80 字符作样本（对齐 JS `text.slice(0, 80)`，按 char 边界截断避免切坏 UTF-8）。
fn sample(text: &str) -> String {
    text.chars().take(80).collect()
}

/// find 带 `-delete` / `-exec`（含 `-execdir`）action 时不进白名单。
///
/// 白名单前缀含 `"find"`（approval.rs READONLY_WHITELIST），但这两个 action
/// 是写操作：`find / -type f -delete` 删除全部匹配项、`-exec` 对每条匹配
/// 执行任意命令，曾整体被判 Safe 免审批。检测按命令段（`;` / `|` / `&` /
/// 换行边界切分）进行：任一段以 find 开头（词边界确认，排除 findx 之类）
/// 且含 `-delete` 或 `-exec` 前缀 token 即命中。命中后落入黄名单/Unknown
/// 层（Minimal 放行记日志 / Strict 人工确认），不进 HardBlock，纯检索的
/// find（如 `find /var/log -name '*.log'`）保持白名单放行。
fn contains_destructive_find(text: &str) -> bool {
    for segment in text.split(|c: char| c == ';' || c == '|' || c == '&' || c == '\n') {
        let seg = segment.trim();
        if !seg.starts_with("find") {
            continue;
        }
        // 词边界：find 后必须是空白或段尾（排除 findmnt / findx）。
        let rest = &seg["find".len()..];
        if !(rest.is_empty() || rest.starts_with(char::is_whitespace)) {
            continue;
        }
        // token 级匹配：-delete 精确、-exec 前缀（覆盖 -execdir）。
        if seg
            .split_whitespace()
            .any(|tok| tok == "-delete" || tok.starts_with("-exec"))
        {
            return true;
        }
    }
    false
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

    // 2. 白名单：**分段后逐段匹配**（v2.6）。曾整串前缀匹配——`df -h; cat
    //    /etc/shadow` 以 `df` 开头即判 Safe，Strict 档也免人工确认。见
    //    shell.rs 模块注释（判据从「字符串前缀」换成「shell 结构」）。
    //    任一形态不成立即不进白名单，落黄名单/Unknown 层（Minimal 记日志
    //    放行 / Strict 人工确认），不误升级为 HardBlock。
    let parsed = crate::shell::split_shell_segments(trimmed);
    let shape_simple = !parsed.found_redirect && !parsed.found_substitution;
    if shape_simple && !parsed.segments.is_empty() {
        let all_whitelisted = parsed.segments.iter().all(|seg| {
            // 例外：find 带 -delete/-exec（写操作）不进白名单。
            !contains_destructive_find(seg)
                && whitelist
                    .iter()
                    .any(|allowed| command_matches_prefix(seg, allowed))
        });
        if all_whitelisted {
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
mod tests;
