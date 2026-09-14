// Resource monitor — periodic remote /proc/* sampling over an existing SSH session.
//
// Design (see .omc/plans/ui-full-refactor-consensus.md Step 4.1 + 4.2):
// - Each active monitor is tied to a session_id and runs in its own tokio task.
// - The task uses the session's SshCommand channel (cmd_tx) to send MonitorExec
//   (myshelltool_core::proc_parse::MONITOR_SAMPLE_COMMAND) into the SSH session
//   loop. The session loop opens a fresh exec channel, runs the combined command,
//   parses the stdout via the /proc/* parsers, builds a ResourceSnapshot, and
//   emits "resource-monitor-snapshot" to the frontend.
// - ResourceMonitorState lives behind std::sync::Mutex and stores cancel handles +
//   the last successful snapshot per session.
//
// v2.8：全部 /proc 解析纯函数（含 ResourceSnapshot/DiskMountInfo 类型、
// build_snapshot、MONITOR_SAMPLE_COMMAND 与测试）已迁入
// `crates/myshelltool-core/src/proc_parse/`——src-tauri 测试二进制因 Tauri
// runtime DLL 缺失跑不起来，留在本文件的测试从未真正执行。此处 re-export
// 保持 ssh.rs / lib.rs 调用点不变。本文件只保留 Tauri 命令层与轮询任务。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, State};

use crate::AppState;

// 解析层符号再导出（调用点：ssh.rs 的 handle_monitor_exec 用 build_snapshot /
// parse_proc_stat / extract_stat_section，lib.rs / 前端事件用 ResourceSnapshot）
pub use myshelltool_core::proc_parse::{
    build_snapshot, extract_stat_section, parse_proc_stat, ResourceSnapshot,
    MONITOR_SAMPLE_COMMAND,
};

/// Per-session monitor handle. `cancel` is signaled on stop / drop.
pub struct ResourceMonitorHandle {
    pub cancel: tokio::sync::oneshot::Sender<()>,
    pub last_snapshot: Option<ResourceSnapshot>,
    /// Previous CPU jiffies (idle, total) used for delta computation.
    pub prev_cpu: Option<(u64, u64)>,
}

#[derive(Default)]
pub struct ResourceMonitorState {
    pub handles: HashMap<String, ResourceMonitorHandle>,
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Start periodic monitoring for `session_id`. Interval defaults to 2000ms.
/// Returns Err if the session is already being monitored (caller should call
/// `resource_monitor_stop` first).
#[tauri::command]
pub async fn resource_monitor_start(
    session_id: String,
    interval_ms: u64,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let interval = if interval_ms == 0 { 2000 } else { interval_ms };

    // Create the cancel channel up-front so its receiver can move into the spawned task.
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

    // Atomically check + insert the session entry. If a handle exists, error.
    {
        let mut mgr = state.resource_monitors.lock().map_err(|e| e.to_string())?;
        if mgr.handles.contains_key(&session_id) {
            return Err(format!(
                "resource_monitor_start: session {session_id} already monitored"
            ));
        }
        mgr.handles.insert(
            session_id.clone(),
            ResourceMonitorHandle {
                cancel: cancel_tx,
                last_snapshot: None,
                prev_cpu: None,
            },
        );
    }

    // Clone Arcs out of State so the spawned task is 'static + Send. State itself
    // borrows from Tauri's request lifetime and cannot cross tokio::spawn.
    let ssh_sessions = state.ssh_sessions.clone();
    let resource_monitors = state.resource_monitors.clone();

    spawn_monitor_task(
        session_id.clone(),
        interval,
        ssh_sessions,
        resource_monitors,
        app,
        cancel_rx,
    );

    Ok(())
}

/// Spawn the periodic sampling loop. The task:
/// 1. Builds the combined /proc command string (MONITOR_SAMPLE_COMMAND, 定义在
///    core::proc_parse 并经本文件 re-export——字符串与解析层的锚点契约必须同处一地)
/// 2. Sends MonitorExec via the session's SshCommand channel
/// 3. The SSH session loop runs the command, parses output, builds snapshot,
///    and emits "resource-monitor-snapshot". This file's parsers are imported
///    by ssh.rs to do the parse.
/// 4. Stops when cancel_rx fires or the session is dropped.
fn spawn_monitor_task(
    session_id: String,
    interval_ms: u64,
    ssh_sessions: Arc<tokio::sync::Mutex<crate::ssh::SshSessionManager>>,
    resource_monitors: Arc<Mutex<ResourceMonitorState>>,
    app: AppHandle,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
) {
    // LC_ALL=C：强制英文表头，"Filesystem" 锚点在任何 locale 下都能命中；
    // -P -k：POSIX 格式 + 1K 块（-B1 是 GNU 扩展，BusyBox 不支持）。
    // `env LC_ALL=C`（而非 `LC_ALL=C` 前缀）：远端登录 shell 可能是 csh/tcsh
    // （FreeBSD root 默认），那里 `VAR=值 命令` 不是赋值语法，df 段整条不执行。
    // 各段以 `;` 分隔：单段失败（如受限容器里 /proc/net/route 不可读）只损失
    // 该段，其余照常。route 段放在最后是 split_proc_output 的切分前提
    // （route 段 = Iface 表头锚点到 EOF）。
    let cmd = MONITOR_SAMPLE_COMMAND;

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(interval_ms));
        // First tick is immediate — we want that, the first sample seeds prev_cpu.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // Resolve cmd_tx each iteration — the session may disconnect
                    // and be removed mid-run; we want to fail gracefully.
                    let tx = {
                        let mgr = ssh_sessions.lock().await;
                        mgr.get_cmd_tx(&session_id)
                    };
                    let Some(tx) = tx else {
                        log::info!(
                            "resource_monitor: session {session_id} disappeared, stopping monitor"
                        );
                        break;
                    };
                    if tx.send(crate::ssh::SshCommand::MonitorExec(cmd.to_string())).is_err() {
                        log::info!(
                            "resource_monitor: session {session_id} cmd channel closed, stopping"
                        );
                        break;
                    }
                }
                _ = &mut cancel_rx => {
                    log::info!("resource_monitor: session {session_id} cancelled");
                    break;
                }
            }
        }

        // Cleanup: drop our entry from the map.
        if let Ok(mut mgr) = resource_monitors.lock() {
            mgr.handles.remove(&session_id);
        }
        // Inform frontend the monitor stopped (lets UI reset state).
        let _ = app.emit("resource-monitor-stopped", session_id.clone());
    });
}

/// Stop monitoring for `session_id`. Safe to call when not monitoring.
#[tauri::command]
pub async fn resource_monitor_stop(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut mgr = state.resource_monitors.lock().map_err(|e| e.to_string())?;
    if let Some(handle) = mgr.handles.remove(&session_id) {
        // Signal the task to stop. Ignore error — task already finished.
        let _ = handle.cancel.send(());
    }
    Ok(())
}

/// Read the last snapshot (if any) for a session. Synchronous one-shot read.
#[tauri::command]
pub async fn resource_monitor_snapshot(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<Option<ResourceSnapshot>, String> {
    let mgr = state.resource_monitors.lock().map_err(|e| e.to_string())?;
    Ok(mgr
        .handles
        .get(&session_id)
        .and_then(|h| h.last_snapshot.clone()))
}

/// List all session_ids currently being monitored.
#[tauri::command]
pub async fn resource_monitor_list_active(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let mgr = state.resource_monitors.lock().map_err(|e| e.to_string())?;
    Ok(mgr.handles.keys().cloned().collect())
}

// ---------------------------------------------------------------------------
// Internal hook used by ssh.rs: the session task has built a fresh snapshot
// from a MonitorExec response. Store it so the synchronous `resource_monitor_snapshot`
// command can return it on demand.
// ---------------------------------------------------------------------------

/// Called by the SSH session task after it parses a MonitorExec response.
/// Updates last_snapshot + prev_cpu. No-op if no monitor is registered for
/// this session (e.g., it was stopped concurrently).
pub fn record_snapshot(
    state: &State<'_, AppState>,
    snapshot: ResourceSnapshot,
    idle_total: (u64, u64),
) {
    if let Ok(mut mgr) = state.resource_monitors.lock() {
        if let Some(handle) = mgr.handles.get_mut(&snapshot.session_id) {
            handle.prev_cpu = Some(idle_total);
            handle.last_snapshot = Some(snapshot);
        }
    }
}
