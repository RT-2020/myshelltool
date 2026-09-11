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
//! + `.form()`（application/x-www-form-urlencoded）编码——不要照抄 sync.rs 的 `.json()` 用法。
//!
//! 安全：token 明文不出后端进程（不返回前端）；无需 client_secret。
//! IPC 契约预留 provider 参数（当前仅 github，为将来 Gitee 等铺路）。
//!
//! 【v2.7 稳定性加固】此前轮询链路把「网络抖动」与「授权被拒」都折成 `Err`，
//! 一次瞬时抖动（DNS/TCP/TLS/代理链路）就作废整个登录，用户只看到一句
//! 「登录失败：轮询 GitHub 授权状态失败: error sending request for url (...)」。
//! 三处根因与对策：
//! 1. **每次 poll 新建 reqwest::Client** → 900s 内最多 180 次轮询 = 180 次全新
//!    DNS+TCP+TLS 握手，每次都是独立的失败机会 → 改为单例客户端，轮询大多复用
//!    已建立的 keep-alive 连接；
//! 2. **单个 10s 整体超时、无 connect 超时** → 跨网络/经代理时一次握手就可能吃光
//!    预算 → 改为 connect 10s + 整体 20s 分离超时；
//! 3. **传输故障与协议拒绝同路** → 判定交给 `myshelltool_core::oauth_flow`：传输故障/
//!    429/5xx/非 JSON 响应返回 `Unstable`（前端退避重试，设备码仍有效，用户不必重新
//!    走浏览器授权），只有 GitHub 明确报错才 `Failed`。
//! 另外错误文案带 reqwest 的 cause 链——`error sending request for url (...)` 只是
//! 最外层笼统描述，真正原因（连接超时/被拒/证书/读取中断）在 `source()` 链里。

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::State;

use crate::AppState;
use myshelltool_core::oauth_flow::{self, PollVerdict, TransportFault};

/// 开发者注册 OAuth App 后，把 GitHub 显示的 Client ID 填在这里（形如 Ov23liAbCdEf12345678，
/// 公开值，非密钥，可随源码分发）。留空则回退到构建期环境变量 MYST_GITHUB_CLIENT_ID（CI 注入用）。
const GITHUB_CLIENT_ID_INLINE: &str = "Ov23liv57OhgF7Md2kw4";

/// 最终生效的 client_id：内联常量优先，其次构建期环境变量；两者皆空 → sync_oauth_start
/// 返回带注册指引的 Err（空置不算编译失败）。
const GITHUB_CLIENT_ID: &str = if !GITHUB_CLIENT_ID_INLINE.is_empty() {
    GITHUB_CLIENT_ID_INLINE
} else {
    match option_env!("MYST_GITHUB_CLIENT_ID") {
        Some(id) => id,
        None => "",
    }
};

const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const ACCESS_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
/// Device Flow 仅申请 gist 读写（与手动 PAT 的 scope 一致）。
const OAUTH_SCOPE: &str = "gist";
const USER_AGENT: &str = "myshelltool";
/// 用户可见错误文案里 cause 链的字符上限（完整链只进应用日志）。
const CHAIN_UI_MAX_CHARS: usize = 240;
/// 非 2xx 正文摘录的字符上限（诊断用，先按秘密字面量脱敏再截断）。
const BODY_EXCERPT_MAX_CHARS: usize = 200;
/// client_id 未注册时的指引文案（UI 直接展示 errorMessage）。
const REGISTER_HINT: &str = "GitHub 登录尚未启用：开发者需先注册 OAuth App——\
     打开 github.com/settings/developers → New OAuth App（回调 URL 必填但用不到，填 https://github.com 即可）→ \
     勾选「Enable Device Flow」→ 将页面显示的 Client ID 填入 sync_oauth.rs 的 GITHUB_CLIENT_ID_INLINE \
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
/// Pending/SlowDown/Unstable 带 user_code：前端比对发起时记下的 userCode，不一致说明
/// 后端 session 已被另一处发起的登录覆盖，旧实例应静默停止轮询。
///
/// 【v2.7】传输故障不再用 `Err` 表达：`Err` 只留给后端内部错误（锁中毒），
/// 「这次没问到」与「GitHub 拒绝了」是两个不同结论，前端处置相反。
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
    /// 本次轮询遇到**临时**故障（网络/代理链路/429/5xx/非 JSON 响应）：设备码仍然有效，
    /// 前端应退避后重试，不要终止登录。
    Unstable { user_code: String, reason: String },
    /// 登录会话已不存在（被另一次登录覆盖，或已被取消/重启）：前端静默回到初始态。
    Superseded,
    /// 不可恢复（GitHub 明确报错 / token 落库失败）：前端停止流程并展示原因。
    Failed { reason: String },
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

/// 单例 HTTP 客户端：**必须复用**（reqwest::Client 持有连接池）。
fn http_client() -> Result<reqwest::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT.get_or_init(build_http_client).clone()
}

fn build_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        // connect 与整体分离限时：只设整体超时时，一次慢握手会把预算吃光，
        // 表现为「偶发失败」而不是「慢」。
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .tcp_keepalive(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {e}"))
}

/// reqwest 的 `Display` 只给最外层笼统描述，逐层取 `source()` 才是真原因。
fn error_chain(err: &dyn std::error::Error) -> String {
    /// cause 链深度上限（病态深链不刷屏；正常 reqwest 链 2-4 层）。
    const MAX_LEVELS: usize = 6;
    let mut parts: Vec<String> = Vec::new();
    let mut cur = Some(err);
    while let Some(e) = cur {
        let text = e.to_string();
        // 相邻重复层去重（reqwest 会在多层里重复同一句话）
        if !text.is_empty() && parts.last().map(|p| p != &text).unwrap_or(true) {
            parts.push(text);
        }
        if parts.len() >= MAX_LEVELS {
            if e.source().is_some() {
                parts.push("…".to_string());
            }
            break;
        }
        cur = e.source();
    }
    parts.join(" ← ")
}

/// 把 reqwest 错误的布尔特征映射成 core 的传输故障分类。
/// 顺序有意：连接超时同时满足 timeout 与 connect，用户更需要知道「超时」。
fn classify_transport_fault(err: &reqwest::Error) -> TransportFault {
    if err.is_timeout() {
        TransportFault::Timeout
    } else if err.is_connect() {
        TransportFault::Connect
    } else if err.is_body() || err.is_decode() {
        TransportFault::Body
    } else {
        TransportFault::Other
    }
}

/// 非 2xx 的正文常含真正的失败原因（代理/网关错误页），按脱敏摘录附到原因后面。
fn append_body_hint(verdict: PollVerdict, body: &str, device_code: &str) -> PollVerdict {
    let excerpt = myshelltool_core::redact_excerpt(body.trim(), &[device_code], BODY_EXCERPT_MAX_CHARS);
    if excerpt.is_empty() {
        return verdict;
    }
    let suffix = format!("；响应：{excerpt}");
    match verdict {
        PollVerdict::Retryable(r) => PollVerdict::Retryable(r + &suffix),
        PollVerdict::Fatal(r) => PollVerdict::Fatal(r + &suffix),
        other => other,
    }
}

/// core 的协议判定 → 命令返回类型（Success 由调用方在 token 落库成功后单独返回）。
fn verdict_to_result(
    verdict: PollVerdict,
    user_code: &str,
    interval: Option<u64>,
    state: &AppState,
) -> Result<OAuthPollResult, String> {
    Ok(match verdict {
        PollVerdict::Pending => OAuthPollResult::Pending {
            user_code: user_code.to_string(),
        },
        PollVerdict::SlowDown => OAuthPollResult::SlowDown {
            user_code: user_code.to_string(),
            interval,
        },
        PollVerdict::Denied => {
            clear_session(state);
            OAuthPollResult::Denied
        }
        PollVerdict::Expired => {
            clear_session(state);
            OAuthPollResult::Expired
        }
        // 临时故障：**不清 session**——设备码在 GitHub 端仍有效，重试仍可成功。
        PollVerdict::Retryable(reason) => OAuthPollResult::Unstable {
            user_code: user_code.to_string(),
            reason,
        },
        // 明确失败：清 session（设备码已无意义，重试只会重复同一结论）。
        PollVerdict::Fatal(reason) => {
            clear_session(state);
            OAuthPollResult::Failed { reason }
        }
        PollVerdict::Success => {
            return Err("内部错误：Success 判定不应走失败映射".to_string());
        }
    })
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
        .form(&[("client_id", GITHUB_CLIENT_ID), ("scope", OAUTH_SCOPE)])
        .send()
        .await
        .map_err(|e| format!("请求 GitHub 设备码失败: {}", error_chain(&e)))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let excerpt = myshelltool_core::redact_excerpt(text.trim(), &[], BODY_EXCERPT_MAX_CHARS);
        return Err(format!("获取设备码失败 (HTTP {status}): {excerpt}"));
    }
    let body: DeviceCodeResponse = resp
        .json()
        .await
        .map_err(|e| format!("解析设备码响应失败: {}", error_chain(&e)))?;

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

/// 单次轮询授权状态（前端按 interval 链式调用，失败时由前端退避重试）。
///
/// 成功（拿到 access_token）→ 直接写 SecretStore（github-pat）→ 清 session。
#[tauri::command]
pub async fn sync_oauth_poll(state: State<'_, AppState>) -> Result<OAuthPollResult, String> {
    // 短暂 lock 取出轮询所需数据，绝不持锁跨 await（后续网络请求耗时最长 20s）。
    let (device_code, user_code, expired) = {
        let guard = state.sync_oauth_pending.lock().map_err(|_| lock_err())?;
        match guard.as_ref() {
            // 无 session = 被另一次登录覆盖 / 已取消 / 应用重启过：前端静默回初始态，
            // 而不是弹一句与用户操作对不上的「登录失败」。
            None => return Ok(OAuthPollResult::Superseded),
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
    let resp = match client
        .post(ACCESS_TOKEN_URL)
        .header("Accept", "application/json")
        .form(&[
            ("client_id", GITHUB_CLIENT_ID),
            ("device_code", device_code.as_str()),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            // 传输故障 ≠ 授权被拒：设备码仍有效，交给前端退避重试（core 判定）。
            let fault = classify_transport_fault(&e);
            let detail = error_chain(&e);
            log::warn!("sync_oauth_poll 传输失败({fault:?}): {detail}");
            let verdict = oauth_flow::verdict_for_transport_fault(fault);
            let PollVerdict::Retryable(hint) = verdict else {
                return Err("内部错误：传输故障应判定为可重试".to_string());
            };
            let shown = myshelltool_core::redact_excerpt(&detail, &[device_code.as_str()], CHAIN_UI_MAX_CHARS);
            return Ok(OAuthPollResult::Unstable {
                user_code,
                reason: format!("{hint}：{shown}"),
            });
        }
    };

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let verdict = append_body_hint(
            oauth_flow::verdict_for_http_status(status.as_u16()),
            &body,
            &device_code,
        );
        return verdict_to_result(verdict, &user_code, None, &state);
    }

    // 2xx 正文**不回显**：它可能含有 access_token（成功响应就是 token 本体），
    // 解析失败时只能报告「不是合法 JSON」这一事实（§8 凭据红线）。
    let text = resp.text().await.unwrap_or_default();
    let body: TokenResponse = match serde_json::from_str(&text) {
        Ok(body) => body,
        Err(e) => {
            log::warn!("sync_oauth_poll 响应非 JSON（HTTP {status}，{} 字节）: {e}", text.len());
            let verdict = oauth_flow::verdict_for_unparsable_body(status.as_u16());
            return verdict_to_result(verdict, &user_code, None, &state);
        }
    };

    let verdict = oauth_flow::verdict_for_token_body(body.access_token.as_deref(), body.error.as_deref());
    if verdict == PollVerdict::Success {
        let token = body.access_token.unwrap_or_default();
        let store = myshelltool_core::SecretStore::new(
            &state.secret_store_dir,
            Box::new(crate::dpapi_codec::DpapiCodec),
        );
        if let Err(e) = store.save("github-pat", &token) {
            // token 已换到手但落库失败：设备码已被消费，重试无意义 —— 如实报失败。
            log::error!("sync_oauth_poll token 落库失败: {e}");
            clear_session(&state);
            return Ok(OAuthPollResult::Failed {
                reason: format!("token 写入本地安全存储失败: {e}"),
            });
        }
        clear_session(&state);
        return Ok(OAuthPollResult::Success); // token 明文不出后端进程
    }

    verdict_to_result(verdict, &user_code, body.interval, &state)
}

/// 取消进行中的登录流程（清内存槽；无 session 也 Ok，容错）。
#[tauri::command]
pub async fn sync_oauth_cancel(state: State<'_, AppState>) -> Result<(), String> {
    clear_session(&state);
    Ok(())
}
