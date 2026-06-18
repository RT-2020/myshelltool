//! Windows DPAPI 凭据加密 codec（v1.3 SecretStore 升级）。
//!
//! 实现 core::SecretCodec trait，用 Windows Data Protection API 加密。
//! Scope::User 绑定当前用户登录态——只有该用户进程能解密，
//! 比 Machine scope（同机任何进程可解）更安全。
//!
//! 仅 Windows 编译（cfg(windows)）。非 Windows 平台 core 层用 LegacyXorCodec/PlaintextCodec，
//! 本项目 Windows 优先，src-tauri 实际只在 Windows 构建。

#![cfg(windows)]

use myshelltool_core::SecretCodec;
use windows_dpapi::{decrypt_data, encrypt_data, Scope};

/// DPAPI 加密 codec（User scope）。
///
/// 与 core::LegacyXorCodec 的区别：DPAPI 是真正的操作系统级加密
/// （密钥由用户登录凭据派生，不落盘），XOR 只是可逆混淆。
pub struct DpapiCodec;

impl SecretCodec for DpapiCodec {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        encrypt_data(plaintext, Scope::User).map_err(|e| format!("DPAPI encrypt failed: {e}"))
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        decrypt_data(ciphertext, Scope::User).map_err(|e| format!("DPAPI decrypt failed: {e}"))
    }
}
