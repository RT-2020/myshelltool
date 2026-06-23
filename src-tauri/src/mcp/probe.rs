//! MCP 就绪探测（v1.4：HTTP 健康检查，取代 v1.2 的一次性 spawn 探测）。
//!
//! v1.4 根本性变化：MCP server 内嵌 GUI 进程，探测 = 向自己的 HTTP endpoint
//! 发一个 MCP initialize 请求验证可达。**不再 spawn 任何子进程**，从源头消除
//! v1.2 的僵尸进程 + os error 32 问题。
//!
//! 为什么不直接读 mcp-endpoint.json 就算「ok」：文件存在 ≠ server 真在响应。
//! HTTP 握手能确认 transport 层 + 协议层都活着（端口被防火墙挡、axum panic、
//! rmcp handler 初始化失败等都能被抓到）。开销极低（localhost 一来回 <1ms）。

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// HTTP 健康检查超时。localhost 一来回正常 <10ms，2s 足够兜底任何卡顿。
const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(2);

/// MCP 协议版本（与 server.rs 握手时声明一致）。
const PROTOCOL_VERSION: &str = "2025-06-18";

/// 探测结果（嵌入 mcp_status 返回前端）。
///
/// 字段刻意与 v1.2 的 McpProbeResult 保持一致（ok/reason/detail/exe_path→endpoint/
/// server_info/probed_at），让前端 mcp.js 零改动。exe_path 语义改为 HTTP endpoint URL。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpProbeResult {
    /// MCP 能否正常工作（HTTP endpoint 可达 + initialize 握手成功）。
    pub ok: bool,
    /// 失败分类码（ok=false 时有值）：endpoint_not_found / http_error / timeout / bad_protocol。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// 失败详情文案（ok=false 时有值，前端直接展示）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 探测到的 HTTP endpoint URL（v1.2 是 exe_path，v1.4 改为 endpoint）。
    /// 字段名保留 exe_path 以兼容前端 mcp.js（读 probe.exePath）。
    pub exe_path: String,
    /// 握手成功时 server 返回的 serverInfo（name/version）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_info: Option<ServerInfoDto>,
    /// 探测发生的本地时间（ISO8601）。
    pub probed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfoDto {
    pub name: String,
    pub version: String,
}

/// 向 MCP HTTP endpoint 发 initialize 请求做健康检查。
///
/// endpoint 形如 `http://127.0.0.1:41235/mcp`。成功返回 ok=true + server_info，
/// 失败按分类码返回（endpoint_not_found=没读到 endpoint 配置 / http_error=连不上 /
/// timeout / bad_protocol=响应非合法 MCP 帧）。
pub async fn probe_endpoint(endpoint: &str) -> McpProbeResult {
    let probed_at = chrono::Utc::now().to_rfc3339();
    let exe_path = endpoint.to_string();

    let check = async {
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "myshelltool-healthcheck", "version": env!("CARGO_PKG_VERSION") }
            }
        });

        let client = reqwest::Client::builder()
            .timeout(HEALTH_CHECK_TIMEOUT)
            .build()
            .map_err(|e| ("http_error", format!("构建 HTTP client 失败: {e}")))?;

        // Streamable HTTP 要求 Accept 头含 text/event-stream。
        let resp = client
            .post(endpoint)
            .header("Accept", "application/json, text/event-stream")
            .json(&init_req)
            .send()
            .await
            .map_err(|e| ("http_error", format!("连接 MCP endpoint 失败: {e}")))?;

        if !resp.status().is_success() {
            return Err((
                "http_error",
                format!("MCP endpoint 返回 HTTP {}", resp.status()),
            ));
        }

        // 响应是 SSE 流（text/event-stream），取第一个 data: 行的 JSON。
        let body = resp
            .text()
            .await
            .map_err(|e| ("http_error", format!("读响应体失败: {e}")))?;

        let json_str = extract_first_json_from_sse(&body)
            .ok_or_else(|| ("bad_protocol", format!("响应中无 JSON-RPC 帧: {}", body.chars().take(200).collect::<String>())))?;

        let resp_val: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| ("bad_protocol", format!("响应非合法 JSON: {e}")))?;

        let id = resp_val.get("id").and_then(|v| v.as_i64());
        if id != Some(1) {
            return Err(("bad_protocol", "响应 id 不匹配".to_string()));
        }
        if let Some(err_obj) = resp_val.get("error") {
            return Err(("bad_protocol", format!("server 返回错误: {}", err_obj)));
        }
        let result = resp_val
            .get("result")
            .ok_or(("bad_protocol", "响应无 result 字段".to_string()))?;
        let server_info = result
            .get("serverInfo")
            .ok_or(("bad_protocol", "响应无 serverInfo".to_string()))?;
        let name = server_info
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let version = server_info
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        Ok::<_, (&str, String)>(ServerInfoDto { name, version })
    };

    match tokio::time::timeout(HEALTH_CHECK_TIMEOUT, check).await {
        Ok(Ok(server_info)) => McpProbeResult {
            ok: true,
            reason: None,
            detail: None,
            exe_path,
            server_info: Some(server_info),
            probed_at,
        },
        Ok(Err((reason, detail))) => fail(&exe_path, detail, probed_at, reason),
        Err(_) => fail(
            &exe_path,
            format!("健康检查超时（{}s）", HEALTH_CHECK_TIMEOUT.as_secs()),
            probed_at,
            "timeout",
        ),
    }
}

/// 探测「endpoint 尚未启动」的便捷构造（mcp_status 读不到 endpoint 时用）。
pub fn fail_no_endpoint(probed_at: &str) -> McpProbeResult {
    fail(
        "",
        "MCP HTTP server 尚未启动或未写入 endpoint 配置".to_string(),
        probed_at.to_string(),
        "endpoint_not_found",
    )
}

/// 从 SSE 响应体提取第一个 `data: {...}` 的 JSON 字符串。
///
/// Streamable HTTP 的成功响应是 `text/event-stream`，每帧 `data: <json>\n\n`。
/// 健康检查只关心第一帧（initialize 响应）。
fn extract_first_json_from_sse(body: &str) -> Option<&str> {
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(json) = trimmed.strip_prefix("data: ") {
            let json = json.trim();
            if json.starts_with('{') {
                return Some(json);
            }
        }
    }
    // 非流式响应可能是裸 JSON（部分实现），兜底尝试整体解析。
    let trimmed = body.trim();
    if trimmed.starts_with('{') {
        return Some(trimmed);
    }
    None
}

fn fail(exe_path: &str, detail: String, probed_at: String, reason: &str) -> McpProbeResult {
    McpProbeResult {
        ok: false,
        reason: Some(reason.to_string()),
        detail: Some(detail),
        exe_path: exe_path.to_string(),
        server_info: None,
        probed_at,
    }
}
