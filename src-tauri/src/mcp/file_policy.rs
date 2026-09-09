//! MCP 文件安全与审批策略（file_policy.rs）。
//!
//! 负责：
//! 1. 远程与本地路径规范化（折叠 `.` 和 `..`，防范路径穿越越权）；
//! 2. 敏感凭据/配置文件判定（/etc/shadow、SSH 私钥、.pem、.env 等）；
//! 3. 本机危险系统目录保护（禁止破坏 Windows/Linux 系统目录）；
//! 4. 严格契合产品决策：读敏感路径审批、写/覆盖/删除恒审批、根级毁灭性删除硬拦截。

use std::path::{Component, Path};

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
    let lower = normalized.to_ascii_lowercase();

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
pub fn is_protected_local_write_path(local_path: &Path) -> bool {
    // 规范化本机路径
    let mut normalized_components = Vec::new();
    for comp in local_path.components() {
        match comp {
            Component::Prefix(p) => normalized_components.push(p.as_os_str().to_string_lossy().to_ascii_lowercase()),
            Component::RootDir => normalized_components.push("/".to_string()),
            Component::Normal(n) => normalized_components.push(n.to_string_lossy().to_ascii_lowercase()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized_components.pop();
            }
        }
    }

    let full_str = normalized_components.join("/");
    // Windows 核心系统目录防护
    if full_str.contains("/windows") || full_str.contains("/system32") || full_str.contains("/syswow64") {
        return true;
    }
    // Unix 核心系统目录防护（若在 Unix 环境运行）
    if full_str.starts_with("/bin")
        || full_str.starts_with("/sbin")
        || full_str.starts_with("/usr")
        || full_str.starts_with("/etc")
    {
        return true;
    }

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
}
