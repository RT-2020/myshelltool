//! MCP 截断输出的服务端缓存与 cursor 取回（MCP服务设计-v3 §3.4.2，阶段 B2）。
//!
//! ## 解决什么
//!
//! `ssh_exec` 的返回有截断保护（头尾保留），但旧实现截断后**原文不可取回**——
//! AI 排查大日志只能猜。现在：截断发生时把完整输出存入本缓存并签发 cursor
//! （随机 uuid，不可猜、不含路径），AI 用 `read_output` 工具按 offset/limit
//! 分页取回。TTL 10 分钟 + 条数/大小上限，内存有界。
//!
//! ## 边界
//!
//! - **只服务 exec 输出**，不做文件读取旁路：`sftp_read_file` 的续读走它自身
//!   的 offset 参数（每次照常过敏感路径审批）——若 read_output 能读文件，
//!   cursor 就成了绕过审批的第二通道。
//! - cursor 到期/不存在返回明确错误而非空结果（防「读到了空」的误判）。
//! - 内存纪律：单条 ≤ 8 MiB（超出截头保尾——最近输出对排查最有价值），
//!   全局 ≤ 32 条，store 时惰性清理过期（>10 min）+ 挤掉最旧。

use std::collections::HashMap;
use std::sync::OnceLock;

/// 单条缓存上限（字节）。超限保尾部（远端命令的报错通常在末尾）。
const MAX_ENTRY_BYTES: usize = 8 * 1024 * 1024;
/// 全局条数上限（超出丢最旧——HashMap 无序，用 created_ms 找最旧）。
const MAX_ENTRIES: usize = 32;
/// 条目存活期。
const TTL_MS: u64 = 10 * 60 * 1000;

struct OutputEntry {
    /// 已按上限截过的完整输出字节。
    bytes: Vec<u8>,
    /// 是否因超单条上限被截头（取回时提示 AI「开头 N 字节未保留」）。
    dropped_head_bytes: u64,
    created_ms: u64,
}

fn cache() -> &'static tokio::sync::Mutex<HashMap<String, OutputEntry>> {
    static CACHE: OnceLock<tokio::sync::Mutex<HashMap<String, OutputEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

/// 存入完整输出，返回 cursor（uuid v4）。
/// 超单条上限时截头保尾并记录被丢弃的字节数（cursor 取回时如实告知）。
pub async fn store(mut bytes: Vec<u8>) -> String {
    let dropped = bytes.len().saturating_sub(MAX_ENTRY_BYTES);
    if dropped > 0 {
        bytes = bytes.split_off(dropped);
    }
    let cursor = uuid::Uuid::new_v4().to_string();
    let mut map = cache().lock().await;
    // 惰性清理：过期条目 + 超条数丢最旧
    let now = now_ms();
    map.retain(|_, e| now.saturating_sub(e.created_ms) < TTL_MS);
    while map.len() >= MAX_ENTRIES {
        if let Some(oldest) = map
            .iter()
            .min_by_key(|(_, e)| e.created_ms)
            .map(|(k, _)| k.clone())
        {
            map.remove(&oldest);
        } else {
            break;
        }
    }
    map.insert(
        cursor.clone(),
        OutputEntry {
            bytes,
            dropped_head_bytes: dropped as u64,
            created_ms: now,
        },
    );
    cursor
}

/// 按 cursor 取回一段。返回 (文本段, 总字节数, 是否已到末尾, 被截头字节数)。
/// cursor 不存在/过期 → Err（明确错误，不返回空段）。
pub async fn read(
    cursor: &str,
    offset: u64,
    limit: usize,
) -> Result<(String, u64, bool, u64), String> {
    let mut map = cache().lock().await;
    let entry = map
        .get_mut(cursor)
        .ok_or_else(|| {
            format!(
                "cursor 不存在或已过期（TTL 10 分钟）：{cursor}。请重新执行原命令获取新 cursor。"
            )
        })?;
    // 读操作顺手续期：AI 正在分页取回的条目不该中途过期
    entry.created_ms = now_ms();
    let total = entry.bytes.len() as u64;
    let start = offset.min(total) as usize;
    let end = (start + limit).min(entry.bytes.len());
    let chunk = String::from_utf8_lossy(&entry.bytes[start..end]).to_string();
    let eof = end as u64 >= total;
    Ok((chunk, total, eof, entry.dropped_head_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn store_and_read_roundtrip_with_paging() {
        let data = vec![b'a'; 1000];
        let cursor = store(data).await;
        // 首页 400 字节
        let (chunk, total, eof, dropped) = read(&cursor, 0, 400).await.unwrap();
        assert_eq!(chunk.len(), 400);
        assert_eq!(total, 1000);
        assert!(!eof);
        assert_eq!(dropped, 0);
        // 跳页 + 末页 eof
        let (_, _, eof2, _) = read(&cursor, 600, 400).await.unwrap();
        assert!(eof2);
        // 越界 offset 夹到 total（返回空段 + eof，不报错）
        let (chunk3, _, eof3, _) = read(&cursor, 5000, 100).await.unwrap();
        assert_eq!(chunk3, "");
        assert!(eof3);
    }

    #[tokio::test]
    async fn oversize_entry_keeps_tail_and_reports_dropped() {
        let data = vec![b'x'; MAX_ENTRY_BYTES + 100];
        let cursor = store(data).await;
        let (chunk, total, eof, dropped) = read(&cursor, 0, 10).await.unwrap();
        assert_eq!(dropped, 100, "应如实报告被截头字节数");
        assert_eq!(total as usize, MAX_ENTRY_BYTES);
        assert_eq!(chunk.len(), 10);
        assert!(!eof);
    }

    #[tokio::test]
    async fn unknown_cursor_is_error_not_empty() {
        let err = read("no-such-cursor", 0, 10).await.unwrap_err();
        assert!(err.contains("不存在"), "必须明确报错而非返回空段: {err}");
    }

    #[tokio::test]
    async fn entry_count_is_bounded() {
        for i in 0..(MAX_ENTRIES + 5) {
            let c = store(vec![i as u8; 16]).await;
            assert!(!c.is_empty());
        }
        // store 后条数不超上限（丢最旧）
        let map = cache().lock().await;
        assert!(map.len() <= MAX_ENTRIES, "全局条数越界: {}", map.len());
    }
}
