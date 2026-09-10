//! MCP ServerHandler 实现（协议层，transport 无关）。
//!
//! v1.4：transport 从 stdio 改为 Streamable HTTP（见 http_server.rs），
//! 但本文件的 ServerHandler impl（get_info/list_tools/call_tool/...）
//! 完全 transport 无关 —— http_server.rs 把 MyshellToolMcpServer 喂给
//! StreamableHttpService::new 即可，协议层零改动。
//!
//! v1.5：审批降级补全。elicitation 仍是主路径；客户端不支持 elicitation 时
//!（如 ZCode）不再直接 fail-secure 拒绝，而是经 AppHandle emit 事件给前端
//! GlobalModals 弹窗（同进程 GUI 审批，比 Arcade 的 URL 重定向更轻）。
//! 无 GUI 句柄（headless/测试）才退回 fail-secure 拒绝。这是补齐 v1.4 标注的
//! follow-up（server.rs:88 旧注释 TODO）。
//!
//! v2：① 审批按用户可配置的拦截等级判定（Minimal 默认仅拦黑名单/超高危，
//! Strict 非白名单一律确认，配置见 config.rs，每次 call_tool 现读快照）；
//! ② call_tool 为真实触发远程执行的工具调用记执行日志（execution_log.rs）。
//! v2.1：Minimal 语义重构为零审批交互——毁灭性（catastrophic）命令
//! HardBlock 直接拒绝（不进审批链，见下方 call_tool），非毁灭黑名单放行
//! 记 minimal_allowed 日志；Strict 审批语义不变，毁灭性命令同样硬拦。

use std::sync::Arc;
use std::time::Duration;

use rmcp::{
    ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult,
        Implementation, InitializeResult, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams,
        ReadResourceResult, ServerCapabilities, ServerInfo,
    },
    service::{Peer, RequestContext},
    ErrorData as McpError,
};
use tauri::Emitter;

use super::approval::{self, ApprovalDecision, AutoApproveReason, ElicitationInfo, McpApprovalEvent};
use super::config::McpInterceptLevel;
use super::execution_log::{self, decision, outcome, ExecutionLogEntry};
use super::tools::{self, McpToolContext};

// ── v1.1 审批辅助 ──

/// 审批通过的来源（v2：执行日志 decision 需区分 elicitation 框 / GUI 弹窗，
/// 用枚举携带而非字符串匹配）。
enum ApprovalVia {
    Elicitation,
    Gui,
}

/// elicitation 结果。
enum ElicitOutcome {
    /// 用户确认执行（区分来源：elicitation 框 / GUI 弹窗）。
    Accepted { via: ApprovalVia },
    /// 用户拒绝（区分来源；reason 是给调用方的错误文本）。
    Declined { via: ApprovalVia, reason: String },
    /// GUI 弹窗 60s 超时（v2 从 NotSupported 中拆出，decision=timeout）。
    Timeout,
    /// emit 失败 / headless 无 GUI 的 fail-secure 拒（decision=rejected）。
    NotSupported(String),
}

impl ApprovalVia {
    /// 执行日志 decision 字段值（接受路径）。
    fn accepted_decision(&self) -> &'static str {
        match self {
            Self::Elicitation => decision::ELICITATION_ACCEPTED,
            Self::Gui => decision::GUI_ACCEPTED,
        }
    }

    /// 执行日志 decision 字段值（拒绝路径）。
    fn declined_decision(&self) -> &'static str {
        match self {
            Self::Elicitation => decision::ELICITATION_DECLINED,
            Self::Gui => decision::GUI_DECLINED,
        }
    }
}

/// 执行日志的记录范围：真实触发远程执行的工具调用所需的静态信息。
///
/// v2：所有真实触发远程执行的工具（ssh_exec / disk_usage / system_status /
/// service_status / sftp_remove）记日志；list_assets / list_sessions / 桩工具 /
/// 未知工具不记。command 填真实执行的命令文本（只读工具用 tools.rs 导出的
/// 固定命令常量，sftp_remove 填目标路径）。
struct LogScope {
    tool: &'static str,
    command: String,
    asset_id: String,
    intent: String,
}

fn log_scope_for(
    tool_name: &str,
    arguments: &Option<serde_json::Map<String, serde_json::Value>>,
) -> Option<LogScope> {
    let args = arguments.as_ref()?;
    let asset_id = args
        .get("asset_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let intent = args
        .get("intent")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    match tool_name {
        "ssh_exec" => Some(LogScope {
            tool: "ssh_exec",
            command: args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            asset_id,
            intent,
        }),
        "disk_usage" => Some(LogScope {
            tool: "disk_usage",
            command: tools::CMD_DISK_USAGE.to_string(),
            asset_id,
            intent,
        }),
        "system_status" => Some(LogScope {
            tool: "system_status",
            command: tools::CMD_SYSTEM_STATUS.to_string(),
            asset_id,
            intent,
        }),
        "service_status" => {
            let service = args.get("service").and_then(|v| v.as_str()).unwrap_or("");
            Some(LogScope {
                tool: "service_status",
                command: tools::service_status_command(service),
                asset_id,
                intent,
            })
        }
        "sftp_remove" => Some(LogScope {
            tool: "sftp_remove",
            command: format!(
                "remove {}",
                args.get("path").and_then(|v| v.as_str()).unwrap_or("")
            ),
            asset_id,
            intent,
        }),
        "sftp_list" => Some(LogScope {
            tool: "sftp_list",
            command: format!(
                "list {}",
                args.get("path").and_then(|v| v.as_str()).unwrap_or(".")
            ),
            asset_id,
            intent,
        }),
        "sftp_read_file" => Some(LogScope {
            tool: "sftp_read_file",
            command: format!(
                "read {}",
                args.get("path").and_then(|v| v.as_str()).unwrap_or("")
            ),
            asset_id,
            intent,
        }),
        "sftp_write_file" => Some(LogScope {
            tool: "sftp_write_file",
            command: format!(
                "write {}",
                args.get("path").and_then(|v| v.as_str()).unwrap_or("")
            ),
            asset_id,
            intent,
        }),
        "sftp_upload" => Some(LogScope {
            tool: "sftp_upload",
            command: format!(
                "upload {} -> {}",
                args.get("local_path").and_then(|v| v.as_str()).unwrap_or(""),
                args.get("remote_path").and_then(|v| v.as_str()).unwrap_or("")
            ),
            asset_id,
            intent,
        }),
        "sftp_download" => Some(LogScope {
            tool: "sftp_download",
            command: format!(
                "download {} -> {}",
                args.get("remote_path").and_then(|v| v.as_str()).unwrap_or(""),
                args.get("local_path").and_then(|v| v.as_str()).unwrap_or("")
            ),
            asset_id,
            intent,
        }),
        "resource_monitor_snapshot" => Some(LogScope {
            tool: "resource_monitor_snapshot",
            command: "snapshot".to_string(),
            asset_id,
            intent,
        }),
        _ => None,
    }
}

/// 对高危工具（ssh_exec / 文件写入删除类）做审批判定。
///
/// 返回 None 表示该工具不需要审批（只读工具），Some 表示需要审批决策。
/// v2：读 ctx 共享配置的当前拦截等级（每次调用现读快照，改档即生效）。
/// v2.3：文件工具（sftp_write_file / sftp_upload / sftp_download /
/// sftp_remove）纳入拦截等级体系——Minimal 直接放行（AutoExecute →
/// 执行日志 decision=minimal_allowed，由 call_tool 统一记日志），Strict
/// 维持人工确认。恒定例外（不受等级影响，判定先于/独立于等级分支）：
/// - sftp_remove 根级/核心目录删除（is_catastrophic_remote_removal）→
///   HardBlock，两档恒拒；
/// - sftp_download 本机系统受保护目录（is_protected_local_write_path）→
///   HardBlock，两档恒拒；
/// - sftp_read_file 敏感凭据路径读取（is_sensitive_remote_path）→ 恒审批
///   （凭据红线，不纳入等级体系）。
async fn check_approval_needed(
    ctx: &McpToolContext,
    tool_name: &str,
    arguments: &Option<serde_json::Map<String, serde_json::Value>>,
) -> Option<ApprovalDecision> {
    let args = arguments.as_ref()?;
    match tool_name {
        "ssh_exec" => {
            let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let intent = args.get("intent").and_then(|v| v.as_str()).unwrap_or("");
            if command.is_empty() {
                return None;
            }
            let level = ctx.config.read().await.level;
            Some(approval::evaluate(
                command,
                intent,
                super::approval::READONLY_WHITELIST,
                &[],
                level,
            ))
        }
        // sftp_remove：根级/核心目录删除两档恒 HardBlock（先于等级判定）；
        // 其余删除 v2.3 纳入等级体系——Minimal 放行记日志 / Strict 人工确认
        "sftp_remove" => {
            let intent = args.get("intent").and_then(|v| v.as_str()).unwrap_or("");
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if super::file_policy::is_catastrophic_remote_removal(path) {
                return Some(ApprovalDecision::HardBlock {
                    reason: format!("拒绝执行：目标路径 '{path}' 属于系统根目录或顶级核心目录，禁止删除。"),
                });
            }
            let level = ctx.config.read().await.level;
            let recursive = args.get("recursive").and_then(|v| v.as_bool()).unwrap_or(false);
            let mode_warning = if recursive {
                "【危险】将递归删除整个目录及其内部所有子文件与子目录！"
            } else {
                "将删除远程单文件或空目录。"
            };
            match level {
                McpInterceptLevel::Minimal => {
                    Some(ApprovalDecision::AutoExecute(AutoApproveReason::MinimalFallback))
                }
                McpInterceptLevel::Strict => {
                    Some(ApprovalDecision::RequestElicitation(ElicitationInfo {
                        intent: intent.to_string(),
                        command: format!("sftp_remove path={} recursive={}", path, recursive),
                        consequence: format!("{mode_warning}此操作不可撤销。"),
                    }))
                }
            }
        }
        "sftp_read_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if super::file_policy::is_sensitive_remote_path(path) {
                Some(ApprovalDecision::RequestElicitation(ElicitationInfo {
                    intent: args.get("intent").and_then(|v| v.as_str()).unwrap_or("读取敏感系统凭据/配置").to_string(),
                    command: format!("sftp_read_file path={}", path),
                    consequence: "检测到目标路径属于系统敏感文件、SSH 私钥或应用环境凭据，请核对是否授权读取。".to_string(),
                }))
            } else {
                Some(ApprovalDecision::AutoExecute(AutoApproveReason::Whitelist))
            }
        }
        // v2.3 纳入等级体系：Minimal 放行记日志 / Strict 人工确认
        "sftp_write_file" => {
            let intent = args.get("intent").and_then(|v| v.as_str()).unwrap_or("");
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let level = ctx.config.read().await.level;
            match level {
                McpInterceptLevel::Minimal => {
                    Some(ApprovalDecision::AutoExecute(AutoApproveReason::MinimalFallback))
                }
                McpInterceptLevel::Strict => {
                    Some(ApprovalDecision::RequestElicitation(ElicitationInfo {
                        intent: intent.to_string(),
                        command: format!("sftp_write_file path={}", path),
                        consequence: "将向远程目标路径原子写入文件。若文件已存在将被完全覆盖，请核验路径与内容。".to_string(),
                    }))
                }
            }
        }
        // v2.3 纳入等级体系：Minimal 放行记日志 / Strict 人工确认
        "sftp_upload" => {
            let intent = args.get("intent").and_then(|v| v.as_str()).unwrap_or("");
            let local = args.get("local_path").and_then(|v| v.as_str()).unwrap_or("");
            let remote = args.get("remote_path").and_then(|v| v.as_str()).unwrap_or("");
            let level = ctx.config.read().await.level;
            match level {
                McpInterceptLevel::Minimal => {
                    Some(ApprovalDecision::AutoExecute(AutoApproveReason::MinimalFallback))
                }
                McpInterceptLevel::Strict => {
                    Some(ApprovalDecision::RequestElicitation(ElicitationInfo {
                        intent: intent.to_string(),
                        command: format!("sftp_upload {} -> {}", local, remote),
                        consequence: "将上传本机文件并覆盖远程已有文件，请核对源路径与目标路径。".to_string(),
                    }))
                }
            }
        }
        // 本机系统受保护目录两档恒 HardBlock（先于等级判定）；
        // 其余 v2.3 纳入等级体系：Minimal 放行记日志 / Strict 人工确认
        "sftp_download" => {
            let intent = args.get("intent").and_then(|v| v.as_str()).unwrap_or("");
            let remote = args.get("remote_path").and_then(|v| v.as_str()).unwrap_or("");
            let local_str = args.get("local_path").and_then(|v| v.as_str()).unwrap_or("");
            let local_path = std::path::Path::new(local_str);
            if super::file_policy::is_protected_local_write_path(local_path) {
                return Some(ApprovalDecision::HardBlock {
                    reason: format!("拒绝执行：本机下载路径 '{local_str}' 属于系统受保护核心目录。"),
                });
            }
            let level = ctx.config.read().await.level;
            // 三态探测（exists() 在权限/IO 错误时也返回 false，会把「实际会覆盖」
            // 误报成「不存在」）：Ok=已存在 / NotFound=不存在 / 其他 Err=无法确认，
            // 按「可能存在」给覆盖警告（保守，与前端 probeRemoteTarget 同哲学）。
            let is_overwrite = match std::fs::metadata(local_path) {
                Ok(_) => true,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(_) => true,
            };
            let warn = if is_overwrite {
                "【注意】本机目标文件已存在，下载将覆盖本机旧文件！"
            } else {
                "将在本机保存远程下载的文件。"
            };
            match level {
                McpInterceptLevel::Minimal => {
                    Some(ApprovalDecision::AutoExecute(AutoApproveReason::MinimalFallback))
                }
                McpInterceptLevel::Strict => {
                    Some(ApprovalDecision::RequestElicitation(ElicitationInfo {
                        intent: intent.to_string(),
                        command: format!("sftp_download {} -> {}", remote, local_str),
                        consequence: warn.to_string(),
                    }))
                }
            }
        }
        _ => None,
    }
}

/// 资产元数据兜底：读库失败或 assetId 不存在时其余字段空串（port=0），
/// 不丢日志条目（assetId 保留原值）。
fn load_asset_meta(ctx: &McpToolContext, asset_id: &str) -> (String, String, u16, String) {
    let empty = || (String::new(), String::new(), 0, String::new());
    let Ok(store) = myshelltool_core::load_connection_asset_store(&ctx.asset_store_path) else {
        return empty();
    };
    match store.assets.iter().find(|a| a.id == asset_id) {
        Some(a) => (a.name.clone(), a.host.clone(), a.port, a.username.clone()),
        None => empty(),
    }
}

/// 落一条执行日志（append_entry 内部 best-effort，失败不阻断工具调用）。
async fn append_execution_log(
    ctx: &McpToolContext,
    scope: &LogScope,
    level_str: &str,
    decision_str: &str,
    outcome_str: &str,
    output_text: &str,
) {
    let (asset_name, host, port, username) = load_asset_meta(ctx, &scope.asset_id);
    let entry = ExecutionLogEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp_ms: execution_log::now_ms(),
        level: level_str.to_string(),
        tool: scope.tool.to_string(),
        asset_id: scope.asset_id.clone(),
        asset_name,
        host,
        port,
        username,
        // 脱敏后落盘：命令原文可能含明文口令（`mysql -pP@ss`、`curl -u u:p`、
        // `--password=x`），而执行日志是**长期保留**的审计文件（30 天 / 1000 条），
        // 且会在 GUI 面板回显——违反 AGENTS.md §8「凭据不进日志」（形态 C 变体：
        // 不是折叠错误，而是把敏感值当普通文本落盘）。
        command: myshelltool_core::redact_command(&scope.command),
        intent: scope.intent.clone(),
        decision: decision_str.to_string(),
        outcome: outcome_str.to_string(),
        output_summary: execution_log::summarize_output(output_text),
    };
    execution_log::append_entry(&ctx.data_dir, entry).await;
}

/// 从 CallToolResult 提取全部 text content（拼成日志的 outputSummary 素材）。
fn extract_result_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 客户端不支持 elicitation（或 elicitation 失败/被自动拒绝）时的降级路径。
///
/// v1.5：三级降级（elicitation → GUI 弹窗 → fail-secure 拒绝）。
/// - McpToolContext 有 AppHandle（GUI 在线）→ emit `mcp-tool-approval` 事件，
///   前端 GlobalModals 弹窗让用户确认。模式照 ssh.rs:55-153 host-key 验证：
///   注册 oneshot::Sender 到 pending 表 + emit + 60s timeout 等待。
/// - 无 AppHandle（headless/测试/probe）→ 退回 fail-secure 拒绝（v1.4 行为）。
///
/// 这是补齐 v1.4 标注的 follow-up（旧 server.rs:88 TODO）。MCP 官方博客把
/// 安全保证定位为 deterministic runtime control——本函数就是那条 runtime 路径：
/// elicitation 只是 UX hint，真正「用户确认」必须落到本函数的确定性等待。
async fn degrade_to_pipe_or_reject(
    ctx: &McpToolContext,
    info: &ElicitationInfo,
) -> ElicitOutcome {
    // 无 GUI 句柄（headless/测试）→ 退回 fail-secure 拒绝（保持 v1.4 行为）
    let Some(app) = ctx.app_handle.as_ref() else {
        log::warn!(
            "elicitation not supported and no GUI handle, fail-secure reject: {}",
            info.command
        );
        return ElicitOutcome::NotSupported(info.to_rejection());
    };

    // 有 GUI：照 ssh.rs:91-136 模板——注册 oneshot + emit + timeout 等待
    let request_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut map = ctx.approval_pending.lock().await;
        map.insert(request_id.clone(), tx);
    }
    // 命令文本在**日志**里脱敏（§8 凭据红线）；下面 emit 给 GUI 弹窗的仍是原命令——
    // 审批透明性优先：用户必须看到真实要执行的东西（含其中的口令）才能判断。
    log::info!(
        "mcp approval: emitting GUI prompt request_id={}, command={}",
        request_id,
        myshelltool_core::redact_command(&info.command)
    );

    let event = McpApprovalEvent {
        request_id: request_id.clone(),
        intent: info.intent.clone(),
        command: info.command.clone(),
        consequence: info.consequence.clone(),
    };
    if app.emit("mcp-tool-approval", event).is_err() {
        // emit 失败（GUI 未正常响应）→ 清理 pending + 退回拒绝
        log::warn!("mcp approval: emit failed, cleaning up {}", request_id);
        let mut map = ctx.approval_pending.lock().await;
        map.remove(&request_id);
        return ElicitOutcome::NotSupported(info.to_rejection());
    }

    // 60s 超时，与 ssh.rs:118 host-key 验证对齐。前端 watcher 65s 兜底清理。
    // v2：超时从 NotSupported 拆成独立 Timeout variant（执行日志 decision=timeout，
    // 与 emit 失败/headless 拒绝的 decision=rejected 区分）。
    match tokio::time::timeout(Duration::from_secs(60), rx).await {
        Ok(Ok(accepted)) => {
            if accepted {
                log::info!("mcp approval: user accepted {}", request_id);
                ElicitOutcome::Accepted {
                    via: ApprovalVia::Gui,
                }
            } else {
                log::info!("mcp approval: user declined {}", request_id);
                ElicitOutcome::Declined {
                    via: ApprovalVia::Gui,
                    reason: "用户在 GUI 弹窗中拒绝".to_string(),
                }
            }
        }
        Ok(Err(_)) => {
            // sender 已被取走但没 send（不应发生，oneshot 对端 drop）
            log::warn!("mcp approval: oneshot closed for {}", request_id);
            ElicitOutcome::NotSupported("GUI 审批通道异常关闭".to_string())
        }
        Err(_) => {
            // 超时：清理 pending 表（前端若后到也会因 request_id 不存在报错）
            log::warn!("mcp approval: timeout (60s) for {}", request_id);
            let mut map = ctx.approval_pending.lock().await;
            map.remove(&request_id);
            ElicitOutcome::Timeout
        }
    }
}

/// 尝试经 MCP elicitation 向用户确认。
///
/// v1.5 三级降级：elicitation（客户端原生框）→ GUI 弹窗（同进程）→ fail-secure 拒绝。
async fn try_elicit(
    peer: &Peer<rmcp::RoleServer>,
    ctx: &McpToolContext,
    info: &ElicitationInfo,
) -> ElicitOutcome {
    // 先检查客户端是否声明了 elicitation 能力
    let modes = peer.supported_elicitation_modes();
    if modes.is_empty() {
        // 客户端不支持 elicitation → 走 GUI 弹窗降级（v1.5：替代 v1.4 的直接拒绝）
        return degrade_to_pipe_or_reject(ctx, info).await;
    }

    // 发起 elicitation（message = 三段式确认文本）
    // elicit::<T>() 返回 Result<Option<T>, ElicitationError>：
    //   Ok(Some(form)) = 用户确认并填表单
    //   Ok(None) = 用户没提供内容
    //   Err(UserDeclined) = 用户拒绝
    //   Err(UserCancelled) = 用户取消
    //   Err(CapabilityNotSupported) = 客户端不支持 → 走 GUI 弹窗降级
    match peer.elicit::<ApprovalForm>(info.to_message()).await {
        Ok(Some(form)) => {
            if form.confirmed {
                ElicitOutcome::Accepted {
                    via: ApprovalVia::Elicitation,
                }
            } else {
                ElicitOutcome::Declined {
                    via: ApprovalVia::Elicitation,
                    reason: "用户在确认框中选择了不执行".to_string(),
                }
            }
        }
        Ok(None) => ElicitOutcome::Declined {
            via: ApprovalVia::Elicitation,
            reason: "用户未提供确认".to_string(),
        },
        Err(rmcp::service::ElicitationError::UserDeclined) => {
            // 无法区分「用户真拒绝」和「客户端自动拒绝」（如 Codex 伪支持
            // elicitation：握手时声明能力，运行时自动 Decline 所有请求）。
            // 降级走 GUI 弹窗：GUI 在线则弹窗让用户真确认（Codex 场景），
            // GUI 离线则 fail-secure 拒绝（真拒绝场景，Claude Code 无 GUI 时）。
            // 副作用：Claude Code + GUI 在线时用户拒了会再弹一次 GUI 窗，
            // 但这只是冗余无害（用户可再拒一次）。
            log::info!(
                "elicitation UserDeclined (可能客户端自动拒绝如 Codex), trying GUI fallback"
            );
            degrade_to_pipe_or_reject(ctx, info).await
        }
        Err(rmcp::service::ElicitationError::UserCancelled) => {
            // 同 UserDeclined：客户端可能自动 Cancel（未实现确认 UI），
            // 降级 GUI 弹窗给用户第二次确认机会。
            log::info!(
                "elicitation UserCancelled (可能客户端未实现确认 UI), trying GUI fallback"
            );
            degrade_to_pipe_or_reject(ctx, info).await
        }
        Err(rmcp::service::ElicitationError::CapabilityNotSupported) => {
            // 客户端运行时不支持 → 走 GUI 弹窗降级
            degrade_to_pipe_or_reject(ctx, info).await
        }
        Err(e) => {
            log::warn!("elicitation error: {:?}, trying GUI fallback", e);
            degrade_to_pipe_or_reject(ctx, info).await
        }
    }
}

/// elicitation 表单 schema（客户端据此渲染确认框）。
#[derive(schemars::JsonSchema, serde::Deserialize)]
struct ApprovalForm {
    /// 确认执行此高危操作。
    confirmed: bool,
}
// 标记为 elicitation 安全类型（rmcp 要求）
rmcp::elicit_safe!(ApprovalForm);

/// 辅助：构造 error CallToolResult。
fn error_result(message: &str) -> CallToolResult {
    let mut result = CallToolResult::success(vec![rmcp::model::Content::text(message.to_string())]);
    result.is_error = Some(true);
    result
}

/// MCP server handler。持有工具上下文（资产库/凭据路径）。
#[derive(Clone)]
pub struct MyshellToolMcpServer {
    ctx: Arc<McpToolContext>,
}

impl MyshellToolMcpServer {
    pub fn new(ctx: McpToolContext) -> Self {
        Self {
            ctx: Arc::new(ctx),
        }
    }
}

impl ServerHandler for MyshellToolMcpServer {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_prompts()
            .build();
        InitializeResult::new(capabilities).with_server_info(Implementation::new(
            "myshelltool",
            env!("CARGO_PKG_VERSION"),
        ))
    }

    /// 返回 9 个工具（7 只读 + 2 高危）。
    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        let tools = tools::list_all_tools();
        std::future::ready(Ok(ListToolsResult {
            next_cursor: None,
            tools,
            meta: None,
        }))
    }

    /// 分发工具调用到 tools::call_tool。
    ///
    /// v1.1：高危工具（ssh_exec/sftp_remove）在分发前做审批拦截——
    /// 命中黑名单/未知 → 经 MCP elicitation 在客户端界面内弹确认框（三段式），
    /// 用户 accept 才执行。客户端不支持 elicitation 时降级为 v1.0 的进程内拒绝。
    /// v2：① 审批按当前拦截等级判定（Minimal 仅拦黑名单/超高危，Strict 非白名单
    /// 一律确认，见 config.rs）；② 每次真实触发远程执行的工具调用记执行日志
    /// （execution_log.rs）：决策路径 decision + 执行结果 outcome + 输出摘要。
    /// v2.1：Minimal 档毁灭性命令 HardBlock 直接拒绝（不弹窗不等超时），
    /// 非毁灭黑名单放行记 minimal_allowed。
    fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        let ctx = self.ctx.clone();
        let tool_name = request.name.clone();
        let arguments = request.arguments.clone();
        async move {
            // ── v2 执行日志：记录范围（真实触发远程执行的工具才记）──
            let log_scope = log_scope_for(&tool_name, &arguments);
            let level_snapshot = ctx.config.read().await.level;
            let level_str = level_snapshot.as_str();
            // 决策初值：范围内但无需审批的工具（disk_usage 等只读）→ not_required
            let mut log_decision: &'static str = decision::NOT_REQUIRED;

            // ── v1.1 审批拦截：高危工具先做危险判定（v2 按当前等级）──
            if let Some(approval_needed) = check_approval_needed(&ctx, &tool_name, &arguments).await
            {
                match approval_needed {
                    super::approval::ApprovalDecision::AutoExecute(reason) => {
                        // 放行：白名单命中 → auto_approved；Minimal 档放行 → minimal_allowed
                        // （v2.1 起非毁灭黑名单在 Minimal 下也走 MinimalFallback 放行）
                        log_decision = match reason {
                            super::approval::AutoApproveReason::Whitelist => {
                                decision::AUTO_APPROVED
                            }
                            super::approval::AutoApproveReason::MinimalFallback => {
                                decision::MINIMAL_ALLOWED
                            }
                        };
                    }
                    super::approval::ApprovalDecision::HardBlock { reason } => {
                        // v2.1：毁灭性命令硬拦——不进审批链（不发 elicitation /
                        // 不弹 GUI 窗，避免不支持 elicitation 的 host 下白等 60s），
                        // 两档等级一致直接拒绝，记 hard_blocked 日志后返回。
                        log::warn!(
                            "approval: catastrophic command hard-blocked, refusing without approval chain"
                        );
                        if let Some(scope) = &log_scope {
                            append_execution_log(
                                &ctx,
                                scope,
                                level_str,
                                decision::HARD_BLOCKED,
                                outcome::SKIPPED,
                                "",
                            )
                            .await;
                        }
                        return Ok(error_result(&reason));
                    }
                    super::approval::ApprovalDecision::RequestElicitation(info) => {
                        // v1.5：elicitation 优先，不支持时降级 GUI 弹窗（ctx 透传）
                        match try_elicit(&context.peer, &ctx, &info).await {
                            ElicitOutcome::Accepted { via } => {
                                log::info!("elicitation: user accepted, proceeding");
                                log_decision = via.accepted_decision();
                                // 放行执行
                            }
                            ElicitOutcome::Declined { via, reason } => {
                                log::info!("elicitation: user declined");
                                if let Some(scope) = &log_scope {
                                    append_execution_log(
                                        &ctx,
                                        scope,
                                        level_str,
                                        via.declined_decision(),
                                        outcome::SKIPPED,
                                        "",
                                    )
                                    .await;
                                }
                                return Ok(error_result(&reason));
                            }
                            ElicitOutcome::Timeout => {
                                if let Some(scope) = &log_scope {
                                    append_execution_log(
                                        &ctx,
                                        scope,
                                        level_str,
                                        decision::TIMEOUT,
                                        outcome::SKIPPED,
                                        "",
                                    )
                                    .await;
                                }
                                return Ok(error_result("GUI 审批超时（60s 未响应），已拒绝执行"));
                            }
                            ElicitOutcome::NotSupported(reason) => {
                                // GUI 审批通道不可用 → fail-secure 拒绝（两档等级行为一致）
                                log::warn!("elicitation not supported, degrading to reject: {}", reason);
                                if let Some(scope) = &log_scope {
                                    append_execution_log(
                                        &ctx,
                                        scope,
                                        level_str,
                                        decision::REJECTED,
                                        outcome::SKIPPED,
                                        "",
                                    )
                                    .await;
                                }
                                return Ok(error_result(&reason));
                            }
                        }
                    }
                }
            }

            // ── 正常分发 ──
            let result = match tools::call_tool(tool_name.as_ref(), request, &ctx).await {
                Ok(result) => result,
                Err(e) => {
                    log::warn!("MCP call_tool error: {}", e);
                    error_result(&e)
                }
            };

            // ── v2 执行日志：终态落盘（outcome：通道成功 ok / 失败 error）──
            if let Some(scope) = &log_scope {
                let outcome_str = if result.is_error == Some(true) {
                    outcome::ERROR
                } else {
                    outcome::OK
                };
                let raw_output = extract_result_text(&result);
                // D4 脱敏红线：针对 sftp_read_file 成功读取的输出，绝对不把文件本体写入审计日志，仅记元信息
                let output_text = if scope.tool == "sftp_read_file" && result.is_error != Some(true) {
                    format!("[文件内容已脱敏：读取成功，共 {} 字符]", raw_output.len())
                } else {
                    raw_output
                };
                append_execution_log(&ctx, scope, level_str, log_decision, outcome_str, &output_text)
                    .await;
            }

            Ok(result)
        }
    }

    /// 列出 3 个静态资源（Layer 4）。
    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, McpError>> + Send + '_ {
        let resources = super::resources::list_resources();
        std::future::ready(Ok(ListResourcesResult {
            next_cursor: None,
            resources,
            meta: None,
        }))
    }

    /// 读取资源内容（Layer 4）。
    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, McpError>> + Send + '_ {
        let asset_path = self.ctx.asset_store_path.clone();
        let known_hosts_path = self.ctx.known_hosts_path.clone();
        std::future::ready(match super::resources::read_resource(
            &request,
            &asset_path,
            &known_hosts_path,
        ) {
            Ok(result) => Ok(result),
            Err(e) => {
                log::warn!("MCP read_resource error: {}", e);
                Err(McpError::invalid_request(e, None))
            }
        })
    }

    /// 列出 resource template（Layer 4）。
    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, McpError>> + Send + '_
    {
        let templates = super::resources::list_resource_templates();
        std::future::ready(Ok(ListResourceTemplatesResult {
            next_cursor: None,
            resource_templates: templates,
            meta: None,
        }))
    }

    /// 列出 3 个诊断 prompt（Layer 5）。
    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, McpError>> + Send + '_ {
        let prompts = super::prompts::list_prompts();
        std::future::ready(Ok(ListPromptsResult {
            next_cursor: None,
            prompts,
            meta: None,
        }))
    }

    /// 生成 prompt messages（Layer 5）。
    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<GetPromptResult, McpError>> + Send + '_ {
        std::future::ready(match super::prompts::get_prompt(&request) {
            Ok(result) => Ok(result),
            Err(e) => {
                log::warn!("MCP get_prompt error: {}", e);
                Err(McpError::invalid_request(e, None))
            }
        })
    }
}
