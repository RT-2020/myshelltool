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

const DF_SAMPLE: &str = "\
Filesystem     1024-blocks      Used  Available Use% Mounted on
/dev/sda1        51475040  29655014   21820026  58% /
tmpfs             1638400      1205   1637195   1% /dev/shm
/dev/sda2          102400     51200     51200  50% /boot
";

#[test]
fn test_parse_proc_stat_typical() {
    let (idle, total, cores) = parse_proc_stat(PROC_STAT_SAMPLE).expect("stat parse");
    // idle = column 4 (idle=539905979) + column 5 (iowait=131358)
    assert_eq!(idle, 539905979 + 131358);
    // total = sum of 10 columns after `cpu`
    let expected_total =
        10181862u64 + 6008336 + 3798227 + 539905979 + 131358 + 0 + 68270 + 0 + 0 + 0;
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
        // 空段（输出里没有 diskstats，如 echo 标记缺失）→ Err → degraded，
        // 不再静默给 (0,0)（签名是 Result 却恒 Ok 的旧形态是死分支）
        assert!(parse_proc_diskstats("").is_err());
        assert!(parse_proc_diskstats("  \n").is_err());
    let (read_bytes, write_bytes) =
        parse_proc_diskstats(PROC_DISKSTATS_SAMPLE).expect("diskstats parse");
    // sda1 是 sda 的分区且 sda 有活动 → 跳过，只计父 sda；nvme0n1 独立计入
    // sda sectors_read=9234567 * 512; nvme0n1 sectors_read=8888888 * 512
    let expected_read = (9234567 + 8888888) * 512;
    // sda sectors_written=7654321 * 512; nvme0n1=5555555 * 512
    let expected_write = (7654321 + 5555555) * 512;
    assert_eq!(read_bytes, expected_read);
    assert_eq!(write_bytes, expected_write);
}

#[test]
fn test_parse_proc_diskstats_nvme_p_suffix_vs_independent_disk() {
    // nvme0n1p1 是 nvme0n1 的分区（"p"+数字后缀）→ 跳过防双计数；
    // nvme0n10 不是 nvme0n1 的分区（数字结尾的父设备必须有 "p" 分隔）→ 独立盘照常计入
    let sample = "\
 259       0 nvme0n1 99999 8888 8888888 77777 66666 5555 5555555 44444 0 333333 111110
 259       1 nvme0n1p1 100 10 50000 100 200 20 100000 5000 0 1500 1500
 259      10 nvme0n10 111 11 11111 111 222 22 22222 2222 0 3300 3300
";
    let (read_bytes, write_bytes) = parse_proc_diskstats(sample).expect("diskstats parse");
    assert_eq!(read_bytes, (8888888 + 11111) * 512);
    assert_eq!(write_bytes, (5555555 + 22222) * 512);
}

#[test]
fn test_parse_proc_diskstats_partition_without_parent_counted() {
    // 父设备 sda 不在样本（内核某些配置只上报分区）→ sda1 照常计入，不静默丢数据
    let sample = "\
   8       1 sda1 1000 100 500000 1000 200 200 100000 5000 0 1500 1500
";
    let (read_bytes, write_bytes) = parse_proc_diskstats(sample).expect("diskstats parse");
    assert_eq!(read_bytes, 500000 * 512);
    assert_eq!(write_bytes, 100000 * 512);
}

#[test]
fn test_parse_proc_diskstats_active_partition_idle_parent_counted() {
    // 父设备在样本但零活动（自身被跳过）→ 分区照常计入
    let sample = "\
   8       0 sda 0 0 0 0 0 0 0 0 0 0
   8       1 sda1 1000 100 500000 1000 200 200 100000 5000 0 1500 1500
";
    let (read_bytes, write_bytes) = parse_proc_diskstats(sample).expect("diskstats parse");
    assert_eq!(read_bytes, 500000 * 512);
    assert_eq!(write_bytes, 100000 * 512);
}

#[test]
fn test_parse_proc_diskstats_sdab_is_not_sda_partition() {
    // sdab 是独立盘（"b" 非数字后缀，不满足 sda+数字 分区规则）→ 两个都计
    let sample = "\
   8      16 sdab 500 50 500000 500 600 60 600000 600 0 1100 1100
   8       0 sda 100 10 100000 100 200 20 200000 200 0 300 300
";
    let (read_bytes, write_bytes) = parse_proc_diskstats(sample).expect("diskstats parse");
    assert_eq!(read_bytes, (500000 + 100000) * 512);
    assert_eq!(write_bytes, (600000 + 200000) * 512);
}

#[test]
fn test_proc_diskstats_lvm_and_raid_not_double_counted() {
    // LVM：一次写 LV 同时记在 dm-0 与 PV 整盘/分区上（三者名字互不构成
    // 「整盘↔分区」关系，旧实现全计 → 磁盘 IO 虚高 2-3 倍）。
    let lvm = "\
   8       0 sda 54321 876 7654321 43210 12345 678 9234567 89012 0 123456 132222
   8       2 sda2 54321 876 7654321 43210 12345 678 9234567 89012 0 123456 132222
 253       0 dm-0 54321 876 7654321 43210 12345 678 9234567 89012 0 123456 132222
";
    let (read_bytes, write_bytes) = sum_proc_diskstats(lvm);
    // 只算一次物理 IO（sda 整盘），dm-0 与 sda2 不叠加
    assert_eq!(read_bytes, 7654321 * 512);
    assert_eq!(write_bytes, 9234567 * 512);

    // 软 RAID1：写 md0 会同时记在 md0 与每块成员盘上 → 只计成员盘
    let raid1 = "\
   8       0 sda 100 10 100000 100 200 20 200000 200 0 300 300
   8      16 sdb 100 10 100000 100 200 20 200000 200 0 300 300
   9       0 md0 100 10 100000 100 200 20 200000 200 0 300 300
";
    let (read_bytes, write_bytes) = sum_proc_diskstats(raid1);
    assert_eq!(read_bytes, (100000 + 100000) * 512);
    assert_eq!(write_bytes, (200000 + 200000) * 512);
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
    // All busy → 100%（总量增长 100、idle 不变 → 增量全是 busy）。
    // 此用例曾是坏测试：prev=(0,100)→(0,100) 总量增量为 0，函数按约定返回
    // 0%，断言却期望 100%——在 src-tauri 从未真正运行过，迁入 core 后现形。
    let pct = compute_cpu_usage(Some((100, 200)), 100, 300);
    assert!((pct - 100.0).abs() < 0.001, "expected 100%, got {pct}");
    // 总量零增量（两次采样间无 jiffy 推进）→ 0%，不得除零
    assert_eq!(compute_cpu_usage(Some((0, 100)), 0, 100), 0.0);
}

#[test]
fn test_monitor_sample_command_carries_diskstats_marker() {
    // 命令串与切分器靠 DISKSTATS_MARKER 结对：命令漏掉 echo 标记 =
    // diskstats 段永远切空（磁盘 IO 恒 0 的真 bug 就这么来的）。锁在一起。
    assert!(MONITOR_SAMPLE_COMMAND.contains(DISKSTATS_MARKER));
    // 标记必须带单引号：zsh EQUALS 默认开启，未引用的 =word 展开报 not found
    // → marker 静默缺失 → 磁盘 IO 恒降级（多角色审查第 1 轮 Issue 2）
    assert!(MONITOR_SAMPLE_COMMAND.contains(&format!("echo '{DISKSTATS_MARKER}'")));
    assert!(MONITOR_SAMPLE_COMMAND.ends_with("cat /proc/net/route"));
}

#[test]
fn test_split_proc_output_finds_all_sections() {
    let combined = format!(
        "{}\n{}\n{}\n{DISKSTATS_MARKER}\n{}\n{}\n{}\n",
        PROC_STAT_SAMPLE,
        PROC_MEMINFO_SAMPLE,
        PROC_NET_DEV_SAMPLE,
        PROC_DISKSTATS_SAMPLE,
        DF_SAMPLE,
        PROC_NET_ROUTE_SAMPLE
    );
    let (stat, mem, net, route, disk, df) = split_proc_output(&combined);
    assert!(stat.contains("cpu  "));
    assert!(mem.contains("MemTotal:"));
    assert!(net.contains("Inter-|"));
    assert!(disk.contains("sda"));
    assert!(df.contains("Filesystem"));
    assert!(route.contains("00000000"), "route 段应含默认路由行");
    // disk 段不应混入 df 内容（df 段单独切出）
    assert!(!disk.contains("Filesystem"));
    // df/route 段互不混入
    assert!(!df.contains("Iface\t"));
    assert!(!route.contains("Filesystem"));
    assert!(!disk.contains("Inter-|"));
    assert!(!disk.contains("eth0:"));
}

#[test]
fn test_split_proc_output_without_route_section() {
    // 旧远端形态（route cat 失败）：其余 5 段照常切出，route 段为空
    let combined = format!(
        "{}\n{}\n{}\n{DISKSTATS_MARKER}\n{}\n{}\n",
        PROC_STAT_SAMPLE,
        PROC_MEMINFO_SAMPLE,
        PROC_NET_DEV_SAMPLE,
        PROC_DISKSTATS_SAMPLE,
        DF_SAMPLE
    );
    let (stat, mem, net, route, disk, df) = split_proc_output(&combined);
    assert!(stat.contains("cpu  "));
    assert!(mem.contains("MemTotal:"));
    assert!(net.contains("Inter-|"));
    assert!(disk.contains("sda"));
    assert!(df.contains("Filesystem"));
    assert!(route.is_empty());
}

#[test]
fn test_split_proc_output_without_diskstats_marker_degrades() {
    // 标记缺失（远端 echo 不可用的极端情形）：disk 段切空 → degraded，
    // 不做「从相邻段猜边界」的回退——推边界的旧实现曾让磁盘 IO 恒 0（真 bug，
    // 由 disk.contains("sda") 断言在测试真正运行后揭穿）
    let combined = format!(
        "{}\n{}\n{}\n{}\n{}\n",
        PROC_STAT_SAMPLE, PROC_MEMINFO_SAMPLE, PROC_NET_DEV_SAMPLE, PROC_DISKSTATS_SAMPLE, DF_SAMPLE
    );
    let (_, _, net, route, disk, _) = split_proc_output(&combined);
    assert!(disk.is_empty(), "无标记时 disk 段必须为空（走 degraded 路径）");
    // 其余段不受影响（net 段顺延到 df 锚点，net 解析器容忍多余行）
    assert!(net.contains("Inter-|"));
    assert!(route.is_empty());
    let snap = build_snapshot("s", &combined, None).expect("主要段正常应 Ok");
    assert!(
        snap.degraded.as_deref().unwrap_or("").contains("diskstats"),
        "标记缺失应标注 diskstats 降级: {:?}",
        snap.degraded
    );
}

#[test]
fn test_parse_proc_stat_tolerates_garbage() {
    // No `cpu` aggregate line → Err
    assert!(parse_proc_stat("nothing here\nin this file").is_err());
}

#[test]
fn test_parse_proc_net_dev_empty_returns_zeros() {
    // Only headers, no data lines → (0, 0)
    let (rx, tx) =
        parse_proc_net_dev("Inter-|   Receive ...\n face |bytes ...\n").expect("net_dev empty");
    assert_eq!((rx, tx), (0, 0));
}

/// Docker + VPN 主机的 route 表形态：物理 eth0 与 VPN wg0 各持一条默认路由
/// （metric 不同），另有容器网段路由与普通子网路由。
const PROC_NET_ROUTE_SAMPLE: &str = "\
Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT
eth0\t00000000\t0100A8C0\t0003\t0\t42\t100\t00000000\t0\t\t0\t\t0
wg0\t00000000\t00000000\t0003\t0\t7\t50\t00000000\t0\t\t0\t\t0
docker0\t0000AC11\t00000000\t0001\t0\t0\t0\tFFFF0000\t0\t\t0\t\t0
eth0\t0000FEA9\t0100A8C0\t0003\t0\t0\t0\tFFFFFF00\t0\t\t0\t\t0
";

/// 与上面 route 配套的 net/dev：同一份流量在 wg0/docker0/veth/物理网卡上
/// 各记一次（容器流量走 veth→docker0→eth0，VPN 流量走 wg0→eth0）。
const PROC_NET_DEV_VIRTUAL_SAMPLE: &str = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 4194304   32768    0    0    0     0          0         0  4194304   32768    0    0    0     0       0          0
  eth0: 1000000     500    0    0    0     0          0         0  2000000     600    0    0    0     0       0          0
   wg0:  300000     100    0    0    0     0          0         0   300000     100    0    0    0     0       0          0
docker0:  300000     100    0    0    0     0          0         0   300000     100    0    0    0     0       0          0
veth1a2b:  300000     100    0    0    0     0          0         0   300000     100    0    0    0     0       0          0
";

#[test]
fn test_parse_proc_net_route_extracts_unique_default_ifaces() {
    let primary = parse_proc_net_route(PROC_NET_ROUTE_SAMPLE);
    // 两条默认路由（eth0/wg0）按出现顺序去重保留；子网路由的 eth0 不再重复，
    // docker0 的容器网段路由不是默认路由。
    assert_eq!(primary, vec!["eth0".to_string(), "wg0".to_string()]);
}

#[test]
fn test_parse_proc_net_route_ignores_garbage_and_lo() {
    // 泄入的无关行（diskstats/meminfo 形态）与 lo 默认路由都必须被忽略
    let mixed = "\
Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT
   8       0 sda 9234567 123 7654321 43210 12345 678 9234567 89012 0 123456 132222
MemTotal:       16266984 kB
lo\t00000000\t00000000\t0003\t0\t0\t0\t00000000\t0\t\t0\t\t0
eth1\t00000000\t0A000001\t0003\t0\t0\t0\t00000000\t0\t\t0\t\t0
";
    assert_eq!(parse_proc_net_route(mixed), vec!["eth1".to_string()]);
    assert!(parse_proc_net_route("").is_empty());
    assert!(parse_proc_net_route("Iface\tDestination\t...\n").is_empty());
}

#[test]
fn test_sum_net_dev_by_primary_no_double_count_with_route() {
    // backlog #1 场景：全量口径会把 docker0/veth 的镜像流量再计 2 次
    let all = parse_proc_net_dev(PROC_NET_DEV_VIRTUAL_SAMPLE).expect("net_dev parse");
    let primary = parse_proc_net_route(PROC_NET_ROUTE_SAMPLE);
    let picked =
        sum_net_dev_by_primary(PROC_NET_DEV_VIRTUAL_SAMPLE, &primary).expect("net_dev by primary");
    // 只计默认路由接口 eth0 + wg0
    assert_eq!(picked, (1000000 + 300000, 2000000 + 300000));
    // 全量口径确实虚高（同份流量被计 4 次），证明口径差异真实存在
    assert_eq!(
        all,
        (
            1000000 + 300000 + 300000 + 300000,
            2000000 + 300000 + 300000 + 300000
        )
    );
    assert!(all.0 > picked.0);
}

#[test]
fn test_sum_net_dev_by_primary_falls_back_without_default_route() {
    // 无默认路由（IPv6-only / 静态路由机器）：回退全量口径，不靠接口名猜
    let by_primary =
        sum_net_dev_by_primary(PROC_NET_DEV_VIRTUAL_SAMPLE, &[]).expect("fallback to all");
    let all = parse_proc_net_dev(PROC_NET_DEV_VIRTUAL_SAMPLE).expect("net_dev parse");
    assert_eq!(by_primary, all);
}

#[test]
fn test_sum_net_dev_by_primary_falls_back_when_primary_absent_in_dev() {
    // route 与 net/dev 不一致（两次 cat 之间接口被改名/拆除）→ 回退全量
    let primary = vec!["eth9".to_string()];
    let by_primary = sum_net_dev_by_primary(PROC_NET_DEV_VIRTUAL_SAMPLE, &primary)
        .expect("primary 全部失配应回退而非 Err");
    let all = parse_proc_net_dev(PROC_NET_DEV_VIRTUAL_SAMPLE).expect("net_dev parse");
    assert_eq!(by_primary, all);
}

#[test]
fn test_parse_proc_net_dev_missing_section_is_err() {
    // 空段（输出里根本没有 net/dev）必须 Err → degraded，不再静默全 0
    assert!(parse_proc_net_dev("").is_err());
    assert!(parse_proc_net_dev("   \n").is_err());
}

#[test]
fn test_parse_df_mounts_filters_pseudo_filesystems() {
    // 设备黑名单（tmpfs/overlay）+ 挂载点前缀黑名单（/run /snap /dev /proc /sys）
    // 双保险；真实分区（/、/boot、/data）必须保留
    let df = "\
Filesystem     1024-blocks      Used  Available Use% Mounted on
/dev/sda1        51475040  29655014   21820026  58% /
tmpfs             1638400      1205   1637195   1% /dev/shm
/dev/sda2          102400     51200     51200  50% /boot
tmpfs             1638400         0   1638400   0% /run/user/1000
/dev/sdb1       103080888  51475040  51475040  50% /data
overlay           1030808  1030808         0 100% /var/lib/docker/overlay2/x/merged
/dev/loop0          65536     65536         0 100% /snap/core22/1380
";
    let mounts = parse_df_mounts(df);
    let got: Vec<&str> = mounts.iter().map(|m| m.mount.as_str()).collect();
    assert_eq!(got, vec!["/", "/boot", "/data"], "伪文件系统行必须被过滤");
    // 数值换算：1K 块 × 1024
    assert_eq!(mounts[0].total, 51475040 * 1024);
    assert_eq!(mounts[2].used, 51475040 * 1024);
}

#[test]
fn test_build_snapshot_df_fallback_to_first_mount_without_root() {
    // df 无 / 挂载点（chroot/容器极端场景）：diskTotal/diskUsed 回退首个真实挂载点
    let df = "Filesystem     1024-blocks      Used  Available Use% Mounted on
/dev/xvda1        100000     40000       60000  40% /mnt/data
";
    let combined = format!(
        "{}
{}
{}
{DISKSTATS_MARKER}
{}
{}
",
        PROC_STAT_SAMPLE, PROC_MEMINFO_SAMPLE, PROC_NET_DEV_SAMPLE, PROC_DISKSTATS_SAMPLE, df
    );
    let snap = build_snapshot("test-session", &combined, None).expect("snapshot");
    assert_eq!(snap.disk_total, 100000 * 1024);
    assert_eq!(snap.disk_used, 40000 * 1024);
    assert_eq!(snap.disks.len(), 1);
    assert_eq!(snap.disks[0].mount, "/mnt/data");
}

#[test]
fn test_parse_df_mounts_caps_at_12_rows() {
    // 异常远端冒出大量挂载点时截断到上限，防 IPC 载荷膨胀
    let mut df = String::from("Filesystem     1024-blocks      Used  Available Use% Mounted on\n");
    for i in 0..20 {
        df.push_str(&format!(
            "/dev/sd{}1        100000     40000       60000  40% /vol{}\n",
            char::from(b'a' + i),
            i
        ));
    }
    assert_eq!(parse_df_mounts(&df).len(), 12);
}

#[test]
fn test_parse_df_mounts_segment_boundary_not_prefix_match() {
    // 挂载点黑名单按路径段判断：/devx、/procfs 不是 /dev、/proc 下的挂载
    let df = "\
Filesystem     1024-blocks      Used  Available Use% Mounted on
/dev/sdz1          100000     40000       60000  40% /devx
/dev/sdy1          100000     40000       60000  40% /procfs
";
    let mounts = parse_df_mounts(df);
    assert_eq!(mounts.len(), 2, "同前缀路径不得被误杀（形态 E）");
}

#[test]
fn test_build_snapshot_populates_disk_capacity() {
    let combined = format!(
        "{}\n{}\n{}\n{DISKSTATS_MARKER}\n{}\n{}\n",
        PROC_STAT_SAMPLE,
        PROC_MEMINFO_SAMPLE,
        PROC_NET_DEV_SAMPLE,
        PROC_DISKSTATS_SAMPLE,
        DF_SAMPLE
    );
    let snap = build_snapshot("test-session", &combined, None).expect("完整样本应构建成功");
    // 全段正常 → 无降级标注
    assert!(
        snap.degraded.is_none(),
        "完整样本不应有 degraded: {:?}",
        snap.degraded
    );
    // disk_total/used 必须来自 df 根分区（KB×1024），而非写死 0
    assert_eq!(snap.disk_total, 51475040 * 1024);
    assert_eq!(snap.disk_used, 29655014 * 1024);
    // 多挂载点明细：DF_SAMPLE 的 / 与 /boot 保留，tmpfs(/dev/shm) 过滤
    let mounts: Vec<&str> = snap.disks.iter().map(|m| m.mount.as_str()).collect();
    assert_eq!(mounts, vec!["/", "/boot"]);
    // 其余字段也应正常填充（serde 字段名是 camelCase，但 struct 字段名仍是 snake）
    assert_eq!(snap.mem_total, 16266984 * 1024);
    assert!(snap.disk_read_bytes > 0);
}

#[test]
fn test_build_snapshot_rejects_non_proc_output() {
    // 非 Linux 远端：cat /proc/* 失败（stderr），stdout 为空或只有 df 段。
    // 必须返回 Err，不能发全 0 假快照（图表恒 0% 伪装成负载健康）。
    let out = "Filesystem     1024-blocks      Used  Available Use% Mounted on\n/dev/sda1        51475040  29655014   21820026  58% /\n";
    let err = build_snapshot("s", out, None).expect_err("无 /proc 输出应 Err");
    assert_eq!(err, "远端无 /proc，资源监控仅支持 Linux");
    // 完全空输出同理
    assert!(build_snapshot("s", "", None).is_err());
}

#[test]
fn test_build_snapshot_df_failure_degrades_not_fakes() {
    // df 段缺失（次要段）→ 快照照发 + degraded 标注，而不是静默 disk=0
    let combined = format!(
        "{}\n{}\n{}\n{DISKSTATS_MARKER}\n{}\n",
        PROC_STAT_SAMPLE, PROC_MEMINFO_SAMPLE, PROC_NET_DEV_SAMPLE, PROC_DISKSTATS_SAMPLE
    );
    let snap = build_snapshot("s", &combined, None).expect("主要段正常应 Ok");
    assert_eq!(snap.degraded.as_deref(), Some("df 解析失败"));
    assert_eq!(snap.disk_total, 0);
    // CPU/内存仍真实
    assert_eq!(snap.mem_total, 16266984 * 1024);
    assert_eq!(snap.cpu_cores, 8);
}

#[test]
fn test_build_snapshot_net_failure_degrades_not_fakes() {
    // net 段缺失 → degraded 标注（此前 parse_proc_net_dev 从不返回 Err，
    // 该分支是死代码，net 缺失时前端看到的是「正常的 0 KB/s」假数据）
    let combined = format!(
        "{}\n{}\n{DISKSTATS_MARKER}\n{}\n",
        PROC_STAT_SAMPLE, PROC_MEMINFO_SAMPLE, PROC_DISKSTATS_SAMPLE
    );
    let snap = build_snapshot("s", &combined, None).expect("主要段正常应 Ok");
    assert_eq!(snap.degraded.as_deref(), Some("net/df 解析失败"));
    assert_eq!(snap.net_rx_bytes, 0);
    assert_eq!(snap.net_tx_bytes, 0);
}

#[test]
fn test_build_snapshot_uses_default_route_interfaces_for_net() {
    // 端到端：route + net/dev 都在 → 网络字节数只计默认路由接口，
    // docker0/veth 的镜像流量不叠加（backlog #1 的用户可见行为）
    let combined = format!(
        "{}\n{}\n{}\n{DISKSTATS_MARKER}\n{}\n{}\n{}\n",
        PROC_STAT_SAMPLE,
        PROC_MEMINFO_SAMPLE,
        PROC_NET_DEV_VIRTUAL_SAMPLE,
        PROC_DISKSTATS_SAMPLE,
        DF_SAMPLE,
        PROC_NET_ROUTE_SAMPLE
    );
    let snap = build_snapshot("s", &combined, None).expect("快照应构建成功");
    assert!(
        snap.degraded.is_none(),
        "全段在场不应降级: {:?}",
        snap.degraded
    );
    assert_eq!(snap.net_rx_bytes, 1000000 + 300000);
    assert_eq!(snap.net_tx_bytes, 2000000 + 300000);
}

#[test]
fn test_parse_proc_meminfo_memfree_fallback() {
    // 老内核无 MemAvailable → 退回 MemFree；两者都缺 → Err
    let old_kernel = "MemTotal:       16266984 kB\nMemFree:          387952 kB\n";
    let (total, used) = parse_proc_meminfo(old_kernel).expect("MemFree 兜底");
    assert_eq!(total, 16266984 * 1024);
    assert_eq!(used, (16266984 - 387952) * 1024);
    assert!(parse_proc_meminfo("MemFree: 1 kB\n").is_err());
    assert!(parse_proc_meminfo("").is_err());
}

#[test]
fn test_resource_snapshot_serializes_camel_case() {
    // 关键回归防护：serde rename_all="camelCase" 必须生效，
    // 否则前端读 cpuUsage/memTotal 全 undefined。
    let snap = ResourceSnapshot {
        session_id: "s1".into(),
        cpu_usage: 42.5,
        cpu_cores: 4,
        mem_total: 1000,
        mem_used: 500,
        net_rx_bytes: 10,
        net_tx_bytes: 20,
        disk_read_bytes: 30,
        disk_write_bytes: 40,
        disk_total: 1000,
        disk_used: 600,
        disks: Vec::new(),
        timestamp: 12345,
        degraded: None,
    };
    let json = serde_json::to_string(&snap).expect("serialize");
    assert!(
        json.contains("\"cpuUsage\""),
        "json must use camelCase: {json}"
    );
    assert!(json.contains("\"memTotal\""));
    assert!(json.contains("\"diskTotal\""));
    assert!(
        !json.contains("\"cpu_usage\""),
        "snake_case must not leak: {json}"
    );
    // degraded = None 时不出现（skip_serializing_if）
    assert!(!json.contains("degraded"), "None 不应序列化: {json}");
    // disks 空 Vec 不出现（skip_serializing_if）
    assert!(!json.contains("disks"), "空 disks 不应序列化: {json}");
    // degraded = Some 时必须以 camelCase 字段名序列化（前端契约字段）
    let snap_degraded = ResourceSnapshot {
        degraded: Some("df 解析失败".into()),
        ..snap
    };
    let json = serde_json::to_string(&snap_degraded).expect("serialize");
    assert!(
        json.contains("\"degraded\":\"df 解析失败\""),
        "degraded 应序列化: {json}"
    );
}
