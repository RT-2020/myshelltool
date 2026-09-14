//! 出站 HTTP 单例客户端 + reqwest 错误链工具。
//!
//! 为什么必须单例（v2.7 同源遗留的收口，见 AGENTS.md §9 v2.7）：
//! `reqwest::Client` 持有连接池，每次请求新建 Client = 丢弃池子重新
//! DNS+TCP+TLS 握手——每一次握手都是独立的失败机会，且无任何超时的
//! `Client::new()` 在网络抖动/代理异常时会让 IPC 调用**无限期挂住**
//!（sync.rs 的 gist_create/get/update 三处曾如此）。
//!
//! 超时画像与 `sync_oauth.rs` 的轮询客户端一致（connect 与整体分离限时：
//! 只设整体超时时一次慢握手会吃光预算，表现为「偶发失败」而非「慢」）。
//! system-proxy 特性保持默认开启（走 Windows 系统代理，Clash/v2rayN 场景）；
//! 需要**绕过**代理的调用方（如 127.0.0.1 健康检查）自建客户端，不用本单例。
//!
//! 当前使用方：`sync.rs`（Gist push/pull）与 `sync_oauth.rs`（Device Flow），
//! 两者都打 GitHub API——共享池对同域请求有真实复用收益。

use std::sync::OnceLock;
use std::time::Duration;

const USER_AGENT: &str = "myshelltool";

/// 单例 HTTP 客户端：**必须复用**（连接池在 Client 里）。
/// 构建失败（如 TLS 后端初始化异常）以 Err 缓存，调用方拿到的永远是同一结论。
pub(crate) fn shared_client() -> Result<reqwest::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT.get_or_init(build_shared_client).clone()
}

fn build_shared_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .tcp_keepalive(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {e}"))
}

/// reqwest 的 `Display` 只给最外层笼统描述（`error sending request for url ...`），
/// 逐层取 `source()` 才是真原因（超时/连接被拒/读取中断）。相邻重复层去重，
/// 病态深链封顶 6 层。
pub(crate) fn error_chain(err: &dyn std::error::Error) -> String {
    /// cause 链深度上限（正常 reqwest 链 2-4 层）。
    const MAX_LEVELS: usize = 6;
    let mut parts: Vec<String> = Vec::new();
    let mut cur = Some(err);
    while let Some(e) = cur {
        let text = e.to_string();
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
