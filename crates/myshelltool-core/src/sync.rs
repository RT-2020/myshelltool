//! Gist 同步引擎（v1.3）—— 载荷结构 + 加解密封装 + 冲突检测。
//!
//! 职责边界：本模块只做「数据封装 + 加解密 + 冲突判定」的纯逻辑，
//! **不发 HTTP 请求**（reqwest 调用在 src-tauri 层，保持 core 可独立 cargo test）。
//!
//! 数据流（v2.7 起**只有一种载荷格式**，旧版格式一律拒绝、不做适配）：
//!   SyncVaultData（资产 + 凭据）──pack_vault / pack_vault_with_key──→ SyncPayload ──HTTP──→ Gist
//!   Gist ──HTTP──→ SyncPayload ──unpack_vault / unpack_vault_with_key──→ SyncVaultData
//!
//! 两条加密路径共用同一份密文语义：密码路径（`pack_vault`，随机 salt 随载荷）与
//! 会话密钥路径（`pack_vault_with_key`，**key 的派生 salt 同样随载荷**）——因此
//! 同一份载荷既可由会话密钥解，也可由主密码在任意机器上重建同一 key 解开。
//!
//! 冲突检测基于 local_rev（上次同步时记录的远端 rev）：
//!   - 拉取时比较「本地 local_rev」vs「远端 remote_rev」判断远端是否变过
//!   - 本地是否变过由调用方判断（比较本地资产 mtime 或内容 hash）

use serde::{Deserialize, Serialize};

use crate::crypto::{self, EncryptedBlob};
use crate::ConnectionAssetStore;

/// 同步载荷格式版本（未来加密算法/结构变更时升版，便于向后兼容）。
/// pub：src-tauri/sync.rs 在打包载荷时复用此常量，跨 crate 引用必须公开。
pub const PAYLOAD_VERSION: u32 = 1;

/// 存入加密载荷的凭据项（密码/私钥口令/托管私钥内容）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncCredentialItem {
    pub id: String,
    pub secret: String,
}

/// 完整的同步数据保管库（包含资产拓扑与关联凭据）。
/// 序列化为 JSON 后被对称加密保存在 SyncPayload.blob 中。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncVaultData {
    pub version: u32,
    pub assets_store: ConnectionAssetStore,
    #[serde(default)]
    pub credentials: Vec<SyncCredentialItem>,
}

impl SyncVaultData {
    pub fn new(assets_store: ConnectionAssetStore, credentials: Vec<SyncCredentialItem>) -> Self {
        Self {
            version: 1,
            assets_store,
            credentials,
        }
    }
}

/// 存入 Gist 的完整同步载荷。
///
/// 整个结构序列化成 JSON 后作为 Gist 内容上传。
/// `blob` 是加密后的资产数据；`version`/`remote_rev`/`updated_at` 是同步元数据（明文，不敏感）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncPayload {
    /// 载荷格式版本（当前 = 1）。
    pub version: u32,
    /// 加密后的 connection-assets.json（含 salt/nonce/ciphertext）。
    pub blob: EncryptedBlob,
    /// 远端版本号：每次 push 递增。用于冲突检测（比较本地记录的 vs 当前远端的）。
    pub remote_rev: u64,
    /// Gist 的 updated_at 时间戳（ISO8601），由 GitHub 返回，push 时记录。
    /// 用于显示「上次同步时间」+ 作为 local_rev 的来源。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// 本地同步状态（存在 app_data_dir/sync-state.json，DPAPI 不保护——它不含秘密）。
///
/// `local_rev` = 上次成功同步时远端的 remote_rev。下次同步时比较它判断远端是否变过。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    /// 上次同步时记录的远端 remote_rev（用于冲突检测）。None = 从未同步过。
    #[serde(default)]
    pub local_rev: Option<u64>,
    /// 上次同步时间（ISO8601，用于 UI 显示）。
    #[serde(default)]
    pub last_synced_at: Option<String>,
    /// Gist ID（首次 create_gist 后记录，后续 update 用）。
    #[serde(default)]
    pub gist_id: Option<String>,
    /// v1.6：是否启用自动同步（会话密钥已派生并 DPAPI 保护存盘）。
    /// 会话密钥本身不在本结构（存 SecretStore，credential id = "sync-session-key"）。
    #[serde(default)]
    pub auto_sync_enabled: bool,
    /// 是否同步凭据与私钥（默认 true）。
    #[serde(default = "default_sync_credentials_true")]
    pub sync_credentials: bool,
}

fn default_sync_credentials_true() -> bool {
    true
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            local_rev: None,
            last_synced_at: None,
            gist_id: None,
            auto_sync_enabled: false,
            sync_credentials: true,
        }
    }
}

/// 冲突检测结果（pull 时判定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncDecision {
    /// 本地和远端都没变（或首次同步远端为空）→ 无需操作。
    NoChange,
    /// 远端有更新，本地没变 → 直接拉取覆盖本地（安全，不丢数据）。
    PullRemote,
    /// 本地有更新，远端没变 → 直接推送覆盖远端（安全）。
    PushLocal,
    /// 双方都变了 → 冲突，需用户选择（本地/远端/取消）。
    Conflict,
}

/// 打包全量保管库（资产 + 凭据）：SyncVaultData → 加密 → SyncPayload。
///
/// 这是**唯一**的载荷打包入口（v2.7 删除了只打包资产的 `pack()`：它产出的载荷
/// 没有凭据段，且是旧格式适配的来源）。`remote_rev` 由调用方传入（push 后递增）。
pub fn pack_vault(
    vault: &SyncVaultData,
    master_password: &str,
    remote_rev: u64,
) -> Result<SyncPayload, String> {
    let json = serde_json::to_string(vault).map_err(|e| format!("序列化同步保管库失败: {e}"))?;
    let blob = crypto::encrypt(json.as_bytes(), master_password)?;
    Ok(SyncPayload {
        version: PAYLOAD_VERSION,
        blob,
        remote_rev,
        updated_at: None,
    })
}

/// 使用会话密钥（AES Key）打包全量保管库（v1.6 自动同步路径）。
///
/// `salt` = 该会话密钥的 Argon2id 派生 salt，**必须随载荷存**（v2.7 修复，见
/// `crypto::encrypt_with_key` 的注释）：这样同一份密文既能用本机会话密钥解，
/// 也能在换机后用「主密码 + salt」重建同一把 key 解开 —— 备份可恢复。
pub fn pack_vault_with_key(
    vault: &SyncVaultData,
    key: &[u8; 32],
    salt: &[u8],
    remote_rev: u64,
) -> Result<SyncPayload, String> {
    let json = serde_json::to_string(vault).map_err(|e| format!("序列化同步保管库失败: {e}"))?;
    let blob = crypto::encrypt_with_key(json.as_bytes(), key, salt)?;
    Ok(SyncPayload {
        version: PAYLOAD_VERSION,
        blob,
        remote_rev,
        updated_at: None,
    })
}

/// 解包：SyncPayload → 解密 → 解析为 SyncVaultData（兼容老版本仅资产 JSON 格式）。
pub fn unpack_vault(payload: &SyncPayload, master_password: &str) -> Result<SyncVaultData, String> {
    if payload.version != PAYLOAD_VERSION {
        return Err(format!(
            "不支持的同步载荷版本 {}（当前支持 {}）",
            payload.version, PAYLOAD_VERSION
        ));
    }
    let plaintext = crypto::decrypt(&payload.blob, master_password)?;
    parse_vault_plaintext(&plaintext)
}

/// 使用会话密钥（AES Key）解包全量保管库。
pub fn unpack_vault_with_key(
    payload: &SyncPayload,
    key: &[u8; 32],
) -> Result<SyncVaultData, String> {
    if payload.version != PAYLOAD_VERSION {
        return Err(format!(
            "不支持的同步载荷版本 {}（当前支持 {}）",
            payload.version, PAYLOAD_VERSION
        ));
    }
    let plaintext = crypto::decrypt_with_key(&payload.blob, key)?;
    parse_vault_plaintext(&plaintext)
}

/// 解析解密后的明文为 SyncVaultData。
///
/// **只认最新格式**（v2.7 决定：项目仅一位用户，旧版「纯 assets JSON」载荷不做适配）。
/// 旧载荷在此明确失败并提示重建，而不是被静默折叠成「有资产、无凭据」的半个保管库
/// ——后者会让用户以为恢复成功，实际连接时全部认证失败。
fn parse_vault_plaintext(plaintext: &[u8]) -> Result<SyncVaultData, String> {
    let json_str = String::from_utf8(plaintext.to_vec())
        .map_err(|e| format!("解密后非合法 UTF-8: {e}"))?;

    serde_json::from_str::<SyncVaultData>(&json_str).map_err(|_| {
        "解密数据不是合法的同步保管库（旧版纯资产载荷已不再支持）：请在本机执行一次「推送到云端」重建备份"
            .to_string()
    })
}

/// 用主密码 + **载荷自带的 salt** 重建「加密这份载荷所用的会话密钥」及其 salt。
///
/// 为什么必须按载荷的 salt 重建：会话密钥 = `Argon2id(主密码, salt)`，salt 不同 → key 不同。
/// 换机恢复时若随手用一个新随机 salt 存 key，本机免密（会话密钥）路径与主密码路径就会
/// 得到**两把不同的 key**，之后免密拉取在载荷上莫名失败（报「会话密钥失效」，极难归因）。
/// 用载荷的 salt 重建则两条路径天然一致：免密可用，主密码也能跨机恢复。
///
/// 返回 `(key, salt)`；载荷缺 salt（旧格式）直接拒绝。
pub fn session_key_for_payload(
    payload: &SyncPayload,
    master_password: &str,
) -> Result<([u8; 32], Vec<u8>), String> {
    if payload.version != PAYLOAD_VERSION {
        return Err(format!(
            "不支持的同步载荷版本 {}（当前支持 {}）",
            payload.version, PAYLOAD_VERSION
        ));
    }
    let salt = crypto::salt_bytes(&payload.blob)?;
    let key = crypto::derive_session_key(master_password, &salt)?;
    Ok((key, salt))
}

/// 冲突检测：根据本地状态 + 远端载荷判定同步决策。
///
/// - `local_has_changes`：本地资产自上次同步后是否改过（调用方判断，如比较 mtime/hash）
/// - `remote`：远端拉到的载荷（None = 远端为空/不存在）
/// - `state`：本地同步状态（local_rev = 上次同步的远端 rev）
pub fn decide(local_has_changes: bool, remote: Option<&SyncPayload>, state: &SyncState) -> SyncDecision {
    match remote {
        None => {
            // 远端没有数据
            if local_has_changes {
                SyncDecision::PushLocal // 首次推送
            } else {
                SyncDecision::NoChange
            }
        }
        Some(remote_payload) => {
            let remote_changed = match state.local_rev {
                None => true, // 本地从没同步过，远端有数据 = 远端是新的
                Some(local_rev) => remote_payload.remote_rev > local_rev,
            };
            match (local_has_changes, remote_changed) {
                (false, false) => SyncDecision::NoChange,
                (false, true) => SyncDecision::PullRemote,
                (true, false) => SyncDecision::PushLocal,
                (true, true) => SyncDecision::Conflict,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PW: &str = "test-master-password";

    fn sample_assets_json() -> &'static str {
        r#"{"assets":[{"id":"a1","name":"prod-db","host":"10.0.0.1","port":22,"username":"root","auth_method":"password","group":"生产/数据库","tags":[],"status":"idle","last_connected":""}],"groups":["生产/数据库"]}"#
    }

    /// 当前唯一载荷格式的样例保管库（v2.7 起不再有「纯资产」载荷）。
    fn sample_vault() -> SyncVaultData {
        let store: ConnectionAssetStore =
            serde_json::from_str(sample_assets_json()).expect("sample store");
        SyncVaultData::new(store, vec![])
    }

    #[test]
    fn unpack_wrong_password_fails() {
        let payload = pack_vault(&sample_vault(), TEST_PW, 1).expect("pack_vault");
        assert!(unpack_vault(&payload, "wrong-password").is_err());
    }

    #[test]
    fn unpack_wrong_version_fails() {
        let mut payload = pack_vault(&sample_vault(), TEST_PW, 1).expect("pack_vault");
        payload.version = 999; // 篡改版本号
        let err = unpack_vault(&payload, TEST_PW).expect_err("版本不符应失败");
        assert!(err.contains("不支持"), "错误文案应说明版本不支持: {err}");
    }

    #[test]
    fn payload_serializes_to_json_for_gist() {
        // SyncPayload 要能序列化成 JSON 存进 Gist
        let payload = pack_vault(&sample_vault(), TEST_PW, 42).expect("pack_vault");
        let json = serde_json::to_string(&payload).expect("serialize");
        assert!(json.contains("\"version\":1"));
        assert!(json.contains("\"remote_rev\":42"));
        // 反序列化回来应相等
        let back: SyncPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, payload);
    }

    // ─── 冲突检测逻辑（decide）───

    #[test]
    fn decide_first_push_when_remote_empty_and_local_changed() {
        let state = SyncState::default(); // local_rev = None
        let decision = decide(true, None, &state);
        assert_eq!(decision, SyncDecision::PushLocal);
    }

    #[test]
    fn decide_no_change_when_both_empty() {
        let state = SyncState::default();
        let decision = decide(false, None, &state);
        assert_eq!(decision, SyncDecision::NoChange);
    }

    #[test]
    fn decide_pull_remote_when_only_remote_changed() {
        // 本地没改，远端 rev 比本地记录的高
        let state = SyncState {
            local_rev: Some(3),
            ..Default::default()
        };
        let remote = pack_vault(&sample_vault(), TEST_PW, 5).expect("pack_vault"); // rev=5 > 3
        let decision = decide(false, Some(&remote), &state);
        assert_eq!(decision, SyncDecision::PullRemote);
    }

    #[test]
    fn decide_push_local_when_only_local_changed() {
        // 本地改了，远端 rev == 本地记录（远端没变）
        let state = SyncState {
            local_rev: Some(5),
            ..Default::default()
        };
        let remote = pack_vault(&sample_vault(), TEST_PW, 5).expect("pack_vault"); // rev=5 == 5
        let decision = decide(true, Some(&remote), &state);
        assert_eq!(decision, SyncDecision::PushLocal);
    }

    #[test]
    fn decide_conflict_when_both_changed() {
        // 本地改了，远端也改了（rev 更高）
        let state = SyncState {
            local_rev: Some(3),
            ..Default::default()
        };
        let remote = pack_vault(&sample_vault(), TEST_PW, 5).expect("pack_vault"); // rev=5 > 3
        let decision = decide(true, Some(&remote), &state);
        assert_eq!(decision, SyncDecision::Conflict);
    }

    #[test]
    fn decide_no_change_when_neither_changed() {
        let state = SyncState {
            local_rev: Some(5),
            ..Default::default()
        };
        let remote = pack_vault(&sample_vault(), TEST_PW, 5).expect("pack_vault");
        let decision = decide(false, Some(&remote), &state);
        assert_eq!(decision, SyncDecision::NoChange);
    }

    #[test]
    fn decide_pull_when_never_synced_but_remote_exists() {
        // 本地从没同步过（local_rev=None），远端有数据 → 拉取
        let state = SyncState::default();
        let remote = pack_vault(&sample_vault(), TEST_PW, 1).expect("pack_vault");
        let decision = decide(false, Some(&remote), &state);
        assert_eq!(decision, SyncDecision::PullRemote);
    }

    #[test]
    fn sync_state_serializes_and_defaults() {
        // SyncState 默认值应能序列化（首次写入 sync-state.json）
        let state = SyncState::default();
        let json = serde_json::to_string(&state).expect("serialize");
        let back: SyncState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.local_rev, None);
        assert_eq!(back.last_synced_at, None);
        assert_eq!(back.gist_id, None);
        assert!(back.sync_credentials);
    }

    #[test]
    fn pack_unpack_vault_roundtrip() {
        let store: ConnectionAssetStore = serde_json::from_str(sample_assets_json()).unwrap();
        let creds = vec![
            SyncCredentialItem {
                id: "a1:password".to_string(),
                secret: "super-secret-pwd".to_string(),
            },
            SyncCredentialItem {
                id: "a1:private_key".to_string(),
                secret: "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----".to_string(),
            },
        ];
        let vault = SyncVaultData::new(store.clone(), creds.clone());

        let payload = pack_vault(&vault, TEST_PW, 10).expect("pack_vault");
        let unpacked = unpack_vault(&payload, TEST_PW).expect("unpack_vault");

        assert_eq!(unpacked.assets_store.assets.len(), store.assets.len());
        assert_eq!(unpacked.credentials, creds);
    }

    #[test]
    fn unpack_vault_rejects_legacy_assets_only_payload() {
        // v2.7：旧版「纯资产 JSON」载荷**不再适配**（项目仅一位用户，明确只支持最新格式）。
        // 用当前 API 已造不出这种载荷，手工构造一个「加密后的纯资产 JSON」来验证被拒绝
        // ——宁可直接报错，也不静默折叠成「有资产、无凭据」的半个保管库。
        let blob = crypto::encrypt(sample_assets_json().as_bytes(), TEST_PW).expect("encrypt");
        let payload = SyncPayload {
            version: PAYLOAD_VERSION,
            blob,
            remote_rev: 2,
            updated_at: None,
        };
        let err = unpack_vault(&payload, TEST_PW).expect_err("旧格式载荷应被拒绝");
        assert!(err.contains("不再支持"), "错误文案要说明旧格式不再支持: {err}");
    }

    #[test]
    fn key_based_pack_unpack_vault_roundtrip() {
        let store: ConnectionAssetStore = serde_json::from_str(sample_assets_json()).unwrap();
        let creds = vec![SyncCredentialItem {
            id: "a1:passphrase".to_string(),
            secret: "pass123".to_string(),
        }];
        let vault = SyncVaultData::new(store, creds.clone());

        // 会话密钥的真实来源：Argon2id(主密码, salt)（见 src-tauri/sync.rs 的 save_session_key）
        let salt = [7u8; crypto::SALT_LEN];
        let key = crypto::derive_session_key(TEST_PW, &salt).expect("derive_session_key");
        let payload = pack_vault_with_key(&vault, &key, &salt, 5).expect("pack_vault_with_key");
        let unpacked = unpack_vault_with_key(&payload, &key).expect("unpack_vault_with_key");
        assert_eq!(unpacked.credentials, creds);

        // 【v2.7 关键回归】同一载荷必须也能被「主密码」解开（换机恢复的前提）：
        // salt 随载荷走 → 换一台机器用主密码 + 载荷里的 salt 重建的正是同一把 key。
        let by_password = unpack_vault(&payload, TEST_PW).expect("主密码路径应能解开");
        assert_eq!(by_password.credentials, creds);
        assert_eq!(by_password.assets_store.assets.len(), 1);
        // 认证加密不退化：错误主密码仍然解不开
        assert!(unpack_vault(&payload, "wrong-pw").is_err(), "错误主密码不应解开");
    }

    #[test]
    fn session_key_for_payload_rebuilds_the_encrypting_key() {
        // 「登录后免密推拉」的基石：本机存的会话密钥必须**按载荷的 salt** 重建，
        // 否则免密路径与主密码路径拿到两把 key，之后免密拉取会莫名失败。
        let salt = [9u8; crypto::SALT_LEN];
        let key = crypto::derive_session_key(TEST_PW, &salt).expect("derive");
        let payload = pack_vault_with_key(&sample_vault(), &key, &salt, 7).expect("pack");

        let (rebuilt, rebuilt_salt) = session_key_for_payload(&payload, TEST_PW).expect("rebuild");
        assert_eq!(rebuilt, key, "必须重建出同一把 key");
        assert_eq!(rebuilt_salt, salt, "salt 也要一并带出来（写载荷时要用）");
        assert!(unpack_vault_with_key(&payload, &rebuilt).is_ok(), "重建的 key 应能解开载荷");
        assert!(unpack_vault(&payload, TEST_PW).is_ok(), "主密码路径同样可用（跨机恢复）");
        // 别的密码重建出的 key 必须解不开（不是 fail-open）
        let (wrong, _) = session_key_for_payload(&payload, "wrong-pw").expect("rebuild wrong");
        assert!(unpack_vault_with_key(&payload, &wrong).is_err(), "错误密码不得解开");
    }

    #[test]
    fn vault_roundtrip_preserves_credentials() {
        // 事故防线：曾有一个「只返回 assets_store」的 `unpack()` 便捷接口，被重置主密码
        // 路径拿去重打包 → Gist 备份里的凭据整段被抹掉。v2.7 直接删掉该接口，
        // 载荷的读写**只能**走保管库级 API，从类型上杜绝这类丢失。
        let store: ConnectionAssetStore = serde_json::from_str(sample_assets_json()).unwrap();
        let creds = vec![SyncCredentialItem {
            id: "a1:password".to_string(),
            secret: "pwd".to_string(),
        }];
        let vault = SyncVaultData::new(store, creds.clone());

        // 换密码 = 用新密码重新打包同一份 vault
        let payload = pack_vault(&vault, "new-pw", 3).expect("pack_vault");
        let back = unpack_vault(&payload, "new-pw").expect("unpack_vault");
        assert_eq!(back.credentials, creds, "重打包不得丢凭据");
        assert_eq!(back.assets_store.assets.len(), vault.assets_store.assets.len());
    }
}

