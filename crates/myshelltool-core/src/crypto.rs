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
pub fn decrypt(blob: &EncryptedBlob, master_password: &str) -> Result<Vec<u8>, String> {
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
}
