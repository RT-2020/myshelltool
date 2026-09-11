//! 恢复密码生成（同步备份的"可带走凭据"）。
//!
//! 背景：资产同步用主密码派生 AES 密钥加密 Gist 载荷。若让用户**自己想**一个主密码，
//! 就会出现"弱口令 / 复用别处口令 / 干脆忘记"三种失败；若把恢复凭据交给外部密码管理器，
//! 又把"能否恢复"押在用户是否装了那个软件上。折中做法：应用生成一个高熵、**只用于这一份
//! 备份**的随机密码，用户点一下即可，并可随时在应用内查看/复制带走。
//!
//! 安全定位（诚实说明，不要误读成"免密更安全"）：
//! - 自动生成的密码由调用方决定是否用 DPAPI 落盘（见 src-tauri 的 `sync-recovery-password`）。
//!   落盘带来的实际风险与「免密」等价 —— 免密本来就把**由它派生的密钥**存在本机 DPAPI 里；
//!   多存一份明文密码，多出的能力只有"可跨机使用"，而这正是换机恢复的定义。
//! - 用户**手输**自己的密码时不应落盘（那是可能复用别处口令的秘密，见 AGENTS.md §8）。
//!
//! 纯函数 + 无 IO，可独立 `cargo test`。

use rand::Rng;

/// 默认生成长度。24 个字符、约 62^24 的取值空间 —— 远超任何离线爆破预算，
/// 同时短到用户愿意手动输入（换机场景要手输）。
pub const DEFAULT_LEN: usize = 24;

/// 去歧义字符集：去掉 `0/O`、`1/l/I` 这类肉眼易混的字符。
/// 恢复密码要被人从屏幕抄到另一台机器，读错一个字符就等于"密码错误"且极难归因。
const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";

/// 生成一个随机恢复密码。
///
/// `len` 为 0 时回退到 `DEFAULT_LEN`（调用方不应传 0，但不静默返回空串 —— 空密码
/// 会让"自动生成"变成"没有密码"，是 fail-open）。
pub fn generate_recovery_password(len: usize) -> String {
    let len = if len == 0 { DEFAULT_LEN } else { len };
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

/// 口令强度下限（与前端校验、后端 setup 校验保持一致的口径）。
/// 放在 core 是为了让"什么算够格的主密码"只有一处定义。
pub const MIN_MASTER_PASSWORD_LEN: usize = 6;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_password_has_requested_length() {
        for len in [8usize, DEFAULT_LEN, 64] {
            assert_eq!(generate_recovery_password(len).chars().count(), len);
        }
    }

    #[test]
    fn zero_length_falls_back_to_default_not_empty() {
        // 空密码 = "自动生成"退化成"没有密码"，必须回退而不是静默放行
        let pw = generate_recovery_password(0);
        assert_eq!(pw.chars().count(), DEFAULT_LEN);
        assert!(!pw.is_empty());
    }

    #[test]
    fn alphabet_excludes_ambiguous_characters() {
        // 供人眼抄写：0/O、1/l/I 必须不在字符集里
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            for c in generate_recovery_password(32).chars() {
                seen.insert(c);
            }
        }
        for bad in ['0', 'O', '1', 'l', 'I'] {
            assert!(!seen.contains(&bad), "字符集不应包含易混字符 {bad}");
        }
        // 且确实用到了大小写与数字三类（不是退化成单一类）
        assert!(seen.iter().any(|c| c.is_ascii_lowercase()));
        assert!(seen.iter().any(|c| c.is_ascii_uppercase()));
        assert!(seen.iter().any(|c| c.is_ascii_digit()));
    }

    #[test]
    fn successive_passwords_differ() {
        // 每次生成必须是新的随机值（否则等于所有人共用一份"自动生成"凭据）
        let a = generate_recovery_password(DEFAULT_LEN);
        let b = generate_recovery_password(DEFAULT_LEN);
        assert_ne!(a, b);
    }
}
