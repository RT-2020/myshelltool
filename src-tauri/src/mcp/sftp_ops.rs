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

// ─── 超时档位（v0.20.2）───
// MCP 工具调用在死/半开连接上永久挂起 = host 的 call_tool 永久 pending。
// headless 会话没有 GUI 的 sftp_cache 可淘汰，超时即 fail-fast：会话随调用方
// drop 关闭，池中半开 Handle 由下次 connect 的 channel open 探测（带超时）淘汰。

/// 建链三步（channel open → subsystem → SFTP init）整体上限——三步全无内建
/// 超时，半开 TCP 上永久挂起（GUI `get_or_create_sftp` 的 15s 同款处置）。
const SFTP_INIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
/// 单命令（list/读写小文件/rename 等短操作）整段上限。
const SFTP_OP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
/// 递归删除整段上限：目录大小无上限（GUI 目录传输限 2000 文件，MCP 无此限），
/// 60s 会误杀大目录，放宽到 300s 换「有界」。
const SFTP_RECURSIVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
/// 流式传输单块 I/O 上限（与 GUI WRITE_TIMEOUT 同档）。
const SFTP_IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

async fn sftp_timeout<T, F>(limit: std::time::Duration, op: &str, fut: F) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, String>>,
{
    match tokio::time::timeout(limit, fut).await {
        Ok(result) => result,
        Err(_) => Err(format!(
            "SFTP {op} 超时（{}s）：连接可能已半开或远端无响应，请重试",
            limit.as_secs()
        )),
    }
}

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

/// Headless SFTP 会话上下文容器。
pub struct HeadlessSftpSession {
    // 保持 handle 活跃，避免 session channel 提前关闭。
    // v0.20（B3）起是 Arc：池化连接与 session 共享同一份（drop session 不关连接）。
    _handle: std::sync::Arc<client::Handle<HeadlessSshClient>>,
    pub sftp: SftpSession,
}

/// 在已打开的 session channel 上协商 SFTP 子系统（池命中与新建两条路径共用）。
/// channel 按值传入（into_stream 消耗它）。
async fn open_sftp_on(
    channel: russh::Channel<russh::client::Msg>,
    asset_id: &str,
) -> Result<SftpSession, String> {
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| format!("SFTP subsystem 请求失败 ({asset_id}): {e}"))?;
    SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| format!("SFTP session 初始化失败 ({asset_id}): {e}"))
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

        // v0.20（B1）scope 收口点②：全部文件类工具的必经之路（与 exec_on_asset
        // 的判定语义一致，双层防旁路；server.rs 预判记 scopeResult 日志）。
        let scope_snapshot = ctx.config.read().await.scope.clone();
        let verdict = myshelltool_core::mcp_scope::evaluate(&scope_snapshot, asset);
        if !verdict.is_allowed() {
            log::warn!(
                "HeadlessSftpSession::connect: asset {} denied by scope ({})",
                asset_id,
                verdict.as_str()
            );
            return Err(myshelltool_core::mcp_scope::denied_message(verdict));
        }

        // v0.20（B3）：headless 连接池复用。命中（凭据快照一致 + 未死）直接开
        // SFTP 子系统；未命中新建后回池。Handle 经 Arc 共享——session drop 不关
        // 连接（池里还持有一份），淘汰时才真正断开。
        // v0.20.2：建链三步套 15s 上限——池的 is_closed() 只反映本地状态，半开
        // TCP 上恒为 false，channel open 带超时同时充当池条目的半开探测：
        // 超时即 invalidate 重建，不再把挂死连接反复发给后续工具调用。
        let auth = super::session_pool::AuthSnapshot::from_asset(asset);
        if let Some(handle) = super::session_pool::get(asset_id, &auth).await {
            let attempt = async {
                let channel = handle
                    .channel_open_session()
                    .await
                    .map_err(|e| format!("pooled channel open failed: {e}"))?;
                open_sftp_on(channel, asset_id).await
            };
            match tokio::time::timeout(SFTP_INIT_TIMEOUT, attempt).await {
                Ok(Ok(sftp)) => {
                    log::info!("HeadlessSftpSession::connect: pooled connection for {asset_id}");
                    return Ok(Self {
                        _handle: handle,
                        sftp,
                    });
                }
                Ok(Err(e)) => {
                    log::warn!("HeadlessSftpSession::connect: pooled connection for {asset_id} unusable ({e}), reconnecting");
                    super::session_pool::invalidate_asset(asset_id).await;
                }
                Err(_) => {
                    log::warn!(
                        "HeadlessSftpSession::connect: pooled connection for {asset_id} timed out (half-open?), reconnecting"
                    );
                    super::session_pool::invalidate_asset(asset_id).await;
                }
            }
        }

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
            // v0.20（B0）：凭据库托管私钥内容（与 GUI 同优先级：SecretStore 优先、文件兜底）
            private_key_credential_id: asset.private_key_credential_id.clone(),
            passphrase: None,
            passphrase_credential_id: asset.passphrase_credential_id.clone(),
            secret_store_dir: ctx.secret_store_dir.clone(),
            known_hosts_path: ctx.known_hosts_path.clone(),            // v0.20（SSH P0-2）：跳板透传
            jump_host: crate::ssh::jump_host_of(asset).map(str::to_string),
            connect_timeout_secs: asset.connect_timeout_secs,
            keepalive_interval_secs: asset.keepalive_interval_secs,
            asset_store_path: Some(ctx.asset_store_path.clone()),
        };

        let handle = std::sync::Arc::new(
            ssh::connect_headless(&params)
                .await
                .map_err(|e| format!("SFTP SSH 建连失败 ({asset_id}): {e}"))?,
        );

        let channel = tokio::time::timeout(SFTP_INIT_TIMEOUT, handle.channel_open_session())
            .await
            .map_err(|_| format!("SFTP channel open 超时 ({asset_id})"))?
            .map_err(|e| format!("SFTP channel open 失败: {e}"))?;

        let sftp = tokio::time::timeout(SFTP_INIT_TIMEOUT, open_sftp_on(channel, asset_id))
            .await
            .map_err(|_| format!("SFTP session 初始化超时 ({asset_id})"))??;

        // v0.20（B3）：健康连接回池（后续同资产文件工具免握手）
        super::session_pool::put(asset_id, auth, handle.clone()).await;

        Ok(Self {
            _handle: handle,
            sftp,
        })
    }

    /// 遍历远程目录。
    pub async fn list_dir(&self, path: &str) -> Result<Vec<SftpListEntry>, String> {
        sftp_timeout(SFTP_OP_TIMEOUT, "list_dir", async {
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
        })
        .await
    }

    /// 删除文件或目录。若为非空目录且未设置 recursive 则拒绝删除。
    pub async fn remove_path(&self, path: &str, recursive: bool) -> Result<(), String> {
        sftp_timeout(SFTP_RECURSIVE_TIMEOUT, "remove", async {
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
        })
        .await
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
        sftp_timeout(SFTP_OP_TIMEOUT, "read_file", async {
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

            // 编码判定放在 core（`decode_remote_text`）：旧实现直接 `from_utf8_lossy`，
            // GBK/GB18030 内容被静默换成 U+FFFD **且返回成功**，前 512 字节无 NUL 的
            // 二进制也被当文本「成功」读出——用户/审计拿到的是看起来正常、实则损坏的内容。
            // 现在三态：文本 / 二进制（引导用下载工具）/ 非 UTF-8（报首个非法字节位置）。
            match myshelltool_core::remote_text::decode_remote_text(&buf) {
                myshelltool_core::remote_text::RemoteTextOutcome::Text { text, has_bom } => {
                    if has_bom {
                        // BOM 会破坏下游 JSON/YAML 解析，这里剥掉但留日志（不静默改变内容语义）
                        log::debug!("sftp_read_file: 目标文件带 UTF-8 BOM，已剥离首部 U+FEFF");
                        return Ok(text.trim_start_matches('\u{feff}').to_string());
                    }
                    Ok(text)
                }
                myshelltool_core::remote_text::RemoteTextOutcome::Binary(reason) => Err(format!(
                    "检测到目标文件为二进制内容（{reason}），文本读取工具不支持，请改用 sftp_download 下载到本地处理。"
                )),
                myshelltool_core::remote_text::RemoteTextOutcome::NotUtf8(reason) => Err(format!(
                    "目标文件不是 UTF-8 文本（{reason}）。文本工具不做有损替换——直接替换会把内容读成乱码却报告成功。\
                     请改用 sftp_download 下载后按源编码处理，或先转码（如 iconv -f GBK -t UTF-8）再读取。"
                )),
            }
        })
        .await
    }

    /// 范围读取（v0.20/B2）：从 `offset` 起读至多 `max_bytes` 字节。
    ///
    /// 与 `read_file_limited` 的差异：
    /// - 不做严格编码三态判定——**续读段可能从多字节字符中间开始**，严格判定
    ///   会把边界切分误报成「非 UTF-8」。调用方已用首次读取（offset=0 走
    ///   read_file_limited 的严格路径）确认过文件是文本；续读段用 lossy 解码，
    ///   边界处最多首尾各一个替换符，响应文本里如实说明。
    /// - 返回 (文本段, 文件总大小)，调用方组装分页提示（nextOffset）。
    pub async fn read_file_range(
        &self,
        path: &str,
        offset: u64,
        max_bytes: usize,
    ) -> Result<(String, u64), String> {
        sftp_timeout(SFTP_OP_TIMEOUT, "read_file_range", async {
            use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

            let norm_path = normalize_remote_path(path);
            let meta = self
                .sftp
                .metadata(&norm_path)
                .await
                .map_err(|e| format!("无法读取文件元数据 ({norm_path}): {e}"))?;
            if meta.is_dir() {
                return Err(format!("目标路径是目录而不是文件: {norm_path}"));
            }
            let total = meta.len();

            let mut file = self
                .sftp
                .open(&norm_path)
                .await
                .map_err(|e| format!("打开文件失败 ({norm_path}): {e}"))?;
            if offset > 0 {
                file.seek(SeekFrom::Start(offset))
                    .await
                    .map_err(|e| format!("定位读取偏移失败 (offset={offset}): {e}"))?;
            }

            // read 不保证填满缓冲：循环到 EOF 或达上限（64 KiB 一块，与下载链路口径一致）
            let mut buf = Vec::with_capacity(max_bytes.min(4 * 1024 * 1024));
            let mut taken = 0usize;
            while taken < max_bytes {
                let want = 64 * 1024.min(max_bytes - taken);
                let mut chunk = vec![0u8; want];
                let n = file
                    .read(&mut chunk)
                    .await
                    .map_err(|e| format!("读取文件内容失败: {e}"))?;
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                taken += n;
            }
            Ok((String::from_utf8_lossy(&buf).to_string(), total))
        })
        .await
    }

    /// rename 覆盖兜底（write_file_atomic / upload_stream 共用）。
    ///
    /// v0.18 起实现提取到 `ssh::text_file::sftp_rename_with_overwrite_fallback`
    /// （GUI 编辑器原子写与 MCP 共用同一份语义），此处仅委托——两边的
    /// 「失败不删 temp」「remove 后重试」等关键行为见该函数注释。
    async fn rename_with_overwrite_fallback(&self, temp: &str, target: &str) -> Result<(), String> {
        crate::ssh::sftp_rename_with_overwrite_fallback(&self.sftp, temp, target).await
    }

    /// 原子写入文件（写入 `<path>.myshelltool.<uuid>.tmp` 后原子 rename）。
    pub async fn write_file_atomic(&self, path: &str, content: &[u8]) -> Result<usize, String> {
        sftp_timeout(SFTP_OP_TIMEOUT, "write_file", async {
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

            // 原子重命名（含覆盖兜底）。失败时**不删 temp**：temp 是新数据唯一
            // 副本（覆盖兜底失败时原文件可能已被 remove），删了就是双重丢失；
            // temp 路径已写进错误信息，指引用户手动恢复。
            if let Err(e) = self.rename_with_overwrite_fallback(&temp_path, &norm_path).await {
                return Err(e);
            }

            Ok(content.len())
        })
        .await
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

        let mut remote_file = sftp_timeout(SFTP_OP_TIMEOUT, "upload create", async {
            self.sftp
                .create(&temp_remote)
                .await
                .map_err(|e| format!("创建远端上传临时文件失败 ({temp_remote}): {e}"))
        })
        .await?;

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
            // 单块写上限（对齐 GUI WRITE_TIMEOUT）：死连接上 write_all 永不返回
            match tokio::time::timeout(SFTP_IO_TIMEOUT, remote_file.write_all(&buffer[..n])).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(format!("写入远端文件失败: {e}")),
                Err(_) => {
                    return Err(format!(
                        "SFTP write 超时（{}s）：远端无响应",
                        SFTP_IO_TIMEOUT.as_secs()
                    ))
                }
            }
            hasher.update(&buffer[..n]);
            total_bytes += n as u64;
        }

        sftp_timeout(SFTP_OP_TIMEOUT, "upload flush", async {
            remote_file
                .shutdown()
                .await
                .map_err(|e| format!("刷新远端文件失败: {e}"))
        })
        .await?;

        // rename（含覆盖兜底）。失败时**不删 temp**（同 write_file_atomic：
        // temp 是新数据唯一副本，路径已写进错误信息供手动恢复）。
        sftp_timeout(
            SFTP_OP_TIMEOUT,
            "upload rename",
            self.rename_with_overwrite_fallback(&temp_remote, &norm_remote),
        )
        .await?;

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

        let mut remote_file = sftp_timeout(SFTP_OP_TIMEOUT, "download open", async {
            self.sftp
                .open(&norm_remote)
                .await
                .map_err(|e| format!("打开远端待下载文件失败 ({norm_remote}): {e}"))
        })
        .await?;

        let mut local_file = tokio::fs::File::create(&temp_local_path)
            .await
            .map_err(|e| format!("创建本机下载临时文件失败 ({}): {e}", temp_local_path.display()))?;

        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut total_bytes: u64 = 0;

        loop {
            // 单块读上限（对齐 GUI READ_CHUNK_TIMEOUT）：死连接上 read 永不返回
            let n = match tokio::time::timeout(SFTP_IO_TIMEOUT, remote_file.read(&mut buffer)).await {
                Ok(Ok(n)) => n,
                Ok(Err(e)) => return Err(format!("读取远端文件数据失败: {e}")),
                Err(_) => {
                    return Err(format!(
                        "SFTP read 超时（{}s）：远端无响应",
                        SFTP_IO_TIMEOUT.as_secs()
                    ))
                }
            };
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
