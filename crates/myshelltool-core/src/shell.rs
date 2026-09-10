//! shell 命令分段（命令层安检的**形状**判据，替代「正则/前缀匹配」的猜测）。
//!
//! 动机（指南 §7 形态 B，实战事故）：白名单曾是**整串前缀匹配**——
//! `df -h; cat /etc/shadow` 以 `df` 开头即被判 `Safe`，于是
//! `approval::evaluate` 在 **Strict 档也免人工确认**、Minimal 档更是零审批
//! 直接执行。前缀匹配假设「命令串 = 命令」，而 shell 语法里 `; | & \n` 让
//! 一条字符串携带多条命令、`> >> < >&` 让其带写副作用、`$( )` 与反引号
//! 让替换结果藏进参数——这些形态都能塞在白名单首段之后。
//!
//! 修法不是「再加几条正则」（同类错误第二次出现就漏），而是把判据换成
//! **结构**：真正按 shell 的词法把命令串切成若干独立命令段，各段都命中
//! 白名单才算 Safe；写副作用与命令替换则直接判为不可白名单放行。
//!
//! 已知边界（有意保守，宁误报不漏报）：
//! - 不做完整 shell 语法分析（变量展开、arithmetic、here-doc）。因此**单引号
//!   之外**出现 `$` 一律视为命令替换，宁可误报（Strict 档多一次人工确认 /
//!   Minimal 档记日志放行），也不放过 `$(rm -rf /tmp/x)` 这类隐藏执行。
//! - 转义分隔符（`\;`，常见于 `find -exec ... \;`）按字面内容处理，不切段。
//! - 换行等价于 `;`（多行命令串不因换行而逃过分段）。

/// shell 命令串的分段结果。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShellSegments {
    /// 各独立命令段（已 trim，空段丢弃）。
    pub segments: Vec<String>,
    /// 出现未加引号的重定向操作符（`>` `>>` `<` `>&` `<>`）——可写任意文件，
    /// 使「只读白名单命令」不再只读（`ls > /etc/passwd`）。
    pub found_redirect: bool,
    /// 出现未加引号的命令替换（`$(` / 反引号）或单引号外的 `$`——替换结果会
    /// 作为参数参与执行，白名单前缀无法覆盖其真实行为。
    pub found_substitution: bool,
}

impl ShellSegments {
    /// 该命令串是否为「单条命令」形态（未出现任何组合/重定向/替换）。
    pub fn is_single(&self) -> bool {
        self.segments.len() <= 1 && !self.found_redirect && !self.found_substitution
    }
}

/// 把命令串按 shell 词法切成独立命令段，并标记写副作用/命令替换形态。
///
/// `;` 换行 `&` `|`（含 `&&` `||`）在引号外即为命令边界；引号内的分隔符
/// 属字面量（`find ... -name 'a;b'` 不是两条命令）。
pub fn split_shell_segments(text: &str) -> ShellSegments {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut found_redirect = false;
    let mut found_substitution = false;

    let mut chars = text.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        // 单引号内一切皆字面量（shell 语义：单引号内无转义、无展开）。
        if in_single {
            current.push(c);
            if c == '\'' {
                in_single = false;
            }
            continue;
        }

        match c {
            '\'' if !in_double => {
                in_single = true;
                current.push(c);
            }
            '"' => {
                in_double = !in_double;
                current.push(c);
            }
            '\\' => {
                // 转义：连同下一个字符一起当字面内容（`\;` 不切段，
                // `find -exec rm {} \;` 的 `\;` 必须留在同一段里）。
                current.push('\\');
                if let Some((_, next)) = chars.next() {
                    current.push(next);
                }
            }
            // 命令替换：单引号之外出现（此处 in_single 已排除）。
            '`' | '$' => {
                found_substitution = true;
                current.push(c);
            }
            '>' | '<' => {
                found_redirect = true;
                current.push(c);
            }
            ';' | '\n' | '\r' | '&' | '|' => {
                // 段边界：`|` 与 `||`、`&` 与 `&&` 在此都切开，剩余部分由
                // 后续字符自然累积。
                let seg = current.trim();
                if !seg.is_empty() {
                    segments.push(seg.to_string());
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }

    let seg = current.trim();
    if !seg.is_empty() {
        segments.push(seg.to_string());
    }

    ShellSegments {
        segments,
        found_redirect,
        found_substitution,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_command_is_single_segment() {
        let s = split_shell_segments("env LC_ALL=C df -h");
        assert_eq!(s.segments, vec!["env LC_ALL=C df -h"]);
        assert!(s.is_single());
    }

    #[test]
    fn semicolon_pipe_and_newline_split_segments() {
        let s = split_shell_segments("env LC_ALL=C df -h; cat /etc/shadow");
        assert_eq!(s.segments, vec!["env LC_ALL=C df -h", "cat /etc/shadow"]);
        assert!(!s.is_single());

        let s = split_shell_segments("env LC_ALL=C df -h | head -3");
        assert_eq!(s.segments, vec!["env LC_ALL=C df -h", "head -3"]);

        let s = split_shell_segments("env LC_ALL=C df -h\ncat /etc/shadow");
        assert_eq!(s.segments, vec!["env LC_ALL=C df -h", "cat /etc/shadow"]);

        let s = split_shell_segments("env LC_ALL=C df -h && cat /etc/shadow");
        assert_eq!(s.segments, vec!["env LC_ALL=C df -h", "cat /etc/shadow"]);
    }

    #[test]
    fn quoted_separator_is_not_a_boundary() {
        let s = split_shell_segments("find /var/log -name 'a;b' -type f");
        assert_eq!(s.segments.len(), 1);
        assert!(!s.found_redirect);

        let s = split_shell_segments("grep 'x|y' /etc/hosts");
        assert_eq!(s.segments.len(), 1);
    }

    #[test]
    fn redirect_and_substitution_are_flagged() {
        assert!(split_shell_segments("ls -la > /etc/passwd").found_redirect);
        assert!(split_shell_segments("ls 2>&1").found_redirect);
        assert!(split_shell_segments("env LC_ALL=C df -h $(cat /etc/shadow)").found_substitution);
        assert!(split_shell_segments("env LC_ALL=C df -h `cat /etc/shadow`").found_substitution);
        // 单引号内是字面量，不算替换
        assert!(!split_shell_segments("env LC_ALL=C df -h '$HOME'").found_substitution);
    }

    #[test]
    fn escaped_delimiter_stays_literal() {
        let s = split_shell_segments("find /tmp -exec rm {} \\;");
        assert_eq!(s.segments.len(), 1);
    }
}
