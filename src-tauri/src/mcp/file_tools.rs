//! MCP 文件传输相关工具定义与分发（file_tools.rs）。
//!
//! 包含 6 个工具：
//! - sftp_list：浏览远程目录（结构化返回）
//! - sftp_read_file：读取远程文本小文件（1MB 上限，带二进制嗅探）
//! - sftp_write_file：写入远程文件（原子写，恒审批）
//! - sftp_upload：本机文件流式上传到远程（SHA256 校验，恒审批）
//! - sftp_download：远程文件流式下载到本机（SHA256 校验，覆盖本机需审批）
//! - sftp_remove：删除远程文件或目录（受控递归删除，恒审批）

use std::path::PathBuf;

use rmcp::model::{CallToolResult, Content, Tool};
use serde_json::{json, Map, Value};

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

fn schema_obj(v: serde_json::Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

/// 返回所有文件传输类 MCP 工具定义。
pub fn list_file_tools() -> Vec<Tool> {
    vec![
        Tool::new(
            "sftp_list",
            "列出指定资产远程目录中的文件与子目录。返回 JSON 结构列表，包含名称、路径、类型（file/directory/symlink）、字节大小、修改时间与权限。",
            schema_obj(json!({
                "type": "object",
                "properties": {
                    "asset_id": {
                        "type": "string",
                        "description": "目标资产 ID（由 list_assets 获取）"
                    },
                    "path": {
                        "type": "string",
                        "description": "远程目录路径，例如 /var/log 或 /home/deploy。留空或传入 . 表示默认用户根目录。"
                    }
                },
                "required": ["asset_id"]
            })),
        ),
        Tool::new(
            "sftp_read_file",
            "读取指定资产上的远程文本文件。单次读取上限 1MB，适合配置排查与代码审计（敏感路径如 /etc/shadow、SSH 私钥需在客户端确认；二进制文件请改用 sftp_download）。",
            schema_obj(json!({
                "type": "object",
                "properties": {
                    "asset_id": {
                        "type": "string",
                        "description": "目标资产 ID"
                    },
                    "path": {
                        "type": "string",
                        "description": "待读取的远程文件绝对路径或相对路径，如 /etc/hosts 或 /var/log/syslog"
                    }
                },
                "required": ["asset_id", "path"]
            })),
        ),
        Tool::new(
            "sftp_write_file",
            "向指定资产写入远程文本文件。采用原子临时文件替换机制，单次上限 1MB。此操作涉及远程文件创建或覆盖，始终需要在客户端确认。调用时必须如实声明 intent 意图。",
            schema_obj(json!({
                "type": "object",
                "properties": {
                    "asset_id": {
                        "type": "string",
                        "description": "目标资产 ID"
                    },
                    "path": {
                        "type": "string",
                        "description": "待写入的远程目标文件路径，如 /tmp/config.json"
                    },
                    "content": {
                        "type": "string",
                        "description": "文件文本正文"
                    },
                    "intent": {
                        "type": "string",
                        "description": "写入或修改文件的操作意图"
                    }
                },
                "required": ["asset_id", "path", "content", "intent"]
            })),
        ),
        Tool::new(
            "sftp_upload",
            "将本机文件流式上传至远程服务器。采用分块传输、临时文件落地原子替换与 SHA256 完整性校验。适用于大文件与二进制包传输。覆盖远程已有文件需在客户端确认。调用时必须声明 intent 意图。",
            schema_obj(json!({
                "type": "object",
                "properties": {
                    "asset_id": {
                        "type": "string",
                        "description": "目标资产 ID"
                    },
                    "local_path": {
                        "type": "string",
                        "description": "本机源文件绝对路径"
                    },
                    "remote_path": {
                        "type": "string",
                        "description": "远程目标文件绝对路径"
                    },
                    "intent": {
                        "type": "string",
                        "description": "上传文件的操作意图"
                    }
                },
                "required": ["asset_id", "local_path", "remote_path", "intent"]
            })),
        ),
        Tool::new(
            "sftp_download",
            "将远程服务器上的文件流式下载至本机。采用分块传输与 SHA256 完整性校验。若本机目标路径已存在文件，覆盖前需在客户端确认。调用时必须声明 intent 意图。",
            schema_obj(json!({
                "type": "object",
                "properties": {
                    "asset_id": {
                        "type": "string",
                        "description": "目标资产 ID"
                    },
                    "remote_path": {
                        "type": "string",
                        "description": "远程源文件绝对路径"
                    },
                    "local_path": {
                        "type": "string",
                        "description": "本机保存的目标文件绝对路径"
                    },
                    "intent": {
                        "type": "string",
                        "description": "下载文件的操作意图"
                    }
                },
                "required": ["asset_id", "remote_path", "local_path", "intent"]
            })),
        ),
        Tool::new(
            "sftp_remove",
            "删除指定资产上的远程文件或目录。高危破坏性操作，始终需要在客户端二次确认。若删除非空目录，必须显式声明 recursive=true。调用时必须如实声明 intent 意图。",
            schema_obj(json!({
                "type": "object",
                "properties": {
                    "asset_id": {
                        "type": "string",
                        "description": "目标资产 ID"
                    },
                    "path": {
                        "type": "string",
                        "description": "待删除的远程文件或目录路径"
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "是否递归删除非空目录（默认 false）"
                    },
                    "intent": {
                        "type": "string",
                        "description": "删除操作的意图"
                    }
                },
                "required": ["asset_id", "path", "intent"]
            })),
        ),
    ]
}

/// 执行文件类工具。若工具名称不匹配则返回 Ok(None)。
pub async fn dispatch_file_tool(
    ctx: &McpToolContext,
    name: &str,
    args: &Map<String, Value>,
) -> Result<Option<CallToolResult>, String> {
    match name {
        "sftp_list" => tool_sftp_list(ctx, args).await.map(Some),
        "sftp_read_file" => tool_sftp_read_file(ctx, args).await.map(Some),
        "sftp_write_file" => tool_sftp_write_file(ctx, args).await.map(Some),
        "sftp_upload" => tool_sftp_upload(ctx, args).await.map(Some),
        "sftp_download" => tool_sftp_download(ctx, args).await.map(Some),
        "sftp_remove" => tool_sftp_remove(ctx, args).await.map(Some),
        _ => Ok(None),
    }
}

async fn tool_sftp_list(
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

async fn tool_sftp_read_file(
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

    let session = HeadlessSftpSession::connect(ctx, asset_id).await?;
    // 单次读取上限 1MB (1,048,576 字节)
    let content = session.read_file_limited(path, 1024 * 1024).await?;
    Ok(text_result(&content))
}

async fn tool_sftp_write_file(
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

async fn tool_sftp_upload(
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

    let local_path = PathBuf::from(local_path_str);
    if !local_path.exists() {
        return Ok(error_result(&format!(
            "本机源文件不存在: {}",
            local_path.display()
        )));
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

async fn tool_sftp_download(
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

    let local_path = PathBuf::from(local_path_str);
    // 保护本机关键系统目录
    if is_protected_local_write_path(&local_path) {
        return Ok(error_result(&format!(
            "拒绝写入：本机路径 {} 位于系统受保护目录中。",
            local_path.display()
        )));
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

async fn tool_sftp_remove(
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

    let session = HeadlessSftpSession::connect(ctx, asset_id).await?;
    session.remove_path(path, recursive).await?;

    let mode_desc = if recursive { "（含递归目录）" } else { "" };
    Ok(text_result(&format!(
        "已成功删除远程路径 {}{}",
        normalize_remote_path(path),
        mode_desc
    )))
}
