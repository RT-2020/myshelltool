// v2.6：危险命令分类与 shell 分段已迁入 core（crates/myshelltool-core），
// 使 `npm run test:core` 能真正执行这批安全判据的单测（src-tauri 测试二进制
// 受 Tauri runtime DLL 限制在本机跑不起来）。此处 re-export 保持内部调用点
// （`crate::dangerous_commands::*` / `crate::shell::*`）不变——`shell` 只经
// `crate::shell::` 路径引用，无需在本文件按名导入。
pub(crate) use myshelltool_core::dangerous_commands;

mod dpapi_codec;
// v0.20（S7）：FileLogger（5 MiB × 3 滚动）——自 lib.rs 拆出守 800 行 Rust 硬上限
mod file_logger;
/// 出站 HTTP 单例客户端（GitHub Gist / OAuth 共用）+ reqwest 错误链工具。
mod http;
pub(crate) mod fs_local; // format_modified 被 ssh.rs 复用（SFTP mtime → Unix 秒）
// v0.18 内置编辑器的草稿/备份存储（app-data；ssh::text_file 与 fs_local 写命令共用）
pub(crate) mod editor_store;
mod mcp;
mod resource_monitor;
mod ssh;
mod sync;
mod sync_credentials;
mod sync_oauth;
// v0.20（SSH P0-1）：OpenSSH config 导入命令层（解析内核在 core::ssh_config）
mod ssh_config_import;

use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::{Manager, State};
use tokio::sync::Mutex as AsyncMutex;

/// 应用级共享状态。
pub struct AppState {
    pub asset_store_path: PathBuf,
    pub secret_store_dir: PathBuf,
    pub ssh_sessions: Arc<AsyncMutex<ssh::SshSessionManager>>,
    pub resource_monitors: Arc<Mutex<resource_monitor::ResourceMonitorState>>,
    /// v1.4：MCP HTTP server 的 graceful shutdown token。GUI 退出时取消。
    pub mcp_shutdown: tokio_util::sync::CancellationToken,
    /// v1.5：MCP 高危工具的 GUI 弹窗审批 pending 表。
    /// 与 McpToolContext 持有同一 Arc clone（lib.rs setup 时共享），让
    /// server.rs（等待审批）与 mcp_confirm_tool 命令（回传决定）互通。
    pub mcp_approval_pending: mcp::tools::ApprovalPending,
    /// v2：MCP 拦截等级配置。与 McpToolContext 持有同一 Arc clone——
    /// mcp_set_config 更新后，server.rs 已建 HTTP 会话下次调用即生效。
    pub mcp_config: Arc<tokio::sync::RwLock<mcp::config::McpConfig>>,
    /// v2：MCP 数据目录（config / 执行日志 / endpoint 同源，mcp_data_dir() 解析）。
    pub mcp_data_dir: PathBuf,
    /// 跨窗口会话迁移数据中转（tab 拆出独立窗口 / 移回主窗口的 scrollback 等，
    /// TTL 60s，put 时惰性清理）。同进程内存强一致——取代 localStorage 中转
    /// （WebView2 跨窗口 localStorage 非实时共享，新窗口 boot 时读不到源窗口
    /// 刚写入的数据，导致 adopt 误判失败走兜底重连）。
    pub session_handoff: Mutex<std::collections::HashMap<String, HandoffEntry>>,
    /// GitHub Device Flow 登录的进行中会话（单槽：新 start 覆盖旧 start）。
    /// 纯内存，重启即失；见 sync_oauth.rs。
    pub sync_oauth_pending: Mutex<Option<sync_oauth::OAuthSession>>,
    /// v0.20（A1）：MCP HTTP 入口鉴权 token。启动时生成（CSPRNG），与
    /// http_server 的鉴权中间件共享同一 Arc——mcp_reset_token 重置后
    /// 运行中的 server 立即按新 token 校验，无需重启。token 不进日志。
    pub mcp_auth_token: Arc<std::sync::RwLock<String>>,
}

/// 会话迁移条目：at 用于 TTL 判定（60s），payload 为前端协议数据原样透传。
pub struct HandoffEntry {
    at: std::time::Instant,
    payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
struct BackendStatus {
    ready: bool,
    mode: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct ConnectionAssetList {
    source: &'static str,
    count: usize,
    assets: Vec<myshelltool_core::ConnectionAsset>,
    /// 显式声明的分组路径（含空分组）。与 assets 一起同步到前端 declaredGroups。
    groups: Vec<String>,
}

#[tauri::command]
fn backend_status() -> BackendStatus {
    BackendStatus {
        ready: true,
        mode: "tauri-core",
    }
}

#[tauri::command]
fn app_relaunch(app: tauri::AppHandle) {
    app.restart();
}

// ─── 跨窗口会话迁移内存中转（TTL 60s）───

/// 会话迁移 TTL（秒）：迁移数据超过此时长未 take 即作废。
const SESSION_HANDOFF_TTL_SECS: u64 = 60;

/// 写入迁移数据（源窗口调用）。从 payload.sessionId 取键；写入前清理过期条目。
#[tauri::command]
fn session_handoff_put(
    state: State<'_, AppState>,
    payload: serde_json::Value,
) -> Result<(), String> {
    let session_id = payload
        .get("sessionId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "session_handoff_put: payload 缺少 sessionId".to_string())?
        .to_string();
    let mut map = state
        .session_handoff
        .lock()
        .map_err(|_| "session_handoff 锁中毒".to_string())?;
    map.retain(|_, entry| entry.at.elapsed().as_secs() < SESSION_HANDOFF_TTL_SECS);
    map.insert(
        session_id,
        HandoffEntry {
            at: std::time::Instant::now(),
            payload,
        },
    );
    Ok(())
}

/// 原子取出迁移数据（目标窗口调用）：take 即删，防多窗口重复 adopt；过期返 None。
#[tauri::command]
fn session_handoff_take(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<serde_json::Value>, String> {
    let mut map = state
        .session_handoff
        .lock()
        .map_err(|_| "session_handoff 锁中毒".to_string())?;
    match map.remove(&session_id) {
        Some(entry) if entry.at.elapsed().as_secs() < SESSION_HANDOFF_TTL_SECS => {
            Ok(Some(entry.payload))
        }
        _ => Ok(None),
    }
}

#[tauri::command]
fn list_connection_assets(state: State<'_, AppState>) -> Result<ConnectionAssetList, String> {
    let store = myshelltool_core::load_connection_asset_store(&state.asset_store_path)?;
    Ok(ConnectionAssetList {
        source: "local asset store",
        count: store.assets.len(),
        assets: store.assets,
        groups: store.groups,
    })
}

#[tauri::command]
async fn save_connection_asset(
    state: State<'_, AppState>,
    asset: myshelltool_core::ConnectionAsset,
) -> Result<ConnectionAssetList, String> {
    let mut store = myshelltool_core::load_connection_asset_store(&state.asset_store_path)?;
    let asset_id = asset.id.clone();
    myshelltool_core::upsert_connection_asset(&mut store, asset)?;
    myshelltool_core::save_connection_asset_store(&state.asset_store_path, &store)?;
    // v0.20（B3）：主机/凭据字段可能已变——主动失效 MCP 会话池条目
    // （AuthSnapshot 比对是第二道防线，这里让失效即时发生）
    mcp::session_pool::invalidate_asset(&asset_id).await;
    Ok(ConnectionAssetList {
        source: "local asset store",
        count: store.assets.len(),
        assets: store.assets,
        groups: store.groups,
    })
}

#[tauri::command]
async fn delete_connection_asset(state: State<'_, AppState>, id: String) -> Result<ConnectionAssetList, String> {
    let mut store = myshelltool_core::load_connection_asset_store(&state.asset_store_path)?;
    myshelltool_core::remove_connection_asset(&mut store, &id)?;
    myshelltool_core::save_connection_asset_store(&state.asset_store_path, &store)?;
    // v0.20（B3）：资产删除即失效其 MCP 会话池条目（连接主动断开）
    mcp::session_pool::invalidate_asset(&id).await;
    Ok(ConnectionAssetList {
        source: "local asset store",
        count: store.assets.len(),
        assets: store.assets,
        groups: store.groups,
    })
}

#[tauri::command]
fn rename_asset_group(
    state: State<'_, AppState>,
    old_path: String,
    new_path: String,
) -> Result<ConnectionAssetList, String> {
    let mut store = myshelltool_core::load_connection_asset_store(&state.asset_store_path)?;
    myshelltool_core::rename_asset_group(&mut store, &old_path, &new_path)?;
    myshelltool_core::save_connection_asset_store(&state.asset_store_path, &store)?;
    Ok(ConnectionAssetList {
        source: "local asset store",
        count: store.assets.len(),
        assets: store.assets,
        groups: store.groups,
    })
}

#[tauri::command]
fn dissolve_asset_group(state: State<'_, AppState>, path: String) -> Result<ConnectionAssetList, String> {
    let mut store = myshelltool_core::load_connection_asset_store(&state.asset_store_path)?;
    myshelltool_core::dissolve_asset_group(&mut store, &path)?;
    myshelltool_core::save_connection_asset_store(&state.asset_store_path, &store)?;
    Ok(ConnectionAssetList {
        source: "local asset store",
        count: store.assets.len(),
        assets: store.assets,
        groups: store.groups,
    })
}

#[tauri::command]
fn create_asset_group(state: State<'_, AppState>, path: String) -> Result<ConnectionAssetList, String> {
    let mut store = myshelltool_core::load_connection_asset_store(&state.asset_store_path)?;
    myshelltool_core::ensure_asset_group(&mut store, &path)?;
    myshelltool_core::save_connection_asset_store(&state.asset_store_path, &store)?;
    Ok(ConnectionAssetList {
        source: "local asset store",
        count: store.assets.len(),
        assets: store.assets,
        groups: store.groups,
    })
}

#[tauri::command]
fn reorder_asset_groups(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<ConnectionAssetList, String> {
    let mut store = myshelltool_core::load_connection_asset_store(&state.asset_store_path)?;
    myshelltool_core::reorder_asset_groups(&mut store, &paths)?;
    myshelltool_core::save_connection_asset_store(&state.asset_store_path, &store)?;
    Ok(ConnectionAssetList {
        source: "local asset store",
        count: store.assets.len(),
        assets: store.assets,
        groups: store.groups,
    })
}

#[tauri::command]
fn save_credential(
    state: State<'_, AppState>,
    id: String,
    secret: String,
) -> Result<myshelltool_core::CredentialStatus, String> {
    let store = myshelltool_core::SecretStore::new(&state.secret_store_dir, Box::new(dpapi_codec::DpapiCodec));
    store.save(&id, &secret)?;
    store.get_status(&id)
}

#[tauri::command]
fn get_credential_status(
    state: State<'_, AppState>,
    id: String,
) -> Result<myshelltool_core::CredentialStatus, String> {
    let store = myshelltool_core::SecretStore::new(&state.secret_store_dir, Box::new(dpapi_codec::DpapiCodec));
    store.get_status(&id)
}

#[tauri::command]
fn delete_credential(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let store = myshelltool_core::SecretStore::new(&state.secret_store_dir, Box::new(dpapi_codec::DpapiCodec));
    store.delete(&id)
}

// ─── v0.20（N3）：自助诊断信息（崩溃走用户 issue，配 bug_report 模板）───

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticInfo {
    app_version: String,
    /// "windows" + 架构（std::env::consts；OS 内部版本拿不到就不猜）。
    os: String,
    /// debug/release 构建形态（debug 的 MCP 端口/行为与正式版不同，排查必看）。
    build_profile: String,
    data_dir: String,
    /// MCP endpoint 基础 URL（不含 token——诊断信息要进 issue 公开区）。
    mcp_endpoint_base: String,
    /// 应用日志尾部（约 4 KiB）。日志内容本身经 redact 门禁（命令已脱敏）；
    /// 路径/事件名低敏。返回体注明「粘贴前可自查」。
    log_tail: String,
    generated_at: String,
}

#[tauri::command]
fn get_diagnostic_info(state: State<'_, AppState>) -> DiagnosticInfo {
    let log_path = state.mcp_data_dir.join("logs").join("myshelltool.log");
    let log_tail = std::fs::read_to_string(&log_path)
        .map(|content| {
            // 取尾 ~4 KiB，按行边界对齐（不从半截行开始）
            let bytes = content.as_bytes();
            let start = bytes.len().saturating_sub(4 * 1024);
            let start = content[start..]
                .find('\n')
                .map(|i| start + i + 1)
                .unwrap_or(start);
            content[start..].to_string()
        })
        .unwrap_or_else(|e| format!("（读取日志失败: {e}）"));
    let mcp_endpoint_base = mcp::http_server::read_endpoint(&state.mcp_data_dir)
        .map(|e| e.url)
        .unwrap_or_else(|| "(server 未启动)".to_string());
    DiagnosticInfo {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os: format!("{} ({})", std::env::consts::OS, std::env::consts::ARCH),
        build_profile: if cfg!(debug_assertions) { "debug" } else { "release" }.to_string(),
        data_dir: state.mcp_data_dir.to_string_lossy().to_string(),
        mcp_endpoint_base,
        log_tail,
        generated_at: chrono::Local::now().to_rfc3339(),
    }
}


#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let log_path = app_data_dir.join("logs").join("myshelltool.log");
            let logger = file_logger::FileLogger::new(&log_path).expect("failed to create log file");
            log::set_boxed_logger(Box::new(logger))
                .map(|()| log::set_max_level(log::LevelFilter::Info))
                .expect("failed to set logger");
            log::info!("myshelltool starting, data dir: {}", app_data_dir.display());
            let ssh_mgr = Arc::new(AsyncMutex::new(ssh::SshSessionManager::new(
                app.handle().clone(),
                app_data_dir.join("credentials"),
                app_data_dir.join("known_hosts.json"),
            )));

            // Option A：ssh.rs 全部命令统一通过 State<'_, AppState> 解析。
            // 不再需要双 manage hack——参见 .omc/plans/followup-ssh-state-unify.md（已完成）。
            let mcp_shutdown = tokio_util::sync::CancellationToken::new();
            // v1.5：GUI 弹窗审批的 pending 表。一份 Arc，McpToolContext 与 AppState 共享。
            let mcp_approval_pending: mcp::tools::ApprovalPending =
                Arc::new(AsyncMutex::new(std::collections::HashMap::new()));
            // v2：MCP 数据目录与拦截等级配置。data_dir 统一从 mcp_data_dir(&app_data_dir)
            // 取（环境变量优先），让 endpoint / mcp-config.json / mcp-execution-log.json
            // 与 mcp_status 命令读到的目录一致。配置 load 失败（损坏/不存在）→ 默认 Minimal。
            let mcp_dir = mcp_data_dir(&app_data_dir);
            let mcp_config: Arc<tokio::sync::RwLock<mcp::config::McpConfig>> = Arc::new(
                tokio::sync::RwLock::new(mcp::config::load_mcp_config(
                    &mcp::config::mcp_config_path(&mcp_dir),
                )),
            );
            // v0.20（A1）：MCP 入口鉴权 token——启动时生成，AppState 与 HTTP
            // 中间件共享同一 Arc（重置即生效，无需重启 server）。
            let mcp_auth_token: Arc<std::sync::RwLock<String>> =
                Arc::new(std::sync::RwLock::new(mcp::http_server::generate_token()));
            app.manage(AppState {
                asset_store_path: app_data_dir.join("connection-assets.json"),
                secret_store_dir: app_data_dir.join("credentials"),
                ssh_sessions: ssh_mgr.clone(),
                resource_monitors: Arc::new(Mutex::new(resource_monitor::ResourceMonitorState::default())),
                mcp_shutdown: mcp_shutdown.clone(),
                mcp_approval_pending: mcp_approval_pending.clone(),
                mcp_config: mcp_config.clone(),
                mcp_data_dir: mcp_dir.clone(),
                session_handoff: Mutex::new(std::collections::HashMap::new()),
                sync_oauth_pending: Mutex::new(None),
                mcp_auth_token: mcp_auth_token.clone(),
            });

            // v1.4：启动 MCP Streamable HTTP server（内嵌 GUI 进程）。
            // 取代 v1.1 的 named pipe server —— MCP server 不再是独立 exe，
            // 直接跑在 GUI 进程内，用 HTTP transport 对外暴露。任何合规 MCP host
            // 经 http://127.0.0.1:<port>/mcp 连入。SSH 会话/资产/审批同进程直接访问。
            //
            // v1.5：McpToolContext 用 new_with_gui 注入 AppHandle + 共享 pending 表，
            // 让 server.rs 的降级路径能 emit GUI 弹窗审批事件。
            // v2：追加注入共享 config Arc（拦截等级）与 mcp_dir（日志/配置落盘）。
            //
            // 注意：setup hook 是同步上下文，此时 Tokio runtime 尚未在当前线程
            // 就绪——裸 `tokio::spawn` 会 panic「no reactor running」。
            // 必须用 `tauri::async_runtime::spawn`，它会绑定到 Tauri 管理的 runtime。
            let mcp_ctx = mcp::tools::McpToolContext::new_with_gui(
                app.handle().clone(),
                mcp_approval_pending,
                app_data_dir.join("connection-assets.json"),
                app_data_dir.join("credentials"),
                app_data_dir.join("known_hosts.json"),
                mcp_config,
                mcp_dir.clone(),
                // v0.20（B3）：注入 GUI 会话管理器——exec 工具复用优先级的第一层
                ssh_mgr,
            );
            tauri::async_runtime::spawn(mcp::http_server::run_http_server(
                mcp_ctx,
                mcp_dir,
                mcp_shutdown,
                mcp_auth_token,
            ));
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            Ok(())
        })
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            backend_status,
            app_relaunch,
            // v1.2：MCP 服务可观测性聚合查询（前端状态栏/管理面板）
            mcp::commands::mcp_status,
            // v1.5：MCP 高危工具 GUI 弹窗审批的用户回传命令
            mcp::commands::mcp_confirm_tool,
            // v0.20：MCP 入口 token 重置（A1 鉴权）
            mcp::commands::mcp_reset_token,
            // v2：MCP 拦截等级配置 + 执行日志（前端 MCP 面板）
            mcp::commands::mcp_get_config,
            mcp::commands::mcp_set_config,
            // v0.20（B1 GUI）：授权范围编辑保存
            mcp::commands::mcp_set_scope,
            mcp::commands::mcp_list_execution_logs,
            mcp::commands::mcp_clear_execution_logs,
            // 跨窗口会话迁移内存中转（TTL 60s）
            session_handoff_put,
            session_handoff_take,
            list_connection_assets,
            // v0.20（N3）：自助诊断（issue 模板引导粘贴）
            get_diagnostic_info,
            // v0.20（SSH P0-1）：OpenSSH config 导入预览（导入执行复用 save_connection_asset）
            ssh_config_import::import_ssh_config_preview,
            save_connection_asset,
            delete_connection_asset,
            rename_asset_group,
            dissolve_asset_group,
            create_asset_group,
            reorder_asset_groups,
            // v1.3：Gist 同步（替代 v1.0 死命令 save_sync_settings）
            sync::sync_status,
            sync::sync_setup,
            sync::sync_push,
            sync::sync_pull,
            sync::sync_resolve_conflict,
            sync::sync_reset_master_password,
            sync::sync_clear,
            // v1.6：自动同步（会话密钥 + DPAPI 保护）
            sync::sync_enable_auto_sync,
            sync::sync_disable_auto_sync,
            sync::sync_check_remote_updates,
            sync::sync_set_credentials_enabled,
            // 备份发现（换机恢复免填 Gist ID：按文件名标记列出账号下的备份候选）
            sync::sync_discover_gists,
            // v2.7：恢复密码（应用生成高熵密码 → DPAPI 保存 → 换机时可查看/复制带走）
            sync::sync_generate_recovery_password,
            sync::sync_reveal_recovery_password,
            // GitHub Device Flow 登录（替代手动粘贴 PAT；provider 参数预留多服务）
            sync_oauth::sync_oauth_start,
            sync_oauth::sync_oauth_poll,
            sync_oauth::sync_oauth_cancel,
            save_credential,
            get_credential_status,
            delete_credential,
            ssh::ssh_connect,
            ssh::session_cmds::ssh_write,
            ssh::session_cmds::ssh_resize,
            ssh::ssh_disconnect,
            ssh::ssh_confirm_host_key,
            ssh::ssh_keyboard_response,
            ssh::sftp_list_dir,
            ssh::sftp_read_file,
            ssh::sftp_write_file,
            ssh::sftp_upload_from_file,
            ssh::sftp_upload_cancel,
            ssh::sftp_download_cancel,
            ssh::sftp_download_to_file,
            ssh::sftp_mkdir,
            ssh::sftp_chmod,
            ssh::sftp_readlink,
            ssh::sftp_rename,
            ssh::sftp_remove,
            ssh::sftp_stat,
            // v0.18 内置编辑器：远端文本读写（2MiB 上限/二进制嗅探/编码白名单/冲突检测/原子写）
            ssh::sftp_read_text,
            ssh::sftp_write_text,
            ssh::tunnel_create,
            ssh::tunnel_start,
            ssh::tunnel_stop,
            ssh::tunnel_list,
            ssh::tunnel_delete,
            fs_local::fs_local_home_dir,
            fs_local::fs_local_list_dir,
            fs_local::fs_local_mkdir,
            fs_local::fs_local_delete,
            fs_local::fs_local_rename,
            fs_local::fs_local_stat,
            // v0.18 内置编辑器：本地文本读写（同远端链路的防线与冲突语义）
            fs_local::fs_local_read_text,
            fs_local::fs_local_write_text,
            // v0.18 内置编辑器：备份/草稿（app-data 滚动存储）
            editor_store::editor_backup_list,
            editor_store::editor_backup_read,
            editor_store::editor_draft_save,
            editor_store::editor_draft_get,
            editor_store::editor_draft_delete,
            resource_monitor::resource_monitor_start,
            resource_monitor::resource_monitor_stop,
            resource_monitor::resource_monitor_snapshot,
            resource_monitor::resource_monitor_list_active
        ])
        .run(tauri::generate_context!())
        .expect("failed to run myshelltool");
}

// ─── MCP 数据目录解析（v1.4：MCP 内嵌 GUI，此函数供 setup 解析用）───

/// 解析 MCP 数据目录。
///
/// 优先级：
/// 1. 环境变量 `MYSHELLTOOL_DATA_DIR`（测试/自定义数据目录时可显式指定）
/// 2. 调用方传入的 Tauri `app_data_dir`（setup 中经 `app.path().app_data_dir()`
///    解析，与资产/凭据同源）
///
/// v2.5：删除 `%APPDATA%+硬编码 identifier` 重建分支——它与此处的 app_data_dir
/// 是两套来源，APPDATA 缺失时会落到相对 CWD `"."` 写配置，identifier 改名时
/// 两者分叉。单一来源：setup 解析一次，经 AppState.mcp_data_dir 全程复用
/// （mcp_status 命令直接读 state，不再重复解析）。
/// v2.6：环境变量值必须过三关——**区分「未设置」与「空串/纯空白」**（`var_os`，
/// 空值不算配置：`Path::new("").join(..)` 会落成相对 CWD 路径，正是 v2.5 删掉
/// `%APPDATA%` 重建分支要防的形态）、**必须是绝对路径**（相对值锚在进程 CWD，
/// 重启后路径随快捷方式的「起始位置」漂移，mcp-config.json 找不到 → 拦截等级
/// 静默回落默认档）、非 UTF-8 值不当作「未设置」丢弃（`var` 对非 UTF-8 直接 Err，
/// 会把用户显式配置误判成缺失）。不满足即记 warn 后回退 app_data_dir——本函数
/// 在 setup 里解析一次即写入 AppState，没有可承接 Err 的调用方（失败会让 GUI
/// 起不来），而 app_data_dir 是稳定且确定的来源，回退只是忽略一个无效覆盖值，
/// 不会写错位置，warn 保证用户看得见。
fn mcp_data_dir(app_data: &std::path::Path) -> std::path::PathBuf {
    match std::env::var_os("MYSHELLTOOL_DATA_DIR") {
        // 纯空白（"   "）与空串同待遇：都不是可用的目录值，按未设置回退
        Some(v) if !v.to_string_lossy().trim().is_empty() => {
            let dir = std::path::PathBuf::from(&v);
            if dir.is_absolute() {
                return dir;
            }
            log::warn!(
                "MYSHELLTOOL_DATA_DIR 不是绝对路径（{:?}），已忽略并回退 app_data_dir——相对路径会随进程工作目录漂移",
                dir
            );
        }
        Some(_) => log::warn!("MYSHELLTOOL_DATA_DIR 存在但为空/纯空白，已忽略并回退 app_data_dir"),
        None => {}
    }
    app_data.to_path_buf()
}
