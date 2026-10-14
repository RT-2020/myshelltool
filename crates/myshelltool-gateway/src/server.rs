//! 内嵌 SSH server —— 网关核心通道。
//!
//! 攻击面收敛（计划 §5 决策 6）：
//! - 仅 publickey 认证，单键白名单（密码 / none / kbd-interactive 全 Reject——
//!   沿用 russh Handler 默认实现）；忽略 `user` 参数（客户端资产 username 仅展示）。
//! - 仅 direct-tcpip 通道；session / x11 / server 侧 tcpip_forward 等默认全拒
//!   （russh 默认回调返回 false = 通道拒绝）。
//! - 只监听回环——外部到达它的唯一路径是挂载器（VPS remote forward → 本机回环），
//!   加密端点在本机客户端与该 server 之间（VPS 只见 TCP 字节流）。
//!
//! 空闲语义：`inactivity_timeout = None`（挂载链上的空闲转发连接不回收），
//! keepalive 30s × 3 探活（对端真死才断）。

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::net::{TcpListener, TcpStream};

use russh::keys::PublicKeyBase64;
use russh::server::{self, Auth, Server};
use russh::Channel;

use crate::config::{target_allowed, TargetRule};
use crate::keys;

/// 网关 server 的全部状态：russh 配置 + 授权公钥 + 白名单。
/// 实现 `server::Server`（每连接产出一个 `GatewaySession`）。
pub struct GatewayServerState {
    russh_config: Arc<server::Config>,
    authorized_key_bytes: Vec<u8>,
    rules: Arc<Vec<TargetRule>>,
}

impl GatewayServerState {
    /// 从配置构件：host key PEM、authorized_keys 单行、白名单条目。
    pub fn new(
        host_key_pem: &str,
        authorized_key_line: &str,
        allowed_targets: &[String],
    ) -> Result<Self, String> {
        let host_key = keys::decode_private_key(host_key_pem, None)?;
        let authorized = keys::parse_public_key_line(authorized_key_line)?;
        let rules = allowed_targets
            .iter()
            .map(|s| TargetRule::parse(s))
            .collect::<Result<Vec<_>, String>>()?;

        let mut config = server::Config::default();
        config.methods = russh::MethodSet::PUBLICKEY;
        config.keys = vec![host_key];
        config.inactivity_timeout = None; // 空闲转发连接不回收（默认 600s 会掐隧道）
        config.keepalive_interval = Some(Duration::from_secs(30));
        config.keepalive_max = 3;
        config.max_auth_attempts = 4; // 单键场景收紧（默认 10）

        Ok(GatewayServerState {
            russh_config: Arc::new(config),
            authorized_key_bytes: authorized.public_key_bytes(),
            rules: Arc::new(rules),
        })
    }

    /// 在给定 listener 上服务（accept 循环由 russh `run_on_socket` 承担）。
    /// 返回任务句柄——测试 abort 即停；生产由进程生命周期终结。
    pub fn spawn_serving(mut self, listener: TcpListener) -> tokio::task::JoinHandle<()> {
        let config = Arc::clone(&self.russh_config);
        tokio::spawn(async move {
            if let Err(e) = self.run_on_socket(config, &listener).await {
                log::error!("gateway server: accept 循环退出：{e}");
            }
        })
    }
}

impl server::Server for GatewayServerState {
    type Handler = GatewaySession;

    fn new_client(&mut self, _peer_addr: Option<SocketAddr>) -> Self::Handler {
        GatewaySession {
            authorized_key_bytes: self.authorized_key_bytes.clone(),
            rules: Arc::clone(&self.rules),
        }
    }

    fn handle_session_error(&mut self, error: russh::Error) {
        // 连接级失败（含认证耗尽）——记日志即可，server 继续服务其他连接
        log::warn!("gateway server: 连接以错误结束：{error}");
    }
}

/// 每连接一个的 Handler。
pub struct GatewaySession {
    authorized_key_bytes: Vec<u8>,
    rules: Arc<Vec<TargetRule>>,
}

impl GatewaySession {
    fn key_authorized(&self, public_key: &russh::keys::ssh_key::PublicKey) -> bool {
        // 字节级比对（与 pin / known_hosts 同判据），不依赖 PublicKey 的 Eq 语义
        public_key.public_key_bytes() == self.authorized_key_bytes
    }
}

#[async_trait]
impl server::Handler for GatewaySession {
    type Error = russh::Error;

    async fn auth_publickey_offered(
        &mut self,
        user: &str,
        public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        // offered 阶段先筛：键不匹配直接拒，省客户端一次签名
        if self.key_authorized(public_key) {
            Ok(Auth::Accept)
        } else {
            log::warn!("gateway server: 拒绝未授权公钥（user={user}）");
            Ok(Auth::Reject { proceed_with_methods: None })
        }
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        // 签名验证通过后再确认一次（russh 已验签，这里只管白名单）
        if self.key_authorized(public_key) {
            Ok(Auth::Accept)
        } else {
            log::warn!("gateway server: 验签后公钥不在白名单（user={user}）");
            Ok(Auth::Reject { proceed_with_methods: None })
        }
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<server::Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        originator_address: &str,
        _originator_port: u32,
        _session: &mut server::Session,
    ) -> Result<bool, Self::Error> {
        let Some(port) = u16::try_from(port_to_connect).ok() else {
            log::warn!("gateway server: direct-tcpip 端口越界（{port_to_connect}）");
            return Ok(false);
        };

        // 白名单判定：字面 IP 直接判；域名 resolve 后**全部候选 IP 过检才放行**，
        // 解析失败 fail-closed 拒绝（计划 §5 决策 7）
        let Some(addrs) = resolve_within_rules(host_to_connect, port, &self.rules).await else {
            log::warn!(
                "gateway server: direct-tcpip {host_to_connect}:{port} 被白名单拒绝（originator {originator_address}）"
            );
            return Ok(false);
        };

        let target_desc = format!("{host_to_connect}:{port}");
        tokio::spawn(async move {
            // addrs 已过白名单；依次尝试（happy-eyeballs 交给 tokio 逐地址语义）
            match TcpStream::connect(addrs.as_slice()).await {
                Ok(mut tcp) => {
                    let mut chan = channel.into_stream();
                    match tokio::io::copy_bidirectional(&mut chan, &mut tcp).await {
                        Ok((a, b)) => {
                            log::debug!("gateway server: {target_desc} 通道结束（↑{a}B ↓{b}B）")
                        }
                        Err(e) => log::debug!("gateway server: {target_desc} 通道中断：{e}"),
                    }
                }
                Err(e) => {
                    // 目标不可达：channel 随作用域 drop，对端连接被关闭
                    log::info!("gateway server: 连接目标 {target_desc} 失败：{e}");
                }
            }
        });
        Ok(true)
    }
}

/// 解析目标地址并按白名单判定。返回可连接的地址列表；`None` = 拒绝
/// （不在白名单 / 域名解析失败 / 无候选）。`rules` 为空 = 允许全部。
async fn resolve_within_rules(
    host: &str,
    port: u16,
    rules: &[TargetRule],
) -> Option<Vec<SocketAddr>> {
    // lookup_host 对字面 IP 不走 DNS（getaddrinfo 语义），字面与域名统一此路径
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port)).await.ok()?.collect();
    if addrs.is_empty() {
        return None;
    }
    if rules.is_empty() {
        return Some(addrs);
    }
    // 全部候选 IP 都过白名单才放行（防 DNS 多值里夹带越界地址）
    if addrs.iter().all(|sa| target_allowed(rules, sa.ip(), port)) {
        Some(addrs)
    } else {
        None
    }
}
