//! Windows DPAPI 凭据加密 codec（v1.3 SecretStore 升级）。
//!
//! 实现 core::SecretCodec trait，用 Windows Data Protection API 加密。
//! Scope::User 绑定当前用户登录态——只有该用户进程能解密，
//! 比 Machine scope（同机任何进程可解）更安全。
//!
//! 跨平台编译约定：模块本体全平台编译（portability job 在 ubuntu/macos
//! 编译 src-tauri，lib.rs 无条件引用 DpapiCodec）；真 DPAPI 调用只在
//! Windows 分支存在（windows-dpapi 依赖本身 target-gated）。非 Windows
//! 运行时 encrypt/decrypt 一律报错拒绝——**不降级**到 XOR/明文（凭据
//! 加密失败只允许 fail-closed，见 AGENTS「失败朝宽松方向折叠」红线）。

use myshelltool_core::SecretCodec;

#[cfg(windows)]
use windows_dpapi::{decrypt_data, encrypt_data, Scope};

/// DPAPI 加密 codec（User scope）。
///
/// 与 core::LegacyXorCodec 的区别：DPAPI 是真正的操作系统级加密
/// （密钥由用户登录凭据派生，不落盘），XOR 只是可逆混淆。
pub struct DpapiCodec;

impl SecretCodec for DpapiCodec {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        #[cfg(windows)]
        {
            encrypt_data(plaintext, Scope::User).map_err(|e| format!("DPAPI encrypt failed: {e}"))
        }
        #[cfg(not(windows))]
        {
            let _ = plaintext;
            Err("DPAPI 仅 Windows 可用：本构建运行在非 Windows 平台，凭据加密被拒绝（fail-closed，不降级弱加密）".to_string())
        }
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        #[cfg(windows)]
        {
            decrypt_data(ciphertext, Scope::User).map_err(|e| format!("DPAPI decrypt failed: {e}"))
        }
        #[cfg(not(windows))]
        {
            let _ = ciphertext;
            Err("DPAPI 仅 Windows 可用：本构建运行在非 Windows 平台，凭据解密被拒绝（fail-closed，不降级弱加密）".to_string())
        }
    }
}
