//! 内置编辑器的远端文本读写（SFTP 会话版）。
//!
//! 与 `sftp_read_file` / `sftp_write_file` 的分工：那两个是裸读写（无防线、
//! GUI 从未调用），本模块是编辑器专用链路——
//! - 2 MiB 大小上限 + 二进制嗅探（`core::remote_text` 三态，选了 GBK 也救不了 PNG）；
//! - 编码白名单严格转码（`core::text_codec`，绝不 lossy）；
//! - 写回冲突检测：stat 比对 expected size/modified，不一致返回 `conflict`
//!   （由前端引导 覆盖/另存/重载），**绝不静默覆盖**；
//! - 原子写（temp + rename 覆盖兜底；兜底逻辑从 MCP `sftp_ops.rs` 提取共享，
//!   两边语义必须一致：「失败不删 temp，temp 是新数据唯一副本」）；
//! - 可选写前备份（`crate::editor_store`，app-data 滚动 3 份）。
//!
//! 错误前缀协议（前端 `lib/editor/editorIo.ts` 靠 `[editor:kind]` 前缀分类，
//! 契约固化在 docs/IPC契约与数据模型.md）：
//! `too-large` / `binary` / `not-utf8` / `encoding` / `lossy` / `not-found` /
//! `dir` / `permission`；其余错误原样透传（前端按未知类、可重试处理）。

use super::*;

/// 编辑器单文件读写上限。2 MiB 内 CodeMirror 流畅；超过就明确拒绝——
/// 「能打开但卡死」比「打不开并说明原因」更糟（验收红线：大文件不卡死界面）。
pub const MAX_EDIT_BYTES: u64 = 2 * 1024 * 1024;

/// 读取结果（远端/本地共用形状；前端 `ReadTextResult` 类型对齐 camelCase）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadTextResult {
    pub content: String,
    /// 实际使用的编码标签（规范名小写）。
    pub encoding: String,
    /// 原文件是否带 UTF-8 BOM（读入时已剥离；写回时按 keepBom 还原）。
    pub has_bom: bool,
    /// "lf" | "crlf" | "mixed"
    pub eol: String,
    pub size: u64,
    /// Unix 秒字符串（与 RemoteFileEntry.modified 同一格式化路径）。
    pub modified: String,
    /// 只读判定：远端=权限八进制无任何 w 位；本地=readonly 属性。None=未知。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
}

/// 写回结果。`status == "conflict"` 时未写任何字节，前端引导三选。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteTextResult {
    /// "written" | "conflict"
    pub status: &'static str,
    pub size: u64,
    pub modified: String,
    /// 冲突补充说明（如「远端文件已被删除」）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict_note: Option<String>,
}

fn ederr(kind: &str, message: String) -> String {
    format!("[editor:{kind}] {message}")
}

pub fn eol_kind_to_str(kind: myshelltool_core::text_codec::EolKind) -> &'static str {
    use myshelltool_core::text_codec::EolKind;
    match kind {
        EolKind::Lf => "lf",
        EolKind::Crlf => "crlf",
        EolKind::Mixed => "mixed",
    }
}

/// "lf"/"crlf" 之外的值（含 None）一律「保持原样」——混合换行不替用户归一。
fn parse_eol_target(eol: &Option<String>) -> Option<myshelltool_core::text_codec::EolKind> {
    use myshelltool_core::text_codec::EolKind;
    match eol.as_deref().map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("lf") | Some("lf\n") => Some(EolKind::Lf),
        Some("crlf") | Some("cr lf") => Some(EolKind::Crlf),
        _ => None,
    }
}

/// 编辑器解码管线（远端字节与本地字节共用）：
/// 1) 先二进制嗅探（NUL/魔数）——GBK 等高覆盖编码会把 PNG「成功」解成乱码，
///    `decode_remote_text` 的三态判定是唯一防线；
/// 2) 未指定编码 → 仅接受 UTF-8（NotUtf8 报偏移，引导用户选编码重试）；
/// 3) 指定编码 → `text_codec` 严格转码（非法序列硬错误，不 lossy）。
/// 返回 (正文, 是否带BOM, 实际编码标签)。
pub fn decode_for_editor(
    bytes: &[u8],
    enc: Option<&'static encoding_rs::Encoding>,
) -> Result<(String, bool, String), String> {
    use myshelltool_core::remote_text::{decode_remote_text, RemoteTextOutcome};
    use myshelltool_core::text_codec;

    // 二进制嗅探对任何编码选择都先行
    match decode_remote_text(bytes) {
        RemoteTextOutcome::Binary(reason) => {
            return Err(ederr(
                "binary",
                format!("文件是二进制内容（{reason}），无法按文本编辑，请改用下载"),
            ));
        }
        RemoteTextOutcome::NotUtf8(reason) if enc.is_none() => {
            return Err(ederr(
                "not-utf8",
                format!(
                    "文件不是 UTF-8 文本（{reason}）。请切换编码（常见：GBK）后重新打开"
                ),
            ));
        }
        outcome => {
            let (text, has_bom, label) = match enc {
                None => match outcome {
                    RemoteTextOutcome::Text { text, has_bom } => {
                        (text, has_bom, "utf-8".to_string())
                    }
                    _ => unreachable!("NotUtf8 已在上方提前返回"),
                },
                Some(e) => {
                    let label = e.name().to_ascii_lowercase();
                    let text = text_codec::decode_bytes(bytes, &label)
                        .map_err(|reason| ederr("encoding", reason))?;
                    // BOM 只在 UTF-8 上有意义；其它编码读出的 U+FEFF 视为正文
                    if e == encoding_rs::UTF_8 {
                        let (rest, had) = text_codec::strip_utf8_bom(&text);
                        (rest.to_string(), had, label)
                    } else {
                        (text, false, label)
                    }
                }
            };
            Ok((text, has_bom, label))
        }
    }
}

/// 编辑器写回前的文本变换：EOL 归一 + （UTF-8 时）BOM 还原。
/// 与 decode_for_editor 对称（那边剥 BOM，这边按 keepBom 加回）。
pub fn encode_for_editor(
    content: &str,
    enc: Option<&'static encoding_rs::Encoding>,
    eol: &Option<String>,
    keep_bom: bool,
) -> Result<Vec<u8>, String> {
    use myshelltool_core::text_codec;
    let mut text = match parse_eol_target(eol) {
        Some(target) => text_codec::apply_eol(content, target),
        None => content.to_string(),
    };
    let label = enc
        .map(|e| e.name().to_ascii_lowercase())
        .unwrap_or_else(|| "utf-8".to_string());
    if keep_bom && label == "utf-8" {
        text = text_codec::prepend_utf8_bom(&text);
    }
    text_codec::encode_text(&text, &label).map_err(|reason| ederr("encoding", reason))
}

/// stat/open 错误里的「文件不存在」分类。判据是 russh-sftp 错误 Display 文本
/// （自家协议错误串的固定契约，与前端 probeRemoteTarget 的 `/no such file/i`
/// 同一来源），不是对外部环境的猜测。
fn classify_missing(prefix: &str, e: &dyn std::fmt::Display, path: &str) -> String {
    let raw = format!("{prefix}: {e}");
    if raw.to_lowercase().contains("no such file") {
        ederr("not-found", format!("文件不存在：{path}"))
    } else {
        raw
    }
}

/// rename 覆盖兜底（编辑器原子写与 MCP `write_file_atomic` 共用，从
/// mcp/sftp_ops.rs 提取——两边语义必须一致，见原注释）：
///
/// russh-sftp 只发标准 FXP_RENAME（无 posix-rename 扩展），规范严格的服务器
/// 会拒绝覆盖已存在目标。兜底：rename 失败时 stat 目标——存在则先 remove
/// 再重试一次。**失败路径不删 temp**：temp 是新数据唯一副本（覆盖兜底失败时
/// 原目标可能已被 remove），所有错误信息带 temp 路径指引用户手动恢复。
pub async fn sftp_rename_with_overwrite_fallback(
    sftp: &SftpSession,
    temp: &str,
    target: &str,
) -> Result<(), String> {
    if let Err(rename_err) = sftp.rename(temp, target).await {
        match sftp.metadata(target).await {
            Ok(_) => {
                if let Err(remove_err) = sftp.remove_file(target).await {
                    return Err(format!(
                        "原子替换失败 (rename to {target}): {rename_err}; \
                         回退删除已存在目标也失败（原文件未删除）: {remove_err}; \
                         新数据保留在临时文件 {temp}，排除故障后可手动恢复或重试"
                    ));
                }
                warn!("sftp rename 覆盖兜底: 目标 {target} 已存在，remove 后重试 rename");
                if let Err(retry_err) = sftp.rename(temp, target).await {
                    return Err(format!(
                        "原子替换失败 (rename to {target}): 首次 {rename_err}; \
                         删除目标后重试仍失败: {retry_err}; \
                         原文件已删除，新数据保留在临时文件 {temp}，请手动恢复"
                    ));
                }
            }
            Err(_) => {
                return Err(format!(
                    "原子替换文件失败 (rename to {target}): {rename_err}; \
                     新数据保留在临时文件 {temp}，排除故障后可手动恢复或重试"
                ));
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn sftp_read_text(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    encoding: Option<String>,
) -> Result<ReadTextResult, String> {
    if myshelltool_core::is_lossy_remote_path(&path) {
        return Err(ederr(
            "lossy",
            myshelltool_core::lossy_remote_path_error(&path),
        ));
    }
    // 编码标签先校验（fail-closed：白名单外直接拒，不回落）
    let enc = match &encoding {
        Some(label) => match myshelltool_core::text_codec::encoding_for_label(label) {
            Ok(e) => Some(e),
            Err(e) => return Err(ederr("encoding", e)),
        },
        None => None,
    };

    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;

    // 句柄外先按路径 stat（大小上限必须在读之前判；目录同理）
    let meta = sftp
        .metadata(&path)
        .await
        .map_err(|e| classify_missing("SFTP stat failed", &e, &path))?;
    if meta.is_dir() {
        return Err(ederr("dir", format!("目标路径是目录：{path}")));
    }
    if meta.len() > MAX_EDIT_BYTES {
        return Err(ederr(
            "too-large",
            format!(
                "文件 {} 字节，超过编辑器上限 {} 字节（2 MiB）。请下载后用本地工具编辑",
                meta.len(),
                MAX_EDIT_BYTES
            ),
        ));
    }

    let mut file = sftp
        .open(&path)
        .await
        .map_err(|e| classify_missing("SFTP open failed", &e, &path))?;
    let mut buf = Vec::with_capacity(meta.len() as usize);
    use tokio::io::AsyncReadExt;
    file.read_to_end(&mut buf)
        .await
        .map_err(|e| format!("SFTP read failed: {e}"))?;

    let modified = crate::fs_local::format_modified(meta.modified());
    let read_only = meta.permissions.map(|p| p & 0o222 == 0);
    let (content, has_bom, used_label) = decode_for_editor(&buf, enc)?;
    let eol = eol_kind_to_str(myshelltool_core::text_codec::detect_eol(&content)).to_string();

    Ok(ReadTextResult {
        content,
        encoding: used_label,
        has_bom,
        eol,
        size: meta.len(),
        modified,
        read_only,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn sftp_write_text(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    content: String,
    encoding: Option<String>,
    eol: Option<String>,
    keep_bom: Option<bool>,
    expected_size: Option<u64>,
    expected_modified: Option<String>,
    backup: Option<bool>,
) -> Result<WriteTextResult, String> {
    if myshelltool_core::is_lossy_remote_path(&path) {
        return Err(ederr(
            "lossy",
            myshelltool_core::lossy_remote_path_error(&path),
        ));
    }
    let enc = match &encoding {
        Some(label) => match myshelltool_core::text_codec::encoding_for_label(label) {
            Ok(e) => Some(e),
            Err(e) => return Err(ederr("encoding", e)),
        },
        None => None,
    };
    let bytes = encode_for_editor(&content, enc, &eol, keep_bom.unwrap_or(false))?;

    let sftp_arc = get_or_create_sftp(&state, &session_id).await?;
    let sftp = sftp_arc.lock().await;

    // 冲突检测：expected 任一字段提供即视为「守护写」，比对失败返回 conflict。
    // 两个 expected 都为 None = 强制写（新建/另存为——前端已做过自己的覆盖确认）。
    let existing = match sftp.metadata(&path).await {
        Ok(meta) => {
            if meta.is_dir() {
                return Err(ederr("dir", format!("目标路径是目录：{path}")));
            }
            if expected_size.is_some() || expected_modified.is_some() {
                let modified = crate::fs_local::format_modified(meta.modified());
                let size_match = expected_size.map_or(true, |v| v == meta.len());
                let mtime_match = expected_modified.as_deref().map_or(true, |v| v == modified);
                if !size_match || !mtime_match {
                    return Ok(WriteTextResult {
                        status: "conflict",
                        size: meta.len(),
                        modified,
                        conflict_note: Some(
                            "远端文件在打开后已被修改（大小或修改时间变化）。".to_string(),
                        ),
                    });
                }
            }
            Some(meta)
        }
        Err(e) => {
            let raw = format!("SFTP stat failed: {e}");
            if raw.to_lowercase().contains("no such file") {
                if expected_size.is_some() || expected_modified.is_some() {
                    // 打开时存在、保存时已消失——如实交还给用户决定
                    return Ok(WriteTextResult {
                        status: "conflict",
                        size: 0,
                        modified: String::new(),
                        conflict_note: Some("远端文件已被删除。".to_string()),
                    });
                }
                None // 新建
            } else {
                return Err(raw);
            }
        }
    };

    // 写前备份（fail-closed：备份写不进本地就中止保存，不带保护继续写等于裸覆盖）
    if backup.unwrap_or(false) {
        if let Some(meta) = &existing {
            let mut old_file = sftp
                .open(&path)
                .await
                .map_err(|e| classify_missing("SFTP open failed", &e, &path))?;
            let mut old_bytes = Vec::with_capacity(meta.len() as usize);
            use tokio::io::AsyncReadExt;
            old_file
                .read_to_end(&mut old_bytes)
                .await
                .map_err(|e| format!("SFTP read failed（备份读取）: {e}"))?;
            crate::editor_store::put_backup(&app, "remote", &path, &old_bytes)?;
        }
    }

    // 原子写：temp + rename（覆盖兜底共享 helper；失败不删 temp，见其注释）
    let temp_path = format!("{path}.myshelltool.{}.tmp", uuid::Uuid::new_v4());
    use tokio::io::AsyncWriteExt;
    let mut temp = sftp
        .create(&temp_path)
        .await
        .map_err(|e| format!("创建临时文件失败 ({temp_path}): {e}"))?;
    temp.write_all(&bytes)
        .await
        .map_err(|e| format!("写入临时文件失败: {e}"))?;
    temp.shutdown()
        .await
        .map_err(|e| format!("刷新临时文件失败: {e}"))?;
    sftp_rename_with_overwrite_fallback(&sftp, &temp_path, &path).await?;

    // 写后 stat 回传新基线（下一次保存的 expected）
    let (new_size, new_modified) = match sftp.metadata(&path).await {
        Ok(meta) => (meta.len(), crate::fs_local::format_modified(meta.modified())),
        Err(_) => (bytes.len() as u64, String::new()),
    };
    Ok(WriteTextResult {
        status: "written",
        size: new_size,
        modified: new_modified,
        conflict_note: None,
    })
}
