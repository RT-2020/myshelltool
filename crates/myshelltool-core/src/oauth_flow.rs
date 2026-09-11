//! GitHub Device Flow 轮询判定（协议语义，与 HTTP 实现解耦，纯函数可单测）。
//!
//! 事故：`sync_oauth_poll` 把「网络抖动」（DNS/连接/TLS 握手/代理链路/服务端 5xx）
//! 与「GitHub 明确拒绝授权」折叠成同一个 `Err`，前端拿到 Err 就停止轮询并显示
//! 「登录失败：轮询 GitHub 授权状态失败: error sending request for url (...)」——
//! **一次瞬时抖动就作废整个登录流程**，而此刻设备码在 GitHub 端仍然有效（900s），
//! 用户却必须重新走一遍浏览器授权。根子是把「取不到结果」当成了「结果是失败」：
//! 两者正确处置相反——前者退避重试，后者立即停止并如实告知原因。
//!
//! 判据（协议 + HTTP 语义，不猜）：
//! - 传输层失败（连不上/超时/响应读了一半）→ 可重试：传输故障不携带授权语义；
//! - HTTP 429 / 5xx → 可重试（限流、服务端临时故障）；
//! - 其它非 2xx（400/401/403/404）→ 不可重试（请求本身有问题，重试只会重复同一错）；
//! - 2xx：`authorization_pending` / `slow_down` 继续等；`access_denied` /
//!   `expired_token` 终止；未知 error 码终止（GitHub 已明确报错）；
//! - 2xx 但响应体不是合法 JSON（代理节点挂掉时返回错误页/截断响应）→ 可重试。
//!
//! 重试安全性：设备码轮询是**幂等只读**操作（授权完成前不产生任何副作用），
//! 退避重试不会「重复消费」设备码；退避调度由前端持有（它本就串行等待一次网络
//! 往返），本模块只回答「这一次往返的结论是什么」。

/// 传输层故障分类（调用方从 `reqwest::Error` 的布尔特征映射而来，core 不依赖 reqwest）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportFault {
    /// 连接或整体请求超时。
    Timeout,
    /// 建立连接失败（DNS/TCP/TLS/代理不可达）。
    Connect,
    /// 响应体读取中断（连上了但被中途掐断）。
    Body,
    /// 其它传输错误。
    Other,
}

impl TransportFault {
    /// 给用户看的一句话（含最可能的排查方向，不武断断言原因）。
    pub fn hint(self) -> &'static str {
        match self {
            TransportFault::Timeout => "连接 GitHub 超时（网络或代理链路慢/不通）",
            TransportFault::Connect => "无法连接 GitHub（网络不通、代理未启动或被拦截）",
            TransportFault::Body => "读取 GitHub 响应中断（连接被中途掐断）",
            TransportFault::Other => "请求 GitHub 失败（网络异常）",
        }
    }
}

/// 单次轮询的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollVerdict {
    /// 用户尚未完成授权，按 interval 继续轮询。
    Pending,
    /// 轮询过快，GitHub 要求放慢。
    SlowDown,
    /// 已拿到 access_token。
    Success,
    /// 用户在授权页拒绝。
    Denied,
    /// 设备码过期。
    Expired,
    /// 临时故障：设备码仍有效，应退避后重试（附可读原因）。
    Retryable(String),
    /// 不可恢复：重试无意义，应停止流程并如实告知原因。
    Fatal(String),
}

/// 传输层失败 → 结论（**一律可重试**：传输故障不携带任何授权语义）。
pub fn verdict_for_transport_fault(fault: TransportFault) -> PollVerdict {
    PollVerdict::Retryable(fault.hint().to_string())
}

/// 非 2xx 状态码 → 结论。429（限流）与 5xx（服务端临时故障）可重试，其余不可重试。
pub fn verdict_for_http_status(status: u16) -> PollVerdict {
    if status == 429 || (500..600).contains(&status) {
        return PollVerdict::Retryable(format!(
            "GitHub 授权接口暂时不可用（HTTP {status}），稍后自动重试"
        ));
    }
    PollVerdict::Fatal(format!("GitHub 授权接口返回 HTTP {status}"))
}

/// 2xx 但响应体无法解析为 JSON（典型来源：代理节点死亡时返回的错误页、被截断的响应）
/// → 可重试。
pub fn verdict_for_unparsable_body(status: u16) -> PollVerdict {
    PollVerdict::Retryable(format!(
        "GitHub 授权接口返回了非 JSON 内容（HTTP {status}，可能是网络中间层改写/截断的响应）"
    ))
}

/// 2xx 且 JSON 解析成功后的响应体判定。`access_token` 优先于 `error`。
pub fn verdict_for_token_body(access_token: Option<&str>, error: Option<&str>) -> PollVerdict {
    if access_token.is_some_and(|t| !t.is_empty()) {
        return PollVerdict::Success;
    }
    match error {
        Some("authorization_pending") => PollVerdict::Pending,
        Some("slow_down") => PollVerdict::SlowDown,
        Some("access_denied") => PollVerdict::Denied,
        Some("expired_token") => PollVerdict::Expired,
        // 未知 error 码：GitHub 已明确报错（如 incorrect_client_credentials、
        // device_flow_disabled），重试不会改变结论。
        Some(other) => PollVerdict::Fatal(format!("GitHub 登录失败：{other}")),
        // 既无 token 也无 error：协议异常（可能是残缺响应），按可重试处理。
        None => PollVerdict::Retryable(
            "GitHub 授权响应异常（既无 access_token 也无 error），可能是残缺响应".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn retryable(v: PollVerdict) -> bool {
        matches!(v, PollVerdict::Retryable(_))
    }

    #[test]
    fn transport_faults_are_always_retryable() {
        for fault in [
            TransportFault::Timeout,
            TransportFault::Connect,
            TransportFault::Body,
            TransportFault::Other,
        ] {
            assert!(retryable(verdict_for_transport_fault(fault)), "{fault:?} 应可重试");
            assert!(!fault.hint().is_empty());
        }
    }

    #[test]
    fn server_side_and_rate_limit_statuses_are_retryable() {
        for status in [429u16, 500, 502, 503, 504] {
            assert!(retryable(verdict_for_http_status(status)), "HTTP {status} 应可重试");
        }
        assert!(retryable(verdict_for_unparsable_body(200)));
    }

    #[test]
    fn client_side_statuses_are_fatal() {
        for status in [400u16, 401, 403, 404, 422] {
            assert!(
                matches!(verdict_for_http_status(status), PollVerdict::Fatal(_)),
                "HTTP {status} 不应重试"
            );
        }
        let fatal = verdict_for_http_status(403);
        assert!(format!("{fatal:?}").contains("403"), "原因里要带状态码");
    }

    #[test]
    fn device_flow_error_codes_map_to_protocol_states() {
        assert_eq!(verdict_for_token_body(None, Some("authorization_pending")), PollVerdict::Pending);
        assert_eq!(verdict_for_token_body(None, Some("slow_down")), PollVerdict::SlowDown);
        assert_eq!(verdict_for_token_body(None, Some("access_denied")), PollVerdict::Denied);
        assert_eq!(verdict_for_token_body(None, Some("expired_token")), PollVerdict::Expired);
    }

    #[test]
    fn unknown_error_code_is_fatal_not_retried() {
        let v = verdict_for_token_body(None, Some("device_flow_disabled"));
        match v {
            PollVerdict::Fatal(reason) => assert!(reason.contains("device_flow_disabled")),
            other => panic!("未知 error 码应终止流程，实际 {other:?}"),
        }
    }

    #[test]
    fn token_wins_over_error_and_empty_token_is_not_success() {
        assert_eq!(verdict_for_token_body(Some("gho_x"), None), PollVerdict::Success);
        assert_eq!(
            verdict_for_token_body(Some("gho_x"), Some("authorization_pending")),
            PollVerdict::Success
        );
        // 空串 token 不算成功（不能把「读到了空字段」当「拿到凭据」）
        assert!(retryable(verdict_for_token_body(Some(""), None)));
    }

    #[test]
    fn body_without_token_or_error_is_retryable() {
        // 协议异常按可重试：无论重试几次都不会把失败伪装成成功，只是多花几次往返
        assert!(retryable(verdict_for_token_body(None, None)));
    }
}
