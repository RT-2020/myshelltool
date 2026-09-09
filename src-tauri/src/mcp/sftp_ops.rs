//! Headless SFTP 操作底层封装（sftp_ops.rs）。
//!
//! 在无需 GUI AppState 的前提下，通过 connect_headless 获取 Handle，
//! 打开 SFTP 子系统会话，提供目录遍历、带限制/嗅探的文本读写、
//! 原子写、受控递归删除、大文件流式上传/下载与 SHA256 校验。

use std::path::{Path, PathBuf};

use russh::client;
use russh_sftp::client::SftpSession;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::ssh::{self, HeadlessConnectParams, HeadlessSshClient};
use super::file_policy::normalize_remote_path;
use super::tools::McpToolContext;

/// 目录条目结构体（与 GUI RemoteFileEntry 同构的精简版）。
#[derive(Debug, Clone, Serialize)]
pub struct SftpListEntry {
    pub name: String,
    pub path: String,
    pub kind: String, // "file" | "directory" | "symlink"
    pub size: u64,
    pub modified: String,
    pub permissions: Option<String>,
}

/// 文件元数据简报（用于覆盖前检查等场景）。
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct SftpFileMeta {
    pub exists: bool,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
}

/// Headless SFTP 会话上下文容器。
pub struct HeadlessSftpSession {
    // 保持 handle 活跃，避免 session channel 提前关闭
    _handle: client::Handle<HeadlessSshClient>,
    pub sftp: SftpSession,
}

impl HeadlessSftpSession {
    /// 根据资产 ID 从资产库解析凭据并建立 headless SFTP 会话。
    pub async fn connect(ctx: &McpToolContext, asset_id: &str) -> Result<Self, String> {
        let store = myshelltool_core::load_connection_asset_store(&ctx.asset_store_path)
            .map_err(|e| format!("加载资产库失败: {e}"))?;
        let asset = store
            .assets
            .iter()
            .find(|a| a.id == asset_id)
            .ok_or_else(|| format!("资产 {} 不存在", asset_id))?;

        log::info!(
            "HeadlessSftpSession::connect to {}@{}:{} (asset={})",
            asset.username,
            asset.host,
            asset.port,
            asset_id
        );

        let params = HeadlessConnectParams {
            host: asset.host.clone(),
            port: asset.port,
            username: asset.username.clone(),
            password: String::new(),
            credential_id: asset.credential_id.clone(),
            auth_method: Some(format!("{:?}", asset.auth_method)),
            private_key_path: asset.private_key_path.clone(),
            passphrase: None,
            passphrase_credential_id: asset.passphrase_credential_id.clone(),
            secret_store_dir: ctx.secret_store_dir.clone(),
            known_hosts_path: ctx.known_hosts_path.clone(),
        };

        let handle = ssh::connect_headless(&params)
            .await
            .map_err(|e| format!("SFTP SSH 建连失败 ({asset_id}): {e}"))?;

        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| format!("SFTP channel open 失败: {e}"))?;

        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| format!("SFTP subsystem 请求失败: {e}"))?;

        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| format!("SFTP session 初始化失败: {e}"))?;

        Ok(Self {
            _handle: handle,
            sftp,
        })
    }

    /// 查询路径元数据（若不存在返回 Ok(None)）。
    #[allow(dead_code)]
    pub async fn stat(&self, path: &str) -> Result<Option<SftpFileMeta>, String> {
        let norm_path = normalize_remote_path(path);
        match self.sftp.metadata(&norm_path).await {
            Ok(meta) => Ok(Some(SftpFileMeta {
                exists: true,
                is_dir: meta.is_dir(),
                size: meta.len(),
                modified: crate::fs_local::format_modified(meta.modified()),
            })),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("no such file") || err_str.contains("not found") {
                    Ok(None)
                } else {
                    Err(format!("SFTP metadata failed: {e}"))
                }
            }
        }
    }

    /// 遍历远程目录。
    pub async fn list_dir(&self, path: &str) -> Result<Vec<SftpListEntry>, String> {
        let norm_path = normalize_remote_path(path);
        let requested_path = if norm_path.is_empty() {
            ".".to_string()
        } else {
            norm_path
        };

        let raw_entries = self
            .sftp
            .read_dir(&requested_path)
            .await
            .map_err(|e| format!("SFTP read_dir 读取目录失败 ({requested_path}): {e}"))?;

        let mut entries: Vec<SftpListEntry> = raw_entries
            .into_iter()
            .map(|entry| {
                let meta = entry.metadata();
                let kind = if meta.is_dir() {
                    "directory"
                } else if meta.is_symlink() {
                    "symlink"
                } else {
                    "file"
                }
                .to_string();

                let permissions = meta.permissions.map(|p| format!("{:04o}", p & 0o7777));

                SftpListEntry {
                    name: entry.file_name(),
                    path: entry.path(),
                    kind,
                    size: meta.len(),
                    modified: crate::fs_local::format_modified(meta.modified()),
                    permissions,
                }
            })
            .collect();

        // 排序：目录优先，同类型按名称字母序
        entries.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
        Ok(entries)
    }

    /// 删除文件或目录。若为非空目录且未设置 recursive 则拒绝删除。
    pub async fn remove_path(&self, path: &str, recursive: bool) -> Result<(), String> {
        let norm_path = normalize_remote_path(path);
        let meta = self
            .sftp
            .metadata(&norm_path)
            .await
            .map_err(|e| format!("目标路径不存在或无法获取元数据: {e}"))?;

        if meta.is_dir() {
            if recursive {
                self.recursive_remove_dir(&norm_path, 0).await?;
            } else {
                self.sftp
                    .remove_dir(&norm_path)
                    .await
                    .map_err(|e| format!("删除目录失败（若为非空目录，需设置 recursive=true）: {e}"))?;
            }
        } else {
            self.sftp
                .remove_file(&norm_path)
                .await
                .map_err(|e| format!("删除文件失败: {e}"))?;
        }

        Ok(())
    }

    /// 递归删除目录（限制最大深度 16 层以防止循环或过深栈）。
    fn recursive_remove_dir<'a>(
        &'a self,
        dir_path: &'a str,
        depth: usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            if depth > 16 {
                return Err(format!("递归删除超出最大安全深度限制 (16 层): {dir_path}"));
            }

            let entries = self
                .sftp
                .read_dir(dir_path)
                .await
                .map_err(|e| format!("遍历待删除子目录失败 ({dir_path}): {e}"))?;

            for entry in entries {
                let name = entry.file_name();
                if name == "." || name == ".." {
                    continue;
                }
                let child_path = format!("{}/{}", dir_path.trim_end_matches('/'), name);
                let child_meta = entry.metadata();
                if child_meta.is_dir() && !child_meta.is_symlink() {
                    self.recursive_remove_dir(&child_path, depth + 1).await?;
                } else {
                    self.sftp
                        .remove_file(&child_path)
                        .await
                        .map_err(|e| format!("删除子文件失败 ({child_path}): {e}"))?;
                }
            }

            self.sftp
                .remove_dir(dir_path)
                .await
                .map_err(|e| format!("删除目录失败 ({dir_path}): {e}"))?;

            Ok(())
        })
    }

    /// 读取小文本文件（上限 max_bytes，包含 512B 二进制探测）。
    pub async fn read_file_limited(&self, path: &str, max_bytes: usize) -> Result<String, String> {
        let norm_path = normalize_remote_path(path);
        let meta = self
            .sftp
            .metadata(&norm_path)
            .await
            .map_err(|e| format!("无法读取文件元数据 ({norm_path}): {e}"))?;

        if meta.is_dir() {
            return Err(format!("目标路径是目录而不是文件: {norm_path}"));
        }

        if meta.len() as usize > max_bytes {
            return Err(format!(
                "文件过大（{} 字节，单次读取上限 {} 字节）。请改用 sftp_download 工具下载到本地后处理。",
                meta.len(),
                max_bytes
            ));
        }

        let mut file = self
            .sftp
            .open(&norm_path)
            .await
            .map_err(|e| format!("打开文件失败 ({norm_path}): {e}"))?;

        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .await
            .map_err(|e| format!("读取文件内容失败: {e}"))?;

        // 探测前 512 字节是否包含 NUL 字符
        let sniff_len = buf.len().min(512);
        if buf[..sniff_len].contains(&0) {
            return Err("检测到目标文件为二进制内容（包含空字节 NUL），文本读取工具不支持，请改用 sftp_download 下载到本地。".to_string());
        }

        Ok(String::from_utf8_lossy(&buf).to_string())
    }

    /// 原子写入文件（写入 `<path>.myshelltool.<uuid>.tmp` 后原子 rename）。
    pub async fn write_file_atomic(&self, path: &str, content: &[u8]) -> Result<usize, String> {
        let norm_path = normalize_remote_path(path);
        let temp_suffix = uuid::Uuid::new_v4().to_string();
        let temp_path = format!("{norm_path}.myshelltool.{temp_suffix}.tmp");

        let mut file = self
            .sftp
            .create(&temp_path)
            .await
            .map_err(|e| format!("创建临时文件失败 ({temp_path}): {e}"))?;

        file.write_all(content)
            .await
            .map_err(|e| format!("写入临时文件失败: {e}"))?;

        file.shutdown()
            .await
            .map_err(|e| format!("刷新文件失败: {e}"))?;

        // 原子重命名
        if let Err(e) = self.sftp.rename(&temp_path, &norm_path).await {
            // 失败时尽力清理临时文件
            let _ = self.sftp.remove_file(&temp_path).await;
            return Err(format!("原子替换文件失败 (rename to {norm_path}): {e}"));
        }

        Ok(content.len())
    }

    /// 流式上传本机文件至远端（临时文件 + rename + SHA256 校验）。
    pub async fn upload_stream(
        &self,
        local_path: &Path,
        remote_path: &str,
    ) -> Result<(u64, String), String> {
        let norm_remote = normalize_remote_path(remote_path);
        let mut local_file = tokio::fs::File::open(local_path)
            .await
            .map_err(|e| format!("打开本机待上传文件失败 ({}): {e}", local_path.display()))?;

        let temp_suffix = uuid::Uuid::new_v4().to_string();
        let temp_remote = format!("{norm_remote}.myshelltool.upload.{temp_suffix}.tmp");

        let mut remote_file = self
            .sftp
            .create(&temp_remote)
            .await
            .map_err(|e| format!("创建远端上传临时文件失败 ({temp_remote}): {e}"))?;

        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024]; // 64KB 缓冲区
        let mut total_bytes: u64 = 0;

        loop {
            let n = local_file
                .read(&mut buffer)
                .await
                .map_err(|e| format!("读取本机文件失败: {e}"))?;
            if n == 0 {
                break;
            }
            remote_file
                .write_all(&buffer[..n])
                .await
                .map_err(|e| format!("写入远端文件失败: {e}"))?;
            hasher.update(&buffer[..n]);
            total_bytes += n as u64;
        }

        remote_file
            .shutdown()
            .await
            .map_err(|e| format!("刷新远端文件失败: {e}"))?;

        if let Err(e) = self.sftp.rename(&temp_remote, &norm_remote).await {
            let _ = self.sftp.remove_file(&temp_remote).await;
            return Err(format!("上传完成重命名失败 (rename to {norm_remote}): {e}"));
        }

        let sha256_hex = format!("{:x}", hasher.finalize());
        Ok((total_bytes, sha256_hex))
    }

    /// 流式下载远端文件至本机（临时文件 + rename + SHA256 校验）。
    pub async fn download_stream(
        &self,
        remote_path: &str,
        local_path: &Path,
    ) -> Result<(u64, String), String> {
        let norm_remote = normalize_remote_path(remote_path);

        // 确保本机父目录存在
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("创建本机目标父目录失败 ({}): {e}", parent.display()))?;
        }

        let temp_local_name = format!(
            "{}.myshelltool.download.{}.tmp",
            local_path.file_name().unwrap_or_default().to_string_lossy(),
            uuid::Uuid::new_v4()
        );
        let temp_local_path = local_path
            .parent()
            .map(|p| p.join(&temp_local_name))
            .unwrap_or_else(|| PathBuf::from(&temp_local_name));

        let mut remote_file = self
            .sftp
            .open(&norm_remote)
            .await
            .map_err(|e| format!("打开远端待下载文件失败 ({norm_remote}): {e}"))?;

        let mut local_file = tokio::fs::File::create(&temp_local_path)
            .await
            .map_err(|e| format!("创建本机下载临时文件失败 ({}): {e}", temp_local_path.display()))?;

        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut total_bytes: u64 = 0;

        loop {
            let n = remote_file
                .read(&mut buffer)
                .await
                .map_err(|e| format!("读取远端文件数据失败: {e}"))?;
            if n == 0 {
                break;
            }
            local_file
                .write_all(&buffer[..n])
                .await
                .map_err(|e| format!("写入本机临时文件失败: {e}"))?;
            hasher.update(&buffer[..n]);
            total_bytes += n as u64;
        }

        local_file
            .flush()
            .await
            .map_err(|e| format!("刷新本机文件失败: {e}"))?;
        drop(local_file);

        if let Err(e) = tokio::fs::rename(&temp_local_path, local_path).await {
            let _ = tokio::fs::remove_file(&temp_local_path).await;
            return Err(format!("下载完成重命名失败 (rename to {}): {e}", local_path.display()));
        }

        let sha256_hex = format!("{:x}", hasher.finalize());
        Ok((total_bytes, sha256_hex))
    }
}
