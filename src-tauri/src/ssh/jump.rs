//! ProxyJump 单跳（v0.20，SSH P0-2）。
//!
//! ## 机制
//!
//! 目标资产带 `jump_host` 时，连接不再直连目标，而是：
//! ① 按主机标识解析跳板资产（host+port 匹配资产库，**复用其凭据与 host key
//!    信任**——跳板必须是已在 GUI 信任过的资产，与 headless 同策略）；
//! ② 正常 SSH 连上跳板（headless 语义：known_hosts 精确匹配，不弹窗）；
//! ③ `channel_open_direct_tcpip(target)` 开转发通道；
//! ④ `channel.into_stream()` 得到 AsyncRead+AsyncWrite 流；
//! ⑤ 目标的 SSH 握手跑在这条流上（russh `connect_stream`）。
//!
//! ## 保活假设（tauri:dev 验收项）
//!
//! 跳板的 `Handle` 在开完通道后即可 drop：russh 0.49 的连接由独立任务驱动，
//! Handle drop 不主动 disconnect；direct-tcpip 通道开着跳板连接就活着，
//! 目标连接关闭 → 通道关 → 跳板连接自然回收。若实测不符，改为把跳板
//! Handle 与目标 Handle 一起存放（调用方持有）。
//!
//! ## 边界（诚实声明）
//!
//! - **单跳**：跳板资产自身的 jump_host 被忽略（检测到则告警）；链式跳板
//!   （A over B over C）显式不支持。
//! - 主机标识只支持 `host` / `host:port`（端口缺省 22）；`user@host` 形态
//!   不支持（跳板登录用户 = 匹配资产的 username——资产库里已表达）。
//! - IPv6 字面量 `[::1]:22` 不解析（资产库里 host 字段单列，无需括号形态）。

use std::path::PathBuf;

use super::{HeadlessConnectParams, HeadlessSshClient};
use crate::ssh::SshSessionManager;

/// 解析跳板标识 "host" / "host:port" → (host, port)。
/// 尾部 :port 非法（非数字/超范围）报错——不静默当 22（配置错误要暴露）。
pub fn parse_jump_host(jump_host: &str) -> Result<(String, u16), String> {
    let s = jump_host.trim();
    if s.is_empty() {
        return Err("跳板主机标识为空".to_string());
    }
    if s.contains('@') || s.contains('/') {
        return Err(format!(
            "跳板主机标识 '{s}' 不支持 user@host / 路径形态（请填 host 或 host:port；跳板登录用户取资产库中该主机的资产）"
        ));
    }
    match s.rsplit_once(':') {
        Some((h, p)) => {
            let port: u16 = p
                .parse()
                .map_err(|_| format!("跳板端口非法（'{p}'，期望 1-65535）"))?;
            if port == 0 {
                return Err("跳板端口不能为 0".to_string());
            }
            Ok((h.to_string(), port))
        }
        None => Ok((s.to_string(), 22)),
    }
}

/// 经跳板开一条到目标的 direct-tcpip 流（目标 SSH 握手在其上跑）。
///
/// 跳板资产解析：host+port 全匹配优先；同主机多账号取第一个匹配（跳板的
/// 登录用户即该资产的 username，文档已声明）。
pub async fn open_direct_stream(
    jump_host: &str,
    target_host: &str,
    target_port: u16,
    asset_store_path: &std::path::Path,
    secret_store_dir: &std::path::Path,
    known_hosts_path: &std::path::Path,
) -> Result<russh::ChannelStream<russh::client::Msg>, String> {
    let (jump_addr, jump_port) = parse_jump_host(jump_host)?;

    let store = myshelltool_core::load_connection_asset_store(asset_store_path)
        .map_err(|e| format!("加载资产库失败（解析跳板）: {e}"))?;
    let jump_asset = store
        .assets
        .iter()
        .find(|a| a.host == jump_addr && a.port == jump_port)
        .ok_or_else(|| {
            format!(
                "跳板主机 {jump_addr}:{jump_port} 不在资产库中——请先添加该主机并完成一次连接（host key 信任），再启用跳板"
            )
        })?;

    // 链式跳板守卫：跳板自身再带 jump_host 属配置错误（显式不支持）
    if jump_asset.jump_host.as_deref().is_some_and(|j| !j.is_empty()) {
        log::warn!(
            "ProxyJump: 跳板资产 {} 自身配置了 jump_host（链式跳板不支持，已忽略）",
            jump_asset.id
        );
    }

    let params = HeadlessConnectParams {
        host: jump_asset.host.clone(),
        port: jump_asset.port,
        username: jump_asset.username.clone(),
        password: String::new(),
        credential_id: jump_asset.credential_id.clone(),
        auth_method: Some(format!("{:?}", jump_asset.auth_method)),
        private_key_path: jump_asset.private_key_path.clone(),
        private_key_credential_id: jump_asset.private_key_credential_id.clone(),
        passphrase: None,
        passphrase_credential_id: jump_asset.passphrase_credential_id.clone(),
        secret_store_dir: PathBuf::from(secret_store_dir),
        known_hosts_path: PathBuf::from(known_hosts_path),
        jump_host: None, // 跳板自身不再经跳板（链式不支持，上面已告警）
        connect_timeout_secs: jump_asset.connect_timeout_secs,
        keepalive_interval_secs: jump_asset.keepalive_interval_secs,
        asset_store_path: None,
    };

    // 跳板建连：直连 + 独立认证段（**不经 connect_headless_with**——本函数被
    // 它调用，绕经会构成 async 递归；host key 由 HeadlessSshClient 精确匹配）。
    let handler = HeadlessSshClient {
        host_port: format!("{jump_addr}:{jump_port}"),
        known_hosts_path: PathBuf::from(known_hosts_path),
    };
    let config = super::build_client_config_with(jump_asset.keepalive_interval_secs);
    let mut handle = russh::client::connect(
        config,
        (jump_asset.host.as_str(), jump_asset.port),
        handler,
    )
    .await
    .map_err(|e| format!("连接跳板 {jump_addr}:{jump_port} 失败: {e}"))?;
    super::authenticate_headless(&mut handle, &params)
        .await
        .map_err(|e| format!("跳板 {jump_addr}:{jump_port} 认证失败: {e}"))?;

    let channel = handle
        .channel_open_direct_tcpip(target_host.to_string(), target_port as u32, "127.0.0.1".to_string(), 0)
        .await
        .map_err(|e| format!("跳板转发通道打开失败（{target_host}:{target_port}）: {e}"))?;

    log::info!(
        "ProxyJump: {}:{} via {}:{} (direct-tcpip 已建立)",
        target_host,
        target_port,
        jump_addr,
        jump_port
    );
    // 跳板 Handle 就地 drop（保活假设见模块头注释；tauri:dev 验收覆盖）
    Ok(channel.into_stream())
}

/// 便捷：判断某资产是否配置了跳板（空串视为未配置——导入/旧数据防御）。
pub fn jump_host_of(asset: &myshelltool_core::ConnectionAsset) -> Option<&str> {
    asset
        .jump_host
        .as_deref()
        .map(str::trim)
        .filter(|j| !j.is_empty())
}

/// headless 连接参数的有效跳板（trim + 空串归 None，构造方防御）。
pub fn jump_host_ref(params: &HeadlessConnectParams) -> Option<&str> {
    params
        .jump_host
        .as_deref()
        .map(str::trim)
        .filter(|j| !j.is_empty())
}

// 引用 SshSessionManager 仅为文档锚点（跳板复用其认证语义），无运行时依赖。
const _: Option<std::sync::Arc<tokio::sync::Mutex<SshSessionManager>>> = None;
// HeadlessSshClient 经 connect_headless 内部使用，此处显式引用保持意图可见。
const _: Option<HeadlessSshClient> = None;
