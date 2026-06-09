use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionAsset {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: AuthMethod,
    pub group: String,
    pub tags: Vec<String>,
    pub status: ConnectionStatus,
    pub last_connected: String,
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
        group: group.to_string(),
        tags: tags.iter().map(|tag| tag.to_string()).collect(),
        status,
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
            group: "测试".to_string(),
            tags: vec!["demo".to_string()],
            status: ConnectionStatus::Idle,
            last_connected: "从未".to_string(),
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

        assert!(!lowered.contains("password_value"));
        assert!(!lowered.contains("passphrase"));
        assert!(!lowered.contains("private_key"));
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

    fn temp_store_path(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time is after epoch")
            .as_nanos();
        env::temp_dir().join(format!("myshelltool-{label}-{nanos}.json"))
    }
}
