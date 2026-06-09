use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionAsset {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: AuthMethod,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthMethod {
    Password,
    PrivateKey,
    Token,
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

pub fn sample_assets() -> Vec<ConnectionAsset> {
    vec![
        ConnectionAsset {
            id: "local-demo".to_string(),
            name: "Local Demo".to_string(),
            host: "127.0.0.1".to_string(),
            port: 22,
            username: "demo".to_string(),
            auth_method: AuthMethod::Password,
        },
        ConnectionAsset {
            id: "staging-demo".to_string(),
            name: "Staging Demo".to_string(),
            host: "staging.example.invalid".to_string(),
            port: 22,
            username: "deploy".to_string(),
            auth_method: AuthMethod::PrivateKey,
        },
    ]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_connection_asset() {
        let asset = ConnectionAsset {
            id: "asset-1".to_string(),
            name: "Asset 1".to_string(),
            host: "example.invalid".to_string(),
            port: 2222,
            username: "user".to_string(),
            auth_method: AuthMethod::Token,
        };

        let json = serde_json::to_string(&asset).expect("asset serializes");

        assert!(json.contains("asset-1"));
        assert!(json.contains("Token"));
    }

    #[test]
    fn provides_sample_assets_without_credentials() {
        let assets = sample_assets();

        assert_eq!(assets.len(), 2);
        assert!(assets.iter().all(|asset| !asset.id.is_empty()));
        assert!(assets.iter().all(|asset| asset.port > 0));
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
}
