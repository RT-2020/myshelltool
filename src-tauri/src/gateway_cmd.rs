//! 网关向导的 Tauri 命令（GUI 侧）。
//!
//! 本机（运维侧）两段式向导——计划 §5 决策 18：
//! - `gateway_wizard_start`：生成挂载密钥对，私钥暂存 SecretStore，返回公钥与
//!   docker run 模板（第一步：用户拿公钥去 VPS 部署 relay）。
//! - `gateway_wizard_finish`：probe 抓 VPS pin → 生成网关 host key 与客户端钥 →
//!   防静默轮换检查 → 组 bundle 加密导出（第二步：用户已部署完成后调用）。
//! - `gateway_pretrust_host_key`：把网关 host key 预写本机 known_hosts
//!   （"VPS:挂载口" → 网关公钥），全链路首次连接零弹窗（决策 5）。
//!
//! 跳板机（客户 Windows PC）GUI 导入——v0.20.1 补「装一个 mst 本体就好」的缺口
//! （此前只有 CLI --import，需拷文件+手敲命令）：
//! - `gateway_import_bundle`：选 bundle 文件 + 输口令，GUI 完成导入（与 CLI 同一
//!   内核 gw_run::import_bundle，DPAPI 落位于本应用数据目录）。
//! - `gateway_launch`：spawn 独立 `--gateway` 常驻进程（detached，关 GUI 不影响）。
//!
//! 凭据分工（v3/L5 修订）：本模块只产钥与 bundle 文件，不落本机资产凭据——
//! 客户端私钥经返回值交前端，走常规 saveAsset 链（credentialIdFor(G_id,'private_key')，
//! 同步收集/删除清理/Gist 备份全复用，计划 §5 决策 16）。

use serde::Serialize;
use tauri::State;

use myshelltool_core::SecretStore;
use myshelltool_gateway::{config as gw_config, keys as gw_keys, run as gw_run};

use crate::ssh::{load_known_hosts, save_known_hosts, KnownHostEntry};
use crate::AppState;

/// 向导第一步暂存挂载私钥的 SecretStore 固定 id（finish 时消费并删除）。
const WIZARD_MOUNT_KEY: &str = "gateway-wizard-mount-key";

fn secret_store_of(state: &State<'_, AppState>) -> SecretStore {
    SecretStore::new(&state.secret_store_dir, Box::new(crate::dpapi_codec::DpapiCodec))
}

#[derive(Serialize)]
pub struct GatewayImportResult {
    pub vps_host: String,
    pub vps_port: u16,
    pub mount_port: u16,
    pub listen_port: u16,
    /// 已代入当前安装路径的 schtasks 自启命令（导入完成页复制用）。
    pub schtasks_command: String,
}

#[tauri::command]
pub async fn gateway_import_bundle(
    state: State<'_, AppState>,
    path: String,
    password: String,
) -> Result<GatewayImportResult, String> {
    if path.trim().is_empty() {
        return Err("请先选择 bundle 文件".to_string());
    }
    // 数据目录 = 本应用 app_data_dir（asset_store_path 的父目录）——GUI 导入与
    // CLI --import 落位同源，两条路径互认（GUI 导入后 CLI 也能直接 --gateway 跑）
    let data_dir = state
        .asset_store_path
        .parent()
        .ok_or("无法定位应用数据目录")?
        .to_path_buf();
    let store = secret_store_of(&state);
    let rt = gw_run::import_bundle(std::path::Path::new(&path), &password, &store, &data_dir)?;
    log::info!(
        "gateway: GUI 导入完成（VPS {}:{}，挂载口 {}）",
        rt.vps_host,
        rt.vps_port,
        rt.mount.remote_port
    );
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "myshelltool.exe".into());
    Ok(GatewayImportResult {
        vps_host: rt.vps_host,
        vps_port: rt.vps_port,
        mount_port: rt.mount.remote_port,
        listen_port: rt.listen_port,
        schtasks_command: format!(
            "schtasks /Create /TN myshelltool-gateway /TR \"\\\"{exe}\\\" --gateway\" /SC ONSTART /RU %USERNAME% /RP"
        ),
    })
}

#[tauri::command]
pub async fn gateway_launch() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("定位自身路径失败：{e}"))?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS：无窗口独立常驻，关 GUI 不随父进程退出
        // （开机自启由 schtasks 承担；本命令只负责「导入完立即跑起来」）
        std::process::Command::new(exe)
            .arg("--gateway")
            .creation_flags(0x0000_0008)
            .spawn()
            .map_err(|e| format!("启动网关进程失败：{e}"))?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new(exe)
            .arg("--gateway")
            .spawn()
            .map_err(|e| format!("启动网关进程失败：{e}"))?;
    }
    Ok(())
}

#[derive(Serialize)]
pub struct WizardStartResult {
    /// 挂载公钥（authorized_keys 单行）——用户授权到 VPS 的就是它。
    pub mount_public_key: String,
    /// 已代入公钥的 docker run 模板（一键复制）。
    pub docker_template: String,
}

#[tauri::command]
pub async fn gateway_wizard_start(
    state: State<'_, AppState>,
    vps_host: String,
    vps_port: u16,
    vps_username: String,
) -> Result<WizardStartResult, String> {
    if vps_host.trim().is_empty() || vps_username.trim().is_empty() {
        return Err("VPS 地址与用户名不能为空".to_string());
    }
    // 提前校验 endpoint 合法性（复用 bundle 校验器对 vps 段的判据），probe 留给 finish
    let (mount_private, mount_public) = gw_keys::generate_ed25519()?;
    secret_store_of(&state).save(WIZARD_MOUNT_KEY, &mount_private)?;

    let docker_template = format!(
        "docker run -d --name myshelltool-relay --network host --restart=always \\\n  -v /data/myshelltool-relay:/etc/ssh/hostkeys \\\n  -e AUTHORIZED_KEYS='{mount_public}' \\\n  -e PORT=22222 -e MOUNT_USER={vps_username} \\\n  ghcr.io/redtei/myshelltool-relay:latest"
    );
    let _ = vps_port; // 端口在 finish 才用于 probe（模板默认 22222，用户可改）
    Ok(WizardStartResult {
        mount_public_key: mount_public,
        docker_template,
    })
}

#[derive(Serialize)]
pub struct WizardFinishResult {
    /// bundle 加密文件路径（拷去跳板机导入）。
    pub bundle_path: String,
    /// 客户端私钥 PEM——前端经常规 saveAsset 链保存（勿另建存储）。
    pub client_private_key: String,
    /// 本机 known_hosts 预写条目（前端调 gateway_pretrust_host_key 写入）。
    pub known_host_port: String,
    pub known_key_type: String,
    pub known_key_hex: String,
    /// 网关资产字段（前端组装 ConnectionAsset 保存：host=VPS、port=挂载口、tag gateway）。
    pub asset_host: String,
    pub asset_port: u16,
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn gateway_wizard_finish(
    state: State<'_, AppState>,
    vps_host: String,
    vps_port: u16,
    vps_username: String,
    bind_addr: Option<String>,
    remote_port: u16,
    listen_port: Option<u16>,
    allowed_targets: Option<Vec<String>>,
    password: String,
    save_path: String,
) -> Result<WizardFinishResult, String> {
    // 1. 取第一步暂存的挂载私钥（没有 = 跳过第一步）
    let store = secret_store_of(&state);
    let mount_private = store
        .read(WIZARD_MOUNT_KEY)?
        .ok_or_else(|| "请先完成向导第一步（生成挂载密钥并部署 VPS）".to_string())?;

    // 2. probe 抓 VPS host key pin（两段式：用户应已完成部署）
    let (pin_type, pin_hex) = gw_run::probe_vps_public_key(
        vps_host.trim(),
        vps_port,
        std::time::Duration::from_secs(15),
    )
    .await
    .map_err(|e| format!("{e}；请确认 VPS relay 已按第一步模板部署并启动"))?;

    // 3. 生成网关 host key + 客户端钥（host key 公钥的 hex 供预写 known_hosts）
    let (host_private, host_public) = gw_keys::generate_ed25519()?;
    let (client_private, client_public) = gw_keys::generate_ed25519()?;
    let host_key_hex = gw_keys::public_key_hex(&gw_keys::parse_public_key_line(&host_public)?);

    // 4. 防静默轮换（v3/M5 修订）：同 "VPS:挂载口" 已有**不同**公钥 → 阻止
    let host_port = format!("{}:{}", vps_host.trim(), remote_port);
    {
        let path = {
            let mgr = state.ssh_sessions.lock().await;
            mgr.known_hosts_path.clone()
        };
        let hosts = load_known_hosts(&path);
        if let Some(existing) = hosts.get(&host_port) {
            if !existing.key_hex.eq_ignore_ascii_case(&host_key_hex) {
                return Err(format!(
                    "「{host_port}」的 known_hosts 已有不同网关的公钥：静默更换会让旧跳板机全部失配。\
                     请先删除旧网关资产与该 known_hosts 条目（或换一个挂载口）"
                ));
            }
        }
    }

    // 5. 组 bundle 并加密导出
    let bundle = gw_config::GatewayBundle {
        version: gw_config::BUNDLE_VERSION,
        vps: gw_config::VpsEndpoint {
            host: vps_host.trim().to_string(),
            port: vps_port,
            username: vps_username.trim().to_string(),
            private_key: mount_private,
            passphrase: None, // 挂载钥由向导生成，无口令
            host_key_pin: gw_config::HostKeyPin {
                key_type: pin_type,
                key_hex: pin_hex,
            },
        },
        mount: gw_config::MountConfig {
            bind_addr: bind_addr.unwrap_or_else(|| "0.0.0.0".into()),
            remote_port,
        },
        gateway: gw_config::GatewayServerConfig {
            listen_port: listen_port.unwrap_or(22222),
            authorized_key: client_public,
            allowed_targets: allowed_targets.unwrap_or_default(),
        },
        host_key_private: host_private,
    };
    let encrypted = gw_config::export_bundle(&bundle, &password)?;
    std::fs::write(&save_path, encrypted)
        .map_err(|e| format!("写 bundle 文件 {save_path} 失败：{e}"))?;

    // 6. 消费暂存私钥（bundle 里已含），防第一步状态残留
    let _ = store.delete(WIZARD_MOUNT_KEY);

    Ok(WizardFinishResult {
        bundle_path: save_path,
        client_private_key: client_private,
        known_host_port: host_port,
        known_key_type: "ssh-ed25519".to_string(),
        known_key_hex: host_key_hex,
        asset_host: vps_host.trim().to_string(),
        asset_port: remote_port,
    })
}

#[tauri::command]
pub async fn gateway_pretrust_host_key(
    state: State<'_, AppState>,
    host_port: String,
    key_type: String,
    key_hex: String,
) -> Result<(), String> {
    if host_port.trim().is_empty() || key_hex.trim().is_empty() {
        return Err("host_port 与 key_hex 不能为空".to_string());
    }
    let path = {
        let mgr = state.ssh_sessions.lock().await;
        mgr.known_hosts_path.clone()
    };
    let mut hosts = load_known_hosts(&path);
    if let Some(existing) = hosts.get(&host_port) {
        if existing.key_hex.eq_ignore_ascii_case(key_hex.trim()) {
            return Ok(()); // 幂等：相同 key 重复预写无害
        }
        return Err(format!(
            "「{host_port}」已有不同公钥的记录：静默覆盖会让既有连接失配，请先人工确认后从 known_hosts 移除"
        ));
    }
    hosts.insert(
        host_port,
        KnownHostEntry {
            key_type,
            key_hex: key_hex.trim().to_lowercase(),
        },
    );
    save_known_hosts(&path, &hosts)
}
