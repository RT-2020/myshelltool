//! MCP 批量编排层（v0.20，3-6 月段第一件；MCP服务设计-v3 §3.3/C2 + G2 决策）。
//!
//! ## 定位
//!
//! `exec_many` 工具的执行内核——多主机 fan-out（「替换 FinalShell」的多机运维
//! 一等公民）。**与 GUI 批量面板共用同一编排层**（G3 决策：Rust 侧先落地，
//! exec_many 第一个消费者，GUI 后消费——同一份实现不写两遍）。
//!
//! ## 并发纪律（G2）
//!
//! fan-out 并发上限 = 会话池容量（8）：超出**排队**而不是突破池上限——一次
//! 20 台的批量会在池门口互相挤死，也会瞬间打满服务端 MaxStartups。用
//! tokio::sync::Semaphore 实现（permit 信号量）。
//!
//! ## 失败语义
//!
//! - 逐目标 scope 判定（范围外 → skipped，记 reason——server.rs 预判只拦单
//!   asset_id 参数，数组目标由本层逐个判）；
//! - 单目标失败**不影响其余**（failed 收集，含 asset 名与错误）；
//! - 每目标输出上限 16 KiB（头 8K + 尾 8K，超限标 truncated——汇总表不爆上下文）；
//! - 审批在 server.rs 按 command 文本判定（ShellExec 策略，与 ssh_exec 同源），
//!   一次审批放行整批——命令文本相同，逐目标重复弹窗是同一决策的复读。

use std::sync::Arc;

use serde_json::Value;

use super::tools::McpToolContext;

/// fan-out 并发上限（= mcp::session_pool 池容量；改一处须同步另一处）。
pub const FANOUT_CONCURRENCY: usize = 8;

/// 单目标输出上限（字符，与 ssh_exec 的 MAX_RETURN_CHARS 同口径）。
const PER_TARGET_LIMIT: usize = 16000;
const PER_TARGET_HEAD: usize = 8000;
const PER_TARGET_TAIL: usize = 8000;

/// 单目标结果（序列化进 exec_many 返回体）。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FanoutTarget {
    pub asset_id: String,
    pub asset_name: String,
    /// allowed / denied_* / ok / error
    pub status: String,
    pub exit_code: Option<u32>,
    pub output: String,
    pub truncated: bool,
    pub error: Option<String>,
    /// gui / pool / new（B3 三层复用的实际来源）
    pub session_source: Option<String>,
}

/// 批量执行结果汇总。
pub struct FanoutOutcome {
    pub targets: Vec<FanoutTarget>,
}

/// 组装 exec_many 的结构化返回体（JSON 文本，逐目标行 + 汇总头）。
pub async fn exec_many_on_assets(
    ctx: &McpToolContext,
    asset_ids: &[String],
    command: &str,
) -> Result<FanoutOutcome, String> {
    if asset_ids.is_empty() {
        return Err("asset_ids 不能为空".to_string());
    }
    if asset_ids.len() > 64 {
        return Err(format!(
            "单次批量上限 64 台（收到 {}）——更大批量请分批执行，防 MaxStartups 与输出爆上下文",
            asset_ids.len()
        ));
    }

    // 解析资产（一次性读库，逐目标取元数据；不存在的目标按 error 记入结果）
    let store = myshelltool_core::load_connection_asset_store(&ctx.asset_store_path)
        .map_err(|e| format!("加载资产库失败: {e}"))?;

    let semaphore = Arc::new(tokio::sync::Semaphore::new(FANOUT_CONCURRENCY));
    let mut handles = Vec::with_capacity(asset_ids.len());

    for asset_id in asset_ids {
        let asset = store.assets.iter().find(|a| a.id == *asset_id).cloned();
        let permit_sem = semaphore.clone();
        // ctx 是 Arc 集合（Clone 廉价）——clone 进 'static 任务
        let ctx_owned = ctx.clone();
        let command = command.to_string();
        handles.push(tokio::spawn(async move {
            let _permit = permit_sem.acquire().await.expect("semaphore 未关闭");
            run_one_target(&ctx_owned, asset, &command).await
        }));
    }

    let mut targets = Vec::with_capacity(handles.len());
    for h in handles {
        match h.await {
            Ok(t) => targets.push(t),
            Err(e) => targets.push(FanoutTarget {
                asset_id: String::new(),
                asset_name: String::new(),
                status: "error".to_string(),
                exit_code: None,
                output: String::new(),
                truncated: false,
                error: Some(format!("任务异常: {e}")),
                session_source: None,
            }),
        }
    }
    Ok(FanoutOutcome { targets })
}

/// 单目标执行（scope 判定 → 三层复用 exec → 输出限幅）。
async fn run_one_target(
    ctx: &McpToolContext,
    asset: Option<myshelltool_core::ConnectionAsset>,
    command: &str,
) -> FanoutTarget {
    let Some(asset) = asset else {
        return FanoutTarget {
            asset_id: String::new(),
            asset_name: String::new(),
            status: "error".to_string(),
            exit_code: None,
            output: String::new(),
            truncated: false,
            error: Some("资产不存在".to_string()),
            session_source: None,
        };
    };
    let asset_id = asset.id.clone();
    let asset_name = asset.name.clone();

    // 逐目标 scope 判定（server.rs 预判只拦单 asset_id 参数；数组目标在此拦）
    let scope_snapshot = ctx.config.read().await.scope.clone();
    let verdict = myshelltool_core::mcp_scope::evaluate(&scope_snapshot, &asset);
    if !verdict.is_allowed() {
        return FanoutTarget {
            asset_id,
            asset_name,
            status: verdict.as_str().to_string(),
            exit_code: None,
            output: String::new(),
            truncated: false,
            error: None,
            session_source: None,
        };
    }

    // 三层复用执行内核（与 ssh_exec 同路径：GUI 会话 → 池 → 新建）
    match super::tools::exec_with_reuse(ctx, &asset, &asset_id, command).await {
        Ok((output, source)) => {
            let mut body = if output.stdout.is_empty() {
                String::new()
            } else {
                output.stdout
            };
            if !output.stderr.is_empty() {
                body.push_str("\n--- stderr ---\n");
                body.push_str(&output.stderr);
            }
            let truncated = body.chars().count() > PER_TARGET_LIMIT;
            if truncated {
                body = super::execution_log::truncate_middle(&body, PER_TARGET_HEAD, PER_TARGET_TAIL);
            }
            FanoutTarget {
                asset_id,
                asset_name,
                status: "ok".to_string(),
                exit_code: output.exit_code,
                output: body,
                truncated,
                error: None,
                session_source: Some(source.to_string()),
            }
        }
        Err(e) => FanoutTarget {
            asset_id,
            asset_name,
            status: "error".to_string(),
            exit_code: None,
            output: String::new(),
            truncated: false,
            error: Some(e),
            session_source: None,
        },
    }
}

/// 汇总为 exec_many 的返回文本（JSON pretty：AI 可直接读结构化结果）。
pub fn summarize(outcome: &FanoutOutcome) -> Value {
    let ok = outcome.targets.iter().filter(|t| t.status == "ok").count();
    let denied = outcome
        .targets
        .iter()
        .filter(|t| t.status.starts_with("denied") || t.status == "deny_all")
        .count();
    let errors = outcome.targets.iter().filter(|t| t.status == "error").count();
    let truncated = outcome.targets.iter().filter(|t| t.truncated).count();
    serde_json::json!({
        "summary": {
            "total": outcome.targets.len(),
            "ok": ok,
            "denied": denied,
            "errors": errors,
            "truncated": truncated,
        },
        "targets": outcome.targets,
        "note": "逐目标输出超 16KiB 已截断（truncated=true）；被拒目标换用 list_assets 确认可访问范围",
    })
}
