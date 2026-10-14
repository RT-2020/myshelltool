//! 网关内嵌 SSH server 的真握手集成测试（原方案的 spike 验证目标——
//! russh 0.49.2 server API 在真实握手下跑通：pubkey 认证、direct-tcpip 数据面、
//! 通道拒绝面、白名单 enforcement）。
//!
//! 拓扑：russh client → 127.0.0.1 回环 → GatewayServerState（spawn_serving）
//! → direct-tcpip → 本地 echo TCP server。

use std::sync::Arc;

use async_trait::async_trait;
use tokio::net::TcpListener;

use myshelltool_gateway::keys;
use myshelltool_gateway::server::GatewayServerState;

// ─── 测试基件 ───

struct NoVerify;

#[async_trait]
impl russh::client::Handler for NoVerify {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true) // 测试不校验 host key（生产路径由 client 侧 known_hosts/pin 负责）
    }
}

/// 起一个回环 echo TCP server，返回 (端口, 任务句柄)。
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

struct Fixture {
    gateway_port: u16,
    echo_port: u16,
    client_private_pem: String,
    _echo_task: tokio::task::JoinHandle<()>,
    _server_task: tokio::task::JoinHandle<()>,
}

/// 生成钥对、起 echo 与网关 server。
async fn setup(allowed_targets: &[String]) -> Fixture {
    let (host_key_pem, _) = keys::generate_ed25519().expect("host key");
    let (client_private_pem, client_public_line) = keys::generate_ed25519().expect("client key");
    let (echo_port, echo_task) = start_echo().await;

    let state = GatewayServerState::new(&host_key_pem, &client_public_line, allowed_targets)
        .expect("server state");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind gateway");
    let gateway_port = listener.local_addr().unwrap().port();
    let server_task = state.spawn_serving(listener);

    Fixture {
        gateway_port,
        echo_port,
        client_private_pem,
        _echo_task: echo_task,
        _server_task: server_task,
    }
}

async fn connect_port(gateway_port: u16) -> russh::client::Handle<NoVerify> {
    let config = Arc::new(russh::client::Config::default());
    russh::client::connect(config, ("127.0.0.1", gateway_port), NoVerify)
        .await
        .expect("connect")
}

async fn connect(fx: &Fixture) -> russh::client::Handle<NoVerify> {
    connect_port(fx.gateway_port).await
}

async fn authenticate(
    handle: &mut russh::client::Handle<NoVerify>,
    private_pem: &str,
) -> bool {
    let key = keys::decode_private_key(private_pem, None).expect("decode");
    let wrapped = russh::keys::key::PrivateKeyWithHashAlg::new(
        Arc::new(key),
        None, // ed25519 无需指定 hash
    )
    .expect("wrap");
    handle
        .authenticate_publickey("gateway-user", wrapped)
        .await
        .expect("auth rpc")
}

/// 经 direct-tcpip 打一段数据到 echo 并读回。
async fn echo_via_channel(
    handle: &russh::client::Handle<NoVerify>,
    target_port: u16,
    payload: &[u8],
) -> Result<Vec<u8>, russh::Error> {
    let mut channel = handle
        .channel_open_direct_tcpip("127.0.0.1", target_port.into(), "test-origin", 0)
        .await?;
    channel.data(payload).await?;
    channel.eof().await?;
    let mut buf = Vec::new();
    loop {
        match channel.wait().await {
            Some(russh::ChannelMsg::Data { data }) => buf.extend_from_slice(&data),
            Some(russh::ChannelMsg::Eof) | None => break,
            _ => {}
        }
        if buf.len() >= payload.len() {
            break;
        }
    }
    Ok(buf)
}

// ─── 认证面 ───

#[tokio::test]
async fn publickey_auth_and_direct_tcpip_roundtrip() {
    let fx = setup(&[]).await; // 白名单空 = 全放
    let mut handle = connect(&fx).await;
    assert!(
        authenticate(&mut handle, &fx.client_private_pem).await,
        "授权钥必须认证通过"
    );

    let payload = b"gateway-direct-tcpip-roundtrip";
    let echoed = echo_via_channel(&handle, fx.echo_port, payload)
        .await
        .expect("echo via gateway");
    assert_eq!(echoed, payload);
}

#[tokio::test]
async fn wrong_key_rejected() {
    let fx = setup(&[]).await;
    let mut handle = connect(&fx).await;
    let (other_pem, _) = keys::generate_ed25519().expect("other key");
    assert!(
        !authenticate(&mut handle, &other_pem).await,
        "未授权钥必须被拒"
    );
}

#[tokio::test]
async fn password_auth_unsupported() {
    let fx = setup(&[]).await;
    let mut handle = connect(&fx).await;
    // server 只宣告 publickey（MethodSet::PUBLICKEY），密码认证必拒
    let ok = handle
        .authenticate_password("gateway-user", "whatever")
        .await
        .expect("auth rpc");
    assert!(!ok, "密码认证必须被拒（无密码面）");
}

// ─── 通道拒绝面 ───

#[tokio::test]
async fn session_channel_rejected() {
    let fx = setup(&[]).await;
    let mut handle = connect(&fx).await;
    assert!(
        authenticate(&mut handle, &fx.client_private_pem).await,
        "先认证通过"
    );
    let result = handle.channel_open_session().await;
    assert!(result.is_err(), "网关不提供 session 通道（无 shell/sftp）");
}

// ─── 白名单 enforcement ───

#[tokio::test]
async fn allowed_targets_blocks_out_of_range() {
    // 只放行 10.0.0.0/8 —— echo 在 127.0.0.1，必须被拒
    let fx = setup(&["10.0.0.0/8".to_string()]).await;
    let mut handle = connect(&fx).await;
    assert!(authenticate(&mut handle, &fx.client_private_pem).await);
    let result = echo_via_channel(&handle, fx.echo_port, b"x").await;
    assert!(result.is_err(), "白名单外的目标必须拒绝建立通道");
}

#[tokio::test]
async fn allowed_targets_port_specific() {
    // 只放行 127.0.0.1/32:9999 —— echo 端口是随机的，几乎必然不等于 9999 → 拒；
    // 再用**实际 echo 端口**重建 server 放行 → 通（端口先起 echo 拿到再建规则）。
    let fx = setup(&["127.0.0.1/32:9999".to_string()]).await;
    let mut handle = connect(&fx).await;
    assert!(authenticate(&mut handle, &fx.client_private_pem).await);
    assert!(
        echo_via_channel(&handle, fx.echo_port, b"x").await.is_err(),
        "端口不匹配必须拒绝"
    );

    // 第二段：echo 端口先就位，规则精确引用它
    let (echo_port, _echo_task) = start_echo().await;
    let (host_key_pem, _) = keys::generate_ed25519().expect("host key");
    let (client_pem, client_pub) = keys::generate_ed25519().expect("client key");
    let state = GatewayServerState::new(
        &host_key_pem,
        &client_pub,
        &[format!("127.0.0.1/32:{echo_port}")],
    )
    .expect("server state");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let gw_port = listener.local_addr().unwrap().port();
    let _server_task = state.spawn_serving(listener);

    let mut handle2 = connect_port(gw_port).await;
    assert!(authenticate(&mut handle2, &client_pem).await);
    let payload = b"port-matched";
    let echoed = echo_via_channel(&handle2, echo_port, payload)
        .await
        .expect("精确端口匹配必须放行");
    assert_eq!(echoed, payload);
}

#[tokio::test]
async fn unresolvable_target_rejected_fail_closed() {
    // 白名单非空 + 目标域名解析失败（.invalid 保留 TLD）→ fail-closed 拒绝通道
    let fx = setup(&["127.0.0.1/32".to_string()]).await;
    let mut handle = connect(&fx).await;
    assert!(authenticate(&mut handle, &fx.client_private_pem).await);
    let result = handle
        .channel_open_direct_tcpip("no-such-host.invalid", 80, "test", 0)
        .await;
    assert!(result.is_err(), "解析失败必须 fail-closed 拒绝");}

// ─── 直连原始 TCP（不经 russh 的数据面旁证） ───

#[tokio::test]
async fn gateway_data_plane_large_payload() {
    // 拉大 payload（超过单 SSH 包），验证流式分片与窗口控制
    let fx = setup(&[]).await;
    let mut handle = connect(&fx).await;
    assert!(authenticate(&mut handle, &fx.client_private_pem).await);

    let payload: Vec<u8> = (0..256 * 1024).map(|i| (i % 251) as u8).collect();
    let mut channel = handle
        .channel_open_direct_tcpip("127.0.0.1", fx.echo_port.into(), "test", 0)
        .await
        .expect("open channel");

    // &[u8] 实现 tokio AsyncRead：整段流式发送（窗口 2MB，256KB 不会塞死回程）
    channel.data(payload.as_slice()).await.expect("write");
    channel.eof().await.expect("eof");

    let mut got = Vec::with_capacity(payload.len());
    loop {
        match channel.wait().await {
            Some(russh::ChannelMsg::Data { data }) => got.extend_from_slice(&data),
            Some(russh::ChannelMsg::Eof) | None => break,
            _ => {}
        }
    }
    assert_eq!(got.len(), payload.len(), "大 payload 必须完整往返");
    assert_eq!(got, payload);
}
