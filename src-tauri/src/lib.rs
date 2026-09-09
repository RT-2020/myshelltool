mod dangerous_commands;
mod dpapi_codec;
pub(crate) mod fs_local; // format_modified 被 ssh.rs 复用（SFTP mtime → Unix 秒）
mod mcp;
mod resource_monitor;
mod ssh;
mod sync;
mod sync_credentials;
mod sync_oauth;

use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};
use tokio::sync::Mutex as AsyncMutex;

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

// ─── v1.2：MCP 服务可观测性（前端状态栏/管理面板的聚合查询）───
//
// 把 pipe server 维护的连接状态 + MCP server 的静态能力声明（tools/resources/prompts）
// + 数据目录路径聚合成一个 DTO，供前端 mcp store 一次性拉取。
//
// 工具的「只读/高危」标记：rmcp 协议层 annotations 当前全为 None（见 tools.rs），
// 这里在 Rust 端按 approval.rs 的判定语义补上 tag 字段，让前端无需硬编码映射表。
// approval.rs 的真实运行时判定逻辑不变，这里只是给 UI 展示用的静态标签。

/// MCP 工具/UI 条目（精简 DTO，避免直接序列化 rmcp 复杂类型）。
#[derive(Debug, Clone, Serialize)]
struct McpToolInfo {
    name: String,
    description: String,
    /// "readonly" | "dangerous"
    tag: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct McpResourceInfo {
    uri: String,
    name: String,
    is_template: bool,
}

#[derive(Debug, Clone, Serialize)]
struct McpPromptInfo {
    name: String,
    description: String,
    arguments: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct McpStatus {
    /// MCP server 协议层名字（静态声明，与 rmcp get_info 一致）。
    server_name: &'static str,
    /// GUI 自身版本（Cargo 包版本，与 tauri.conf.json 对齐）。
    server_version: &'static str,
    /// v1.4：MCP HTTP endpoint URL（供用户配置 MCP host）。
    /// v1.3 是 pipe_name（named pipe 路径），内嵌后改为 HTTP URL。
    endpoint: String,
    /// MCP 进程读写的数据目录（GUI 与 MCP 共享同一份资产/凭据）。
    data_dir: String,
    /// v1.4：MCP 就绪探测结果（HTTP 健康检查）。这是状态灯的唯一信号源——
    /// 向自己的 HTTP endpoint 发 initialize 握手，回答「能否正常工作」。
    /// 不再 spawn 子进程（v1.2 的一次性 spawn 已废弃）。
    probe: mcp::probe::McpProbeResult,
    /// MCP 暴露的 9 个工具（7 只读 + 2 高危）。
    tools: Vec<McpToolInfo>,
    /// 3 静态资源 + 1 template。
    resources: Vec<McpResourceInfo>,
    /// 3 个诊断 prompt。
    prompts: Vec<McpPromptInfo>,
}

/// 工具名 → 静态标签。语义镜像 approval.rs 的判定：
/// - ssh_exec：白名单只读放行，其余高危
/// - sftp_remove：恒高危
/// - 其余 7 个：纯只读
fn tool_tag(name: &str) -> &'static str {
    match name {
        "ssh_exec" | "sftp_remove" => "dangerous",
        _ => "readonly",
    }
}

#[tauri::command]
async fn mcp_status(_state: State<'_, AppState>) -> Result<McpStatus, String> {
    let data_dir = mcp_data_dir();

    // v1.4：HTTP 健康检查。读 mcp-endpoint.json 拿实际监听地址，向它发 initialize。
    // 不再 spawn 子进程（v1.2 的 probe_mcp 已废弃）。
    let endpoint = mcp::http_server::read_endpoint(&data_dir)
        .map(|e| e.url)
        .unwrap_or_default();
    let probe = if endpoint.is_empty() {
        mcp::probe::fail_no_endpoint(&chrono::Utc::now().to_rfc3339())
    } else {
        mcp::probe::probe_endpoint(&endpoint).await
    };

    // 静态能力声明：直接复用 mcp 模块的 schema 构造函数，保证与协议实际暴露一致。
    let tools: Vec<McpToolInfo> = mcp::tools::list_all_tools()
        .into_iter()
        .map(|t| {
            // rmcp Tool 的 name/description 是 Cow<'_, str>（非 String），
            // annotations 当前无（None）。用静态 tag 表补充只读/高危标记。
            let tag = tool_tag(&t.name);
            McpToolInfo {
                name: t.name.to_string(),
                description: t.description.map(|d| d.to_string()).unwrap_or_default(),
                tag,
            }
        })
        .collect();
    let resources: Vec<McpResourceInfo> = mcp::resources::list_resources()
        .into_iter()
        .map(|r| McpResourceInfo {
            uri: r.uri.to_string(),
            name: r.name.clone(),
            is_template: false,
        })
        .chain(
            mcp::resources::list_resource_templates()
                .into_iter()
                .map(|t| McpResourceInfo {
                    uri: t.uri_template.to_string(),
                    name: t.name.clone(),
                    is_template: true,
                }),
        )
        .collect();
    let prompts: Vec<McpPromptInfo> = mcp::prompts::list_prompts()
        .into_iter()
        .map(|p| McpPromptInfo {
            name: p.name.clone(),
            description: p.description.clone().unwrap_or_default(),
            arguments: p
                .arguments
                .as_ref()
                .map(|args| args.iter().map(|a| a.name.clone()).collect())
                .unwrap_or_default(),
        })
        .collect();

    Ok(McpStatus {
        server_name: "myshelltool",
        server_version: env!("CARGO_PKG_VERSION"),
        endpoint,
        data_dir: data_dir.to_string_lossy().to_string(),
        probe,
        tools,
        resources,
        prompts,
    })
}

/// v1.5：前端 GUI 弹窗审批的用户回传命令。
///
/// server.rs 在客户端不支持 elicitation 时，经 AppHandle emit `mcp-tool-approval`
/// 事件给前端 GlobalModals 弹窗；用户点确认/拒绝后，前端调本命令回传决定。
/// 复用 AppState 持有的 mcp_approval_pending Arc（与 McpToolContext 共享），
/// 取出 server.rs 注册的 oneshot::Sender 并 send。
///
/// 模式照 ssh.rs:998 ssh_confirm_host_key。
#[tauri::command]
async fn mcp_confirm_tool(
    state: State<'_, AppState>,
    request_id: String,
    accepted: bool,
) -> Result<(), String> {
    mcp::approval::resolve_approval(&state.mcp_approval_pending, &request_id, accepted).await
}

// ─── v2：MCP 拦截等级配置 + 执行日志（前端 MCP 面板）───

/// 读当前拦截等级（内存 Arc，不动盘）。
#[tauri::command]
async fn mcp_get_config(state: State<'_, AppState>) -> Result<mcp::config::McpConfig, String> {
    Ok(state.mcp_config.read().await.clone())
}

/// 切换拦截等级：解析枚举 → 更新共享 Arc（已建 MCP 会话下次调用即生效）→ 落盘。
/// 无效 level（非 minimal/strict）返回 Err。
#[tauri::command]
async fn mcp_set_config(state: State<'_, AppState>, level: String) -> Result<mcp::config::McpConfig, String> {
    let level_enum = mcp::config::McpInterceptLevel::parse(&level).ok_or_else(|| {
        format!("无效的拦截等级: {level}（可选 minimal / strict）")
    })?;
    let new_config = mcp::config::McpConfig { level: level_enum };
    *state.mcp_config.write().await = new_config.clone();
    mcp::config::save_mcp_config(&mcp::config::mcp_config_path(&state.mcp_data_dir), &new_config)?;
    Ok(new_config)
}

/// 读执行日志最近条目（timestampMs 倒序）。limit 缺省 200。
#[tauri::command]
fn mcp_list_execution_logs(
    state: State<'_, AppState>,
    limit: Option<u16>,
) -> Result<Vec<mcp::execution_log::ExecutionLogEntry>, String> {
    Ok(mcp::execution_log::list_entries(
        &state.mcp_data_dir,
        limit.unwrap_or(200) as usize,
    ))
}

/// 清空执行日志。
#[tauri::command]
async fn mcp_clear_execution_logs(state: State<'_, AppState>) -> Result<(), String> {
    mcp::execution_log::clear_entries(&state.mcp_data_dir).await
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
fn save_connection_asset(
    state: State<'_, AppState>,
    asset: myshelltool_core::ConnectionAsset,
) -> Result<ConnectionAssetList, String> {
    let mut store = myshelltool_core::load_connection_asset_store(&state.asset_store_path)?;
    myshelltool_core::upsert_connection_asset(&mut store, asset)?;
    myshelltool_core::save_connection_asset_store(&state.asset_store_path, &store)?;
    Ok(ConnectionAssetList {
        source: "local asset store",
        count: store.assets.len(),
        assets: store.assets,
        groups: store.groups,
    })
}

#[tauri::command]
fn delete_connection_asset(state: State<'_, AppState>, id: String) -> Result<ConnectionAssetList, String> {
    let mut store = myshelltool_core::load_connection_asset_store(&state.asset_store_path)?;
    myshelltool_core::remove_connection_asset(&mut store, &id)?;
    myshelltool_core::save_connection_asset_store(&state.asset_store_path, &store)?;
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

struct FileLogger {
    file: Mutex<std::fs::File>,
}

impl FileLogger {
    fn new(path: &std::path::Path) -> Result<Self, std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            // 本地时间前缀（便于按时间排查问题；chrono 处理时区/闰秒等）。
            let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let msg = format!(
                "[{} {} {}] {}\n",
                now,
                record.level(),
                record.target(),
                record.args()
            );
            eprint!("{}", msg);
            if let Ok(mut file) = self.file.lock() {
                let _ = file.write_all(msg.as_bytes());
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let log_path = app_data_dir.join("logs").join("myshelltool.log");
            let logger = FileLogger::new(&log_path).expect("failed to create log file");
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
            // v2：MCP 数据目录与拦截等级配置。data_dir 统一从 mcp_data_dir() 取
            //（环境变量优先），让 endpoint / mcp-config.json / mcp-execution-log.json
            // 与 mcp_status 命令读到的目录一致。配置 load 失败（损坏/不存在）→ 默认 Minimal。
            let mcp_dir = mcp_data_dir();
            let mcp_config: Arc<tokio::sync::RwLock<mcp::config::McpConfig>> = Arc::new(
                tokio::sync::RwLock::new(mcp::config::load_mcp_config(
                    &mcp::config::mcp_config_path(&mcp_dir),
                )),
            );
            app.manage(AppState {
                asset_store_path: app_data_dir.join("connection-assets.json"),
                secret_store_dir: app_data_dir.join("credentials"),
                ssh_sessions: ssh_mgr,
                resource_monitors: Arc::new(Mutex::new(resource_monitor::ResourceMonitorState::default())),
                mcp_shutdown: mcp_shutdown.clone(),
                mcp_approval_pending: mcp_approval_pending.clone(),
                mcp_config: mcp_config.clone(),
                mcp_data_dir: mcp_dir.clone(),
                session_handoff: Mutex::new(std::collections::HashMap::new()),
                sync_oauth_pending: Mutex::new(None),
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
            );
            tauri::async_runtime::spawn(mcp::http_server::run_http_server(
                mcp_ctx,
                mcp_dir,
                mcp_shutdown,
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
            mcp_status,
            // v1.5：MCP 高危工具 GUI 弹窗审批的用户回传命令
            mcp_confirm_tool,
            // v2：MCP 拦截等级配置 + 执行日志（前端 MCP 面板）
            mcp_get_config,
            mcp_set_config,
            mcp_list_execution_logs,
            mcp_clear_execution_logs,
            // 跨窗口会话迁移内存中转（TTL 60s）
            session_handoff_put,
            session_handoff_take,
            list_connection_assets,
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
            // GitHub Device Flow 登录（替代手动粘贴 PAT；provider 参数预留多服务）
            sync_oauth::sync_oauth_start,
            sync_oauth::sync_oauth_poll,
            sync_oauth::sync_oauth_cancel,
            save_credential,
            get_credential_status,
            delete_credential,
            ssh::ssh_connect,
            ssh::ssh_list_directory,
            ssh::ssh_write,
            ssh::ssh_resize,
            ssh::ssh_disconnect,
            ssh::ssh_confirm_host_key,
            ssh::ssh_keyboard_response,
            ssh::sftp_list_dir,
            ssh::sftp_read_file,
            ssh::sftp_write_file,
            ssh::sftp_upload_start,
            ssh::sftp_upload_chunk,
            ssh::sftp_upload_finalize,
            ssh::sftp_download_with_progress,
            ssh::sftp_mkdir,
            ssh::sftp_rename,
            ssh::sftp_remove,
            ssh::sftp_stat,
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
            fs_local::fs_local_read_chunk,
            resource_monitor::resource_monitor_start,
            resource_monitor::resource_monitor_stop,
            resource_monitor::resource_monitor_snapshot,
            resource_monitor::resource_monitor_list_active
        ])
        .run(tauri::generate_context!())
        .expect("failed to run myshelltool");
}

// ─── MCP 数据目录解析（v1.4：MCP 内嵌 GUI，此函数供 mcp_status 命令用）───

/// 解析 MCP 数据目录。
///
/// 优先级：
/// 1. 环境变量 `MYSHELLTOOL_DATA_DIR`（测试/自定义数据目录时可显式指定）
/// 2. `%APPDATA%/com.redtei.myshelltool`（与 GUI 的 Tauri app_data_dir 一致，
///    目录名取自 tauri.conf.json 的 identifier）
///
/// 注意：Tauri 2 的 app_data_dir 用 **identifier**（com.redtei.myshelltool）
/// 作目录名，不是 productName（myshelltool）——两者不能混。
///
/// v1.4 变化：v1.0-v1.3 此函数主要给独立 myshelltool-mcp.exe 用（解析自己的
/// 数据目录）；v1.4 MCP 内嵌 GUI 后，MCP server 直接用 GUI 的 app_data_dir，
/// 此函数仅保留给 mcp_status 命令读取 mcp-endpoint.json 用。
fn mcp_data_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("MYSHELLTOOL_DATA_DIR") {
        return std::path::PathBuf::from(dir);
    }
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(appdata).join("com.redtei.myshelltool")
}
