//! SFTP 元数据/单请求命令（mkdir/rename/remove/chmod/readlink/stat）。
//!
//! 从 ssh/sftp.rs 拆出（v0.20.2 SFTP 超时治理后 sftp.rs 触及 Rust 800 行硬上限；
//! 拆分方向见 architecture-log.md）。与 sftp.rs 共享通道缓存（get_or_create_sftp）
//! 与数据面超时包装（sftp_with_timeout，超时即淘汰缓存条目——见其注释），
//! 命令路径经 ssh.rs 的 `pub use sftp_meta::*` 保持 `ssh::sftp_*` 不变。

use super::*;

#[tauri::command]
pub async fn sftp_mkdir(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<(), String> {
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    sftp_with_timeout(&state, &session_id, "mkdir", async {
        let sftp = sftp_arc.lock().await;
        sftp.create_dir(&path)
            .await
            .map_err(|e| format!("SFTP mkdir failed: {e}"))
    })
    .await
    .inspect_err(|m| error!("sftp_mkdir (session {session_id}, path {path}): {m}"))
}

#[tauri::command]
pub async fn sftp_rename(
    state: State<'_, AppState>,
    session_id: String,
    old_path: String,
    new_path: String,
) -> Result<(), String> {
    // 只拦「寻址既有文件」的 old_path；new_path 是用户显式输入的新名字（v2.6 backlog #2）
    if myshelltool_core::is_lossy_remote_path(&old_path) {
        return Err(myshelltool_core::lossy_remote_path_error(&old_path));
    }
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    sftp_with_timeout(&state, &session_id, "rename", async {
        let sftp = sftp_arc.lock().await;
        sftp.rename(&old_path, &new_path)
            .await
            .map_err(|e| format!("SFTP rename failed: {e}"))
    })
    .await
    .inspect_err(|m| error!("sftp_rename (session {session_id}, {old_path} -> {new_path}): {m}"))
}

#[tauri::command]
pub async fn sftp_remove(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    kind: String,
) -> Result<(), String> {
    // 删除是最高危操作：失真名可能误删「字面含 U+FFFD 的另一个真实文件」，fail-closed
    if myshelltool_core::is_lossy_remote_path(&path) {
        return Err(myshelltool_core::lossy_remote_path_error(&path));
    }
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    sftp_with_timeout(&state, &session_id, "remove", async {
        let sftp = sftp_arc.lock().await;
        if kind == "directory" {
            sftp.remove_dir(&path).await
        } else {
            sftp.remove_file(&path).await
        }
        .map_err(|e| format!("SFTP remove failed: {e}"))
    })
    .await
    .inspect_err(|m| error!("sftp_remove (session {session_id}, path {path}): {m}"))
}

/// v0.20（SSH P2）：chmod——russh-sftp setstat 只改 permissions 位。
/// mode 输入是八进制字符串（如 "644"/"0755"）——前置解析校验（仅 0-7 数字，
/// 3-4 位；4 位时首位须 0——不静默截断错误输入）。高危：Strict 审批之外还应有
/// GUI 确认（远程列表面板右键入口带确认弹窗）。
#[tauri::command]
pub async fn sftp_chmod(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    mode: String,
) -> Result<(), String> {
    if myshelltool_core::is_lossy_remote_path(&path) {
        return Err(myshelltool_core::lossy_remote_path_error(&path));
    }
    let trimmed = mode.trim();
    if trimmed.len() < 3 || trimmed.len() > 4 || !trimmed.chars().all(|c| ('0'..='7').contains(&c)) {
        return Err(format!("权限格式无效：{mode:?}（期望 3-4 位八进制，如 644 或 0755）"));
    }
    if trimmed.len() == 4 && !trimmed.starts_with('0') {
        return Err(format!("权限格式无效：{mode:?}（4 位时首位须 0，setuid/setgid/sticky 位暂不支持）"));
    }
    let perms = u32::from_str_radix(trimmed, 8)
        .map_err(|e| format!("权限解析失败: {e}"))?;
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    sftp_with_timeout(&state, &session_id, "chmod", async {
        let sftp = sftp_arc.lock().await;
        let attrs = russh_sftp::protocol::FileAttributes {
            permissions: Some(perms),
            ..Default::default()
        };
        sftp.set_metadata(&path, attrs)
            .await
            .map_err(|e| format!("SFTP chmod 失败: {e}"))
    })
    .await
    .inspect_err(|m| error!("sftp_chmod (session {session_id}, path {path}): {m}"))
}

/// v0.20（SSH P2）：readlink——返回符号链接的目标路径（协议 raw Name）。
#[tauri::command]
pub async fn sftp_readlink(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<String, String> {
    if myshelltool_core::is_lossy_remote_path(&path) {
        return Err(myshelltool_core::lossy_remote_path_error(&path));
    }
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    sftp_with_timeout(&state, &session_id, "readlink", async {
        let sftp = sftp_arc.lock().await;
        sftp.read_link(&path)
            .await
            .map_err(|e| format!("SFTP readlink 失败: {e}"))
    })
    .await
    .inspect_err(|m| error!("sftp_readlink (session {session_id}, path {path}): {m}"))
}

#[tauri::command]
pub async fn sftp_stat(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<Option<RemoteFileEntry>, String> {
    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let meta = sftp_with_timeout(&state, &session_id, "stat", async {
        let sftp = sftp_arc.lock().await;
        match sftp.metadata(&path).await {
            Ok(meta) => Ok(Some(meta)),
            // NoSuchFile → Ok(None)（结构化三态，与 fs_local_stat 契约对齐）：前端
            // 覆盖确认门需要区分「明确不存在」与「无法确认」——此前前端对错误串
            // 做 /no such file/i 正则匹配（形状猜测），错误文案一变就会把「无法
            // 确认」误判成「不存在」，绕过覆盖确认静默覆盖远端文件。其余错误
            // （权限/超时/连接断）保持 Err，前端保守走确认路径（fail-closed）。
            Err(russh_sftp::client::error::Error::Status(status))
                if status.status_code == russh_sftp::protocol::StatusCode::NoSuchFile =>
            {
                Ok(None)
            }
            Err(e) => Err(format!("SFTP stat failed: {e}")),
        }
    })
    .await
    .inspect_err(|m| error!("sftp_stat (session {session_id}, path {path}): {m}"))?;

    let Some(meta) = meta else { return Ok(None) };

    let name = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&path)
        .to_string();

    Ok(Some(RemoteFileEntry {
        name,
        path,
        kind: if meta.is_dir() {
            "directory"
        } else if meta.is_symlink() {
            "symlink"
        } else {
            "file"
        }
        .to_string(),
        size: meta.len(),
        // Unix 秒字符串（复用 fs_local::format_modified），与 sftp_list_dir 对齐
        modified: crate::fs_local::format_modified(meta.modified()),
        permissions: meta.permissions.map(|p| format!("{:04o}", p & 0o7777)),
        user: meta.user.clone(),
        group: meta.group.clone(),
    }))
}
