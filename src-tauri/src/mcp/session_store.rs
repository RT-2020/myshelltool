//! MCP session 名单持久化（v0.21：会话跨重启恢复的存储层）。
//!
//! 解决什么：`LocalSessionManager` 的 session 表纯内存，GUI 重启后外部 host
//! 持有的旧 `Mcp-Session-Id` 全部 404——host 收到 404 后重新 initialize 是
//! 协议 SHOULD 而非 MUST（ZCode 实测不自愈），于是每次发版/重启后 host 侧
//! MCP 连接就死。rmcp 3.x 的 tower 层已内置完整恢复编排：initialize 成功后
//! `store()` 落名单、HTTP DELETE 时 `delete()` 摘除、内存 miss 时自动
//! `load()` → `restore_session()`（用原 id 重建 worker）→ 重放 initialize
//! 握手（带 SessionRestoreMarker，handler 可区分恢复与真实握手）。本模块
//! 只提供名单的磁盘形态；前提是 token 也跨重启复用（lib.rs setup 恢复，
//! 否则 host 的 URL 在鉴权层就 401，到不了 session 层）。
//!
//! 安全：名单内容只有 session id 与 host 的 initialize 参数（客户端名/协议
//! 版本/能力声明），无凭据；恢复仍受 token 鉴权门（http_server.rs
//! mcp_auth_gate）前置拦截。名单保留最近 [`MAX_SESSIONS`] 条，超出按插入序
//! 淘汰最旧；文件损坏/读失败按「无名单」处理（等价旧行为：404），不阻断
//! server 启动——丢的只是恢复能力，不是用户数据。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rmcp::transport::streamable_http_server::session::{
    SessionState, SessionStore, SessionStoreError,
};

/// 名单保留条数上限。host 反复 initialize 会不断产生新条目（旧 id 对应的
/// session 已废弃），16 条足够覆盖「多 host 常驻 + 重启窗口」的恢复需求。
const MAX_SESSIONS: usize = 16;

/// JSON 文件版 SessionStore：`<data_dir>/mcp-sessions.json`。
pub struct FileSessionStore {
    path: PathBuf,
    /// 串行化「读-改-写」；临界区内全是同步 fs 操作，guard 不跨 await。
    lock: Mutex<()>,
}

/// 文件形态。sessions 用 Vec 保持插入序（淘汰最旧 = remove(0)）。
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct FileBody {
    sessions: Vec<FileEntry>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FileEntry {
    id: String,
    /// host 的 initialize 参数（SessionState 未 derive Serialize，落盘存其
    /// serde_json::Value 形态，load 时反序列化回类型）。
    initialize_params: serde_json::Value,
}

impl FileSessionStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("mcp-sessions.json"),
            lock: Mutex::new(()),
        }
    }

    /// 读名单：读不到/解析失败 → 空名单（见模块头：丢恢复能力不丢数据）。
    fn read_body(&self) -> FileBody {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// 原子写：temp + rename（与 persist_endpoint / 编辑器写回同款兜底思路）。
    fn write_body(&self, body: &FileBody) -> Result<(), String> {
        let json = serde_json::to_string(body).map_err(|e| e.to_string())?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }
}

fn store_err(e: String) -> SessionStoreError {
    Box::new(std::io::Error::other(e))
}

#[async_trait::async_trait]
impl SessionStore for FileSessionStore {
    async fn load(&self, session_id: &str) -> Result<Option<SessionState>, SessionStoreError> {
        // 锁中毒恢复：临界区只有文件读写，无可破坏的不变量（into_inner 惯例）。
        let _g = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let body = self.read_body();
        let Some(entry) = body.sessions.iter().find(|e| e.id == session_id) else {
            return Ok(None);
        };
        let initialize_params = serde_json::from_value(entry.initialize_params.clone())
            .map_err(|e| store_err(format!("session 名单条目反序列化失败: {e}")))?;
        Ok(Some(SessionState::new(initialize_params)))
    }

    async fn store(&self, session_id: &str, state: &SessionState) -> Result<(), SessionStoreError> {
        let _g = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let mut body = self.read_body();
        body.sessions.retain(|e| e.id != session_id);
        body.sessions.push(FileEntry {
            id: session_id.to_string(),
            initialize_params: serde_json::to_value(&state.initialize_params)
                .map_err(|e| store_err(format!("session 名单条目序列化失败: {e}")))?,
        });
        while body.sessions.len() > MAX_SESSIONS {
            body.sessions.remove(0);
        }
        self.write_body(&body).map_err(store_err)
    }

    async fn delete(&self, session_id: &str) -> Result<(), SessionStoreError> {
        let _g = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let mut body = self.read_body();
        let before = body.sessions.len();
        body.sessions.retain(|e| e.id != session_id);
        if body.sessions.len() == before {
            return Ok(());
        }
        self.write_body(&body).map_err(store_err)
    }
}

/// 清空名单文件（`mcp_reset_token` 连带调用：token 已轮换，旧 session 条目
/// 对应的 host URL 在鉴权层就 401，永远到不了 session 层，留着是死数据）。
/// 删除失败只 warn——名单残留无安全影响。
pub fn clear(data_dir: &Path) {
    let path = data_dir.join("mcp-sessions.json");
    match std::fs::remove_file(&path) {
        Ok(()) => log::info!("MCP session 名单已随 token 重置清空"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("清空 MCP session 名单失败（残留无安全影响）: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(tag: &str) -> (FileSessionStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("myst-session-store-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        // 每次干净起点
        let _ = std::fs::remove_file(dir.join("mcp-sessions.json"));
        (FileSessionStore::new(&dir), dir)
    }

    fn sample_state(client: &str) -> SessionState {
        SessionState::new(
            serde_json::from_value(serde_json::json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": client, "version": "0.1.0"}
            }))
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn store_load_roundtrip() {
        let (store, _dir) = temp_store("roundtrip");
        store.store("s1", &sample_state("zcode")).await.unwrap();
        let loaded = store.load("s1").await.unwrap().unwrap();
        assert_eq!(loaded.initialize_params.client_info.name, "zcode");
        assert!(store.load("missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_removes_entry() {
        let (store, _dir) = temp_store("delete");
        store.store("s1", &sample_state("a")).await.unwrap();
        store.delete("s1").await.unwrap();
        assert!(store.load("s1").await.unwrap().is_none());
        // 幂等：删除不存在的条目不出错
        store.delete("s1").await.unwrap();
    }

    #[tokio::test]
    async fn capacity_evicts_oldest() {
        let (store, _dir) = temp_store("evict");
        for i in 0..(MAX_SESSIONS + 3) {
            store
                .store(&format!("s{i}"), &sample_state(&format!("c{i}")))
                .await
                .unwrap();
        }
        assert!(store.load("s0").await.unwrap().is_none());
        assert!(store.load("s2").await.unwrap().is_none());
        assert!(store.load("s3").await.unwrap().is_some());
        assert!(store.load(&format!("s{}", MAX_SESSIONS + 2)).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn corrupted_file_reads_as_empty() {
        let (store, dir) = temp_store("corrupt");
        std::fs::write(dir.join("mcp-sessions.json"), "{not json").unwrap();
        assert!(store.load("s1").await.unwrap().is_none());
        // store 仍可自愈写入
        store.store("s1", &sample_state("a")).await.unwrap();
        assert!(store.load("s1").await.unwrap().is_some());
    }
}
