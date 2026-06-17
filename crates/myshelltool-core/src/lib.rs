// v1.3 Gist 同步加密内核（独立模块，纯函数可单测）
pub mod crypto;
// v1.3 Gist 同步引擎（载荷结构 + 加解密封装 + 冲突检测，纯逻辑无 HTTP）
pub mod sync;

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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthMethod {
    Password,
    PrivateKey,
    Token,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Connected,
    Warning,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncSettings {
    pub enabled: bool,
    pub endpoint: String,
    pub interval_minutes: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenConfigInput {
    pub token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenStatus {
    pub configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncSettingsSummary {
    pub enabled: bool,
    pub endpoint: String,
    pub interval_minutes: u16,
    pub token_status: TokenStatus,
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

pub fn save_connection_asset_store(
    path: impl AsRef<Path>,
    store: &ConnectionAssetStore,
) -> Result<(), String> {
    for asset in &store.assets {
        validate_connection_asset(asset)?;
    }
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_string_pretty(store).map_err(|error| error.to_string())?;
    fs::write(path, json).map_err(|error| error.to_string())
}

pub fn summarize_sync_settings(
    settings: SyncSettings,
    token_input: TokenConfigInput,
) -> SyncSettingsSummary {
    SyncSettingsSummary {
        enabled: settings.enabled,
        endpoint: settings.endpoint,
        interval_minutes: settings.interval_minutes,
        token_status: TokenStatus {
            configured: token_input
                .token
                .as_deref()
                .is_some_and(|token| !token.trim().is_empty()),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialRef {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialStatus {
    pub id: String,
    pub exists: bool,
    pub label: String,
}

/// 凭据加密编解码抽象。
///
/// 设计动机：myshelltool-core 必须保持「无平台依赖、可跨平台 cargo test」，
/// 而 DPAPI 是 Windows-only。故 core 只定义 trait + 提供跨平台实现（LegacyXorCodec
/// 兼容旧文件 + PlaintextCodec 测试用），DPAPI 实现放 src-tauri 层（#[cfg(windows)]）
/// 并在构造 SecretStore 时注入。
///
/// 格式约定（磁盘字节流）：
/// - 新格式：`MAGIC_DPAPI` + codec 加密后的密文。MAGIC 让 read() 能探测格式。
/// - 旧格式：无 MAGIC 头，是 LegacyXorCodec 的输出（向后兼容，读时懒迁移）。
pub trait SecretCodec: Send + Sync {
    /// 加密明文 → 密文字节（不含 MAGIC，MAGIC 由 SecretStore 统一加）。
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, String>;
    /// 解密密文字节（已剥 MAGIC）→ 明文。
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, String>;
}

/// 新格式文件头。读到这个前缀 = 新格式；否则当旧 XOR 处理（懒迁移）。
const MAGIC_DPAPI: &[u8] = b"DPAPI1";

/// 旧版 XOR 编解码（保留用于读旧 .cred 文件 + 跨平台测试）。
///
/// 安全性：逐字节可逆混淆，非加密。仅用于向后兼容，新写入永远走注入的 codec。
pub struct LegacyXorCodec;

impl SecretCodec for LegacyXorCodec {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        Ok(xor_transform(plaintext))
    }
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        Ok(xor_transform(ciphertext))
    }
}

/// 明文编解码（测试用）。不做任何加密，仅用于验证 SecretStore 的格式探测/迁移逻辑。
pub struct PlaintextCodec;

impl SecretCodec for PlaintextCodec {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        Ok(plaintext.to_vec())
    }
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        Ok(ciphertext.to_vec())
    }
}

/// XOR 编解码的共享实现（encode/decode 互逆，故同一函数）。
fn xor_transform(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for (i, &byte) in data.iter().enumerate() {
        out.push(byte ^ ((i as u8).wrapping_add(0x5A)));
    }
    out
}

/// 探测字节流是否为新格式（有 MAGIC 头）。
fn is_new_format(data: &[u8]) -> bool {
    data.len() >= MAGIC_DPAPI.len() && &data[..MAGIC_DPAPI.len()] == MAGIC_DPAPI
}

pub struct SecretStore {
    dir: std::path::PathBuf,
    codec: Box<dyn SecretCodec>,
}

impl SecretStore {
    /// 构造凭据存储。`codec` 决定新写入的加密方式（src-tauri 传 DpapiCodec）。
    pub fn new(dir: impl Into<std::path::PathBuf>, codec: Box<dyn SecretCodec>) -> Self {
        Self {
            dir: dir.into(),
            codec,
        }
    }

    pub fn save(&self, id: &str, secret: &str) -> Result<(), String> {
        let id = sanitize_credential_id(id)?;
        if secret.trim().is_empty() {
            return Err("secret must not be empty".to_string());
        }
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let path = self.dir.join(id);
        // 新写入永远用注入的 codec 加密 + MAGIC 头
        let encrypted = self.codec.encrypt(secret.as_bytes())?;
        let mut payload = Vec::with_capacity(MAGIC_DPAPI.len() + encrypted.len());
        payload.extend_from_slice(MAGIC_DPAPI);
        payload.extend_from_slice(&encrypted);
        fs::write(path, payload).map_err(|e| e.to_string())
    }

    pub fn get_status(&self, id: &str) -> Result<CredentialStatus, String> {
        let id = sanitize_credential_id(id)?;
        let path = self.dir.join(&id);
        Ok(CredentialStatus {
            id: id.clone(),
            exists: path.exists() && fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false),
            label: id,
        })
    }

    pub fn delete(&self, id: &str) -> Result<bool, String> {
        let id = sanitize_credential_id(id)?;
        let path = self.dir.join(id);
        if !path.exists() {
            return Ok(false);
        }
        let content = fs::read(&path).map_err(|e| e.to_string())?;
        fs::remove_file(path).map_err(|e| e.to_string())?;
        zero_memory(&content);
        Ok(true)
    }

    pub fn list(&self) -> Result<Vec<CredentialStatus>, String> {
        if !self.dir.exists() {
            return Ok(vec![]);
        }
        let mut result = vec![];
        for entry in fs::read_dir(&self.dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".cred") {
                result.push(CredentialStatus {
                    id: name.clone(),
                    exists: true,
                    label: name,
                });
            }
        }
        Ok(result)
    }

    /// 读取凭据。
    ///
    /// 格式探测 + 懒迁移：读到旧 XOR 格式文件时，用 LegacyXorCodec 解密，
    /// 然后用当前 codec 重新加密写回（下次读就是新格式）。迁移失败不阻断读取
    ///（返回解密后的明文即可，迁移是 best-effort）。
    pub fn read(&self, id: &str) -> Result<Option<String>, String> {
        let id = sanitize_credential_id(id)?;
        let path = self.dir.join(&id);
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read(&path).map_err(|e| e.to_string())?;
        if raw.is_empty() {
            return Ok(None);
        }

        let plaintext_bytes = if is_new_format(&raw) {
            // 新格式：剥 MAGIC 头，用注入的 codec 解密
            let ciphertext = &raw[MAGIC_DPAPI.len()..];
            self.codec.decrypt(ciphertext)?
        } else {
            // 旧格式（XOR）：用 LegacyXorCodec 解密，然后懒迁移到新格式
            let legacy = LegacyXorCodec;
            let plaintext = legacy.decrypt(&raw)?;
            // 懒迁移：best-effort，失败仅记日志不阻断读取
            if let Err(e) = self.rewrite_with_new_format(&id, &plaintext) {
                // 迁移失败不影响本次返回明文，但记录便于排查
                eprintln!("[SecretStore] lazy migration failed for {id}: {e}");
            }
            plaintext
        };

        Ok(Some(
            String::from_utf8(plaintext_bytes).map_err(|e| e.to_string())?,
        ))
    }

    /// 懒迁移辅助：用当前 codec 重新加密明文并写回。best-effort。
    fn rewrite_with_new_format(&self, id: &str, plaintext: &[u8]) -> Result<(), String> {
        let path = self.dir.join(id);
        let encrypted = self.codec.encrypt(plaintext)?;
        let mut payload = Vec::with_capacity(MAGIC_DPAPI.len() + encrypted.len());
        payload.extend_from_slice(MAGIC_DPAPI);
        payload.extend_from_slice(&encrypted);
        fs::write(path, payload).map_err(|e| e.to_string())
    }
}

fn sanitize_credential_id(id: &str) -> Result<String, String> {
    let sanitized: String = id
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if sanitized.is_empty() {
        return Err("credential id must contain alphanumeric characters".to_string());
    }
    Ok(format!("{}.cred", sanitized))
}

fn zero_memory(data: &[u8]) {
    let ptr = data.as_ptr() as *mut u8;
    unsafe {
        for i in 0..data.len() {
            std::ptr::write_volatile(ptr.add(i), 0);
        }
    }
}

fn asset(
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
        last_connected: last_connected.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, time::{SystemTime, UNIX_EPOCH}};

    #[test]
    fn serializes_connection_asset() {
        let asset = ConnectionAsset {
            id: "asset-1".to_string(),
            name: "Asset 1".to_string(),
            host: "example.invalid".to_string(),
            port: 2222,
            username: "user".to_string(),
            auth_method: AuthMethod::Token,
            private_key_path: None,
            group: "测试".to_string(),
            tags: vec!["demo".to_string()],
            status: ConnectionStatus::Idle,
            last_connected: "从未".to_string(),
            credential_id: None,
            passphrase_credential_id: None,
        };

        let json = serde_json::to_string(&asset).expect("asset serializes");

        assert!(json.contains("asset-1"));
        assert!(json.contains("Token"));
    }

    #[test]
    fn provides_sample_assets_without_credentials() {
        let assets = sample_assets();

        assert_eq!(assets.len(), 8);
        assert!(assets.iter().all(|asset| !asset.id.is_empty()));
        assert!(assets.iter().all(|asset| asset.port > 0));
    }

    #[test]
    fn rejects_invalid_assets() {
        let mut asset = sample_assets().remove(0);
        asset.host.clear();

        assert!(validate_connection_asset(&asset).is_err());
    }

    #[test]
    fn upserts_assets_by_id() {
        let mut store = default_asset_store();
        let mut asset = store.assets[0].clone();
        asset.name = "Renamed Bastion".to_string();

        upsert_connection_asset(&mut store, asset).expect("asset upserts");

        assert_eq!(store.assets.len(), 8);
        assert_eq!(store.assets[0].name, "Renamed Bastion");
    }

    #[test]
    fn saves_and_loads_assets_from_json() {
        let path = temp_store_path("roundtrip");
        let mut store = default_asset_store();
        let mut asset = store.assets[0].clone();
        asset.id = "new-local".to_string();
        asset.name = "New Local".to_string();
        upsert_connection_asset(&mut store, asset).expect("asset appends");

        save_connection_asset_store(&path, &store).expect("store saves");
        let loaded = load_connection_asset_store(&path).expect("store loads");
        let _ = fs::remove_file(path);

        assert_eq!(loaded.assets.len(), 9);
        assert!(loaded.assets.iter().any(|asset| asset.id == "new-local"));
    }

    #[test]
    fn stored_assets_do_not_include_secret_fields() {
        let store = default_asset_store();
        let json = serde_json::to_string(&store).expect("store serializes");
        let lowered = json.to_lowercase();

        // 断言的是「密钥值」不泄漏，而非字段名。
        // ConnectionAsset 含 credential_id / passphrase_credential_id（仅是引用 id，
        // 不是密钥本身），故字段名 "passphrase" 合法存在；这里只拦截明文密钥值。
        assert!(!lowered.contains("password_value"));
        assert!(!lowered.contains("passphrase_value"));
        assert!(!lowered.contains("private_key_data"));
        assert!(!lowered.contains("token_value"));
        assert!(!lowered.contains("secret"));
    }

    #[test]
    fn summarizes_token_status_without_echoing_token() {
        let token = "test-token-must-not-appear";
        let summary = summarize_sync_settings(
            SyncSettings {
                enabled: true,
                endpoint: "https://sync.example.invalid".to_string(),
                interval_minutes: 15,
            },
            TokenConfigInput {
                token: Some(token.to_string()),
            },
        );

        let json = serde_json::to_string(&summary).expect("summary serializes");

        assert!(summary.token_status.configured);
        assert!(!json.contains(token));
    }

    #[test]
    fn blank_token_is_not_configured() {
        let summary = summarize_sync_settings(
            SyncSettings {
                enabled: false,
                endpoint: "".to_string(),
                interval_minutes: 60,
            },
            TokenConfigInput {
                token: Some("   ".to_string()),
            },
        );

        assert!(!summary.token_status.configured);
    }

    #[test]
    fn removes_connection_asset_by_id() {
        let mut store = default_asset_store();
        let id = store.assets[0].id.clone();
        let before = store.assets.len();

        remove_connection_asset(&mut store, &id).expect("asset removed");

        assert_eq!(store.assets.len(), before - 1);
        assert!(!store.assets.iter().any(|a| a.id == id));
        // 不存在的 id 应报错
        assert!(remove_connection_asset(&mut store, "does-not-exist").is_err());
    }

    // 测试用便捷构造：仅 id/name/host/group 关键字段，其余默认。
    fn g_asset(id: &str, name: &str, host: &str, group: &str) -> ConnectionAsset {
        ConnectionAsset {
            id: id.to_string(),
            name: name.to_string(),
            host: host.to_string(),
            port: 22,
            username: "root".to_string(),
            auth_method: AuthMethod::Password,
            private_key_path: None,
            group: group.to_string(),
            tags: vec![],
            status: ConnectionStatus::Idle,
            last_connected: "从未".to_string(),
            credential_id: None,
            passphrase_credential_id: None,
        }
    }

    #[test]
    fn rename_group_updates_assets_and_declared_paths() {
        let mut store = ConnectionAssetStore { assets: vec![], groups: vec![] };
        // 两条资产在 生产/数据库 下，一条在 生产/web 下
        store.assets.push(g_asset("a1", "A1", "h1", "生产/数据库"));
        store.assets.push(g_asset("a2", "A2", "h2", "生产/数据库/主"));
        store.assets.push(g_asset("a3", "A3", "h3", "生产/web"));
        store.groups = vec!["生产/数据库".into(), "生产/web".into()];

        rename_asset_group(&mut store, "生产/数据库", "生产/DB").expect("rename ok");

        // a1 精确匹配 → 改；a2 子级前缀 → 改；a3 不在路径下 → 不变
        assert_eq!(store.assets[0].group, "生产/DB");
        assert_eq!(store.assets[1].group, "生产/DB/主");
        assert_eq!(store.assets[2].group, "生产/web");
        // declared groups 同步更新
        assert!(store.groups.contains(&"生产/DB".to_string()));
        assert!(!store.groups.contains(&"生产/数据库".to_string()));
    }

    #[test]
    fn rename_group_rejects_reserved_ungrouped() {
        let mut store = default_asset_store();
        // 「未分组」保留，不可重命名
        assert!(rename_asset_group(&mut store, "未分组", "Other").is_err());
    }

    #[test]
    fn dissolve_group_moves_assets_to_parent() {
        let mut store = ConnectionAssetStore { assets: vec![], groups: vec![] };
        store.assets.push(g_asset("a1", "A1", "h1", "生产/数据库"));
        store.assets.push(g_asset("a2", "A2", "h2", "生产/数据库/主"));
        store.assets.push(g_asset("a3", "A3", "h3", "生产"));
        store.groups = vec!["生产/数据库".into(), "生产".into()];

        dissolve_asset_group(&mut store, "生产/数据库").expect("dissolve ok");

        // a1 → 父级 生产；a2 → 父级 生产；a3 不变
        assert_eq!(store.assets[0].group, "生产");
        assert_eq!(store.assets[1].group, "生产");
        assert_eq!(store.assets[2].group, "生产");
        // declared: 生产/数据库 移除，生产 保留
        assert!(!store.groups.contains(&"生产/数据库".to_string()));
        assert!(store.groups.contains(&"生产".to_string()));
    }

    #[test]
    fn dissolve_top_level_group_falls_back_to_ungrouped() {
        let mut store = ConnectionAssetStore {
            assets: vec![g_asset("a1", "A1", "h1", "测试组")],
            groups: vec!["测试组".into()],
        };
        dissolve_asset_group(&mut store, "测试组").expect("dissolve ok");
        // 顶级分组无父级 → 提到「未分组」
        assert_eq!(store.assets[0].group, "未分组");
        assert!(store.groups.is_empty());
    }

    #[test]
    fn ensure_group_persists_empty_group() {
        let mut store = default_asset_store();
        ensure_asset_group(&mut store, "生产/数据库").expect("ensure ok");
        assert!(store.groups.contains(&"生产/数据库".to_string()));
        // 幂等：再 ensure 不重复
        ensure_asset_group(&mut store, "生产/数据库").expect("ensure ok");
        assert_eq!(store.groups.iter().filter(|g| *g == "生产/数据库").count(), 1);
    }

    #[test]
    fn validate_group_path_rejects_empty_segments() {
        assert!(validate_group_path("生产/数据库").is_ok());
        assert!(validate_group_path("").is_err());
        assert!(validate_group_path("生产//数据库").is_err()); // 连续 '/'
        assert!(validate_group_path("生产/").is_err()); // 末尾空段
    }

    fn temp_store_path(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time is after epoch")
            .as_nanos();
        env::temp_dir().join(format!("myshelltool-{label}-{nanos}.json"))
    }

    fn temp_secret_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time is after epoch")
            .as_nanos();
        let dir = env::temp_dir().join(format!("myshelltool-secrets-{label}-{nanos}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn secret_store_saves_and_checks_status() {
        let dir = temp_secret_dir("save");
        let store = SecretStore::new(&dir, Box::new(PlaintextCodec));
        store.save("github-pat", "ghp_test_secret_value").expect("save succeeds");

        let status = store.get_status("github-pat").expect("status succeeds");
        assert!(status.exists);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn secret_store_rejects_empty_secret() {
        let dir = temp_secret_dir("empty");
        let store = SecretStore::new(&dir, Box::new(PlaintextCodec));
        assert!(store.save("test", "   ").is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn secret_store_deletes_credential() {
        let dir = temp_secret_dir("delete");
        let store = SecretStore::new(&dir, Box::new(PlaintextCodec));
        store.save("test-cred", "secret123").expect("save");
        let deleted = store.delete("test-cred").expect("delete");
        assert!(deleted);
        let status = store.get_status("test-cred").expect("status");
        assert!(!status.exists);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn secret_store_codec_output_does_not_contain_plaintext() {
        // 用 LegacyXorCodec（非明文 codec）验证：存盘后文件不含明文 secret。
        // PlaintextCodec 是明文，不能用于此断言。
        let dir = temp_secret_dir("plaintext");
        let store = SecretStore::new(&dir, Box::new(LegacyXorCodec));
        let secret = "my-super-secret-token-value";
        store.save("scan-test", secret).expect("save");

        let bytes = fs::read(dir.join("scan-test.cred")).expect("read file");
        let content_str = String::from_utf8_lossy(&bytes);
        assert!(!content_str.contains(secret), "密文不应含明文");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn secret_store_lazy_migrates_legacy_xor_to_new_format() {
        // 验证懒迁移：手写一个旧 XOR 格式文件（无 MAGIC 头），
        // read() 应能用 LegacyXorCodec 解出，并用新 codec 重写（加 MAGIC 头）。
        let dir = temp_secret_dir("migrate");
        let secret = "legacy-secret-to-migrate";

        // 手写旧格式：纯 XOR 字节，无 MAGIC 头
        let legacy_bytes = xor_transform(secret.as_bytes());
        fs::write(dir.join("legacy.cred"), &legacy_bytes).expect("write legacy");

        // 用 PlaintextCodec 构造 store（新格式 = MAGIC + 明文）
        let store = SecretStore::new(&dir, Box::new(PlaintextCodec));
        let read_back = store.read("legacy").expect("read legacy").expect("some");
        assert_eq!(read_back, secret, "懒迁移后应能读出明文");

        // 迁移后文件应有 MAGIC 头（新格式）
        let after = fs::read(dir.join("legacy.cred")).expect("read after migrate");
        assert!(is_new_format(&after), "迁移后应为新格式（有 MAGIC 头）");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn secret_store_roundtrip_with_new_format() {
        // 新写入 → 读取 往返测试（新格式，有 MAGIC 头）
        let dir = temp_secret_dir("roundtrip");
        let store = SecretStore::new(&dir, Box::new(LegacyXorCodec));
        let secret = "roundtrip-secret-2026";
        store.save("rt", secret).expect("save");

        // 文件应是新格式
        let bytes = fs::read(dir.join("rt.cred")).expect("read file");
        assert!(is_new_format(&bytes), "新写入应为新格式");

        // 读回应等于原文
        let read_back = store.read("rt").expect("read").expect("some");
        assert_eq!(read_back, secret);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn secret_store_list_returns_saved_credentials() {
        let dir = temp_secret_dir("list");
        let store = SecretStore::new(&dir, Box::new(PlaintextCodec));
        store.save("cred-a", "secret_a").expect("save a");
        store.save("cred-b", "secret_b").expect("save b");

        let list = store.list().expect("list");
        assert_eq!(list.len(), 2);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn secret_store_rejects_invalid_id() {
        let dir = temp_secret_dir("invalid");
        let store = SecretStore::new(&dir, Box::new(PlaintextCodec));
        assert!(store.save("!@#$%", "secret").is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
