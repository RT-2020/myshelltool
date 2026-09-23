//! MCP 长任务工具 handlers（v0.20，3-6 月段 C1）：
//! ssh_exec_async（后台执行返回 job_id）/ job_status / job_output / job_cancel。
//! 状态机与存储在 job_store.rs（内存态、TTL 30 分钟、不落盘）。

use rmcp::model::{CallToolResult, Content};
use serde_json::{Map, Value};

use super::registry::ToolFuture;
use super::tools::McpToolContext;
use crate::ssh::HeadlessConnectParams;

fn text_result(text: String) -> CallToolResult {
    CallToolResult::success(vec![Content::text(text)])
}

fn error_result(message: &str) -> CallToolResult {
    let mut result = CallToolResult::success(vec![Content::text(message.to_string())]);
    result.is_error = Some(true);
    result
}

/// ssh_exec_async：后台执行（立即返回 job_id）。
/// 执行体：headless 建连（B3 池复用）→ exec_command_once → 终态写回 job store。
/// 取消 = disconnect 连接（远端命令收 SIGHUP）→ 执行侧收敛为 Cancelled。
pub(crate) fn h_ssh_exec_async<'a>(ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(async move {
        let asset_id = args
            .get("asset_id")
            .and_then(|v| v.as_str())
            .ok_or("缺少 asset_id 参数")?
            .to_string();
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or("缺少 command 参数")?
            .to_string();
        let intent = args.get("intent").and_then(|v| v.as_str()).unwrap_or("");
        log::info!(
            "ssh_exec_async: asset={} intent={:?} command={:?}",
            asset_id,
            intent,
            myshelltool_core::redact_command(&command)
        );

        // 解析资产 + scope（长任务与同步执行同一授权面）
        let store = myshelltool_core::load_connection_asset_store(&ctx.asset_store_path)
            .map_err(|e| format!("加载资产库失败: {e}"))?;
        let asset = store
            .assets
            .iter()
            .find(|a| a.id == asset_id)
            .ok_or_else(|| format!("资产 {asset_id} 不存在"))?
            .clone();
        let scope_snapshot = ctx.config.read().await.scope.clone();
        let verdict = myshelltool_core::mcp_scope::evaluate(&scope_snapshot, &asset);
        if !verdict.is_allowed() {
            return Err(myshelltool_core::mcp_scope::denied_message(verdict));
        }

        // 先登记 job（running，AI 立即拿到可轮询的句柄），连接句柄建连后注入
        let (job_id, _done_rx) = super::job_store::create(&asset_id, None).await;

        // 后台执行（不占工具调用超时；连接不回池——长任务独占直到终态）
        let ctx_owned = ctx.clone();
        let job_id_for_task = job_id.clone();
        let spawned = tokio::spawn(async move {
            run_job(&ctx_owned, &asset, &asset_id, &job_id_for_task, &command).await;
        });
        // spawn 立即失败（极端）：直接落 Failed 终态
        if spawned.is_finished() {
            if let Err(e) = spawned.await {
                super::job_store::settle(&job_id, super::job_store::JobStatus::Failed, None, Some(format!("任务启动失败: {e}"))).await;
            }
        }
        Ok(text_result(format!(
            "job_id={job_id}\nstatus=running\n命令已在后台执行；用 job_status 轮询状态、job_output 分页读输出、job_cancel 取消（TTL 30 分钟）"
        )))
    })
}

/// 后台执行体（含建连 + 执行 + 终态收敛）。
async fn run_job(ctx: &McpToolContext, asset: &myshelltool_core::ConnectionAsset, asset_id: &str, job_id: &str, command: &str) {
    let params = HeadlessConnectParams {
        host: asset.host.clone(),
        port: asset.port,
        username: asset.username.clone(),
        password: String::new(),
        credential_id: asset.credential_id.clone(),
        auth_method: Some(format!("{:?}", asset.auth_method)),
        private_key_path: asset.private_key_path.clone(),
        private_key_credential_id: asset.private_key_credential_id.clone(),
        passphrase: None,
        passphrase_credential_id: asset.passphrase_credential_id.clone(),
        secret_store_dir: ctx.secret_store_dir.clone(),
        known_hosts_path: ctx.known_hosts_path.clone(),
        jump_host: crate::ssh::jump_host_of(asset).map(str::to_string),
        connect_timeout_secs: asset.connect_timeout_secs,
        keepalive_interval_secs: asset.keepalive_interval_secs,
        asset_store_path: Some(ctx.asset_store_path.clone()),
    };

    let connect_result = crate::ssh::connect_headless(&params).await;
    let handle = match connect_result {
        Ok(h) => std::sync::Arc::new(h),
        Err(e) => {
            super::job_store::settle(job_id, super::job_store::JobStatus::Failed, None, Some(format!("连接失败: {e}"))).await;
            return;
        }
    };

    // 连接就绪后注入句柄（cancel 的断连通道）
    super::job_store::set_handle(job_id, handle.clone()).await;
    log::info!("ssh_exec_async job {job_id} started on {asset_id}");

    let output = crate::ssh::exec_command_once(&handle, command).await;
    match output {
        Ok(out) => {
            // v0.20（S9 真机验收修正）：断连取消时 exec 的 channel wait 收到 Close
            // 自然结束循环，返回 **Ok**（exit_code=None）而非 Err——Err 分支的取消
            // 检测覆盖不到。Ok 分支同样要查 cancel_requested：置位则收敛 Cancelled
            // （exit_code unknown），否则才记 Done。
            let mut body = out.stdout;
            if !out.stderr.is_empty() {
                body.push_str("\n--- stderr ---\n");
                body.push_str(&out.stderr);
            }
            super::job_store::append_output(&job_id, body.as_bytes()).await;
            let cancelled = super::job_store::snapshot(&job_id)
                .await
                .map(|s| s.cancel_requested)
                .unwrap_or(false);
            if cancelled {
                super::job_store::settle(&job_id, super::job_store::JobStatus::Cancelled, None, None).await;
            } else {
                super::job_store::settle(&job_id, super::job_store::JobStatus::Done, out.exit_code, None).await;
            }
        }
        Err(e) => {
            // 区分取消与失败：cancel_requested 已置 → 用户取消
            let cancelled = super::job_store::snapshot(&job_id)
                .await
                .map(|s| s.cancel_requested)
                .unwrap_or(false);
            if cancelled {
                super::job_store::settle(&job_id, super::job_store::JobStatus::Cancelled, None, None).await;
            } else {
                super::job_store::settle(&job_id, super::job_store::JobStatus::Failed, None, Some(e)).await;
            }
        }
    }
}

pub(crate) fn h_job_status<'a>(_ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(async move {
        let job_id = args.get("job_id").and_then(|v| v.as_str()).ok_or("缺少 job_id 参数")?;
        let Some(s) = super::job_store::snapshot(job_id).await else {
            return Ok(error_result(&format!("job {job_id} 不存在或已过期（TTL 30 分钟）")));
        };
        let exit_line = s.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "unknown".to_string());
        let finished_line = s.finished_ms.map(|f| f.to_string()).unwrap_or_else(|| "-".to_string());
        let error_line = s.error.clone().unwrap_or_else(|| "-".to_string());
        Ok(text_result(format!(
            "job_id={job_id}\nasset_id={}\nstatus={}\nexit_code={exit_line}\nerror={error_line}\noutput_bytes={}\ndropped_head_bytes={}\ncreated_ms={}\nfinished_ms={finished_line}\ncancel_requested={}",
            s.asset_id, s.status.as_str(), s.output_bytes, s.dropped_head_bytes, s.created_ms, s.cancel_requested
        )))
    })
}

pub(crate) fn h_job_output<'a>(_ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(async move {
        let job_id = args.get("job_id").and_then(|v| v.as_str()).ok_or("缺少 job_id 参数")?;
        let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(64 * 1024).min(1024 * 1024) as usize;
        let Some((chunk, total, eof, dropped)) = super::job_store::read_output(job_id, offset, limit).await else {
            return Ok(error_result(&format!("job {job_id} 不存在或已过期（TTL 30 分钟）")));
        };
        let head_note = if dropped > 0 {
            format!("\n[注：该输出开头 {dropped} 字节因超单 job 上限（8 MiB）未保留]")
        } else {
            String::new()
        };
        let status = if eof { "已到末尾（EOF）" } else { "未完" };
        Ok(text_result(format!(
            "[job_output：总 {total} 字节，本段 offset={offset} 起 {len} 字节，{status}]{head_note}\n{chunk}",
            len = chunk.len()
        )))
    })
}

pub(crate) fn h_job_cancel<'a>(_ctx: &'a McpToolContext, args: &'a Map<String, Value>) -> ToolFuture<'a> {
    Box::pin(async move {
        let job_id = args.get("job_id").and_then(|v| v.as_str()).ok_or("缺少 job_id 参数")?;
        if super::job_store::request_cancel(job_id).await {
            Ok(text_result(format!("已请求取消 job {job_id}（断开连接，远端命令将收 SIGHUP）；状态由执行侧收敛为 cancelled，用 job_status 确认")))
        } else {
            Ok(error_result(&format!("job {job_id} 不存在、已终态或已在取消中")))
        }
    })
}
