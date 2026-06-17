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
    let payload = sync::pack(&local_json, &master_password, new_rev)?;
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
            let remote_json = sync::unpack(&remote_payload, &master_password)?;
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
            let remote_json = sync::unpack(&remote_payload, &master_password)?;
            Ok(SyncPullResult::Conflict {
                local_json,
                remote_json,
                remote_rev: remote_payload.remote_rev,
            })
        }
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
            // 用本地覆盖远端：推送本地
            let local_json = read_local_assets(&state)?;
            let new_rev = remote_rev + 1;
            let payload = sync::pack(&local_json, &master_password, new_rev)?;
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
    save_sync_state(&state, &new_state)?;
    Ok(())
}

/// 清空同步配置（忘了主密码的逃生口）。
///
/// 删除本地 sync-state.json。Gist 上的数据保留（用户可手动去 GitHub 删）。
#[tauri::command]
pub async fn sync_clear(state: State<'_, AppState>) -> Result<(), String> {
    let path = sync_state_path(&state)?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| format!("删除 sync-state.json: {e}"))?;
    }
    Ok(())
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
