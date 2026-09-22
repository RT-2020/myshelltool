//! MCP 文件传输相关工具实现（file_tools.rs）。
//!
//! v0.20（A2）：schema 声明与分发已收敛到 registry.rs（单一事实源），
//! 本文件只保留六个 handler 的实现。
//!
//! 包含 6 个工具：
//! - sftp_list：浏览远程目录（结构化返回）
//! - sftp_read_file：读取远程文本小文件（1MB 上限，带二进制嗅探；敏感凭据路径恒审批）
//! - sftp_write_file：写入远程文件（原子写，按拦截等级判定：Minimal 放行记日志 / Strict 审批）
//! - sftp_upload：本机文件流式上传到远程（SHA256 校验，按拦截等级判定）
//! - sftp_download：远程文件流式下载到本机（SHA256 校验，按拦截等级判定；本机系统目录恒拒）
//! - sftp_remove：删除远程文件或目录（受控递归删除，按拦截等级判定；根级/核心目录恒拒）

use std::path::PathBuf;

use rmcp::model::{CallToolResult, Content};
use serde_json::{Map, Value};

use super::file_policy::{is_protected_local_write_path, normalize_remote_path};
use super::sftp_ops::HeadlessSftpSession;
use super::tools::McpToolContext;

fn text_result(text: &str) -> CallToolResult {
    CallToolResult::success(vec![Content::text(text.to_string())])
}

fn error_result(message: &str) -> CallToolResult {
    let mut result = CallToolResult::success(vec![Content::text(message.to_string())]);
    result.is_error = Some(true);
    result
}

pub(crate) async fn tool_sftp_list(
    ctx: &McpToolContext,
    args: &Map<String, Value>,
) -> Result<CallToolResult, String> {
    let asset_id = args
        .get("asset_id")
        .and_then(|v| v.as_str())
        .ok_or("缺少 asset_id 参数")?;
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

    let session = HeadlessSftpSession::connect(ctx, asset_id).await?;
    let entries = session.list_dir(path).await?;
    let json_text = serde_json::to_string_pretty(&entries)
        .map_err(|e| format!("序列化目录列表失败: {e}"))?;

    Ok(text_result(&json_text))
}

pub(crate) async fn tool_sftp_read_file(
    ctx: &McpToolContext,
    args: &Map<String, Value>,
) -> Result<CallToolResult, String> {
    let asset_id = args
        .get("asset_id")
        .and_then(|v| v.as_str())
        .ok_or("缺少 asset_id 参数")?;
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("缺少 path 参数")?;
    // v0.20（B2）分页参数：offset 续读 / maxBytes 上限（默认 256 KiB，封顶 4 MiB）
    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);
    let max_bytes = args
        .get("maxBytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(256 * 1024)
        .min(4 * 1024 * 1024) as usize;

    // lossy 文件名守卫（v2.6 backlog #2）：失真名要么 No such file，要么命中
    // 字面含 U+FFFD 的另一个真实文件——fail-closed，与 GUI 侧同一拒绝文案
    if myshelltool_core::is_lossy_remote_path(path) {
        return Err(myshelltool_core::lossy_remote_path_error(path));
    }
    let session = HeadlessSftpSession::connect(ctx, asset_id).await?;

    // v0.20（B2）语义变更：文件超过 maxBytes **不再失败**——首读返回前缀 +
    // 分页提示（nextOffset），续读走 read_file_range。
    // 首读（offset=0）先走旧的严格编码路径（二进制/非 UTF-8 照旧明确报错）；
    // 只有「文件过大」转分页。续读（offset>0）直走范围读取（文件已被首读
    // 验证过是文本，边界切分用 lossy 并在响应里说明）。
    if offset == 0 {
        match session.read_file_limited(path, max_bytes).await {
            // 文件整体在限内：直接返回完整内容（旧行为，无分页头）
            Ok(content) => return Ok(text_result(&content)),
            Err(e) if e.contains("文件过大") => { /* 落到下方分页读取 */ }
            Err(e) => return Err(e),
        }
    }

    let (chunk, total) = session.read_file_range(path, offset, max_bytes).await?;
    let start = offset;
    let end = offset + chunk.len() as u64;
    let boundary_note = if offset > 0 {
        "\n[注：续读段按字节边界切分，段首/段尾至多各有一个 � 替换符属正常]"
    } else {
        ""
    };
    if end >= total {
        Ok(text_result(&format!(
            "[read_file：文件共 {total} 字节，本段 {start}..{end}（EOF）]{boundary_note}\n{chunk}"
        )))
    } else {
        Ok(text_result(&format!(
            "[read_file：文件共 {total} 字节，本段 {start}..{end}，未完——续读传 offset={end}]{boundary_note}\n{chunk}"
        )))
    }
}

pub(crate) async fn tool_sftp_write_file(
    ctx: &McpToolContext,
    args: &Map<String, Value>,
) -> Result<CallToolResult, String> {
    let asset_id = args
        .get("asset_id")
        .and_then(|v| v.as_str())
        .ok_or("缺少 asset_id 参数")?;
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("缺少 path 参数")?;
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or("缺少 content 参数")?;

    if content.len() > 1024 * 1024 {
        return Ok(error_result(
            "写入内容超出单次 1MB 上限，请改用 sftp_upload 工具直接传输大文件。",
        ));
    }

    // lossy 守卫（多角色审查 Issue 3）：AI 宿主常把 sftp_list 的失真名（U+FFFD）
    // 回灌成 write/upload 目标；临时文件 rename 的原子替换会静默覆盖字面含
    // U+FFFD 的另一个真实文件。与 read/remove 同一 fail-closed 防线。
    if myshelltool_core::is_lossy_remote_path(path) {
        return Err(myshelltool_core::lossy_remote_path_error(path));
    }
    let session = HeadlessSftpSession::connect(ctx, asset_id).await?;
    let bytes_written = session
        .write_file_atomic(path, content.as_bytes())
        .await?;

    Ok(text_result(&format!(
        "已成功原子写入文件 {}（写入 {} 字节）",
        normalize_remote_path(path),
        bytes_written
    )))
}

pub(crate) async fn tool_sftp_upload(
    ctx: &McpToolContext,
    args: &Map<String, Value>,
) -> Result<CallToolResult, String> {
    let asset_id = args
        .get("asset_id")
        .and_then(|v| v.as_str())
        .ok_or("缺少 asset_id 参数")?;
    let local_path_str = args
        .get("local_path")
        .and_then(|v| v.as_str())
        .ok_or("缺少 local_path 参数")?;
    let remote_path = args
        .get("remote_path")
        .and_then(|v| v.as_str())
        .ok_or("缺少 remote_path 参数")?;

    // v0.20（B1）本机 FS 闸门：上传要读本机文件。scope 未配置时不收紧；
    // 一旦配置了资产限制，本机 FS 默认关（需面板显式 allow_local_fs 打开）。
    let fs_verdict = myshelltool_core::mcp_scope::evaluate_local_fs(&ctx.config.read().await.scope);
    if !fs_verdict.is_allowed() {
        return Err(myshelltool_core::mcp_scope::denied_message(fs_verdict));
    }

    let local_path = PathBuf::from(local_path_str);
    if !local_path.exists() {
        return Ok(error_result(&format!(
            "本机源文件不存在: {}",
            local_path.display()
        )));
    }

    // lossy 守卫（同 tool_sftp_write_file：上传是覆盖语义，失真名可能命中
    // 字面含 U+FFFD 的既有真实文件）
    if myshelltool_core::is_lossy_remote_path(remote_path) {
        return Err(myshelltool_core::lossy_remote_path_error(remote_path));
    }
    let session = HeadlessSftpSession::connect(ctx, asset_id).await?;
    let (bytes, sha256) = session.upload_stream(&local_path, remote_path).await?;

    Ok(text_result(&format!(
        "上传成功：\n- 本机源文件: {}\n- 远程目标路径: {}\n- 传输大小: {} 字节\n- SHA256 校验和: {}",
        local_path.display(),
        normalize_remote_path(remote_path),
        bytes,
        sha256
    )))
}

pub(crate) async fn tool_sftp_download(
    ctx: &McpToolContext,
    args: &Map<String, Value>,
) -> Result<CallToolResult, String> {
    let asset_id = args
        .get("asset_id")
        .and_then(|v| v.as_str())
        .ok_or("缺少 asset_id 参数")?;
    let remote_path = args
        .get("remote_path")
        .and_then(|v| v.as_str())
        .ok_or("缺少 remote_path 参数")?;
    let local_path_str = args
        .get("local_path")
        .and_then(|v| v.as_str())
        .ok_or("缺少 local_path 参数")?;

    // v0.20（B1）本机 FS 闸门：下载要写本机文件（语义同上传）
    let fs_verdict = myshelltool_core::mcp_scope::evaluate_local_fs(&ctx.config.read().await.scope);
    if !fs_verdict.is_allowed() {
        return Err(myshelltool_core::mcp_scope::denied_message(fs_verdict));
    }

    let local_path = PathBuf::from(local_path_str);
    // 保护本机关键系统目录
    if is_protected_local_write_path(&local_path) {
        return Ok(error_result(&format!(
            "拒绝写入：本机路径 {} 位于系统受保护目录中。",
            local_path.display()
        )));
    }

    // lossy 文件名守卫：静默下到「字面含 U+FFFD 的另一个文件」的内容比报错危险
    if myshelltool_core::is_lossy_remote_path(remote_path) {
        return Err(myshelltool_core::lossy_remote_path_error(remote_path));
    }
    let session = HeadlessSftpSession::connect(ctx, asset_id).await?;
    let (bytes, sha256) = session.download_stream(remote_path, &local_path).await?;

    Ok(text_result(&format!(
        "下载成功：\n- 远程源文件: {}\n- 本机保存路径: {}\n- 传输大小: {} 字节\n- SHA256 校验和: {}",
        normalize_remote_path(remote_path),
        local_path.display(),
        bytes,
        sha256
    )))
}

pub(crate) async fn tool_sftp_remove(
    ctx: &McpToolContext,
    args: &Map<String, Value>,
) -> Result<CallToolResult, String> {
    let asset_id = args
        .get("asset_id")
        .and_then(|v| v.as_str())
        .ok_or("缺少 asset_id 参数")?;
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("缺少 path 参数")?;
    let recursive = args
        .get("recursive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // 删除（尤其 recursive）是最高危操作：失真名可能误删字面含 U+FFFD 的
    // 另一个真实文件/目录树，fail-closed（v2.6 backlog #2）
    if myshelltool_core::is_lossy_remote_path(path) {
        return Err(myshelltool_core::lossy_remote_path_error(path));
    }
    let session = HeadlessSftpSession::connect(ctx, asset_id).await?;
    session.remove_path(path, recursive).await?;

    let mode_desc = if recursive { "（含递归目录）" } else { "" };
    Ok(text_result(&format!(
        "已成功删除远程路径 {}{}",
        normalize_remote_path(path),
        mode_desc
    )))
}
