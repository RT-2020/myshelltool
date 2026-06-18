//! Gist 同步引擎（v1.3）—— 载荷结构 + 加解密封装 + 冲突检测。
//!
//! 职责边界：本模块只做「数据封装 + 加解密 + 冲突判定」的纯逻辑，
//! **不发 HTTP 请求**（reqwest 调用在 src-tauri 层，保持 core 可独立 cargo test）。
//!
//! 数据流：
//!   connection-assets.json ──pack()──→ SyncPayload（含密文）──HTTP──→ Gist
//!   Gist ──HTTP──→ SyncPayload ──unpack()──→ connection-assets.json
//!
//! 冲突检测基于 local_rev（上次同步时记录的远端 updated_at）：
//!   - 拉取时比较「本地 local_rev」vs「远端 updated_at」判断远端是否变过
//!   - 本地是否变过由调用方判断（比较本地资产 mtime 或内容 hash）

use serde::{Deserialize, Serialize};

use crate::crypto::{self, EncryptedBlob};

/// 同步载荷格式版本（未来加密算法/结构变更时升版，便于向后兼容）。
const PAYLOAD_VERSION: u32 = 1;

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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

/// 打包：资产 JSON → 加密 → SyncPayload。
///
/// `remote_rev` 由调用方传入（通常是当前远端的 rev，push 后递增）。
pub fn pack(json: &str, master_password: &str, remote_rev: u64) -> Result<SyncPayload, String> {
    let blob = crypto::encrypt(json.as_bytes(), master_password)?;
    Ok(SyncPayload {
        version: PAYLOAD_VERSION,
        blob,
        remote_rev,
        updated_at: None, // 由 src-tauri 层 push 后填 GitHub 返回的 updated_at
    })
}

/// 解包：SyncPayload → 解密 → 资产 JSON 字符串。
pub fn unpack(payload: &SyncPayload, master_password: &str) -> Result<String, String> {
    if payload.version != PAYLOAD_VERSION {
        return Err(format!(
            "不支持的同步载荷版本 {}（当前支持 {}）",
            payload.version, PAYLOAD_VERSION
        ));
    }
    let plaintext = crypto::decrypt(&payload.blob, master_password)?;
    String::from_utf8(plaintext).map_err(|e| format!("解密后非合法 UTF-8: {e}"))
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

    #[test]
    fn pack_unpack_roundtrip() {
        let json = sample_assets_json();
        let payload = pack(json, TEST_PW, 1).expect("pack");
        let unpacked = unpack(&payload, TEST_PW).expect("unpack");
        assert_eq!(unpacked, json);
    }

    #[test]
    fn unpack_wrong_password_fails() {
        let payload = pack(sample_assets_json(), TEST_PW, 1).expect("pack");
        let result = unpack(&payload, "wrong-password");
        assert!(result.is_err());
    }

    #[test]
    fn unpack_wrong_version_fails() {
        let mut payload = pack(sample_assets_json(), TEST_PW, 1).expect("pack");
        payload.version = 999; // 篡改版本号
        let result = unpack(&payload, TEST_PW);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("不支持"));
    }

    #[test]
    fn payload_serializes_to_json_for_gist() {
        // SyncPayload 要能序列化成 JSON 存进 Gist
        let payload = pack(sample_assets_json(), TEST_PW, 42).expect("pack");
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
        let remote = pack(sample_assets_json(), TEST_PW, 5).expect("pack"); // rev=5 > 3
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
        let remote = pack(sample_assets_json(), TEST_PW, 5).expect("pack"); // rev=5 == 5
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
        let remote = pack(sample_assets_json(), TEST_PW, 5).expect("pack"); // rev=5 > 3
        let decision = decide(true, Some(&remote), &state);
        assert_eq!(decision, SyncDecision::Conflict);
    }

    #[test]
    fn decide_no_change_when_neither_changed() {
        let state = SyncState {
            local_rev: Some(5),
            ..Default::default()
        };
        let remote = pack(sample_assets_json(), TEST_PW, 5).expect("pack");
        let decision = decide(false, Some(&remote), &state);
        assert_eq!(decision, SyncDecision::NoChange);
    }

    #[test]
    fn decide_pull_when_never_synced_but_remote_exists() {
        // 本地从没同步过（local_rev=None），远端有数据 → 拉取
        let state = SyncState::default();
        let remote = pack(sample_assets_json(), TEST_PW, 1).expect("pack");
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
    }
}
