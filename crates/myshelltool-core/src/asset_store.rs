//! 连接资产域：ConnectionAsset/存储结构/校验/分组操作/加载与原子写盘
//! （从 lib.rs 按域拆出，v2.8 第四轮）。write_atomic 是全仓原子写的单一实现
//! （配置/凭据/同步状态/执行日志共用），放本域并经 lib 根再导出。

use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ConnectionAsset {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: AuthMethod,
    #[serde(default)]
    pub private_key_path: Option<String>,
    pub group: String,
    pub tags: Vec<String>,
    pub status: ConnectionStatus,
    pub last_connected: String,
    /// Password 模式下，本地安全存储中的密码引用 ID（如 "<asset_id>:password"）
    #[serde(default, alias = "credentialId")]
    pub credential_id: Option<String>,
    /// PrivateKey 模式下，passphrase 的本地安全存储引用 ID
    #[serde(default, alias = "passphraseCredentialId")]
    pub passphrase_credential_id: Option<String>,
    /// PrivateKey 模式下，托管在 SecretStore 中的私钥内容引用 ID（如 "<asset_id>:private_key"）
    #[serde(default, alias = "privateKeyCredentialId")]
    pub private_key_credential_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthMethod {
    #[serde(alias = "password")]
    Password,
    #[serde(alias = "private_key", alias = "privateKey")]
    PrivateKey,
    #[serde(alias = "token")]
    Token,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    #[serde(alias = "connected")]
    Connected,
    #[serde(alias = "warning")]
    Warning,
    #[serde(alias = "idle")]
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionAssetStore {
    pub assets: Vec<ConnectionAsset>,
    /// 显式声明的分组路径（含空分组）。`asset.group` 仅存路径，刷新后无法
    /// 还原无资产的分组，故用一个独立列表持久化。`#[serde(default)]` 兼容旧文件。
    #[serde(default)]
    pub groups: Vec<String>,
}

pub fn default_asset_store() -> ConnectionAssetStore {
    ConnectionAssetStore {
        assets: vec![
            asset(
                "prod-bastion",
                "prod-bastion",
                "10.10.4.8",
                "root",
                "收藏",
                &["favorite", "ProxyJump"],
                ConnectionStatus::Connected,
                "15 分钟前",
                AuthMethod::PrivateKey,
            ),
            asset(
                "web-01",
                "web-01",
                "10.10.8.21",
                "deploy",
                "收藏",
                &["web", "release"],
                ConnectionStatus::Connected,
                "1 小时前",
                AuthMethod::PrivateKey,
            ),
            asset(
                "db-readonly",
                "db-readonly",
                "10.10.9.32",
                "audit",
                "收藏",
                &["db", "readonly"],
                ConnectionStatus::Warning,
                "昨天",
                AuthMethod::Password,
            ),
            asset(
                "app-cluster-01",
                "app-cluster-01",
                "172.18.1.44",
                "ubuntu",
                "生产环境",
                &["prod", "app"],
                ConnectionStatus::Connected,
                "今天",
                AuthMethod::PrivateKey,
            ),
            asset(
                "cache-redis-02",
                "cache-redis-02",
                "172.18.2.19",
                "redis",
                "生产环境",
                &["redis", "idle"],
                ConnectionStatus::Idle,
                "3 天前",
                AuthMethod::Password,
            ),
            asset(
                "ops-jump-gateway",
                "ops-jump-gateway",
                "172.18.0.10",
                "ops",
                "生产环境",
                &["jump", "proxy"],
                ConnectionStatus::Connected,
                "刚刚",
                AuthMethod::PrivateKey,
            ),
            asset(
                "lab-windows-dev",
                "lab-windows-dev",
                "192.168.31.70",
                "administrator",
                "最近连接",
                &["win", "dev"],
                ConnectionStatus::Connected,
                "刚刚",
                AuthMethod::Password,
            ),
            asset(
                "nas-backup",
                "nas-backup",
                "192.168.31.9",
                "backup",
                "最近连接",
                &["sftp", "backup"],
                ConnectionStatus::Warning,
                "昨天",
                AuthMethod::PrivateKey,
            ),
        ],
        groups: vec![],
    }
}

pub fn sample_assets() -> Vec<ConnectionAsset> {
    default_asset_store().assets
}

pub fn validate_connection_asset(asset: &ConnectionAsset) -> Result<(), String> {
    if asset.id.trim().is_empty() {
        return Err("asset id is required".to_string());
    }
    if asset.name.trim().is_empty() {
        return Err("asset name is required".to_string());
    }
    if asset.host.trim().is_empty() {
        return Err("asset host is required".to_string());
    }
    if asset.username.trim().is_empty() {
        return Err("asset username is required".to_string());
    }
    if asset.port == 0 {
        return Err("asset port must be between 1 and 65535".to_string());
    }
    Ok(())
}

pub fn upsert_connection_asset(
    store: &mut ConnectionAssetStore,
    asset: ConnectionAsset,
) -> Result<(), String> {
    validate_connection_asset(&asset)?;
    if let Some(existing) = store.assets.iter_mut().find(|item| item.id == asset.id) {
        *existing = asset;
    } else {
        store.assets.push(asset);
    }
    Ok(())
}

/// 校验分组路径：各段非空且不含 `/`（分隔符保留给层级）。
/// 允许空字符串（表示未命名，调用方应已处理），但不允许段内出现 `/`。
pub fn validate_group_path(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("group path is empty".to_string());
    }
    for segment in trimmed.split('/') {
        if segment.is_empty() {
            return Err("group path has empty segment (consecutive '/')".to_string());
        }
    }
    Ok(())
}

/// 删除指定 id 的连接资产。不存在则报错。
pub fn remove_connection_asset(store: &mut ConnectionAssetStore, id: &str) -> Result<(), String> {
    let before = store.assets.len();
    store.assets.retain(|item| item.id != id);
    if store.assets.len() == before {
        return Err(format!("asset id not found: {}", id));
    }
    Ok(())
}

/// 重命名分组路径（含其所有子级）。`old_path` 与 `new_path` 均为完整路径，
/// 如 "生产/数据库" → "生产/DB"。同步更新 assets.group 与 store.groups。
pub fn rename_asset_group(
    store: &mut ConnectionAssetStore,
    old_path: &str,
    new_path: &str,
) -> Result<(), String> {
    let old_path = old_path.trim();
    let new_path = new_path.trim();
    if old_path.is_empty() {
        return Err("old group path is empty".to_string());
    }
    if old_path == "未分组" {
        return Err("cannot rename the reserved '未分组' group".to_string());
    }
    validate_group_path(new_path)?;
    if new_path == old_path {
        return Ok(()); // no-op
    }

    let prefix = format!("{}/", old_path);
    // 更新 assets.group：精确匹配或前缀匹配（子级路径）
    for asset in store.assets.iter_mut() {
        if asset.group == old_path {
            asset.group = new_path.to_string();
        } else if asset.group.starts_with(&prefix) {
            asset.group = format!("{}{}", new_path, &asset.group[old_path.len()..]);
        }
    }
    // 同步 store.groups
    for g in store.groups.iter_mut() {
        if *g == old_path {
            *g = new_path.to_string();
        } else if g.starts_with(&prefix) {
            *g = format!("{}{}", new_path, &g[old_path.len()..]);
        }
    }
    // 去重（重命名可能产生重复声明）
    store.groups.sort();
    store.groups.dedup();
    Ok(())
}

/// 解散分组：将其下所有 asset 提到父级（无父级则「未分组」），
/// 并从 store.groups 移除该路径及其所有子级声明。
pub fn dissolve_asset_group(
    store: &mut ConnectionAssetStore,
    path: &str,
) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("group path is empty".to_string());
    }
    if path == "未分组" {
        return Err("cannot dissolve the reserved '未分组' group".to_string());
    }

    // 父级 = 最后一个 '/' 之前的部分；无 '/' 则提到「未分组」
    let parent = match path.rfind('/') {
        Some(idx) => &path[..idx],
        None => "未分组",
    };

    let prefix = format!("{}/", path);
    for asset in store.assets.iter_mut() {
        if asset.group == path || asset.group.starts_with(&prefix) {
            asset.group = parent.to_string();
        }
    }
    // 移除 path 及其所有子级声明
    store.groups.retain(|g| *g != path && !g.starts_with(&prefix));
    Ok(())
}

/// 声明一个分组路径（支撑新建空分组）。已存在则 no-op。
pub fn ensure_asset_group(store: &mut ConnectionAssetStore, path: &str) -> Result<(), String> {
    let path = path.trim();
    validate_group_path(path)?;
    if !store.groups.iter().any(|g| g == path) {
        store.groups.push(path.to_string());
    }
    Ok(())
}

/// 重排分组声明顺序（支撑拖拽排序）。`ordered_paths` 必须是当前 store.groups 的一个排列：
/// 集合相等才接受新顺序，否则报错（防止前端漏传导致丢分组）。
/// 顺序即 `store.groups` 数组顺序——buildGroupTree 在前端按此顺序渲染。
/// 保留节点「未分组」不在 declaredGroups 内，传入也不应出现（出现会被忽略）。
pub fn reorder_asset_groups(store: &mut ConnectionAssetStore, ordered_paths: &[String]) -> Result<(), String> {
    let new_list: Vec<String> = ordered_paths
        .iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty() && p != "未分组")
        .collect();

    // 集合校验：新列表必须是当前 groups 的一个排列（允许两者都为空）。
    let mut cur_sorted: Vec<String> = store.groups.iter().cloned().collect();
    cur_sorted.sort();
    let mut new_sorted: Vec<String> = new_list.clone();
    new_sorted.sort();
    if cur_sorted != new_sorted {
        return Err(format!(
            "reorder_asset_groups: ordered_paths is not a permutation of current groups (got {}, expected {})",
            new_sorted.len(),
            cur_sorted.len()
        ));
    }

    store.groups = new_list;
    Ok(())
}

pub fn load_connection_asset_store(path: impl AsRef<Path>) -> Result<ConnectionAssetStore, String> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(default_asset_store());
    }
    let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let store: ConnectionAssetStore = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
    for asset in &store.assets {
        validate_connection_asset(asset)?;
    }
    Ok(store)
}

/// 原子写：先写**同目录**临时文件再 `fs::rename` 替换目标。
///
/// 为什么必须：`fs::write` 语义是 truncate + write_all，就地覆盖。磁盘满、断电、
/// 进程被杀、AV 扫描的瞬间会留下半截或 0 字节文件——`connection-assets.json` 是
/// 用户资产的**本地唯一副本**，半截后 `load_connection_asset_store` 直接 Err，
/// 整个资产列表不可读；`sync-state.json` 截成 0 字节还会被当「首次运行」静默清零。
/// rename 在同一文件系统内是原子的：读方只会看到「完整旧文件」或「完整新文件」。
///
/// 失败语义（fail-secure）：临时文件写失败或 rename 失败都返回 Err 并**保留原文件**；
/// 失败路径尽力清理临时文件（清理失败无副作用，故可忽略）。
///
/// 临时文件带进程 id + 计数后缀：多实例/多线程并发写同一目标时不会互相覆盖对方的
/// 临时文件（用固定 `.tmp` 名会）。
pub fn write_atomic(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<(), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);

    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录 {}: {e}", parent.display()))?;
        }
    }
    let tmp_path = {
        let mut s = path.as_os_str().to_os_string();
        s.push(format!(
            ".tmp-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::path::PathBuf::from(s)
    };
    fs::write(&tmp_path, contents.as_ref())
        .map_err(|e| format!("写临时文件 {}: {e}", tmp_path.display()))?;
    fs::rename(&tmp_path, path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path); // best-effort 清理，失败无副作用
        format!("原子替换 {} 失败（原文件保留）: {e}", path.display())
    })
}

pub fn save_connection_asset_store(
    path: impl AsRef<Path>,
    store: &ConnectionAssetStore,
) -> Result<(), String> {
    for asset in &store.assets {
        validate_connection_asset(asset)?;
    }
    let json = serde_json::to_string_pretty(store).map_err(|error| error.to_string())?;
    // 原子写：资产 JSON 是本地唯一副本，截断即全部资产不可读（见 write_atomic 注释）
    write_atomic(path, json)
}

/// 测试与示例数据的构造助手（default_asset_store/sample_assets 与 lib_tests 共用）。
pub(crate) fn asset(
    id: &str,
    name: &str,
    host: &str,
    username: &str,
    group: &str,
    tags: &[&str],
    status: ConnectionStatus,
    last_connected: &str,
    auth_method: AuthMethod,
) -> ConnectionAsset {
    ConnectionAsset {
        id: id.to_string(),
        name: name.to_string(),
        host: host.to_string(),
        port: 22,
        username: username.to_string(),
        auth_method,
        private_key_path: None,
        group: group.to_string(),
        tags: tags.iter().map(|tag| tag.to_string()).collect(),
        status,
        credential_id: None,
        passphrase_credential_id: None,
        private_key_credential_id: None,
        last_connected: last_connected.to_string(),
    }
}
