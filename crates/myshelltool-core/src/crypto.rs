//! Gist 同步加密内核（v1.3）。
//!
//! 主密码 → Argon2id 派生 → AES-256-GCM 认证加密。用于加密 connection-assets.json
//! 后上传 Gist，保证即使 Gist 泄露，攻击者拿到的也是密文。
//!
//! 设计要点：
//! - Argon2id（抗 GPU/ASIC 暴力破解）：主密码 + 随机 salt → 32 字节 AES key
//! - AES-256-GCM（AEAD，认证加密）：密文自带完整性校验，篡改即解密失败
//! - 每次 encrypt 生成新随机 salt + nonce（nonce 复用在 AES-GCM 下是灾难性的）
//! - salt / nonce 不敏感，随密文一起存（EncryptedBlob），密钥只从主密码派生
//!
//! 纯函数，无 IO，可独立 cargo test。

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::Argon2;
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// Argon2 salt 长度（16 字节，OWASP 推荐下限）。
pub const SALT_LEN: usize = 16;
/// AES-GCM nonce 长度（12 字节，AES-GCM 标准）。
pub const NONCE_LEN: usize = 12;
/// AES-256 密钥长度。
const KEY_LEN: usize = 32;

/// 加密后的数据块（随 Gist 载荷一起传输/存储）。
///
/// salt + nonce 不敏感（公开无妨），ciphertext 是密文。
/// 解密方只需主密码 + 这个 blob 即可还原明文。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedBlob {
    /// Argon2 salt（base64 编码，便于 JSON 序列化进 Gist）。
    pub salt: String,
    /// AES-GCM nonce（base64）。
    pub nonce: String,
    /// AES-GCM 密文（base64）。
    pub ciphertext: String,
}

/// 从主密码 + salt 派生 32 字节 AES 密钥（Argon2id）。
///
/// Argon2id 参数用 argon2 crate 默认（Argon2::default()），
/// 即 m=19456KB/t=2/p=1——对个人桌面应用是合理的强度/延迟平衡。
fn derive_key(master_password: &str, salt: &[u8]) -> Result<[u8; KEY_LEN], String> {
    let argon2 = Argon2::default();
    let mut key = [0u8; KEY_LEN];
    argon2
        .hash_password_into(master_password.as_bytes(), salt, &mut key)
        .map_err(|e| format!("Argon2 key derivation failed: {e}"))?;
    Ok(key)
}

/// 从主密码 + 固定 salt **确定性**派生会话密钥（v1.6 自动同步）。
///
/// 与 `derive_key` 的区别：本函数 `pub`，供自动同步路径持久化会话密钥。
/// **确定性**是关键——同一主密码 + 同一 salt 永远派生出同一 key，
/// 所以首次启用自动同步时派生的 key 可以加密存盘（DPAPI 保护），后续复用。
///
/// 返回 32 字节 AES 密钥（拷贝给调用方，避免持有引用）。
pub fn derive_session_key(master_password: &str, salt: &[u8]) -> Result<[u8; KEY_LEN], String> {
    derive_key(master_password, salt)
}

/// 用现成 key 加密明文（v1.6 自动同步路径）。
///
/// 与 `encrypt` 的区别：跳过 Argon2id 派生（key 由调用方提供——会话密钥已由
/// `derive_session_key(主密码, salt)` 派生并用 DPAPI 加密存盘）。
///
/// **`salt` 必须随载荷写进 `blob.salt`（v2.7 修复）**：会话密钥 = Argon2id(主密码, salt)。
/// v2.7 之前这里把 salt 留空、salt 只存在本机 SecretStore，后果是换机/重装/换 Windows
/// 用户后**即使主密码正确也无法重建同一把 key** —— Gist 备份永久解不开，而 UI 明写
/// 「主密码解密即可恢复全部资产」，承诺被打破。salt 本身不敏感（password-based 路径本来
/// 就把它随密文公开存储，`decrypt` 直接从 `blob.salt` 读），写进去只是让**同一份密文**
/// 同时可由「本机会话密钥」与「主密码 + salt」两条路解开：安全强度不变（都取决于
/// 主密码的 Argon2id 代价）。
///
/// 空 salt 直接拒绝：那会静默产出「只有本机能解」的载荷，正是本函数要根治的缺陷形态。
pub fn encrypt_with_key(
    plaintext: &[u8],
    key: &[u8; KEY_LEN],
    salt: &[u8],
) -> Result<EncryptedBlob, String> {
    if salt.is_empty() {
        return Err(
            "会话密钥加密必须带上派生 salt：缺了它，备份将无法用主密码在其它机器恢复".to_string(),
        );
    }
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| format!("AES key init failed: {e}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("AES-GCM encrypt failed: {e}"))?;

    Ok(EncryptedBlob {
        salt: base64_encode(salt),
        nonce: base64_encode(&nonce_bytes),
        ciphertext: base64_encode(&ciphertext),
    })
}

/// 旧格式载荷（缺派生 salt）的统一拒绝文案。
///
/// v2.7 起只支持「salt 随载荷」的格式：项目仅一位用户，明确放弃旧版适配。
/// 宁可明确报错并指导重建，也不留「密码路径报错、会话密钥路径却能悄悄解开」的半支持状态。
pub const LEGACY_PAYLOAD_REJECTED: &str =
    "载荷缺少派生 salt（v2.7 之前的旧格式备份，已不再支持）：请在本机执行一次「推送到云端」重建备份";

/// 取载荷里的 Argon2 派生 salt（base64 解码）。空 salt（旧格式）直接拒绝。
///
/// 供「按载荷的 salt 重建会话密钥」使用（见 `sync::session_key_for_payload`）。
pub fn salt_bytes(blob: &EncryptedBlob) -> Result<Vec<u8>, String> {
    if blob.salt.is_empty() {
        return Err(LEGACY_PAYLOAD_REJECTED.to_string());
    }
    base64_decode(&blob.salt)
}

/// 用现成 key 解密（v1.6 自动同步路径）。
///
/// 配合 `encrypt_with_key`。`salt` 不参与解密（key 由调用方提供），但**必须非空**：
/// 空 salt 一律判为旧格式载荷并拒绝，见 `LEGACY_PAYLOAD_REJECTED`。
pub fn decrypt_with_key(blob: &EncryptedBlob, key: &[u8; KEY_LEN]) -> Result<Vec<u8>, String> {
    if blob.salt.is_empty() {
        return Err(LEGACY_PAYLOAD_REJECTED.to_string());
    }
    let nonce_bytes = base64_decode(&blob.nonce)?;
    let ciphertext = base64_decode(&blob.ciphertext)?;

    if nonce_bytes.len() != NONCE_LEN {
        return Err(format!("invalid nonce length: {}", nonce_bytes.len()));
    }

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| format!("AES key init failed: {e}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| format!("AES-GCM decrypt failed (会话密钥失效或密文被篡改): {e}"))
}

/// 加密明文。
///
/// 生成随机 salt + nonce，从主密码派生 key，AES-256-GCM 加密。
/// 返回的 EncryptedBlob 可安全传输（salt/nonce 公开，ciphertext 是密文）。
pub fn encrypt(plaintext: &[u8], master_password: &str) -> Result<EncryptedBlob, String> {
    let mut salt = [0u8; SALT_LEN];
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut salt);
    rand::thread_rng().fill_bytes(&mut nonce_bytes);

    let key = derive_key(master_password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| format!("AES key init failed: {e}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("AES-GCM encrypt failed: {e}"))?;

    Ok(EncryptedBlob {
        salt: base64_encode(&salt),
        nonce: base64_encode(&nonce_bytes),
        ciphertext: base64_encode(&ciphertext),
    })
}

/// 解密。
///
/// 用 blob 里的 salt 重新派生 key（同一主密码 → 同一 key），
/// AES-256-GCM 解密 + 完整性校验。密文被篡改或主密码错误 → Err。
///
/// 空 salt = v2.7 之前的旧格式载荷（会话密钥的派生 salt 当时不随载荷走）→ 明确拒绝，
/// 而不是让 Argon2 抛「salt is too short」这种看不出所以然的错。
pub fn decrypt(blob: &EncryptedBlob, master_password: &str) -> Result<Vec<u8>, String> {
    if blob.salt.is_empty() {
        return Err(LEGACY_PAYLOAD_REJECTED.to_string());
    }
    let salt = base64_decode(&blob.salt)?;
    let nonce_bytes = base64_decode(&blob.nonce)?;
    let ciphertext = base64_decode(&blob.ciphertext)?;

    if nonce_bytes.len() != NONCE_LEN {
        return Err(format!("invalid nonce length: {}", nonce_bytes.len()));
    }

    let key = derive_key(master_password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| format!("AES key init failed: {e}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| format!("AES-GCM decrypt failed (密码错误或密文被篡改): {e}"))
}

// ─── base64 编解码（避免引入 base64 crate，用标准库的简单实现）───
//
// 仅用于 salt/nonce/ciphertext 的 JSON 序列化，标准 RFC 4648 base64。

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(CHARS[((n >> 18) & 63) as usize] as char);
        out.push(CHARS[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARS[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARS[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if s.len() % 4 != 0 {
        return Err(format!("invalid base64 length: {}", s.len()));
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let bytes = s.as_bytes();
    for chunk in bytes.chunks(4) {
        let mut vals = [0u8; 4];
        let mut pad = 0;
        for (i, &b) in chunk.iter().enumerate() {
            vals[i] = if b == b'=' {
                pad += 1;
                0
            } else {
                CHARS
                    .iter()
                    .position(|&c| c == b)
                    .ok_or_else(|| format!("invalid base64 char: {}", b as char))? as u8
            };
        }
        let n = ((vals[0] as u32) << 18)
            | ((vals[1] as u32) << 12)
            | ((vals[2] as u32) << 6)
            | (vals[3] as u32);
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let plaintext = br#"{"assets":[{"id":"a1","host":"10.0.0.1"}]}"#;
        let password = "correct-horse-battery-staple";
        let blob = encrypt(plaintext, password).expect("encrypt");
        let decrypted = decrypt(&blob, password).expect("decrypt");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_password_fails_decryption() {
        let blob = encrypt(b"secret data", "right-password").expect("encrypt");
        let result = decrypt(&blob, "wrong-password");
        assert!(result.is_err(), "错误密码应解密失败");
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let blob = encrypt(b"secret data", "pw").expect("encrypt");
        let mut tampered = blob.clone();
        // 篡改密文：翻转第一个字符
        let mut chars: Vec<char> = tampered.ciphertext.chars().collect();
        if chars[0] == 'A' {
            chars[0] = 'B';
        } else {
            chars[0] = 'A';
        }
        tampered.ciphertext = chars.into_iter().collect();
        let result = decrypt(&tampered, "pw");
        assert!(result.is_err(), "篡改后的密文应解密失败（AEAD 完整性校验）");
    }

    #[test]
    fn each_encrypt_uses_random_salt_and_nonce() {
        // 同一明文 + 密码加密两次，salt/nonce/ciphertext 应都不同（随机性）
        let blob1 = encrypt(b"same data", "pw").expect("encrypt 1");
        let blob2 = encrypt(b"same data", "pw").expect("encrypt 2");
        assert_ne!(blob1.salt, blob2.salt, "salt 应随机");
        assert_ne!(blob1.nonce, blob2.nonce, "nonce 应随机");
        assert_ne!(blob1.ciphertext, blob2.ciphertext, "密文应不同");
        // 但都能用同一密码解出
        assert_eq!(
            decrypt(&blob1, "pw").unwrap(),
            decrypt(&blob2, "pw").unwrap()
        );
    }

    #[test]
    fn empty_plaintext_works() {
        let blob = encrypt(b"", "pw").expect("encrypt empty");
        let decrypted = decrypt(&blob, "pw").expect("decrypt empty");
        assert!(decrypted.is_empty());
    }

    #[test]
    fn unicode_password_works() {
        let blob = encrypt(b"data", "密码🔑123").expect("encrypt");
        let decrypted = decrypt(&blob, "密码🔑123").expect("decrypt");
        assert_eq!(decrypted, b"data");
    }

    #[test]
    fn base64_roundtrip() {
        for case in [&b""[..], &[0u8], &[0u8, 1], &[0u8, 1, 2], b"hello world", &[255; 32]] {
            let encoded = base64_encode(case);
            let decoded = base64_decode(&encoded).expect("decode");
            assert_eq!(decoded, case, "base64 往返失败: {:?}", case);
        }
    }

    #[test]
    fn blob_serializes_to_json() {
        // EncryptedBlob 要能序列化进 Gist 载荷（JSON）
        let blob = encrypt(b"test", "pw").expect("encrypt");
        let json = serde_json::to_string(&blob).expect("serialize");
        assert!(json.contains("\"salt\""));
        assert!(json.contains("\"nonce\""));
        assert!(json.contains("\"ciphertext\""));
        let back: EncryptedBlob = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, blob);
    }

    // ─── v1.6 会话密钥（key-based）路径测试 ───

    #[test]
    fn derive_session_key_is_deterministic() {
        // 确定性派生：同一密码 + 同一 salt → 同一 key（这是会话密钥可持久化的前提）
        let salt = [0xABu8; SALT_LEN];
        let key1 = derive_session_key("master-pw", &salt).expect("derive 1");
        let key2 = derive_session_key("master-pw", &salt).expect("derive 2");
        assert_eq!(key1, key2, "同密码+同salt应派生出相同key");

        // 不同密码 → 不同 key
        let key3 = derive_session_key("other-pw", &salt).expect("derive 3");
        assert_ne!(key1, key3, "不同密码应派生出不同key");

        // 不同 salt → 不同 key
        let salt2 = [0xCDu8; SALT_LEN];
        let key4 = derive_session_key("master-pw", &salt2).expect("derive 4");
        assert_ne!(key1, key4, "不同salt应派生出不同key");
    }

    #[test]
    fn encrypt_decrypt_with_key_roundtrip() {
        let salt = [0x11u8; SALT_LEN];
        let key = derive_session_key("pw", &salt).expect("derive");
        let plaintext = br#"{"assets":[{"id":"x","host":"10.0.0.1"}]}"#;
        let blob = encrypt_with_key(plaintext, &key, &salt).expect("encrypt_with_key");
        let decrypted = decrypt_with_key(&blob, &key).expect("decrypt_with_key");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let salt = [0x11u8; SALT_LEN];
        let key1 = derive_session_key("pw1", &salt).expect("derive 1");
        let key2 = derive_session_key("pw2", &salt).expect("derive 2");
        let blob = encrypt_with_key(b"secret", &key1, &salt).expect("encrypt");
        // 用错误的 key 解密应失败（AES-GCM 认证校验）
        let result = decrypt_with_key(&blob, &key2);
        assert!(result.is_err(), "错误 key 应解密失败");
    }

    #[test]
    fn key_based_tampered_ciphertext_fails() {
        let salt = [0x11u8; SALT_LEN];
        let key = derive_session_key("pw", &salt).expect("derive");
        let blob = encrypt_with_key(b"secret", &key, &salt).expect("encrypt");
        let mut tampered = blob.clone();
        // 篡改密文首字符
        let mut chars: Vec<char> = tampered.ciphertext.chars().collect();
        chars[0] = if chars[0] == 'A' { 'B' } else { 'A' };
        tampered.ciphertext = chars.into_iter().collect();
        let result = decrypt_with_key(&tampered, &key);
        assert!(result.is_err(), "篡改后应解密失败（AEAD 完整性校验）");
    }

    #[test]
    fn key_based_blob_carries_salt_and_master_password_can_decrypt_it() {
        // 【v2.7 关键回归】会话密钥加密的载荷必须能被**主密码**解开：
        // 会话密钥 = Argon2id(主密码, salt)，salt 随载荷存 → 换机后主密码可重建同一 key。
        // （修复前 salt 留空，换机后任何密码都解不开 → 备份不可恢复。）
        let salt = [0x2Au8; SALT_LEN];
        let key = derive_session_key("master-pw", &salt).expect("derive");
        let blob = encrypt_with_key(b"vault", &key, &salt).expect("encrypt_with_key");

        assert!(!blob.salt.is_empty(), "salt 必须随载荷存（否则换机不可恢复）");
        assert_eq!(decrypt_with_key(&blob, &key).expect("会话密钥路径"), b"vault");
        assert_eq!(decrypt(&blob, "master-pw").expect("主密码路径"), b"vault");
        // 仍然是「认证加密」：密码错误 → 解不开（不是 fail-open）
        assert!(decrypt(&blob, "wrong-pw").is_err(), "错误主密码不应解开");
    }

    #[test]
    fn key_based_encryption_rejects_empty_salt() {
        // 缺 salt 的载荷 = 只有本机能解，必须当场失败而不是静默产出
        let key = derive_session_key("pw", &[0u8; SALT_LEN]).expect("derive");
        let result = encrypt_with_key(b"data", &key, &[]);
        assert!(result.is_err(), "空 salt 应被拒绝");
    }

    #[test]
    fn legacy_payload_without_salt_is_rejected_on_both_paths() {
        // v2.7：旧格式（salt 缺省）不再适配 —— 两条解密路径都必须**明确拒绝**，
        // 不能出现「密码路径报错、会话密钥路径却能悄悄解开」的半支持状态。
        let salt = [0x3Bu8; SALT_LEN];
        let key = derive_session_key("master-pw", &salt).expect("derive");
        let blob = encrypt_with_key(b"secret", &key, &salt).expect("encrypt_with_key");
        let legacy = EncryptedBlob { salt: String::new(), ..blob };

        for err in [
            decrypt(&legacy, "master-pw").expect_err("密码路径应拒绝旧格式"),
            decrypt_with_key(&legacy, &key).expect_err("会话密钥路径应拒绝旧格式"),
        ] {
            assert!(err.contains("不再支持"), "文案要说清旧格式不再支持: {err}");
        }
    }

    #[test]
    fn each_encrypt_with_key_uses_random_nonce() {
        // 同一 key 加密同一明文两次，nonce/ciphertext 应不同
        let salt = [0x11u8; SALT_LEN];
        let key = derive_session_key("pw", &salt).expect("derive");
        let blob1 = encrypt_with_key(b"same", &key, &salt).expect("encrypt 1");
        let blob2 = encrypt_with_key(b"same", &key, &salt).expect("encrypt 2");
        assert_ne!(blob1.nonce, blob2.nonce, "nonce 应随机");
        assert_ne!(blob1.ciphertext, blob2.ciphertext, "密文应不同");
        // 但都能用同一 key 解出
        assert_eq!(
            decrypt_with_key(&blob1, &key).unwrap(),
            decrypt_with_key(&blob2, &key).unwrap()
        );
    }
}
