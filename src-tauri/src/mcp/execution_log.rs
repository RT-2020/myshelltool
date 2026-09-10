//! MCP 工具执行日志（v2）。
//!
//! 记录每次真实触发远程执行的工具调用（ssh_exec / disk_usage / system_status /
//! service_status / sftp_remove）：哪台服务器、什么命令、什么审批决策、什么结果。
//! 持久化在 `<data_dir>/mcp-execution-log.json`，GUI 面板经
//! mcp_list_execution_logs / mcp_clear_execution_logs 查看/清空。
//!
//! 凭据红线（AGENTS.md §8）：Entry 不含任何密码/passphrase 字段，绝不记录。
//!
//! 并发与可靠性（评审 P0）：
//! - append/clear 经模块级 `tokio::sync::Mutex` 串行化（多 HTTP 会话并发 append
//!   不会交错写坏文件）。
//! - 落盘用「写 `.tmp` + fs::rename 原子替换」，读方永远读到完整旧文件或完整
//!   新文件，不会读到半截 JSON。
//! - 落盘失败 best-effort：`log::warn!` 后正常返回，**不阻断工具调用本身**。
//! - 惰性清理：append 时距上次清理 >24h 才做（删 30 天前条目 + 按条数截断保留
//!   最新 1000 条），无常驻定时任务。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// 单条执行日志。全部字段 camelCase 序列化（前端 AppTable 直接消费）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionLogEntry {
    /// 条目 id（uuid v4，倒序 tie-breaker 用）。
    pub id: String,
    /// 调用时刻（Unix 毫秒）。
    pub timestamp_ms: u64,
    /// 当次调用的拦截等级快照（"minimal" / "strict"）。
    pub level: String,
    /// 工具名（ssh_exec / disk_usage / ...）。
    pub tool: String,
    /// 目标资产 id（资产已删除时仍保留原值）。
    pub asset_id: String,
    /// 资产名（读库失败/资产不存在时为空串，不丢条目）。
    pub asset_name: String,
    /// 主机地址（兜底语义同上）。
    pub host: String,
    /// 端口（元数据缺失时为 0）。
    pub port: u16,
    /// 登录用户名（兜底语义同上）。
    pub username: String,
    /// 真实执行的命令文本（只读工具填内部拼接的固定命令；sftp_remove 填目标路径）。
    pub command: String,
    /// AI 声明的意图（来自工具调用 intent 参数，可为空）。
    pub intent: String,
    /// 审批决策（decision 模块常量）。
    pub decision: String,
    /// 执行结果（outcome 模块常量）。
    pub outcome: String,
    /// 输出摘要（头 250 + 尾 250 字符，中间截断标记）。
    pub output_summary: String,
}

/// decision 字段取值（审批决策路径）。
pub mod decision {
    /// 无需审批的执行工具（disk_usage 等只读工具 / sftp_remove 之外）。
    pub const NOT_REQUIRED: &str = "not_required";
    /// 白名单命中放行。
    pub const AUTO_APPROVED: &str = "auto_approved";
    /// Minimal 档对未知/黄名单及**非毁灭黑名单**命令的放行（v2.1 起非毁灭
    /// 黑名单不再恒拦，一并走此口径记日志供事后审计）。
    pub const MINIMAL_ALLOWED: &str = "minimal_allowed";
    /// 毁灭性（catastrophic）命令恒拦，未执行（v2.1：两档等级一致，不进审批链）。
    pub const HARD_BLOCKED: &str = "hard_blocked";
    /// elicitation 确认框中用户接受。
    pub const ELICITATION_ACCEPTED: &str = "elicitation_accepted";
    /// elicitation 确认框中用户拒绝。
    pub const ELICITATION_DECLINED: &str = "elicitation_declined";
    /// GUI 弹窗中用户接受。
    pub const GUI_ACCEPTED: &str = "gui_accepted";
    /// GUI 弹窗中用户拒绝。
    pub const GUI_DECLINED: &str = "gui_declined";
    /// fail-secure 拒绝（emit 失败 / headless 无 GUI）。
    pub const REJECTED: &str = "rejected";
    /// GUI 弹窗 60s 超时。
    pub const TIMEOUT: &str = "timeout";
}

/// outcome 字段取值（执行结果）。
pub mod outcome {
    /// SSH 通道执行成功。注意语义是通道成功，非命令退出码 0（退出码已随
    /// exec 返回文本结构化给出，本字段不重复记录）。
    pub const OK: &str = "ok";
    /// 执行出错（CallToolResult.is_error）。
    pub const ERROR: &str = "error";
    /// 审批未通过，未执行。
    pub const SKIPPED: &str = "skipped";
}

/// 落盘结构：上次清理时间 + 条目列表（追加序，旧在前）。
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecutionLogStore {
    last_cleanup_ms: u64,
    entries: Vec<ExecutionLogEntry>,
}

// ─── 常量：保留策略 ───

/// 条目保留期：30 天。
const RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1000;
/// 惰性清理最小间隔：24h（append 时检查）。
const CLEANUP_INTERVAL_MS: u64 = 24 * 60 * 60 * 1000;
/// 条数硬上限：保留最新 1000 条。
const MAX_ENTRIES: usize = 1000;

// ─── 并发：模块级写锁（append/clear 串行化）───

static WRITE_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn write_lock() -> &'static tokio::sync::Mutex<()> {
    WRITE_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

// ─── 路径与读写 ───

/// mcp-execution-log.json 路径。data_dir 与 config.rs 同源（mcp_data_dir()）。
pub fn execution_log_path(data_dir: &Path) -> PathBuf {
    data_dir.join("mcp-execution-log.json")
}

/// 读 store。文件不存在 / 空内容 → 重新开始（空 store，默认 lastCleanupMs=0
/// 会触发下次 append 时立即清理一次）。
///
/// **文件损坏 → 改名隔离保留证据**（`mcp-execution-log.corrupt-<unix秒>.json`）
/// + log::error，再以空 store 继续：审计历史不能静默消失，隔离文件可供人工
/// 抢救。已知边界：list_entries 不持写锁，隔离改名与并发 append 的原子替换
/// 存在极窄竞窗——若此刻文件刚被替换，改名移走的是新内容（仍完整保留在
/// 隔离文件中，且下一次 append 会重建），不丢数据，只错位。
fn load_store(path: &Path) -> ExecutionLogStore {
    if !path.exists() {
        return ExecutionLogStore::default();
    }
    match std::fs::read_to_string(path) {
        Ok(json) if !json.trim().is_empty() => match serde_json::from_str(&json) {
            Ok(store) => store,
            Err(e) => {
                let secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let quarantine = path.with_file_name(format!(
                    "{}.corrupt-{secs}.json",
                    path.file_stem().and_then(|s| s.to_str()).unwrap_or("mcp-execution-log")
                ));
                match std::fs::rename(path, &quarantine) {
                    Ok(()) => log::error!(
                        "mcp execution log 文件损坏（解析失败: {e}），原文件已隔离到 {}，从空日志重新开始",
                        quarantine.display()
                    ),
                    Err(rename_err) => log::error!(
                        "mcp execution log 文件损坏（解析失败: {e}），隔离改名失败: {rename_err}，从空日志重新开始（原损坏文件保留原地）"
                    ),
                }
                ExecutionLogStore::default()
            }
        },
        _ => ExecutionLogStore::default(),
    }
}

/// 原子写：先写 `<path>.tmp` 再 fs::rename 替换（Windows 上 std rename 语义为
/// 覆盖已存在目标），读方不会见到半截文件。
fn save_store_atomic(path: &Path, store: &ExecutionLogStore) -> Result<(), String> {
    let tmp_path = {
        let mut s = path.as_os_str().to_os_string();
        s.push(".tmp");
        PathBuf::from(s)
    };
    let json = serde_json::to_string(store)
        .map_err(|e| format!("序列化 mcp-execution-log.json: {e}"))?;
    std::fs::write(&tmp_path, json).map_err(|e| format!("写日志临时文件: {e}"))?;
    std::fs::rename(&tmp_path, path).map_err(|e| {
        // 失败时尽力清掉 tmp，避免残留占用。
        let _ = std::fs::remove_file(&tmp_path);
        format!("原子替换 mcp-execution-log.json: {e}")
    })
}

/// 惰性清理：距上次清理 >24h 时删 30 天前条目，再按条数截断保留最新 1000 条。
fn maybe_cleanup(store: &mut ExecutionLogStore, now_ms: u64) {
    if now_ms.saturating_sub(store.last_cleanup_ms) <= CLEANUP_INTERVAL_MS {
        return;
    }
    let cutoff = now_ms.saturating_sub(RETENTION_MS);
    store.entries.retain(|e| e.timestamp_ms >= cutoff);
    if store.entries.len() > MAX_ENTRIES {
        let excess = store.entries.len() - MAX_ENTRIES;
        store.entries.drain(0..excess);
    }
    store.last_cleanup_ms = now_ms;
}

// ─── 对外 API ───

/// 追加一条执行日志。锁内 load→push→惰性清理→原子 save。
/// 落盘失败 best-effort（log::warn 后正常返回），不阻断工具调用。
pub async fn append_entry(data_dir: &Path, entry: ExecutionLogEntry) {
    let _guard = write_lock().lock().await;
    let path = execution_log_path(data_dir);
    let mut store = load_store(&path);
    let now_ms = entry.timestamp_ms;
    store.entries.push(entry);
    maybe_cleanup(&mut store, now_ms);
    if let Err(e) = save_store_atomic(&path, &store) {
        log::warn!("mcp execution log: 落盘失败（best-effort 忽略）: {e}");
    }
}

/// 读最近条目：timestampMs 倒序（同时间戳按 id 倒序 tie-break），最多 limit 条。
/// 只读不改盘上数据，无需拿写锁（rename 原子性保证读到完整文件）。
pub fn list_entries(data_dir: &Path, limit: usize) -> Vec<ExecutionLogEntry> {
    let mut store = load_store(&execution_log_path(data_dir));
    store.entries.sort_by(|a, b| {
        b.timestamp_ms
            .cmp(&a.timestamp_ms)
            .then_with(|| b.id.cmp(&a.id))
    });
    store.entries.truncate(limit);
    store.entries
}

/// 清空全部条目并重置 lastCleanupMs（下次 append 重新计时）。
pub async fn clear_entries(data_dir: &Path) -> Result<(), String> {
    let _guard = write_lock().lock().await;
    let path = execution_log_path(data_dir);
    let mut store = load_store(&path);
    store.entries.clear();
    store.last_cleanup_ms = now_ms();
    save_store_atomic(&path, &store)
}

/// 当前 Unix 毫秒（chrono，与项目其余时间戳一致）。
pub fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

/// 通用头尾截断：保留头 `head` + 尾 `tail` 个字符，中间用 `…[截断]…` 标记，
/// 按 char 边界切（不破坏 UTF-8）。总字符数不超 `head + tail` 时原样返回。
/// v2.1 起泛化为 pub：tools.rs 的 ssh_exec 返回组装（8000+8000）与本模块
/// 的输出摘要（250+250）共用同一实现。
pub fn truncate_middle(text: &str, head: usize, tail: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= head + tail {
        return text.to_string();
    }
    let head_s: String = chars[..head].iter().collect();
    let tail_s: String = chars[chars.len() - tail..].iter().collect();
    format!("{head_s}…[截断]…{tail_s}")
}

/// 输出摘要：头 250 字符 + 尾 250 字符，中间用 `…[截断]…` 标记，
/// 保证尾部 stderr（命令错误通常在末尾）不丢。按 char 切避免 UTF-8 边界。
pub fn summarize_output(text: &str) -> String {
    const HEAD: usize = 250;
    const TAIL: usize = 250;
    truncate_middle(text, HEAD, TAIL)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(ts: u64, id: &str) -> ExecutionLogEntry {
        ExecutionLogEntry {
            id: id.to_string(),
            timestamp_ms: ts,
            level: "minimal".to_string(),
            tool: "ssh_exec".to_string(),
            asset_id: "a1".to_string(),
            asset_name: "测试".to_string(),
            host: "127.0.0.1".to_string(),
            port: 22,
            username: "root".to_string(),
            command: "echo hi".to_string(),
            intent: "测试".to_string(),
            decision: decision::AUTO_APPROVED.to_string(),
            outcome: outcome::OK.to_string(),
            output_summary: "hi".to_string(),
        }
    }

    #[test]
    fn summarize_keeps_head_and_tail() {
        let long = "A".repeat(300) + "B".repeat(300).as_str();
        let s = summarize_output(&long);
        assert!(s.contains("…[截断]…"));
        assert!(s.starts_with("A"));
        assert!(s.ends_with("B"));
        // 头尾各 250 char + 标记
        assert_eq!(s.chars().count(), 250 + "…[截断]…".chars().count() + 250);
    }

    #[test]
    fn summarize_short_text_untouched() {
        assert_eq!(summarize_output("hello"), "hello");
    }

    #[test]
    fn cleanup_drops_expired_and_truncates_count() {
        let mut store = ExecutionLogStore {
            last_cleanup_ms: 0,
            entries: (0..1200).map(|i| sample_entry(i, &format!("id{i:04}"))).collect(),
        };
        // now = 1000：条目时间戳 < (1000 - 30天) 全删；再截断到 1000 条
        maybe_cleanup(&mut store, 30 * 24 * 60 * 60 * 1000 + 1000);
        assert_eq!(store.entries.len(), MAX_ENTRIES);
        // 保留的是时间戳最新（最大）的 1000 条
        assert_eq!(store.entries.first().unwrap().timestamp_ms, 200);
        assert_eq!(store.entries.last().unwrap().timestamp_ms, 1199);
        assert!(store.last_cleanup_ms > 0);
    }

    #[test]
    fn cleanup_respects_interval() {
        let mut store = ExecutionLogStore::default();
        store.last_cleanup_ms = 10_000;
        store.entries.push(sample_entry(1, "x"));
        maybe_cleanup(&mut store, 10_000 + 1000); // 间隔 < 24h → 不清理
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.last_cleanup_ms, 10_000);
    }
}
