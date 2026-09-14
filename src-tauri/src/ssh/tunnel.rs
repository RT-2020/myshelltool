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
        "remote" => start_remote_forward(handle, config.clone()).await?,
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
                let mut channel = match ssh_handle
                    .channel_open_direct_tcpip(&remote_addr, remote_port, "127.0.0.1", 0)
                    .await
                {
                    Ok(ch) => ch,
                    Err(_) => return,
                };

                let (mut tcp_read, mut tcp_write) = tokio::io::split(tcp_stream);

                // Use a single loop with select, reading from SSH channel and TCP concurrently.
                // channel.wait() borrows &mut self, so we can't have it in a separate future
                // alongside channel.data(). Instead, interleave reads and writes in one task.
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

                let _ = app.emit(&format!("tunnel-traffic-{tid}"), "connection-closed");
            });
        }
    });

    Ok(jh)
}

// --- Remote port forwarding ---

async fn start_remote_forward(
    _handle: Arc<client::Handle<SshClient>>,
    _config: TunnelConfig,
) -> Result<tokio::task::JoinHandle<()>, String> {
    Err(
        "Remote forwarding is not yet supported. Use local or dynamic forwarding instead."
            .to_string(),
    )
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
    let mut channel = match handle
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

    // Bidirectional copy using single select loop to avoid borrow conflicts
    // on the SSH channel (wait() takes &mut self, data() takes &self)
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

    Ok(())
}
