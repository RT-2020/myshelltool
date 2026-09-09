//! GitHub Device Flow 登录（OAuth Device Authorization Grant，替代手动粘贴 PAT）。
//!
//! 协议（https://docs.github.com/oauth-apps/oauth-device-flow）两步：
//! 1. sync_oauth_start：POST github.com/login/device/code 拿 device_code/user_code，
//!    存入 AppState 内存槽（单槽，新 start 覆盖旧 start）。
//! 2. sync_oauth_poll：前端按 interval 轮询 github.com/login/oauth/access_token，
//!    authorization_pending / slow_down 继续等，成功时 token 直接写 SecretStore
//!    （id = "github-pat"，与手动 PAT 同槽，sync.rs 的 Bearer 逻辑零改动）。
//!
//! ⚠️ OAuth 端点与 api.github.com 的 JSON API 不同：**必须** Accept: application/json
//! + `.form()`（application/x-www-form-urlencoded）编码，不解析 JSON body——不要
//! 照抄 sync.rs 的 `.json()` 用法。
//!
//! 安全：token 明文不出后端进程（不返回前端）；无需 client_secret。
//! IPC 契约预留 provider 参数（当前仅 github，为将来 Gitee 等铺路）。

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tauri::State;

use crate::AppState;

/// OAuth App client_id。默认取构建期环境变量 MYST_GITHUB_CLIENT_ID（option_env!），
/// 未注入则为空串 → sync_oauth_start 返回带注册指引的 Err（空置不算编译失败）。
const GITHUB_CLIENT_ID: &str = match option_env!("MYST_GITHUB_CLIENT_ID") {
    Some(id) => id,
    None => "",
};

const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const ACCESS_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
/// Device Flow 仅申请 gist 读写（与手动 PAT 的 scope 一致）。
const OAUTH_SCOPE: &str = "gist";
/// client_id 未注册时的指引文案（UI 直接展示 errorMessage）。
const REGISTER_HINT: &str = "GitHub 登录尚未启用：开发者需先注册 OAuth App——\
     打开 github.com/settings/developers → New OAuth App（回调 URL 可留空）→ \
     编辑 App 勾选「Enable Device Flow」→ 将 client_id 填入代码常量 GITHUB_CLIENT_ID \
     或构建环境变量 MYST_GITHUB_CLIENT_ID";

// ─── 设备码会话（内存槽，重启即失）───

pub struct OAuthSession {
    pub provider: String,
    pub device_code: String,
    pub user_code: String,
    /// 轮询间隔（秒）。slow_down 响应可能带回新值（见 poll）。
    pub interval: u64,
    /// 本地判过期：超过此时刻前端再 poll 直接返回 Expired（GitHub 端同样 900s 失效）。
    pub expires_at: Instant,
}

// ─── GitHub OAuth 响应 DTO ───

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    error: Option<String>,
    /// slow_down 时 GitHub 在响应里下发新间隔；优先用响应值。
    interval: Option<u64>,
}

// ─── 命令返回类型 ───

/// sync_oauth_start 返回（前端据此展示 user_code 并启动轮询/倒计时）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthStartResult {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// sync_oauth_poll 返回（tag enum，风格参照 sync.rs 的 SyncPullResult）。
///
/// Pending/SlowDown 带 user_code：前端比对发起时记下的 userCode，不一致说明
/// 后端 session 已被另一处发起的登录覆盖，旧实例应静默停止轮询。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum OAuthPollResult {
    /// 用户尚未在浏览器完成授权，继续按 interval 轮询。
    Pending { user_code: String },
    /// 轮询过快：GitHub 要求放慢。interval 为响应下发的新间隔（可能缺省）。
    SlowDown { user_code: String, interval: Option<u64> },
    /// 用户在授权页拒绝。
    Denied,
    /// 设备码过期（本地判定或 GitHub 端 expired_token）。
    Expired,
    /// 授权成功，token 已写入本地安全存储（github-pat）。
    Success,
}

// ─── 辅助 ───

fn lock_err() -> String {
    "sync_oauth_pending 锁中毒".to_string()
}

/// 清空内存槽中的设备码会话（容错：无 session 也 Ok）。
fn clear_session(state: &AppState) {
    if let Ok(mut guard) = state.sync_oauth_pending.lock() {
        *guard = None;
    }
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {e}"))
}

// ─── Tauri 命令 ───

/// 发起 Device Flow：向 GitHub 请求设备码，存入内存槽（覆盖旧 session）。
#[tauri::command]
pub async fn sync_oauth_start(
    state: State<'_, AppState>,
    provider: String,
) -> Result<OAuthStartResult, String> {
    // IPC 契约预留多 provider；当前仅实现 github。
    if provider != "github" {
        return Err("暂不支持该同步服务（预留）".to_string());
    }
    if GITHUB_CLIENT_ID.is_empty() {
        return Err(REGISTER_HINT.to_string());
    }

    let client = http_client()?;
    let resp = client
        .post(DEVICE_CODE_URL)
        .header("Accept", "application/json")
        .header("User-Agent", "myshelltool")
        .form(&[("client_id", GITHUB_CLIENT_ID), ("scope", OAUTH_SCOPE)])
        .send()
        .await
        .map_err(|e| format!("请求 GitHub 设备码失败: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("获取设备码失败 (HTTP {status}): {text}"));
    }
    let body: DeviceCodeResponse = resp
        .json()
        .await
        .map_err(|e| format!("解析设备码响应失败: {e}"))?;

    let interval = body.interval.unwrap_or(5);
    *state.sync_oauth_pending.lock().map_err(|_| lock_err())? = Some(OAuthSession {
        provider,
        device_code: body.device_code,
        user_code: body.user_code.clone(),
        interval,
        expires_at: Instant::now() + Duration::from_secs(body.expires_in),
    });

    Ok(OAuthStartResult {
        user_code: body.user_code,
        verification_uri: body.verification_uri,
        expires_in: body.expires_in,
        interval,
    })
}

/// 单次轮询授权状态（前端按 interval 链式调用）。
///
/// 成功（拿到 access_token）→ 直接写 SecretStore（github-pat）→ 清 session。
#[tauri::command]
pub async fn sync_oauth_poll(state: State<'_, AppState>) -> Result<OAuthPollResult, String> {
    // 短暂 lock 取出轮询所需数据，绝不持锁跨 await（后续网络请求耗时最长 10s）。
    let (device_code, user_code, expired) = {
        let guard = state.sync_oauth_pending.lock().map_err(|_| lock_err())?;
        match guard.as_ref() {
            None => return Err("无进行中的登录流程".to_string()),
            Some(s) => (
                s.device_code.clone(),
                s.user_code.clone(),
                Instant::now() >= s.expires_at,
            ),
        }
    };
    if expired {
        clear_session(&state);
        return Ok(OAuthPollResult::Expired);
    }

    let client = http_client()?;
    let resp = client
        .post(ACCESS_TOKEN_URL)
        .header("Accept", "application/json")
        .header("User-Agent", "myshelltool")
        .form(&[
            ("client_id", GITHUB_CLIENT_ID),
            ("device_code", device_code.as_str()),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()
        .await
        .map_err(|e| format!("轮询 GitHub 授权状态失败: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("轮询授权状态失败 (HTTP {status}): {text}"));
    }
    let body: TokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("解析授权状态响应失败: {e}"))?;

    if let Some(token) = body.access_token {
        let store = myshelltool_core::SecretStore::new(
            &state.secret_store_dir,
            Box::new(crate::dpapi_codec::DpapiCodec),
        );
        store
            .save("github-pat", &token)
            .map_err(|e| format!("token 写入本地安全存储失败: {e}"))?;
        clear_session(&state);
        return Ok(OAuthPollResult::Success); // token 明文不出后端进程
    }

    match body.error.as_deref() {
        Some("authorization_pending") => Ok(OAuthPollResult::Pending { user_code }),
        Some("slow_down") => Ok(OAuthPollResult::SlowDown {
            user_code,
            interval: body.interval,
        }),
        Some("access_denied") => {
            clear_session(&state);
            Ok(OAuthPollResult::Denied)
        }
        Some("expired_token") => {
            clear_session(&state);
            Ok(OAuthPollResult::Expired)
        }
        Some(other) => Err(format!("GitHub 登录失败: {other}")),
        None => Err("GitHub 登录响应异常（无 access_token 也无 error）".to_string()),
    }
}

/// 取消进行中的登录流程（清内存槽；无 session 也 Ok，容错）。
#[tauri::command]
pub async fn sync_oauth_cancel(state: State<'_, AppState>) -> Result<(), String> {
    clear_session(&state);
    Ok(())
}
