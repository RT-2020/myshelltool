//! MCP 就绪探测（无状态、按需、一次性）。
//!
//! 这是 v1.2 状态检测的**唯一正确形态**：不是运行时状态机，而是一个 ping 一样的
//! 纯函数查询。GUI 主动 spawn 一份 myshelltool-mcp.exe，跑一次 MCP initialize
//! 握手，成功即「MCP 可用」。
//!
//! 为什么放弃心跳/连接计数：那些方案都在回答「Claude 此刻有没有在用 MCP」，
//! 依赖外部宿主行为；而用户要的是「我装的这份 MCP 能不能正常工作」，与 Claude
//! 无关。探测 = 自己验证，打开程序即可见结果。
//!
//! 为什么手写 JSON-RPC 而不用 rmcp client：只发一行 initialize 读一行响应，
//! 比引入 rmcp client + transport-child-process 两个 feature gate 简单一个数量级，
//! 零依赖风险。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

/// MCP 协议版本（rmcp 1.7 / Cargo.toml 注释标注的基线）。
/// server 端通常协商接受，即使不完全匹配。
const PROTOCOL_VERSION: &str = "2025-06-18";

/// 握手超时。spawn + initialize 在本地子进程，正常 <1s，5s 足够兜底卡死。
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// 探测结果（嵌入 mcp_status 返回前端）。
#[derive(Debug, Clone, Serialize)]
pub struct McpProbeResult {
    /// MCP 能否正常工作（exe 存在 + 能启动 + initialize 握手成功）。
    pub ok: bool,
    /// 失败分类码（ok=false 时有值）：exe_not_found / spawn_failed / timeout / bad_protocol / io_error。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// 失败详情文案（ok=false 时有值，前端直接展示给用户看原因）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 探测到的 exe 路径（用于前端展示 + 诊断）。
    pub exe_path: String,
    /// 握手成功时，server 返回的 serverInfo（name/version）。
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

/// 定位 myshelltool-mcp.exe 路径。
///
/// 策略：优先用 `current_exe()`（GUI 自身）的同级目录拼 `myshelltool-mcp.exe`。
/// dev 模式两者同在 `target/debug/`，release 同在 `target/release/`，零配置。
///
/// 安装版当前不打包 mcp exe（独立 follow-up），此时探测会 exe_not_found——
/// 这本身是正确信息（确实没装）。后续打包时在此适配。
pub fn resolve_mcp_exe_path(custom: Option<&str>) -> Result<std::path::PathBuf, String> {
    if let Some(p) = custom {
        let path = std::path::PathBuf::from(p);
        return Ok(path);
    }
    let gui_exe =
        std::env::current_exe().map_err(|e| format!("无法获取 GUI exe 路径: {e}"))?;
    let dir = gui_exe
        .parent()
        .ok_or_else(|| "GUI exe 无父目录".to_string())?;
    Ok(dir.join("myshelltool-mcp.exe"))
}

/// 执行一次 MCP 就绪探测。
///
/// 流程：找 exe → 检查存在 → spawn + stdio pipes → 发 initialize → 读响应 → kill。
/// 子进程用 RAII guard 保证无论成功失败都 kill+wait，绝不泄漏。
pub async fn probe_mcp(custom_exe_path: Option<&str>) -> McpProbeResult {
    let probed_at = chrono::Utc::now().to_rfc3339();

    // 1. 定位 exe
    let exe_path = match resolve_mcp_exe_path(custom_exe_path) {
        Ok(p) => p,
        Err(e) => {
            return fail("", format!("resolve_path: {e}"), probed_at, "io_error");
        }
    };
    let exe_path_str = exe_path.to_string_lossy().to_string();

    // 2. 检查存在
    if !exe_path.exists() {
        return fail(
            &exe_path_str,
            "未找到 myshelltool-mcp.exe（需构建或拷贝到 GUI 同级目录）".to_string(),
            probed_at,
            "exe_not_found",
        );
    }

    // 3. spawn + 握手（整体包超时）
    let probe_inner = async {
        let mut child = Command::new(&exe_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null()) // 丢弃 stderr（logger 走文件，不干扰）
            .kill_on_drop(true) // RAII 兜底：guard drop 时 tokio 会 kill
            .spawn()
            .map_err(|e| ("spawn_failed", format!("启动 exe 失败: {e}")))?;

        // stdin/stdout 取出后 child 仍持有进程，guard drop 时 kill_on_drop 生效
        let stdin = child
            .stdin
            .take()
            .ok_or(("spawn_failed", "无 stdin pipe".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(("spawn_failed", "无 stdout pipe".to_string()))?;

        // 4. 发 initialize（每行一个 JSON，非 Content-Length 分帧）
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "myshelltool-probe", "version": env!("CARGO_PKG_VERSION") }
            }
        });
        let mut stdin = stdin;
        let line = format!("{}\n", init_req);
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| ("io_error", format!("写 stdin 失败: {e}")))?;
        stdin
            .flush()
            .await
            .map_err(|e| ("io_error", format!("flush stdin 失败: {e}")))?;
        drop(stdin); // 关闭 stdin 让 server 知道不再有输入

        // 5. 读 stdout 第一行（initialize 响应）
        let mut reader = BufReader::new(stdout);
        let mut response_line = String::new();
        // 可能先读到日志行（若 logger 配置漂移），循环跳过直到拿到 JSON-RPC 响应。
        // 加迭代上限（32 行）防御 server 因 bug 持续输出非 JSON 行——避免靠 5s 超时兜底。
        let mut skipped = 0usize;
        const MAX_NOISE_LINES: usize = 32;
        loop {
            if skipped >= MAX_NOISE_LINES {
                return Err((
                    "bad_protocol",
                    format!("连续 {MAX_NOISE_LINES} 行无 JSON-RPC 帧，疑似 server 输出异常"),
                ));
            }
            response_line.clear();
            let n = reader
                .read_line(&mut response_line)
                .await
                .map_err(|e| ("io_error", format!("读 stdout 失败: {e}")))?;
            if n == 0 {
                return Err(("bad_protocol", "exe 提前关闭 stdout（可能启动崩溃）".to_string()));
            }
            let trimmed = response_line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // 必须是 JSON 且有 jsonrpc 字段才算协议帧
            if trimmed.starts_with('{') && trimmed.contains("\"jsonrpc\"") {
                break;
            }
            // 否则视为噪音行，继续读
            skipped += 1;
        }

        // 6. 解析响应
        let resp: serde_json::Value = serde_json::from_str(response_line.trim())
            .map_err(|e| ("bad_protocol", format!("响应非合法 JSON: {e}")))?;

        // 校验 id==1 且有 result
        let id = resp.get("id").and_then(|v| v.as_i64());
        if id != Some(1) {
            return Err(("bad_protocol", "响应 id 不匹配".to_string()));
        }
        if let Some(err_obj) = resp.get("error") {
            return Err((
                "bad_protocol",
                format!("server 返回错误: {}", err_obj),
            ));
        }
        let result = resp
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

    match tokio::time::timeout(HANDSHAKE_TIMEOUT, probe_inner).await {
        Ok(Ok(server_info)) => McpProbeResult {
            ok: true,
            reason: None,
            detail: None,
            exe_path: exe_path_str,
            server_info: Some(server_info),
            probed_at,
        },
        Ok(Err((reason, detail))) => fail(&exe_path_str, detail, probed_at, reason),
        Err(_) => fail(
            &exe_path_str,
            format!("握手超时（{}s）", HANDSHAKE_TIMEOUT.as_secs()),
            probed_at,
            "timeout",
        ),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// resolve_mcp_exe_path(None) 应基于 current_exe 同级拼 myshelltool-mcp.exe。
    /// 不验证文件存在（那是 probe_mcp 的职责），只验证路径解析逻辑。
    #[test]
    fn resolve_exe_path_default_is_sibling_of_current_exe() {
        let path = resolve_mcp_exe_path(None).expect("current_exe 应可解析");
        assert!(
            path.ends_with("myshelltool-mcp.exe"),
            "默认路径应以 myshelltool-mcp.exe 结尾，实际: {}",
            path.display()
        );
        // 与 GUI exe 同目录
        let gui_dir = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        assert_eq!(path.parent(), Some(gui_dir.as_path()));
    }

    /// custom 路径优先于默认探测，原样返回（不规范化、不校验存在）。
    #[test]
    fn resolve_exe_path_custom_overrides_default() {
        let custom = "/some/custom/path/mcp.exe";
        let path = resolve_mcp_exe_path(Some(custom)).unwrap();
        assert_eq!(path, std::path::PathBuf::from(custom));
    }

    /// custom 空串也应原样返回（解析层不判空，存在性由 probe_mcp 兜底）。
    #[test]
    fn resolve_exe_path_custom_empty_string_passes_through() {
        let path = resolve_mcp_exe_path(Some("")).unwrap();
        assert_eq!(path, std::path::PathBuf::from(""));
    }

    /// McpProbeResult 的 Serialize 输出 ok=true 时省略 reason/detail。
    #[test]
    fn probe_result_serialize_ok_omits_failure_fields() {
        let ok_result = McpProbeResult {
            ok: true,
            reason: None,
            detail: None,
            exe_path: "/x/mcp.exe".into(),
            server_info: Some(ServerInfoDto {
                name: "myshelltool".into(),
                version: "0.3.0".into(),
            }),
            probed_at: "2026-06-17T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&ok_result).unwrap();
        assert!(json.contains("\"ok\":true"));
        assert!(!json.contains("reason"), "成功时不应有序列化的 reason 字段");
        assert!(!json.contains("detail"));
        assert!(json.contains("\"name\":\"myshelltool\""));
    }

    /// McpProbeResult 失败时 reason/detail 必须出现（前端依赖这俩字段显示原因）。
    #[test]
    fn probe_result_serialize_failure_includes_reason_detail() {
        let fail_result = fail(
            "/missing/mcp.exe",
            "未找到 exe".into(),
            "2026-06-17T00:00:00Z".into(),
            "exe_not_found",
        );
        let json = serde_json::to_string(&fail_result).unwrap();
        assert!(json.contains("\"ok\":false"));
        assert!(json.contains("\"reason\":\"exe_not_found\""));
        assert!(json.contains("未找到 exe"));
        assert!(json.contains("\"exe_path\":\"/missing/mcp.exe\""));
        // server_info 因 skip_serializing_if 在失败时不出现
        assert!(!json.contains("server_info"));
    }
}
