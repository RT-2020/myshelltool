//! 远程 /proc/* 采样输出的纯解析层。
//!
//! 为什么放 core（v2.8，与 dangerous_commands / shell / redact 迁入同一纪律）：
//! src-tauri 的测试二进制在本机因 Tauri runtime DLL 缺失报
//! `STATUS_ENTRYPOINT_NOT_FOUND` **根本起不来**——留在 src-tauri 的
//! resource_monitor.rs 测试模块（20+ 个解析用例）从未真正运行过，是假覆盖。
//! 解析判据（网络双计、LVM/RAID 叠加、伪文件系统过滤、分段失败语义）必须在
//! `npm run test:core` 里真跑。
//!
//! 本模块无 Tauri 依赖：输入是采样命令（[`MONITOR_SAMPLE_COMMAND`]）的拼接
//! stdout，输出是 [`ResourceSnapshot`]。src-tauri 的 resource_monitor.rs
//! re-export 本模块符号，调用点（ssh.rs / lib.rs）不变。
//!
//! 段落顺序约定（[`split_proc_output`] 的切分前提）：stat → meminfo → net/dev
//! → diskstats → df → route。route 必须在最后（route 段 = Iface 表头锚点到
//! EOF），各段以 `;` 分隔，单段失败只损失该段。

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// 单个挂载点的容量信息（df 全量输出的过滤产物）。单位字节。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskMountInfo {
    pub mount: String,
    pub total: u64,
    pub used: u64,
}

/// One sampling of remote resource usage. All byte fields are cumulative
/// (since boot) unless otherwise noted; CPU is instantaneous percent.
//
// `rename_all = "camelCase"` 是关键：前端 store/chart 全部按 camelCase 读
// (cpuUsage / memTotal / diskTotal ...)，缺这条会序列化成 snake_case
// 导致前端每个字段都读 undefined，图表全显示 0。这是「数据不正确」的根因。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
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
    /// 根分区容量（diskTotal/diskUsed 曲线与兜底单行展示沿用；多挂载点明细见 disks）。
    pub disk_total: u64,
    pub disk_used: u64,
    /// 全部真实文件系统挂载点（伪文件系统已过滤）。空 = df 段失败或无可展示行，
    /// 前端回退到 diskTotal/diskUsed 单行。serde 缺省不序列化，兼容旧载荷消费方。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disks: Vec<DiskMountInfo>,
    pub timestamp: u64,
    /// 次要段（net/diskstats/df）解析失败时的降级说明（如 "df 解析失败"）。
    /// None = 全部段正常。前端读 `degraded` 字段展示降级提示（事件契约 v2.5 冻结）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degraded: Option<String>,
}

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
///
/// MemTotal 缺失 → Err（/proc/meminfo 不存在或不可读 = 非 Linux 远端，
/// 不能再静默返回 (0,0) 假快照——前端图表恒 0% 看上去像「负载健康」）。
/// MemAvailable 缺失（老内核）→ 退回 MemFree。
pub fn parse_proc_meminfo(content: &str) -> Result<(u64, u64), String> {
    let mut mem_total_kb: Option<u64> = None;
    let mut mem_avail_kb: Option<u64> = None;
    let mut mem_free_kb: Option<u64> = None;

    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            mem_total_kb = parse_kb_value(rest);
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            mem_avail_kb = parse_kb_value(rest);
        } else if let Some(rest) = line.strip_prefix("MemFree:") {
            mem_free_kb = parse_kb_value(rest);
        }
    }

    let total = mem_total_kb
        .ok_or_else(|| "parse_proc_meminfo: no MemTotal line found".to_string())?
        .saturating_mul(1024);
    // MemAvailable 缺失（老内核）退回 MemFree；两者都缺则无法计算 used → Err。
    let avail = mem_avail_kb
        .or(mem_free_kb)
        .ok_or_else(|| "parse_proc_meminfo: no MemAvailable/MemFree line found".to_string())?
        .saturating_mul(1024);
    let used = total.saturating_sub(avail);
    Ok((total, used))
}

fn parse_kb_value(rest: &str) -> Option<u64> {
    let trimmed = rest.trim().trim_end_matches("kB").trim();
    trimmed.parse::<u64>().ok()
}

/// /proc/net/dev 数据行 → (iface, rx_bytes, tx_bytes) 列表。
///
/// 返回 `Err` 当段落整体缺失（空串 = 输出里没有 "Inter-|" 锚点，例如非 Linux
/// 内核或 /proc 被掩蔽）：此时数值没有任何事实来源，调用方必须按 degraded 处理，
/// 不得静默给全 0（v2.5 失败语义；此前签名虽是 Result 却从不返回 Err，
/// build_snapshot 的 `degraded "net"` 分支因此是死代码，net 段缺失时前端看到
/// 的是「正常的 0 KB/s」）。
/// 「有表头但没有任何接口数据行」不属缺失：返回空列表 → 求和为 0（既有行为）。
fn net_dev_entries(content: &str) -> Result<Vec<(&str, u64, u64)>, String> {
    if content.trim().is_empty() {
        return Err("net/dev section missing".to_string());
    }
    let mut out = Vec::new();
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
        out.push((iface, iface_rx, iface_tx));
    }

    Ok(out)
}

/// Parse /proc/net/dev: sum rx_bytes / tx_bytes across all non-loopback
/// interfaces. Returns `(rx_bytes, tx_bytes)` cumulative since boot.
///
/// 全量口径（不区分接口角色）。作为「无默认路由可用」时的回退路径保留；
/// 主路径见 [`sum_net_dev_by_primary`]。
pub fn parse_proc_net_dev(content: &str) -> Result<(u64, u64), String> {
    let entries = net_dev_entries(content)?;
    let mut rx: u64 = 0;
    let mut tx: u64 = 0;
    for (_, iface_rx, iface_tx) in &entries {
        rx += *iface_rx;
        tx += *iface_tx;
    }
    Ok((rx, tx))
}

/// 按默认路由接口求和 /proc/net/dev 字节数（网络口径的主路径）。
///
/// 为什么不能全量累加：隧道（tun*/wg*）、容器桥（docker0/br-*）与 veth 对和
/// 物理网卡记的是**同一份**字节流——一次容器进出流量在 veth、docker0、物理网卡
/// 的计数器上各加一次，VPN 流量在 tun0 与物理网卡上各加一次，全量求和会把网速
/// 虚高约 2 倍（AGENTS.md v2.6 审计 backlog #1）。默认路由接口是流量真正离开
/// 主机的出口，按它求和每字节只计一次。
///
/// 两个回退（都退回全量口径 [`parse_proc_net_dev`]，不靠接口名前缀猜角色——
/// 接口可以被随意改名，前缀判据必然被等价形态绕过）：
/// - `primary` 为空：远端没有默认路由（IPv6-only / 静态路由机器 / 受限 procfs）；
/// - `primary` 非空但一个都没匹配上：route 与 net/dev 两次 `cat` 之间接口被
///   改名/拆除（微秒级竞态窗口），路由表此刻已不可信。
pub fn sum_net_dev_by_primary(
    content: &str,
    primary: &[String],
) -> Result<(u64, u64), String> {
    let entries = net_dev_entries(content)?;
    if primary.is_empty() {
        return parse_proc_net_dev(content);
    }
    let mut rx: u64 = 0;
    let mut tx: u64 = 0;
    let mut matched = false;
    for (iface, iface_rx, iface_tx) in &entries {
        if primary.iter().any(|p| p == iface) {
            matched = true;
            rx += *iface_rx;
            tx += *iface_tx;
        }
    }
    if matched {
        Ok((rx, tx))
    } else {
        parse_proc_net_dev(content)
    }
}

/// 解析 /proc/net/route：返回持有**默认路由**（Destination == "00000000"）的
/// 接口名，去重并保持出现顺序。
///
/// 数据行 11 列（tab 分隔，内核 `net/ipv4/fib_trie.c` 以 `%08X` 打印）：
/// `Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT`，
/// Destination/Gateway/Mask 为无 0x 前缀的 8 位十六进制。
///
/// 多条默认路由（ECMP / 双上联 / VPN 全局接管）**全部保留**：同一字节只会
/// 经过其中一条路径，求和不会重复计数（计数器为开机累计值，前端按快照差分
/// 得速率，与路由何时切换无关）。
///
/// 严格形态校验（恰好 8 个十六进制字符）同时把泄入本段的无关行挡在外面：
/// split_proc_output 在 `df` 段缺失时会把 route 段切进 diskstats 的范围之后…
/// 实际上 route 永远在输出末尾，但段切分对「锚点缺失」是尽力而为，解析器
/// 自身必须能容忍垃圾行（diskstats 数字行、meminfo 行等都不满足 8 位 hex +
/// "00000000" 的组合）。
pub fn parse_proc_net_route(content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in content.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            continue;
        }
        let dest = fields[1];
        if dest.len() != 8 || !dest.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        if dest != "00000000" {
            continue;
        }
        let iface = fields[0];
        if iface.is_empty() || iface == "lo" {
            continue;
        }
        if !out.iter().any(|name| name == iface) {
            out.push(iface.to_string());
        }
    }
    out
}

/// 从 /proc/diskstats 文本累加 IO 字节（纯函数，可单测）。
///
/// 判据（每条都对应一次「换个环境就炸」）：
/// - 过滤虚拟/光学/内存盘（loop/ram/sr/fd）；
/// - 过滤**栈式设备** dm-*（LVM）/ md*（软 RAID）/ nbd*/zd*/drbd*：它们与底层
///   物理设备记同一份 IO（一次写 LVM 卷同时记在 dm-0 与 PV 整盘，RAID1 记在
///   md0 与每块成员盘），累加会虚高 2-3 倍。物理设备始终在样本里，跳过不丢数据；
/// - 分区去重：同一 IO 在内核里同时记在父设备与分区上（sda 与 sda1），仅当父设备
///   在本样本中**有活动**时跳过分区（父设备缺失/零活动则照常计入，不静默丢数据）。
pub fn sum_proc_diskstats(content: &str) -> (u64, u64) {
    // 第一遍：收集本样本全部候选设备（已过滤 loop/ram/sr/fd）。
    // 元组：(name, reads_completed, writes_completed, sectors_read, sectors_written)
    let mut devices: Vec<(&str, u64, u64, u64, u64)> = Vec::new();

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

        // 栈式设备（LVM 的 dm-*、软 RAID 的 md*、网络块设备 nbd*/zd*/drbd*）
        // 与底层物理设备**记同一份 IO**：一次写 LVM 卷会同时记在 dm-0、PV 分区
        // 与整盘三处，RAID1 写还会记在 md0 与每块成员盘上。名字后缀规则
        //（is_partition_suffix）只能识别「整盘↔分区」，识别不了这种栈式关系，
        // 全部累加会让磁盘 IO 虚高 2-3 倍（Linux 上 iostat 对 dm 设备同样双计）。
        // 物理设备始终在样本中，跳过栈式设备不会丢数据。
        if name.starts_with("dm-")
            || name.starts_with("md")
            || name.starts_with("nbd")
            || name.starts_with("zd")
            || name.starts_with("drbd")
        {
            continue;
        }

        devices.push((
            name,
            fields[3].parse().unwrap_or(0),
            fields[7].parse().unwrap_or(0),
            fields[5].parse().unwrap_or(0),
            fields[9].parse().unwrap_or(0),
        ));
    }

    // 第二遍：零活动设备跳过；分区在同样本有活动父设备时跳过（只计父）。
    let mut read_bytes: u64 = 0;
    let mut write_bytes: u64 = 0;
    for (name, reads_completed, writes_completed, sectors_read, sectors_written) in &devices {
        // Skip devices that have no reads *and* no writes — typically virtual / idle
        // sub-devices that would double-count against their parent.
        if *reads_completed == 0 && *writes_completed == 0 {
            continue;
        }

        // 分区判定：存在同名父设备（有活动）且本设备是其分区 → 跳过
        let parent_active = devices.iter().any(|(base, b_reads, b_writes, _, _)| {
            (*b_reads != 0 || *b_writes != 0) && is_partition_suffix(base, name)
        });
        if parent_active {
            continue;
        }

        read_bytes += sectors_read.saturating_mul(512);
        write_bytes += sectors_written.saturating_mul(512);
    }

    (read_bytes, write_bytes)
}

/// Parse /proc/diskstats: sum read_sectors * 512 and write_sectors * 512
/// across all real block devices. 去重规则见 `sum_proc_diskstats`（可单测纯函数）。
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
///
/// 段整体缺失（空串）→ Err → degraded（与 net/dev 同一口径，v2.5 失败语义：
/// 缺数据必须可见，不得伪装成「正常 0 字节」）。「文件存在但没有设备行」
/// （极端精简容器）不属缺失 → Ok((0, 0))。
pub fn parse_proc_diskstats(content: &str) -> Result<(u64, u64), String> {
    if content.trim().is_empty() {
        return Err("diskstats section missing".to_string());
    }
    Ok(sum_proc_diskstats(content))
}

/// `part` 是否是 `base` 的分区名（仅名字规则判断，规则见 parse_proc_diskstats 文档）。
fn is_partition_suffix(base: &str, part: &str) -> bool {
    if !part.starts_with(base) {
        return false;
    }
    let suffix = &part[base.len()..];
    if base.as_bytes()[base.len() - 1].is_ascii_digit() {
        // base 以数字结尾（nvme0n1、mmcblk0）：后缀须为 "p" + 至少一位数字
        suffix.len() >= 2
            && suffix.starts_with('p')
            && suffix[1..].bytes().all(|b| b.is_ascii_digit())
    } else {
        // base 以非数字结尾（sda、vdab）：后缀须为纯数字（sdab 不是 sda 的分区）
        !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
    }
}

/// 伪文件系统设备名黑名单：内核/容器运行时挂的虚拟 FS，容量对运维无意义，
/// 不过滤会把磁盘列表刷成 tmpfs/overlay 噪音（FinalShell 同样隐藏这类行）。
/// 判据按 df 第一列的**完整设备名**精确匹配（除 `fuse.`/`/dev/loop` 前缀外），
/// 不做子串匹配——`/dev/sda1` 不因含 "a" 之类被误杀。
fn is_pseudo_fs_device(dev: &str) -> bool {
    const NAMES: &[&str] = &[
        "tmpfs", "devtmpfs", "overlay", "squashfs", "shm", "efivarfs", "iso9660",
        "cgroup", "cgroup2", "none", "fusectl", "mqueue", "hugetlbfs", "nsfs",
        "proc", "sysfs", "devpts", "binfmt_misc", "configfs", "pstore",
        "securityfs", "debugfs", "tracefs", "autofs", "rpc_pipefs", "sunrpc",
        "bpf", "tracefs", "devfs", "fdescfs", "linprocfs", "linsysfs",
    ];
    NAMES.contains(&dev) || dev.starts_with("fuse.")
}

/// 虚拟挂载点前缀黑名单：/proc /sys /dev /run /snap 下的挂载（tmpfs/devtmpfs/
/// loop 快照等）与设备名黑名单双保险——按**路径段边界**判断（"/devx" 不算 "/dev" 下）。
fn is_virtual_mount_point(mount: &str) -> bool {
    const PREFIXES: &[&str] = &["/proc", "/sys", "/dev", "/run", "/snap"];
    PREFIXES
        .iter()
        .any(|&p| mount == p || mount.starts_with(&format!("{}/", p)))
}

/// Parse `env LC_ALL=C df -P -k`（无参数 = 全部挂载）为挂载点容量列表。
///
/// 过滤伪文件系统（设备名 + 挂载点双黑名单）后按 df 输出顺序返回，
/// 上限 [`MAX_DF_MOUNTS`] 行（防异常远端冒出成百行把 IPC 载荷撑爆；根分区
/// 通常在最前，截断只影响尾部）。数值 1K 块 × 1024 = 字节。
pub fn parse_df_mounts(content: &str) -> Vec<DiskMountInfo> {
    /// 多挂载点列表上限（FinalShell 同款信息密度；正常服务器过滤后 < 8 行）
    const MAX_DF_MOUNTS: usize = 12;

    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        // 至少 6 列：Filesystem 1024-blocks Used Available Use% Mounted-on
        if fields.len() < 6 {
            continue;
        }
        // 挂载点是最后一列（-P 保证单行，含空格的挂载点 POSIX 下不存在——
        // `-P` 本身就是为防换行而设，空格路径需转义，df -P 不会输出）
        let mount = fields[fields.len() - 1];
        // 数值列解析失败 → 表头行（"1024-blocks"）或畸形行，跳过
        let (total_kb, used_kb): (u64, u64) = match (fields[1].parse(), fields[2].parse()) {
            (Ok(t), Ok(u)) => (t, u),
            _ => continue,
        };
        if is_pseudo_fs_device(fields[0]) || is_virtual_mount_point(mount) {
            continue;
        }
        out.push(DiskMountInfo {
            mount: mount.to_string(),
            total: total_kb.saturating_mul(1024),
            used: used_kb.saturating_mul(1024),
        });
        if out.len() >= MAX_DF_MOUNTS {
            break;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Snapshot builder — given combined /proc/* output, run all parsers
// ---------------------------------------------------------------------------

/// Build a ResourceSnapshot from the combined stdout of:
///   cat /proc/stat; cat /proc/meminfo; cat /proc/net/dev; cat /proc/diskstats; env LC_ALL=C df -P -k
///
/// We split the combined output by looking for known section anchors
/// ("cpu " for stat, "MemTotal:" for meminfo, "Inter-|" for net/dev,
/// first diskstats line, and "Filesystem" for df).
///
/// 分层失败语义（v2.5，杜绝全 0 假快照）：
/// - `parse_proc_stat` / `parse_proc_meminfo` 失败 = 无 /proc（非 Linux 远端），
///   快照主体数据全部不可信 → `Err`（调用侧停轮询 + emit 错误事件）。
/// - 仅 net / diskstats / df 等次要段失败 → 快照照发（CPU/内存仍可信），
///   `degraded` 字段标注失败段，前端展示降级提示。
pub fn build_snapshot(
    session_id: &str,
    combined: &str,
    prev_cpu: Option<(u64, u64)>,
) -> Result<ResourceSnapshot, String> {
    let (stat_section, meminfo_section, net_section, route_section, disk_section, df_section) =
        split_proc_output(combined);

    // 主要段（CPU/内存）：失败 = 无 /proc → 不可信，整份快照拒绝。
    let (idle, total, cores) = parse_proc_stat(&stat_section)
        .map_err(|_| "远端无 /proc，资源监控仅支持 Linux".to_string())?;
    let (mem_total, mem_used) = parse_proc_meminfo(&meminfo_section)
        .map_err(|_| "远端无 /proc，资源监控仅支持 Linux".to_string())?;

    // 次要段：失败 → degraded 标注，数值置 0（该段本来就没有数据）。
    let mut degraded_parts: Vec<&str> = Vec::new();
    // 网络口径：默认路由接口优先（防隧道/容器桥与物理网卡双计，见
    // sum_net_dev_by_primary），取不到时由解析器内部回退全量口径。
    let route_primary = parse_proc_net_route(&route_section);
    let (net_rx, net_tx) = sum_net_dev_by_primary(&net_section, &route_primary).unwrap_or_else(|_| {
        degraded_parts.push("net");
        (0, 0)
    });
    let (disk_read, disk_write) = parse_proc_diskstats(&disk_section).unwrap_or_else(|_| {
        degraded_parts.push("diskstats");
        (0, 0)
    });
    let mounts = parse_df_mounts(&df_section);
    let (disk_total, disk_used) = if mounts.is_empty() {
        degraded_parts.push("df");
        (0, 0)
    } else {
        // 根分区优先，无 / 行（chroot/容器极端场景）回退首个真实挂载点
        let root = mounts.iter().find(|m| m.mount == "/").unwrap_or(&mounts[0]);
        (root.total, root.used)
    };
    let degraded = (!degraded_parts.is_empty()).then(|| format!("{} 解析失败", degraded_parts.join("/")));

    let cpu_usage = compute_cpu_usage(prev_cpu, idle, total);

    Ok(ResourceSnapshot {
        session_id: session_id.to_string(),
        cpu_usage,
        cpu_cores: cores,
        mem_total,
        mem_used,
        net_rx_bytes: net_rx,
        net_tx_bytes: net_tx,
        disk_read_bytes: disk_read,
        disk_write_bytes: disk_write,
        disk_total,
        disk_used,
        disks: mounts,
        timestamp: unix_millis(),
        degraded,
    })
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

/// Split the combined [`MONITOR_SAMPLE_COMMAND`] output into 6 sections.
/// Parsers tolerate sections that contain unrelated lines, so we use
/// generous anchor-based splitting.
///
/// diskstats 是唯一没有自然文本锚点的段（/proc/stat 有 `cpu `、meminfo 有
/// `MemTotal:`、net/dev 有 `Inter-|`、df 有 `Filesystem`、route 有
/// `Iface\tDestination`，diskstats 首行只是裸数字）。因此采样命令在它前面
/// 显式 `echo` 一个 [`DISKSTATS_MARKER`]——按协议注入边界，而不是从
/// 相邻段推边界。历史教训（迁移到 core 前测试从未真正运行，此 bug 长期
/// 藏着）：曾用「net 段结束 = 下一个锚点(df)」推 disk 段起点，而 disk 段
/// 终点也是 df 锚点 → `[df, df)` 恒为空串，**磁盘 IO 读写字节自引入 df 段
/// 起恒为 0**，被 `test_split_proc_output_finds_all_sections` 的
/// `disk.contains("sda")` 断言当场揭穿。
fn split_proc_output(combined: &str) -> (String, String, String, String, String, String) {
    // Find anchor byte offsets
    let stat_idx = combined.find("cpu ");
    let mem_idx = combined.find("MemTotal:");
    let net_idx = combined.find("Inter-|");
    // diskstats 显式边界标记（echo 注入，见函数文档）
    let disk_idx = combined.find(DISKSTATS_MARKER);
    // df anchor: the POSIX header "Filesystem" (df -P 输出首行).
    let df_idx = combined.find("Filesystem");
    // route anchor: 表头前两列（tab 分隔）。net/dev 数据行理论上可以有接口叫
    // "Iface0"（Linux 允许任意接口名），但锚点要求 Iface 后紧跟 tab，接口名与
    // 列头不会混淆。
    let route_idx = combined.find("Iface\tDestination");

    // Strategy: order the anchors by offset (filter None), then carve sections between them.
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
    if let Some(i) = disk_idx {
        anchors.push((i, "disk"));
    }
    if let Some(i) = df_idx {
        anchors.push((i, "df"));
    }
    if let Some(i) = route_idx {
        anchors.push((i, "route"));
    }
    anchors.sort_by_key(|(i, _)| *i);

    let stat_end = next_anchor_end(&anchors, "stat");
    let mem_end = next_anchor_end(&anchors, "mem");
    let net_end = next_anchor_end(&anchors, "net");
    let df_start = anchors.iter().find(|(_, n)| *n == "df").map(|(i, _)| *i);

    let stat_section = slice_between(combined, stat_idx, stat_end);
    let mem_section = slice_between(combined, mem_idx, mem_end);
    let net_section = slice_between(combined, net_idx, net_end);

    // disk_section = 标记之后到 df 之前（df 缺失则到 route 之前或 EOF——route
    // 数据行在 diskstats 映射下 sectors 列是 Use/Window，Use 非零时会污染磁盘
    // 字节数，必须截在 route 锚点前）。标记缺失（远端 echo 不可用的极端情形）
    // → 空段 → degraded，不做「从相邻段猜边界」的回退。
    let disk_start = disk_idx.map(|i| i + DISKSTATS_MARKER.len());
    let disk_end = [df_start, route_idx].into_iter().flatten().min();
    let disk_section = match (disk_start, disk_end) {
        (Some(s), Some(e)) if s < e => combined[s..e].to_string(),
        (Some(s), None) if s < combined.len() => combined[s..].to_string(),
        _ => String::new(),
    };

    // df_section = 从 Filesystem 锚点到 route 锚点（route 在输出末尾）或 EOF。
    let df_end = next_anchor_end(&anchors, "df");
    let df_section = slice_between(combined, df_idx, df_end);

    // route_section = 从 Iface 表头锚点到 EOF（采样命令把它放在最后）。
    let route_section = slice_between(combined, route_idx, None);

    (stat_section, mem_section, net_section, route_section, disk_section, df_section)
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
    let (stat_section, _, _, _, _, _) = split_proc_output(combined);
    stat_section
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// diskstats 段的显式边界标记（采样命令在 `cat /proc/diskstats` 前 `echo` 它）。
/// diskstats 是六段里唯一没有自然文本锚点的段（首行是裸数字），其余五段靠
/// `cpu `/`MemTotal:`/`Inter-|`/`Filesystem`/`Iface\tDestination` 天然锚定。
/// 标记长度 20 > IFNAMSIZ(16)，不可能被当成接口名混进 net/dev 段。
pub(crate) const DISKSTATS_MARKER: &str = "===PROC_DISKSTATS===";

/// 采样命令的单一来源（避免字符串散落多处后漂移）。
/// - 末尾的 `cat /proc/net/route` 供 [`parse_proc_net_route`] 取默认路由接口，
///   放最后是切分前提（route 段 = Iface 表头锚点到 EOF）；
/// - `echo '===PROC_DISKSTATS==='` 给无自然锚点的 diskstats 段注入显式边界
///   （见 [`DISKSTATS_MARKER`]）。**必须带单引号**：zsh 的 EQUALS 选项默认
///   开启，未引用的 `=word` 会按命令名做 `=` 展开并报 not found——marker
///   静默缺失、磁盘 IO 恒降级（多角色审查第 1 轮 Issue 2）。带引号对
///   POSIX sh / BusyBox / csh / tcsh / fish 均无副作用（引号由 shell 剥离）。
pub const MONITOR_SAMPLE_COMMAND: &str = "cat /proc/stat; cat /proc/meminfo; cat /proc/net/dev; echo '===PROC_DISKSTATS==='; cat /proc/diskstats; env LC_ALL=C df -P -k; cat /proc/net/route";


#[cfg(test)]
mod tests;
