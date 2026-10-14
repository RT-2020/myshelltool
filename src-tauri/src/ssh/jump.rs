//! ProxyJump 链式跳板（v0.21 B3；单跳为 v0.20 SSH P0-2）。
//!
//! ## 机制（循环展开，无递归）
//!
//! 目标资产带 `jump_host` 时，链由资产库嵌套表达（target.jump_host=j1，
//! j1 资产.jump_host=j2，最内层为空）。[`open_direct_stream`] 先用
//! `core::jump_chain::resolve_jump_chain` 展开链（环/深度/缺资产 fail-closed
//! 判据在 core 真跑单测），再逐跳建立：
//! ① 第一跳直连 `client::connect` + headless 认证；
//! ② 第 2..N 跳在上一跳 handle 上 `channel_open_direct_tcpip` → 通道流上
//!    `client::connect_stream` 握手 + 认证（host key 按该跳自己的 host:port
//!    精确匹配 known_hosts，跳板必须是已在 GUI 信任过的资产）；
//! ③ 最末跳开 direct-tcpip 到目标，目标 SSH 握手由调用方在这条流上跑。
//! 旧单跳版「跳板建连绕开 connect_headless_with 防互相递归」的特殊结构被
//! 循环展开天然取代。
//!
//! ## 每跳超时（v0.21）
//!
//! 每跳的 connect/connect_stream + 认证整体纳入该跳资产
//! `connect_timeout_secs` 的 timeout 包裹（None 不包，语义与 headless.rs
//! 直连分支一致）。最末跳开目标通道不包（开在已认证连接上，失败路径明确，
//! keepalive 兜底半开）。
//!
//! ## 保活假设（tauri:dev 验收项）
//!
//! 中间跳板的 `Handle` 在开完通道后即可 drop：russh 0.49 连接由独立任务
//! 驱动，Handle drop 不主动 disconnect；direct-tcpip 通道开着跳板连接就活着。
//! 多跳下任一中间跳断开则下游全部断链——真机验收覆盖。
//!
//! ## 边界（诚实声明）
//!
//! - 链上限 [`myshelltool_core::jump_chain::MAX_JUMP_HOPS`]（4 跳）；
//! - 主机标识只支持 `host` / `host:port`（端口缺省 22）；`user@host` /
//!   IPv6 字面量 / 逗号链形态不支持（跳板登录用户 = 匹配资产的 username）；
//! - 每会话独立建链：N 跳 = N 条 SSH 连接（服务器 MaxStartups 配额视角）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use myshelltool_core::jump_chain::{describe_chain, resolve_jump_chain};

use super::{build_client_config_with, authenticate_headless, HeadlessConnectParams, HeadlessSshClient};

/// 从跳板资产构造 headless 认证参数（凭据引用照搬该资产——链的每一跳
/// 复用自己的凭据与 host key 信任，这是 headless 语义的根基）。
fn params_from_asset(
    asset: &myshelltool_core::ConnectionAsset,
    secret_store_dir: &Path,
    known_hosts_path: &Path,
) -> HeadlessConnectParams {
    HeadlessConnectParams {
        host: asset.host.clone(),
        port: asset.port,
        username: asset.username.clone(),
        password: String::new(),
        credential_id: asset.credential_id.clone(),
        auth_method: Some(format!("{:?}", asset.auth_method)),
        private_key_path: asset.private_key_path.clone(),
        private_key_credential_id: asset.private_key_credential_id.clone(),
        passphrase: None,
        passphrase_credential_id: asset.passphrase_credential_id.clone(),
        secret_store_dir: PathBuf::from(secret_store_dir),
        known_hosts_path: PathBuf::from(known_hosts_path),
        jump_host: None,
        connect_timeout_secs: asset.connect_timeout_secs,
        keepalive_interval_secs: asset.keepalive_interval_secs,
        asset_store_path: None,
    }
}

/// 经跳板链开一条到目标的 direct-tcpip 流（目标 SSH 握手在其上跑）。
///
/// 链解析与环/深度/缺资产判据在 `core::jump_chain::resolve_jump_chain`
/// （fail-closed，单测真跑）；本函数只做逐跳建连。错误信息带链位置
/// （「跳板链 t→j1→j2：第 2 跳 …」）。
pub async fn open_direct_stream(
    jump_host: &str,
    target_host: &str,
    target_port: u16,
    asset_store_path: &std::path::Path,
    secret_store_dir: &std::path::Path,
    known_hosts_path: &std::path::Path,
) -> Result<russh::ChannelStream<russh::client::Msg>, String> {
    let store = myshelltool_core::load_connection_asset_store(asset_store_path)
        .map_err(|e| format!("加载资产库失败（解析跳板链）: {e}"))?;
    let hops = resolve_jump_chain(&store.assets, jump_host, target_host, target_port)?;
    let chain_desc = describe_chain(target_host, target_port, &hops);

    // 逐跳建连：第一跳直连，后续跳在上一跳的 direct-tcpip 流上握手。
    // handle 在开完下一跳通道后就地重赋值（旧 Handle drop，保活假设见模块头）。
    let mut handle: Option<russh::client::Handle<HeadlessSshClient>> = None;
    for (i, hop) in hops.iter().enumerate() {
        let pos = format!("{chain_desc}：第 {} 跳 {}:{}", i + 1, hop.host, hop.port);
        let params = params_from_asset(hop, secret_store_dir, known_hosts_path);
        let handler = HeadlessSshClient {
            host_port: format!("{}:{}", hop.host, hop.port),
            known_hosts_path: PathBuf::from(known_hosts_path),
        };
        let config = build_client_config_with(hop.keepalive_interval_secs);
        let prev = handle.take();

        let hop_fut = async {
            let mut h = match prev {
                Some(prev) => {
                    let channel = prev
                        .channel_open_direct_tcpip(
                            hop.host.clone(),
                            hop.port as u32,
                            "127.0.0.1".to_string(),
                            0,
                        )
                        .await
                        .map_err(|e| format!("{pos} 转发通道打开失败: {e}"))?;
                    russh::client::connect_stream(config, channel.into_stream(), handler)
                        .await
                        .map_err(|e| format!("{pos} SSH 握手失败: {e}"))?
                }
                None => russh::client::connect(config, (hop.host.as_str(), hop.port), handler)
                    .await
                    .map_err(|e| format!("{pos} 连接失败: {e}"))?,
            };
            authenticate_headless(&mut h, &params)
                .await
                .map_err(|e| format!("{pos} 认证失败: {e}"))?;
            Ok::<_, String>(h)
        };
        handle = Some(match hop.connect_timeout_secs {
            Some(secs) => tokio::time::timeout(Duration::from_secs(secs as u64), hop_fut)
                .await
                .map_err(|_| format!("{pos} 超时（{secs}s，含建连+认证）"))??,
            None => hop_fut.await?,
        });
        log::info!("ProxyJump: {pos} 已建立");
    }

    let handle = handle
        .ok_or_else(|| "跳板链为空（内部错误：resolve_jump_chain 返回空链）".to_string())?;
    let channel = handle
        .channel_open_direct_tcpip(target_host.to_string(), target_port as u32, "127.0.0.1".to_string(), 0)
        .await
        .map_err(|e| format!("跳板链 {chain_desc}：打开目标通道失败（{target_host}:{target_port}）: {e}"))?;
    log::info!(
        "ProxyJump: {}:{} 经 {} 跳链建立（direct-tcpip 已通）",
        target_host,
        target_port,
        hops.len()
    );
    // 最末跳 Handle 就地 drop（保活假设见模块头注释；真机验收覆盖）
    Ok(channel.into_stream())
}

/// 便捷：判断某资产是否配置了跳板（薄委托 core——MCP/隧道 4 处入口共用，
/// 保持既有调用路径 `crate::ssh::jump_host_of` 稳定）。
pub fn jump_host_of(asset: &myshelltool_core::ConnectionAsset) -> Option<&str> {
    myshelltool_core::jump_chain::jump_host_of(asset)
}

/// headless 连接参数的有效跳板（trim + 空串归 None，构造方防御）。
pub fn jump_host_ref(params: &HeadlessConnectParams) -> Option<&str> {
    params
        .jump_host
        .as_deref()
        .map(str::trim)
        .filter(|j| !j.is_empty())
}
