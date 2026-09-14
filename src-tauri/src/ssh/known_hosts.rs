//! known_hosts 信任记录的加载/保存（GUI 与 headless 连接共用）。
//! 从 ssh.rs 按域拆出（architecture-log Target 1），零逻辑变更。

use super::*;

// --- known_hosts helpers ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownHostEntry {
    pub key_type: String,
    pub key_hex: String,
}

pub fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn load_known_hosts(path: &PathBuf) -> HashMap<String, KnownHostEntry> {
    if !path.exists() {
        return HashMap::new();
    }
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_known_hosts(path: &PathBuf, hosts: &HashMap<String, KnownHostEntry>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(hosts).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}
