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
    /// 【v0.20/A4】工具声明的风险等级（registry RiskClass：readonly/write/destructive）。
    #[serde(default)]
    pub risk: String,
    /// 【v0.20/A4】命中的审批策略分支（registry Policy：shell_exec/remote_write/…）。
    #[serde(default)]
    pub policy: String,
    /// 【v0.20/A4】执行段耗时毫秒（审批等待不计入；未执行=0）。
    #[serde(default)]
    pub duration_ms: u64,
    /// 【v0.20/A4】会话来源：new=新建 headless 连接（当前恒为此值；
    /// B3 会话池落地后还有 gui/pool）。
    #[serde(default)]
    pub session_source: String,
    /// 【v0.20/B1】授权范围判定结果（allowed / deny_all / denied_id /
    /// denied_no_match / denied_local_fs）；未启用 scope 的工具与旧日志为空串。
    #[serde(default)]
    pub scope_result: String,
    // truncated（B2 输出契约）随该阶段加入——当前截断是文本内嵌前缀，
    // 提前加字段只能靠形状探测填空（形态 E）。
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
    /// 【v0.20/B1】授权范围拒绝（资产在 scope 外 / deny_all / 本机 FS 未开放）。
    /// 与审批链的 REJECTED 分开——运维看日志时要能区分「用户拒绝了确认」
    /// 与「配置的边界拒绝了访问」。
    pub const SCOPE_DENIED: &str = "scope_denied";
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

/// `load_store` 的读取结果（**三态**，见函数注释）。
enum LoadOutcome {
    /// 文件不存在或为空内容 → 空 store（首次使用，合法）。
    Fresh,
    /// 读到且解析成功（含损坏已隔离后的空 store）。
    Loaded(ExecutionLogStore),
    /// 文件**存在但读不出来**（非 UTF-8 / 权限 / 被独占）→ 带原因。
    Unreadable(String),
}

/// 把不可用的日志文件改名隔离为 `<name>.corrupt-<unix秒>.json`，返回隔离路径
/// 的展示串（成功＝新路径，失败＝原路径 + 原因）。
///
/// 隔离而非删除：损坏/读不出的文件是排查依据（磁盘问题 / 旧版本写坏 / 被独占），
/// 改名后下一次 append 会重建全新文件。改名失败也不阻断（原文件留原地）。
fn quarantine_file(path: &Path, reason: &str) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let quarantine = path.with_file_name(format!(
        "{}.corrupt-{secs}.json",
        path.file_stem().and_then(|s| s.to_str()).unwrap_or("mcp-execution-log")
    ));
    match std::fs::rename(path, &quarantine) {
        Ok(()) => quarantine.display().to_string(),
        Err(rename_err) => format!(
            "{}（隔离改名失败: {rename_err}，{reason}）",
            path.display()
        ),
    }
}

/// 读 store。三态分明，**「读失败」绝不与「不存在」合并**：
/// - 不存在 / 空内容 → 空 store 继续（首次使用合法）；
/// - JSON 解析失败 → 改名隔离保留证据（`mcp-execution-log.corrupt-<unix秒>.json`）
///   + log::error，再以空 store 继续：审计历史不能静默消失，隔离文件可供人工抢救。
///   已知边界：list_entries 不持写锁，隔离改名与并发 append 的原子替换存在极窄
///   竞窗——若此刻文件刚被替换，改名移走的是新内容（仍完整保留在隔离文件中，
///   且下一次 append 会重建），不丢数据，只错位。
/// - **读取失败 → `Unreadable`（区别于前两者）**：曾把读取失败也折叠成空 store，
///   紧接着 append 原子覆盖 → 整份审计历史被「仅含 1 条」的新文件替换、无隔离
///   无日志（v2.5 只修了「解析失败」这一半）。
fn load_store(path: &Path) -> LoadOutcome {
    let json = match std::fs::read_to_string(path) {
        Ok(json) => json,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return LoadOutcome::Fresh,
        Err(e) => return LoadOutcome::Unreadable(e.to_string()),
    };
    if json.trim().is_empty() {
        return LoadOutcome::Fresh;
    }
    match serde_json::from_str::<ExecutionLogStore>(&json) {
        Ok(store) => LoadOutcome::Loaded(store),
        Err(e) => {
            let target = quarantine_file(path, &format!("解析失败: {e}"));
            log::error!(
                "mcp execution log 文件损坏（解析失败: {e}），原文件已隔离到 {target}，从空日志重新开始"
            );
            LoadOutcome::Loaded(ExecutionLogStore::default())
        }
    }
}

/// 原子写：委托 `myshelltool_core::write_atomic`（同目录临时文件 + rename；临时名带
/// pid+序号，多实例/并发不会互踩，失败保留原文件并清理临时文件）。
fn save_store_atomic(path: &Path, store: &ExecutionLogStore) -> Result<(), String> {
    let json = serde_json::to_string(store)
        .map_err(|e| format!("序列化 mcp-execution-log.json: {e}"))?;
    myshelltool_core::write_atomic(path, json).map_err(|e| format!("写执行日志: {e}"))
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
///
/// 两条 best-effort 语义（都不阻断工具调用本身）：
/// - 落盘失败 → `log::warn` 后返回；
/// - **读取失败**（文件存在但读不出来）→ 隔离原文件保留证据 + `log::error`，
///   本次**不写盘**。绝不能像早期实现那样把「读不出来」当「空日志」，
///   否则这次 append 会把整份审计历史替换成仅含 1 条的新文件（形态 C 事故）。
pub async fn append_entry(data_dir: &Path, entry: ExecutionLogEntry) {
    let _guard = write_lock().lock().await;
    let path = execution_log_path(data_dir);
    let mut store = match load_store(&path) {
        LoadOutcome::Fresh => ExecutionLogStore::default(),
        LoadOutcome::Loaded(store) => store,
        LoadOutcome::Unreadable(reason) => {
            let quarantine = quarantine_file(&path, &format!("读取失败: {reason}"));
            log::error!(
                "mcp execution log 读取失败（{reason}），已隔离到 {quarantine}；本次条目未落盘，避免覆盖原有审计历史"
            );
            return;
        }
    };
    let now_ms = entry.timestamp_ms;
    store.entries.push(entry);
    maybe_cleanup(&mut store, now_ms);
    if let Err(e) = save_store_atomic(&path, &store) {
        log::warn!("mcp execution log: 落盘失败（best-effort 忽略）: {e}");
    }
}

/// 读最近条目：timestampMs 倒序（同时间戳按 id 倒序 tie-break），最多 limit 条。
/// 只读不改盘上数据，无需拿写锁（rename 原子性保证读到完整文件）。
/// 读取失败 → 空列表（GUI 显示「暂无执行日志」），原因已由 load_store 记日志。
pub fn list_entries(data_dir: &Path, limit: usize) -> Vec<ExecutionLogEntry> {
    let mut store = match load_store(&execution_log_path(data_dir)) {
        LoadOutcome::Fresh => ExecutionLogStore::default(),
        LoadOutcome::Loaded(store) => store,
        LoadOutcome::Unreadable(_) => return Vec::new(),
    };
    store.entries.sort_by(|a, b| {
        b.timestamp_ms
            .cmp(&a.timestamp_ms)
            .then_with(|| b.id.cmp(&a.id))
    });
    store.entries.truncate(limit);
    store.entries
}

/// 清空全部条目并重置 lastCleanupMs（下次 append 重新计时）。
///
/// 读取失败时**拒绝清空**并回报原因：此时无法确认盘上历史内容，直接原子写空
/// store 等于把一份读不出来的审计历史静默抹掉（用户以为「清空成功」）。
/// 需用户显式处理（隔离/修复文件）后再清。
pub async fn clear_entries(data_dir: &Path) -> Result<(), String> {
    let _guard = write_lock().lock().await;
    let path = execution_log_path(data_dir);
    let mut store = match load_store(&path) {
        LoadOutcome::Fresh => ExecutionLogStore::default(),
        LoadOutcome::Loaded(store) => store,
        LoadOutcome::Unreadable(reason) => {
            return Err(format!(
                "执行日志文件读取失败（{reason}），已拒绝清空以免抹掉无法确认的历史内容；\
                 文件路径 {}，请先手动处理（重命名/删除）后重试",
                path.display()
            ));
        }
    };
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
            risk: "destructive".to_string(),
            policy: "shell_exec".to_string(),
            duration_ms: 12,
            session_source: "new".to_string(),
            scope_result: "allowed".to_string(),
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

    // ─── A4（v0.20）：新字段的 serde 兼容与凭据红线回归 ───

    /// 旧日志文件（无 risk/policy/durationMs/sessionSource 字段）必须能读：
    /// #[serde(default)] 兜底为空串/0，历史审计数据不因升级而丢失。
    #[test]
    fn legacy_entry_without_a4_fields_deserializes() {
        let legacy = serde_json::json!({
            "id": "old-1",
            "timestampMs": 1700000000000u64,
            "level": "strict",
            "tool": "ssh_exec",
            "assetId": "a1",
            "assetName": "旧机器",
            "host": "10.0.0.1",
            "port": 22,
            "username": "root",
            "command": "df -h", // fact-guard:allow locale-pinned-df serde 兼容测试的样例数据，非真实执行命令
            "intent": "巡检",
            "decision": "auto_approved",
            "outcome": "ok",
            "outputSummary": "..."
        });
        let entry: ExecutionLogEntry =
            serde_json::from_value(legacy).expect("旧格式条目必须可反序列化");
        assert_eq!(entry.risk, "");
        assert_eq!(entry.policy, "");
        assert_eq!(entry.duration_ms, 0);
        assert_eq!(entry.session_source, "");
    }

    /// 新字段序列化为 camelCase（前端 McpExecutionLogEntry 消费口径）。
    #[test]
    fn a4_fields_serialize_camel_case() {
        let json = serde_json::to_value(sample_entry(1, "x")).unwrap();
        assert!(json.get("durationMs").is_some(), "durationMs 字段缺失");
        assert!(json.get("sessionSource").is_some(), "sessionSource 字段缺失");
        assert_eq!(json.get("risk").and_then(|v| v.as_str()), Some("destructive"));
        assert_eq!(json.get("policy").and_then(|v| v.as_str()), Some("shell_exec"));
    }

    /// 凭据红线回归：A4 新字段不含任何凭据位（risk/policy/duration/source
    /// 均为枚举/数值，设计上无秘密；此测试锁住「未来给这些字段加自由文本」的意外）。
    #[test]
    fn a4_fields_contain_no_secret_by_design() {
        let entry = sample_entry(1, "x");
        for field in [&entry.risk, &entry.policy, &entry.session_source] {
            let lower = field.to_lowercase();
            assert!(
                !lower.contains("password") && !lower.contains("secret") && !lower.contains("token"),
                "A4 字段出现凭据类词形：{field}"
            );
        }
    }
}

// ─── v2.8 第五轮自 server.rs 迁入：执行日志组装三件（load_asset_meta/append/extract）───

use super::tools::McpToolContext;
/// 执行日志的记录范围：真实触发远程执行的工具调用所需的静态信息。
///
/// v2：所有真实触发远程执行的工具（ssh_exec / disk_usage / system_status /
/// service_status / sftp_remove）记日志；list_assets / list_sessions / 桩工具 /
/// 未知工具不记。command 填真实执行的命令文本（只读工具用 tools.rs 导出的
/// 固定命令常量，sftp_remove 填目标路径）。
pub(crate) struct LogScope {
    pub(crate) tool: &'static str,
    pub(crate) command: String,
    pub(crate) asset_id: String,
    pub(crate) intent: String,
}

use rmcp::model::CallToolResult;

pub(crate) fn load_asset_meta(ctx: &McpToolContext, asset_id: &str) -> (String, String, u16, String) {
    let empty = || (String::new(), String::new(), 0, String::new());
    let Ok(store) = myshelltool_core::load_connection_asset_store(&ctx.asset_store_path) else {
        return empty();
    };
    match store.assets.iter().find(|a| a.id == asset_id) {
        Some(a) => (a.name.clone(), a.host.clone(), a.port, a.username.clone()),
        None => empty(),
    }
}

/// A4/B1 附加字段（risk/policy/duration/session_source/scope_result），
/// 由 server.rs 从 registry spec + 执行段计时 + scope 预判构造。
pub(crate) struct LogExtras {
    pub(crate) risk: &'static str,
    pub(crate) policy: &'static str,
    pub(crate) duration_ms: u64,
    pub(crate) session_source: &'static str,
    pub(crate) scope_result: &'static str,
}

/// 落一条执行日志（append_entry 内部 best-effort，失败不阻断工具调用）。
pub(crate) async fn append_execution_log(
    ctx: &McpToolContext,
    scope: &LogScope,
    level_str: &str,
    decision_str: &str,
    outcome_str: &str,
    output_text: &str,
    extras: &LogExtras,
) {
    let (asset_name, host, port, username) = load_asset_meta(ctx, &scope.asset_id);
    let entry = ExecutionLogEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp_ms: now_ms(),
        level: level_str.to_string(),
        tool: scope.tool.to_string(),
        asset_id: scope.asset_id.clone(),
        asset_name,
        host,
        port,
        username,
        // 脱敏后落盘：命令原文可能含明文口令（`mysql -pP@ss`、`curl -u u:p`、
        // `--password=x`），而执行日志是**长期保留**的审计文件（30 天 / 1000 条），
        // 且会在 GUI 面板回显——违反 AGENTS.md §8「凭据不进日志」（形态 C 变体：
        // 不是折叠错误，而是把敏感值当普通文本落盘）。
        command: myshelltool_core::redact_command(&scope.command),
        intent: scope.intent.clone(),
        decision: decision_str.to_string(),
        outcome: outcome_str.to_string(),
        risk: extras.risk.to_string(),
        policy: extras.policy.to_string(),
        duration_ms: extras.duration_ms,
        session_source: extras.session_source.to_string(),
        scope_result: extras.scope_result.to_string(),
        // 输出同样脱敏（v2.6 backlog #5）：远端 stdout 可能含口令——`env` 输出的
        // `GITHUB_TOKEN=…`、`show create user` 的 `IDENTIFIED BY '<明文>'` 曾原样
        // 进摘要。先脱敏再截断：先截断会把 KEY=value 形态切坏、遮不全。
        output_summary: summarize_output(&myshelltool_core::redact_output(output_text)),
    };
    append_entry(&ctx.data_dir, entry).await;
}

/// 从 CallToolResult 提取全部 text content（拼成日志的 outputSummary 素材）。
pub(crate) fn extract_result_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}
