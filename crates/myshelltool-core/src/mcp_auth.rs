//! MCP HTTP 入口鉴权判定（MCP服务设计-v3 §3.1，阶段 A1）。
//!
//! ## 背景
//!
//! MCP server 内嵌 GUI 进程、监听 127.0.0.1。但「只监听回环」在多进程桌面上
//! 不是安全边界：本机任意进程（含同一台机器上其他 Windows 用户的进程）都能
//! 向该端口发请求，而工具面含 `ssh_exec`——等于本机任意进程可遥控全部已托管
//! 服务器。A1 给入口加 token 鉴权：URL 路径内嵌（`/mcp/<token>`）为主，
//! `Authorization: Bearer` header 为辅。
//!
//! ## 为什么判定逻辑在 core
//!
//! 安全判据必须真跑测试（AGENTS.md §9：src-tauri 的测试二进制缺 Tauri runtime
//! DLL 跑不起来，`cargo check --tests` 只能验编译）。本模块是纯字符串判定，
//! 在 core 里接受穷举式单测——特别是**段边界**（`/mcp/<token>evil` 不得命中）
//! 与 **fail-closed**（token 为空 = 全拒，绝不裸奔）两条安全性质。
//!
//! ## 与 server 侧的契约
//!
//! - `decide` 只看 `uri.path()`（不含 query），bearer 只认 `Authorization: Bearer <t>`；
//! - 拒绝语义统一 401：不区分「路径不存在」与「token 错」，不做探测预言机；
//! - token 不出现在日志（凭据红线延伸）：拒绝时只记请求路径的**段数与首段**，
//!   由调用方（http_server.rs 中间件）落实。

/// 鉴权判定结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpAuthDecision {
    /// 放行（Bearer header 形式，URI 不动）。
    Allow,
    /// 放行且把路径重写为给定值（URL 内嵌 token 形式剥掉 token 段再进路由）。
    AllowRewrite(String),
    /// 拒绝（调用方统一返回 401）。
    Deny,
}

/// 常量时间字符串比较：防计时侧信道（localhost 威胁模型下要求不高，但实现成本近零）。
/// 长度不同不短路返回——用长度差折叠进累加器，避免「逐字符早退」泄漏前缀长度。
pub fn token_eq(a: &str, b: &str) -> bool {
    let (x, y) = (a.as_bytes(), b.as_bytes());
    let mut acc = (x.len() ^ y.len()) as u8;
    let n = x.len().max(y.len());
    for i in 0..n {
        acc |= x.get(i % x.len().max(1)).copied().unwrap_or(0)
            ^ y.get(i % y.len().max(1)).copied().unwrap_or(0);
    }
    acc == 0 && !x.is_empty()
}

/// 判定一次 HTTP 请求是否可进入 MCP 服务。
///
/// - `path`：`uri.path()`（不含 query）；`bearer`：Bearer header 值（无则 None）。
/// - `token` 为空串 = **fail-closed 全拒**（配置/锁损坏时宁可服务不可用）。
pub fn decide(path: &str, bearer: Option<&str>, token: &str) -> McpAuthDecision {
    if token.is_empty() {
        return McpAuthDecision::Deny;
    }
    // 只服务 /mcp 前缀（段边界：/mcp 本身或 /mcp/...；/mcpx 之类不命中）
    let under_mcp = path == "/mcp" || path.starts_with("/mcp/");
    if !under_mcp {
        return McpAuthDecision::Deny;
    }
    // 路径形式：/mcp/<token>[/rest] —— seg 必须整段相等（前缀碰撞拒）
    if let Some(rest) = path.strip_prefix("/mcp/") {
        let (seg, tail) = rest.split_once('/').unwrap_or((rest, ""));
        if token_eq(seg, token) {
            let rewritten = if tail.is_empty() {
                "/mcp".to_string()
            } else {
                format!("/mcp/{tail}")
            };
            return McpAuthDecision::AllowRewrite(rewritten);
        }
    }
    // Bearer 形式：header 值与 token 全等（路径不再含 token 段——路径形式的
    // 段校验失败已经落到这里，bearer 对错 token 路径 + 对 header 的组合放行无害：
    // 持合法 bearer 者本就已被授权）
    if let Some(b) = bearer {
        if token_eq(b, token) {
            return McpAuthDecision::Allow;
        }
    }
    McpAuthDecision::Deny
}

/// 401 日志用的路径脱敏摘要：只保留段数与首段（/mcp/<猜错的 token> 整体不落盘）。
/// 返回形如 `"mcp (3 段)"`；非 /mcp 路径返回首段。
pub fn redacted_path_summary(path: &str) -> String {
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match segs.first() {
        Some(first) => format!("{first} ({} 段)", segs.len()),
        None => "(根路径)".to_string(),
    }
}

/// base64url（无 padding）编码（RFC 4648 §5）。token 上 URL 路径用，
/// 不引 base64 crate——编码表 + 位拼装约 20 行，单测锁 RFC 测试向量。
pub fn base64url_nopad(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 63] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "kX9v2mQ7aB3cD4eF5gH6jJ8kL9mN0pQ1rS2tU3vW4xY";

    // ── decide：路径形式 ──
    #[test]
    fn path_form_exact_token_allows_and_rewrites() {
        let d = decide(&format!("/mcp/{TOKEN}"), None, TOKEN);
        assert_eq!(d, McpAuthDecision::AllowRewrite("/mcp".to_string()));
    }

    #[test]
    fn path_form_with_tail_preserves_tail() {
        let d = decide(&format!("/mcp/{TOKEN}/sse"), None, TOKEN);
        assert_eq!(d, McpAuthDecision::AllowRewrite("/mcp/sse".to_string()));
    }

    #[test]
    fn path_form_wrong_token_denied() {
        let d = decide("/mcp/wrong-token-value", None, TOKEN);
        assert_eq!(d, McpAuthDecision::Deny);
    }

    #[test]
    fn path_form_prefix_collision_denied() {
        // 段边界是安全性质：token 是另一段的前缀也不得放行
        let d = decide(&format!("/mcp/{TOKEN}evil"), None, TOKEN);
        assert_eq!(d, McpAuthDecision::Deny);
        // token 比该段长同样拒（段必须全等）
        let d2 = decide("/mcp/kX9v2", None, TOKEN);
        assert_eq!(d2, McpAuthDecision::Deny);
    }

    #[test]
    fn no_token_at_all_denied() {
        assert_eq!(decide("/mcp", None, TOKEN), McpAuthDecision::Deny);
        assert_eq!(decide("/mcp/", None, TOKEN), McpAuthDecision::Deny);
    }

    // ── decide：Bearer 形式 ──
    #[test]
    fn bearer_form_allows_base_and_subpath() {
        assert_eq!(decide("/mcp", Some(TOKEN), TOKEN), McpAuthDecision::Allow);
        assert_eq!(decide("/mcp/", Some(TOKEN), TOKEN), McpAuthDecision::Allow);
    }

    #[test]
    fn bearer_wrong_denied() {
        assert_eq!(decide("/mcp", Some("nope"), TOKEN), McpAuthDecision::Deny);
    }

    // ── decide：边界与 fail-closed ──
    #[test]
    fn non_mcp_path_denied_even_with_valid_bearer() {
        // 探测面收敛：/evil 拿到合法 bearer 也不服务（middleware 统一 401）
        assert_eq!(decide("/evil", Some(TOKEN), TOKEN), McpAuthDecision::Deny);
        assert_eq!(decide("/mcpx", Some(TOKEN), TOKEN), McpAuthDecision::Deny);
        assert_eq!(decide("/", None, TOKEN), McpAuthDecision::Deny);
    }

    #[test]
    fn empty_token_fails_closed() {
        // 配置损坏/锁中毒 → 空 token → 全部拒绝，绝不裸奔
        assert_eq!(decide("/mcp", Some(""), ""), McpAuthDecision::Deny);
        assert_eq!(decide("/mcp/", None, ""), McpAuthDecision::Deny);
        assert_eq!(decide("/mcp/x", Some("x"), ""), McpAuthDecision::Deny);
    }

    // ── token_eq ──
    #[test]
    fn token_eq_semantics() {
        assert!(token_eq("abc", "abc"));
        assert!(!token_eq("abc", "abd"));
        assert!(!token_eq("abc", "abcd"));
        assert!(!token_eq("abcd", "abc"));
        assert!(!token_eq("", "")); // 空串永不匹配（配合 fail-closed）
        assert!(!token_eq("", "a"));
        assert!(!token_eq("a", ""));
    }

    // ── redacted_path_summary ──
    #[test]
    fn path_summary_never_contains_token() {
        let s = redacted_path_summary(&format!("/mcp/{TOKEN}"));
        assert!(!s.contains(TOKEN));
        assert_eq!(s, "mcp (2 段)");
        assert_eq!(redacted_path_summary("/"), "(根路径)");
        assert_eq!(redacted_path_summary("/evil/x"), "evil (2 段)");
    }

    // ── base64url_nopad：RFC 4648 测试向量 ──
    #[test]
    fn base64url_rfc4648_vectors() {
        assert_eq!(base64url_nopad(b""), "");
        assert_eq!(base64url_nopad(b"f"), "Zg");
        assert_eq!(base64url_nopad(b"fo"), "Zm8");
        assert_eq!(base64url_nopad(b"foo"), "Zm9v");
        assert_eq!(base64url_nopad(b"foob"), "Zm9vYg");
        assert_eq!(base64url_nopad(b"fooba"), "Zm9vYmE");
        assert_eq!(base64url_nopad(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64url_url_safe_alphabet() {
        // 0xfb 0xff 0xfe → 标准 base64 为 "+//+"，url-safe 必须 "-__-"
        assert_eq!(base64url_nopad(&[0xfb, 0xff, 0xfe]), "-__-");
        // 32 字节 token → 43 字符（无 padding）
        assert_eq!(base64url_nopad(&[7u8; 32]).len(), 43);
    }
}
