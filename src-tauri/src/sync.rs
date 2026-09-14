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
        // v2.6：**存在但为空 = 损坏**，不再当「首次运行」静默清零。旧实现把 0 字节
        // 文件折叠成默认值，于是 gist_id / last_synced_at / auto_sync_enabled 全部
        // 无声丢失（下次保存还把这份空状态写回，等于确认丢失）。与 read_local_assets
        // 的三态判据保持一致：NotFound 才是首次运行，其余按损坏显式报错。
        return Err(format!(
            "sync-state.json 存在但为空（{}）——疑为上次写入被截断。已停止同步操作以免把空状态写回；\
             请删除该文件重新配置同步（或从 {}.tmp-* 残留中恢复）",
            path.display(),
            path.display()
        ));
    }
    serde_json::from_str(&json).map_err(|e| format!("解析 sync-state.json: {e}"))
}

fn save_sync_state(state: &AppState, sync_state: &SyncState) -> Result<(), String> {
    let path = sync_state_path(state)?;
    let json = serde_json::to_string_pretty(sync_state).map_err(|e| e.to_string())?;
    // 原子写：sync-state 截断会被下次读取判为损坏并中止同步（见 load_sync_state）
    myshelltool_core::write_atomic(path, json).map_err(|e| format!("写 sync-state.json: {e}"))
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
    /// 是否启用凭据与私钥同步。
    pub sync_credentials: bool,
    /// 【v2.7】本机资产自上次同步后是否有改动（保守判定，见 has_local_changes_since_last_sync）。
    ///
    /// 面板据此把「推送到云端」提为主操作并显示「有改动待推送」——此前这个信号只服务于
    /// pull 的冲突判定，前端拿不到，用户面对两个等重按钮不知道该按哪个。
    pub local_has_changes: bool,
    /// 【v2.7】本机是否保存了自动生成的恢复密码（供「查看恢复密码」入口显隐）。
    pub recovery_password_saved: bool,
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
        // 按字符切片（同 mask_gist_id）：字节切片遇多字节 id 会 panic
        if id.chars().count() <= 6 {
            id.clone()
        } else {
            let tail: String = id.chars().skip(id.chars().count() - 6).collect();
            format!("...{tail}")
        }
    });
    Ok(SyncStatusResult {
        configured: sync_state.gist_id.is_some(),
        last_synced_at: sync_state.last_synced_at.clone(),
        gist_id_masked,
        pat_configured,
        auto_sync_enabled: sync_state.auto_sync_enabled,
        sync_credentials: sync_state.sync_credentials,
        // 未配置同步时谈不上"待推送"（面板此时不显示动作区），给 false 更少歧义
        local_has_changes: sync_state.gist_id.is_some()
            && has_local_changes_since_last_sync(&state, &sync_state),
        recovery_password_saved: recovery_password_saved(&state),
    })
}

/// 切换是否同步凭据与托管私钥。
#[tauri::command]
pub async fn sync_set_credentials_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<bool, String> {
    let mut sync_state = load_sync_state(&state)?;
    sync_state.sync_credentials = enabled;
    save_sync_state(&state, &sync_state)?;
    Ok(enabled)
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
///
/// **成功后顺手记住本机密钥**（DPAPI，`remember_session_key`）：主密码只在这一次（或换机时）
/// 输入，此后 push/pull 与自动同步都免密 —— 「登录一次就能用」的关键。

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
/// - 钟差：last_synced_at 已统一记本地完成时刻（与 mtime 同钟域），不存在
///   跨钟比较；但系统时钟被 NTP 回拨时仍可能短暂误判
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

// ─── 按域拆出的子模块（v2.8 第四轮；命令名不变，generate_handler 零改动）───
mod gist;
mod ops;
mod secrets;

// glob 再导出：tauri generate_handler 的隐藏宏（__cmd__X）随 glob 可见
pub use gist::*;
pub use ops::*;
pub use secrets::*;
