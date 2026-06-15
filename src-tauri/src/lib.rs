mod fs_local;
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
}

#[tauri::command]
fn backend_status() -> BackendStatus {
    BackendStatus {
        ready: true,
        mode: "tauri-core",
    }
}

#[tauri::command]
fn list_connection_assets(state: State<'_, AppState>) -> Result<ConnectionAssetList, String> {
    let assets = myshelltool_core::load_connection_asset_store(&state.asset_store_path)?.assets;
    Ok(ConnectionAssetList {
        source: "local asset store",
        count: assets.len(),
        assets,
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
    let store = myshelltool_core::SecretStore::new(&state.secret_store_dir);
    store.save(&id, &secret)?;
    store.get_status(&id)
}

#[tauri::command]
fn get_credential_status(
    state: State<'_, AppState>,
    id: String,
) -> Result<myshelltool_core::CredentialStatus, String> {
    let store = myshelltool_core::SecretStore::new(&state.secret_store_dir);
    store.get_status(&id)
}

#[tauri::command]
fn delete_credential(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let store = myshelltool_core::SecretStore::new(&state.secret_store_dir);
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
            let msg = format!(
                "[{} {}] {}\n",
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
            app.manage(AppState {
                asset_store_path: app_data_dir.join("connection-assets.json"),
                secret_store_dir: app_data_dir.join("credentials"),
                ssh_sessions: ssh_mgr,
                resource_monitors: Arc::new(Mutex::new(resource_monitor::ResourceMonitorState::default())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            backend_status,
            list_connection_assets,
            save_connection_asset,
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
