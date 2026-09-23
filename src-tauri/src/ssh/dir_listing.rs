//! 无会话回落的远程目录列举命令（v0.20 从 session.rs 拆出，行数红线）。
//!
//! `ssh_list_directory`：文件面板在资产无活跃会话时的一次性 SFTP 列目录
//! 通道——独立建连（交互式 handler：host key 事件可达 GUI）、列完即弃。
//! v0.20（2026-09-23）：SFTP 各阶段整体 30s 超时（此前全无超时，服务器 SFTP
//! 挂起时前端 busy 无限悬挂——真机验收发现的刷新死等）。

use super::session::connect_authenticated;
use super::*;


#[tauri::command]
pub async fn ssh_list_directory(
    state: State<'_, AppState>,
    host: String,
    port: u16,
    username: String,
    password: String,
    credential_id: Option<String>,
    auth_method: Option<String>,
    private_key_path: Option<String>,
    passphrase: Option<String>,
    passphrase_credential_id: Option<String>,
    private_key_credential_id: Option<String>,
    jump_host: Option<String>,
    connect_timeout_secs: Option<u32>,
    keepalive_interval_secs: Option<u32>,
    path: String,
) -> Result<RemoteDirectoryList, String> {
    let handle = connect_authenticated(
        &state,
        &host,
        port,
        &username,
        password,
        credential_id,
        auth_method,
        private_key_path,
        passphrase,
        passphrase_credential_id,
        private_key_credential_id,
        jump_host,
        connect_timeout_secs,
        keepalive_interval_secs,
    )
    .await?;

    // 走 SFTP 子系统列目录（对齐 sftp_list_dir 通道），替代旧 exec
    // `find -printf`：-printf 是 GNU 扩展，BusyBox/Alpine/BSD/macOS 上整个
    // 目录列表直接失败；且 %TY-%Tm-%Td 时间格式与 SFTP 路径的 epoch 秒
    // 不一致（前端按数字排序）。该连接本为一次性，SFTP 会话随连接丢弃。
    //
    // v0.20 修复（真机验收 2026-09-23）：SFTP 各阶段（开通道/子系统/会话初始化/
    // canonicalize/read_dir）**整体包 30s 超时**——此前全无超时，服务器 SFTP
    // 子系统挂起时前端 withFileOperation 的 busy 栈无限挂着（loading 遮罩 +
    // 刷新按钮永久禁用），而连接级 inactivity_timeout（默认 300s）远水救不了
    // 近火。超时报错文案如实，前端收到 reject 后走既有错误提示分支。
    const SFTP_STAGE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
    let sftp_stage = async {
        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| format!("SFTP channel open failed: {e}"))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| format!("SFTP subsystem request failed: {e}"))?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| format!("SFTP session init failed: {e}"))?;

        // 空 path = 服务器默认目录：canonicalize(".") 解析出真实绝对路径
        //（SFTP 服务进程 cwd 起始于登录用户家目录，与 sftp_list_dir 语义一致）。
        let requested_path = if path.trim().is_empty() {
            sftp.canonicalize(".")
                .await
                .map_err(|e| format!("SFTP canonicalize failed: {e}"))?
        } else {
            path
        };

        let mut entries: Vec<RemoteFileEntry> = sftp
            .read_dir(&requested_path)
            .await
            .map_err(|e| format!("SFTP read_dir failed: {e}"))?
            .into_iter()
            .map(dir_entry_to_remote_file_entry)
            .collect();

        // 排序与 sftp_list_dir 一致：目录优先，同类型按名称字母序。
        entries.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));

        Ok::<RemoteDirectoryList, String>(RemoteDirectoryList {
            host: host.clone(),
            path: requested_path,
            entries,
        })
    };
    tokio::time::timeout(SFTP_STAGE_TIMEOUT, sftp_stage)
        .await
        .map_err(|_| {
            let msg = format!("SFTP 列目录超时（{SFTP_STAGE_TIMEOUT:?}，服务器无响应或目录过大）: {host}");
            error!("ssh_list_directory (host {host}): {msg}");
            msg
        })?
        .inspect_err(|m| error!("ssh_list_directory (host {host}): {m}"))
}
