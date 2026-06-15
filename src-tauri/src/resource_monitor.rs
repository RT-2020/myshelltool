// Resource monitor — periodic remote /proc/* sampling over an existing SSH session.
//
// Design (see .omc/plans/ui-full-refactor-consensus.md Step 4.1 + 4.2):
// - Each active monitor is tied to a session_id and runs in its own tokio task.
// - The task uses the session's SshCommand channel (cmd_tx) to send MonitorExec("cat ...")
//   into the SSH session loop. The session loop opens a fresh exec channel, runs the
//   combined command, parses the stdout via the 4 /proc/* parsers below, builds a
//   ResourceSnapshot, and emits "resource-monitor-snapshot" to the frontend.
// - ResourceMonitorState lives behind std::sync::Mutex and stores cancel handles +
//   the last successful snapshot per session.
// - ssh_disconnect emits "resource-monitor-cleanup" so the monitor module can stop
//   any active monitor for the disconnecting session (no circular dep between modules).
//
// Parser robustness:
// - All parsers are tolerant: missing fields, malformed lines, or unknown format →
//   that field returns 0 (or a sensible default) and the rest still parse.
// - We never depend on external binaries (top/free/vmstat/iostat). Only /proc/*.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::AppState;

// ---------------------------------------------------------------------------
// Snapshot + state types
// ---------------------------------------------------------------------------

/// One sampling of remote resource usage. All byte fields are cumulative
/// (since boot) unless otherwise noted; CPU is instantaneous percent.
#[derive(Debug, Clone, Serialize)]
pub struct ResourceSnapshot {
    pub session_id: String,
    pub cpu_usage: f32,
    pub cpu_cores: u32,
    pub mem_total: u64,
    pub mem_used: u64,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub disk_total: u64,
    pub disk_used: u64,
    pub timestamp: u64,
}

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
// /proc/* parsers
// ---------------------------------------------------------------------------

/// Parse the first line (`cpu  ...`) of /proc/stat.
/// Returns `(idle_jiffies, total_jiffies, num_logical_cores)`.
///
/// Format: `cpu  user nice system idle iowait irq softirq steal guest guest_nice`
/// idle = idle + iowait; total = sum of all columns (guest / guest_nice are
/// already accounted for in user/nice, but Linux always reports them as 0-or-
/// duplicate, so summing is safe).
/// Core count = number of `cpuN` lines that follow the aggregate `cpu` line.
pub fn parse_proc_stat(content: &str) -> Result<(u64, u64, u32), String> {
    let mut idle: u64 = 0;
    let mut total: u64 = 0;
    let mut cores: u32 = 0;
    let mut saw_aggregate = false;

    for line in content.lines() {
        let line = line.trim();
        // Aggregate line: `cpu  1234 56 ...`
        if !saw_aggregate {
            if line.starts_with("cpu ") || line.starts_with("cpu\t") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                // parts[0] == "cpu"; parts[1..] are jiffy counts
                let nums: Vec<u64> = parts[1..]
                    .iter()
                    .filter_map(|s| s.parse::<u64>().ok())
                    .collect();
                if nums.is_empty() {
                    return Err("parse_proc_stat: cpu aggregate line has no numeric fields".into());
                }
                total = nums.iter().sum();
                // idle = column index 3 (idle) + 4 (iowait) if present
                idle = nums.get(3).copied().unwrap_or(0);
                if let Some(iowait) = nums.get(4) {
                    idle += iowait;
                }
                saw_aggregate = true;
                continue;
            }
            // Skip non-cpu lines (intr, softirq, ctxt, ...) above the aggregate
            continue;
        }

        // After aggregate: per-core lines `cpu0 ...`, `cpu1 ...`, ...
        if line.starts_with("cpu") {
            let after = &line[3..];
            // after should be digits then whitespace
            let mut chars = after.chars();
            if let Some(c) = chars.next() {
                if c.is_ascii_digit() {
                    cores += 1;
                }
            }
        }
        // Other lines (intr, softirq, ctxt, btime, processes, ...) — ignore
    }

    if !saw_aggregate {
        return Err("parse_proc_stat: no `cpu` aggregate line found".into());
    }

    Ok((idle, total, cores))
}

/// Parse /proc/meminfo for MemTotal + MemAvailable.
/// Returns `(total_bytes, used_bytes)`. `used = MemTotal - MemAvailable`.
/// Values in meminfo are in kB; we multiply by 1024 to get bytes.
pub fn parse_proc_meminfo(content: &str) -> Result<(u64, u64), String> {
    let mut mem_total_kb: Option<u64> = None;
    let mut mem_avail_kb: Option<u64> = None;

    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            mem_total_kb = parse_kb_value(rest);
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            mem_avail_kb = parse_kb_value(rest);
        }
        if mem_total_kb.is_some() && mem_avail_kb.is_some() {
            break;
        }
    }

    let total = mem_total_kb.unwrap_or(0).saturating_mul(1024);
    let avail = mem_avail_kb.unwrap_or(0).saturating_mul(1024);
    // Fallback to MemFree if MemAvailable missing (older kernels)
    let used = total.saturating_sub(avail);
    Ok((total, used))
}

fn parse_kb_value(rest: &str) -> Option<u64> {
    let trimmed = rest.trim().trim_end_matches("kB").trim();
    trimmed.parse::<u64>().ok()
}

/// Parse /proc/net/dev: sum rx_bytes / tx_bytes across all physical interfaces
/// (skip loopback `lo`). Returns `(rx_bytes, tx_bytes)` cumulative since boot.
///
/// Format header:
/// ```text
/// Inter-|   Receive                                                |  Transmit
///  face |bytes packets errs drop fifo frame compressed multicast|bytes packets ...
/// ```
/// Each data line: `  eth0: 12345 67 ... | 89012 ...`
pub fn parse_proc_net_dev(content: &str) -> Result<(u64, u64), String> {
    let mut rx: u64 = 0;
    let mut tx: u64 = 0;
    let mut past_header = false;

    for line in content.lines() {
        let trimmed = line.trim();
        // Two header lines start with "Inter-|" and "face |..."
        if !past_header {
            if trimmed.starts_with("Inter-") || trimmed.starts_with("face") {
                continue;
            }
            // The first non-header line marks data start
            past_header = true;
        }

        // Each data line: `iface: rx_bytes rx_packets ... tx_bytes ...`
        let colon = match trimmed.find(':') {
            Some(p) => p,
            None => continue,
        };
        let iface = trimmed[..colon].trim();
        if iface.is_empty() {
            continue;
        }
        // Skip loopback
        if iface == "lo" {
            continue;
        }

        let fields: Vec<&str> = trimmed[colon + 1..].split_whitespace().collect();
        // Field layout per /proc/net/dev:
        // 0:rx_bytes 1:rx_packets 2:rx_errs 3:rx_drop 4:rx_fifo 5:rx_frame
        // 6:rx_compressed 7:rx_multicast | 8:tx_bytes 9:tx_packets ...
        let iface_rx = fields.get(0).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        let iface_tx = fields.get(8).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        rx += iface_rx;
        tx += iface_tx;
    }

    Ok((rx, tx))
}

/// Parse /proc/diskstats: sum read_sectors * 512 and write_sectors * 512
/// across all real block devices (skip loop*, ram*, sr*).
/// Returns `(read_bytes, write_bytes)` cumulative since boot.
///
/// /proc/diskstats line format (17 fields):
/// `major minor name reads_completed reads_merged sectors_read ms_reading
///  writes_completed writes_merged sectors_written ms_writing ...`
/// field indices (0-based after the leading 3): reads_completed=3, sectors_read=5,
/// writes_completed=7, sectors_written=9.
///
/// We use sectors_read (field 5) and sectors_written (field 9); each sector = 512 bytes
/// per Linux kernel docs regardless of actual hardware block size.
pub fn parse_proc_diskstats(content: &str) -> Result<(u64, u64), String> {
    let mut read_bytes: u64 = 0;
    let mut write_bytes: u64 = 0;

    for line in content.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        // Need at least name + 7 numeric fields to reach sectors_written (idx 9).
        // Absolute minimum: 14 fields. Be lenient: require >= 11 (idx 0..=10).
        if fields.len() < 11 {
            continue;
        }
        // fields[0]=major, [1]=minor, [2]=name, [3]=reads_completed, [5]=sectors_read,
        // [7]=writes_completed, [9]=sectors_written
        let name = fields[2];

        // Filter virtual / optical / ram devices
        if name.starts_with("loop")
            || name.starts_with("ram")
            || name.starts_with("sr")
            || name.starts_with("fd")
        {
            continue;
        }

        // Skip partitions that have no reads *and* no writes — typically virtual / idle
        // sub-devices that would double-count against their parent.
        let reads_completed: u64 = fields[3].parse().unwrap_or(0);
        let writes_completed: u64 = fields[7].parse().unwrap_or(0);
        if reads_completed == 0 && writes_completed == 0 {
            continue;
        }

        let sectors_read: u64 = fields[5].parse().unwrap_or(0);
        let sectors_written: u64 = fields[9].parse().unwrap_or(0);
        read_bytes += sectors_read.saturating_mul(512);
        write_bytes += sectors_written.saturating_mul(512);
    }

    Ok((read_bytes, write_bytes))
}

// ---------------------------------------------------------------------------
// Snapshot builder — given combined /proc/* output, run all 4 parsers
// ---------------------------------------------------------------------------

/// Build a ResourceSnapshot from the combined stdout of:
///   cat /proc/stat; cat /proc/meminfo; cat /proc/net/dev; cat /proc/diskstats
///
/// We split the combined output by looking for known section anchors
/// ("cpu " for stat, "MemTotal:" for meminfo, "Inter-|" for net/dev, and
/// the first diskstats line). Parsers are tolerant of partial / missing
/// sections — every field gracefully degrades to 0.
pub fn build_snapshot(
    session_id: &str,
    combined: &str,
    prev_cpu: Option<(u64, u64)>,
) -> ResourceSnapshot {
    let (stat_section, meminfo_section, net_section, disk_section) = split_proc_output(combined);

    let (idle, total, cores) = parse_proc_stat(&stat_section).unwrap_or((0, 0, 0));
    let (mem_total, mem_used) = parse_proc_meminfo(&meminfo_section).unwrap_or((0, 0));
    let (net_rx, net_tx) = parse_proc_net_dev(&net_section).unwrap_or((0, 0));
    let (disk_read, disk_write) = parse_proc_diskstats(&disk_section).unwrap_or((0, 0));

    let cpu_usage = compute_cpu_usage(prev_cpu, idle, total);

    // disk_total / disk_used are NOT available from /proc/* alone — they need
    // a separate `df` call. We surface 0 here; the frontend can either ignore
    // these fields or call a separate command. Keeping them in the struct
    // preserves wire-compat with the design.
    ResourceSnapshot {
        session_id: session_id.to_string(),
        cpu_usage,
        cpu_cores: cores,
        mem_total,
        mem_used,
        net_rx_bytes: net_rx,
        net_tx_bytes: net_tx,
        disk_read_bytes: disk_read,
        disk_write_bytes: disk_write,
        disk_total: 0,
        disk_used: 0,
        timestamp: unix_millis(),
    }
}

fn compute_cpu_usage(prev: Option<(u64, u64)>, idle: u64, total: u64) -> f32 {
    match prev {
        Some((prev_idle, prev_total)) => {
            let total_d = total.saturating_sub(prev_total);
            let idle_d = idle.saturating_sub(prev_idle);
            if total_d == 0 {
                return 0.0;
            }
            let busy_d = total_d.saturating_sub(idle_d);
            let pct = (busy_d as f64 / total_d as f64) * 100.0;
            // Clamp 0..=100 to defend against counter wraparound / kernel quirks.
            pct.clamp(0.0, 100.0) as f32
        }
        None => 0.0, // First sample — no delta yet
    }
}

/// Split the combined `cat /proc/stat; cat /proc/meminfo; cat /proc/net/dev;
/// cat /proc/diskstats` output into 4 sections. Parsers tolerate sections that
/// contain unrelated lines, so we use generous anchor-based splitting.
fn split_proc_output(combined: &str) -> (String, String, String, String) {
    // Find anchor byte offsets
    let stat_idx = combined.find("cpu ");
    let mem_idx = combined.find("MemTotal:");
    let net_idx = combined.find("Inter-|");
    // diskstats anchor: a line beginning with digits then digits then "sd" or "nvme" or "vd"
    // — too brittle. Instead use "the section after net/dev" = everything after net_idx's
    // data block. We approximate by picking the next anchor: a line that looks like
    // diskstats (3 leading numeric fields). But simpler: assume the 4 sections appear in
    // the canonical order and split by the next anchor.

    // Strategy: order the anchors by offset (filter None), then carve sections between them.
    // diskstats is whatever falls after net_idx (or mem_idx if no net).
    let mut anchors: Vec<(usize, &str)> = Vec::new();
    if let Some(i) = stat_idx {
        anchors.push((i, "stat"));
    }
    if let Some(i) = mem_idx {
        anchors.push((i, "mem"));
    }
    if let Some(i) = net_idx {
        anchors.push((i, "net"));
    }
    anchors.sort_by_key(|(i, _)| *i);

    let stat_end = next_anchor_end(&anchors, "stat");
    let mem_end = next_anchor_end(&anchors, "mem");
    let net_end = next_anchor_end(&anchors, "net");

    let stat_section = slice_between(combined, stat_idx, stat_end);
    let mem_section = slice_between(combined, mem_idx, mem_end);
    let net_section = slice_between(combined, net_idx, net_end);

    // disk_section = everything after the net block (or after mem if net missing,
    // or after stat if both missing, or full output if all missing).
    let disk_start = net_end.or(mem_end).or(stat_end).or(Some(combined.len()));
    let disk_section = match disk_start {
        Some(s) if s < combined.len() => combined[s..].to_string(),
        _ => String::new(),
    };

    (stat_section, mem_section, net_section, disk_section)
}

/// Return the end offset (exclusive) of the section starting at the named anchor.
/// The section ends where the next anchor begins, or at EOF if it's the last one.
fn next_anchor_end(anchors: &[(usize, &str)], name: &str) -> Option<usize> {
    // Find the anchor with this name
    let idx = anchors.iter().position(|(_, n)| *n == name)?;
    // The section ends at the start of the next anchor, or None (→ EOF) if last.
    anchors.get(idx + 1).map(|(i, _)| *i)
}

fn slice_between(s: &str, start: Option<usize>, end: Option<usize>) -> String {
    match (start, end) {
        (Some(st), Some(en)) if st <= en => s[st..en].to_string(),
        (Some(st), None) => s[st..].to_string(),
        _ => String::new(),
    }
}

/// Public helper used by ssh.rs to extract just the /proc/stat section from a
/// combined MonitorExec stdout. Used to read the latest (idle, total) jiffies
/// after build_snapshot so we can stash prev_cpu for the next tick.
pub fn extract_stat_section(combined: &str) -> String {
    let (stat_section, _, _, _) = split_proc_output(combined);
    stat_section
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
/// 1. Builds the combined /proc command string
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
    let cmd = "cat /proc/stat; cat /proc/meminfo; cat /proc/net/dev; cat /proc/diskstats";

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

// ---------------------------------------------------------------------------
// Tests — parser robustness against representative /proc samples
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const PROC_STAT_SAMPLE: &str = "\
cpu  10181862 6008336 3798227 539905979 131358 0 68270 0 0 0
cpu0 1289010 782334 488985 67406790 16331 0 8928 0 0 0
cpu1 1295837 767081 470201 67459848 16842 0 8512 0 0 0
cpu2 1240736 731072 468199 67504808 16515 0 8643 0 0 0
cpu3 1269921 759083 484392 67434836 15759 0 8433 0 0 0
cpu4 1315556 745223 471556 67468609 16196 0 8471 0 0 0
cpu5 1245223 741347 472986 67479872 16107 0 8431 0 0 0
cpu6 1270316 741231 471148 67492788 16477 0 8531 0 0 0
cpu7 1255263 748965 472760 67458428 16131 0 8321 0 0 0
intr 2334604478 47 9 0 0 0 0 0 0 0
softirq 48350279 2 16209860 0 0 0 0 3 4835028 0 0
ctxt 1479188286
btime 1735008000
processes 4482890
procs_running 1
procs_blocked 0
";

    const PROC_MEMINFO_SAMPLE: &str = "\
MemTotal:       16266984 kB
MemFree:          387952 kB
MemAvailable:   11247792 kB
Buffers:          286712 kB
Cached:          7824540 kB
SwapCached:            0 kB
Active:          5091184 kB
Inactive:        6137180 kB
";

    const PROC_NET_DEV_SAMPLE: &str = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 4194304   32768    0    0    0     0          0         0  4194304   32768    0    0    0     0       0          0
  eth0: 123456789 987654    0    0    0     0          0         0 987654321 654321    0    0    0     0       0          0
  eth1:     1024      16    0    0    0     0          0         0      2048      32    0    0    0     0       0          0
";

    const PROC_DISKSTATS_SAMPLE: &str = "\
   7       0 loop0 0 0 0 0 0 0 0 0 0 0
   7       1 loop1 0 0 0 0 0 0 0 0 0 0
   1       0 ram0 0 0 0 0 0 0 0 0 0 0
  11       0 sr0 0 0 0 0 0 0 0 0 0 0
   8       0 sda 12345 678 9234567 89012 54321 876 7654321 43210 0 123456 132222
   8       1 sda1 1000 100 500000 1000 200 200 100000 5000 0 1500 1500
 259       0 nvme0n1 99999 8888 8888888 77777 66666 5555 5555555 44444 0 333333 111110
";

    #[test]
    fn test_parse_proc_stat_typical() {
        let (idle, total, cores) = parse_proc_stat(PROC_STAT_SAMPLE).expect("stat parse");
        // idle = column 4 (idle=539905979) + column 5 (iowait=131358)
        assert_eq!(idle, 539905979 + 131358);
        // total = sum of 10 columns after `cpu`
        let expected_total = 10181862u64
            + 6008336
            + 3798227
            + 539905979
            + 131358
            + 0
            + 68270
            + 0
            + 0
            + 0;
        assert_eq!(total, expected_total);
        // 8 cores (cpu0..cpu7)
        assert_eq!(cores, 8);
    }

    #[test]
    fn test_parse_proc_meminfo_typical() {
        let (total, used) = parse_proc_meminfo(PROC_MEMINFO_SAMPLE).expect("meminfo parse");
        // MemTotal 16266984 kB → bytes
        assert_eq!(total, 16266984 * 1024);
        // used = total - MemAvailable (11247792 kB)
        assert_eq!(used, (16266984 - 11247792) * 1024);
    }

    #[test]
    fn test_parse_proc_net_dev_typical() {
        let (rx, tx) = parse_proc_net_dev(PROC_NET_DEV_SAMPLE).expect("net_dev parse");
        // lo must be skipped. Only eth0 + eth1 counted.
        assert_eq!(rx, 123456789 + 1024);
        assert_eq!(tx, 987654321 + 2048);
        // Sanity: lo's 4194304 must NOT be included
        assert_ne!(rx, 4194304 + 123456789 + 1024);
    }

    #[test]
    fn test_parse_proc_diskstats_typical() {
        let (read_bytes, write_bytes) =
            parse_proc_diskstats(PROC_DISKSTATS_SAMPLE).expect("diskstats parse");
        // sda sectors_read=9234567 * 512; sda1 sectors_read=500000 * 512
        // nvme0n1 sectors_read=8888888 * 512
        let expected_read = (9234567 + 500000 + 8888888) * 512;
        // sda sectors_written=7654321 * 512; sda1=100000 * 512; nvme0n1=5555555 * 512
        let expected_write = (7654321 + 100000 + 5555555) * 512;
        assert_eq!(read_bytes, expected_read);
        assert_eq!(write_bytes, expected_write);
    }

    #[test]
    fn test_compute_cpu_usage_delta() {
        // First sample: no prev → 0
        assert_eq!(compute_cpu_usage(None, 100, 200), 0.0);
        // Total doubled, idle grew by 50 → busy grew by 50 → 50%
        let pct = compute_cpu_usage(Some((100, 200)), 150, 300);
        assert!((pct - 50.0).abs() < 0.001, "expected 50%, got {pct}");
        // All idle → 0%
        let pct = compute_cpu_usage(Some((0, 100)), 100, 200);
        assert!((pct - 0.0).abs() < 0.001, "expected 0%, got {pct}");
        // All busy → 100%
        let pct = compute_cpu_usage(Some((0, 100)), 0, 100);
        assert!((pct - 100.0).abs() < 0.001, "expected 100%, got {pct}");
    }

    #[test]
    fn test_split_proc_output_finds_all_sections() {
        let combined = format!(
            "{}\n{}\n{}\n{}\n",
            PROC_STAT_SAMPLE, PROC_MEMINFO_SAMPLE, PROC_NET_DEV_SAMPLE, PROC_DISKSTATS_SAMPLE
        );
        let (stat, mem, net, disk) = split_proc_output(&combined);
        assert!(stat.contains("cpu  "));
        assert!(mem.contains("MemTotal:"));
        assert!(net.contains("Inter-|"));
        assert!(disk.contains("sda"));
    }

    #[test]
    fn test_parse_proc_stat_tolerates_garbage() {
        // No `cpu` aggregate line → Err
        assert!(parse_proc_stat("nothing here\nin this file").is_err());
    }

    #[test]
    fn test_parse_proc_net_dev_empty_returns_zeros() {
        // Only headers, no data lines → (0, 0)
        let (rx, tx) = parse_proc_net_dev(
            "Inter-|   Receive ...\n face |bytes ...\n",
        )
        .expect("net_dev empty");
        assert_eq!((rx, tx), (0, 0));
    }
}
