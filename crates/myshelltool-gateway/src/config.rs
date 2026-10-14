//! 网关配置：导出方向加密 bundle、运行方向明文结构、fail-closed 校验、
//! direct-tcpip 目标白名单、加密容器读写。
//!
//! 两个方向的边界：
//! - **导出方向 `GatewayBundle`**（本机向导产出 → 加密文件 → 拷去跳板机）：
//!   含全部秘密（VPS 挂载私钥、网关 host key 私钥）。只存在于加密容器
//!   （Argon2id + AES-256-GCM，复用 `core::crypto`）与跳板机 SecretStore，明文不落盘。
//! - **运行方向 `GatewayRuntimeConfig`**（跳板机 `gateway-config.json`）：零秘密，
//!   敏感项以 SecretStore 固定 id 引用，由 `run::import_bundle` 落位。
//!
//! 校验一律 fail-closed：读不到、解不开、版本不认识、字段非法 → 拒绝并给明确文案，
//! 绝不回落默认值继续跑（AGENTS.md 安全级红线）。

use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use myshelltool_core::crypto::{self, EncryptedBlob};

/// 加密 bundle 的结构版本。读取不认识即拒绝（不猜兼容）。
pub const BUNDLE_VERSION: u32 = 1;
/// 运行方向 gateway-config.json 的结构版本。
pub const RUNTIME_VERSION: u32 = 1;

/// 跳板机 SecretStore 固定 id（DPAPI User scope；谁导入就谁运行，见部署指南）。
pub const SECRET_HOST_KEY: &str = "gateway-host-key";
pub const SECRET_VPS_KEY: &str = "gateway-vps-key";
pub const SECRET_VPS_PASSPHRASE: &str = "gateway-vps-passphrase";

// ─── 导出方向（加密容器内） ───

/// 一份完整的网关引导包：本机向导生成，加密导出，跳板机导入。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct GatewayBundle {
    pub version: u32,
    pub vps: VpsEndpoint,
    pub mount: MountConfig,
    pub gateway: GatewayServerConfig,
    /// 网关 host key 私钥（PEM，OpenSSH 格式）。由本机向导预生成——导出时本机即
    /// 预写 known_hosts，全链路首次连接零弹窗。
    pub host_key_private: String,
}

/// VPS 侧挂载点（sshd，宿主直配或 Docker relay 镜像）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct VpsEndpoint {
    pub host: String,
    /// sshd 控制口（挂载器出站 SSH 连这里；Docker relay 默认 22222）。
    pub port: u16,
    /// 挂载专用账户（Docker relay 镜像内置账户）。
    pub username: String,
    /// 挂载私钥（PEM）——挂载器用它认证 VPS sshd。向导生成的挂载密钥对之私钥侧。
    pub private_key: String,
    /// 挂载私钥口令（向导生成的密钥对通常无口令；Option 保留兼容用户自带私钥）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,
    /// VPS sshd host key pin：不依赖跳板机上的 known_hosts（配置自包含）。
    pub host_key_pin: HostKeyPin,
}

/// host key pin。判据与 known_hosts 同源：`public_key_bytes()` 小写 hex 精确比对。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HostKeyPin {
    /// SSH key 类型名（如 "ssh-ed25519"），仅用于报错指认。
    pub key_type: String,
    pub key_hex: String,
}

/// remote forward 挂载参数。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MountConfig {
    /// 请求 VPS sshd 绑定的地址（默认 "0.0.0.0"——本机要直连挂载口；
    /// 需 VPS sshd `GatewayPorts clientspecified`，Docker relay 镜像已内置）。
    pub bind_addr: String,
    /// 挂载口（VPS 上对外暴露的端口；固定请求，不做动态协商）。
    pub remote_port: u16,
}

/// 内嵌 SSH server 参数。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct GatewayServerConfig {
    /// 回环监听口（挂载器把 VPS 来的连接桥到这里；默认 22222）。
    pub listen_port: u16,
    /// 客户端公钥单行（authorized_keys 格式）——本机 myshelltool 持对应私钥。
    pub authorized_key: String,
    /// direct-tcpip 目标白名单，条目 `CIDR` / `CIDR:port` / `单主机` / `单主机:port`，
    /// 端口可 `*`。**空 = 允许全部**（默认零摩擦；防私钥泄露后横跳全网段时填写）。
    #[serde(default)]
    pub allowed_targets: Vec<String>,
}

// ─── 运行方向（gateway-config.json，零秘密） ───

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct GatewayRuntimeConfig {
    pub version: u32,
    /// VPS 地址与认证账户（私钥在 SecretStore `gateway-vps-key`）。
    pub vps_host: String,
    pub vps_port: u16,
    pub vps_username: String,
    pub host_key_pin: HostKeyPin,
    pub mount: MountConfig,
    pub listen_port: u16,
    pub authorized_key: String,
    #[serde(default)]
    pub allowed_targets: Vec<String>,
}

impl GatewayBundle {
    /// 导入时剥离秘密、降为运行方向结构（秘密另落 SecretStore）。
    pub fn to_runtime(&self) -> GatewayRuntimeConfig {
        GatewayRuntimeConfig {
            version: RUNTIME_VERSION,
            vps_host: self.vps.host.clone(),
            vps_port: self.vps.port,
            vps_username: self.vps.username.clone(),
            host_key_pin: self.vps.host_key_pin.clone(),
            mount: self.mount.clone(),
            listen_port: self.gateway.listen_port,
            authorized_key: self.gateway.authorized_key.clone(),
            allowed_targets: self.gateway.allowed_targets.clone(),
        }
    }
}

// ─── 校验（fail-closed） ───

fn validate_port(port: u16, what: &str) -> Result<(), String> {
    if port == 0 {
        return Err(format!("{what} 不能为 0"));
    }
    Ok(())
}

/// hex 串校验：非空、偶数长度、全小写 hex（对齐 known_hosts 的 bytes_to_hex 产出）。
fn is_lowercase_hex(s: &str) -> bool {
    !s.is_empty() && s.len() % 2 == 0 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// PEM 私钥粗校验：真解析发生在 russh 侧（连接时报错）；这里只挡明显空值/非 PEM，
/// 避免把「凭据缺失」拖到跳板机运行期才炸。
fn validate_pem(key: &str, what: &str) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err(format!("{what} 为空"));
    }
    if !key.contains("PRIVATE KEY") {
        return Err(format!("{what} 不是 PEM 私钥（缺少 PRIVATE KEY 标头）"));
    }
    Ok(())
}

fn validate_host_key_pin(pin: &HostKeyPin) -> Result<(), String> {
    if pin.key_type.trim().is_empty() {
        return Err("host key pin 的 key_type 为空".to_string());
    }
    if !is_lowercase_hex(&pin.key_hex) {
        return Err("host key pin 的 key_hex 不是合法的小写 hex 串".to_string());
    }
    Ok(())
}

fn validate_mount(mount: &MountConfig) -> Result<(), String> {
    if mount.bind_addr.trim().is_empty() {
        return Err("挂载绑定地址（mount.bind_addr）为空".to_string());
    }
    // 绑定地址由 VPS sshd 解释（GatewayPorts），这里只挡空串；非法地址在
    // tcpip_forward 请求时由服务器拒绝并回明确错误。
    validate_port(mount.remote_port, "挂载口（mount.remote_port）")
}

fn validate_server(gateway: &GatewayServerConfig) -> Result<(), String> {
    validate_port(gateway.listen_port, "回环监听口（listen_port）")?;
    if gateway.authorized_key.trim().is_empty() {
        return Err("客户端公钥（authorized_key）为空".to_string());
    }
    for rule in &gateway.allowed_targets {
        TargetRule::parse(rule)?; // 逐条解析，坏条目整份拒绝（不静默跳过）
    }
    Ok(())
}

fn validate_vps_common(host: &str, port: u16, username: &str, pin: &HostKeyPin) -> Result<(), String> {
    if host.trim().is_empty() {
        return Err("VPS 主机（vps.host）为空".to_string());
    }
    validate_port(port, "VPS 端口（vps.port）")?;
    if username.trim().is_empty() {
        return Err("VPS 用户名（vps.username）为空".to_string());
    }
    validate_host_key_pin(pin)
}

impl GatewayBundle {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != BUNDLE_VERSION {
            return Err(format!(
                "配置版本 {} 不被支持（当前支持 {BUNDLE_VERSION}）：请用新版本向导重新导出",
                self.version
            ));
        }
        validate_vps_common(&self.vps.host, self.vps.port, &self.vps.username, &self.vps.host_key_pin)?;
        validate_pem(&self.vps.private_key, "VPS 挂载私钥（vps.private_key）")?;
        if let Some(p) = &self.vps.passphrase {
            if p.is_empty() {
                return Err("vps.passphrase 存在但为空（无口令请置 null）".to_string());
            }
        }
        validate_mount(&self.mount)?;
        validate_server(&self.gateway)?;
        validate_pem(&self.host_key_private, "网关 host key 私钥（host_key_private）")
    }
}

impl GatewayRuntimeConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != RUNTIME_VERSION {
            return Err(format!(
                "运行配置版本 {} 不被支持（当前支持 {RUNTIME_VERSION}）：请重新导出并导入",
                self.version
            ));
        }
        validate_vps_common(&self.vps_host, self.vps_port, &self.vps_username, &self.host_key_pin)?;
        validate_mount(&self.mount)?;
        validate_port(self.listen_port, "回环监听口（listen_port）")?;
        if self.authorized_key.trim().is_empty() {
            return Err("客户端公钥（authorized_key）为空".to_string());
        }
        for rule in &self.allowed_targets {
            TargetRule::parse(rule)?;
        }
        Ok(())
    }
}

// ─── direct-tcpip 目标白名单 ───

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortSpec {
    Any,
    Exact(u16),
}

/// 一条白名单规则：`net`（CIDR 或单主机，IPv4/IPv6）+ 端口限定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetRule {
    pub net: ipnet::IpNet,
    pub port: PortSpec,
}

impl TargetRule {
    /// 解析条目：`CIDR` / `CIDR:port` / `单主机` / `单主机:port`，端口 `*` = 任意。
    ///
    /// 拆分规则（IPv6 地址自带冒号，不能按「首个冒号」切）：取**最后一个**冒号，
    /// 仅当其后是 `*` 或 1-65535 纯数字、且去掉该段的剩余部分能解析为 IpNet 时才
    /// 视为端口限定；否则整串按 IpNet 解析。单主机（无 `/`）按全前缀长度处理。
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s.is_empty() {
            return Err("白名单条目为空".to_string());
        }
        if let Some((net_part, port_part)) = split_trailing_port(s) {
            let net = parse_net(net_part)?;
            let port = parse_port_spec(port_part)?;
            return Ok(TargetRule { net, port });
        }
        let net = parse_net(s)?;
        Ok(TargetRule { net, port: PortSpec::Any })
    }

    pub fn allows(&self, ip: IpAddr, port: u16) -> bool {
        self.net.contains(&ip)
            && match self.port {
                PortSpec::Any => true,
                PortSpec::Exact(p) => p == port,
            }
    }
}

fn parse_net(s: &str) -> Result<ipnet::IpNet, String> {
    let with_prefix = if s.contains('/') {
        s.to_string()
    } else {
        // 单主机 → 全前缀（ipnet 对裸 IP 的宽容度不作假设，显式补 /32 或 /128）
        let full = if s.contains(':') { 128 } else { 32 };
        format!("{s}/{full}")
    };
    with_prefix
        .parse::<ipnet::IpNet>()
        .map_err(|e| format!("白名单网段 «{s}» 解析失败：{e}"))
}

fn parse_port_spec(s: &str) -> Result<PortSpec, String> {
    if s == "*" {
        return Ok(PortSpec::Any);
    }
    let port: u32 = s.parse().map_err(|_| format!("白名单端口 «{s}» 不是数字或 *"))?;
    if port == 0 || port > 65535 {
        return Err(format!("白名单端口 «{s}» 超出 1-65535"));
    }
    Ok(PortSpec::Exact(port as u16))
}

/// 最后一个冒号切分；两端都过一遍可解析性预判（net 侧由调用方终判）。
fn split_trailing_port(s: &str) -> Option<(&str, &str)> {
    let idx = s.rfind(':')?;
    let (net_part, port_part) = (&s[..idx], &s[idx + 1..]);
    let port_looks_right = port_part == "*"
        || (!port_part.is_empty() && port_part.len() <= 5 && port_part.bytes().all(|b| b.is_ascii_digit()));
    if !port_looks_right {
        return None;
    }
    // 「::1」这类裸 IPv6 单主机：最后冒号后是数字，但剩余部分不是合法 net——
    // 让 net 侧先试解析，失败则整串重来（返回 None 走整串路径）。
    if parse_net(net_part).is_err() {
        return None;
    }
    Some((net_part, port_part))
}

/// 白名单判定。**规则为空 = 允许全部**（默认零摩擦）；非空时任一规则命中即允许
/// （deny-by-default）。域名目标由调用方先 resolve，全部候选 IP 都过才连。
pub fn target_allowed(rules: &[TargetRule], ip: IpAddr, port: u16) -> bool {
    if rules.is_empty() {
        return true;
    }
    rules.iter().any(|r| r.allows(ip, port))
}

// ─── 加密容器（复用 core::crypto：Argon2id + AES-256-GCM） ───

/// 导出：bundle → JSON → 加密 → EncryptedBlob JSON 文本（写 .json 文件，受控渠道传输）。
pub fn export_bundle(bundle: &GatewayBundle, password: &str) -> Result<String, String> {
    bundle.validate()?;
    let plain = serde_json::to_vec(bundle).map_err(|e| format!("序列化 bundle 失败：{e}"))?;
    let blob = crypto::encrypt(&plain, password)?;
    serde_json::to_string(&blob).map_err(|e| format!("序列化加密容器失败：{e}"))
}

/// 导入：加密容器 JSON 文本 + 口令 → bundle（含版本检查与 fail-closed 校验）。
pub fn parse_bundle(encrypted_json: &str, password: &str) -> Result<GatewayBundle, String> {
    let blob: EncryptedBlob = serde_json::from_str(encrypted_json)
        .map_err(|e| format!("加密容器损坏（不是合法的 bundle 文件）：{e}"))?;
    let plain = crypto::decrypt(&blob, password)?;
    let bundle: GatewayBundle = serde_json::from_slice(&plain)
        .map_err(|e| format!("bundle 结构解析失败（文件可能被截断）：{e}"))?;
    bundle.validate()?;
    Ok(bundle)
}

/// 运行方向：gateway-config.json 文本 → 结构（版本检查 + 校验）。
pub fn parse_runtime(json: &str) -> Result<GatewayRuntimeConfig, String> {
    let cfg: GatewayRuntimeConfig =
        serde_json::from_str(json).map_err(|e| format!("gateway-config.json 解析失败：{e}"))?;
    cfg.validate()?;
    Ok(cfg)
}

/// 运行方向：结构 → gateway-config.json 文本（调用方负责原子写盘）。
pub fn runtime_to_json(cfg: &GatewayRuntimeConfig) -> Result<String, String> {
    cfg.validate()?;
    serde_json::to_string_pretty(cfg).map_err(|e| format!("序列化运行配置失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin() -> HostKeyPin {
        HostKeyPin {
            key_type: "ssh-ed25519".into(),
            key_hex: "ab".repeat(32),
        }
    }

    fn bundle() -> GatewayBundle {
        GatewayBundle {
            version: BUNDLE_VERSION,
            vps: VpsEndpoint {
                host: "203.0.113.10".into(),
                port: 22222,
                username: "mount".into(),
                private_key: "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----".into(),
                passphrase: None,
                host_key_pin: pin(),
            },
            mount: MountConfig { bind_addr: "0.0.0.0".into(), remote_port: 2222 },
            gateway: GatewayServerConfig {
                listen_port: 22222,
                authorized_key: "ssh-ed25519 AAAAC3 test-key".into(),
                allowed_targets: vec![],
            },
            host_key_private: "-----BEGIN OPENSSH PRIVATE KEY-----\ndef\n-----END OPENSSH PRIVATE KEY-----".into(),
        }
    }

    // ─── 加密容器 ───

    #[test]
    fn bundle_roundtrip() {
        let exported = export_bundle(&bundle(), "pw").expect("export");
        let parsed = parse_bundle(&exported, "pw").expect("parse");
        assert_eq!(parsed, bundle());
    }

    #[test]
    fn wrong_password_rejected() {
        let exported = export_bundle(&bundle(), "right").expect("export");
        assert!(parse_bundle(&exported, "wrong").is_err(), "错误口令必须解不开");
    }

    #[test]
    fn tampered_container_rejected() {
        let mut exported = export_bundle(&bundle(), "pw").expect("export");
        // 翻转密文首字符（AEAD 完整性校验兜底）
        exported = exported.replace("\"ciphertext\":\"", "\"ciphertext\":\"X");
        assert!(parse_bundle(&exported, "pw").is_err(), "篡改后的容器必须解不开");
    }

    #[test]
    fn unknown_bundle_version_rejected() {
        let mut b = bundle();
        b.version = 999;
        let plain = serde_json::to_string(&b).unwrap();
        let blob = crypto::encrypt(plain.as_bytes(), "pw").unwrap();
        let json = serde_json::to_string(&blob).unwrap();
        let err = parse_bundle(&json, "pw").expect_err("未知版本必须拒绝");
        assert!(err.contains("不被支持"), "文案要说清版本问题：{err}");
    }

    #[test]
    fn garbage_container_rejected() {
        let err = parse_bundle("not a json", "pw").expect_err("非 JSON 容器必须拒绝");
        assert!(err.contains("损坏"));
    }

    // ─── validate fail-closed ───

    #[test]
    fn validate_rejects_bad_fields() {
        let cases: Vec<(&str, Box<dyn FnOnce(&mut GatewayBundle)>)> = vec![
            ("空 host", Box::new(|b| b.vps.host = "  ".into())),
            ("零端口", Box::new(|b| b.vps.port = 0)),
            ("空用户名", Box::new(|b| b.vps.username = String::new())),
            ("空挂载口", Box::new(|b| b.mount.remote_port = 0)),
            ("空私钥", Box::new(|b| b.vps.private_key = String::new())),
            ("非 PEM", Box::new(|b| b.vps.private_key = "not a key".into())),
            ("坏 pin hex", Box::new(|b| b.vps.host_key_pin.key_hex = "ZZ".into())),
            ("大写 hex", Box::new(|b| b.vps.host_key_pin.key_hex = "AB".repeat(32))),
            ("奇数 hex", Box::new(|b| b.vps.host_key_pin.key_hex = "abc".into())),
            ("空 authorized_key", Box::new(|b| b.gateway.authorized_key = " ".into())),
            ("坏白名单条目", Box::new(|b| b.gateway.allowed_targets = vec!["not-a-cidr".into()])),
            ("坏白名单端口", Box::new(|b| b.gateway.allowed_targets = vec!["10.0.0.0/8:99999".into()])),
            ("空 host key 私钥", Box::new(|b| b.host_key_private = String::new())),
        ];
        for (name, mutate) in cases {
            let mut b = bundle();
            mutate(&mut b);
            assert!(b.validate().is_err(), "case «{name}» 必须 fail-closed 拒绝");
        }
    }

    #[test]
    fn empty_passphrase_string_rejected() {
        let mut b = bundle();
        b.vps.passphrase = Some(String::new());
        assert!(b.validate().is_err(), "Some(空串) 是矛盾态，必须拒绝");
    }

    // ─── TargetRule::parse 矩阵 ───

    #[test]
    fn target_rule_parse_ipv4() {
        use ipnet::IpNet;
        use std::str::FromStr;

        let r = TargetRule::parse("10.0.0.0/8").unwrap();
        assert_eq!(r.net, IpNet::from_str("10.0.0.0/8").unwrap());
        assert_eq!(r.port, PortSpec::Any);

        let r = TargetRule::parse("10.0.0.0/8:22").unwrap();
        assert_eq!(r.port, PortSpec::Exact(22));

        let r = TargetRule::parse("10.0.0.0/8:*").unwrap();
        assert_eq!(r.port, PortSpec::Any);

        // 单主机 → 全前缀
        let r = TargetRule::parse("192.168.1.5").unwrap();
        assert_eq!(r.net, IpNet::from_str("192.168.1.5/32").unwrap());

        let r = TargetRule::parse("192.168.1.5:3389").unwrap();
        assert_eq!(r.port, PortSpec::Exact(3389));
    }

    #[test]
    fn target_rule_parse_ipv6() {
        use ipnet::IpNet;
        use std::str::FromStr;

        // 裸 IPv6 单主机：最后冒号后是数字，但剩余部分不是合法 net → 整串解析
        let r = TargetRule::parse("::1").unwrap();
        assert_eq!(r.net, IpNet::from_str("::1/128").unwrap());
        assert_eq!(r.port, PortSpec::Any);

        // 同理 ::2（数字结尾歧义样例）
        let r = TargetRule::parse("::2").unwrap();
        assert_eq!(r.net, IpNet::from_str("::2/128").unwrap());

        let r = TargetRule::parse("fd00::/8").unwrap();
        assert_eq!(r.net, IpNet::from_str("fd00::/8").unwrap());

        // IPv6 + 端口限定：剩余部分（fd00::/8）是合法 net → 拆分成立
        let r = TargetRule::parse("fd00::/8:22").unwrap();
        assert_eq!(r.port, PortSpec::Exact(22));

        let r = TargetRule::parse("2001:db8::1").unwrap();
        assert_eq!(r.net, IpNet::from_str("2001:db8::1/128").unwrap());
    }

    #[test]
    fn target_rule_parse_rejects_garbage() {
        for bad in ["", "  ", "abc", ":22", "10.0.0.0/8:", "10.0.0.0/33", "10.0.0.0/8:abc", "10.0.0.0/8:0"] {
            assert!(TargetRule::parse(bad).is_err(), "«{bad}» 必须解析失败");
        }
    }

    #[test]
    fn target_allowed_matrix() {
        use std::net::IpAddr;
        use std::str::FromStr;

        let ip = |s: &str| IpAddr::from_str(s).unwrap();
        let rules: Vec<TargetRule> = ["10.0.0.0/8:22", "192.168.0.0/16"]
            .iter()
            .map(|s| TargetRule::parse(s).unwrap())
            .collect();

        // 空规则 = 全放
        assert!(target_allowed(&[], ip("1.2.3.4"), 9999));
        // 命中：网段 + 端口
        assert!(target_allowed(&rules, ip("10.1.2.3"), 22));
        // 网段命中但端口不符，且无其他规则覆盖 → 拒
        assert!(!target_allowed(&rules, ip("10.1.2.3"), 80));
        // 第二条规则不限端口
        assert!(target_allowed(&rules, ip("192.168.5.5"), 8080));
        // 完全不在网段 → 拒（deny-by-default）
        assert!(!target_allowed(&rules, ip("172.16.0.1"), 22));
        // IPv6 与 v4 规则互不匹配
        assert!(!target_allowed(&rules, ip("::1"), 22));
    }

    // ─── 运行方向 ───

    #[test]
    fn runtime_roundtrip_and_version_gate() {
        let rt = bundle().to_runtime();
        let json = runtime_to_json(&rt).expect("to_json");
        let back = parse_runtime(&json).expect("parse");
        assert_eq!(back, rt);

        // to_runtime 不携带任何秘密
        assert!(!json.contains("private_key"), "运行配置不得包含私钥字段");
        assert!(!json.contains("host_key_private"));

        let mut bad = rt.clone();
        bad.version = 99;
        let err = parse_runtime(&serde_json::to_string(&bad).unwrap()).expect_err("版本必须拒绝");
        assert!(err.contains("不被支持"));
    }

    #[test]
    fn bundle_to_runtime_drops_secrets() {
        let b = bundle();
        let rt = b.to_runtime();
        // 运行方向只保留 pin（公开），不含私钥/passphrase
        assert_eq!(rt.host_key_pin, b.vps.host_key_pin);
        let s = serde_json::to_string(&rt).unwrap();
        assert!(!s.contains("BEGIN"), "运行配置序列化后不得出现 PEM 内容");
    }
}
