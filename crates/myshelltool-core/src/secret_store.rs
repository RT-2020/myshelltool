//! 凭据域：SecretStore 与编解码器（LegacyXorCodec/PlaintextCodec）、
//! 凭据状态/引用结构（从 lib.rs 按域拆出，v2.8 第四轮）。
//! DPAPI 编解码在 dpapi_codec（src-tauri 侧），本模块只管存储与格式判别。

use serde::{Deserialize, Serialize};

use std::fs;

use crate::asset_store::write_atomic;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialRef {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialStatus {
    pub id: String,
    pub exists: bool,
    pub label: String,
}

/// 凭据加密编解码抽象。
///
/// 设计动机：myshelltool-core 必须保持「无平台依赖、可跨平台 cargo test」，
/// 而 DPAPI 是 Windows-only。故 core 只定义 trait + 提供跨平台实现（LegacyXorCodec
/// 兼容旧文件 + PlaintextCodec 测试用），DPAPI 实现放 src-tauri 层（#[cfg(windows)]）
/// 并在构造 SecretStore 时注入。
///
/// 格式约定（磁盘字节流）：
/// - 新格式：`MAGIC_DPAPI` + codec 加密后的密文。MAGIC 让 read() 能探测格式。
/// - 旧格式：无 MAGIC 头，是 LegacyXorCodec 的输出（向后兼容，读时懒迁移）。
pub trait SecretCodec: Send + Sync {
    /// 加密明文 → 密文字节（不含 MAGIC，MAGIC 由 SecretStore 统一加）。
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, String>;
    /// 解密密文字节（已剥 MAGIC）→ 明文。
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, String>;
}

/// 新格式文件头。读到这个前缀 = 新格式；否则当旧 XOR 处理（懒迁移）。
const MAGIC_DPAPI: &[u8] = b"DPAPI1";

/// 旧版 XOR 编解码（保留用于读旧 .cred 文件 + 跨平台测试）。
///
/// 安全性：逐字节可逆混淆，非加密。仅用于向后兼容，新写入永远走注入的 codec。
pub struct LegacyXorCodec;

impl SecretCodec for LegacyXorCodec {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        Ok(xor_transform(plaintext))
    }
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        Ok(xor_transform(ciphertext))
    }
}

/// 明文编解码（测试用）。不做任何加密，仅用于验证 SecretStore 的格式探测/迁移逻辑。
pub struct PlaintextCodec;

impl SecretCodec for PlaintextCodec {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        Ok(plaintext.to_vec())
    }
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        Ok(ciphertext.to_vec())
    }
}

/// XOR 编解码的共享实现（encode/decode 互逆，故同一函数）。
pub(crate) fn xor_transform(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for (i, &byte) in data.iter().enumerate() {
        out.push(byte ^ ((i as u8).wrapping_add(0x5A)));
    }
    out
}

/// 探测字节流是否为新格式（有 MAGIC 头）。
pub(crate) fn is_new_format(data: &[u8]) -> bool {
    data.len() >= MAGIC_DPAPI.len() && &data[..MAGIC_DPAPI.len()] == MAGIC_DPAPI
}

pub struct SecretStore {
    dir: std::path::PathBuf,
    codec: Box<dyn SecretCodec>,
}

impl SecretStore {
    /// 构造凭据存储。`codec` 决定新写入的加密方式（src-tauri 传 DpapiCodec）。
    pub fn new(dir: impl Into<std::path::PathBuf>, codec: Box<dyn SecretCodec>) -> Self {
        Self {
            dir: dir.into(),
            codec,
        }
    }

    pub fn save(&self, id: &str, secret: &str) -> Result<(), String> {
        let id = sanitize_credential_id(id)?;
        if secret.trim().is_empty() {
            return Err("secret must not be empty".to_string());
        }
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let path = self.dir.join(id);
        // 新写入永远用注入的 codec 加密 + MAGIC 头
        let encrypted = self.codec.encrypt(secret.as_bytes())?;
        let mut payload = Vec::with_capacity(MAGIC_DPAPI.len() + encrypted.len());
        payload.extend_from_slice(MAGIC_DPAPI);
        payload.extend_from_slice(&encrypted);
        // 原子写：凭据文件半截会被后续读取判为「损坏/不存在」，用户表现为认证失败
        write_atomic(path, payload)
    }

    pub fn get_status(&self, id: &str) -> Result<CredentialStatus, String> {
        let id = sanitize_credential_id(id)?;
        let path = self.dir.join(&id);
        Ok(CredentialStatus {
            id: id.clone(),
            exists: path.exists() && fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false),
            label: id,
        })
    }

    pub fn delete(&self, id: &str) -> Result<bool, String> {
        let id = sanitize_credential_id(id)?;
        let path = self.dir.join(id);
        if !path.exists() {
            return Ok(false);
        }
        let content = fs::read(&path).map_err(|e| e.to_string())?;
        fs::remove_file(path).map_err(|e| e.to_string())?;
        zero_memory(&content);
        Ok(true)
    }

    pub fn list(&self) -> Result<Vec<CredentialStatus>, String> {
        if !self.dir.exists() {
            return Ok(vec![]);
        }
        let mut result = vec![];
        for entry in fs::read_dir(&self.dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".cred") {
                result.push(CredentialStatus {
                    id: name.clone(),
                    exists: true,
                    label: name,
                });
            }
        }
        Ok(result)
    }

    /// 读取凭据。
    ///
    /// 格式探测 + 懒迁移：读到旧 XOR 格式文件时，用 LegacyXorCodec 解密，
    /// 然后用当前 codec 重新加密写回（下次读就是新格式）。迁移失败不阻断读取
    ///（返回解密后的明文即可，迁移是 best-effort）。
    pub fn read(&self, id: &str) -> Result<Option<String>, String> {
        let id = sanitize_credential_id(id)?;
        let path = self.dir.join(&id);
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read(&path).map_err(|e| e.to_string())?;
        if raw.is_empty() {
            return Ok(None);
        }

        let plaintext_bytes = if is_new_format(&raw) {
            // 新格式：剥 MAGIC 头，用注入的 codec 解密
            let ciphertext = &raw[MAGIC_DPAPI.len()..];
            self.codec.decrypt(ciphertext)?
        } else {
            // 旧格式（XOR）：用 LegacyXorCodec 解密，然后懒迁移到新格式
            let legacy = LegacyXorCodec;
            let plaintext = legacy.decrypt(&raw)?;
            // 懒迁移：best-effort，失败仅记日志不阻断读取
            if let Err(e) = self.rewrite_with_new_format(&id, &plaintext) {
                // 迁移失败不影响本次返回明文，但记录便于排查
                eprintln!("[SecretStore] lazy migration failed for {id}: {e}");
            }
            plaintext
        };

        Ok(Some(
            String::from_utf8(plaintext_bytes).map_err(|e| e.to_string())?,
        ))
    }

    /// 懒迁移辅助：用当前 codec 重新加密明文并写回。best-effort。
    fn rewrite_with_new_format(&self, id: &str, plaintext: &[u8]) -> Result<(), String> {
        let path = self.dir.join(id);
        let encrypted = self.codec.encrypt(plaintext)?;
        let mut payload = Vec::with_capacity(MAGIC_DPAPI.len() + encrypted.len());
        payload.extend_from_slice(MAGIC_DPAPI);
        payload.extend_from_slice(&encrypted);
        write_atomic(path, payload)
    }
}

fn sanitize_credential_id(id: &str) -> Result<String, String> {
    let sanitized: String = id
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if sanitized.is_empty() {
        return Err("credential id must contain alphanumeric characters".to_string());
    }
    Ok(format!("{}.cred", sanitized))
}

fn zero_memory(data: &[u8]) {
    let ptr = data.as_ptr() as *mut u8;
    unsafe {
        for i in 0..data.len() {
            std::ptr::write_volatile(ptr.add(i), 0);
        }
    }
}

