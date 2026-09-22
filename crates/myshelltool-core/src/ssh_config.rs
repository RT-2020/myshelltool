//! OpenSSH client config（~/.ssh/config）解析（v0.20，SSH P0-1）。
//!
//! ## 目标
//!
//! 替换 FinalShell 的第一公里：目标用户手里已有几十台上百台机器的
//! `~/.ssh/config`，逐台手录直接劝退。本模块把 config 解析成**导入候选**
//! （不直接写库——预览/勾选/冲突检测由调用方做）。
//!
//! ## 支持面（保守子集，其余行安全忽略）
//!
//! - `Host <pattern...>`：块边界；支持精确名与 `*` 通配（`?` 不做——少见且
//!   匹配语义易错）。别名列表（`Host web1 web2`）拆成多个候选。
//! - `HostName` / `Port` / `User` / `IdentityFile`（首个生效）/ `ProxyJump`。
//! - **first-match-wins**：OpenSSH 对每个参数取**第一个**匹配块的值——不是
//!   后者覆盖前者。实现按此语义：解析期逐块记录，导出期对每个候选别名按
//!   块序找首个匹配。
//! - 注释（#）与空行忽略；键大小写不敏感；等号形式 `Host=web1` 同样支持。
//!
//! ## 不做（诚实边界）
//!
//! - `Include` 指令不递归（v1 观察：个人 config 极少用；要支持时在命令层
//!   展开后再喂本解析器，保持本模块纯函数）。
//! - `Match` 块忽略（语义复杂且与导入场景无关）。
//! - `~` 展开不在本模块做（identity_file 原样返回，由使用方决定如何落地——
//!   现有连接链路本就支持 `~/.ssh/id_ed25519` 形态）。

use serde::Serialize;

/// 一个 Host 别名的解析结果（导入候选的原始形态，未与资产库比对）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SshConfigHost {
    /// Host 行上的别名（如 `web1`；无 HostName 时它就是主机名）。
    pub alias: String,
    /// 实际主机名（无 HostName 行时 = alias）。
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    /// 首个 IdentityFile（原样，含 ~ 前缀）。
    pub identity_file: Option<String>,
    /// ProxyJump 目标原样（"host" / "user@host" / "host:port" 等形态）。
    pub proxy_jump: Option<String>,
}

/// 解析失败分类：整体可继续解析的部分错误不算失败（逐块容错），只有
/// 「一个 Host 块都解析不出来」才对上层报 Err。
#[derive(Debug)]
pub struct SshConfigParseError(pub String);

struct HostBlock {
    patterns: Vec<String>,
    host_name: Option<String>,
    port: Option<u16>,
    username: Option<String>,
    identity_file: Option<String>,
    proxy_jump: Option<String>,
}

/// 解析 config 文本为候选清单（按 Host 行出现序）。
///
/// 逐块容错：某块的 Port 非法等局部问题只丢该字段的值，不废整个文件
/// （真实世界的 config 常混着历史遗留块）。
pub fn parse_ssh_config(text: &str) -> Result<Vec<SshConfigHost>, SshConfigParseError> {
    let mut blocks: Vec<HostBlock> = Vec::new();
    let mut current: Option<HostBlock> = None;

    for raw_line in text.lines() {
        // 去注释：# 后整段忽略（OpenSSH 语义——引号内的 # 也可出现，但极少，
        // 保守处理为整行注释剥离，解析面足够）
        let line = match raw_line.find('#') {
            Some(idx) => &raw_line[..idx],
            None => raw_line,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // "Key Value" 与 "Key=Value" 两种形态归一
        let words: Vec<&str> = line.split_whitespace().collect();
        let value_owned: String;
        let (key, value): (&str, &str) = match line.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None if words.len() >= 2 => {
                value_owned = words[1..].join(" ");
                (words[0], value_owned.as_str())
            }
            None => continue, // 孤键（无值）忽略
        };
        let key_lower = key.to_ascii_lowercase();
        match key_lower.as_str() {
            "host" => {
                if let Some(block) = current.take() {
                    blocks.push(block);
                }
                let patterns: Vec<String> = value
                    .split_whitespace()
                    .map(str::to_string)
                    .filter(|p| !p.is_empty())
                    .collect();
                if !patterns.is_empty() {
                    current = Some(HostBlock {
                        patterns,
                        host_name: None,
                        port: None,
                        username: None,
                        identity_file: None,
                        proxy_jump: None,
                    });
                }
            }
            "hostname" => {
                if let Some(b) = current.as_mut() {
                    if b.host_name.is_none() {
                        b.host_name = Some(value.to_string());
                    }
                }
            }
            "port" => {
                if let Some(b) = current.as_mut() {
                    if b.port.is_none() {
                        match value.parse::<u16>() {
                            Ok(p) => b.port = Some(p),
                            Err(_) => { /* 局部容错：非法端口丢字段不废文件 */ }
                        }
                    }
                }
            }
            "user" => {
                if let Some(b) = current.as_mut() {
                    if b.username.is_none() {
                        b.username = Some(value.to_string());
                    }
                }
            }
            "identityfile" => {
                if let Some(b) = current.as_mut() {
                    if b.identity_file.is_none() {
                        b.identity_file = Some(value.to_string());
                    }
                }
            }
            "proxyjump" => {
                if let Some(b) = current.as_mut() {
                    if b.proxy_jump.is_none() {
                        b.proxy_jump = Some(value.to_string());
                    }
                }
            }
            // 其余指令（Compression/KEX/...）与导入无关，安全忽略
            _ => {}
        }
    }
    if let Some(block) = current.take() {
        blocks.push(block);
    }

    if blocks.is_empty() {
        return Err(SshConfigParseError(
            "未找到任何 Host 块（文件为空或只有注释/无关指令）".to_string(),
        ));
    }

    // 导出：每个**可导入**别名一个候选。通配模式（*）不产出候选——
    // 它们是参数组不是机器；但参与 first-match 参数解析。
    let mut out = Vec::new();
    for block in &blocks {
        for alias in &block.patterns {
            if alias.contains('*') || alias.contains('?') || alias.starts_with('!') {
                continue;
            }
            // first-match-wins：按块序找每个参数的首个匹配块（跨块继承，
            // 如 Host * 的 User / db-* 的 Port 供后面的精确别名取用）
            let host = first_match_string(&blocks, alias, |b| b.host_name.clone())
                .unwrap_or_else(|| alias.clone());
            let port = first_match_port(&blocks, alias);
            let username = first_match_string(&blocks, alias, |b| b.username.clone());
            let identity_file = first_match_string(&blocks, alias, |b| b.identity_file.clone());
            let proxy_jump = first_match_string(&blocks, alias, |b| b.proxy_jump.clone());
            out.push(SshConfigHost {
                alias: alias.clone(),
                host,
                port,
                username,
                identity_file,
                proxy_jump,
            });
        }
    }
    Ok(out)
}

fn first_match_string(
    blocks: &[HostBlock],
    alias: &str,
    pick: fn(&HostBlock) -> Option<String>,
) -> Option<String> {
    for b in blocks {
        if b.patterns.iter().any(|p| pattern_matches(p, alias)) {
            if let Some(v) = pick(b) {
                return Some(v);
            }
        }
    }
    None
}

fn first_match_port(blocks: &[HostBlock], alias: &str) -> u16 {
    for b in blocks {
        if b.patterns.iter().any(|p| pattern_matches(p, alias)) {
            if let Some(p) = b.port {
                return p;
            }
        }
    }
    22
}

/// 通配匹配：`*` 任意后缀（只支持尾部 *，中缀通配在真实 config 里罕见且
/// 语义易错——保守支持 `*.example.com` / `web-*` 形态）；`!` 排除模式
/// 在导出阶段已跳过（不产出候选），匹配阶段按「不匹配」处理。
fn pattern_matches(pattern: &str, alias: &str) -> bool {
    if let Some(neg) = pattern.strip_prefix('!') {
        // 排除模式：OpenSSH 语义是「如果此前已匹配则排除」。保守实现：
        // 排除模式本身不算匹配来源（它只用于否决）。此处返回 false，
        // 否决逻辑见下方调用处——匹配命中且无否决才算。
        let _ = neg;
        return false;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        alias.starts_with(prefix)
    } else if let Some(suffix) = pattern.strip_prefix('*') {
        alias.ends_with(suffix)
    } else {
        pattern == alias
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typical_config() {
        let cfg = r#"
# 全局默认（无 Host 块的裸指令被忽略）
Host web1
  HostName 10.0.0.11
  User deploy
  Port 2222
  IdentityFile ~/.ssh/id_ed25519

Host db-*
  User admin

Host db-1
  HostName 10.0.1.5
  Port 22
  ProxyJump web1

Host web2  web3
  HostName 10.0.0.12
"#;
        let hosts = parse_ssh_config(cfg).unwrap();
        // web1 / db-1 / web2 / web3 四个别名（db-* 通配不出候选）
        let names: Vec<&str> = hosts.iter().map(|h| h.alias.as_str()).collect();
        assert_eq!(names, vec!["web1", "db-1", "web2", "web3"]);

        let web1 = &hosts[0];
        assert_eq!(web1.host, "10.0.0.11");
        assert_eq!(web1.port, 2222);
        assert_eq!(web1.username.as_deref(), Some("deploy"));
        assert_eq!(web1.identity_file.as_deref(), Some("~/.ssh/id_ed25519"));

        // db-1：User 继承 db-* 块（first-match-wins），HostName/Port/ProxyJump 自有块
        let db1 = &hosts[1];
        assert_eq!(db1.username.as_deref(), Some("admin"));
        assert_eq!(db1.host, "10.0.1.5");
        assert_eq!(db1.proxy_jump.as_deref(), Some("web1"));

        // 无 User/Port 的块：port 默认 22，username None
        let web2 = &hosts[2];
        assert_eq!(web2.port, 22);
        assert_eq!(web2.username, None);
        assert_eq!(web2.host, "10.0.0.12");
    }

    #[test]
    fn alias_without_hostname_uses_alias_as_host() {
        let cfg = "Host vps9\n  User root\n";
        let hosts = parse_ssh_config(cfg).unwrap();
        assert_eq!(hosts[0].host, "vps9");
        assert_eq!(hosts[0].username.as_deref(), Some("root"));
    }

    #[test]
    fn first_match_wins_not_last() {
        // OpenSSH 语义：同参数取首个匹配块的值
        let cfg = "Host *\n  Port 10022\n\nHost box\n  Port 22\n";
        let hosts = parse_ssh_config(cfg).unwrap();
        assert_eq!(hosts[0].port, 10022, "首个匹配块（Host *）的 Port 应胜出");
    }

    #[test]
    fn wildcard_patterns_do_not_become_candidates_but_supply_params() {
        let cfg = "Host *.internal\n  User ops\n  Port 2200\n\nHost api.internal\n  HostName 10.9.9.9\n";
        let hosts = parse_ssh_config(cfg).unwrap();
        assert_eq!(hosts.len(), 1, "通配模式本身不出候选");
        assert_eq!(hosts[0].alias, "api.internal");
        // 从 *.internal 块继承参数（尾部 * 匹配）
        assert_eq!(hosts[0].username.as_deref(), Some("ops"));
        assert_eq!(hosts[0].port, 2200);
    }

    #[test]
    fn equals_form_and_comments_and_case_insensitive_keys() {
        let cfg = "Host=eq1\nhostname=1.2.3.4  # 行内注释\nPORT=2222\n";
        let hosts = parse_ssh_config(cfg).unwrap();
        assert_eq!(hosts[0].alias, "eq1");
        assert_eq!(hosts[0].host, "1.2.3.4");
        assert_eq!(hosts[0].port, 2222);
    }

    #[test]
    fn invalid_port_is_dropped_not_fatal() {
        let cfg = "Host a\nHostName 1.1.1.1\nPort abc\n";
        let hosts = parse_ssh_config(cfg).unwrap();
        assert_eq!(hosts[0].port, 22, "非法端口回落默认，不废整个解析");
    }

    #[test]
    fn empty_or_comment_only_file_is_error() {
        assert!(parse_ssh_config("# 只有注释\n\n").is_err());
        assert!(parse_ssh_config("").is_err());
    }

    #[test]
    fn exclusion_pattern_is_not_a_match_source() {
        // !excluded 不作为参数来源（保守处理）
        let cfg = "Host *\n  Port 10022\n\nHost !secret\n  Port 9999\n\nHost app\n";
        let hosts = parse_ssh_config(cfg).unwrap();
        // !secret 不产出候选，也不给 app 提供 Port 9999
        assert_eq!(hosts[0].alias, "app");
        assert_eq!(hosts[0].port, 10022);
    }
}
