//! MCP 工具的 audit_command / approval 文案构造（v0.20 自 registry.rs 拆出，
//! S2 后续刀）。与旧 server.rs 逐工具 match 逐字一致。

use serde_json::{Map, Value};

use super::registry::{arg_str, arg_str_or, arg_bool};

// ─── audit_command / approval 文案构造（与旧 server.rs 逐工具 match 逐字一致）───

pub(crate) fn audit_ssh_exec(args: &Map<String, Value>) -> String {
    arg_str(args, "command")
}

pub(crate) fn audit_sftp_list(args: &Map<String, Value>) -> String {
    format!("list {}", arg_str_or(args, "path", "."))
}

pub(crate) fn audit_sftp_read_file(args: &Map<String, Value>) -> String {
    format!("read {}", arg_str(args, "path"))
}

pub(crate) fn audit_sftp_write_file(args: &Map<String, Value>) -> String {
    format!("write {}", arg_str(args, "path"))
}

pub(crate) fn audit_sftp_upload(args: &Map<String, Value>) -> String {
    format!(
        "upload {} -> {}",
        arg_str(args, "local_path"),
        arg_str(args, "remote_path")
    )
}

pub(crate) fn audit_sftp_download(args: &Map<String, Value>) -> String {
    format!(
        "download {} -> {}",
        arg_str(args, "remote_path"),
        arg_str(args, "local_path")
    )
}

pub(crate) fn audit_sftp_remove(args: &Map<String, Value>) -> String {
    format!("remove {}", arg_str(args, "path"))
}

pub(crate) fn approval_cmd_sftp_read_file(args: &Map<String, Value>) -> String {
    format!("sftp_read_file path={}", arg_str(args, "path"))
}

pub(crate) fn approval_cmd_sftp_write_file(args: &Map<String, Value>) -> String {
    format!("sftp_write_file path={}", arg_str(args, "path"))
}

pub(crate) fn approval_cmd_sftp_upload(args: &Map<String, Value>) -> String {
    format!(
        "sftp_upload {} -> {}",
        arg_str(args, "local_path"),
        arg_str(args, "remote_path")
    )
}

pub(crate) fn approval_cmd_sftp_download(args: &Map<String, Value>) -> String {
    format!(
        "sftp_download {} -> {}",
        arg_str(args, "remote_path"),
        arg_str(args, "local_path")
    )
}

pub(crate) fn approval_cmd_sftp_remove(args: &Map<String, Value>) -> String {
    format!(
        "sftp_remove path={} recursive={}",
        arg_str(args, "path"),
        arg_bool(args, "recursive")
    )
}

pub(crate) fn approval_consequence_sftp_read_file(_: &Map<String, Value>) -> String {
    "检测到目标路径属于系统敏感文件、SSH 私钥或应用环境凭据，请核对是否授权读取。".to_string()
}

pub(crate) fn approval_consequence_sftp_write_file(_: &Map<String, Value>) -> String {
    "将向远程目标路径原子写入文件。若文件已存在将被完全覆盖，请核验路径与内容。".to_string()
}

pub(crate) fn approval_consequence_sftp_upload(_: &Map<String, Value>) -> String {
    "将上传本机文件并覆盖远程已有文件，请核对源路径与目标路径。".to_string()
}

pub(crate) fn approval_consequence_sftp_download(args: &Map<String, Value>) -> String {
    // 三态探测（exists() 在权限/IO 错误时也返回 false，会把「实际会覆盖」误报成
    // 「不存在」）：Ok=已存在 / NotFound=不存在 / 其他 Err=无法确认，按「可能存在」
    // 给覆盖警告（保守，与前端 probeRemoteTarget 同哲学）。
    let local_str = arg_str(args, "local_path");
    let is_overwrite = match std::fs::metadata(std::path::Path::new(&local_str)) {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => true,
    };
    if is_overwrite {
        "【注意】本机目标文件已存在，下载将覆盖本机旧文件！".to_string()
    } else {
        "将在本机保存远程下载的文件。".to_string()
    }
}

pub(crate) fn approval_consequence_sftp_remove(args: &Map<String, Value>) -> String {
    let mode_warning = if arg_bool(args, "recursive") {
        "【危险】将递归删除整个目录及其内部所有子文件与子目录！"
    } else {
        "将删除远程单文件或空目录。"
    };
    format!("{mode_warning}此操作不可撤销。")
}

