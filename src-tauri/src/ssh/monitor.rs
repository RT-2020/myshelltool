//! 资源监控的 exec 采样桥（会话循环 → core::proc_parse 快照组装）。
//! 从 ssh.rs 按域拆出（architecture-log Target 1），零逻辑变更。

use super::*;

/// 资源监控不可恢复错误事件（v2.5，事件名 `resource-monitor-error`）。
///
/// 触发场景：远端无 /proc（非 Linux 主机）导致快照主体不可信。emit 后该
/// session 的轮询即被停止（前端同时会收到 resource-monitor-stopped）。
/// 字段 camelCase 序列化（sessionId / reason），与 resource-monitor-snapshot
/// 的 ResourceSnapshot 命名约定一致。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceMonitorErrorEvent {
    session_id: String,
    reason: String,
}

/// 单次 monitor exec 等待远端输出的总时限。
///
/// 依据（`resource_monitor.rs::resource_monitor_start`）：轮询间隔默认 2000ms
/// （`interval_ms == 0` 时的缺省值），正常采样（`cat /proc/*` + `df`）在毫秒级完成。
/// 由于会话级在途守卫会让挂住期间的 tick 直接跳过（不排队），超时值不需要小于轮询
/// 间隔；15s 是给「一次采样」的宽松安全上限 —— 正常情况远达不到，异常时保证该
/// 任务一定终止，不会永久占用 channel 与远端 session 配额。
const MONITOR_EXEC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// RAII 守卫：drop 时清除会话的「monitor exec 在途」标记，保证超时/错误/正常
/// 结束任何返回路径都释放，不依赖在每个 return 前手动 reset。
pub struct MonitorExecInFlightGuard(pub Arc<AtomicBool>);

impl Drop for MonitorExecInFlightGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Run a one-shot exec command on a fresh channel and parse the /proc/* output
/// into a ResourceSnapshot. Used by the resource_monitor module's polling loop
/// (via SshCommand::MonitorExec). Does NOT touch the interactive PTY channel.
///
/// Flow:
/// 1. Open a new session channel
/// 2. exec(command) — the command is `crate::resource_monitor::MONITOR_SAMPLE_COMMAND`
///    (cat /proc/stat; cat /proc/meminfo; cat /proc/net/dev; cat /proc/diskstats;
///    env LC_ALL=C df -P -k; cat /proc/net/route)
/// 3. Read all stdout into a String
/// 4. Use resource_monitor::build_snapshot to parse + build a ResourceSnapshot
///    (this requires prev_cpu delta — we read the previous snapshot from the
///    monitor handle to compute the CPU delta)
/// 5. Emit "resource-monitor-snapshot" event with the parsed ResourceSnapshot
/// 6. Record the snapshot + prev_cpu back into the monitor handle for next tick
///
/// v2.5 失败语义：build_snapshot 返回 Err（无 /proc，非 Linux）→ 停止该
/// session 的轮询 + emit `resource-monitor-error`，绝不发全 0 假快照；
/// 次要段失败时 snapshot 自带 `degraded` 字段，照常 emit（前端展示降级）。
pub async fn handle_monitor_exec(
    session_id: String,
    command: String,
    ssh_handle: &Arc<client::Handle<SshClient>>,
    app: &AppHandle,
) {
    // Open a fresh exec channel (separate from the interactive PTY).
    let mut exec_channel = match ssh_handle.channel_open_session().await {
        Ok(ch) => ch,
        Err(e) => {
            warn!(
                "resource_monitor: session {session_id} exec channel open failed: {e}"
            );
            return;
        }
    };

    if let Err(e) = exec_channel.exec(true, command).await {
        warn!(
            "resource_monitor: session {session_id} exec failed: {e}"
        );
        return;
    }

    // Collect stdout + stderr, 整体包一层超时：远端命令挂住时 exec channel 永不
    // EOF，没有上限这个任务就永久挂在这里（超时后直接结束本次采样，不发任何快照
    // —— 遵守 v2.5 失败语义：失败绝不发全 0 假快照）。
    let collected = tokio::time::timeout(MONITOR_EXEC_TIMEOUT, async {
        let mut stdout = Vec::new();
        while let Some(msg) = exec_channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => stdout.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, ext: _ } => {
                    // stderr — log but don't fail the snapshot
                    let msg = String::from_utf8_lossy(&data);
                    warn!(
                        "resource_monitor: session {session_id} stderr: {}",
                        msg.trim()
                    );
                }
                ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        stdout
    })
    .await;

    let stdout = match collected {
        Ok(stdout) => stdout,
        Err(_) => {
            warn!(
                "resource_monitor: session {session_id} monitor exec timed out after {}s, sample aborted",
                MONITOR_EXEC_TIMEOUT.as_secs()
            );
            return;
        }
    };

    let combined = String::from_utf8_lossy(&stdout);

    // Resolve AppState via the AppHandle so we can read/write resource_monitors.
    let resource_state = app.state::<crate::AppState>();

    // Fetch prev_cpu so build_snapshot can compute a CPU delta.
    let prev_cpu = match resource_state.resource_monitors.lock() {
        Ok(m) => m.handles.get(&session_id).and_then(|h| h.prev_cpu.clone()),
        Err(_) => None,
    };

    // Build snapshot (parses /proc/stat, /proc/meminfo, /proc/net/dev, /proc/diskstats).
    // 主要段（stat/meminfo）失败 = 无 /proc（非 Linux 远端）→ 停轮询 + 错误事件，
    // 不再发全 0 假快照（恒 0% 的图表伪装成「负载健康」）。
    let snapshot = match crate::resource_monitor::build_snapshot(&session_id, &combined, prev_cpu)
    {
        Ok(s) => s,
        Err(reason) => {
            warn!(
                "resource_monitor: session {session_id} snapshot aborted: {reason}, stopping monitor"
            );
            if let Ok(mut mgr) = resource_state.resource_monitors.lock() {
                if let Some(handle) = mgr.handles.remove(&session_id) {
                    // 轮询任务收到 cancel 后会自行 emit resource-monitor-stopped。
                    let _ = handle.cancel.send(());
                }
            }
            let _ = app.emit(
                "resource-monitor-error",
                ResourceMonitorErrorEvent {
                    session_id: session_id.clone(),
                    reason,
                },
            );
            return;
        }
    };

    // Compute prev_cpu for the NEXT tick = (idle, total) from this sample.
    // We re-parse /proc/stat to get the current jiffies.
    let (idle_now, total_now, _) =
        match crate::resource_monitor::parse_proc_stat(
            &crate::resource_monitor::extract_stat_section(&combined),
        ) {
            Ok(v) => v,
            Err(_) => (0, 0, 0),
        };

    // Emit the structured snapshot to the frontend.
    if let Err(e) = app.emit("resource-monitor-snapshot", snapshot.clone()) {
        warn!(
            "resource_monitor: session {session_id} emit snapshot failed: {e}"
        );
    }

    // Record the snapshot + prev_cpu back into the monitor handle.
    crate::resource_monitor::record_snapshot(&resource_state, snapshot, (idle_now, total_now));
}
