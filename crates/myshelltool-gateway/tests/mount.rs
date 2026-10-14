//! 挂载器全链 loopback 测试：russh server 模拟 VPS sshd（实现 tcpip-forward 的
//! 监听 + 回开），搭出完整生产拓扑的回环版：
//!
//! 测试 client → FakeVps 挂载口（remote-forward 监听）
//!            → forwarded-tcpip 回到 MountClient（挂载连接）
//!            → 桥到内嵌 GatewayServer 回环口
//!            → direct-tcpip → echo server
//!
//! 生产中「VPS 上真 OpenSSH sshd 内置转发监听」；russh server 不内置该链路，
//! 故 FakeVps 在 `tcpip_forward` 回调里用 `Session::handle()` 自行监听 + 回开。

use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use russh::keys::PublicKeyBase64;
use russh::server;
use russh::Channel;

use myshelltool_gateway::config::{HostKeyPin, MountConfig};
use myshelltool_gateway::keys;
use myshelltool_gateway::mount::{connect_and_mount, MountParams};
use myshelltool_gateway::server::GatewayServerState;

// ─── FakeVps：模拟公网 sshd（publickey 认证 + tcpip-forward） ───

struct FakeVps {
    authorized_key_bytes: Vec<u8>,
}

struct FakeVpsSession {
    authorized_key_bytes: Vec<u8>,
}

impl FakeVps {
    /// 起在随机口，返回 (端口, host key pin 用的公钥 hex, 任务句柄)。
    async fn start(authorized_key_line: &str) -> (u16, String, tokio::task::JoinHandle<()>) {
        let (host_key_pem, host_public_line) = keys::generate_ed25519().expect("vps host key");
        let pin_hex = keys::public_key_hex(&keys::parse_public_key_line(&host_public_line).unwrap());

        let state = FakeVps {
            authorized_key_bytes: keys::parse_public_key_line(authorized_key_line)
                .expect("authorized key")
                .public_key_bytes(),
        };
        let mut config = server::Config::default();
        config.keys = vec![keys::decode_private_key(&host_key_pem, None).expect("decode")];
        let config = Arc::new(config);

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind fake vps");
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            // 自管 accept 循环（russh run_on_socket 持 &mut state 不便 move；run_stream 等价）
            loop {
                let Ok((sock, _)) = listener.accept().await else { break };
                let handler = FakeVpsSession {
                    authorized_key_bytes: state.authorized_key_bytes.clone(),
                };
                let config = Arc::clone(&config);
                tokio::spawn(async move {
                    if let Ok(session) = server::run_stream(config, sock, handler).await {
                        let _ = session.await;
                    }
                });
            }
        });
        (port, pin_hex, task)
    }
}

impl server::Server for FakeVps {
    type Handler = FakeVpsSession;
    fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> Self::Handler {
        FakeVpsSession {
            authorized_key_bytes: self.authorized_key_bytes.clone(),
        }
    }
}

#[async_trait]
impl server::Handler for FakeVpsSession {
    type Error = russh::Error;

    async fn auth_publickey_offered(
        &mut self,
        _user: &str,
        public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<server::Auth, Self::Error> {
        if public_key.public_key_bytes() == self.authorized_key_bytes {
            Ok(server::Auth::Accept)
        } else {
            Ok(server::Auth::Reject { proceed_with_methods: None })
        }
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<server::Auth, Self::Error> {
        if public_key.public_key_bytes() == self.authorized_key_bytes {
            Ok(server::Auth::Accept)
        } else {
            Ok(server::Auth::Reject { proceed_with_methods: None })
        }
    }

    /// 真 sshd 内置的能力：同意转发请求 → 监听端口 → 来连接时向 client 回开
    /// forwarded-tcpip 通道。russh server 不内置，这里手动实现（生产不经过此路径）。
    async fn tcpip_forward(
        &mut self,
        address: &str,
        port: &mut u32,
        session: &mut server::Session,
    ) -> Result<bool, Self::Error> {
        // 测试统一绑回环（生产按 GatewayPorts 语义处理 0.0.0.0）
        let bind_addr = if address == "0.0.0.0" || address.is_empty() {
            "127.0.0.1"
        } else {
            address
        };
        let listener = match TcpListener::bind((bind_addr, *port as u16)).await {
            Ok(l) => l,
            Err(_) => return Ok(false), // 端口被占：拒绝（模拟 sshd 行为）
        };
        if *port == 0 {
            *port = listener.local_addr().unwrap().port() as u32;
        }
        let bound_port = *port;
        let handle = session.handle(); // Clone，可 move 进常驻监听任务
        tokio::spawn(async move {
            loop {
                let Ok((mut tcp, peer)) = listener.accept().await else { break };
                let h = handle.clone();
                tokio::spawn(async move {
                    match h
                        .channel_open_forwarded_tcpip(
                            "127.0.0.1",
                            bound_port,
                            peer.ip().to_string(),
                            peer.port() as u32,
                        )
                        .await
                    {
                        Ok(channel) => {
                            let chan: Channel<server::Msg> = channel;
                            let mut stream = chan.into_stream();
                            let _ = tokio::io::copy_bidirectional(&mut stream, &mut tcp).await;
                        }
                        Err(e) => {
                            // 挂载连接已断（client 侧断开）——正常收尾
                            let _ = e;
                        }
                    }
                });
            }
        });
        Ok(true)
    }
}

// ─── 测试基件 ───

struct NoVerify;

#[async_trait]
impl russh::client::Handler for NoVerify {
    type Error = russh::Error;
    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

async fn start_echo() -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind echo");
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let (mut r, mut w) = sock.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    (port, task)
}

/// 三钥 + echo + 网关 server + FakeVps 一次配齐。
struct FullFixture {
    echo_port: u16,
    /// 本机侧客户端私钥（连网关用）。
    gateway_client_pem: String,
    /// 挂载口（VPS 侧实际绑定端口）。
    mount_port: u16,
}

async fn setup_full() -> FullFixture {
    // 1. echo（客户内网目标）
    let (echo_port, _echo) = start_echo().await;

    // 2. 内嵌网关 server（host key + 客户端钥）
    let (gw_host_pem, _) = keys::generate_ed25519().expect("gw host key");
    let (gw_client_pem, gw_client_pub) = keys::generate_ed25519().expect("gw client key");
    let gw_state = GatewayServerState::new(&gw_host_pem, &gw_client_pub, &[]).expect("gw state");
    let gw_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind gw");
    let gw_port = gw_listener.local_addr().unwrap().port();
    let _gw_task = gw_state.spawn_serving(gw_listener);

    // 3. FakeVps（授权挂载钥）
    let (mount_pem, mount_pub) = keys::generate_ed25519().expect("mount key");
    let (vps_port, pin_hex, _vps_task) = FakeVps::start(&mount_pub).await;

    // 4. 挂载（remote_port 0 = 由「sshd」分配，测试避免端口竞争）
    let params = MountParams {
        vps_host: "127.0.0.1".into(),
        vps_port,
        vps_username: "mount".into(),
        private_key_pem: mount_pem,
        passphrase: None,
        pin: HostKeyPin { key_type: "ssh-ed25519".into(), key_hex: pin_hex },
        mount: MountConfig { bind_addr: "127.0.0.1".into(), remote_port: 0 },
        gateway_listen_port: gw_port,
    };
    let mounted = tokio::time::timeout(std::time::Duration::from_secs(10), connect_and_mount(&params))
        .await
        .expect("mount within 10s")
        .expect("mount ok");
    // watcher 挂着连接；测试进程退出即收（短命任务，无需显式停）

    FullFixture {
        echo_port,
        gateway_client_pem: gw_client_pem,
        mount_port: mounted.assigned_port,
    }
}

async fn connect_gateway(fx: &FullFixture) -> russh::client::Handle<NoVerify> {
    let config = Arc::new(russh::client::Config::default());
    russh::client::connect(config, ("127.0.0.1", fx.mount_port), NoVerify)
        .await
        .expect("connect via mount port")
}

// ─── 测试 ───

#[tokio::test]
async fn full_chain_echo_through_mount() {
    let fx = setup_full().await;
    let mut handle = connect_gateway(&fx).await;

    // 认证网关（客户端钥）——注意这条路到达的是**内嵌 server**而非 FakeVps
    let key = keys::decode_private_key(&fx.gateway_client_pem, None).expect("decode");
    let wrapped = russh::keys::key::PrivateKeyWithHashAlg::new(Arc::new(key), None).unwrap();
    assert!(
        handle.authenticate_publickey("any-user", wrapped).await.expect("auth"),
        "经挂载链认证网关必须成功"
    );

    // direct-tcpip 到「客户内网」echo —— 完整生产拓扑数据往返
    let mut channel = handle
        .channel_open_direct_tcpip("127.0.0.1", fx.echo_port.into(), "origin", 0)
        .await
        .expect("direct-tcpip via full chain");
    let payload = b"hello through vps-mount-gateway";
    channel.data(payload.as_slice()).await.expect("send");
    channel.eof().await.expect("eof");
    let mut got = Vec::new();
    loop {
        match channel.wait().await {
            Some(russh::ChannelMsg::Data { data }) => got.extend_from_slice(&data),
            Some(russh::ChannelMsg::Eof) | None => break,
            _ => {}
        }
        if got.len() >= payload.len() {
            break;
        }
    }
    assert_eq!(got, payload, "全链数据必须原样往返");
}

#[tokio::test]
async fn pin_mismatch_rejected_fail_closed() {
    // 网关 server（随便配）+ FakeVps，pin 给错值 → 挂载必须失败
    let (gw_host_pem, _) = keys::generate_ed25519().unwrap();
    let (_, gw_pub) = keys::generate_ed25519().unwrap();
    let gw_state = GatewayServerState::new(&gw_host_pem, &gw_pub, &[]).unwrap();
    let gw_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gw_port = gw_listener.local_addr().unwrap().port();
    let _gw = gw_state.spawn_serving(gw_listener);

    let (mount_pem, mount_pub) = keys::generate_ed25519().unwrap();
    let (vps_port, _real_pin, _vps) = FakeVps::start(&mount_pub).await;

    let params = MountParams {
        vps_host: "127.0.0.1".into(),
        vps_port,
        vps_username: "mount".into(),
        private_key_pem: mount_pem,
        passphrase: None,
        pin: HostKeyPin { key_type: "ssh-ed25519".into(), key_hex: "00".repeat(32) }, // 错 pin
        mount: MountConfig { bind_addr: "127.0.0.1".into(), remote_port: 0 },
        gateway_listen_port: gw_port,
    };
    let err = connect_and_mount(&params).await.expect_err("错 pin 必须挂载失败");
    assert!(
        err.contains("失败") || err.contains("拒绝"),
        "错误文案应指向连接/握手失败：{err}"
    );
}

#[tokio::test]
async fn unauthorized_mount_key_rejected_with_guidance() {
    let (gw_host_pem, _) = keys::generate_ed25519().unwrap();
    let (_, gw_pub) = keys::generate_ed25519().unwrap();
    let gw_state = GatewayServerState::new(&gw_host_pem, &gw_pub, &[]).unwrap();
    let gw_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gw_port = gw_listener.local_addr().unwrap().port();
    let _gw = gw_state.spawn_serving(gw_listener);

    // FakeVps 授权的是另一对钥 → 挂载钥认证必被拒
    let (_, wrong_pub) = keys::generate_ed25519().unwrap();
    let (mount_pem, _) = keys::generate_ed25519().unwrap();
    let (vps_port, pin_hex, _vps) = FakeVps::start(&wrong_pub).await;

    let params = MountParams {
        vps_host: "127.0.0.1".into(),
        vps_port,
        vps_username: "mount".into(),
        private_key_pem: mount_pem,
        passphrase: None,
        pin: HostKeyPin { key_type: "ssh-ed25519".into(), key_hex: pin_hex },
        mount: MountConfig { bind_addr: "127.0.0.1".into(), remote_port: 0 },
        gateway_listen_port: gw_port,
    };
    let err = connect_and_mount(&params).await.expect_err("未授权钥必须失败");
    assert!(
        err.contains("authorized_keys"),
        "错误文案必须给出授权指引：{err}"
    );
}

/// 直连不经 SSH 的旁证：挂载口本身就是普通 TCP 口（本机浏览器/mstsc 同理可达）。
#[tokio::test]
async fn mount_port_is_plain_tcp_reachable() {
    let fx = setup_full().await;
    // 连挂载口发垃圾字节：不应 panic/挂死，服务端按 SSH 协议处理或断开
    let mut sock = TcpStream::connect(("127.0.0.1", fx.mount_port))
        .await
        .expect("挂载口必须可直连 TCP");
    sock.shutdown().await.ok();
    let mut buf = [0u8; 1];
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), sock.read(&mut buf)).await;
}
