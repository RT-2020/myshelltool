//! OpenSSH config 导入命令层（v0.20，SSH P0-1）。
//!
//! 从 lib.rs 拆出（lib.rs 贴近 800 行 Rust 硬上限）：命令 + DTO 在本模块，
//! 解析内核在 `myshelltool_core::ssh_config`（单测真跑），路径展开复用
//! `ssh::expand_home_path`（与连接链路同源）。

use serde::Serialize;
use tauri::State;

use crate::AppState;

/// 预览候选：解析 config → 与现有资产按 host:port:username 比对冲突。
/// 不写库——导入执行走既有 save_connection_asset（前端逐条提交，经过
/// 与手工建资产完全相同的校验/ID 生成链路）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SshImportCandidate {
    alias: String,
    host: String,
    port: u16,
    username: Option<String>,
    identity_file: Option<String>,
    proxy_jump: Option<String>,
    /// 与现有资产冲突（同 host:port:username）时的资产 id。
    conflict_existing_id: Option<String>,
    conflict_existing_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshImportPreview {
    source_path: String,
    candidates: Vec<SshImportCandidate>,
}

/// `import_ssh_config_preview({ path? })`：path 缺省 = ~/.ssh/config。
#[tauri::command]
pub fn import_ssh_config_preview(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<SshImportPreview, String> {
    let raw = path.unwrap_or_else(|| "~/.ssh/config".to_string());
    let resolved = crate::ssh::expand_home_path(&raw)
        .map_err(|e| format!("解析配置路径失败（{raw}）: {e}"))?;
    let text = std::fs::read_to_string(&resolved)
        .map_err(|e| format!("读取 {resolved} 失败: {e}。若文件在其他位置，请用「选择文件」指定"))?;

    let hosts = myshelltool_core::ssh_config::parse_ssh_config(&text)
        .map_err(|e| format!("解析 {} 失败: {}", resolved, e.0))?;

    // 冲突比对：同 host:port:username 视为同一台机器（用户名缺省按当前 Windows
    // 用户名近似——OpenSSH 的默认 User 语义就是本地用户名，导入预览如实展示）
    let store = myshelltool_core::load_connection_asset_store(&state.asset_store_path)?;
    let local_user = local_username();
    let candidates = hosts
        .into_iter()
        .map(|h| {
            let username = h.username.clone().unwrap_or_else(|| local_user.clone());
            let conflict = store
                .assets
                .iter()
                .find(|a| a.host == h.host && a.port == h.port && a.username == username);
            SshImportCandidate {
                alias: h.alias,
                host: h.host,
                port: h.port,
                username: Some(username),
                identity_file: h.identity_file,
                proxy_jump: h.proxy_jump,
                conflict_existing_id: conflict.map(|a| a.id.clone()),
                conflict_existing_name: conflict.map(|a| a.name.clone()),
            }
        })
        .collect();

    Ok(SshImportPreview {
        source_path: resolved,
        candidates,
    })
}

/// 本地用户名（OpenSSH 缺省 User 语义）。取不到时给空串——预览会显示
/// 「用户名: (空)」，用户可在导入后编辑；不猜常见名。
fn local_username() -> String {
    std::env::var("USERNAME").unwrap_or_default()
}
