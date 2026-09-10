// 本地文件系统命令（fs_local_*）。
// 浏览器预览模式（npm run dev）调用会失败——前端 backend.js 在非 Tauri runtime 直接抛错。
//
// 安全模型（评审修复 + v2.5 短名/UNC/verbatim 加固 + v2.6 verbatim 等价形态）：
// - 路径规范化（去 `..` / `.`）；**返回值**不跟随 symlink（防穿越、不暴露物理路径），
//   但**黑名单判定**在路径存在时补 canonicalize 归一（封 NTFS 8.3 短名绕过）
// - 黑名单：拒绝系统目录（Windows: 任意盘符、UNC share、verbatim UNC
//   `\\?\UNC\server\share` 内的 Windows、Program Files、ProgramData、
//   $Recycle.Bin 等，判定前剥 `\\?\`/`\\.\` verbatim 前缀与 UNC `\\server\share`
//   两段；`\\?\Volume{…}` / `\\?\GLOBALROOT\…` 这类没有盘符结构的卷形态一律拒绝；
//   Unix: /etc、/usr、/var、/boot、/sys、/proc、/dev、/root、/bin、/sbin、/lib）
// - 拒绝根目录 / 盘符根（防止递归删除整盘）
// - is_sensitive_path 是本机系统目录判定的**单一事实源**：MCP 本机写保护
//   （mcp::file_policy::is_protected_local_write_path）复用本函数，不再另存一份清单
// - 删除类操作：路径存在但无法 canonicalize 时按拒绝处理（fail-secure）
// - 删除时用 symlink_metadata 不跟随 symlink，防止"删 symlink → 删目标"
// - list_dir 用 resolved.join(name) 返回 logical path，不暴露 symlink 物理路径

use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tauri::command;

use crate::ssh::RemoteFileEntry;

#[derive(Debug, Clone, Serialize)]
pub struct LocalDirectoryList {
    pub path: String,
    pub parent: String,
    pub entries: Vec<RemoteFileEntry>,
}

fn home_dir_string() -> Result<String, String> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|home| PathBuf::from(home).to_string_lossy().into_owned())
        // v2.5：不再静默回退 "."（进程 CWD）——文件面板会默默列出进程
        // 工作目录，用户以为看到的是家目录。显式报错让前端引导指定路径。
        .ok_or_else(|| {
            "无法确定本机用户主目录（USERPROFILE/HOME 环境变量均未设置），请显式指定路径".to_string()
        })
}

// 规范化路径（解析 ~ / home / 相对路径），但不 canonicalize（不跟随 symlink、不要求存在）。
// 然后用 is_sensitive_path 黑名单拒绝系统目录。
//
// v2.5 短名归一：路径存在时补一次 std::fs::canonicalize 再判（NTFS 8.3 短名
// 如 C:\PROGRA~1 经 canonicalize 还原为 C:\Program Files，字面比对封不住短名）。
// canonicalize 返回 verbatim 形式（\\?\C:\...），由 is_sensitive_path 内部剥离。
// 判定用 canonical 形态，返回值仍用 logical 路径（不暴露 symlink 物理路径）。
fn resolve_input_path(input: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim();
    let candidate: PathBuf = if trimmed.is_empty() || trimmed == "." || trimmed == "~" {
        PathBuf::from(home_dir_string()?)
    } else if trimmed.starts_with('~') {
        // `~` 仅在**开头**时表示家目录（`~user` 形态不支持，按「家目录 + user 子路径」
        // 处理会得到不存在的路径并显式报错，不静默落到别处）。
        // 曾写 `trimmed.replacen('~', &home, 1)`：replacen 替换的是**全串第一个** `~`，
        // 与 strip_prefix 语义容易混淆——若将来有人放宽守卫，`C:\tmp\~backup` 会被
        // 替换成 `C:\tmp\C:\Users\me\backup` 这种荒谬路径。改成显式拼接，语义只看开头。
        let rest = trimmed
            .trim_start_matches('~')
            .trim_start_matches(['/', '\\']);
        let home = home_dir_string()?;
        if rest.is_empty() {
            PathBuf::from(home)
        } else {
            PathBuf::from(home).join(rest)
        }
    } else {
        PathBuf::from(trimmed)
    };

    // std::path::absolute 在 Rust 1.79+ 稳定：规范化 `..` / `.`，不解析 symlink，不要求存在。
    let abs = std::path::absolute(&candidate)
        .map_err(|e| format!("path normalization failed for {}: {e}", candidate.display()))?;

    // 存在则用 canonicalize 归一形态再判（封 8.3 短名绕过）。解析失败
    //（权限等）不视为敏感——删除类操作另有 fail-secure 拒绝（fs_local_delete）。
    if abs.exists() {
        if let Ok(canon) = std::fs::canonicalize(&abs) {
            if is_sensitive_path(&canon) {
                return Err(format!(
                    "access to system path is not allowed: {}",
                    abs.display()
                ));
            }
            return Ok(abs);
        }
    }

    if is_sensitive_path(&abs) {
        return Err(format!(
            "access to system path is not allowed: {}",
            abs.display()
        ));
    }
    Ok(abs)
}

// 系统敏感路径黑名单（本机系统目录判定的**单一事实源**：GUI 本地面板经
// resolve_input_path 调用，MCP 本机写保护经 mcp::file_policy 调用）。
// canonical 后路径含 `..` 已被 absolute 规范化掉。
//
// 为什么 verbatim / UNC / 卷根三类必须**分别**显式处理：Windows 上同一条路径有
// 多种等价写法，判定只认其中一种就等于放行其余写法，而两个入口的输入形态都不受
// 我们控制（前端字符串 / MCP 宿主）。三类的差别在于剥掉 `\\?\` 前缀后剩余串的
// 结构不同——UNC 同义写法以 `unc\` 开头（既不是 `\\` 也不是 `X:`），卷形态则
// 根本没有盘符结构可换算。漏掉哪一类，那一类就整段跳过下面的系统目录黑名单。
//
// v2.5/v2.6 逃逸形态封堵（判定前先归一化）：
// (a) verbatim 前缀 `\\?\C:\Windows` / device 前缀 `\\.\C:\`——Win32 原样
//     透传，不剥则不满足「盘符+\」分支判断；canonicalize 的返回值必带
//     `\\?\`，也依赖此剥离。
// (b) UNC `\\server\share\Windows`——首字符 `\` 整个绕过盘符分支，剥掉
//     `server\share` 两段后按盘内路径同样比对（share 根本身拒绝）。
// (c) NTFS 8.3 短名 `C:\PROGRA~1`——字面不匹配目录名，由
//     resolve_input_path 的 canonicalize 归一（见其注释）。
pub(crate) fn is_sensitive_path(abs: &Path) -> bool {
    let s = abs.to_string_lossy().to_lowercase().replace('/', "\\");

    // (a) 剥 verbatim / device 前缀
    let s = s
        .strip_prefix("\\\\?\\")
        .or_else(|| s.strip_prefix("\\\\.\\"))
        .unwrap_or(&s);

    // 根目录 / 盘符根
    if s == "\\" || s.len() == 3 && s.ends_with(":\\") {
        return true;
    }

    // (c) verbatim 卷根/设备路径：`\\?\Volume{GUID}\…`、`\\?\GLOBALROOT\…`
    //     这类路径**没有**「盘符+盘内路径」结构，下面两条分支都提取不出卷内
    //     路径 → 会被整段跳过（曾放行 `\\?\UNC\…`、`\\?\Volume{…}\`）。无法
    //     按盘内路径比对的形态一律保守拒绝（fail-secure）：卷根相等或以其为
    //     前缀都拒，防递归删除整卷/整个共享。
    let lower_s = s.to_lowercase();
    if lower_s.starts_with("volume{") || lower_s.starts_with("globalroot") {
        return true;
    }

    // (d) UNC，含 verbatim 同义写法 `\\?\UNC\server\share\rest`。
    //     verbatim 前缀已由 (a) 剥离，于是 UNC 路径在此可能以 `unc\` 开头
    //     （**不是** `\\`）——不显式识别就会掉进「盘符形态」分支并得到 None，
    //     整个系统目录黑名单被跳过。
    let unc_tail: Option<&str> = if let Some(tail) = s.strip_prefix("\\\\") {
        Some(tail)
    } else {
        s.strip_prefix("unc\\")
    };
    // 提取「卷内路径」：盘符形态 c:\rest → rest；UNC share 内 → share 之后的
    // 部分；其余（Unix 绝对路径 / 无法解析的形态）→ None。
    let volume_rest: Option<&str> = if let Some(tail) = unc_tail {
        // tail = server\share\rest...
        let mut it = tail.splitn(3, '\\');
        let _server = it.next();
        match (it.next(), it.next()) {
            // \\server 或 \\server\share（share 根）：整体拒绝
            (None, _) | (Some(_), None) => return true,
            (Some(_), Some(rest)) => Some(rest),
        }
    } else if s.len() > 3 {
        let b = s.as_bytes();
        if b[0].is_ascii_lowercase() && b[1] == b':' && b[2] == b'\\' {
            Some(&s[3..])
        } else {
            None
        }
    } else {
        None
    };

    // Windows 系统目录：任意盘符（系统未必装在 C 盘，D:/E: 盘的 windows
    // 同样必须受保护）+ UNC share 内同名目录。匹配「恰好等于目录本身」或
    // 以 `<dir>\` 为前缀。
    if let Some(rest) = volume_rest {
        const WIN_SYSTEM_DIRS: &[&str] = &[
            "windows",
            "program files",
            "program files (x86)",
            "programdata",
            "$recycle.bin",
            "system volume information",
        ];
        for dir in WIN_SYSTEM_DIRS {
            if rest == *dir || rest.starts_with(&format!("{dir}\\")) {
                return true;
            }
        }
    }

    // Unix 系统目录
    const BLOCKED: &[&str] = &[
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

// Unix 秒数字符串；前端 new Date(Number(s) * 1000) 本地化展示。
// ssh.rs 的 sftp_list_dir / sftp_stat 复用（russh-sftp modified() 同为
// io::Result<SystemTime>，None → Err(ErrorKind::InvalidData)）。
pub(crate) fn format_modified(modified: std::io::Result<std::time::SystemTime>) -> String {
    modified
        .map(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

#[command]
pub fn fs_local_home_dir() -> Result<String, String> {
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
            // 本地权限：Unix 上取 mode 位，Windows 上 None（无 Unix 权限模型）。
            // user/group 本地暂不解析（非运维主场景），保持 None。
            #[cfg(unix)]
            let permissions = {
                use std::os::unix::fs::PermissionsExt;
                Some(format!("{:04o}", meta.permissions().mode() & 0o7777))
            };
            #[cfg(not(unix))]
            let permissions = None;
            Some(RemoteFileEntry {
                name,
                path,
                kind,
                size: meta.len(),
                modified,
                permissions,
                user: None,
                group: None,
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
    // fail-secure（v2.5）：删除类操作在路径存在但无法 canonicalize（权限/句柄
    // 占用）时按拒绝处理——黑名单判定依赖 canonicalize 归一（8.3 短名），
    // 解析失败 = 无法确认目标不属系统目录，宁可误拒不可误删。
    if target.exists() && std::fs::canonicalize(&target).is_err() {
        return Err(format!(
            "无法解析路径 {}（权限或句柄占用），为安全起见拒绝删除",
            target.display()
        ));
    }
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

#[command]
pub fn fs_local_read_chunk(path: String, offset: u64, length: u64) -> Result<Vec<u8>, String> {
    let target = resolve_input_path(&path)?;
    let meta = std::fs::symlink_metadata(&target)
        .map_err(|e| format!("stat failed for {}: {e}", target.display()))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(format!("not a regular file: {}", target.display()));
    }

    let mut file = std::fs::File::open(&target)
        .map_err(|e| format!("open failed for {}: {e}", target.display()))?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("seek failed for {}: {e}", target.display()))?;
    let len = length.min(8 * 1024 * 1024) as usize;
    let mut buf = vec![0; len];
    // 读满请求长度或读到真 EOF（read 返回 0）为止。单次 read 不保证填满缓冲，
    // 短读会让前端把「短块」误判为 EOF 提前终止上传循环。
    // 契约：返回 Vec 长度 < 请求长度 ⟺ 真 EOF。
    let mut filled = 0usize;
    while filled < len {
        let n = file
            .read(&mut buf[filled..])
            .map_err(|e| format!("read failed for {}: {e}", target.display()))?;
        if n == 0 {
            break; // 真 EOF
        }
        filled += n;
    }
    buf.truncate(filled);
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensitive_path_windows_dirs_any_drive() {
        // C 盘系统目录（原有保护保持）
        assert!(is_sensitive_path(Path::new("C:\\Windows")));
        assert!(is_sensitive_path(Path::new("c:\\WINDOWS\\System32")));
        assert!(is_sensitive_path(Path::new("C:\\Program Files\\App")));
        assert!(is_sensitive_path(Path::new("C:\\Program Files (x86)\\App")));
        assert!(is_sensitive_path(Path::new("C:\\ProgramData\\App")));
        // 任意盘符（系统装在 D:/E: 时同样拦截）
        assert!(is_sensitive_path(Path::new("D:\\Windows")));
        assert!(is_sensitive_path(Path::new("e:\\program files\\app")));
        assert!(is_sensitive_path(Path::new("F:\\ProgramData")));
        // 正斜杠输入同样归一拦截
        assert!(is_sensitive_path(Path::new("D:/Windows/System32")));
    }

    #[test]
    fn test_sensitive_path_drive_root_and_normal_paths() {
        // 盘符根 / 根目录
        assert!(is_sensitive_path(Path::new("C:\\")));
        assert!(is_sensitive_path(Path::new("D:\\")));
        // 普通用户路径不拦截
        assert!(!is_sensitive_path(Path::new("C:\\Users\\me\\file.txt")));
        assert!(!is_sensitive_path(Path::new("D:\\MyData\\windows-backup")));
        assert!(!is_sensitive_path(Path::new("E:\\myprogram files")));
    }

    // ─── v2.5 逃逸形态封堵：verbatim / UNC / 8.3 短名 ───

    #[test]
    fn test_sensitive_path_verbatim_prefix_blocked() {
        // (a) verbatim 前缀：Win32 API 原样透传 `\\?\`，不剥离则不满足
        // 「盘符+\」判断而绕过黑名单。canonicalize 的返回值必带 `\\?\`。
        assert!(is_sensitive_path(Path::new("\\\\?\\C:\\Windows")));
        assert!(is_sensitive_path(Path::new("\\\\?\\C:\\Windows\\System32")));
        assert!(is_sensitive_path(Path::new("\\\\?\\C:\\Program Files\\App")));
        // device 前缀同剥
        assert!(is_sensitive_path(Path::new("\\\\.\\C:\\Windows")));
        // verbatim 的**任意盘符**形态同样按盘内路径比对（不是只认 C:）
        assert!(is_sensitive_path(Path::new("\\\\?\\D:\\Windows")));
        // verbatim 普通路径不误拦
        assert!(!is_sensitive_path(Path::new("\\\\?\\C:\\Users\\me\\file.txt")));
    }

    #[test]
    fn test_sensitive_path_unc_blocked() {
        // (b) UNC：`\\server\share\windows` 首字符 `\` 原本整个绕过盘符分支；
        // 剥掉 server\share 两段后按盘内路径同样比对。
        assert!(is_sensitive_path(Path::new("\\\\fileserver\\share\\Windows")));
        assert!(is_sensitive_path(Path::new("\\\\fileserver\\share\\Program Files\\App")));
        assert!(is_sensitive_path(Path::new("\\\\nas\\public\\programdata")));
        // share 根 / server 根整体拒绝；share 内普通目录不拦
        assert!(is_sensitive_path(Path::new("\\\\fileserver\\share")));
        assert!(!is_sensitive_path(Path::new("\\\\fileserver\\share\\data\\file.txt")));
        assert!(!is_sensitive_path(Path::new("\\\\fileserver\\share\\windows-backup")));
    }

    #[test]
    fn test_sensitive_path_verbatim_unc_and_volume_root_blocked() {
        // (c)(d) verbatim UNC 是同义写法：`\\?\UNC\srv\share\…` ≡ `\\srv\share\…`。
        // 剥 `\\?\` 后剩余串以 `unc\` 开头（不是 `\\`），若不显式识别就会掉进
        // 「盘符形态」分支得到 None，整个系统目录黑名单被跳过——曾放行，随后
        // fs_local_delete 的 remove_dir_all 会递归清空共享目录。
        assert!(is_sensitive_path(Path::new("\\\\?\\UNC\\fileserver\\share\\Windows")));
        assert!(is_sensitive_path(Path::new("\\\\?\\UNC\\fileserver\\share\\Windows\\System32")));
        assert!(is_sensitive_path(Path::new("\\\\?\\UNC\\fileserver\\share\\Program Files\\App")));
        // share 根 / server 根（verbatim 形态）整体拒绝
        assert!(is_sensitive_path(Path::new("\\\\?\\UNC\\fileserver\\share")));
        assert!(is_sensitive_path(Path::new("\\\\?\\UNC\\fileserver")));
        // share 内普通目录不误拦
        assert!(!is_sensitive_path(Path::new("\\\\?\\UNC\\fileserver\\share\\data\\file.txt")));
        assert!(!is_sensitive_path(Path::new("\\\\?\\UNC\\fileserver\\share\\windows-backup")));
        // verbatim 卷根 / GLOBALROOT：无「盘符+盘内路径」结构，无法比对 → 保守拒绝
        assert!(is_sensitive_path(Path::new("\\\\?\\Volume{12345678-1234-1234-1234-123456789abc}\\")));
        assert!(is_sensitive_path(Path::new(
            "\\\\?\\Volume{12345678-1234-1234-1234-123456789abc}\\Windows"
        )));
        assert!(is_sensitive_path(Path::new("\\\\?\\GLOBALROOT\\Device\\HarddiskVolume1")));
    }

    #[test]
    fn test_sensitive_path_short_name_via_canonical_form() {
        // (c) NTFS 8.3 短名（C:\PROGRA~1）字面不匹配黑名单目录名，纯字面
        // 判定拦不住（这是 resolve_input_path 补 canonicalize 的原因）。
        // 短名在 canonicalize 后还原为 `\\?\C:\Program Files`——本测试锁定
        // 「canonicalize 产物形态必被拦截」，即短名经归一后无路可逃：
        //   C:\PROGRA~1 --canonicalize--> \\?\C:\Program Files --(a)剥离--> 命中
        assert!(!is_sensitive_path(Path::new("C:\\PROGRA~1")), "字面形态本就不命中（归一由 resolve_input_path 负责）");
        assert!(is_sensitive_path(Path::new("\\\\?\\C:\\Program Files")), "canonicalize 产物必须命中");
        assert!(is_sensitive_path(Path::new("\\\\?\\C:\\PROGRA~1")) == false, "verbatim 短名仍需 canonicalize 归一，黑名单不做 8.3 展开");
    }
}
