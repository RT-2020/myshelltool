//! headless 编排：导入 bundle、装载运行配置、网关主循环、VPS 公钥探测。
//!
//! SecretStore 沿 core 的 codec 注入式（生产 DpapiCodec 由 src-tauri 注入，
//! 测试 PlaintextCodec）——本模块只经 `&SecretStore` 存取，不碰加密实现，
//! 因此「解密 → 校验 → 落位 → 写盘」全链可独立单测（计划 §5 决策 13）。

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::net::TcpListener;

use russh::client;
use russh::keys::ssh_key::PublicKey;

use myshelltool_core::SecretStore;
use myshelltool_core::write_atomic;

use crate::config::{
    self, GatewayRuntimeConfig, SECRET_HOST_KEY, SECRET_VPS_KEY, SECRET_VPS_PASSPHRASE,
};
use crate::keys;
use crate::mount::{run_mount_loop, MountParams};
use crate::server::GatewayServerState;

/// 数据目录内的运行配置文件名（零秘密，秘密在 SecretStore）。
pub const RUNTIME_CONFIG_FILE: &str = "gateway-config.json";

// ─── 导入 ───

/// 导入加密 bundle：解密 → 校验 → 敏感项落 SecretStore → 运行配置原子写盘。
///
/// 顺序刻意保守：先落秘密、后写运行配置——中途失败时宁可「秘密多写一份、
/// 运行配置保持旧版」（旧配置仍完整可用），也不出现「运行配置指到不存在的
/// 秘密」的半配置态（启动 fail-closed 报错，用户难归因）。
pub fn import_bundle(
    bundle_path: &Path,
    password: &str,
    secret_store: &SecretStore,
    data_dir: &Path,
) -> Result<GatewayRuntimeConfig, String> {
    let encrypted = std::fs::read_to_string(bundle_path)
        .map_err(|e| format!("读取 bundle 文件 {} 失败：{e}", bundle_path.display()))?;
    let bundle = config::parse_bundle(&encrypted, password)?;

    secret_store.save(SECRET_HOST_KEY, &bundle.host_key_private)?;
    secret_store.save(SECRET_VPS_KEY, &bundle.vps.private_key)?;
    match &bundle.vps.passphrase {
        Some(p) => secret_store.save(SECRET_VPS_PASSPHRASE, p)?,
        None => {
            // 清理旧导入的残留口令（缺省态必须真缺省，防上次导入的口令暗中介入）
            let _ = secret_store.delete(SECRET_VPS_PASSPHRASE);
        }
    }

    let rt = bundle.to_runtime();
    write_atomic(data_dir.join(RUNTIME_CONFIG_FILE), config::runtime_to_json(&rt)?)?;
    Ok(rt)
}

// ─── 装载与运行 ───

/// 运行所需的全部构件（秘密已从 SecretStore 读出）。
#[derive(Debug)]
pub struct LoadedGateway {
    pub runtime: GatewayRuntimeConfig,
    pub host_key_private: String,
    pub authorized_key_line: String,
    pub allowed_targets: Vec<String>,
    pub mount_params: MountParams,
}

const SECRET_MISSING_HINT: &str = "SecretStore 缺少网关秘密项：请重新导入 bundle；\
若确认已导入，检查运行用户与导入用户是否为同一 Windows 用户（DPAPI User scope 绑定用户）";

pub fn load_runtime(data_dir: &Path, secret_store: &SecretStore) -> Result<LoadedGateway, String> {
    let path = data_dir.join(RUNTIME_CONFIG_FILE);
    let json = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取 {} 失败（尚未导入配置？）：{e}", path.display()))?;
    let rt = config::parse_runtime(&json)?;

    if rt.mode == config::MODE_SECRET {
        // v0.21「服务器+口令」：口令在 SecretStore，密钥现派生（不落盘）
        let secret = secret_store
            .read(config::SECRET_GATEWAY_SECRET)?
            .ok_or("缺少网关口令（gateway-secret）：请在跳板机重新输入服务器地址与口令")?;
        return loaded_from_secret(&rt, &secret);
    }

    let host_key_private = secret_store
        .read(SECRET_HOST_KEY)?
        .ok_or_else(|| SECRET_MISSING_HINT.to_string())?;
    let vps_private_key = secret_store
        .read(SECRET_VPS_KEY)?
        .ok_or_else(|| SECRET_MISSING_HINT.to_string())?;
    let passphrase = secret_store.read(SECRET_VPS_PASSPHRASE).ok().flatten();

    Ok(LoadedGateway {
        mount_params: MountParams {
            vps_host: rt.vps_host.clone(),
            vps_port: rt.vps_port,
            vps_username: rt.vps_username.clone(),
            private_key_pem: vps_private_key,
            passphrase,
            pin: rt.host_key_pin.clone(),
            mount: rt.mount.clone(),
            gateway_listen_port: rt.listen_port,
        },
        runtime: rt,
        host_key_private,
        authorized_key_line: String::new(), // 由 load 完成后经 keys 补齐（见下）
        allowed_targets: Vec::new(),
    })
    .and_then(finish_loading)
}

/// 二段装载（授权公钥与白名单来自 runtime 结构本身，独立出来便于单测覆盖）。
fn finish_loading(mut loaded: LoadedGateway) -> Result<LoadedGateway, String> {
    loaded.authorized_key_line = loaded.runtime.authorized_key.clone();
    loaded.allowed_targets = loaded.runtime.allowed_targets.clone();
    Ok(loaded)
}

/// 网关主循环：起内嵌 server（回环）+ 挂载重连循环。永不正常返回（连接/认证
/// 失败在循环内退避重试）——唯一的返回是 `Err`（配置/绑定级致命错误）。
/// 停止方式：abort 外层任务或进程退出。
/// 网关主循环（Windows exe 路径）：装载（数据目录 + SecretStore）→ 运行。
pub async fn run_gateway(data_dir: &Path, secret_store: &SecretStore) -> Result<(), String> {
    let loaded = load_runtime(data_dir, secret_store)?;
    run_loaded(loaded).await
}

/// 内存直通装载（v0.20.1 容器路径）：加密 bundle 解密后**不落盘**——秘密只在
/// 内存，容器重启即重新解密，天然幂等；无 SecretStore/DPAPI 依赖（Linux 容器
/// 可跑）。bundle 不变 → 网关 host key 不变 → 本机 known_hosts 预写长期有效。
pub fn loaded_from_bundle(bundle: &crate::config::GatewayBundle) -> Result<LoadedGateway, String> {
    bundle.validate()?;
    let rt = bundle.to_runtime();
    Ok(LoadedGateway {
        mount_params: MountParams {
            vps_host: bundle.vps.host.clone(),
            vps_port: bundle.vps.port,
            vps_username: bundle.vps.username.clone(),
            private_key_pem: bundle.vps.private_key.clone(),
            passphrase: bundle.vps.passphrase.clone(),
            pin: bundle.vps.host_key_pin.clone(),
            mount: bundle.mount.clone(),
            gateway_listen_port: bundle.gateway.listen_port,
        },
        host_key_private: bundle.host_key_private.clone(),
        authorized_key_line: bundle.gateway.authorized_key.clone(),
        allowed_targets: bundle.gateway.allowed_targets.clone(),
        runtime: rt,
    })
}

/// v0.21「服务器+口令」的内存直通装载：从口令现派生挂载钥、网关 host key、
/// 客户端钥（authorized_key），pin 由派生 relay host key 确定性得出——零秘密落盘。
pub fn loaded_from_secret(
    rt: &crate::config::GatewayRuntimeConfig,
    secret: &str,
) -> Result<LoadedGateway, String> {
    rt.validate()?;
    let (mount_pem, _) = keys::derive_ed25519(secret, keys::LABEL_MOUNT)?;
    let (host_pem, host_pub) = keys::derive_ed25519(secret, keys::LABEL_GATEWAY_HOSTKEY)?;
    let (_, client_pub) = keys::derive_ed25519(secret, keys::LABEL_CLIENT)?;
    let (_, relay_pub) = keys::derive_ed25519(secret, keys::LABEL_RELAY_HOSTKEY)?;
    let pin = crate::config::HostKeyPin {
        key_type: "ssh-ed25519".into(),
        key_hex: keys::public_key_hex(&keys::parse_public_key_line(&relay_pub)?),
    };
    let _ = host_pub;
    Ok(LoadedGateway {
        mount_params: MountParams {
            vps_host: rt.vps_host.clone(),
            vps_port: rt.vps_port,
            vps_username: rt.vps_username.clone(),
            private_key_pem: mount_pem,
            passphrase: None,
            pin,
            mount: rt.mount.clone(),
            gateway_listen_port: rt.listen_port,
        },
        host_key_private: host_pem,
        authorized_key_line: client_pub,
        allowed_targets: rt.allowed_targets.clone(),
        runtime: rt.clone(),
    })
}

/// 运行已装载的网关（两条装载路径汇合点）：回环 server + 挂载重连循环。
/// 永不正常返回；唯一返回 Err 是配置/绑定级致命错误。
pub async fn run_loaded(loaded: LoadedGateway) -> Result<(), String> {
    let state = GatewayServerState::new(
        &loaded.host_key_private,
        &loaded.authorized_key_line,
        &loaded.allowed_targets,
    )?;
    let listen_port = loaded.runtime.listen_port;
    let listener = TcpListener::bind(("127.0.0.1", listen_port))
        .await
        .map_err(|e| format!("内嵌 server 绑定 127.0.0.1:{listen_port} 失败（端口被占？）：{e}"))?;
    state.spawn_serving(listener);
    log::info!(
        "gateway: 内嵌 server 已监听 127.0.0.1:{listen_port}，开始挂载 {}:{}",
        loaded.runtime.vps_host,
        loaded.runtime.vps_port
    );

    run_mount_loop(loaded.mount_params).await;
    Ok(())
}

// ─── VPS 公钥探测（pin 采集，向导复用） ───

/// 连 VPS 到 KEX 即捕获公钥（认证前），不做认证。用于向导抓 pin + 展示指纹。
/// 返回 (key_type, key_hex)。
pub async fn probe_vps_public_key(
    host: &str,
    port: u16,
    timeout: Duration,
) -> Result<(String, String), String> {
    struct Probe {
        captured: Arc<Mutex<Option<(String, String)>>>,
    }

    #[async_trait]
    impl client::Handler for Probe {
        type Error = russh::Error;

        async fn check_server_key(&mut self, server_public_key: &PublicKey) -> Result<bool, Self::Error> {
            if let Ok(mut g) = self.captured.lock() {
                *g = Some((
                    server_public_key.algorithm().as_str().to_string(),
                    keys::public_key_hex(server_public_key),
                ));
            }
            // 拒绝（返回 false）→ 握手立即失败断开，不进入认证——探测即完成
            Ok(false)
        }
    }

    let captured = Arc::new(Mutex::new(None));
    let handler = Probe {
        captured: Arc::clone(&captured),
    };
    let config = Arc::new(client::Config::default());
    let _ = tokio::time::timeout(
        timeout,
        client::connect(config, (host, port), handler),
    )
    .await; // connect 必然因 check_server_key=false 失败；成功失败都不看，只看捕获

    captured
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .ok_or_else(|| {
            format!(
                "探测 {host}:{port} 失败：连接不通/超时（检查 VPS 地址端口与跳板机出站连通性）"
            )
        })
}
