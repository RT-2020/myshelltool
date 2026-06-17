mod dangerous_commands;
mod dpapi_codec;
mod fs_local;
mod mcp;
mod resource_monitor;
mod ssh;

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
    /// v1.1：MCP 审批弹窗 pending 表（request_id → oneshot sender）。
    /// GUI pipe server dispatch emit 弹窗后在此存 sender，前端回传命令从此取。
    pub mcp_approval_pending: mcp::pipe::ApprovalPending,
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
    /// Windows named pipe 路径。
    pipe_name: &'static str,
    /// MCP 进程读写的数据目录（GUI 与 MCP 共享同一份资产/凭据）。
    data_dir: String,
    /// v1.2：MCP 就绪探测结果（无状态、按需）。这是状态灯的唯一信号源——
    /// GUI 主动 spawn myshelltool-mcp.exe + initialize 握手，回答「能否正常工作」。
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
    // v1.2：无状态探测——现场 spawn myshelltool-mcp.exe + initialize 握手。
    // 不依赖 State（探测是纯函数），_state 保留以维持 Tauri 命令签名一致性。
    let probe = mcp::probe::probe_mcp(None).await;

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
        pipe_name: mcp::pipe::PIPE_NAME,
        data_dir: mcp_data_dir().to_string_lossy().to_string(),
        probe,
        tools,
        resources,
        prompts,
    })
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
fn save_sync_settings(
    settings: myshelltool_core::SyncSettings,
    token: myshelltool_core::TokenConfigInput,
) -> myshelltool_core::SyncSettingsSummary {
    myshelltool_core::summarize_sync_settings(settings, token)
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
            let mcp_approval_pending: mcp::pipe::ApprovalPending =
                Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
            app.manage(AppState {
                asset_store_path: app_data_dir.join("connection-assets.json"),
                secret_store_dir: app_data_dir.join("credentials"),
                ssh_sessions: ssh_mgr,
                resource_monitors: Arc::new(Mutex::new(resource_monitor::ResourceMonitorState::default())),
                mcp_approval_pending: mcp_approval_pending.clone(),
            });

            // v1.1 M8：启动 MCP named pipe server，让 myshelltool-mcp.exe
            // 能复用 GUI 已建立的 SSH 会话（避免 headless 重连 + 二次 host key 验证）。
            // v1.1 审批降级：MCP 客户端不支持 elicitation 时，经此 pipe 委托 GUI 弹窗。
            // pipe 的职责回归纯粹（v1.2 删掉了连接状态追踪），「MCP 能否工作」改由
            // probe.rs 无状态探测回答，与 pipe 无关。
            //
            // 从已 manage 的 AppState 抽取 ssh_sessions + approval_pending 的 Arc
            //（照 resource_monitor 的抽取范式），连同 AppHandle clone 给独立 spawn 的
            // pipe server 任务。pipe 挂了不影响 GUI。
            //
            // 注意：setup hook 是同步上下文，此时 Tokio runtime 尚未在当前线程
            // 就绪——裸 `tokio::spawn` 会 panic「no reactor running」。
            // 必须用 `tauri::async_runtime::spawn`，它会绑定到 Tauri 管理的 runtime。
            let state = app.state::<AppState>();
            tauri::async_runtime::spawn(mcp::pipe::run_pipe_server(
                state.ssh_sessions.clone(),
                app.handle().clone(),
                state.mcp_approval_pending.clone(),
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            backend_status,
            // v1.2：MCP 服务可观测性聚合查询（前端状态栏/管理面板）
            mcp_status,
            list_connection_assets,
            save_connection_asset,
            delete_connection_asset,
            rename_asset_group,
            dissolve_asset_group,
            create_asset_group,
            save_sync_settings,
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
            // v1.1：MCP 审批弹窗回传（前端用户点击确认/拒绝后调用）
            mcp::pipe::mcp_approval_resolve,
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
            resource_monitor::resource_monitor_start,
            resource_monitor::resource_monitor_stop,
            resource_monitor::resource_monitor_snapshot,
            resource_monitor::resource_monitor_list_active
        ])
        .run(tauri::generate_context!())
        .expect("failed to run myshelltool");
}

// ─── MCP server 入口（D1 双二进制：myshelltool-mcp.exe console 子系统调用）───

/// 解析 MCP 进程的数据目录。
///
/// 优先级：
/// 1. 环境变量 `MYSHELLTOOL_DATA_DIR`（Claude Desktop 配置里可显式指定）
/// 2. `%APPDATA%/com.redtei.myshelltool`（与 GUI 的 Tauri app_data_dir 一致，
///    目录名取自 tauri.conf.json 的 identifier，保证 GUI 与 MCP 读同一份
///    资产/凭据/known_hosts）
///
/// 注意：Tauri 2 的 app_data_dir 用 **identifier**（com.redtei.myshelltool）
/// 作目录名，不是 productName（myshelltool）——两者不能混。
fn mcp_data_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("MYSHELLTOOL_DATA_DIR") {
        return std::path::PathBuf::from(dir);
    }
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(appdata).join("com.redtei.myshelltool")
}

/// 初始化 MCP 专用 logger。
///
/// 复用 GUI 的 `FileLogger`（stderr + 文件双写，仅 Info 及以上）——
/// 它本来就**绝不写 stdout**（会破坏 JSON-RPC 协议帧解析），符合 MCP 要求。
///
/// 日志路径：`<data_dir>/logs/myshelltool-mcp.log`（与 GUI 日志分开，便于排查）。
pub fn init_mcp_logger() {
    let log_path = mcp_data_dir().join("logs").join("myshelltool-mcp.log");
    match FileLogger::new(&log_path) {
        Ok(logger) => {
            if let Err(e) = log::set_boxed_logger(Box::new(logger))
                .map(|()| log::set_max_level(log::LevelFilter::Info))
            {
                eprintln!("[myshelltool-mcp] failed to set logger: {e}");
            }
            log::info!("myshelltool-mcp logger initialized, log: {}", log_path.display());
        }
        Err(e) => {
            // 日志初始化失败不致命：退化为仅 stderr
            eprintln!("[myshelltool-mcp] failed to open log file {}: {e}", log_path.display());
        }
    }
}

/// MCP stdio server 主入口。被 `src/bin/mcp.rs` 调用。
///
/// Layer 2-5：加载资产/凭据路径 → rmcp stdio serve（三原语）。
///
/// Layer 7（v1.0 降级语义）：v1.0 是「MCP 进程独立建连」，**不依赖 GUI 进程**。
/// 这里的「降级」不是「GUI 未运行」（那是 v1.1 named pipe 场景），而是
/// 「数据目录未初始化 / 资产库为空」时的优雅处理：
/// - 数据目录不存在 → 自动创建（日志目录等），记录警告
/// - 资产库为空 → MCP server 仍正常启动，list_assets 返回空，
///   disk_usage 等工具调用时给出「请先在 GUI 配置资产」的引导错误
///
/// 这样保证 Claude Desktop 始终能连上 MCP server（initialize/tools/list 永远响应），
/// 即使是全新安装未配置任何资产的状态。
pub async fn run_mcp_stdio() -> Result<(), String> {
    log::info!("myshelltool-mcp stdio server starting");
    let data_dir = mcp_data_dir();
    let asset_store_path = data_dir.join("connection-assets.json");
    let secret_store_dir = data_dir.join("credentials");
    let known_hosts_path = data_dir.join("known_hosts.json");

    // Layer 7：数据目录降级——确保目录存在，缺失资产库给出警告但不阻断启动。
    if !data_dir.exists() {
        log::warn!(
            "MCP data dir does not exist, creating: {}",
            data_dir.display()
        );
        if let Err(e) = std::fs::create_dir_all(&data_dir) {
            log::warn!("Failed to create data dir {}: {}", data_dir.display(), e);
            // 不阻断——工具调用时会自然报错
        }
    }
    if !asset_store_path.exists() {
        log::warn!(
            "Asset store not found at {}. list_assets will return empty. \
             Please configure assets in myshelltool GUI first, or place connection-assets.json in the data dir.",
            asset_store_path.display()
        );
    }

    mcp::server::serve_stdio(asset_store_path, secret_store_dir, known_hosts_path)
        .await
        .map_err(|e| format!("MCP server error: {e}"))?;
    Ok(())
}
