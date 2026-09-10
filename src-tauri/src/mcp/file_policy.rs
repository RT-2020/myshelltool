//! MCP 文件安全与审批策略（file_policy.rs）。
//!
//! 负责：
//! 1. 远程与本地路径规范化（折叠 `.` 和 `..`，防范路径穿越越权）；
//! 2. 敏感凭据/配置文件判定（/etc/shadow、SSH 私钥、.pem、.env 等）；
//! 3. 本机危险系统目录保护（禁止破坏 Windows/Linux 系统目录；清单复用
//!    `fs_local::is_sensitive_path`，全仓单一事实源，不另立第二份黑名单）；
//! 4. 严格契合产品决策：读敏感路径恒审批（凭据红线，不受等级影响）、写/上传/下载/删除按拦截等级判定（Minimal 放行记日志 / Strict 审批）、根级毁灭性删除与本机系统目录写入恒硬拦截。

use std::path::Path;

/// 规范化远程 Unix 风格路径，折叠 `.`、`..` 和冗余斜杠，杜绝路径穿越绕过。
pub fn normalize_remote_path(raw: &str) -> String {
    let raw = raw.trim().replace('\\', "/");
    let is_absolute = raw.starts_with('/');

    let mut parts: Vec<&str> = Vec::new();
    for seg in raw.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            // 遇到 .. 弹出上一级目录，如已到顶则留在顶层
            parts.pop();
        } else {
            parts.push(seg);
        }
    }

    if is_absolute {
        format!("/{}", parts.join("/"))
    } else if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

/// 判定指定的远程路径是否属于高危敏感凭据/密钥/系统文件。
///
/// 命中此清单的文件：
/// - 读操作：即使在 Minimal 档也必须弹窗审批（D2 硬红线）；
/// - 内容本体绝对不录入执行日志（D4 硬红线）。
pub fn is_sensitive_remote_path(raw_path: &str) -> bool {
    let normalized = normalize_remote_path(raw_path);
    // 段匹配补前导分隔符：`sftp_list(path=".")` 返回的条目 path 形如
    // `./.ssh/id_rsa`（russh-sftp 原样拼接请求路径），归一后成 `.ssh/id_rsa`
    // ——相对形态没有前导 `/`，下面所有按 `/.ssh/`、`/etc/...` 的匹配全部落空，
    // 私钥会被当成普通文件**免审批**读出（D2 红线）。统一补一个前导 `/` 作为
    // 段边界哨兵：绝对路径本就是 `/` 开头（幂等），相对路径变成 `/x/y` 形态，
    // 段匹配语义不变（`/.ssh/` 仍要求 `.ssh` 是完整一段）。
    let lower = if normalized.starts_with('/') {
        normalized.to_ascii_lowercase()
    } else {
        format!("/{}", normalized.to_ascii_lowercase())
    };

    // 1. Linux/Unix 核心认证与敏感文件
    let exact_sensitive = [
        "/etc/shadow",
        "/etc/gshadow",
        "/etc/master.passwd",
        "/etc/sudoers",
    ];
    if exact_sensitive.iter().any(|&p| lower == p) {
        return true;
    }

    // 2. /etc/ssh 宿主私钥文件（排除 .pub 公钥）
    if lower.starts_with("/etc/ssh/") && !lower.ends_with(".pub") {
        if lower.contains("key") {
            return true;
        }
    }

    // 3. 用户主目录下的 .ssh 私钥文件
    if lower.contains("/.ssh/") {
        let filename = lower.split('/').last().unwrap_or("");
        // 排除已知的安全公钥或普通非密钥文件
        if !filename.ends_with(".pub")
            && !filename.ends_with("known_hosts")
            && !filename.ends_with("authorized_keys")
            && !filename.ends_with("config")
        {
            if filename.starts_with("id_")
                || filename.ends_with(".pem")
                || filename.ends_with(".key")
                || filename.ends_with(".pkcs12")
                || filename.ends_with(".p12")
            {
                return true;
            }
        }
    }

    // 4. 常见通用密钥与凭据文件后缀
    let sensitive_extensions = [
        ".pem", ".key", ".pkcs12", ".p12", ".pfx", ".kdbx",
    ];
    if sensitive_extensions.iter().any(|&ext| lower.ends_with(ext)) {
        return true;
    }

    // 5. 应用环境凭据文件（.env、数据库密码等）
    let filename = lower.split('/').last().unwrap_or("");
    let sensitive_filenames = [
        ".env",
        ".env.local",
        ".env.production",
        ".env.staging",
        ".env.secret",
        "wp-config.php",
    ];
    if sensitive_filenames.contains(&filename) {
        return true;
    }

    false
}

/// 判定是否属于极度危险的系统级根路径删除（类似于 rm -rf /）。
pub fn is_catastrophic_remote_removal(raw_path: &str) -> bool {
    let normalized = normalize_remote_path(raw_path);
    let lower = normalized.trim_end_matches('/');

    let dangerous_roots = [
        "", // "/" 剥离末尾斜杠后为空
        "/bin",
        "/boot",
        "/dev",
        "/etc",
        "/lib",
        "/lib64",
        "/proc",
        "/root",
        "/sbin",
        "/sys",
        "/usr",
        "/var",
    ];

    dangerous_roots.contains(&lower)
}

/// 检查本机路径是否企图写入或覆盖系统受保护的核心目录（例如 Windows 核心系统目录）。
///
/// 判定基准必须与**落盘基准**一致：`sftp_ops::download_stream` 用同一个字符串做
/// `create_dir_all` / `File::create`，相对路径以进程 CWD 为基准。旧实现只对本字符串
/// 做词法子串比对（`full_str.contains("/windows")`）：`..\..\..\..\..\Windows\Temp\x.dll`
/// 的 `..` 逐级 pop 后 `full_str` 没有前导分隔符 → 三条 contains 全不命中 → 放行 →
/// 文件静默落进 `C:\Windows\Temp`（普通用户默认可写）。
///
/// 与 `fs_local::is_sensitive_path` 的关系（单一事实源）：本机系统目录清单只有那一份，
/// 此处**复用**而非另存一份。旧实现是第二份清单，与 fs_local 互不为子集——缺
/// program files / programdata，又把 `D:\backup\windows\x` 这种「目录名含 windows
/// 但不是系统目录」误拒。两处语义差异只在**归一程度**：fs_local 的 GUI 入口
/// （resolve_input_path）先 absolute、存在时再 canonicalize 才调黑名单；而 MCP 的
/// local_path 是宿主直接给的原始字符串，没有任何前序归一，所以这里自己补
/// absolute 归一与 canonicalize（封 NTFS 8.3 短名 `C:\PROGRA~1` 与指向系统目录的
/// symlink/junction 父目录），并把「无法判定」折叠到敏感一侧。
pub fn is_protected_local_write_path(local_path: &Path) -> bool {
    // 相对路径按进程 CWD 绝对化并归一 `.` / `..`。absolute 失败（空路径、CWD 已
    // 删除等）＝判定基准不可知 → fail-secure 拒绝，绝不退回纯词法比对。
    let absolute = match std::path::absolute(local_path) {
        Ok(p) => p,
        Err(_) => return true,
    };

    if crate::fs_local::is_sensitive_path(&absolute) {
        return true;
    }

    // 目标存在 → 按 canonicalize 后的物理路径再判；目标不存在但父目录存在 →
    // 至少解析父目录（下载会在该父目录下建临时文件再 rename，父目录若是指向系统
    // 目录的 symlink/junction，词法形态完全看不出来）。解析失败＝无法确认目标不属
    // 系统目录 → fail-secure 拒绝（与 fs_local_delete 的删除前策略同口径）。
    let resolvable = if absolute.exists() {
        Some(absolute.clone())
    } else {
        absolute.parent().filter(|p| p.exists()).map(Path::to_path_buf)
    };
    if let Some(probe) = resolvable {
        return match std::fs::canonicalize(&probe) {
            Ok(canon) => crate::fs_local::is_sensitive_path(&canon),
            Err(_) => true,
        };
    }

    // 目标与父目录都不存在：落盘会新建它们，路径上没有任何已存在的间接层
    // （symlink/junction），已绝对化归一的字面判定即最终判定（放行新文件下载）。
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_remote_path() {
        assert_eq!(normalize_remote_path("/var/log/../log/syslog"), "/var/log/syslog");
        assert_eq!(normalize_remote_path("/home/user/../../etc/shadow"), "/etc/shadow");
        assert_eq!(normalize_remote_path("////var////log///"), "/var/log");
        assert_eq!(normalize_remote_path("/../../../"), "/");
        assert_eq!(normalize_remote_path("a/b/../c"), "a/c");
        assert_eq!(normalize_remote_path(""), ".");
    }

    #[test]
    fn test_sensitive_paths() {
        assert!(is_sensitive_remote_path("/etc/shadow"));
        assert!(is_sensitive_remote_path("/home/admin/../../etc/shadow"));
        assert!(is_sensitive_remote_path("/root/.ssh/id_rsa"));
        assert!(is_sensitive_remote_path("/home/deploy/.ssh/id_ed25519"));
        assert!(is_sensitive_remote_path("/var/www/site/.env"));
        assert!(is_sensitive_remote_path("/opt/certs/server.key"));
        assert!(is_sensitive_remote_path("/opt/certs/client.p12"));

        // 普通文件不应误判
        assert!(!is_sensitive_remote_path("/var/log/nginx/access.log"));
        assert!(!is_sensitive_remote_path("/home/user/.ssh/known_hosts"));
        assert!(!is_sensitive_remote_path("/home/user/.ssh/authorized_keys"));
        assert!(!is_sensitive_remote_path("/home/user/.ssh/id_rsa.pub"));
        assert!(!is_sensitive_remote_path("/etc/nginx/nginx.conf"));

        // 相对路径形态（sftp_list(path=".") 的真实返回）：曾因无前导 `/` 而整类漏判
        assert!(is_sensitive_remote_path("./.ssh/id_rsa"));
        assert!(is_sensitive_remote_path(".ssh/id_ed25519"));
        assert!(is_sensitive_remote_path("./.env"));
        assert!(is_sensitive_remote_path("./certs/server.key"));
        // 相对形态下排除项仍要排除（不能把整类相对路径一刀切当敏感）
        assert!(!is_sensitive_remote_path("./.ssh/id_rsa.pub"));
        assert!(!is_sensitive_remote_path("./.ssh/known_hosts"));
    }

    #[test]
    fn test_catastrophic_removal() {
        assert!(is_catastrophic_remote_removal("/"));
        assert!(is_catastrophic_remote_removal("/etc"));
        assert!(is_catastrophic_remote_removal("/etc/"));
        assert!(is_catastrophic_remote_removal("/var/../usr"));
        assert!(!is_catastrophic_remote_removal("/tmp/test.txt"));
        assert!(!is_catastrophic_remote_removal("/var/log/test"));
    }

    // ─── 本机写保护（B-3）：先绝对化归一，再走 fs_local 的单点系统目录清单 ───

    #[test]
    fn local_write_guard_allows_normal_user_paths() {
        // 真实存在的普通用户路径（会走 canonicalize 分支）不得误判
        let normal = std::env::temp_dir().join("myshelltool-policy-probe.txt");
        assert!(!is_protected_local_write_path(&normal));
        // 「目录名含 windows 但不是系统目录」不再误拒（旧实现用 contains("/windows")）
        assert!(!is_protected_local_write_path(Path::new(r"D:\backup\windows\x.dll")));
        assert!(!is_protected_local_write_path(Path::new(r"D:\backup\syswow64\x.dll")));
        assert!(!is_protected_local_write_path(Path::new(r"D:\MyData\windows-backup\x.dll")));
    }

    /// Windows 路径形态（盘符 / UNC / verbatim / 卷）在其他平台上只是一个普通
    /// 文件名，测不出目标语义，故本组用例仅 Windows 编译。
    #[cfg(windows)]
    #[test]
    fn local_write_guard_rejects_system_dirs_via_shared_blacklist() {
        assert!(is_protected_local_write_path(Path::new(r"C:\Windows\Temp\x.dll")));
        // 收敛到 fs_local 的清单后，program files / programdata 同样受保护
        assert!(is_protected_local_write_path(Path::new(r"C:\Program Files\x.dll")));
        assert!(is_protected_local_write_path(Path::new(r"C:\ProgramData\x.dll")));
        // SysWOW64 必在 \Windows\ 之下，由 "windows" 前缀命中
        assert!(is_protected_local_write_path(Path::new(r"C:\Windows\SysWOW64\x.dll")));
        // verbatim / verbatim UNC / 卷形态：旧实现三条 contains 全部不命中
        assert!(is_protected_local_write_path(Path::new(r"\\?\C:\Windows\Temp\x.dll")));
        assert!(is_protected_local_write_path(Path::new(
            r"\\?\UNC\fileserver\share\Windows\x.dll"
        )));
        assert!(is_protected_local_write_path(Path::new(
            r"\\?\Volume{12345678-1234-1234-1234-123456789abc}\x.dll"
        )));
    }

    #[cfg(windows)]
    #[test]
    fn local_write_guard_rejects_relative_parent_dir_escape() {
        // 相对路径的基准是进程 CWD（落盘 create_dir_all/File::create 同基准）：
        // `..` 给够时在盘符根饱和（GetFullPathName 语义），归一后落进 Windows 目录。
        // 旧实现逐级 pop 后 full_str 无前导分隔符 → 三条 contains 全不命中 → 放行。
        let depth = std::env::current_dir().expect("cwd").components().count();
        let escape = std::iter::repeat("..")
            .take(depth + 2)
            .collect::<Vec<_>>()
            .join("\\");
        let escaped = format!(r"{escape}\Windows\Temp\payload.dll");
        assert!(
            is_protected_local_write_path(Path::new(&escaped)),
            "相对路径逃逸到系统目录必须拒绝: {escaped}"
        );
        // 无法绝对化（空路径，absolute 返回 InvalidInput）＝基准不可知 → 保守拒绝
        assert!(is_protected_local_write_path(Path::new("")));
        // 8.3 短名经父目录 canonicalize 还原后命中（PROGRA~1 在标准 Windows 上必存在；
        // 缺失时跳过断言并向 stderr 说明，避免机器差异导致的假失败）
        if Path::new(r"C:\PROGRA~1").exists() {
            assert!(is_protected_local_write_path(Path::new(r"C:\PROGRA~1\x.dll")));
        } else {
            eprintln!("跳过 8.3 短名断言：本机不存在 C:\\PROGRA~1");
        }
    }
}
