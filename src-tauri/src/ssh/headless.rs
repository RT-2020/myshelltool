//! Headless 会话支持（MCP 用，D4：仅服务已在 GUI 信任过的资产）。
//! 从 ssh.rs 按域拆出（architecture-log Target 1），零逻辑变更。

use super::*;

// ─── Headless 会话支持（MCP 进程用，D4：仅服务已在 GUI 信任过的资产）───
//
// 与 GUI 的 SshSessionManager 路径完全隔离，零回归风险：
// - HeadlessSshClient 不持 AppHandle（无法弹窗），check_server_key 对
//   known_hosts 未记录/变更的主机直接返回 Ok(false) 拒绝连接。
// - connect_headless 接收裸参数（不依赖 State/AppState）。
// - exec_command_once 走 channel_open_session + exec，一次性返回结构化输出
//   （exit_code/stdout/stderr，见 ExecOnceOutput），不建立持久 PTY，
//   符合只读工具（df/uptime/...）的语义。

/// Headless SSH Handler（MCP 进程用）。
pub struct HeadlessSshClient {
    host_port: String,
    known_hosts_path: PathBuf,
}

/// 后台连接（headless / 远程转发）的 host key 判定：仅 known_hosts 精确匹配才
/// 接受，未记录/变更一律拒绝（无弹窗通道，D4 不静默信任）。
/// 返回 true = 匹配受信。日志在此处统一打（调用方不再各自重复）。
pub(crate) fn check_known_host_exact(
    host_port: &str,
    known_hosts_path: &PathBuf,
    server_public_key: &russh::keys::ssh_key::PublicKey,
) -> bool {
    let key_bytes = server_public_key.public_key_bytes();
    let key_hex = bytes_to_hex(&key_bytes);
    let known = load_known_hosts(known_hosts_path);

    if let Some(entry) = known.get(host_port) {
        if entry.key_hex == key_hex {
            info!("headless check_server_key: {host_port} matched known_hosts, accepting");
            return true;
        }
        warn!("headless check_server_key: {host_port} key mismatch (expected {}, got {key_hex})", entry.key_hex);
    } else {
        warn!("headless check_server_key: {host_port} not in known_hosts, rejecting (headless mode requires pre-trust via GUI)");
    }
    false
}

#[async_trait::async_trait]
impl client::Handler for HeadlessSshClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // D4：headless 模式不弹窗，未知/变更主机直接拒绝。
        Ok(check_known_host_exact(
            &self.host_port,
            &self.known_hosts_path,
            server_public_key,
        ))
    }
}

/// Headless 连接参数（不依赖 Tauri State）。
pub struct HeadlessConnectParams {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub credential_id: Option<String>,
    pub auth_method: Option<String>,
    pub private_key_path: Option<String>,
    pub passphrase: Option<String>,
    pub passphrase_credential_id: Option<String>,
    pub secret_store_dir: PathBuf,
    pub known_hosts_path: PathBuf,
}

/// Headless 建连（MCP 进程用）。
///
/// 复用 connect_authenticated 的认证逻辑（密码/私钥/keyboard-interactive
/// 自动响应密码类 prompt），但不走 GUI 弹窗。host key 未信任由
/// HeadlessSshClient 直接拒绝。
pub async fn connect_headless(
    params: &HeadlessConnectParams,
) -> Result<client::Handle<HeadlessSshClient>, String> {
    let handler = HeadlessSshClient {
        host_port: format!("{}:{}", params.host, params.port),
        known_hosts_path: params.known_hosts_path.clone(),
    };
    connect_headless_with(params, handler).await
}

/// 泛型版 headless 建连：handler 由调用方注入（远程转发用它挂上
/// forwarded-tcpip 接管，见 tunnel.rs RemoteForwardClient）。
/// 认证流程与 connect_headless 完全一致（密码/私钥/keyboard-interactive 密码类
/// prompt 自动响应），host key 策略由注入的 handler 决定。
pub async fn connect_headless_with<H>(
    params: &HeadlessConnectParams,
    handler: H,
) -> Result<client::Handle<H>, String>
where
    H: client::Handler<Error = russh::Error> + Send + 'static,
{
    let config = build_client_config();
    let host_port = format!("{}:{}", params.host, params.port);

    let mut handle = client::connect(config, (params.host.as_str(), params.port), handler)
        .await
        .map_err(|e| format!("SSH connect failed: {e}"))?;

    // 解析密码（从凭据存储或参数）
    let resolved_password: Option<String> = if params.auth_method.as_deref() == Some("PrivateKey") {
        None
    } else if params.password.is_empty() {
        if let Some(ref cred_id) = params.credential_id {
            Some(
                myshelltool_core::SecretStore::new(&params.secret_store_dir, Box::new(crate::dpapi_codec::DpapiCodec))
                    .read(cred_id.as_str())
                    .map_err(|e| format!("Failed to read credential: {e}"))?
                    .ok_or_else(|| "Stored credential not found".to_string())?,
            )
        } else {
            return Err("No password provided and no stored credential".to_string());
        }
    } else {
        Some(params.password.clone())
    };

    // 认证
    let auth_ok = if params.auth_method.as_deref() == Some("PrivateKey") {
        let key_path = params
            .private_key_path
            .as_deref()
            .unwrap_or("~/.ssh/id_ed25519");
        let expanded = expand_home_path(key_path)?;
        let resolved_passphrase = if let Some(ref cred_id) = params.passphrase_credential_id {
            myshelltool_core::SecretStore::new(&params.secret_store_dir, Box::new(crate::dpapi_codec::DpapiCodec))
                .read(cred_id.as_str())
                .map_err(|e| format!("Failed to read passphrase: {e}"))?
        } else {
            params.passphrase.as_deref().and_then(|p| {
                if p.is_empty() {
                    None
                } else {
                    Some(p.to_string())
                }
            })
        };
        let key_data = std::fs::read(&expanded)
            .map_err(|e| format!("Failed to read key file '{}': {e}", expanded))?;
        let key_str = String::from_utf8_lossy(&key_data);
        let key_pair = russh::keys::decode_secret_key(&key_str, resolved_passphrase.as_deref())
            .map_err(|e| format!("Failed to load private key: {e}"))?;
        let key_with_hash = wrap_key_with_preferred_hash(key_pair)?;
        handle
            .authenticate_publickey(&params.username, key_with_hash)
            .await
            .map_err(|e| format!("Public key auth failed: {e}"))?
    } else {
        handle
            .authenticate_password(
                &params.username,
                resolved_password.as_deref().unwrap_or(""),
            )
            .await
            .map_err(|e| format!("Auth failed: {e}"))?
    };

    if !auth_ok {
        // headless: 尝试 keyboard-interactive，但只用密码自动响应；
        // 非密码类 prompt（MFA 等）无法弹窗，直接失败。
        let resp = handle
            .authenticate_keyboard_interactive_start(&params.username, None::<String>)
            .await
            .map_err(|e| format!("Keyboard-interactive start failed: {e}"))?;
        match resp {
            client::KeyboardInteractiveAuthResponse::Success => {}
            client::KeyboardInteractiveAuthResponse::Failure => {
                return Err("Authentication failed".to_string());
            }
            client::KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                let all_password_like = !prompts.is_empty()
                    && prompts.iter().all(|p| {
                        let lower = p.prompt.to_lowercase();
                        lower.contains("password")
                            || lower.contains("passphrase")
                            || lower.contains("密码")
                    });
                if all_password_like && resolved_password.is_some() {
                    let pwd = resolved_password.as_deref().unwrap();
                    let responses: Vec<String> = prompts.iter().map(|_| pwd.to_string()).collect();
                    let resp = handle
                        .authenticate_keyboard_interactive_respond(responses)
                        .await
                        .map_err(|e| format!("Keyboard-interactive respond failed: {e}"))?;
                    match resp {
                        client::KeyboardInteractiveAuthResponse::Success => {}
                        _ => {
                            return Err(
                                "Authentication failed (keyboard-interactive)".to_string()
                            );
                        }
                    }
                } else {
                    return Err(
                        "Headless mode cannot handle non-password interactive prompts (MFA etc.); please connect via GUI first"
                            .to_string(),
                    );
                }
            }
        }
    }

    info!("headless auth succeeded for {}@{}", params.username, host_port);
    Ok(handle)
}

/// exec_command_once 的结构化输出（v2.1）。
///
/// exit_code 修复现行时序 bug：旧实现 `ChannelMsg::Eof | ExitStatus { .. } => break`
/// 使 Eof（几乎总先于 ExitStatus 到达）提前退出循环，exit_status 分支不可达，
/// 调用方永远拿不到退出码。现拆开处理（见 exec_command_once）。
#[derive(Debug, Clone)]
pub struct ExecOnceOutput {
    /// 远端命令退出码（SSH 协议 ExitStatus；通道异常关闭/连接断开时为 None）。
    pub exit_code: Option<u32>,
    /// stdout（UTF-8 lossy 解码，原始字节不做拼接加工）。
    pub stdout: String,
    /// stderr（同上；拼接展示逻辑由调用方（tools.rs）负责）。
    pub stderr: String,
}

/// 在已认证的会话上执行一次性命令，返回结构化输出（exit_code/stdout/stderr）。
///
/// 走 channel_open_session + exec，命令执行完通道即关闭。
/// 适用于只读查询（df/uptime/systemctl status 等），不适合交互式程序。
///
/// 消息时序（v2.1 修复）：`Eof` 只表示数据流结束，退出码可能还在后面，
/// **不再 break**；`ExitStatus` 记录退出码后 break（此后一般跟 Close）；
/// `Close` 或循环自然结束（wait 返回 None，通道关闭/连接断开）也结束。
pub async fn exec_command_once(
    handle: &client::Handle<HeadlessSshClient>,
    command: &str,
) -> Result<ExecOnceOutput, String> {
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|e| format!("Channel open failed: {e}"))?;
    channel
        .exec(true, command.to_string())
        .await
        .map_err(|e| format!("Command exec failed: {e}"))?;

    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    let mut exit_code: Option<u32> = None;
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } => stdout_buf.extend_from_slice(&data),
            ChannelMsg::ExtendedData { data, ext: _ } => stderr_buf.extend_from_slice(&data),
            // Eof 仅标志数据流结束，ExitStatus/Close 可能还在后面，继续等。
            ChannelMsg::Eof => {}
            ChannelMsg::ExitStatus { exit_status } => {
                exit_code = Some(exit_status);
                break;
            }
            ChannelMsg::Close => break,
            _ => {}
        }
    }

    Ok(ExecOnceOutput {
        exit_code,
        stdout: String::from_utf8_lossy(&stdout_buf).to_string(),
        stderr: String::from_utf8_lossy(&stderr_buf).to_string(),
    })
}
