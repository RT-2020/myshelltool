//! MCP headless 会话池（MCP服务设计-v3 §3.4.1，阶段 B3）。
//!
//! ## 解决什么
//!
//! 旧路径每次工具调用都完整走一遍 TCP + 认证 + host key 校验（`diagnose_server`
//! prompt 连调 4 个工具 = 4 次握手），高频调用易触发服务端 MaxStartups。现在：
//! **GUI 已连接会话 → 池中 headless 连接 → 新建** 三层复用（GUI 层在
//! tools.rs::exec_on_asset 里判，本模块只管 headless 连接池）。
//!
//! ## 生命周期纪律（本设计最容易出错的地方）
//!
//! 四条失效路径：
//! 1. **用户手动断开**：GUI 会话复用层天然规避（find_session_by_host 查不到
//!    就落到池层）；池内 headless 连接是 MCP 自有的，不受 GUI 断开影响。
//! 2. **服务器侧掐断**（ClientAliveInterval）：`is_closed()` 快速判死 +
//!    使用失败自动失效重建（`log::warn` 留痕，不静默——重建的连接在返回里
//!    语义不变，但日志可查）。
//! 3. **资产被删除**：`invalidate_asset` 钩子（lib.rs 删除命令处调用）。
//! 4. **凭据更新**：`AuthSnapshot` 比对——get 时与当前资产的凭据三元组比对，
//!    不一致即失效（防改密码后复用旧连接产生难解释的认证失败）。
//!
//! 上界：`MAX_CONNECTIONS`（默认 8，与规划书池容量一致；exec_many 的 fan-out
//! 并发上限与此对齐）+ idle TTL 5 分钟（get 时惰性清理）。

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::ssh::HeadlessSshClient;

/// 池容量（与 exec_many fan-out 上限对齐的设计值）。
const MAX_CONNECTIONS: usize = 8;
/// 空闲存活期。
const IDLE_TTL_MS: u64 = 5 * 60 * 1000;

/// 凭据形态快照：命中池条目时与当前资产比对，不一致即失效（凭据更新路径）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuthSnapshot {
    pub credential_id: Option<String>,
    pub private_key_path: Option<String>,
    pub private_key_credential_id: Option<String>,
    pub passphrase_credential_id: Option<String>,
}

impl AuthSnapshot {
    pub fn from_asset(asset: &myshelltool_core::ConnectionAsset) -> Self {
        Self {
            credential_id: asset.credential_id.clone(),
            private_key_path: asset.private_key_path.clone(),
            private_key_credential_id: asset.private_key_credential_id.clone(),
            passphrase_credential_id: asset.passphrase_credential_id.clone(),
        }
    }
}

struct PoolEntry {
    /// russh 0.49 的 Handle 不可 Clone（同 tcpip_forward 要 &mut 的那族约束），
    /// 用 Arc 共享所有权：get 分发 Arc clone，池与使用方各持一份——
    /// 全部 drop 时连接才关闭。
    handle: std::sync::Arc<russh::client::Handle<HeadlessSshClient>>,
    auth: AuthSnapshot,
    last_used_ms: u64,
}

fn pool() -> &'static tokio::sync::Mutex<HashMap<String, PoolEntry>> {
    static POOL: OnceLock<tokio::sync::Mutex<HashMap<String, PoolEntry>>> = OnceLock::new();
    POOL.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

/// 取一条可用连接：key 命中 + 凭据快照一致 + 未超时 + 未关闭。
/// 未命中/失效返回 None（调用方新建，用完 `put` 回池）。
/// 取用即续期（正在被使用的连接不该被 TTL 淘汰）。
pub async fn get(
    asset_id: &str,
    auth: &AuthSnapshot,
) -> Option<std::sync::Arc<russh::client::Handle<HeadlessSshClient>>> {
    let mut map = pool().lock().await;
    let now = now_ms();
    // 惰性清理过期条目（顺手，锁内一次性做完）
    map.retain(|_, e| now.saturating_sub(e.last_used_ms) < IDLE_TTL_MS);
    let entry = map.get_mut(asset_id)?;
    entry.last_used_ms = now;
    if entry.auth != *auth {
        // 凭据已更新：旧连接作废（调用方会新建，新凭据生效）
        let _ = map.remove(asset_id);
        return None;
    }
    let handle = entry.handle.clone();
    if handle.is_closed() {
        let _ = map.remove(asset_id);
        return None;
    }
    Some(handle)
}

/// 归还一条连接（新建后调用）。超容量丢最旧。
pub async fn put(
    asset_id: &str,
    auth: AuthSnapshot,
    handle: std::sync::Arc<russh::client::Handle<HeadlessSshClient>>,
) {
    let mut map = pool().lock().await;
    let now = now_ms();
    map.retain(|_, e| now.saturating_sub(e.last_used_ms) < IDLE_TTL_MS);
    while map.len() >= MAX_CONNECTIONS {
        if let Some(oldest) = map.iter().min_by_key(|(_, e)| e.last_used_ms).map(|(k, _)| k.clone()) {
            map.remove(&oldest);
        } else {
            break;
        }
    }
    map.insert(
        asset_id.to_string(),
        PoolEntry {
            handle,
            auth,
            last_used_ms: now,
        },
    );
}

/// 主动失效（资产删除/编辑钩子）。幂等。
pub async fn invalidate_asset(asset_id: &str) {
    let mut map = pool().lock().await;
    let removed = map.remove(asset_id);
    if let Some(entry) = removed {
        // 主动断开而不是等 drop：服务器侧立即释放会话（drop 只关本地句柄，
        // TCP 半开时远端要等自己的超时）。失败无妨（连接可能已死）。
        let _ = entry
            .handle
            .disconnect(russh::Disconnect::ByApplication, "", "mcp session pool invalidated")
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 池的连接句柄无法在单测里伪造（需要真 russh 会话），这里只测纯逻辑面：
    // 键管理语义（容量/淘汰/失效）通过 put 的无句柄变体在测试中不便构造，
    // 故池的行为验证依赖真实链路（tauri:dev 手动验收 + CI 集成），此处锁
    // AuthSnapshot 语义——失效路径④的判定基础。

    #[test]
    fn auth_snapshot_detects_credential_change() {
        let mut asset = myshelltool_core::ConnectionAsset {
            id: "a".into(),
            name: "n".into(),
            host: "h".into(),
            port: 22,
            username: "u".into(),
            auth_method: myshelltool_core::AuthMethod::Password,
            private_key_path: None,
            group: "g".into(),
            tags: vec![],
            status: myshelltool_core::ConnectionStatus::Idle,
            last_connected: String::new(),
            credential_id: Some("cred-1".into()),
            passphrase_credential_id: None,
            private_key_credential_id: None,
            jump_host: None,
            connect_timeout_secs: None,
            keepalive_interval_secs: None,
        };
        let snap1 = AuthSnapshot::from_asset(&asset);
        // 改密码凭据引用 → 快照不一致 → 池条目失效
        asset.credential_id = Some("cred-2".into());
        let snap2 = AuthSnapshot::from_asset(&asset);
        assert_ne!(snap1, snap2);
        // 换私钥路径同理
        asset.private_key_path = Some("~/.ssh/new_key".into());
        let snap3 = AuthSnapshot::from_asset(&asset);
        assert_ne!(snap2, snap3);
    }
}
