//! v0.21（B3）ProxyJump 链解析内核：跳板标识解析 + 链展开 + 循环检测 +
//! 深度上限，纯函数。原单跳版在 src-tauri/src/ssh/jump.rs，链化时迁入 core
//! 使 `cargo test` 真正执行（src-tauri 测试二进制因 Tauri runtime DLL 缺失
//! 跑不起来，正确性判据不可测——与 dangerous_commands 迁入同纪律）。
//!
//! 链的表达：只由资产嵌套构成（target.jump_host=j1，j1 资产.jump_host=j2，
//! 最内层跳板 jump_host 为空）。jump_host 字段不支持逗号链（"j1,j2"）——
//! 链的每一跳必须是资产库成员（复用其凭据/host key 信任），单一权威表达。

use std::collections::HashSet;

use crate::asset_store::ConnectionAsset;

/// 链上跳板数上限（4 台；超限整链拒绝——防配置失控与连接数放大）。
pub const MAX_JUMP_HOPS: usize = 4;

/// 解析跳板标识 "host" / "host:port" → (host, port)。
/// 尾部 :port 非法（非数字/超范围）报错——不静默当 22（配置错误要暴露）。
pub fn parse_jump_host(jump_host: &str) -> Result<(String, u16), String> {
    let s = jump_host.trim();
    if s.is_empty() {
        return Err("跳板主机标识为空".to_string());
    }
    if s.contains('@') || s.contains('/') {
        return Err(format!(
            "跳板主机标识 '{s}' 不支持 user@host / 路径形态（请填 host 或 host:port；跳板登录用户取资产库中该主机的资产）"
        ));
    }
    match s.rsplit_once(':') {
        Some((h, p)) => {
            let port: u16 = p
                .parse()
                .map_err(|_| format!("跳板端口非法（'{p}'，期望 1-65535）"))?;
            if port == 0 {
                return Err("跳板端口不能为 0".to_string());
            }
            Ok((h.to_string(), port))
        }
        None => Ok((s.to_string(), 22)),
    }
}

/// 资产的有效跳板（trim + 空串归 None，导入/旧数据防御）。
pub fn jump_host_of(asset: &ConnectionAsset) -> Option<&str> {
    asset
        .jump_host
        .as_deref()
        .map(str::trim)
        .filter(|j| !j.is_empty())
}

/// 链描述（错误信息/日志共用）：流量方向 target→j1→j2。
pub fn describe_chain(target_host: &str, target_port: u16, hops: &[&ConnectionAsset]) -> String {
    let mut s = format!("{target_host}:{target_port}");
    for a in hops {
        s.push_str(&format!("→{}:{}", a.host, a.port));
    }
    s
}

/// 展开跳板链：从目标的 jump_host 出发，沿每跳资产的 jump_host 走到头，
/// 返回连接顺序的跳板资产 [j1, j2, …]（j1 = 本机直连的最外层跳板）。
///
/// 资产匹配：host+port 全匹配；同主机多账号取第一个（跳板登录用户即该资产
/// 的 username，与既有单跳语义一致）。
///
/// fail-closed 判据（拒绝整链，不静默降级）：
/// - 环：链内 host:port 重复，或任一跳 == 目标主机（target→j1→target 的
///   配置错误形态）；
/// - 深度：跳板数超过 [`MAX_JUMP_HOPS`]；
/// - 某跳不在资产库 / 标识形态非法（parse_jump_host 判）。
pub fn resolve_jump_chain<'a>(
    assets: &'a [ConnectionAsset],
    first_jump: &str,
    target_host: &str,
    target_port: u16,
) -> Result<Vec<&'a ConnectionAsset>, String> {
    let mut hops: Vec<&ConnectionAsset> = Vec::new();
    // 目标自身入 seen：任一跳指回目标 = 环（direct-tcpip 回环是配置错误）
    let mut seen: HashSet<(String, u16)> = HashSet::from([(target_host.to_string(), target_port)]);
    let mut cursor = first_jump.to_string();

    loop {
        let (host, port) = parse_jump_host(&cursor)?;
        if hops.len() >= MAX_JUMP_HOPS {
            return Err(format!(
                "跳板链 {}→… 超过 {} 跳上限（当前已 {} 跳仍要继续）——请检查各资产 jump_host 配置",
                describe_chain(target_host, target_port, &hops),
                MAX_JUMP_HOPS,
                hops.len()
            ));
        }
        if !seen.insert((host.clone(), port)) {
            return Err(format!(
                "跳板链 {} 检测到环（{host}:{port} 在路径上重复出现，含指回目标）——请检查各资产 jump_host 配置",
                describe_chain(target_host, target_port, &hops)
            ));
        }
        let asset = assets
            .iter()
            .find(|a| a.host == host && a.port == port)
            .ok_or_else(|| {
                format!(
                    "跳板链 {} 中转主机 {host}:{port} 不在资产库中——请先添加该主机并完成一次连接（host key 信任），再启用跳板",
                    describe_chain(target_host, target_port, &hops)
                )
            })?;
        hops.push(asset);
        match jump_host_of(asset) {
            Some(next) => cursor = next.to_string(),
            None => return Ok(hops),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_store::{AuthMethod, ConnectionStatus};

    fn asset(id: &str, host: &str, port: u16, jump: Option<&str>) -> ConnectionAsset {
        ConnectionAsset {
            id: id.to_string(),
            name: id.to_string(),
            host: host.to_string(),
            port,
            username: "root".to_string(),
            auth_method: AuthMethod::Password,
            private_key_path: None,
            group: String::new(),
            tags: vec![],
            status: ConnectionStatus::Idle,
            last_connected: String::new(),
            credential_id: None,
            passphrase_credential_id: None,
            private_key_credential_id: None,
            jump_host: jump.map(str::to_string),
            connect_timeout_secs: None,
            keepalive_interval_secs: None,
        }
    }

    #[test]
    fn parses_host_and_host_port() {
        assert_eq!(parse_jump_host("j1").unwrap(), ("j1".into(), 22));
        assert_eq!(parse_jump_host(" j1:2222 ").unwrap(), ("j1".into(), 2222));
        assert!(parse_jump_host("").is_err());
        assert!(parse_jump_host("u@j1").is_err());
        assert!(parse_jump_host("j1/not").is_err());
        assert!(parse_jump_host("j1:abc").is_err());
        assert!(parse_jump_host("j1:0").is_err());
        assert!(parse_jump_host("j1:70000").is_err());
    }

    #[test]
    fn resolves_single_and_double_hop() {
        let assets = vec![
            asset("t", "target", 22, Some("j1.example")),
            asset("j1", "j1.example", 22, Some("j2.example:2222")),
            asset("j2", "j2.example", 2222, None),
        ];
        let chain = resolve_jump_chain(&assets, "j1.example", "target", 22).unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].id, "j1");
        assert_eq!(chain[1].id, "j2"); // host:port 形态解析 + 库内精确匹配
    }

    #[test]
    fn rejects_self_and_mutual_loops() {
        let self_loop = vec![asset("t", "target", 22, Some("j1")), asset("j1", "j1", 22, Some("j1"))];
        let err = resolve_jump_chain(&self_loop, "j1", "target", 22).unwrap_err();
        assert!(err.contains("环"), "{err}");

        let mutual = vec![
            asset("t", "target", 22, Some("a")),
            asset("a", "a", 22, Some("b")),
            asset("b", "b", 22, Some("a")),
        ];
        let err = resolve_jump_chain(&mutual, "a", "target", 22).unwrap_err();
        assert!(err.contains("环"), "{err}");
    }

    #[test]
    fn rejects_hop_pointing_back_to_target() {
        // target 的 jump 直接指回 target 自己
        let assets = vec![asset("t", "target", 22, Some("target"))];
        let err = resolve_jump_chain(&assets, "target", "target", 22).unwrap_err();
        assert!(err.contains("环"), "{err}");

        // 链中途经过 target（target→j1→target）
        let assets = vec![
            asset("t", "target", 22, Some("j1")),
            asset("j1", "j1", 22, Some("target")),
        ];
        let err = resolve_jump_chain(&assets, "j1", "target", 22).unwrap_err();
        assert!(err.contains("环"), "{err}");
    }

    #[test]
    fn rejects_over_depth_allows_boundary() {
        // j1→j2→j3→j4 恰好 4 跳：允许
        let mut assets = vec![asset("t", "target", 22, Some("h1"))];
        for i in 1..=4 {
            let next = if i < 4 { Some(format!("h{}", i + 1)) } else { None };
            assets.push(asset(&format!("j{i}"), &format!("h{i}"), 22, next.as_deref()));
        }
        let chain = resolve_jump_chain(&assets, "h1", "target", 22).unwrap();
        assert_eq!(chain.len(), MAX_JUMP_HOPS);

        // j1→…→j5 共 5 跳：拒绝
        let mut assets = vec![asset("t", "target", 22, Some("h1"))];
        for i in 1..=5 {
            let next = if i < 5 { Some(format!("h{}", i + 1)) } else { None };
            assets.push(asset(&format!("j{i}"), &format!("h{i}"), 22, next.as_deref()));
        }
        let err = resolve_jump_chain(&assets, "h1", "target", 22).unwrap_err();
        assert!(err.contains("上限"), "{err}");
    }

    #[test]
    fn rejects_missing_asset_and_bad_identifier() {
        let assets = vec![asset("t", "target", 22, Some("ghost"))];
        let err = resolve_jump_chain(&assets, "ghost", "target", 22).unwrap_err();
        assert!(err.contains("不在资产库"), "{err}");

        let err = resolve_jump_chain(&assets, "u@ghost", "target", 22).unwrap_err();
        assert!(err.contains("不支持"), "{err}");
    }

    #[test]
    fn empty_jump_host_string_is_none() {
        // 导入/旧数据防御：空白串不构成一跳
        let a = asset("j1", "j1", 22, Some("  "));
        assert!(jump_host_of(&a).is_none());
    }
}
