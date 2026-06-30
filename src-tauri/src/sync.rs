//! Gist 同步 Tauri 命令层（v1.3）。
//!
//! 粘合 core 层（crypto + sync 纯逻辑）与 HTTP（reqwest 调 Gist API）+ 本地文件
//! （connection-assets.json + sync-state.json）+ 凭据（SecretStore 读 github-pat）。
//!
//! 7 个命令（前端经 invokeBackend 调用）：
//! - sync_setup：首次设置（主密码 + 可选 gist_id 拉取已有）
//! - sync_push：加密本地资产 → 上传 Gist
//! - sync_pull：拉 Gist → 解密 → 冲突检测 → 返回决策（前端据此弹窗或直接覆盖）
//! - sync_status：返回同步配置状态（是否已配置/上次同步时间/gist_id）
//! - sync_reset_master_password：重置主密码（需旧密码验证）
//! - sync_clear：清空同步配置（忘了主密码的逃生口）

use std::path::PathBuf;

use myshelltool_core::sync::{self, SyncDecision, SyncPayload, SyncState};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

const GITHUB_API_BASE: &str = "https://api.github.com";
/// Gist 内容的文件名（Gist 用文件名索引内容）。
const GIST_FILENAME: &str = "myshelltool-sync.json";

// ─── sync-state.json 读写 ───

fn sync_state_path(state: &AppState) -> Result<PathBuf, String> {
    state
        .asset_store_path
        .parent()
        .map(|dir| dir.join("sync-state.json"))
        .ok_or_else(|| "无法解析 app_data_dir（asset_store_path 无父目录）".to_string())
}

fn load_sync_state(state: &AppState) -> Result<SyncState, String> {
    let path = sync_state_path(state)?;
    if !path.exists() {
        return Ok(SyncState::default());
    }
    let json = std::fs::read_to_string(&path).map_err(|e| format!("读 sync-state.json: {e}"))?;
    if json.trim().is_empty() {
        return Ok(SyncState::default());
    }
    serde_json::from_str(&json).map_err(|e| format!("解析 sync-state.json: {e}"))
}

fn save_sync_state(state: &AppState, sync_state: &SyncState) -> Result<(), String> {
    let path = sync_state_path(state)?;
    let json = serde_json::to_string_pretty(sync_state).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("写 sync-state.json: {e}"))
}

/// 读 GitHub PAT（从 SecretStore，credential id = "github-pat"）。
fn read_github_pat(state: &AppState) -> Result<String, String> {
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    store
        .read("github-pat")?
        .ok_or_else(|| "未配置 GitHub PAT（请先在设置中配置）".to_string())
}

/// 会话密钥 credential id（v1.6 自动同步）。
const SESSION_KEY_ID: &str = "sync-session-key";

/// v1.6 自动同步：会话密钥 + 固定 salt 的持久化载体。
///
/// DPAPI 加密后存 SecretStore（credential id = SESSION_KEY_ID）。
/// 存储格式：`base64(key):base64(salt)`——key 是 32 字节 AES 密钥，salt 是派生时用的 16 字节。
/// salt 不敏感（与 blob.salt 同理），与 key 一起存是为了让 reset_master_password 时
/// 能用同一 salt 重新派生新 key 验证（实际 reset 会重新派生 + 重存）。
struct SessionKey {
    key: [u8; 32],
    _salt: Vec<u8>, // 派生时的 salt（key-based 路径下不参与加解密，仅留档）
}

/// 派生会话密钥并 DPAPI 加密存盘（v1.6 启用自动同步）。
///
/// 用主密码 + 随机 salt 派生 32 字节 AES key，连同 salt 一起序列化后交 SecretStore
/// （DPAPI User scope 加密）。主密码派生后即丢弃，不落盘。
fn save_session_key(state: &AppState, master_password: &str) -> Result<SessionKey, String> {
    use myshelltool_core::crypto;
    let mut salt = vec![0u8; crypto::SALT_LEN];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
    let key = crypto::derive_session_key(master_password, &salt)?;

    // 存储格式：base64(key):base64(salt)。SecretStore 的 DPAPI codec 会整体加密。
    let payload = format!("{}:{}", b64(&key), b64(&salt));
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    store.save(SESSION_KEY_ID, &payload)?;

    Ok(SessionKey { key, _salt: salt })
}

/// 从 SecretStore 读会话密钥（DPAPI 解密）。未配置返回 None（调用方回退手动模式）。
fn read_session_key(state: &AppState) -> Result<Option<[u8; 32]>, String> {
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    let payload = match store.read(SESSION_KEY_ID)? {
        Some(s) => s,
        None => return Ok(None), // 未启用自动同步
    };
    let (key_b64, _salt_b64) = payload
        .split_once(':')
        .ok_or_else(|| "会话密钥载荷格式损坏".to_string())?;
    let key_bytes = b64_decode(key_b64)?;
    let key: [u8; 32] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "会话密钥长度异常".to_string())?;
    Ok(Some(key))
}

/// 删除会话密钥（关闭自动同步 / 重置密码 / 清空同步）。
fn delete_session_key(state: &AppState) -> Result<(), String> {
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    store.delete(SESSION_KEY_ID).map(|_| ())
}

// ─── base64 小工具（与 core::crypto 内部实现一致，sync 命令层用于会话密钥存盘）───
fn b64(data: &[u8]) -> String {
    // 复用标准 base64（与 core crypto 的 base64_encode 同算法，避免在命令层再依赖 core 私有函数）
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(CHARS[((n >> 18) & 63) as usize] as char);
        out.push(CHARS[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { CHARS[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { CHARS[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if s.len() % 4 != 0 {
        return Err(format!("invalid base64 length: {}", s.len()));
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    for chunk in s.as_bytes().chunks(4) {
        let mut vals = [0u8; 4];
        let mut pad = 0;
        for (i, &b) in chunk.iter().enumerate() {
            vals[i] = if b == b'=' {
                pad += 1;
                0
            } else {
                CHARS.iter().position(|&c| c == b)
                    .ok_or_else(|| format!("invalid base64 char: {}", b as char))? as u8
            };
        }
        let n = ((vals[0] as u32) << 18) | ((vals[1] as u32) << 12)
            | ((vals[2] as u32) << 6) | (vals[3] as u32);
        out.push((n >> 16) as u8);
        if pad < 2 { out.push((n >> 8) as u8); }
        if pad < 1 { out.push(n as u8); }
    }
    Ok(out)
}

/// 读本地资产 JSON。
///
/// **文件不存在 → 降级为空资产**（首次使用、尚未创建 connection-assets.json 的合法场景）。
/// **其他 IO 错误（权限/磁盘/损坏）→ 返回 Err**（绝不能静默降级成空资产再覆盖远端，
/// 那会静默销毁 Gist 上的备份）。
///
/// 这是对原 `unwrap_or_else(|_| empty)` masking fallback 的根因修复：
/// 原版把 NotFound 和真错误一并吞了，sync_push 会在资产文件损坏时把空资产推到 Gist。
fn read_local_assets(state: &AppState) -> Result<String, String> {
    match std::fs::read_to_string(&state.asset_store_path) {
        Ok(json) => Ok(json),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(r#"{"assets":[],"groups":[]}"#.to_string())
        }
        Err(e) => Err(format!(
            "读取本地资产文件失败（{}）：{e}",
            state.asset_store_path.display()
        )),
    }
}

// ─── Gist API 客户端 ───

#[derive(Serialize)]
struct CreateGistRequest<'a> {
    description: &'a str,
    #[serde(rename = "public")]
    _public: bool,
    files: std::collections::HashMap<&'a str, GistFileContent<'a>>,
}

#[derive(Serialize)]
struct GistFileContent<'a> {
    content: &'a str,
}

#[derive(Deserialize, Debug)]
struct GistResponse {
    id: String,
    updated_at: Option<String>,
}

#[derive(Deserialize, Debug)]
struct GistGetResponse {
    updated_at: Option<String>,
    files: Option<std::collections::HashMap<String, GistFileMeta>>,
}

#[derive(Deserialize, Debug)]
struct GistFileMeta {
    content: Option<String>,
}

/// 创建 Gist（首次推送）。返回 (gist_id, updated_at)。
async fn gist_create(pat: &str, content: &str) -> Result<(String, Option<String>), String> {
    let mut files = std::collections::HashMap::new();
    files.insert(
        GIST_FILENAME,
        GistFileContent { content },
    );
    let body = CreateGistRequest {
        description: "myshelltool connection assets sync (encrypted)",
        _public: false, // 私有 Gist
        files,
    };
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{GITHUB_API_BASE}/gists"))
        .header("Authorization", format!("Bearer {pat}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "myshelltool")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gist create 请求失败: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Gist create 失败 (HTTP {status}): {text}"));
    }
    let gist: GistResponse = resp
        .json()
        .await
        .map_err(|e| format!("Gist create 响应解析失败 (HTTP {status}): {e}"))?;
    Ok((gist.id, gist.updated_at))
}

/// 获取 Gist 内容 + updated_at。Gist 不存在返回 Ok(None)。
async fn gist_get(pat: &str, gist_id: &str) -> Result<Option<(String, Option<String>)>, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{GITHUB_API_BASE}/gists/{gist_id}"))
        .header("Authorization", format!("Bearer {pat}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "myshelltool")
        .send()
        .await
        .map_err(|e| format!("Gist get 请求失败: {e}"))?;

    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None); // Gist 不存在（可能被手动删了）
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Gist get 失败 (HTTP {status}): {text}"));
    }
    let gist: GistGetResponse = resp
        .json()
        .await
        .map_err(|e| format!("Gist get 响应解析失败: {e}"))?;
    let content = gist
        .files
        .and_then(|f| f.get(GIST_FILENAME).and_then(|m| m.content.clone()))
        .ok_or_else(|| "Gist 中无 sync 文件".to_string())?;
    Ok(Some((content, gist.updated_at)))
}

/// 更新 Gist 内容。返回 updated_at。
async fn gist_update(pat: &str, gist_id: &str, content: &str) -> Result<Option<String>, String> {
    let mut files = std::collections::HashMap::new();
    files.insert(GIST_FILENAME, GistFileContent { content });
    let body = CreateGistRequest {
        description: "myshelltool connection assets sync (encrypted)",
        _public: false,
        files,
    };
    let client = reqwest::Client::new();
    let resp = client
        .patch(format!("{GITHUB_API_BASE}/gists/{gist_id}"))
        .header("Authorization", format!("Bearer {pat}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "myshelltool")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gist update 请求失败: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Gist update 失败 (HTTP {status}): {text}"));
    }
    let gist: GistResponse = resp
        .json()
        .await
        .map_err(|e| format!("Gist update 响应解析失败 (HTTP {status}): {e}"))?;
    Ok(gist.updated_at)
}

// ─── Tauri 命令 ───

#[derive(Debug, Clone, Serialize)]
pub struct SyncStatusResult {
    /// 是否已配置同步（有 gist_id）。
    pub configured: bool,
    /// 上次同步时间（ISO8601）。
    pub last_synced_at: Option<String>,
    /// Gist ID（脱敏，只显示后 6 位 + ...）。
    pub gist_id_masked: Option<String>,
    /// 是否配置了 GitHub PAT。
    pub pat_configured: bool,
    /// v1.6：是否启用自动同步（会话密钥已派生）。
    pub auto_sync_enabled: bool,
}

#[tauri::command]
pub async fn sync_status(state: State<'_, AppState>) -> Result<SyncStatusResult, String> {
    let sync_state = load_sync_state(&state)?;
    let pat_configured = {
        let store = myshelltool_core::SecretStore::new(
            &state.secret_store_dir,
            Box::new(crate::dpapi_codec::DpapiCodec),
        );
        store.get_status("github-pat")?.exists
    };
    let gist_id_masked = sync_state.gist_id.as_ref().map(|id| {
        if id.len() <= 6 {
            id.clone()
        } else {
            format!("...{}", &id[id.len() - 6..])
        }
    });
    Ok(SyncStatusResult {
        configured: sync_state.gist_id.is_some(),
        last_synced_at: sync_state.last_synced_at,
        gist_id_masked,
        pat_configured,
        auto_sync_enabled: sync_state.auto_sync_enabled,
    })
}

/// sync_setup 返回结果（前端据此决定下一步）。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum SyncSetupResult {
    /// 首次设置成功（创建了新 Gist 并推送）。
    Created { gist_id_masked: String },
    /// 拉取到已有 Gist 数据（用户填了 gist_id），返回解密后的资产 JSON 供前端确认导入。
    PulledRemote { assets_json: String },
    /// 已存在同步配置，无需重复 setup。
    AlreadyConfigured { gist_id_masked: String },
}

/// 首次设置同步：验证主密码 + PAT，创建/拉取 Gist。
///
/// - `gist_id` 为空：创建新 Gist，推送当前本地资产（加密）。
/// - `gist_id` 非空：拉取已有 Gist，解密返回资产（换机器场景）。
#[tauri::command]
pub async fn sync_setup(
    state: State<'_, AppState>,
    master_password: String,
    gist_id: Option<String>,
) -> Result<SyncSetupResult, String> {
    if master_password.trim().is_empty() {
        return Err("主密码不能为空".to_string());
    }
    let pat = read_github_pat(&state)?;

    // 检查是否已配置
    let mut sync_state = load_sync_state(&state)?;
    if sync_state.gist_id.is_some() {
        let masked = mask_gist_id(sync_state.gist_id.as_deref().unwrap());
        return Ok(SyncSetupResult::AlreadyConfigured { gist_id_masked: masked });
    }

    if let Some(existing_gist_id) = gist_id.filter(|s| !s.trim().is_empty()) {
        // 换机器/导入场景：拉取已有 Gist
        let remote = gist_get(&pat, &existing_gist_id)
            .await?
            .ok_or_else(|| format!("Gist {existing_gist_id} 不存在"))?;
        let (content, updated_at) = remote;
        let payload: SyncPayload = serde_json::from_str(&content)
            .map_err(|e| format!("Gist 内容非合法同步载荷: {e}"))?;
        let assets_json = sync::unpack(&payload, &master_password)?;

        // 记录 sync state（不导入资产，让前端确认后再 import）
        sync_state.gist_id = Some(existing_gist_id.clone());
        sync_state.local_rev = Some(payload.remote_rev);
        sync_state.last_synced_at = updated_at;
        save_sync_state(&state, &sync_state)?;

        Ok(SyncSetupResult::PulledRemote { assets_json })
    } else {
        // 首次推送：加密当前本地资产 → 创建 Gist
        let local_json = read_local_assets(&state)?;
        let payload = sync::pack(&local_json, &master_password, 1)?;
        let payload_json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
        let (new_gist_id, updated_at) = gist_create(&pat, &payload_json).await?;

        sync_state.gist_id = Some(new_gist_id.clone());
        sync_state.local_rev = Some(1);
        sync_state.last_synced_at = updated_at;
        save_sync_state(&state, &sync_state)?;

        Ok(SyncSetupResult::Created {
            gist_id_masked: mask_gist_id(&new_gist_id),
        })
    }
}

/// sync_push 返回结果。
#[derive(Debug, Clone, Serialize)]
pub struct SyncPushResult {
    pub success: bool,
    pub message: String,
    pub new_rev: Option<u64>,
}

/// 推送本地资产到 Gist（加密）。
///
/// **v1.6 自动同步**：`master_password` 为空时尝试用会话密钥（key-based 路径）；
/// 非空时走传统主密码路径（向后兼容）。
#[tauri::command]
pub async fn sync_push(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<SyncPushResult, String> {
    let mut sync_state = load_sync_state(&state)?;
    let gist_id = sync_state
        .gist_id
        .clone()
        .ok_or_else(|| "未配置同步（请先 sync_setup）".to_string())?;
    let pat = read_github_pat(&state)?;

    let local_json = read_local_assets(&state)?;
    let new_rev = sync_state.local_rev.unwrap_or(0) + 1;

    // v1.6：优先会话密钥路径（master_password 为空时），否则走主密码派生
    let payload = if master_password.trim().is_empty() {
        // 会话密钥路径：自动同步 / 手动但已启用自动同步
        let key = read_session_key(&state)?
            .ok_or_else(|| "未启用自动同步，需提供主密码".to_string())?;
        let blob = myshelltool_core::crypto::encrypt_with_key(local_json.as_bytes(), &key)?;
        myshelltool_core::sync::SyncPayload {
            version: myshelltool_core::sync::PAYLOAD_VERSION,
            blob,
            remote_rev: new_rev,
            updated_at: None,
        }
    } else {
        sync::pack(&local_json, &master_password, new_rev)?
    };
    let payload_json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;

    let updated_at = gist_update(&pat, &gist_id, &payload_json).await?;

    sync_state.local_rev = Some(new_rev);
    sync_state.last_synced_at = updated_at;
    save_sync_state(&state, &sync_state)?;

    Ok(SyncPushResult {
        success: true,
        message: format!("已推送（rev {new_rev}）"),
        new_rev: Some(new_rev),
    })
}

/// sync_pull 返回结果（含冲突决策）。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "decision")]
pub enum SyncPullResult {
    /// 无需操作（双方都没变）。
    NoChange,
    /// 安全拉取：远端更新，本地没变。assets_json 是解密后的资产，前端直接导入。
    Pulled { assets_json: String, new_rev: u64 },
    /// 本地比远端新，建议 push 而非 pull。
    LocalNewer,
    /// 冲突：双方都变了。返回本地+远端资产 JSON，前端弹窗让用户选。
    Conflict {
        local_json: String,
        remote_json: String,
        remote_rev: u64,
    },
}

/// 拉取 Gist + 冲突检测。
///
/// **v1.6 自动同步**：`master_password` 为空时尝试用会话密钥解密（key-based 路径）。
/// 注意：会话密钥只能解 key-based 加密的载荷；若 Gist 上是旧的主密码加密载荷，
/// 会话密钥解密失败 → 返回明确错误引导用户手动输入主密码。
#[tauri::command]
pub async fn sync_pull(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<SyncPullResult, String> {
    let sync_state = load_sync_state(&state)?;
    let gist_id = sync_state
        .gist_id
        .clone()
        .ok_or_else(|| "未配置同步（请先 sync_setup）".to_string())?;
    let pat = read_github_pat(&state)?;

    // 1. 拉远端
    let remote_raw = gist_get(&pat, &gist_id)
        .await?
        .ok_or_else(|| "Gist 不存在（可能被手动删除）".to_string())?;
    let (content, updated_at) = remote_raw;
    let remote_payload: SyncPayload = serde_json::from_str(&content)
        .map_err(|e| format!("Gist 内容非合法同步载荷: {e}"))?;

    // 2. 本地是否有变更：比较本地资产 mtime vs 上次同步时间
    //    （粗略判定：本地文件 mtime > last_synced_at 则视为有变更）
    let local_has_changes = has_local_changes_since_last_sync(&state, &sync_state);

    // 3. 冲突判定
    let decision = sync::decide(local_has_changes, Some(&remote_payload), &sync_state);

    match decision {
        SyncDecision::NoChange => Ok(SyncPullResult::NoChange),
        SyncDecision::PullRemote => {
            // 安全拉取：解密远端，直接覆盖本地
            let remote_json = decrypt_payload(&state, &master_password, &remote_payload)?;
            std::fs::write(&state.asset_store_path, &remote_json)
                .map_err(|e| format!("写回 connection-assets.json: {e}"))?;

            let mut new_state = sync_state;
            new_state.local_rev = Some(remote_payload.remote_rev);
            new_state.last_synced_at = updated_at;
            save_sync_state(&state, &new_state)?;

            Ok(SyncPullResult::Pulled {
                assets_json: remote_json,
                new_rev: remote_payload.remote_rev,
            })
        }
        SyncDecision::PushLocal => Ok(SyncPullResult::LocalNewer),
        SyncDecision::Conflict => {
            // 冲突：返回双方 JSON，前端弹窗让用户选
            let local_json = read_local_assets(&state)?;
            let remote_json = decrypt_payload(&state, &master_password, &remote_payload)?;
            Ok(SyncPullResult::Conflict {
                local_json,
                remote_json,
                remote_rev: remote_payload.remote_rev,
            })
        }
    }
}

/// v1.6 统一解密 helper：优先会话密钥（master_password 为空时），回退主密码。
///
/// - 远端载荷 salt 为空（key-based 加密）→ 必须用会话密钥
/// - 远端载荷 salt 非空（主密码加密）→ 必须用主密码（会话密钥解不开，见单测 key_based_and_password_based_are_incompatible）
fn decrypt_payload(
    state: &AppState,
    master_password: &str,
    payload: &SyncPayload,
) -> Result<String, String> {
    if payload.blob.salt.is_empty() {
        // key-based 载荷：必须会话密钥
        let key = read_session_key(state)?
            .ok_or_else(|| "此 Gist 载荷由会话密钥加密，但本机未启用自动同步".to_string())?;
        let plaintext = myshelltool_core::crypto::decrypt_with_key(&payload.blob, &key)?;
        String::from_utf8(plaintext).map_err(|e| format!("解密后非合法 UTF-8: {e}"))
    } else if master_password.trim().is_empty() {
        // 主密码载荷但未提供密码 → 引导用户输入
        Err("此 Gist 载荷需主密码解密（旧版或他机加密），请输入主密码".to_string())
    } else {
        sync::unpack(payload, master_password)
    }
}

/// 用户在冲突对话框选择后，强制用某一方的数据覆盖。
///
/// `choice`: "local" | "remote"
/// - local：加密本地数据推送（覆盖远端）
/// - remote：用 remote_json 覆盖本地
#[tauri::command]
pub async fn sync_resolve_conflict(
    state: State<'_, AppState>,
    master_password: String,
    choice: String,
    remote_json: String,
    remote_rev: u64,
) -> Result<(), String> {
    let pat = read_github_pat(&state)?;
    let sync_state = load_sync_state(&state)?;
    let gist_id = sync_state
        .gist_id
        .clone()
        .ok_or_else(|| "未配置同步".to_string())?;

    match choice.as_str() {
        "local" => {
            // 用本地覆盖远端：推送本地（v1.6：优先会话密钥）
            let local_json = read_local_assets(&state)?;
            let new_rev = remote_rev + 1;
            let payload = if master_password.trim().is_empty() {
                let key = read_session_key(&state)?
                    .ok_or_else(|| "未启用自动同步，需提供主密码".to_string())?;
                let blob = myshelltool_core::crypto::encrypt_with_key(local_json.as_bytes(), &key)?;
                myshelltool_core::sync::SyncPayload {
                    version: myshelltool_core::sync::PAYLOAD_VERSION,
                    blob,
                    remote_rev: new_rev,
                    updated_at: None,
                }
            } else {
                sync::pack(&local_json, &master_password, new_rev)?
            };
            let payload_json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
            let updated_at = gist_update(&pat, &gist_id, &payload_json).await?;

            let mut new_state = sync_state;
            new_state.local_rev = Some(new_rev);
            new_state.last_synced_at = updated_at;
            save_sync_state(&state, &new_state)?;
        }
        "remote" => {
            // 用远端覆盖本地
            std::fs::write(&state.asset_store_path, &remote_json)
                .map_err(|e| format!("写回 connection-assets.json: {e}"))?;
            let mut new_state = sync_state;
            new_state.local_rev = Some(remote_rev);
            new_state.last_synced_at = Some(chrono::Utc::now().to_rfc3339());
            save_sync_state(&state, &new_state)?;
        }
        other => return Err(format!("无效的冲突选择: {other}")),
    }
    Ok(())
}

/// 重置主密码（需验证旧密码）。
///
/// 重新加密当前本地数据并用新密码推送（gist_id 不变）。
#[tauri::command]
pub async fn sync_reset_master_password(
    state: State<'_, AppState>,
    old_password: String,
    new_password: String,
) -> Result<(), String> {
    if new_password.trim().is_empty() {
        return Err("新主密码不能为空".to_string());
    }
    let sync_state = load_sync_state(&state)?;
    let gist_id = sync_state
        .gist_id
        .clone()
        .ok_or_else(|| "未配置同步".to_string())?;
    let pat = read_github_pat(&state)?;

    // 验证旧密码：拉远端用旧密码解密
    let remote_raw = gist_get(&pat, &gist_id)
        .await?
        .ok_or_else(|| "Gist 不存在".to_string())?;
    let (content, _) = remote_raw;
    let payload: SyncPayload = serde_json::from_str(&content)
        .map_err(|e| format!("Gist 内容非合法载荷: {e}"))?;
    // 用旧密码解密——失败即旧密码错误
    let assets_json = sync::unpack(&payload, &old_password)
        .map_err(|_| "旧主密码错误".to_string())?;

    // 用新密码重新加密推送
    let new_rev = sync_state.local_rev.unwrap_or(0) + 1;
    let new_payload = sync::pack(&assets_json, &new_password, new_rev)?;
    let payload_json = serde_json::to_string(&new_payload).map_err(|e| e.to_string())?;
    let updated_at = gist_update(&pat, &gist_id, &payload_json).await?;

    let mut new_state = sync_state;
    new_state.local_rev = Some(new_rev);
    new_state.last_synced_at = updated_at;
    // v1.6：若已启用自动同步，用新密码重新派生会话密钥（保持一致性，旧密钥失效）
    if new_state.auto_sync_enabled {
        save_session_key(&state, &new_password)?;
    }
    save_sync_state(&state, &new_state)?;
    Ok(())
}

/// 清空同步配置（忘了主密码的逃生口）。
///
/// 删除本地 sync-state.json + 会话密钥。Gist 上的数据保留（用户可手动去 GitHub 删）。
#[tauri::command]
pub async fn sync_clear(state: State<'_, AppState>) -> Result<(), String> {
    let path = sync_state_path(&state)?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| format!("删除 sync-state.json: {e}"))?;
    }
    // v1.6：连同会话密钥一起清（容错：可能不存在）
    let _ = delete_session_key(&state);
    Ok(())
}

// ─── v1.6 自动同步命令 ───

/// sync_enable_auto_sync 返回结果。
#[derive(Debug, Clone, Serialize)]
pub struct AutoSyncResult {
    pub enabled: bool,
    pub message: String,
}

/// v1.6 启用自动同步：验证主密码 → 派生会话密钥 → DPAPI 加密存 SecretStore。
///
/// 必须先完成 sync_setup（有 gist_id）。主密码派生后即丢弃，仅 DPAPI 密文持久化。
/// 会话密钥的加解密能力与主密码等价（同一 Argon2id 派生），但绑定本机 Windows 用户。
#[tauri::command]
pub async fn sync_enable_auto_sync(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<AutoSyncResult, String> {
    if master_password.trim().is_empty() {
        return Err("主密码不能为空".to_string());
    }
    let mut sync_state = load_sync_state(&state)?;
    let gist_id = sync_state
        .gist_id
        .clone()
        .ok_or_else(|| "未配置同步（请先完成 sync_setup）".to_string())?;

    // 验证主密码正确性：拉远端用主密码解密（兼容旧载荷），或本地无资产时跳过验证
    let pat = read_github_pat(&state)?;
    if let Some((content, _)) = gist_get(&pat, &gist_id).await? {
        let payload: SyncPayload = serde_json::from_str(&content)
            .map_err(|e| format!("Gist 内容非合法载荷: {e}"))?;
        // 仅当载荷是主密码加密的（salt 非空）才验证；key-based 载荷跳过（无法用主密码验）
        if !payload.blob.salt.is_empty() {
            sync::unpack(&payload, &master_password)
                .map_err(|_| "主密码错误".to_string())?;
        }
    }

    // 派生会话密钥 + DPAPI 加密存盘
    save_session_key(&state, &master_password)?;

    sync_state.auto_sync_enabled = true;
    save_sync_state(&state, &sync_state)?;

    Ok(AutoSyncResult {
        enabled: true,
        message: "自动同步已启用（会话密钥已用 DPAPI 保护）".to_string(),
    })
}

/// v1.6 关闭自动同步：删除会话密钥。
#[tauri::command]
pub async fn sync_disable_auto_sync(state: State<'_, AppState>) -> Result<AutoSyncResult, String> {
    let mut sync_state = load_sync_state(&state)?;
    delete_session_key(&state)?;
    sync_state.auto_sync_enabled = false;
    save_sync_state(&state, &sync_state)?;
    Ok(AutoSyncResult {
        enabled: false,
        message: "自动同步已关闭（会话密钥已删除）".to_string(),
    })
}

/// v1.6 远端更新探测结果。
#[derive(Debug, Clone, Serialize)]
pub struct RemoteUpdateStatus {
    /// 远端是否比本地记录的 rev 更新。
    pub has_updates: bool,
    /// 本地记录的 rev（上次同步时的远端 rev）。
    pub local_rev: Option<u64>,
    /// 远端当前 rev。
    pub remote_rev: Option<u64>,
}

/// v1.6 启动时探测远端是否有更新（轻量：只读远端 rev，不解密内容）。
///
/// 用会话密钥？—— **不需要**。探测只比较 remote_rev vs local_rev，
/// 不涉及加解密（remote_rev 是明文元数据）。所以无需会话密钥也能探测。
/// 失败（网络/PAT 错误）返回 has_updates=false，不阻塞启动。
#[tauri::command]
pub async fn sync_check_remote_updates(state: State<'_, AppState>) -> Result<RemoteUpdateStatus, String> {
    let sync_state = load_sync_state(&state)?;
    let gist_id = match sync_state.gist_id.clone() {
        Some(id) => id,
        None => return Ok(RemoteUpdateStatus { has_updates: false, local_rev: None, remote_rev: None }),
    };
    let pat = read_github_pat(&state)?;

    let remote_raw = gist_get(&pat, &gist_id).await?;
    let remote_rev = match remote_raw {
        Some((content, _)) => {
            let payload: SyncPayload = serde_json::from_str(&content)
                .map_err(|e| format!("Gist 内容非合法载荷: {e}"))?;
            Some(payload.remote_rev)
        }
        None => None, // Gist 被删
    };

    let has_updates = match (sync_state.local_rev, remote_rev) {
        (Some(local), Some(remote)) => remote > local,
        _ => false,
    };

    Ok(RemoteUpdateStatus {
        has_updates,
        local_rev: sync_state.local_rev,
        remote_rev,
    })
}

// ─── 辅助 ───

fn mask_gist_id(id: &str) -> String {
    if id.len() <= 6 {
        id.to_string()
    } else {
        format!("...{}", &id[id.len() - 6..])
    }
}

/// 粗略判定本地资产是否在上次同步后变更过：比较文件 mtime vs last_synced_at。
///
/// **已知局限**（列为 follow-up，根治需改用 content hash 比较）：
/// - 秒级粒度：同一秒内"保存→立即 pull"可能漏报（mtime 截到秒）
/// - 钟差：last_synced_at 来自 GitHub 服务器钟，本地钟慢时可能误判
/// - rsync -p / 备份还原保留旧 mtime 会漏报
///
/// **保守原则**：任何不确定（从没同步/时间解析失败/mtime 读失败）都返回 true
///（视为有变更），避免 pull 静默覆盖本地未保存的修改。
fn has_local_changes_since_last_sync(state: &AppState, sync_state: &SyncState) -> bool {
    let last_synced = match &sync_state.last_synced_at {
        Some(t) => t,
        None => return true, // 从没同步过，视为有变更
    };
    let last_synced_time = match chrono::DateTime::parse_from_rfc3339(last_synced) {
        Ok(t) => t.with_timezone(&chrono::Utc),
        Err(_) => return true, // 时间解析失败，保守视为有变更
    };
    // mtime 读失败 → 保守判有变更（u64::MAX > 任何 sync_secs）。
    // 原版 unwrap_or(0) 会永远判"无变更"导致 pull 静默覆盖，是 masking fallback。
    let file_mtime = std::fs::metadata(&state.asset_store_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(u64::MAX);
    let sync_secs = last_synced_time.timestamp().max(0) as u64;
    file_mtime > sync_secs
}
