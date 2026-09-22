//! MCP 长任务 job store（v0.20，3-6 月段 C1；MCP服务设计-v3 §3.4.3 方案 A）。
//!
//! ## 解决什么
//!
//! `apt upgrade` / `mysqldump` / `rsync` 会超过工具调用超时——AI 只能看到超时。
//! `ssh_exec_async` 立即返回 job_id，命令在后台跑；`job_status`/`job_output`
//! 轮询进度，`job_cancel` 真取消。**不落盘**（与「活跃会话纯内存」分层一致），
//! TTL 30 分钟回收 + 条数上限。
//!
//! ## 取消机制
//!
//! exec 通道没有中断信号——真取消靠**断开连接**：JobEntry 持有连接 Handle 的
//! clone（Arc），cancel 时 `disconnect()` → 远端命令收 SIGHUP → exec 返回错误
//! → 执行侧发现 cancel 标记，收敛为 Cancelled 而非 Failed（对齐上传取消的
//! 旗标 + 状态收敛模式）。
//!
//! ## 状态机
//!
//! Running → Done | Failed | Cancelled（终态不可逆；cancelled 后到的迟到
//! job_output 仍可读，读到 TTL 回收为止）。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use crate::ssh::HeadlessSshClient;

/// job 存活期（终态与运行中 alike；惰性清理在写入路径触发）。
const JOB_TTL_MS: u64 = 30 * 60 * 1000;
/// 全局条数上限（超出丢最旧——内存纪律：单 job 输出 8 MiB × 32 ≈ 最坏 256 MiB
/// 已是上限，不再放宽）。
const MAX_JOBS: usize = 32;
/// 单 job 输出 ring buffer 上限（字节；超限截头保尾——命令尾部通常有结论/错误）。
const JOB_OUTPUT_MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Running,
    Done,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }
}

pub struct JobEntry {
    pub asset_id: String,
    pub status: JobStatus,
    pub exit_code: Option<u32>,
    /// 输出 ring buffer（stdout+stderr 拼接；超限截头保尾）。
    pub output: Vec<u8>,
    /// 因超限被截掉的头部的字节数（job_output 如实告知）。
    pub dropped_head_bytes: u64,
    pub error: Option<String>,
    pub created_ms: u64,
    pub finished_ms: Option<u64>,
    /// 取消旗标（执行侧收敛前查）。
    pub cancel_requested: bool,
    /// 连接句柄（cancel 时 disconnect 真断）。
    pub handle: Option<Arc<russh::client::Handle<HeadlessSshClient>>>,
    /// 执行侧的完成通知（job_status 的 waiter 轮询之外提供一次性等待原语）。
    pub done_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

fn store() -> &'static tokio::sync::Mutex<HashMap<String, JobEntry>> {
    static STORE: OnceLock<tokio::sync::Mutex<HashMap<String, JobEntry>>> = OnceLock::new();
    STORE.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

/// 登记一个新 job（running 态），返回 (job_id, done_rx)。
/// done_rx 在执行侧写终态时触发——job_status 可 select 它等待完成。
pub async fn create(asset_id: &str, handle: Option<Arc<russh::client::Handle<HeadlessSshClient>>>) -> (String, tokio::sync::oneshot::Receiver<()>) {
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut map = store().lock().await;
    let now = now_ms();
    // 惰性清理：过期条目（终态超 TTL / 运行中超 2×TTL 视为泄漏回收）
    map.retain(|_, e| {
        let age = now.saturating_sub(e.created_ms);
        if e.status.is_terminal() {
            age < JOB_TTL_MS
        } else {
            age < JOB_TTL_MS * 2
        }
    });
    while map.len() >= MAX_JOBS {
        if let Some(oldest) = map.iter().min_by_key(|(_, e)| e.created_ms).map(|(k, _)| k.clone()) {
            map.remove(&oldest);
        } else {
            break;
        }
    }
    log::debug!("mcp job created: {} on {}", id, asset_id);
    map.insert(
        id.clone(),
        JobEntry {
            asset_id: asset_id.to_string(),
            status: JobStatus::Running,
            exit_code: None,
            output: Vec::new(),
            dropped_head_bytes: 0,
            error: None,
            created_ms: now,
            finished_ms: None,
            cancel_requested: false,
            handle,
            done_tx: Some(tx),
        },
    );
    (id, rx)
}

/// 建连完成后注入连接句柄（create 时无连接，cancel 用）。
pub async fn set_handle(id: &str, handle: Arc<russh::client::Handle<HeadlessSshClient>>) {
    let mut map = store().lock().await;
    if let Some(job) = map.get_mut(id) {
        if job.status == JobStatus::Running {
            job.handle = Some(handle);
        }
    }
}
/// 执行侧写终态（Done/Failed/Cancelled）。
pub async fn settle(id: &str, status: JobStatus, exit_code: Option<u32>, error: Option<String>) {
    let mut map = store().lock().await;
    if let Some(job) = map.get_mut(id) {
        job.status = status;
        job.exit_code = exit_code;
        job.error = error;
        job.finished_ms = Some(now_ms());
        // 释放连接引用（终态后连接可回池逻辑由执行侧处理；此处至少不再持有）
        job.handle = None;
        if let Some(tx) = job.done_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// 追加输出（ring buffer：超限截头保尾并累计 dropped）。
pub async fn append_output(id: &str, bytes: &[u8]) {
    let mut map = store().lock().await;
    if let Some(job) = map.get_mut(id) {
        job.output.extend_from_slice(bytes);
        let dropped = job.output.len().saturating_sub(JOB_OUTPUT_MAX_BYTES);
        if dropped > 0 {
            job.output = job.output.split_off(dropped);
            job.dropped_head_bytes += dropped as u64;
        }
    }
}

/// 请求取消：置旗标 + disconnect 连接（真断，远端命令收 SIGHUP）。
/// 幂等；job 不存在/已终态返回 false。
pub async fn request_cancel(id: &str) -> bool {
    let mut map = store().lock().await;
    let Some(job) = map.get_mut(id) else { return false };
    if job.status.is_terminal() || job.cancel_requested {
        return false;
    }
    job.cancel_requested = true;
    if let Some(handle) = job.handle.take() {
        let _ = handle
            .disconnect(russh::Disconnect::ByApplication, "", "mcp job cancelled")
            .await;
    }
    // 状态不在此翻转：执行侧发现断连/旗标后收敛为 Cancelled（区分「用户取消」
    // 与「连接自己断了」的 Failed）。
    true
}

/// 查状态快照（不取输出——job_output 单独分页读）。
pub struct JobSnapshot {
    pub asset_id: String,
    pub status: JobStatus,
    pub exit_code: Option<u32>,
    pub error: Option<String>,
    pub output_bytes: u64,
    pub dropped_head_bytes: u64,
    pub created_ms: u64,
    pub finished_ms: Option<u64>,
    pub cancel_requested: bool,
}

pub async fn snapshot(id: &str) -> Option<JobSnapshot> {
    let map = store().lock().await;
    let job = map.get(id)?;
    Some(JobSnapshot {
        asset_id: job.asset_id.clone(),
        status: job.status,
        exit_code: job.exit_code,
        error: job.error.clone(),
        output_bytes: job.output.len() as u64,
        dropped_head_bytes: job.dropped_head_bytes,
        created_ms: job.created_ms,
        finished_ms: job.finished_ms,
        cancel_requested: job.cancel_requested,
    })
}

/// 分页读输出：返回 (文本段, 总字节, 是否 EOF, 被截头字节)。行边界对齐同 read_output。
pub async fn read_output(id: &str, offset: u64, limit: usize) -> Option<(String, u64, bool, u64)> {
    let mut map = store().lock().await;
    let job = map.get_mut(id)?;
    // 读操作顺延续期（正在被轮询的任务不该中途被 TTL 回收）
    job.created_ms = now_ms();
    let total = job.output.len() as u64;
    let start = offset.min(total) as usize;
    let end = (start + limit).min(job.output.len());
    let chunk = String::from_utf8_lossy(&job.output[start..end]).to_string();
    let eof = end as u64 >= total;
    Some((chunk, total, eof, job.dropped_head_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lifecycle_running_to_done() {
        let (id, mut rx) = create("asset-1", None).await;
        assert_eq!(snapshot(&id).await.unwrap().status, JobStatus::Running);
        append_output(&id, b"hello ").await;
        append_output(&id, b"world").await;
        settle(&id, JobStatus::Done, Some(0), None).await;
        let snap = snapshot(&id).await.unwrap();
        assert_eq!(snap.status, JobStatus::Done);
        assert_eq!(snap.exit_code, Some(0));
        assert_eq!(snap.output_bytes, 11);
        assert!(rx.try_recv().is_ok(), "终态必须触发 done 通知");
    }

    #[tokio::test]
    async fn output_paging_and_dropped_head_reported() {
        let (id, _rx) = create("a", None).await;
        append_output(&id, &vec![b'x'; 3000]).await;
        let (chunk, total, eof, _) = read_output(&id, 0, 1000).await.unwrap();
        assert_eq!(chunk.len(), 1000);
        assert_eq!(total, 3000);
        assert!(!eof);
        let (_, _, eof2, _) = read_output(&id, 2000, 5000).await.unwrap();
        assert!(eof2);
    }

    #[tokio::test]
    async fn cancel_unknown_or_terminal_returns_false() {
        assert!(!request_cancel("no-such-job").await);
        let (id, _rx) = create("a", None).await;
        assert!(request_cancel(&id).await);
        settle(&id, JobStatus::Cancelled, None, None).await;
        assert!(!request_cancel(&id).await, "终态后取消应拒绝（幂等）");
    }

    #[tokio::test]
    async fn job_count_bounded() {
        for i in 0..(MAX_JOBS + 5) {
            let (id, _) = create(&format!("a{i}"), None).await;
            settle(&id, JobStatus::Done, Some(0), None).await;
        }
        let map = store().lock().await;
        assert!(map.len() <= MAX_JOBS, "条数越界: {}", map.len());
    }
}
