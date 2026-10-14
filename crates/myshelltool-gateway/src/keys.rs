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
}
