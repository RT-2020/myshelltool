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

use myshelltool_core::{recovery_code, SecretStore};
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

#[derive(Serialize)]
pub struct GatewayStatusView {
    /// running(mounted/mounting/reconnecting) | stopped(心跳超时/进程死) | not_configured
    pub state: String,
    pub vps_host: String,
    pub vps_port: u16,
    pub mount_port: u16,
    #[serde(rename = "detail")]
    pub detail: Option<String>,
    /// 状态文件最后更新距今秒数（stopped 判定依据）。
    pub age_secs: u64,
}

#[tauri::command]
pub async fn gateway_status(state: State<'_, AppState>) -> Result<GatewayStatusView, String> {
    let data_dir = state
        .asset_store_path
        .parent()
        .ok_or("无法定位应用数据目录")?
        .to_path_buf();
    let path = data_dir.join(gw_run::STATUS_FILE);
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => {
            return Ok(GatewayStatusView {
                state: "not_configured".into(),
                vps_host: String::new(),
                vps_port: 0,
                mount_port: 0,
                detail: None,
                age_secs: 0,
            })
        }
    };
    let st: serde_json::Value = serde_json::from_str(&raw).map_err(|e| format!("状态文件损坏：{e}"))?;
    let updated_at = st.get("updated_at").and_then(|v| v.as_u64()).unwrap_or(0);
    let age_secs = gw_run::now_ms().saturating_sub(updated_at) / 1000;
    // 25s 心跳超时 = 网关进程已死/挂起（心跳 10s 一拍，容 2 拍抖动）
    let state_str = if age_secs > 25 {
        "stopped".to_string()
    } else {
        st.get("state").and_then(|v| v.as_str()).unwrap_or("stopped").to_string()
    };
    Ok(GatewayStatusView {
        state: state_str,
        vps_host: st.get("vps_host").and_then(|v| v.as_str()).unwrap_or("").into(),
        vps_port: st.get("vps_port").and_then(|v| v.as_u64()).unwrap_or(0) as u16,
        mount_port: st.get("mount_port").and_then(|v| v.as_u64()).unwrap_or(0) as u16,
        detail: st.get("detail").and_then(|v| v.as_str()).map(String::from),
        age_secs,
    })
}

/// v0.21「服务器+口令」跳板机配置（用户理想口径：只输 relay 地址与口令）。
/// 落位：口令存 SecretStore（gateway-secret，DPAPI）；运行配置零秘密（mode=secret，
/// 密钥各运行时现派生）。relay 侧须以同一口令 `docker run -e GATEWAY_SECRET=...` 部署。
/// v0.21.2：挂载口/白名单参数化——安全组只放行特权口的中转架构（如 iptables
/// 80→18080）需要非默认挂载口；白名单执法在本机内嵌 server（空 = 允许全部）。
#[tauri::command]
pub async fn gateway_setup_from_secret(
    state: State<'_, AppState>,
    host: String,
    port: Option<u16>,
    mount_port: Option<u16>,
    allowed_targets: Option<Vec<String>>,
    secret: String,
) -> Result<GatewayImportResult, String> {
    if host.trim().is_empty() {
        return Err("请输入 relay 服务器地址".to_string());
    }
    // 提前校验口令强度与派生可用性（fail-fast，不在启动期才炸）
    gw_keys::derive_ed25519(secret.trim(), gw_keys::LABEL_MOUNT)?;

    // 全量校验前置（v0.21.2 审查修订）：from_secret 含白名单 TargetRule 逐条校验，
    // runtime_to_json 含序列化校验——两者都过了才落口令与配置，消除半写态
    //（否则坏白名单时新口令已入 store 而旧配置仍在，secret 模式重配会装载
    // 「旧 host/port + 新口令」的错配身份，陷入难归因的重连循环）。
    let rt = gw_config::GatewayRuntimeConfig::from_secret(
        host.trim(),
        port.unwrap_or(22222),
        mount_port.unwrap_or(2222),
        secret.trim(),
        allowed_targets.unwrap_or_default(),
    )?;
    let rt_json = gw_config::runtime_to_json(&rt)?;
    let data_dir = state
        .asset_store_path
        .parent()
        .ok_or("无法定位应用数据目录")?
        .to_path_buf();
    let store = secret_store_of(&state);
    store.save(gw_config::SECRET_GATEWAY_SECRET, secret.trim())?;
    myshelltool_core::write_atomic(data_dir.join(gw_run::RUNTIME_CONFIG_FILE), &rt_json)?;
    log::info!(
        "gateway: 口令模式配置完成（relay {}:{}，挂载口 {}）",
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

/// 读状态文件的 (pid, 心跳年龄 ms)。无文件/损坏/无 pid 字段 → (None, u64::MAX)。
fn read_status_pid(data_dir: &std::path::Path) -> (Option<u32>, u64) {
    let Ok(raw) = std::fs::read_to_string(data_dir.join(gw_run::STATUS_FILE)) else {
        return (None, u64::MAX);
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return (None, u64::MAX);
    };
    let pid = v.get("pid").and_then(|p| p.as_u64()).map(|p| p as u32);
    let updated = v.get("updated_at").and_then(|t| t.as_u64()).unwrap_or(0);
    (pid, gw_run::now_ms().saturating_sub(updated))
}

/// 停掉仍在运行的旧网关进程，返回其 pid（供接管校验比对）。
/// 安全判据：**仅当心跳新鲜（≤25s）才停**——新鲜 pid 必属活网关（10s 心跳）；
/// 陈旧 pid 可能已被系统复用，杀之会误伤无辜进程，宁可走接管校验兜底。
/// v0.21.3 修复「更新配置后点击启动，实际仍是旧进程旧配置」：
/// 旧实现 spawn 完就报成功，新进程被旧进程占着的 22222 顶掉后静默退出。
async fn stop_stale_gateway(data_dir: &std::path::Path) -> Result<Option<u32>, String> {
    let (pid, age_ms) = read_status_pid(data_dir);
    let Some(pid) = pid else { return Ok(None) };
    if pid == 0 || pid == std::process::id() || age_ms > 25_000 {
        return Ok(None);
    }
    #[cfg(windows)]
    {
        let out = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .output();
        match out {
            Ok(o) if o.status.success() => {
                // 等心跳停跳（进程死透、端口释放），最多 5s
                for _ in 0..25 {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    let (_, age) = read_status_pid(data_dir);
                    if age > 26_000 {
                        break;
                    }
                }
                Ok(Some(pid))
            }
            _ => Err(format!(
                "旧网关进程（pid {pid}）停止失败：请在任务管理器结束 myshelltool 后重试"
            )),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        Ok(None)
    }
}

/// spawn 后校验新进程真的接管了（写出自己的 pid）——不再让 spawn 成功冒充
/// 启动成功。超时 Err：典型原因是旧进程（含无 pid 字段的存量旧版）仍占着
/// 回环 22222，把新进程顶掉后静默退出。
async fn verify_takeover(data_dir: &std::path::Path, old_pid: Option<u32>) -> Result<(), String> {
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let (pid, age) = read_status_pid(data_dir);
        if let Some(p) = pid {
            if p != 0 && Some(p) != old_pid && age < 5_000 {
                return Ok(());
            }
        }
    }
    Err(
        "网关进程未能接管（回环 22222 可能仍被占用）：请查看 logs/gateway.log，\
         或在任务管理器结束所有 myshelltool 后重试"
            .into(),
    )
}

#[tauri::command]
pub async fn gateway_launch(state: State<'_, AppState>) -> Result<(), String> {
    let data_dir = state
        .asset_store_path
        .parent()
        .ok_or("无法定位应用数据目录")?
        .to_path_buf();
    let old_pid = stop_stale_gateway(&data_dir).await?;
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
    verify_takeover(&data_dir, old_pid).await
}

/// 生成本机侧「口令模式」用的强随机口令（v0.21.2）。
/// 复用 core 恢复密码生成器（去歧义字母数字——口令要被人工读给客户/手输跳板机，
/// 无引号/符号恰是正确属性），20 位 ≈ 119 bit 熵。不落盘：是否采用由用户决定。
#[tauri::command]
pub async fn gateway_generate_secret() -> Result<String, String> {
    Ok(recovery_code::generate_recovery_password(20))
}

/// 本机侧口令模式的一次性派生结果（v0.21.2）——纯函数零状态，两个按钮同源消费：
/// 「复制 relay 部署命令」用 docker_command，「写入本机配置」用其余字段。
/// 落地链复用 bundle 向导既有约定：本命令不落资产/凭据，客户端私钥经返回值交
/// 前端走常规 saveAsset 链，known_hosts 预写由前端调 gateway_pretrust_host_key。
#[derive(Serialize)]
pub struct SecretDeriveResult {
    /// 已代入口令与控制口的 relay 部署命令（口令模式，GATEWAY_SECRET 派生身份）。
    pub docker_command: String,
    /// 客户端私钥 PEM（本机连挂载口的认证身份）。
    pub client_private_key: String,
    /// 本机 known_hosts 预写三件（键为「VPS:挂载口」——本机实际连的口）。
    pub known_host_port: String,
    pub known_key_type: String,
    pub known_key_hex: String,
    /// 网关资产字段（前端组装 ConnectionAsset：host=VPS、port=挂载口、tag gateway）。
    pub asset_host: String,
    pub asset_port: u16,
}

#[tauri::command]
pub async fn gateway_secret_derive(
    host: String,
    port: Option<u16>,
    mount_port: Option<u16>,
    secret: String,
) -> Result<SecretDeriveResult, String> {
    let host = host.trim().to_string();
    if host.is_empty() {
        return Err("请输入 relay 服务器地址".to_string());
    }
    let secret = secret.trim().to_string();
    // 手输口令守卫：部署命令以单引号嵌 GATEWAY_SECRET，含 ' 会撕裂命令
    //（生成器字母表天然无此字符，此守卫只拦手输）。
    if secret.contains('\'') {
        return Err("口令不能包含单引号（会让 relay 部署命令失效）——请改用「生成口令」".to_string());
    }
    let port = port.unwrap_or(22222);
    let mount_port = mount_port.unwrap_or(2222);
    // from_secret 内含派生 fail-fast（口令强度）+ 全量字段校验（端口/白名单合法性）
    gw_config::GatewayRuntimeConfig::from_secret(&host, port, mount_port, &secret, vec![])?
        .validate()?;
    let (client_pem, _) = gw_keys::derive_ed25519(&secret, gw_keys::LABEL_CLIENT)?;
    let (_, gateway_pub) = gw_keys::derive_ed25519(&secret, gw_keys::LABEL_GATEWAY_HOSTKEY)?;
    let gateway_pk = gw_keys::parse_public_key_line(&gateway_pub)?;
    let docker_command = format!(
        "docker run -d --name myshelltool-relay --network host --restart=always \\\n  -e GATEWAY_SECRET='{secret}' -e PORT={port} \\\n  ghcr.io/redtei/myshelltool-relay:latest"
    );
    Ok(SecretDeriveResult {
        docker_command,
        client_private_key: client_pem,
        known_host_port: format!("{host}:{mount_port}"),
        known_key_type: gateway_pk.algorithm().as_str().to_string(),
        known_key_hex: gw_keys::public_key_hex(&gateway_pk),
        asset_host: host,
        asset_port: mount_port,
    })
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
    // 显式确认替换既有条目（v0.21.2）：默认 false 拒绝——同 host:port 换了新网关
    // 身份的迁移场景（如 bundle→口令模式切换、外部入口口不变）由 GUI 确认弹窗
    // 置 true 重调。禁的是静默轮换，用户显式确认后的替换不违反红线。
    confirm_replace: Option<bool>,
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
        if !confirm_replace.unwrap_or(false) {
            return Err(format!(
                "[pretrust:conflict] 「{host_port}」已有不同公钥的记录（可能是旧网关身份，如 bundle 时代的配置）：\
                 如确认切换到新网关，请在确认后重试以替换该条目"
            ));
        }
        // 显式确认后的替换：继续走 insert 覆盖
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
