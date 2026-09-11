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
const SESSION_KEY_ID: &str = "sync-session-key";

/// 【v2.7】自动生成的恢复密码 credential id。
///
/// 只保存**应用自己生成**的恢复密码（见 `recovery_code` 模块的安全定位）：
/// 用户手输的主密码一律不落盘 —— 那可能是他在别处复用的口令，属于 §8 凭据红线的范围。
const RECOVERY_PASSWORD_ID: &str = "sync-recovery-password";

/// 生成一个高熵恢复密码并存进本机安全存储（DPAPI），返回明文供界面填入。
///
/// 为什么返回明文：用户需要**看到**它（并在换机时能抄/能复制），否则"自动生成"等于
/// 给他一个他自己都不知道的密钥 —— 换机恢复能力就静默消失了。明文只回到本机 webview，
/// 不落日志、不进资产 JSON。
#[tauri::command]
pub async fn sync_generate_recovery_password(state: State<'_, AppState>) -> Result<String, String> {
    let password = myshelltool_core::recovery_code::generate_recovery_password(
        myshelltool_core::recovery_code::DEFAULT_LEN,
    );
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    store.save(RECOVERY_PASSWORD_ID, &password)?;
    Ok(password)
}

/// 读回已保存的自动生成恢复密码（供「换机恢复 → 查看恢复密码」）。
/// 没有保存过（用户手输自己的密码）返回 None —— 不假装有。
#[tauri::command]
pub async fn sync_reveal_recovery_password(
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    store.read(RECOVERY_PASSWORD_ID)
}

/// 若本次使用的主密码与已保存的恢复密码不同，删掉后者。
///
/// 必要性：用户先用「生成强密码」建了备份，随后又改成自己的密码（或换机恢复时填了
/// 原密码）—— 那份旧恢复密码**已经打不开当前备份**了。留着它会让「查看恢复密码」显示
/// 一个错误的值，用户抄走后在换机时得到"密码错误"，且无从归因。宁可没有，不可给错。
fn drop_stale_recovery_password(state: &AppState, used_password: &str) {
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    match store.read(RECOVERY_PASSWORD_ID) {
        Ok(Some(saved)) if saved != used_password => {
            if let Err(e) = store.delete(RECOVERY_PASSWORD_ID) {
                log::warn!("sync: 清理已失效的恢复密码失败（下次仍会显示旧值）: {e}");
            } else {
                log::info!("sync: 本次主密码与已保存的恢复密码不同，已清除后者（它会打开不当前备份）");
            }
        }
        Ok(_) => {}
        Err(e) => log::warn!("sync: 读取已保存的恢复密码失败，跳过一致性清理: {e}"),
    }
}

/// 是否已保存自动生成的恢复密码（面板据此显示「查看恢复密码」入口）。
fn recovery_password_saved(state: &AppState) -> bool {
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    matches!(store.read(RECOVERY_PASSWORD_ID), Ok(Some(_)))
}

/// 派生会话密钥并 DPAPI 加密存盘（v1.6 启用自动同步）。
///
/// 用主密码 + **新的随机 salt** 派生：适用于「本机已在用某份载荷，现在补一个免密密钥」
/// （此后推送会按这把 key + salt 重写载荷，自洽）。换机恢复/首次初始化请改用
/// `session_key_for_payload` + `store_session_key`，**按载荷的 salt** 重建，否则两条路径
/// 会得到不同的 key。
///
/// 返回 `()`：副作用（落盘）即本函数全部职责，派生出的内存 key 副本无需回传给调用方——
/// 后续解密需要 key 时统一从 SecretStore 读（见 `read_session_key`）。
fn save_session_key(state: &AppState, master_password: &str) -> Result<(), String> {
    use myshelltool_core::crypto;
    let mut salt = vec![0u8; crypto::SALT_LEN];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
    let key = crypto::derive_session_key(master_password, &salt)?;
    store_session_key(state, &key, &salt)
}

/// 会话密钥落盘（`base64(key):base64(salt)`，整体交 SecretStore 的 DPAPI 加密）。
///
/// salt 必须一并存：写载荷时要用它（见 `pack_local_vault`），缺了就写不出可用主密码
/// 恢复的备份 —— 因此空 salt 直接拒绝（fail-closed）。
fn store_session_key(state: &AppState, key: &[u8; 32], salt: &[u8]) -> Result<(), String> {
    if salt.is_empty() {
        return Err("会话密钥缺少派生 salt：拒绝写出无法用主密码恢复的备份".to_string());
    }
    let payload = format!("{}:{}", b64(key), b64(salt));
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    store.save(SESSION_KEY_ID, &payload)
}

/// 开启/恢复同步时**顺手记住本机密钥**（DPAPI）：这是「登录 → 初始化一次 → 之后推拉与
/// 自动同步全免密」的关键一步。
///
/// 按**载荷自己的 salt** 重建 key（`session_key_for_payload`），保证免密路径与主密码路径
/// 解的是同一把 key。存盘失败**不阻断 setup**（免密只是便利，主密码路径照常可用），但
/// 留 warn 且不置 `auto_sync_enabled` —— 前端会退回「输主密码」模式，用户可再点一次
/// 「本机免密 & 自动同步」重试，不会被静默糊弄。
fn remember_session_key(
    state: &AppState,
    master_password: &str,
    payload: &SyncPayload,
    sync_state: &mut SyncState,
) {
    match myshelltool_core::sync::session_key_for_payload(payload, master_password) {
        Ok((key, salt)) => match store_session_key(state, &key, &salt) {
            Ok(()) => {
                sync_state.auto_sync_enabled = true;
                log::info!("sync_setup: 已记住本机会话密钥（DPAPI）——后续推拉/自动同步免密");
            }
            Err(e) => log::warn!("sync_setup: 会话密钥存盘失败，免密未启用（可稍后手动启用）: {e}"),
        },
        Err(e) => log::warn!("sync_setup: 建立会话密钥失败，免密未启用: {e}"),
    }
}

/// 从 SecretStore 读会话密钥 + 其派生 salt（DPAPI 解密）。未配置返回 None（调用方回退手动模式）。
///
/// **两者必须成对取用**：salt 要随每次加密的载荷写进 `blob.salt`，否则换机/重装后
/// 主密码无法重建同一把 key（v2.7 修复的可恢复性缺陷，见 `crypto::encrypt_with_key`）。
fn read_session_key_material(state: &AppState) -> Result<Option<([u8; 32], Vec<u8>)>, String> {
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    let payload = match store.read(SESSION_KEY_ID)? {
        Some(s) => s,
        None => return Ok(None), // 未启用自动同步
    };
    let (key_b64, salt_b64) = payload
        .split_once(':')
        .ok_or_else(|| "会话密钥载荷格式损坏".to_string())?;
    let key_bytes = b64_decode(key_b64)?;
    let key: [u8; 32] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "会话密钥长度异常".to_string())?;
    let salt = b64_decode(salt_b64)?;
    // fail-closed：salt 缺失/为空时**拒绝**继续（否则会写出「只有本机能解」的载荷，
    // 正是本次修复要根治的形态）。修法是关闭再重新启用自动同步以重新派生。
    if salt.is_empty() {
        return Err(
            "本机会话密钥缺少派生 salt（无法写出可用主密码恢复的备份）：请关闭并重新启用自动同步".to_string(),
        );
    }
    Ok(Some((key, salt)))
}

/// 只要会话密钥的便捷包装（解密/探测用，不需要 salt）。
fn read_session_key(state: &AppState) -> Result<Option<[u8; 32]>, String> {
    Ok(read_session_key_material(state)?.map(|(key, _salt)| key))
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
        let (content, _) = remote;
        let payload: SyncPayload = serde_json::from_str(&content)
            .map_err(|e| format!("Gist 内容非合法同步载荷: {e}"))?;
        // 统一走 decrypt_vault（给了密码 → 密码路径）。只有「salt 随载荷」的新格式能解开，
        // 旧格式由 core 明确拒绝并提示重建。
        let vault = decrypt_vault(&state, &master_password, &payload)?;
        let assets_json = serde_json::to_string_pretty(&vault.assets_store)
            .map_err(|e| format!("序列化资产失败: {e}"))?;

        // 换机拉取：自动将远端解密出的密码与私钥通过本机 DPAPI 写入本地 SecretStore。
        // 恢复失败不阻断导入（资产 JSON 已就绪、必须返回前端），但**逐个失败都要有痕**：
        // 曾只打一条 warn，用户看到「已拉取」却连不上（认证失败），归因成本极高。
        if !vault.credentials.is_empty() {
            match crate::sync_credentials::restore_sync_credentials(&state, &vault.credentials) {
                Ok((restored, failed)) => {
                    if !failed.is_empty() {
                        log::warn!(
                            "sync_setup: {} 项凭据恢复失败（成功 {restored} 项）：{}",
                            failed.len(),
                            failed
                                .iter()
                                .map(|(id, r)| format!("{id}: {r}"))
                                .collect::<Vec<_>>()
                                .join("；")
                        );
                    }
                }
                Err(e) => log::warn!("sync_setup: 恢复同步凭据到本机 SecretStore 失败: {e}"),
            }
        }
        // 资产文件写回失败必须中止（对照 sync_pull 的写法）：否则本地仍是旧内容，
        // 而下方 last_synced_at 照常前移，此后 pull 会以「安全拉取」误判并覆盖。
        // 原子写：资产 JSON 是本地唯一副本，半截即整个列表不可读。
        myshelltool_core::write_atomic(&state.asset_store_path, &assets_json).map_err(|e| {
            format!(
                "写回 connection-assets.json（{}）失败: {e}",
                state.asset_store_path.display()
            )
        })?;

        // 记录 sync state。last_synced_at 记本地完成时刻（与刚落盘的资产文件
        // mtime 同钟域）；记 Gist 的 updated_at（远端历史时刻）会让此后每次
        // has_local_changes_since_last_sync 都误判「本地有变更」。
        sync_state.gist_id = Some(existing_gist_id.clone());
        sync_state.local_rev = Some(payload.remote_rev);
        sync_state.last_synced_at = Some(chrono::Utc::now().to_rfc3339());
        // 按**这份载荷的 salt** 记住本机密钥：以后推拉/自动同步免密
        remember_session_key(&state, &master_password, &payload, &mut sync_state);
        save_sync_state(&state, &sync_state)?;
        // 换机恢复用的是原机主密码：若本机还存着一份**不同**的自动生成密码，它已打不开
        // 当前备份 —— 清掉，避免「查看恢复密码」给出错值
        drop_stale_recovery_password(&state, &master_password);

        Ok(SyncSetupResult::PulledRemote { assets_json })
    } else {
        // 首次推送：全量打包当前本地资产与凭据 → 创建 Gist
        let payload = pack_local_vault(&state, &sync_state, &master_password, 1)?;
        let payload_json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
        let (new_gist_id, _) = gist_create(&pat, &payload_json).await?;

        sync_state.gist_id = Some(new_gist_id.clone());
        sync_state.local_rev = Some(1);
        // 与 push 同理：记本地完成时刻，与本地文件 mtime 保持同钟域
        sync_state.last_synced_at = Some(chrono::Utc::now().to_rfc3339());
        // 按刚推上去的载荷的 salt 记住本机密钥（同一把 key 两条路都能解）
        remember_session_key(&state, &master_password, &payload, &mut sync_state);
        save_sync_state(&state, &sync_state)?;
        // 用户可能先点了「生成强密码」又改用自己的密码：清掉那份已失效的
        drop_stale_recovery_password(&state, &master_password);

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

/// 辅助函数：根据当前本地资产及关联凭据打包加密载荷（优先会话密钥路径，否则走主密码）。
fn pack_local_vault(
    state: &AppState,
    sync_state: &SyncState,
    master_password: &str,
    new_rev: u64,
) -> Result<SyncPayload, String> {
    let local_json = read_local_assets(state)?;
    // 文件可读但 JSON 损坏 → 直接中止（push / setup 首推 / resolve_conflict(local)
    // 都经此打包）。旧版静默折叠成空 vault 再加密推 Gist，会无声清空远端备份
    // 且 last_synced_at 照常前移（用户看到「已推送」）。read_local_assets 已单独
    // 放行 NotFound（首次使用合法为空），这里只拦「可读但损坏」。
    let store: myshelltool_core::ConnectionAssetStore = serde_json::from_str(&local_json)
        .map_err(|e| {
            format!(
                "本地资产文件损坏，已中止同步以免覆盖远端备份（{}）：{e}。\
                 请检查该文件内容，或改用 sync_pull 从 Gist 恢复。",
                state.asset_store_path.display()
            )
        })?;

    let credentials = if sync_state.sync_credentials {
        // 三态处置（v2.6）：读到 → 上传；**读失败 → 中止本次推送**；本地缺失 → 提示但继续。
        // 曾用 `unwrap_or_default()` + 内部 warn-and-skip：读不出的凭据被静默丢弃，
        // 却仍把「不完整的 vault」推上 Gist 覆盖完整备份并报「已推送」——换机恢复后
        // 该资产没密码且无从归因。宁可本次不同步，也不能用残缺备份覆盖好备份。
        let outcome = crate::sync_credentials::collect_sync_credentials(state, &store.assets)?;
        if outcome.has_failures() {
            return Err(format!(
                "同步已中止：{} 项凭据读取失败（{}）。这些凭据不会进入备份，若继续推送会用不完整的备份覆盖 Gist 上的完整备份。\
                 请先修复（例如确认以同一 Windows 账户运行、凭据文件未被占用）后重试。",
                outcome.failed.len(),
                outcome.failure_summary()
            ));
        }
        if !outcome.missing.is_empty() {
            log::warn!(
                "sync_push: 资产引用了本地不存在的凭据 {} 项（可能已被清理），本次备份不含这些项",
                outcome.missing.len()
            );
        }
        outcome.items
    } else {
        vec![]
    };

    let vault = myshelltool_core::sync::SyncVaultData::new(store, credentials);

    if master_password.trim().is_empty() {
        // 免密路径（自动同步）：**连 salt 一起取**，写进载荷 —— 这样换机后主密码
        // 仍能重建同一把 key 解开这份备份（v2.7 可恢复性修复）。
        let (key, salt) = read_session_key_material(state)?
            .ok_or_else(|| "未启用自动同步，需提供主密码".to_string())?;
        myshelltool_core::sync::pack_vault_with_key(&vault, &key, &salt, new_rev)
    } else {
        myshelltool_core::sync::pack_vault(&vault, master_password, new_rev)
    }
}

/// 统一解密 helper：按调用方是否给了主密码选路径，解析为 SyncVaultData。
///
/// - 没给密码 → 免密路径（自动同步，必须本机有会话密钥）；
/// - 给了密码 → 密码路径。会话密钥加密的载荷也随带派生 salt，因此主密码能重建同一把
///   key —— 换机恢复即依赖此点；
/// - 旧格式载荷（`blob.salt` 缺省）由 core 统一明确拒绝（v2.7：不做旧版适配），
///   这里不再有任何回退分支。
fn decrypt_vault(
    state: &AppState,
    master_password: &str,
    payload: &SyncPayload,
) -> Result<myshelltool_core::sync::SyncVaultData, String> {
    if master_password.trim().is_empty() {
        let key = read_session_key(state)?
            .ok_or_else(|| "未启用自动同步，需提供主密码".to_string())?;
        return myshelltool_core::sync::unpack_vault_with_key(payload, &key);
    }
    myshelltool_core::sync::unpack_vault(payload, master_password)
}

/// 推送本地资产及关联凭据到 Gist（端到端加密）。
///
/// `master_password` 为空 → 会话密钥路径（需已启用自动同步）；非空 → 主密码路径。
/// 两条路径产出的都是同一种载荷格式（都带派生 salt），因此备份始终可用主密码恢复。
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

    // v2.6：推送前先读远端 rev，与本地记录的 local_rev 对齐（乐观并发检查）。
    //
    // 事故形态：两台机器共用同一 Gist，A 推 rev5 后 B 推自己的 rev5'，`gist_update`
    // 是无条件 PATCH → **A 的改动在远端被静默抹掉**，而双方都显示「已推送」；此后
    // B 再 pull 时 decide() 见 rev 相等 → NoChange「已是最新」，两边都以为同步健康。
    // 这里不引入强制冲突弹窗（推送语义就是「让远端等于本地」），但**不允许在不知情
    // 的情况下覆盖别人的提交**：远端 rev 与本地认知不一致即中止并给出可操作提示。
    match gist_get(&pat, &gist_id).await? {
        Some((content, _updated_at)) => {
            let local_rev = sync_state.local_rev.unwrap_or(0);
            // 远端 rev 在载荷里（明文元数据，无需解密）。解析失败说明远端不是本应用的
            // 载荷格式 → 同样不能盲推覆盖，按「状态异常」中止。
            let remote_rev = match serde_json::from_str::<myshelltool_core::sync::SyncPayload>(&content) {
                Ok(payload) => payload.remote_rev,
                Err(e) => {
                    return Err(format!(
                        "推送已中止：远端备份内容不是可识别的同步载荷（解析失败: {e}）。\
                         继续推送会覆盖远端文件，请先确认该 Gist 是否被其他工具/版本改写。"
                    ));
                }
            };
            if remote_rev != local_rev {
                return Err(format!(
                    "推送已中止：远端备份已被其他设备/会话更新（远端 rev {remote_rev}，本地记录 rev {local_rev}）。\
                     继续推送会用本地内容覆盖数据、丢掉远端那次保存。请先执行「拉取」确认远端改动后再推送。"
                ));
            }
        }
        None => {
            // Gist 被删（手动清理/权限变化）：不是「远端为空可以随便写」，而是状态异常。
            return Err(
                "推送已中止：Gist 不存在（可能已被手动删除或失去访问权限）。请重新配置同步（sync_setup）后再推送。"
                    .to_string(),
            );
        }
    }

    let new_rev = sync_state.local_rev.unwrap_or(0) + 1;
    let payload = pack_local_vault(&state, &sync_state, &master_password, new_rev)?;
    let payload_json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;

    gist_update(&pat, &gist_id, &payload_json).await?;

    sync_state.local_rev = Some(new_rev);
    // push 后本地文件 mtime ≤ 此刻；last_synced_at 记本地完成时刻与 mtime 同钟域，
    // 才不会被 Gist 服务器钟与本机钟的偏差误报成「本地有变更」
    sync_state.last_synced_at = Some(chrono::Utc::now().to_rfc3339());
    // 【v2.7】用主密码推成功、且本机还没有免密密钥时，顺手记住这把 key：
    // 「输一次主密码」就足够把免密打开（已免密的老用户不受影响，不做 key churn）。
    if !master_password.trim().is_empty() && !sync_state.auto_sync_enabled {
        remember_session_key(&state, &master_password, &payload, &mut sync_state);
    }
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
    /// `credentials_failed`（v2.6）：本次有 N 项凭据未能写入本机 SecretStore——
    /// 资产已拉取成功，但这些资产的密码/私钥仍不可用，前端必须提示（曾静默只 warn，
    /// 用户看到「已拉取」却连不上，归因困难）。
    Pulled {
        assets_json: String,
        new_rev: u64,
        #[serde(default)]
        credentials_failed: usize,
    },
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
    let (content, _) = remote_raw;
    let remote_payload: SyncPayload = serde_json::from_str(&content)
        .map_err(|e| format!("Gist 内容非合法同步载荷: {e}"))?;

    // 2. 本地是否有变更：比较本地资产 mtime vs 上次同步时间
    let local_has_changes = has_local_changes_since_last_sync(&state, &sync_state);

    // 3. 冲突判定
    let decision = sync::decide(local_has_changes, Some(&remote_payload), &sync_state);

    match decision {
        SyncDecision::NoChange => Ok(SyncPullResult::NoChange),
        SyncDecision::PullRemote => {
            // 安全拉取：解密远端 vault，覆盖本地资产并恢复凭据到 SecretStore
            let vault = decrypt_vault(&state, &master_password, &remote_payload)?;
            let remote_json = serde_json::to_string_pretty(&vault.assets_store)
                .map_err(|e| format!("序列化资产失败: {e}"))?;
            myshelltool_core::write_atomic(&state.asset_store_path, &remote_json)
                .map_err(|e| format!("写回 connection-assets.json: {e}"))?;

            // 自动将远端解密的密码与托管私钥存入本地 SecretStore（DPAPI 重新加密）。
            // 资产已落盘 → 不阻断 pull；但失败项数要回传前端（toast 里显式提示），
            // 否则用户看到「已拉取」却连不上（凭据没恢复），归因困难。
            let mut credentials_failed = 0usize;
            if !vault.credentials.is_empty() {
                match crate::sync_credentials::restore_sync_credentials(&state, &vault.credentials) {
                    Ok((restored, failed)) => {
                        credentials_failed = failed.len();
                        if credentials_failed > 0 {
                            log::warn!(
                                "sync_pull: {} 项凭据恢复失败（成功 {restored} 项）：{}",
                                credentials_failed,
                                failed
                                    .iter()
                                    .map(|(id, r)| format!("{id}: {r}"))
                                    .collect::<Vec<_>>()
                                    .join("；")
                            );
                        }
                    }
                    Err(e) => {
                        credentials_failed = vault.credentials.len();
                        log::warn!("sync_pull: 恢复同步凭据到本机 SecretStore 失败: {e}");
                    }
                }
            }

            let mut new_state = sync_state;
            new_state.local_rev = Some(remote_payload.remote_rev);
            // last_synced_at 记本地完成时刻（与刚落盘的资产文件 mtime 同钟域）：
            // 记 Gist 的 updated_at（远端历史时刻）会导致此后每次
            // has_local_changes_since_last_sync 都误判「本地有变更」
            new_state.last_synced_at = Some(chrono::Utc::now().to_rfc3339());
            // 【v2.7】用主密码拉成功、且本机还没有免密密钥时顺手记住（同 push）：
            // 按**远端载荷的 salt** 重建，免密路径与主密码路径才是同一把 key。
            if !master_password.trim().is_empty() && !new_state.auto_sync_enabled {
                remember_session_key(&state, &master_password, &remote_payload, &mut new_state);
            }
            save_sync_state(&state, &new_state)?;

            Ok(SyncPullResult::Pulled {
                assets_json: remote_json,
                new_rev: remote_payload.remote_rev,
                credentials_failed,
            })
        }
        SyncDecision::PushLocal => Ok(SyncPullResult::LocalNewer),
        SyncDecision::Conflict => {
            // 冲突：返回双方 JSON，前端弹窗让用户选
            let local_json = read_local_assets(&state)?;
            let vault = decrypt_vault(&state, &master_password, &remote_payload)?;
            let remote_json = serde_json::to_string_pretty(&vault.assets_store)
                .map_err(|e| format!("序列化远端资产失败: {e}"))?;
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
/// - local：加密本地数据与凭据推送（覆盖远端）
/// - remote：用 remote_json 覆盖本地，并拉取凭据恢复
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
            // 用本地覆盖远端：打包全量本地资产及凭据推送
            let new_rev = remote_rev + 1;
            let payload = pack_local_vault(&state, &sync_state, &master_password, new_rev)?;
            let payload_json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
            gist_update(&pat, &gist_id, &payload_json).await?;

            let mut new_state = sync_state;
            new_state.local_rev = Some(new_rev);
            // 与 sync_push 同理：记本地完成时刻，与 mtime 同钟域
            new_state.last_synced_at = Some(chrono::Utc::now().to_rfc3339());
            save_sync_state(&state, &new_state)?;
        }
        "remote" => {
            // 用远端覆盖本地：写入资产文件，并尝试解密远端凭据恢复
            myshelltool_core::write_atomic(&state.asset_store_path, &remote_json)
                .map_err(|e| format!("写回 connection-assets.json: {e}"))?;

            // 尽力恢复远端凭据（remote_json 只含资产，凭据在 Gist 载荷里）。
            // 任何一步失败都不阻断「用远端覆盖本地」的结果，但逐层留日志——
            // 用户选了 remote 通常预期凭据也一并恢复，静默跳过会掩盖这一点。
            match gist_get(&pat, &gist_id).await {
                Ok(Some((content, _))) => {
                    match serde_json::from_str::<SyncPayload>(&content) {
                        Ok(payload) => match decrypt_vault(&state, &master_password, &payload) {
                            Ok(vault) => {
                                if !vault.credentials.is_empty() {
                                    match crate::sync_credentials::restore_sync_credentials(
                                        &state,
                                        &vault.credentials,
                                    ) {
                                        Ok((_, failed)) if !failed.is_empty() => log::warn!(
                                            "sync_resolve_conflict: {} 项凭据恢复失败：{}",
                                            failed.len(),
                                            failed
                                                .iter()
                                                .map(|(id, r)| format!("{id}: {r}"))
                                                .collect::<Vec<_>>()
                                                .join("；")
                                        ),
                                        Ok(_) => {}
                                        Err(e) => log::warn!(
                                            "sync_resolve_conflict: 恢复同步凭据失败: {e}"
                                        ),
                                    }
                                }
                            }
                            Err(e) => log::warn!(
                                "sync_resolve_conflict: 解密远端载荷以恢复凭据失败: {e}"
                            ),
                        },
                        Err(e) => log::warn!(
                            "sync_resolve_conflict: 解析 Gist 载荷以恢复凭据失败: {e}"
                        ),
                    }
                }
                Ok(None) => {
                    log::warn!("sync_resolve_conflict: Gist 不存在，跳过凭据恢复");
                }
                Err(e) => {
                    log::warn!("sync_resolve_conflict: 重新拉取 Gist 以恢复凭据失败: {e}");
                }
            }

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
    if old_password.trim().is_empty() {
        return Err("旧主密码不能为空".to_string());
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
    // 验证旧密码 + 取出**完整保管库（含凭据）**。
    //
    // v2.7 修两处缺陷：
    // ① 原实现用已删除的 `sync::unpack()` 取明文再 `sync::pack()` 回写 —— 那个接口在明文是
    //    SyncVaultData 时**只返回 assets_store、丢弃 credentials**，于是「重置主密码」
    //    会把 Gist 备份里的密码/私钥整段抹掉，且 UI 只提示「✓ 主密码已重置」（静默丢数据）。
    //    现在走保管库级 `unpack_vault` / `pack_vault`，原样搬运凭据（旧接口已删，从类型上杜绝）。
    // ② 旧格式载荷曾恒报「旧主密码错误」（正确密码也被判错）→ 现在 core 统一明确拒绝该格式，
    //    文案直接说明「旧格式不再支持、请重推一次」，不再伪装成密码错误。
    let vault = decrypt_vault(&state, &old_password, &payload)?;

    // 用新密码重新加密同一份保管库并推送
    let new_rev = sync_state.local_rev.unwrap_or(0) + 1;
    let new_payload = myshelltool_core::sync::pack_vault(&vault, &new_password, new_rev)?;
    let payload_json = serde_json::to_string(&new_payload).map_err(|e| e.to_string())?;
    gist_update(&pat, &gist_id, &payload_json).await?;

    let mut new_state = sync_state;
    new_state.local_rev = Some(new_rev);
    // 与 sync_push 同理：记本地完成时刻，与 mtime 同钟域
    new_state.last_synced_at = Some(chrono::Utc::now().to_rfc3339());
    // v1.6：若已启用自动同步，用新密码重新派生会话密钥（保持一致性，旧密钥失效）
    if new_state.auto_sync_enabled {
        save_session_key(&state, &new_password)?;
    }
    // 换了主密码 → 旧主密码（含可能保存过的自动生成密码）已打不开备份，清掉免得给错值
    drop_stale_recovery_password(&state, &new_password);
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
    // v1.6：连同会话密钥一起清。
    // 失败必须可见：曾用 `let _ =` 吞掉删除错误，UI 显示「已清空同步」而等价于
    // 主密码的会话密钥仍留在 credentials/ 里——用户以为这个「忘了主密码的逃生口」
    // 已生效（形态 C 静默兜底）。delete_session_key 内部已把「本来就不存在」当成功。
    if let Err(e) = delete_session_key(&state) {
        log::warn!("sync_clear: 会话密钥删除失败: {e}");
        return Err(format!(
            "sync-state 已清除，但会话密钥删除失败：{e}。请手动删除 credentials/sync-session-key.cred 后重试"
        ));
    }
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

    // 验证主密码正确性：拉远端用主密码解密。
    //
    // v2.7：唯一载荷格式都带派生 salt，主密码**一律**能验（不再有「旧格式跳过验证」的分支）；
    // 旧格式载荷由 core 明确拒绝，用户会看到「旧格式不再支持，请重推一次」而不是被静默放行。
    let pat = read_github_pat(&state)?;
    if let Some((content, _)) = gist_get(&pat, &gist_id).await? {
        let payload: SyncPayload = serde_json::from_str(&content)
            .map_err(|e| format!("Gist 内容非合法载荷: {e}"))?;
        myshelltool_core::sync::unpack_vault(&payload, &master_password)
            .map_err(|e| format!("主密码验证失败：{e}"))?;
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
