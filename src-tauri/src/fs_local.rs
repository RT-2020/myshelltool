// 本地文件系统命令（fs_local_*）。
// 浏览器预览模式（npm run dev）调用会失败——前端 backend.js 在非 Tauri runtime 直接抛错。
//
// 安全模型（评审修复）：
// - 路径规范化（去 `..` / `.`）但**不**跟随 symlink（防穿越）
// - 黑名单：拒绝系统目录（Windows: C:\Windows、Program Files、ProgramData；
//   Unix: /etc、/usr、/var、/boot、/sys、/proc、/dev、/root、/bin、/sbin、/lib）
// - 拒绝根目录（防止递归删除整盘）
// - 删除时用 symlink_metadata 不跟随 symlink，防止"删 symlink → 删目标"
// - list_dir 用 resolved.join(name) 返回 logical path，不暴露 symlink 物理路径

use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::command;

use crate::ssh::RemoteFileEntry;

#[derive(Debug, Clone, Serialize)]
pub struct LocalDirectoryList {
    pub path: String,
    pub parent: String,
    pub entries: Vec<RemoteFileEntry>,
}

fn home_dir_string() -> String {
    if let Some(home) = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
    {
        return PathBuf::from(home).to_string_lossy().into_owned();
    }
    ".".to_string()
}

// 规范化路径（解析 ~ / home / 相对路径），但不 canonicalize（不跟随 symlink、不要求存在）。
// 然后用 is_sensitive_path 黑名单拒绝系统目录。
fn resolve_input_path(input: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim();
    let candidate: PathBuf = if trimmed.is_empty() || trimmed == "." || trimmed == "~" {
        PathBuf::from(home_dir_string())
    } else if trimmed.starts_with('~') {
        PathBuf::from(trimmed.replacen('~', &home_dir_string(), 1))
    } else {
        PathBuf::from(trimmed)
    };

    // std::path::absolute 在 Rust 1.79+ 稳定：规范化 `..` / `.`，不解析 symlink，不要求存在。
    let abs = std::path::absolute(&candidate)
        .map_err(|e| format!("path normalization failed for {}: {e}", candidate.display()))?;

    if is_sensitive_path(&abs) {
        return Err(format!(
            "access to system path is not allowed: {}",
            abs.display()
        ));
    }
    Ok(abs)
}

// 系统敏感路径黑名单。canonical 后路径含 `..` 已被 absolute 规范化掉。
fn is_sensitive_path(abs: &Path) -> bool {
    let s = abs.to_string_lossy().to_lowercase().replace('/', "\\");

    // 根目录 / 盘符根
    if s == "\\" || s.len() == 3 && s.ends_with(":\\") {
        return true;
    }

    const BLOCKED: &[&str] = &[
        // Windows 系统目录
        "c:\\windows",
        "c:\\program files",
        "c:\\program files (x86)",
        "c:\\programdata",
        "c:\\$recycle.bin",
        "c:\\system volume information",
        // Unix 系统目录
        "\\etc", "\\usr", "\\var", "\\boot", "\\sys", "\\proc",
        "\\dev", "\\root", "\\sbin", "\\bin", "\\lib", "\\lib64",
        "\\system32",
    ];
    BLOCKED.iter().any(|p| s == *p || s.starts_with(&format!("{}\\", p)))
}

fn parent_string(path: &Path) -> String {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_string_lossy().into_owned(),
        _ => path.to_string_lossy().into_owned(),
    }
}

fn format_modified(modified: std::io::Result<std::time::SystemTime>) -> String {
    // Unix 秒数字符串；前端 new Date(Number(s) * 1000) 本地化展示。
    modified
        .map(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

#[command]
pub fn fs_local_home_dir() -> String {
    home_dir_string()
}

#[command]
pub fn fs_local_list_dir(path: String) -> Result<LocalDirectoryList, String> {
    let resolved = resolve_input_path(&path)?;
    if !resolved.exists() {
        return Err(format!("Path does not exist: {}", resolved.display()));
    }
    if !resolved.is_dir() {
        return Err(format!("Path is not a directory: {}", resolved.display()));
    }

    let read = std::fs::read_dir(&resolved)
        .map_err(|e| format!("read_dir failed for {}: {e}", resolved.display()))?;

    let mut entries: Vec<RemoteFileEntry> = read
        .filter_map(|item| item.ok())
        .filter_map(|entry| {
            // 用 symlink_metadata 不跟随 symlink（防止通过 symlink 探测外部内容）。
            let meta = match std::fs::symlink_metadata(entry.path()) {
                Ok(m) => m,
                Err(_) => return None,
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            // logical path = resolved + name；不暴露 symlink 解析后的物理路径。
            let path = resolved.join(&name).to_string_lossy().into_owned();
            let kind = if meta.file_type().is_symlink() {
                "symlink"
            } else if meta.is_dir() {
                "directory"
            } else {
                "file"
            }
            .to_string();
            let modified = format_modified(meta.modified());
            Some(RemoteFileEntry {
                name,
                path,
                kind,
                size: meta.len(),
                modified,
            })
        })
        .collect();

    entries.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(LocalDirectoryList {
        path: resolved.to_string_lossy().into_owned(),
        parent: parent_string(&resolved),
        entries,
    })
}

#[command]
pub fn fs_local_mkdir(path: String) -> Result<(), String> {
    let target = resolve_input_path(&path)?;
    std::fs::create_dir(&target)
        .map_err(|e| format!("mkdir failed for {}: {e}", target.display()))
}

#[command]
pub fn fs_local_delete(path: String, kind: String) -> Result<(), String> {
    let target = resolve_input_path(&path)?;
    // symlink_metadata 不跟随 symlink，防"删 symlink → 删目标"。
    let meta = std::fs::symlink_metadata(&target)
        .map_err(|e| format!("stat failed for {}: {e}", target.display()))?;

    if meta.file_type().is_symlink() {
        // symlink 始终只删 link 本身，无论 kind 字段。
        std::fs::remove_file(&target)
            .map_err(|e| format!("remove symlink failed for {}: {e}", target.display()))?;
        return Ok(());
    }
    if kind == "directory" {
        std::fs::remove_dir_all(&target)
            .map_err(|e| format!("remove_dir_all failed for {}: {e}", target.display()))
    } else {
        std::fs::remove_file(&target)
            .map_err(|e| format!("remove_file failed for {}: {e}", target.display()))
    }
}

#[command]
pub fn fs_local_rename(old_path: String, new_path: String) -> Result<(), String> {
    let from = resolve_input_path(&old_path)?;
    let to = resolve_input_path(&new_path)?;
    std::fs::rename(&from, &to)
        .map_err(|e| format!("rename failed {} -> {}: {e}", from.display(), to.display()))
}
