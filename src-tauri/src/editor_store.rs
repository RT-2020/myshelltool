//! 内置编辑器的草稿与备份存储（app-data 下的滚动小存储，纯本地文件）。
//!
//! 目录布局：
//! - `editor-backups/<key>/meta.json + <毫秒时间戳>.bin`——保存前的旧内容快照，
//!   每个目标文件滚动保留最近 3 份；
//! - `editor-drafts/<key>/draft.json + content.txt`——用户显式「保存草稿」的
//!   未落盘内容（UTF-8），重开同一文件时若草稿较新则前端引导恢复。
//!
//! `<key>` = FNV-1a64(target + 0x01 + path) 十六进制。目录名不受用户输入
//! 控制（无路径穿越面）；FNV 非加密哈希，碰撞（概率可忽略）只会表现为
//! 「读到别的文件的索引」——meta/draft 里回存原始 target+path，读取时校验
//! 一致，不一致按「无备份/无草稿」处理，绝不串内容。
//!
//! 只存文本编辑范围内的内容（凭据文件不在文本扩展名单，进不来）；
//! 备份/草稿属用户数据，不参与 Gist 同步。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use crate::ssh::{ReadTextResult};

/// FNV-1a 64 位确定性哈希（非安全用途，仅目录命名）。
fn storage_key(target: &str, path: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in format!("{target}\u{1}{path}").as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn now_millis() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|e| format!("{}", e.duration().as_millis()))
}

fn editor_data_dir(app: &AppHandle, domain: &str) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法定位应用数据目录: {e}"))?;
    Ok(base.join(domain))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorBackupMeta {
    /// 版本 id（= 创建时刻的毫秒时间戳，纯数字）。
    pub id: String,
    pub at: String,
    pub size: u64,
    pub target: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EditorBackupIndex {
    target: String,
    path: String,
    versions: Vec<EditorBackupMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorDraft {
    pub target: String,
    pub path: String,
    /// 草稿正文（编辑器内部即 UTF-8 字符串）。
    pub content: String,
    pub encoding: String,
    pub eol: String,
    /// 保存时刻（毫秒时间戳字符串）。
    pub saved_at: String,
}

fn backup_dir(app: &AppHandle, target: &str, path: &str) -> Result<PathBuf, String> {
    Ok(editor_data_dir(app, "editor-backups")?.join(storage_key(target, path)))
}

fn draft_dir(app: &AppHandle, target: &str, path: &str) -> Result<PathBuf, String> {
    Ok(editor_data_dir(app, "editor-drafts")?.join(storage_key(target, path)))
}

fn read_backup_index(dir: &PathBuf) -> Option<EditorBackupIndex> {
    let raw = std::fs::read_to_string(dir.join("meta.json")).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_backup_index(dir: &PathBuf, index: &EditorBackupIndex) -> Result<(), String> {
    let json = serde_json::to_string(index).map_err(|e| format!("序列化备份索引失败: {e}"))?;
    myshelltool_core::write_atomic(dir.join("meta.json"), json)
}

/// 保存前快照（编辑器写命令在 backup=true 且目标存在时调用）。
///
/// fail-closed 契约：返回 Err 时调用方必须中止保存——没有备份的覆盖等于
/// 裸覆盖，宁可让用户修好备份目录（或关掉备份开关）再存。
pub(crate) fn put_backup(app: &AppHandle, target: &str, path: &str, bytes: &[u8]) -> Result<(), String> {
    let dir = backup_dir(app, target, path)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建备份目录失败 {}: {e}", dir.display()))?;

    let id = now_millis();
    std::fs::write(dir.join(format!("{id}.bin")), bytes)
        .map_err(|e| format!("写入备份失败 {}: {e}", dir.display()))?;

    let mut index = read_backup_index(&dir).unwrap_or(EditorBackupIndex {
        target: target.to_string(),
        path: path.to_string(),
        versions: Vec::new(),
    });
    // 索引与目录不符（哈希碰撞/手工搬运）时按「无索引」重建，不串文件
    if index.target != target || index.path != path {
        index = EditorBackupIndex {
            target: target.to_string(),
            path: path.to_string(),
            versions: Vec::new(),
        };
    }
    index.versions.push(EditorBackupMeta {
        id: id.clone(),
        at: id,
        size: bytes.len() as u64,
        target: target.to_string(),
        path: path.to_string(),
    });
    // 滚动保留最近 3 份（毫秒时间戳字符串同长，字典序=数值序）
    index.versions.sort_by(|a, b| b.id.cmp(&a.id));
    while index.versions.len() > 3 {
        let removed = index.versions.pop();
        if let Some(meta) = removed {
            let _ = std::fs::remove_file(dir.join(format!("{}.bin", meta.id)));
        }
    }
    write_backup_index(&dir, &index)
}

#[tauri::command]
pub fn editor_backup_list(
    app: AppHandle,
    target: String,
    path: String,
) -> Result<Vec<EditorBackupMeta>, String> {
    let dir = backup_dir(&app, &target, &path)?;
    match read_backup_index(&dir) {
        Some(index) if index.target == target && index.path == path => Ok(index.versions),
        _ => Ok(Vec::new()),
    }
}

/// 读取一份备份内容（按指定编码解码；未指定按编辑器默认管线：UTF-8 严格 +
/// 二进制嗅探）。id 必须是纯数字（它进文件名——校验即防穿越）。
#[tauri::command]
pub fn editor_backup_read(
    app: AppHandle,
    target: String,
    path: String,
    id: String,
    encoding: Option<String>,
) -> Result<ReadTextResult, String> {
    if !id.chars().all(|c| c.is_ascii_digit()) || id.is_empty() {
        return Err(format!("[editor:not-found] 非法的备份版本 id：{id}"));
    }
    let dir = backup_dir(&app, &target, &path)?;
    let bytes = std::fs::read(dir.join(format!("{id}.bin")))
        .map_err(|e| format!("[editor:not-found] 读取备份失败: {e}"))?;
    let enc = match &encoding {
        Some(label) => match myshelltool_core::text_codec::encoding_for_label(label) {
            Ok(e) => Some(e),
            Err(e) => return Err(format!("[editor:encoding] {e}")),
        },
        None => None,
    };
    let (content, has_bom, used_label) = crate::ssh::decode_for_editor(&bytes, enc)?;
    let eol = crate::ssh::eol_kind_to_str(myshelltool_core::text_codec::detect_eol(&content)).to_string();
    Ok(ReadTextResult {
        content,
        encoding: used_label,
        has_bom,
        eol,
        size: bytes.len() as u64,
        modified: String::new(),
        read_only: Some(true),
    })
}

#[tauri::command]
pub fn editor_draft_save(
    app: AppHandle,
    target: String,
    path: String,
    content: String,
    encoding: String,
    eol: String,
) -> Result<String, String> {
    let dir = draft_dir(&app, &target, &path)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建草稿目录失败 {}: {e}", dir.display()))?;
    let saved_at = now_millis();
    let draft = EditorDraft {
        target,
        path,
        content,
        encoding,
        eol,
        saved_at: saved_at.clone(),
    };
    let meta = serde_json::to_string(&draft).map_err(|e| format!("序列化草稿失败: {e}"))?;
    myshelltool_core::write_atomic(dir.join("content.txt"), draft.content.as_bytes())?;
    myshelltool_core::write_atomic(dir.join("draft.json"), meta)?;
    Ok(saved_at)
}

#[tauri::command]
pub fn editor_draft_get(
    app: AppHandle,
    target: String,
    path: String,
) -> Result<Option<EditorDraft>, String> {
    let dir = draft_dir(&app, &target, &path)?;
    let raw = match std::fs::read_to_string(dir.join("draft.json")) {
        Ok(raw) => raw,
        Err(_) => return Ok(None),
    };
    let draft: EditorDraft = match serde_json::from_str(&raw) {
        Ok(d) => d,
        // 损坏的草稿元数据按「无草稿」处理（不静默丢正文：content.txt 仍在，
        // 用户可从目录手工找回；fail-closed 的「读不出来≠当作不存在」红线
        // 针对审批/凭据等级数据，草稿属可再生的便利性数据）
        Err(e) => {
            log::warn!("editor_store: 草稿元数据损坏（{}）：{e}", dir.display());
            return Ok(None);
        }
    };
    if draft.target != target || draft.path != path {
        return Ok(None);
    }
    let content = std::fs::read_to_string(dir.join("content.txt"))
        .map_err(|e| format!("读取草稿正文失败 {}: {e}", dir.display()))?;
    Ok(Some(EditorDraft { content, ..draft }))
}

#[tauri::command]
pub fn editor_draft_delete(
    app: AppHandle,
    target: String,
    path: String,
) -> Result<(), String> {
    let dir = draft_dir(&app, &target, &path)?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("删除草稿失败 {}: {e}", dir.display()))?;
    }
    Ok(())
}
