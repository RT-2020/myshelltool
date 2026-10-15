//! 挂载器：出站 SSH 连公网 VPS，remote forward 把内嵌 server 暴露到挂载口。
//!
//! 与 src-tauri `ssh/tunnel.rs` 的 remote 隧道同构（russh 0.49 `tcpip_forward` 要
//! `&mut Handle`，专用连接独占持有），但本实现是 headless 的（无 AppHandle/事件，
//! 日志代替），且带 autossh 语义的断线重连。安全判据（未登记通道不接管、
//! bind 地址匹配）与 tunnel.rs `RemoteForwardClient`/`bind_addr_matches` 同源——
//! 两处各自维护，改判据时互相同步（见 docs/计划/网关模式-实施计划.md §10）。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::net::TcpStream;

use russh::client;
use russh::keys::ssh_key::PublicKey;

use crate::config::{HostKeyPin, MountConfig};
use crate::keys;

/// 挂载参数（运行方向；秘密由调用方从 SecretStore 读出后传入，本模块不碰存储）。
#[derive(Debug, Clone)]
pub struct MountParams {
    pub vps_host: String,
    pub vps_port: u16,
    pub vps_username: String,
    pub private_key_pem: String,
    pub passphrase: Option<String>,
    pub pin: HostKeyPin,
    pub mount: MountConfig,
    /// 内嵌 server 的回环监听口（VPS 来的连接桥到这里完成 SSH 握手）。
    pub gateway_listen_port: u16,
}

/// 一次成功挂载的产物。
#[derive(Debug)]
pub struct Mounted {
    /// VPS sshd 实际绑定端口（请求固定口被占时会漂移——见连接处 warn）。
    pub assigned_port: u16,
    /// 断线监视任务：结束 = 挂载连接已断（is_closed 5s 轮询）。
    pub watcher: tokio::task::JoinHandle<()>,
}

/// 挂载专用 client handler：VPS host key pin 校验 + forwarded-tcpip 接管。
struct MountClient {
    pin: HostKeyPin,
    /// 已登记挂载的期望（归一化绑定地址 + 实际端口）；写入前到达的通道一律丢弃
    /// （防恶意服务器开未请求的转发通道借道探测）。
    expected: Arc<Mutex<Option<(String, u32)>>>,
    gateway_listen_port: u16,
}

#[async_trait]
impl client::Handler for MountClient {
    type Error = russh::Error;

    async fn check_server_key(&mut self, server_public_key: &PublicKey) -> Result<bool, Self::Error> {
        // pin 判据：公钥字节 hex 精确比对（配置自包含，不依赖跳板机 known_hosts）
        if keys::pin_matches(server_public_key, &self.pin.key_hex) {
            Ok(true)
        } else {
            // fail-closed：pin 不符 = VPS host key 变更/被中间人，拒绝并给出明确文案
            log::error!(
                "gateway mount: VPS host key 与 pin 不符（期望类型 {}）：拒绝连接",
                self.pin.key_type
            );
            Ok(false)
        }
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<client::Msg>,
        connected_address: &str,
        connected_port: u32,
        originator_address: &str,
        originator_port: u32,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let expected = self.expected.lock().ok().and_then(|g| g.clone());
        let registered = expected
            .is_some_and(|(addr, port)| port == connected_port && bind_addr_matches(&addr, connected_address));
        if !registered {
            // 未登记的转发通道：不接管，drop 即关（返回 Err 会打死整条挂载连接）
            log::warn!(
                "gateway mount: 丢弃未登记的 forwarded-tcpip 通道 {connected_address}:{connected_port}（来源 {originator_address}:{originator_port}）"
            );
            return Ok(());
        }
        let target = format!("127.0.0.1:{}", self.gateway_listen_port);
        tokio::spawn(async move {
            match TcpStream::connect(&target).await {
                Ok(mut tcp) => {
                    // VPS 侧来的 TCP ↔ 挂载通道；对端之上是本机客户端与内嵌 server
                    // 的端到端 SSH（VPS 只见密文字节流）
                    let mut chan = channel.into_stream();
                    match tokio::io::copy_bidirectional(&mut chan, &mut tcp).await {
                        Ok(_) => {}
                        Err(e) => log::debug!("gateway mount: 通道结束：{e}"),
                    }
                }
                Err(e) => log::error!("gateway mount: 桥接内嵌 server {target} 失败（server 未运行？）：{e}"),
            }
        });
        Ok(())
    }
}

/// 请求绑定地址与服务器回报地址比对（与 tunnel.rs:260 同语义，两处同步维护）：
/// localhost 归一 127.0.0.1；通配请求（0.0.0.0/::/空）只按端口匹配。
fn bind_addr_matches(requested: &str, connected: &str) -> bool {
    fn norm(a: &str) -> &str {
        if a == "localhost" { "127.0.0.1" } else { a }
    }
    let req = norm(requested);
    req == norm(connected) || req == "0.0.0.0" || req == "::" || req.is_empty()
}

/// RSA 强制 rsa-sha2-256（OpenSSH 8.8+ 禁 SHA-1）；非 RSA 必须传 None。
/// 与 src-tauri session.rs `wrap_key_with_preferred_hash` 同判据（两处同步维护）。
fn wrap_key(key: russh::keys::PrivateKey) -> Result<russh::keys::key::PrivateKeyWithHashAlg, String> {
    let hash_alg = if key.algorithm().is_rsa() {
        Some(russh::keys::HashAlg::Sha256)
    } else {
        None
    };
    russh::keys::key::PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg)
        .map_err(|e| format!("挂载私钥封装失败：{e}"))
}

fn mount_client_config() -> russh::client::Config {
    let mut config = russh::client::Config::default();
    config.keepalive_interval = Some(Duration::from_secs(30)); // NAT 保活（v3 修订#6）
    config.keepalive_max = 3;
    config.inactivity_timeout = Some(Duration::from_secs(300)); // keepalive 有流量，不应触发
    config
}

/// 一次性挂载：连接 VPS → 私钥认证 → tcpip_forward → 登记期望 → 起断线监视。
pub async fn connect_and_mount(params: &MountParams) -> Result<Mounted, String> {
    let expected: Arc<Mutex<Option<(String, u32)>>> = Arc::new(Mutex::new(None));
    let handler = MountClient {
        pin: params.pin.clone(),
        expected: Arc::clone(&expected),
        gateway_listen_port: params.gateway_listen_port,
    };

    let mut handle = client::connect(
        Arc::new(mount_client_config()),
        (params.vps_host.as_str(), params.vps_port),
        handler,
    )
    .await
    .map_err(|e| {
        format!(
            "连接 VPS {}:{} 失败（检查跳板机出站连通性/端口）：{e}",
            params.vps_host, params.vps_port
        )
    })?;

    let key = keys::decode_private_key(&params.private_key_pem, params.passphrase.as_deref())?;
    let authed = handle
        .authenticate_publickey(&params.vps_username, wrap_key(key)?)
        .await
        .map_err(|e| format!("VPS 认证通信失败：{e}"))?;
    if !authed {
        return Err(format!(
            "VPS 拒绝挂载私钥认证（用户 {}）：确认公钥已加入 VPS 的 authorized_keys（Docker relay 为 AUTHORIZED_KEYS 环境变量）",
            params.vps_username
        ));
    }

    let requested = params.mount.remote_port as u32;
    let assigned = handle
        .tcpip_forward(params.mount.bind_addr.clone(), requested)
        .await
        .map_err(|e| {
            format!(
                "请求挂载口失败（VPS sshd 需 AllowTcpForwarding + GatewayPorts clientspecified；\
                 Docker relay 镜像已内置）：{e}"
            )
        })?;
    // russh 语义（client/mod.rs:559）：固定端口请求成功时服务器回 0 =「端口未变」；
    // 仅请求 0（动态分配）时回实际端口。0 不是漂移，是成功确认。
    let assigned = if assigned == 0 { requested } else { assigned };
    if assigned != requested {
        // 固定端口语义被破坏（被占时 sshd 分配了别的口）——本机资产还指着请求口，
        // 必须显式告警而不是静默换口
        log::warn!(
            "gateway mount: 挂载口漂移（请求 {}，实际绑定 {assigned}）——VPS 上 {} 端口可能被占；本机资产需指向实际端口",
            requested, requested
        );
    }

    // 登记期望（归一化绑定地址 + 实际端口），此后到达的 forwarded-tcpip 才接管
    if let Ok(mut g) = expected.lock() {
        *g = Some((params.mount.bind_addr.clone(), assigned));
    }

    // 断线监视：is_closed 5s 轮询（同 tunnel.rs 驻守语义；handle 独占移入本任务）
    let watcher = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if handle.is_closed() {
                break;
            }
        }
    });

    Ok(Mounted {
        assigned_port: assigned as u16,
        watcher,
    })
}

/// 常驻挂载循环（autossh 语义）：挂载成功后守护连接，断了/失败后指数退避重试
/// （1s 起、×2、30s 封顶；每次成功重置）。停止方式：abort 本任务所在的
/// 外层任务（进程退出/测试 abort），挂载连接随 Handle drop 断开释放。
/// 各状态迁移点同步上报（status 文件，见 run.rs 状态通道设计）。
pub async fn run_mount_loop(
    params: MountParams,
    status_dir: Option<&std::path::Path>,
    sink: &crate::run::StatusSink,
) {
    use crate::run::{update_status, GatewayStatus, STATE_MOUNTED, STATE_MOUNTING, STATE_RECONNECTING};
    let mut backoff = Duration::from_secs(1);
    loop {
        let status = |state: &str, mount_port: u16, detail: Option<String>| GatewayStatus {
            updated_at: 0, // update_status 统一盖时间戳
            state: state.to_string(),
            vps_host: params.vps_host.clone(),
            vps_port: params.vps_port,
            mount_port,
            detail,
        };
        if let Some(dir) = status_dir {
            update_status(sink, dir, status(STATE_MOUNTING, params.mount.remote_port, None));
        }
        match connect_and_mount(&params).await {
            Ok(mounted) => {
                log::info!(
                    "gateway mount: 已挂载（VPS {}:{} 绑定口 {}）",
                    params.vps_host,
                    params.vps_port,
                    mounted.assigned_port
                );
                backoff = Duration::from_secs(1);
                if let Some(dir) = status_dir {
                    update_status(sink, dir, status(STATE_MOUNTED, mounted.assigned_port, None));
                }
                let _ = mounted.watcher.await;
                log::warn!("gateway mount: 挂载连接断开，准备重连");
                if let Some(dir) = status_dir {
                    update_status(sink, dir, status(STATE_RECONNECTING, params.mount.remote_port, None));
                }
            }
            Err(e) => {
                log::error!("gateway mount: {e}");
                if let Some(dir) = status_dir {
                    update_status(sink, dir, status(STATE_RECONNECTING, params.mount.remote_port, Some(e)));
                }
            }
        }
        log::info!("gateway mount: {}s 后重试挂载", backoff.as_secs());
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}
