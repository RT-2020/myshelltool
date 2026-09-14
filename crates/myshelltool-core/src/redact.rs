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

/// 把外部响应文本裁成可安全回显/落日志的摘录。
///
/// 为什么需要：诊断网络故障时最有价值的信息往往在**对端返回的正文**里（代理节点
/// 挂掉时返回的错误页、网关的 502 页面），但正文可能夹带凭据类字面量——OAuth
/// 场景里 `device_code` 就是换 token 的两要素之一（client_id 是公开值）。所以
/// 「原样回显正文」违反 §8 凭据红线，「完全不回显」又让用户只能看到一句
/// 「请求失败」。此处按**调用方声明的秘密字面量**先遮蔽、再按**字符**截断。
///
/// - 逐字符截断：不会切在 UTF-8 边界上 panic（`&s[..n]` 会）；
/// - 秘密替换在截断之前做：短摘录里也不能出现明文；
/// - 空秘密串忽略（`replace("")` 会在每字符间插入标记）。
pub fn redact_excerpt(text: &str, secrets: &[&str], max_chars: usize) -> String {
    let mut masked = text.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            masked = masked.replace(secret, MASK);
        }
    }
    masked.chars().take(max_chars).collect()
}

/// 对远端命令**输出**文本做凭据脱敏（落审计日志 / 组装回显摘要前）。
///
/// 与 [`redact_command`]（命令形态：`-p` / `--password` / `-u`）互补，这里处理
/// 输出形态（v2.6 审计 backlog：`env` 输出的 `GITHUB_TOKEN=...`、
/// `show create user` 的 `IDENTIFIED BY '<明文>'` 曾原样进 output_summary）：
/// - `SENSITIVE_KEY=value`：键命中 [`is_sensitive_env_name`]（全大写
///   [A-Z0-9_]+ 且含 TOKEN/PASSWORD/SECRET/… 片段）即遮值；逐 token 扫，
///   同一行多个赋值都处理，空白原样保留；
/// - mysql `IDENTIFIED … BY '<…>'` / `IDENTIFIED … AS '<哈希>'`：只认
///   IDENTIFIED 之后 80 字节内的 BY/AS 子句，普通 `ORDER BY 'x'` 不误伤。
///
/// 取向是**宁过度勿遗漏**：审计日志少看一列值无害，多看一个口令就是
/// §8 凭据红线事故。已知局限：SQL 字符串里的转义引号（`\'`）按首个闭引号
/// 截断，极罕见形态可能遮蔽不全——遮蔽目标始终覆盖到值的首段。
pub fn redact_output(text: &str) -> String {
    text.lines()
        .map(redact_output_line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// 单行输出脱敏：先处理 mysql IDENTIFIED 子句（字符串级），再做 token 级
/// `KEY=value` 扫描（保持原空白，不折叠多空格）。
fn redact_output_line(line: &str) -> String {
    let subject = redact_mysql_identified(line);
    let chars: Vec<char> = subject.chars().collect();
    let mut out = String::with_capacity(subject.len());
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            out.push(c);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        let token: String = chars[start..i].iter().collect();
        if let Some(eq) = token.find('=') {
            let name = &token[..eq];
            // 只遮非空值：`KEY=` 后面本就什么都没有，遮了反而制造「这里曾有值」的暗示
            if !name.is_empty() && is_sensitive_env_name(name) && token.len() > eq + 1 {
                out.push_str(name);
                out.push('=');
                out.push_str(MASK);
                continue;
            }
        }
        out.push_str(&token);
    }
    out
}

/// 遮蔽 mysql `IDENTIFIED [WITH …] BY '<明文>'` / `IDENTIFIED [WITH …] AS '<哈希>'`
/// 引号内的内容。键词大小写不敏感（show create user 输出恒大写，手写 SQL 常小写）。
fn redact_mysql_identified(line: &str) -> String {
    // to_ascii_lowercase 逐字节映射，字节长度与原文一致，下标可互通
    let lower = line.to_ascii_lowercase();
    let Some(id_end) = lower.find("identified").map(|p| p + "identified".len()) else {
        return line.to_string();
    };
    // 只在 IDENTIFIED 之后 80 字节内找 BY/AS：隔太远的多半不是同一条子句。
    // 窗口右界必须回退到字符边界——id_end+80 是任意字节偏移，落在 UTF-8
    // 多字节序列中间时切片直接 panic（多角色审查第 1 轮 Issue 1：MySQL 8
    // `IDENTIFIED BY '…' COMMENT '<中文>'` 的合法输出即可触发，远端输出
    // 打崩 MCP 请求任务 = 拒绝服务面）。
    let mut window_end = (id_end + 80).min(lower.len());
    while window_end < lower.len() && !lower.is_char_boundary(window_end) {
        window_end += 1;
    }
    let window = &lower[id_end..window_end];
    let Some(rel) = window.find(" by ").or_else(|| window.find(" as ")) else {
        return line.to_string();
    };
    let bytes = line.as_bytes();
    // 跳过 BY/AS 关键字（含两侧空格之一）与其后的空白
    let mut pos = id_end + rel + 3;
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    // 可选 PASSWORD 关键字（5.7 的 IDENTIFIED BY PASSWORD '*hash…'）
    if lower[pos..].starts_with("password") {
        pos += "password".len();
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
    }
    if pos >= bytes.len() {
        return line.to_string();
    }
    let quote = bytes[pos];
    if quote != b'\'' && quote != b'"' {
        return line.to_string();
    }
    let Some(close) = lower[pos + 1..].find(quote as char).map(|r| pos + 1 + r) else {
        return line.to_string();
    };
    let mut out = String::with_capacity(line.len());
    out.push_str(&line[..pos + 1]);
    out.push_str(MASK);
    out.push_str(&line[close..]);
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

    #[test]
    fn excerpt_masks_declared_secrets() {
        let out = redact_excerpt("error=bad_verification_code&device_code=abc123XYZ", &["abc123XYZ"], 200);
        assert!(!out.contains("abc123XYZ"), "设备码泄露: {out}");
        assert!(out.contains(MASK));
        assert!(out.contains("bad_verification_code"), "非敏感诊断信息应保留: {out}");
    }

    #[test]
    fn excerpt_truncates_by_chars_not_bytes() {
        // 多字节字符上按字节切会 panic：这里必须按字符数截断且不 panic
        let text = "中文错误页".repeat(20);
        let out = redact_excerpt(&text, &[], 5);
        assert_eq!(out.chars().count(), 5);
        assert_eq!(out, "中文错误页");
    }

    #[test]
    fn excerpt_ignores_empty_secret() {
        // 空秘密串不能参与替换（否则每个字符之间都会被插入标记）
        let out = redact_excerpt("abc", &[""], 10);
        assert_eq!(out, "abc");
    }

    #[test]
    fn output_env_assignment_is_masked() {
        let out = redact_output("GITHUB_TOKEN=ghp_abc123secret PATH=/usr/bin");
        assert!(!out.contains("ghp_abc123secret"), "token 泄露: {out}");
        assert!(out.contains("GITHUB_TOKEN=<redacted>"));
        // 非敏感键不受影响
        assert!(out.contains("PATH=/usr/bin"));
    }

    #[test]
    fn output_multiple_sensitive_assignments_same_line() {
        let out = redact_output("DB_PASSWORD=hunter2   MY_API_KEY=sk-123 host=db1");
        assert!(!out.contains("hunter2") && !out.contains("sk-123"), "泄露: {out}");
        assert!(out.contains("DB_PASSWORD=<redacted>"));
        assert!(out.contains("MY_API_KEY=<redacted>"));
        // 多空格原样保留
        assert!(out.contains("<redacted>   MY_API_KEY"));
        assert!(out.contains("host=db1"));
    }

    #[test]
    fn output_lowercase_and_empty_assignments_untouched() {
        // 小写参数赋值（`host=db`）不参与判定；`KEY=` 空值不遮（无值可泄）
        let out = redact_output("host=db TOKEN= password=");
        assert_eq!(out, "host=db TOKEN= password=");
    }

    #[test]
    fn output_mysql_identified_by_plain_is_masked() {
        let line = "CREATE USER 'app'@'%' IDENTIFIED BY 'P@ssw0rd!';";
        let out = redact_output(line);
        assert!(!out.contains("P@ssw0rd!"), "明文口令泄露: {out}");
        assert!(out.contains("IDENTIFIED BY '<redacted>'"), "应保留子句结构: {out}");
    }

    #[test]
    fn output_mysql_identified_variants_are_masked() {
        // BY PASSWORD（5.7 哈希）/ WITH plugin BY / WITH plugin AS（8.0 哈希）/ 小写
        for line in [
            "IDENTIFIED BY PASSWORD '*C5B1B1A1E1E1E1E1E1E1E1E1E1E1E1'",
            "IDENTIFIED WITH mysql_native_password BY 'plainsecret'",
            "IDENTIFIED WITH caching_sha2_password AS '$A$005$HASHVALUE'",
            "identified by 'low'",
        ] {
            let out = redact_output(line);
            assert!(!out.contains("plainsecret"));
            assert!(!out.contains("HASHVALUE"));
            assert!(!out.contains("C5B1B1"));
            assert!(!out.contains("low'"), "行 {line} 未脱敏: {out}");
        }
    }

    #[test]
    fn output_order_by_not_affected() {
        // 普通 ORDER BY 'x' 无 IDENTIFIED 前缀，不误伤
        let out = redact_output("SELECT * FROM t ORDER BY 'name' LIMIT 1");
        assert_eq!(out, "SELECT * FROM t ORDER BY 'name' LIMIT 1");
    }

    #[test]
    fn output_multiline_each_line_handled() {
        let text = "HOME=/root\nGITHUB_TOKEN=ghp_leak\nmysql: IDENTIFIED BY 'leak'";
        let out = redact_output(text);
        assert!(out.contains("HOME=/root"));
        assert!(!out.contains("ghp_leak") && !out.contains("'leak'"));
        assert_eq!(out.matches("<redacted>").count(), 2);
    }

    #[test]
    fn output_multibyte_window_boundary_does_not_panic() {
        // 多角色审查第 1 轮 Issue 1 的回归用例：窗口右界 id_end+80 落在中文
        // COMMENT 的 UTF-8 序列中间时，旧实现按任意字节偏移切片直接 panic
        //（MySQL 8 show create user 对带中文备注用户的真实输出形态）。
        let filler = "字".repeat(60);
        let line = format!("CREATE USER 'u'@'%' IDENTIFIED BY 'abcdef' COMMENT '{filler}'");
        let out = redact_output(&line);
        assert!(!out.contains("abcdef"), "口令未脱敏: {out}");
        assert!(out.contains("IDENTIFIED BY '<redacted>'"));
        // WITH 子句里只有多字节串、无引号值 → 不误遮，也不 panic
        let safe = redact_output(&format!("IDENTIFIED WITH plugin {filler}"));
        assert!(!safe.contains(MASK), "无引号值不应误遮: {safe}");
    }

}
