//! MCP 工具 schema 构造（v0.20 自 registry.rs 拆出，S2 后续刀：注册表要加行，
//! registry.rs 贴 800 行 Rust 硬上限）。json! 字面量与迁移前逐字一致——
//! scripts/mcp-tools-snapshot.mjs 快照比对为证。

use serde_json::{json, Map, Value};

// ─── schema 构造（json! 字面量保持与迁移前逐字一致，快照比对为证）───
// empty_object_schema / schema_with_required_session 复用 tools.rs 的既有定义
// （不重复造轮子）；本文件新增的只是文件工具原 schema_obj(json!(...)) 的平移。

pub(crate) fn schema_service_status() -> Map<String, Value> {
    json!({
        "type": "object",
        "properties": {
            "asset_id": { "type": "string", "description": "资产 ID" },
            "service": { "type": "string", "description": "服务名，如 nginx / mysql / docker" }
        },
        "required": ["asset_id", "service"]
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

pub(crate) fn schema_ssh_exec() -> Map<String, Value> {
    json!({
        "type": "object",
        "properties": {
            "asset_id": { "type": "string", "description": "资产 ID（先用 list_assets 查看）" },
            "command": { "type": "string", "description": "要执行的 Shell 命令" },
            "intent": { "type": "string", "description": "AI 对此命令的真实意图说明（用于审批对照识破伪装）" }
        },
        "required": ["asset_id", "command", "intent"]
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

pub(crate) fn schema_sftp_list() -> Map<String, Value> {
    json!({
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
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

pub(crate) fn schema_sftp_read_file() -> Map<String, Value> {
    json!({
        "type": "object",
        "properties": {
            "asset_id": {
                "type": "string",
                "description": "目标资产 ID"
            },
            "path": {
                "type": "string",
                "description": "待读取的远程文件绝对路径或相对路径，如 /etc/hosts 或 /var/log/syslog"
            },
            "offset": {
                "type": "integer",
                "description": "起始字节偏移（默认 0）。大文件首读被截断时，响应里会给出 totalSize 与 nextOffset，续读传它即可（每次调用照常过敏感路径审批）"
            },
            "maxBytes": {
                "type": "integer",
                "description": "单次读取字节上限（默认 262144，最大 4194304）。超限不再报错——返回前 maxBytes 字节 + 续读提示"
            }
        },
        "required": ["asset_id", "path"]
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

pub(crate) fn schema_read_output() -> Map<String, Value> {
    json!({
        "type": "object",
        "properties": {
            "cursor": {
                "type": "string",
                "description": "ssh_exec 截断提示里给出的取回凭据（10 分钟内有效）"
            },
            "offset": {
                "type": "integer",
                "description": "起始字节偏移（默认 0，响应里带下一段 offset）"
            },
            "limit": {
                "type": "integer",
                "description": "本段字节上限（默认 65536，最大 1048576）"
            }
        },
        "required": ["cursor"]
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

pub(crate) fn schema_sftp_write_file() -> Map<String, Value> {
    json!({
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
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

pub(crate) fn schema_sftp_upload() -> Map<String, Value> {
    json!({
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
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

pub(crate) fn schema_sftp_download() -> Map<String, Value> {
    json!({
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
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

pub(crate) fn schema_sftp_remove() -> Map<String, Value> {
    json!({
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
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}


pub(crate) fn schema_exec_many() -> Map<String, Value> {
    json!({
        "type": "object",
        "properties": {
            "asset_ids": {
                "type": "array",
                "items": { "type": "string" },
                "description": "目标资产 ID 列表（先用 list_assets 查看；单次上限 64 台）"
            },
            "command": {
                "type": "string",
                "description": "在全部目标上执行的同一 Shell 命令"
            },
            "intent": {
                "type": "string",
                "description": "AI 对此命令的真实意图说明（用于审批对照识破伪装；一次审批放行整批——命令文本相同）"
            }
        },
        "required": ["asset_ids", "command", "intent"]
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

pub(crate) fn schema_ssh_exec_async() -> Map<String, Value> {
    json!({
        "type": "object",
        "properties": {
            "asset_id": { "type": "string", "description": "目标资产 ID（先用 list_assets 查看）" },
            "command": { "type": "string", "description": "要执行的长时 Shell 命令" },
            "intent": { "type": "string", "description": "AI 对此命令的真实意图说明（用于审批对照识破伪装）" }
        },
        "required": ["asset_id", "command", "intent"]
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

pub(crate) fn schema_job_ref() -> Map<String, Value> {
    json!({
        "type": "object",
        "properties": {
            "job_id": { "type": "string", "description": "ssh_exec_async 返回的 job_id" }
        },
        "required": ["job_id"]
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

pub(crate) fn schema_job_output() -> Map<String, Value> {
    json!({
        "type": "object",
        "properties": {
            "job_id": { "type": "string", "description": "ssh_exec_async 返回的 job_id" },
            "offset": { "type": "integer", "description": "起始字节偏移（默认 0，响应里带 EOF 与总字节数）" },
            "limit": { "type": "integer", "description": "本段字节上限（默认 65536，最大 1048576）" }
        },
        "required": ["job_id"]
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}
pub(crate) fn schema_journal_query() -> Map<String, Value> {
    json!({
        "type": "object",
        "properties": {
            "asset_id": { "type": "string", "description": "目标资产 ID（先用 list_assets 查看）" },
            "unit": { "type": "string", "description": "systemd 单元名过滤（如 nginx / sshd），可选" },
            "since": { "type": "string", "description": "起始时间（journalctl --since 语法，如 ’2026-01-01 00:00:00‘ 或 ’-1h‘），可选" },
            "until": { "type": "string", "description": "结束时间（同 since 语法），可选" },
            "grep": { "type": "string", "description": "输出后置过滤（固定字符串匹配，大小写敏感），可选" },
            "limit": { "type": "integer", "description": "返回行数上限（默认 100，最大 2000）" }
        },
        "required": ["asset_id"]
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}
pub(crate) fn schema_port_listen() -> Map<String, Value> {
    json!({
        "type": "object",
        "properties": {
            "asset_id": { "type": "string", "description": "目标资产 ID（先用 list_assets 查看）" },
            "tcpOnly": { "type": "boolean", "description": "仅 TCP 端口（默认 false = TCP+UDP）" }
        },
        "required": ["asset_id"]
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}
pub(crate) fn schema_process_list() -> Map<String, Value> {
    json!({
        "type": "object",
        "properties": {
            "asset_id": { "type": "string", "description": "目标资产 ID（先用 list_assets 查看）" },
            "sortBy": { "type": "string", "enum": ["cpu", "mem"], "description": "排序列（默认 mem）" },
            "limit": { "type": "integer", "description": "返回进程数上限（默认 20，最大 200）" }
        },
        "required": ["asset_id"]
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}
pub(crate) fn schema_file_search() -> Map<String, Value> {
    json!({
        "type": "object",
        "properties": {
            "asset_id": { "type": "string", "description": "目标资产 ID（先用 list_assets 查看）" },
            "path": { "type": "string", "description": "起始目录（如 /var/log；不允许空格与分号）" },
            "name": { "type": "string", "description": "文件名通配模式（如 *.log、nginx*；find -name 语义）" },
            "minSizeMb": { "type": "integer", "description": "最小体积（MB，find -size +NM 语义）" },
            "mtimeDays": { "type": "integer", "description": "修改时间过滤：负值=最近 N 天内修改（-mtime -N），正值=超过 N 天未改（-mtime +N）" },
            "limit": { "type": "integer", "description": "结果上限（默认 100，最大 1000——超限自动截断防炸上下文）" }
        },
        "required": ["asset_id", "path"]
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}
pub(crate) fn schema_service_control() -> Map<String, Value> {
    json!({
        "type": "object",
        "properties": {
            "asset_id": { "type": "string", "description": "目标资产 ID" },
            "service": { "type": "string", "description": "systemd 服务名（如 nginx / docker）" },
            "action": { "type": "string", "enum": ["start", "stop", "restart", "reload"], "description": "生命周期操作" }
        },
        "required": ["asset_id", "service", "action"]
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

pub(crate) fn schema_tunnel_list() -> Map<String, Value> {
    json!({ "type": "object", "properties": {} })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

pub(crate) fn schema_tunnel_create() -> Map<String, Value> {
    json!({
        "type": "object",
        "properties": {
            "asset_id": { "type": "string", "description": "remote 转发的认证资产 ID" },
            "kind": { "type": "string", "enum": ["remote"], "description": "MVP 仅支持 remote（专用连接）；local/dynamic 请用 GUI" },
            "bindAddr": { "type": "string", "description": "服务器侧监听地址（默认 127.0.0.1）" },
            "localPort": { "type": "integer", "description": "服务器侧监听端口（remote 转发的对外端口）" },
            "remoteAddr": { "type": "string", "description": "转发目标地址（remote 转发到达后连向哪里）" },
            "remotePort": { "type": "integer", "description": "转发目标端口" }
        },
        "required": ["asset_id", "kind", "localPort", "remoteAddr", "remotePort"]
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}
