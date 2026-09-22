//! SSH 隧道/端口转发（本地/远程/SOCKS5 动态转发）。
//! 从 ssh.rs 按域拆出（architecture-log Target 1），零逻辑变更。

use super::*;

// --- Tunnel / port-forwarding types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    pub id: String,
    pub name: String,
    pub kind: String, // "local", "remote", "dynamic"
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: String,
    pub remote_port: u16,
    pub session_id: String,
    pub auto_start: bool,
    /// 远程转发的认证上下文来源（本地资产库 id，与 MCP exec_on_asset 同一解析
    /// 路径）。local/dynamic 复用会话句柄不需要它；remote 用专用连接，必须携带。
    #[serde(default)]
    pub asset_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TunnelStatus {
    pub id: String,
    pub config: TunnelConfig,
    pub active: bool,
    pub error: Option<String>,
}

// --- Tunnel / port-forwarding commands ---

#[tauri::command]
pub async fn tunnel_create(
    state: State<'_, AppState>,
    config: TunnelConfig,
) -> Result<TunnelStatus, String> {
    let id = config.id.clone();
    let status = TunnelStatus {
        id: id.clone(),
        config,
        active: false,
        error: None,
    };
    let mut mgr = state.ssh_sessions.lock().await;
    mgr.tunnels.insert(id, status.clone());
    Ok(status)
}

#[tauri::command]
pub async fn tunnel_start(
    state: State<'_, AppState>,
    session_id: String,
    tunnel_id: String,
) -> Result<(), String> {
    let (handle, config, app) = {
        let mgr = state.ssh_sessions.lock().await;
        let handle = mgr
            .ssh_handles
            .get(&session_id)
            .ok_or_else(|| format!("SSH session {session_id} not found"))?
            .clone();
        let tunnel = mgr
            .tunnels
            .get(&tunnel_id)
            .ok_or_else(|| format!("Tunnel {tunnel_id} not found"))?;
        if tunnel.active {
            return Err("Tunnel is already active".to_string());
        }
        (handle, tunnel.config.clone(), mgr.app.clone())
    };

    let join_handle = match config.kind.as_str() {
        "local" => start_local_forward(handle, config.clone(), app).await?,
        // remote 用专用连接（russh 0.49 的 tcpip_forward 要 &mut Handle，共享的
        // Arc<Handle> 给不出——详见 start_remote_forward 注释），不吃会话 handle。
        "remote" => start_remote_forward(&state, config.clone()).await?,
        "dynamic" => start_dynamic_forward(handle, config.clone(), app).await?,
        other => return Err(format!("Unknown tunnel kind: {other}")),
    };

    {
        let mut mgr = state.ssh_sessions.lock().await;
        if let Some(status) = mgr.tunnels.get_mut(&tunnel_id) {
            status.active = true;
            status.error = None;
        }
        mgr.tunnel_handles.insert(tunnel_id.clone(), join_handle);
        info!("tunnel {tunnel_id} ({}) started", config.kind);
    }

    Ok(())
}

#[tauri::command]
pub async fn tunnel_stop(
    state: State<'_, AppState>,
    tunnel_id: String,
) -> Result<(), String> {
    let mut mgr = state.ssh_sessions.lock().await;
    if let Some(jh) = mgr.tunnel_handles.remove(&tunnel_id) {
        jh.abort();
        info!("tunnel {tunnel_id} stopped");
    }
    if let Some(status) = mgr.tunnels.get_mut(&tunnel_id) {
        status.active = false;
    }
    Ok(())
}

#[tauri::command]
pub async fn tunnel_list(
    state: State<'_, AppState>,
) -> Result<Vec<TunnelStatus>, String> {
    let mgr = state.ssh_sessions.lock().await;
    Ok(mgr.tunnels.values().cloned().collect())
}

#[tauri::command]
pub async fn tunnel_delete(
    state: State<'_, AppState>,
    tunnel_id: String,
) -> Result<(), String> {
    let mut mgr = state.ssh_sessions.lock().await;
    if let Some(jh) = mgr.tunnel_handles.remove(&tunnel_id) {
        jh.abort();
    }
    mgr.tunnels.remove(&tunnel_id);
    Ok(())
}

// --- Local port forwarding ---

/// 双向泵：SSH channel ↔ 本地 TCP 流（local/dynamic/remote 三种转发的数据面同构）。
///
/// `channel.wait()` 借 `&mut self`，与 `channel.data()` 不能分属两个并发 future，
/// 因此读写交错在同一个 select 循环里。TCP 侧 EOF：给 channel 发 eof 后退出；
/// channel 侧 Eof/None（含连接断开）：直接退出。调用方负责各自的事件 emit。
async fn pipe_tcp_over_channel<R, W>(
    mut channel: russh::Channel<client::Msg>,
    mut tcp_read: R,
    mut tcp_write: W,
) where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut tcp_buf = vec![0u8; 32 * 1024];
    loop {
        tokio::select! {
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        if tcp_write.write_all(&data).await.is_err() {
                            break;
                        }
                    }
                    Some(ChannelMsg::Eof) | None => break,
                    _ => {}
                }
            }
            n = tcp_read.read(&mut tcp_buf) => {
                match n {
                    Ok(0) => {
                        let _ = channel.eof().await;
                        break;
                    }
                    Ok(n) => {
                        if channel.data(&tcp_buf[..n]).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }
}

async fn start_local_forward(
    handle: Arc<client::Handle<SshClient>>,
    config: TunnelConfig,
    app: AppHandle,
) -> Result<tokio::task::JoinHandle<()>, String> {
    let listen_addr = format!("{}:{}", config.local_addr, config.local_port);
    let listener = tokio::net::TcpListener::bind(&listen_addr)
        .await
        .map_err(|e| format!("Failed to bind {listen_addr}: {e}"))?;

    let remote_addr = config.remote_addr.clone();
    let remote_port = config.remote_port as u32;
    let tunnel_id = config.id.clone();

    let jh = tokio::spawn(async move {
        loop {
            let tcp_stream = match listener.accept().await {
                Ok((s, _)) => s,
                Err(_) => continue,
            };

            let ssh_handle = handle.clone();
            let remote_addr = remote_addr.clone();
            let app = app.clone();
            let tid = tunnel_id.clone();

            tokio::spawn(async move {
                let channel = match ssh_handle
                    .channel_open_direct_tcpip(&remote_addr, remote_port, "127.0.0.1", 0)
                    .await
                {
                    Ok(ch) => ch,
                    Err(_) => return,
                };
                let (tcp_read, tcp_write) = tokio::io::split(tcp_stream);
                pipe_tcp_over_channel(channel, tcp_read, tcp_write).await;
                let _ = app.emit(&format!("tunnel-traffic-{tid}"), "connection-closed");
            });
        }
    });

    Ok(jh)
}

// --- Remote port forwarding ---
//
// 实现路径（russh 0.49 API 约束驱动）：`Handle::tcpip_forward` 需要 `&mut Handle`，
// 而会话句柄在连接成功后即进入共享的 `Arc<Handle>`（GUI 会话/SFTP/隧道/监控共用），
// 无法再取得 &mut。因此远程转发使用**专用连接**（connect_headless_with 注入
// RemoteForwardClient，复用 headless 认证/known_hosts 策略）：隧道任务独占持有
// Handle，tcpip_forward 可用；服务器上有人连接被转发端口时，sshd 在该连接上回开
// forwarded-tcpip 通道，由 handler 接管并桥接到本机目标。
//
// 安全边界：
// - host key 用 headless 同款「仅 known_hosts 精确匹配」策略（后台连接不弹窗，
//   未信任/已变更一律拒绝并给出明确错误）；
// - forwarded-tcpip 只在「端口与已登记转发匹配、且地址与请求绑定地址相容」时才
//   接管——恶意服务器主动开未请求的转发通道（借客户端探测/攻击本机服务）一律
//   丢弃并记日志。russh 在调回调前已 confirm 通道（encrypted.rs 的 confirm() 在
//   handler 之前），回调返回 Err 会打死整条会话——所以「拒绝」= 不接管直接 drop。

/// 远程转发专用 handler。
struct RemoteForwardClient {
    host_port: String,
    known_hosts_path: PathBuf,
    /// 已登记转发的期望（归一化绑定地址 + 实际绑定端口）：tcpip_forward 成功后
    /// 写入；写入前（或不匹配时）到达的 forwarded-tcpip 一律丢弃。
    expected: Arc<std::sync::Mutex<Option<(String, u32)>>>,
    /// 转发目标（本机侧）：到达的连接桥接到 local_addr:local_port。
    target_addr: String,
    target_port: u16,
    app: AppHandle,
    tunnel_id: String,
}

/// 请求绑定地址与服务器回报的 forwarded-tcpip 地址比对：
/// - localhost 归一为 127.0.0.1（服务器回报字面常是解析后的地址）；
/// - 通配请求（0.0.0.0 / :: / 空）只按端口匹配（回报地址是服务器实际监听面）。
fn bind_addr_matches(requested: &str, connected: &str) -> bool {
    fn norm(a: &str) -> &str {
        if a == "localhost" { "127.0.0.1" } else { a }
    }
    let req = norm(requested);
    req == norm(connected) || req == "0.0.0.0" || req == "::" || req.is_empty()
}

#[async_trait]
impl client::Handler for RemoteForwardClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(check_known_host_exact(
            &self.host_port,
            &self.known_hosts_path,
            server_public_key,
        ))
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
            // 未登记的转发通道：不接管，drop 即关（永不返回 Err——见模块注释）。
            warn!(
                "remote forward {}: dropping unregistered forwarded-tcpip channel {}:{} (originator {}:{})",
                self.tunnel_id, connected_address, connected_port, originator_address, originator_port
            );
            return Ok(());
        }
        let target = format!("{}:{}", self.target_addr, self.target_port);
        let app = self.app.clone();
        let tid = self.tunnel_id.clone();
        let origin = format!("{originator_address}:{originator_port}");
        tokio::spawn(async move {
            let _ = app.emit(&format!("tunnel-traffic-{tid}"), format!("connected:{origin}"));
            match tokio::net::TcpStream::connect(&target).await {
                Ok(tcp) => {
                    let (tcp_read, tcp_write) = tokio::io::split(tcp);
                    pipe_tcp_over_channel(channel, tcp_read, tcp_write).await;
                    let _ = app.emit(&format!("tunnel-traffic-{tid}"), "connection-closed");
                }
                Err(e) => {
                    // 本机目标不可达：channel 随作用域 drop，对端连接随即被关闭。
                    let _ = app.emit(
                        &format!("tunnel-error-{tid}"),
                        format!("connect local target {target} failed: {e}"),
                    );
                }
            }
        });
        Ok(())
    }
}

async fn start_remote_forward(
    state: &State<'_, AppState>,
    config: TunnelConfig,
) -> Result<tokio::task::JoinHandle<()>, String> {
    // 认证上下文来自本地资产库（config.asset_id），与 MCP exec_on_asset 同一解析
    // 路径——不靠 host:port 反猜资产（同主机多资产会歧义）。
    let asset_id = config.asset_id.clone().ok_or_else(|| {
        "远程转发需要资产引用（隧道配置缺 asset_id，请删除后重新创建该隧道）".to_string()
    })?;
    let (secret_store_dir, known_hosts_path, app) = {
        let mgr = state.ssh_sessions.lock().await;
        (
            mgr.secret_store_dir.clone(),
            mgr.known_hosts_path.clone(),
            mgr.app.clone(),
        )
    };
    let store = myshelltool_core::load_connection_asset_store(&state.asset_store_path)
        .map_err(|e| format!("加载资产库失败: {e}"))?;
    let asset = store
        .assets
        .iter()
        .find(|a| a.id == asset_id)
        .ok_or_else(|| format!("资产 {asset_id} 不存在（隧道创建后资产已被删除？）"))?;

    let expected = Arc::new(std::sync::Mutex::new(None::<(String, u32)>));
    let handler = RemoteForwardClient {
        host_port: format!("{}:{}", asset.host, asset.port),
        known_hosts_path: known_hosts_path.clone(),
        expected: expected.clone(),
        target_addr: config.local_addr.clone(),
        target_port: config.local_port,
        app: app.clone(),
        tunnel_id: config.id.clone(),
    };
    let params = HeadlessConnectParams {
        host: asset.host.clone(),
        port: asset.port,
        username: asset.username.clone(),
        password: String::new(), // 从 credential_id 读
        credential_id: asset.credential_id.clone(),
        auth_method: Some(format!("{:?}", asset.auth_method)),
        private_key_path: asset.private_key_path.clone(),
        // v0.20（B0）：凭据库托管私钥内容（与 GUI 同优先级：SecretStore 优先、文件兜底）
        private_key_credential_id: asset.private_key_credential_id.clone(),
        passphrase: None,
        passphrase_credential_id: asset.passphrase_credential_id.clone(),
        secret_store_dir,
        known_hosts_path,        // v0.20（SSH P0-2）：跳板透传
        jump_host: crate::ssh::jump_host_of(asset).map(str::to_string),
        connect_timeout_secs: asset.connect_timeout_secs,
        keepalive_interval_secs: asset.keepalive_interval_secs,
        asset_store_path: Some(state.asset_store_path.clone()),
    };
    let mut handle = connect_headless_with(&params, handler)
        .await
        .map_err(|e| format!("远程转发专用连接失败: {e}"))?;

    // 请求服务器在 remote_addr:remote_port 监听（端口 0 = 服务器分配，回包给实际端口）。
    // 拒绝的典型原因：AllowTcpForwarding=remote 未开 / GatewayPorts 限制非回环绑定。
    let bind_addr = config.remote_addr.clone();
    let requested_port = config.remote_port as u32;
    let assigned = handle
        .tcpip_forward(bind_addr.clone(), requested_port)
        .await
        .map_err(|e| {
            format!(
                "远端端口转发请求被拒绝（{bind_addr}:{requested_port}）: {e}（检查服务器 sshd 的 AllowTcpForwarding/GatewayPorts）"
            )
        })?;
    let bound_port = if requested_port == 0 { assigned } else { requested_port };
    // 登记期望：此后到达的匹配 forwarded-tcpip 才被接管。
    if let Ok(mut guard) = expected.lock() {
        *guard = Some((bind_addr, bound_port));
    }

    let tunnel_id = config.id.clone();
    let ssh_sessions = state.ssh_sessions.clone();
    let jh = tokio::spawn(async move {
        // 驻守任务：转发通道的接管全在 handler（russh 会话循环驱动），这里只做
        // 活性监视——连接死了（网络断/服务器关）隧道即失效，回写状态后退出
        // （tunnel_list 不能继续谎报 active）。
        // 停止路径（tunnel_stop / tunnel_delete / cleanup_session_tables）abort
        // 本任务 → drop Handle → TCP 断开，服务器侧监听随连接消失（SSH 语义：
        // 转发绑定跟随连接生命周期），无需显式 cancel_tcpip_forward。
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            tick.tick().await;
            if handle.is_closed() {
                let _ = app.emit(
                    &format!("tunnel-error-{tunnel_id}"),
                    "远程转发连接已断开".to_string(),
                );
                // 回写状态（条目可能已被 cleanup 摘掉——no-op 可重入）。
                let mut mgr = ssh_sessions.lock().await;
                if let Some(status) = mgr.tunnels.get_mut(&tunnel_id) {
                    status.active = false;
                    status.error = Some("远程转发连接已断开".to_string());
                }
                break;
            }
        }
    });
    Ok(jh)
}

// --- Dynamic (SOCKS5) port forwarding ---

async fn start_dynamic_forward(
    handle: Arc<client::Handle<SshClient>>,
    config: TunnelConfig,
    app: AppHandle,
) -> Result<tokio::task::JoinHandle<()>, String> {
    let listen_addr = format!("{}:{}", config.local_addr, config.local_port);
    let listener = tokio::net::TcpListener::bind(&listen_addr)
        .await
        .map_err(|e| format!("Failed to bind {listen_addr}: {e}"))?;

    let tunnel_id = config.id.clone();

    let jh = tokio::spawn(async move {
        loop {
            let tcp_stream = match listener.accept().await {
                Ok((s, _)) => s,
                Err(_) => continue,
            };

            let ssh_handle = handle.clone();
            let app = app.clone();
            let tid = tunnel_id.clone();

            tokio::spawn(async move {
                if let Err(e) =
                    handle_socks5_connection(tcp_stream, ssh_handle, app.clone(), &tid).await
                {
                    let _ = app.emit(&format!("tunnel-error-{tid}"), format!("SOCKS5 error: {e}"));
                }
            });
        }
    });

    Ok(jh)
}

async fn handle_socks5_connection(
    tcp_stream: tokio::net::TcpStream,
    handle: Arc<client::Handle<SshClient>>,
    app: AppHandle,
    tunnel_id: &str,
) -> Result<(), String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (mut tcp_read, mut tcp_write) = tokio::io::split(tcp_stream);

    // SOCKS5 handshake: read version and auth methods
    let mut buf = [0u8; 2];
    tcp_read
        .read_exact(&mut buf)
        .await
        .map_err(|e| format!("SOCKS5 read version: {e}"))?;
    if buf[0] != 0x05 {
        return Err("Not a SOCKS5 request".to_string());
    }
    let nmethods = buf[1] as usize;
    let mut methods = vec![0u8; nmethods];
    tcp_read
        .read_exact(&mut methods)
        .await
        .map_err(|e| format!("SOCKS5 read methods: {e}"))?;

    // RFC 1928：必须校验客户端提供的方法列表。本实现只支持免认证（0x00），
    // 客户端未提供 0x00 时回 0xFF（NO ACCEPTABLE METHODS）并断开——不能
    // 不读不判就应答 0x00，那等于对要求认证的客户端谎报「无需认证」。
    if !methods.contains(&0x00) {
        let _ = tcp_write.write_all(&[0x05, 0xFF]).await;
        return Err(
            "SOCKS5 客户端未提供免认证方法（0x00），本服务端不支持认证，已按 RFC 1928 回 0xFF 断开".to_string(),
        );
    }

    // Reply: no auth required
    tcp_write
        .write_all(&[0x05, 0x00])
        .await
        .map_err(|e| format!("SOCKS5 write method: {e}"))?;

    // Read connect request
    let mut head = [0u8; 4];
    tcp_read
        .read_exact(&mut head)
        .await
        .map_err(|e| format!("SOCKS5 read request: {e}"))?;
    if head[0] != 0x05 || head[1] != 0x01 {
        // Only support CONNECT (0x01)
        tcp_write
            .write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .await
            .ok();
        return Err("SOCKS5 only supports CONNECT".to_string());
    }

    let target_addr = match head[3] {
        // IPv4
        0x01 => {
            let mut addr = [0u8; 4];
            tcp_read
                .read_exact(&mut addr)
                .await
                .map_err(|e| format!("SOCKS5 read IPv4: {e}"))?;
            format!("{}.{}.{}.{}", addr[0], addr[1], addr[2], addr[3])
        }
        // Domain name
        0x03 => {
            let mut len_buf = [0u8; 1];
            tcp_read
                .read_exact(&mut len_buf)
                .await
                .map_err(|e| format!("SOCKS5 read domain len: {e}"))?;
            let len = len_buf[0] as usize;
            let mut domain = vec![0u8; len];
            tcp_read
                .read_exact(&mut domain)
                .await
                .map_err(|e| format!("SOCKS5 read domain: {e}"))?;
            String::from_utf8_lossy(&domain).to_string()
        }
        // IPv6
        0x04 => {
            let mut addr = [0u8; 16];
            tcp_read
                .read_exact(&mut addr)
                .await
                .map_err(|e| format!("SOCKS5 read IPv6: {e}"))?;
            addr.iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .chunks(2)
                .map(|c| c.join(""))
                .collect::<Vec<_>>()
                .join(":")
        }
        _ => {
            tcp_write
                .write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await
                .ok();
            return Err("Unsupported SOCKS5 address type".to_string());
        }
    };

    let mut port_buf = [0u8; 2];
    tcp_read
        .read_exact(&mut port_buf)
        .await
        .map_err(|e| format!("SOCKS5 read port: {e}"))?;
    let target_port = u16::from_be_bytes(port_buf) as u32;

    // Open SSH direct TCP/IP channel to the target
    let channel = match handle
        .channel_open_direct_tcpip(&target_addr, target_port, "127.0.0.1", 0)
        .await
    {
        Ok(ch) => ch,
        Err(e) => {
            // SOCKS5 reply: connection refused
            tcp_write
                .write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await
                .ok();
            return Err(format!("SSH channel open failed: {e}"));
        }
    };

    // SOCKS5 success reply
    tcp_write
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
        .map_err(|e| format!("SOCKS5 write success: {e}"))?;

    let _ = app.emit(
        &format!("tunnel-traffic-{tunnel_id}"),
        format!("connected:{target_addr}:{target_port}"),
    );

    // Bidirectional copy：三种转发共用一个泵（pipe_tcp_over_channel）。
    pipe_tcp_over_channel(channel, tcp_read, tcp_write).await;

    Ok(())
}
