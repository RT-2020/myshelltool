//! 密钥工具：网关三对钥（host key / 客户端钥 / 挂载钥）的生成、解析、公钥序列化。
//!
//! 向导（本机侧）与测试共用。全部 ed25519（现代、短、russh 原生支持）。

use russh::keys::ssh_key::{Algorithm, LineEnding, PrivateKey, PublicKey};
use russh::keys::PublicKeyBase64;

/// 生成一对 ed25519 密钥。返回 (私钥 PEM, 公钥 authorized_keys 单行)。
pub fn generate_ed25519() -> Result<(String, String), String> {
    let mut rng = rand::thread_rng();
    let key = PrivateKey::random(&mut rng, Algorithm::Ed25519)
        .map_err(|e| format!("生成 ed25519 密钥失败：{e}"))?;
    let private_pem = key
        .to_openssh(LineEnding::LF)
        .map_err(|e| format!("序列化私钥 PEM 失败：{e}"))?
        .to_string();
    let public_line = public_key_line(key.public_key())?;
    Ok((private_pem, public_line))
}

/// 公钥 → authorized_keys 单行（"ssh-ed25519 AAAA... 无注释"）。
pub fn public_key_line(pk: &PublicKey) -> Result<String, String> {
    pk.to_openssh().map_err(|e| format!("序列化公钥失败：{e}"))
}

/// 解析私钥 PEM（挂载私钥 / host key 从配置读出后用）。
pub fn decode_private_key(pem: &str, passphrase: Option<&str>) -> Result<PrivateKey, String> {
    russh::keys::decode_secret_key(pem, passphrase)
        .map_err(|e| format!("私钥解析失败：{e}"))
}

/// 解析 authorized_keys 单行为公钥。
pub fn parse_public_key_line(line: &str) -> Result<PublicKey, String> {
    PublicKey::from_openssh(line.trim()).map_err(|e| format!("公钥解析失败：{e}"))
}

/// 公钥字节的小写 hex（pin / 白名单比对判据，与 known_hosts 的 bytes_to_hex 同源）。
pub fn public_key_hex(pk: &PublicKey) -> String {
    hex_lower(&pk.public_key_bytes())
}

fn hex_lower(bytes: &[u8]) -> String {
    const CHARS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(CHARS[(b >> 4) as usize] as char);
        out.push(CHARS[(b & 0x0f) as usize] as char);
    }
    out
}

/// 按 pin 判定公钥是否匹配（key_hex 精确比对；key_type 不参与判定，仅报错指认用）。
pub fn pin_matches(pk: &PublicKey, key_hex: &str) -> bool {
    public_key_hex(pk).eq_ignore_ascii_case(key_hex)
}

// ─── 口令派生密钥（v0.21「服务器 + 口令」零文件口径） ───
//
// 设计：一个强随机口令 = 全部信任根。挂载钥 / relay host key / 网关 host key /
// 客户端钥都从「口令 + 用途 label」经 Argon2id 确定性派生（同口令同 label 永远
// 同钥匙）——三方（relay / 跳板机 / 本机）之间不再传输任何密钥文件：
// relay 侧 docker run -e GATEWAY_SECRET=<口令> 自派生 host key 与 authorized_keys；
// 跳板机输入「服务器 + 口令」本地派生全部身份；本机向导同派生预写 known_hosts。
// 代价：口令泄露即全失（对比 bundle 文件 + 口令双因子）——故强制 ≥8 位且向导
// 默认生成 20 位强随机，这是易用性换来的明确权衡。

/// 口令派生的用途 label（新增用途必须在此登记——label 即密钥隔离域）。
pub const LABEL_RELAY_HOSTKEY: &str = "relay-hostkey";
pub const LABEL_MOUNT: &str = "mount";
pub const LABEL_GATEWAY_HOSTKEY: &str = "gateway-hostkey";
pub const LABEL_CLIENT: &str = "client";

/// 从口令 + label 确定性派生一对 ed25519 密钥。
/// 返回 (私钥 PEM, 公钥 authorized_keys 单行)。
pub fn derive_ed25519(secret: &str, label: &str) -> Result<(String, String), String> {
    let secret = secret.trim();
    if secret.len() < 8 {
        return Err("口令至少 8 位（口令是网关全部身份的信任根，请用强随机口令）".to_string());
    }
    use argon2::Argon2;
    // label 作盐：同口令不同 label 派生不同钥匙（密钥隔离域）
    let salt = format!("myshelltool-gateway:{label}");
    let mut seed = [0u8; 32];
    Argon2::default()
        .hash_password_into(secret.as_bytes(), salt.as_bytes(), &mut seed)
        .map_err(|e| format!("口令派生失败：{e}"))?;
    let keypair = russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&seed);
    let key = PrivateKey::from(keypair);
    let pem = key
        .to_openssh(LineEnding::LF)
        .map_err(|e| format!("序列化派生私钥失败：{e}"))?
        .to_string();
    let pub_line = public_key_line(key.public_key())?;
    Ok((pem, pub_line))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_roundtrip() {
        let (pem, line) = generate_ed25519().expect("generate");
        assert!(pem.contains("OPENSSH PRIVATE KEY"));
        assert!(line.starts_with("ssh-ed25519 "));

        let sk = decode_private_key(&pem, None).expect("decode private");
        let pk = parse_public_key_line(&line).expect("decode public");
        // 同一对钥：私钥派生公钥 = 解析出的公钥
        assert_eq!(sk.public_key().fingerprint(Default::default()), pk.fingerprint(Default::default()));

        // hex 往返 + pin 匹配
        let hex = public_key_hex(&pk);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert!(pin_matches(&pk, &hex));
        assert!(!pin_matches(&pk, &"00".repeat(32)));
    }

    #[test]
    fn derive_is_deterministic_and_label_isolated() {
        let (pem1, pub1) = derive_ed25519("correct-horse-battery-20", LABEL_MOUNT).unwrap();
        let (pem2, pub2) = derive_ed25519("correct-horse-battery-20", LABEL_MOUNT).unwrap();
        assert_eq!(pem1, pem2, "同口令同 label 必须派生同一把钥匙");
        assert_eq!(pub1, pub2);

        let (_, pub_other) = derive_ed25519("correct-horse-battery-20", LABEL_RELAY_HOSTKEY).unwrap();
        assert_ne!(pub1, pub_other, "不同 label 必须隔离");

        let (_, pub_wrong) = derive_ed25519("correct-horse-battery-21", LABEL_MOUNT).unwrap();
        assert_ne!(pub1, pub_wrong, "不同口令必须不同钥匙");
    }

    #[test]
    fn derive_rejects_weak_secret_and_roundtrips() {
        assert!(derive_ed25519("short", LABEL_MOUNT).is_err(), "短口令必须拒绝");
        let (pem, pub_line) = derive_ed25519("a-strong-20-char-secret!", LABEL_CLIENT).unwrap();
        // 派生出的私钥可被标准解析路径读回，且公钥与私钥匹配
        let sk = decode_private_key(&pem, None).expect("派生私钥可解析");
        let pk = parse_public_key_line(&pub_line).expect("派生公钥可解析");
        assert_eq!(
            sk.public_key().fingerprint(Default::default()),
            pk.fingerprint(Default::default()),
            "派生的私钥与公钥必须是同一对"
        );
    }
}
