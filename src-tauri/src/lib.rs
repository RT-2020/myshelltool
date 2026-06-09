use serde::Serialize;
use std::path::PathBuf;
use tauri::{Manager, State};

#[derive(Debug, Clone)]
struct AppState {
    asset_store_path: PathBuf,
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            app.manage(AppState {
                asset_store_path: app_data_dir.join("connection-assets.json"),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            backend_status,
            list_connection_assets,
            save_connection_asset,
            save_sync_settings
        ])
        .run(tauri::generate_context!())
        .expect("failed to run myshelltool");
}
