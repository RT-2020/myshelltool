//! 命令文本脱敏（AGENTS.md §8 凭据红线）。
//!
//! 事故：MCP 执行日志把**命令原文**落盘并在 GUI 回显（`mcp-execution-log.json`，
//! 30 天 / 1000 条长期保留），应用日志也以 `{:?}` 打印命令。AI 宿主执行的命令
//! 常带明文口令——`mysql -uroot -pP@ss`、`curl -u user:pass`、`--password=x`、
//! `export TOKEN=...`——于是凭据以明文进入审计文件与日志文件，违反「凭据绝不进
//! 日志」红线。这与「靠猜测」同类：**把命令串当无敏感信息的普通文本**是一种臆断
//! （假设里面没有口令），而正确做法是落盘前统一过一道脱敏。
//!
//! 设计取舍：
//! - 无正则依赖（core 里没引 regex，也不值得为这一处引）：按 shell 词法做单趟
//!   字符扫描——单调推进、无回溯，不存在灾难性回溯风险。
//! - **宁可多遮**：口令类值一律替换成 `<redacted>`，不保留长度/前缀/尾字符。
//! - 不改变命令结构（分隔符、其它 token 原样保留），脱敏后的文本仍可读、可审计
//!   （看得出「执行了什么类型的命令」，看不出「用了什么凭据」）。
//! - 已知边界：`-p`/`-u` 的**分离式**写法（`-p P@ss`）与 `-u user pass` 需看下一个
//!   token，已覆盖；`--password` 后跟独立值也覆盖。无法识别自定义脚本参数
//!   （如 `myscript --secret xxx`），那些应由调用方避免。

/// 需要脱敏的敏感参数名（长选项，`--name=value` 或 `--name value` 形态）。
const SENSITIVE_LONG: &[&str] = &[
    "password",
    "passwd",
    "pass",
    "token",
    "secret",
    "apikey",
    "api-key",
    "access-key",
    "auth",
    "credential",
];

/// 环境变量名里出现这些片段即视为凭据（`TOKEN=...`、`PASSWORD=...`、`SECRET=...`）。
const SENSITIVE_ENV_HINTS: &[&str] = &["TOKEN", "PASSWORD", "PASSWD", "SECRET", "APIKEY", "API_KEY", "CREDENTIAL"];

/// 遮蔽标记（固定文本，不泄露长度）。
const MASK: &str = "<redacted>";

/// 对命令文本做凭据脱敏。
pub fn redact_command(command: &str) -> String {
    let chars: Vec<char> = command.chars().collect();
    let mut out = String::with_capacity(command.len());
    let mut i = 0usize;
    // 上一个 token 是否要求「吃掉下一个 token」（`-p` 分离式、`--password` 分离式）
    let mut expect_value = false;
    // 当前 token 是否位于「命令起始」位置（用于识别 `TOKEN=value` 形态的赋值）
    let mut at_command_start = true;

    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || matches!(c, ';' | '|' | '&' | '\n' | '\r') {
            out.push(c);
            if matches!(c, ';' | '|' | '&' | '\n' | '\r') {
                at_command_start = true;
            }
            i += 1;
            continue;
        }

        // 收集一个 token（尊重单/双引号：引号内的空白不是分隔符；引号原样保留）
        let start = i;
        let mut quote: Option<char> = None;
        while i < chars.len() {
            let ch = chars[i];
            if let Some(q) = quote {
                if ch == q {
                    quote = None;
                }
                i += 1;
                continue;
            }
            if ch == '\'' || ch == '"' {
                quote = Some(ch);
                i += 1;
                continue;
            }
            if ch.is_whitespace() || matches!(ch, ';' | '|' | '&' | '\n' | '\r') {
                break;
            }
            i += 1;
        }
        let token: String = chars[start..i].iter().collect();
        let unquoted: &str = token.trim_matches(|q| q == '\'' || q == '"');

        if expect_value {
            // 值形态：遮掉整个 token（不保留长度/首尾字符），并补一个空格维持可读性
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push_str(MASK);
            expect_value = false;
            continue;
        }

        // 每个 token 独立判断是否处于命令起始位置；`sudo/env/...` 前缀之后仍算起始
        let starts_command = at_command_start;
        at_command_start = false;

        if starts_command {
            // 形态一：`NAME=value`（指令名不含 `=`）
            if let Some(eq) = unquoted.find('=') {
                let name = &unquoted[..eq];
                let value = &unquoted[eq + 1..];
                if !value.is_empty() && is_sensitive_env_name(name) {
                    out.push_str(name);
                    out.push('=');
                    out.push_str(MASK);
                    continue;
                }
            }
            // 形态二：前缀命令之后仍可能是赋值/命令名（`export TOKEN=...` 是最常见形态）
            if matches!(
                unquoted,
                "sudo" | "env" | "export" | "nohup" | "command" | "exec" | "set"
            ) {
                out.push_str(&token);
                at_command_start = true;
                continue;
            }
        }

        // `--password=value` / `--token=value`
        if let Some(rest) = unquoted.strip_prefix("--") {
            if let Some(eq) = rest.find('=') {
                let name = &rest[..eq];
                if is_sensitive_long_name(name) {
                    let indent = token.len() - unquoted.len();
                    out.push_str(&token[..indent]);
                    out.push_str("--");
                    out.push_str(name);
                    out.push('=');
                    out.push_str(MASK);
                    continue;
                }
            } else if is_sensitive_long_name(rest) {
                // 只认**精确**敏感名：`--password-stdin` / `--token-file` 这类
                // 「名字里含 password/token 但语义不是取值」的选项不能被吞值
                if matches!(rest.to_ascii_lowercase().as_str(), "password" | "passwd" | "token" | "secret") {
                    out.push_str(&token);
                    expect_value = true;
                    continue;
                }
            }
        }

        // `curl -u user:pass`（只认精确 `-u`；`-uroot` 是 mysql 的用户名，不是值标记）
        if unquoted == "-u" || unquoted == "--user" {
            out.push_str(&token);
            expect_value = true;
            continue;
        }
        if let Some(rest) = unquoted.strip_prefix("-u") {
            // 紧凑式 `-uuser:pass`：含 `:` 才是凭据（`-uroot` 无冒号 → 用户名，保留）
            if rest.contains(':') {
                out.push_str("-u");
                out.push_str(MASK);
                continue;
            }
        }

        // `mysql -pP@ss`（紧凑式）与 `-p` + 下一 token（分离式）
        if let Some(rest) = unquoted.strip_prefix("-p") {
            // 只认小写 `-p`：`mysql -P 3306` 的 `-P` 是端口，不能遮
            if rest.is_empty() {
                out.push_str(&token);
                expect_value = true;
                continue;
            }
            if !rest.starts_with('-') {
                out.push_str("-p");
                out.push_str(MASK);
                continue;
            }
        }

        out.push_str(&token);
    }

    out
}

/// 长选项名（去掉 `--` 后的部分）是否属于敏感参数。
fn is_sensitive_long_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.contains("password") || lower.contains("passwd") || lower.contains("token") {
        return true;
    }
    SENSITIVE_LONG.contains(&lower.as_str())
}

/// 环境变量名是否像凭据（`GITHUB_TOKEN`、`DB_PASSWORD`、`MY_SECRET`…）。
///
/// 只校验**名字**的全大写形态（`a=b` 这类普通参数不误判），值与大小写无关
/// （`GITHUB_TOKEN=ghp_abc123` 的值是小写也要遮）。
fn is_sensitive_env_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    // 仅接受 [A-Z0-9_]+ 形态：命令行里 `host=db`、`port=3306` 这类参数不参与判定
    if !name
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    {
        return false;
    }
    let upper = name.to_ascii_uppercase();
    SENSITIVE_ENV_HINTS.iter().any(|h| upper.contains(h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_compact_password_is_masked() {
        let out = redact_command("mysql -uroot -pP@ssw0rd -e 'select 1'");
        assert!(!out.contains("P@ssw0rd"), "口令泄露: {out}");
        assert!(out.contains("-p<redacted>"));
        // 结构与非敏感部分保留，日志仍可读可审计
        assert!(out.contains("mysql"));
        assert!(out.contains("-uroot"));
    }

    #[test]
    fn mysql_separated_password_is_masked() {
        let out = redact_command("mysql -u root -p P@ssw0rd -e 'show databases'");
        assert!(!out.contains("P@ssw0rd"), "口令泄露: {out}");
        assert!(out.contains("-p <redacted>"));
    }

    #[test]
    fn uppercase_port_flag_is_not_masked() {
        // `-P` 是端口（mysql/ssh 等），不能当成口令遮掉——过度脱敏会让审计失去意义
        let out = redact_command("mysql -h db -P 3306 -uroot");
        assert!(out.contains("3306"), "端口被误遮: {out}");
        assert!(!out.contains(MASK));
    }

    #[test]
    fn curl_basic_auth_is_masked() {
        let out = redact_command("curl -u user:S3cret https://api.example.com");
        assert!(!out.contains("S3cret"), "口令泄露: {out}");
        assert!(out.contains("-u <redacted>"));
        // 紧凑式 `-uuser:pass`
        let out2 = redact_command("curl -uuser:S3cret https://x");
        assert!(!out2.contains("S3cret"), "口令泄露: {out2}");
    }

    #[test]
    fn long_option_forms_are_masked() {
        let a = redact_command("mytool --password=P@ss --verbose");
        assert!(!a.contains("P@ss"), "泄露: {a}");
        assert!(a.contains("--password=<redacted>"));
        assert!(a.contains("--verbose"), "非敏感参数不该被动: {a}");

        let b = redact_command("mytool --password P@ss");
        assert!(!b.contains("P@ss"), "泄露: {b}");

        let c = redact_command("docker login --password-stdin");
        // 无值形态不吞掉后一个 token（`--password-stdin` 不是敏感名精确匹配）
        assert!(c.contains("--password-stdin"));
    }

    #[test]
    fn env_assignment_is_masked() {
        let out = redact_command("export GITHUB_TOKEN=ghp_abc123 && deploy.sh");
        assert!(!out.contains("ghp_abc123"), "令牌泄露: {out}");
        assert!(out.contains("GITHUB_TOKEN=<redacted>"));
        // 非凭据赋值保留原样
        let keep = redact_command("export PATH=/usr/bin:/bin; ls");
        assert!(keep.contains("/usr/bin"), "普通变量被误遮: {keep}");
    }

    #[test]
    fn quoted_and_compound_commands_keep_structure() {
        let out = redact_command("echo 'a;b' | mysql -pSecret; ls -la");
        assert!(!out.contains("Secret"), "泄露: {out}");
        assert!(out.contains("echo 'a;b'"), "引号内结构被破坏: {out}");
        assert!(out.contains("; ls -la"), "复合命令结构被破坏: {out}");
    }

    #[test]
    fn benign_commands_are_untouched() {
        for cmd in [
            "df -h", // fact-guard:allow locale-pinned-df 脱敏单测的样例文本，不执行任何命令
            "ls -la /var/log",
            "systemctl status nginx",
            "find /tmp -name '*.log' -delete",
            "ps aux | grep nginx",
        ] {
            assert_eq!(redact_command(cmd), cmd, "无害命令被改动: {cmd}");
        }
    }
}
