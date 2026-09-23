//! MCP C3 只读工具四件（v0.20，3-6 月段：journal_query/port_listen/
//! process_list/file_search——跨发行版口径固化在服务端，各函数头注释即口径）。
//! 自 tools.rs 拆出（tools.rs 贴 800 行 Rust 硬限）。

use rmcp::model::CallToolResult;
use serde_json::Map;

use super::tools::{McpToolContext, exec_on_asset};

/// 内联错误结果（tools.rs 的 error_result 是私有——此处四件共用一个本地副本）。
fn error_result(message: &str) -> CallToolResult {
    let mut result = CallToolResult::success(vec![rmcp::model::Content::text(message.to_string())]);
    result.is_error = Some(true);
    result
}

/// 值禁止单引号/反斜杠（构造时校验后拼入 shell 单引号字面量）。
pub(crate) async fn tool_journal_query(
    ctx: &McpToolContext,
    args: &Map<String, serde_json::Value>,
) -> Result<CallToolResult, String> {
    // 参数存在性校验（exec_on_asset 内部自取同名参数）
    args.get("asset_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("缺少 asset_id 参数")?;
    let unit = args.get("unit").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
    let since = args.get("since").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
    let until = args.get("until").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
    let grep = args.get("grep").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(100).min(2000);

    if let Some(u) = unit {
        if !u.chars().all(|c| c.is_alphanumeric() || "-_@:.".contains(c)) {
            return Ok(error_result("unit 参数含非法字符"));
        }
    }
    for (label, v) in [("since", since), ("until", until), ("grep", grep)] {
        if let Some(val) = v {
            if val.contains('\'') || val.contains('\\') {
                return Ok(error_result(&format!("{label} 参数不允许包含单引号或反斜杠")));
            }
        }
    }

    let mut cmd = String::from(
        "command -v journalctl >/dev/null 2>&1 || { echo 'journalctl not found (non-systemd system?) — try /var/log via ssh_exec'; exit 127; }; \
         env LC_ALL=C journalctl --no-pager -o short-iso -n ",
    );
    cmd.push_str(&limit.to_string());
    if let Some(u) = unit {
        cmd.push_str(&format!(" -u {u}"));
    }
    if let Some(sv) = since {
        cmd.push_str(&format!(" --since '{}'", sv));
    }
    if let Some(un) = until {
        cmd.push_str(&format!(" --until '{}'", un));
    }
    if let Some(g) = grep {
        cmd.push_str(&format!(" | env LC_ALL=C grep -F -- '{}'", g));
    }
    exec_on_asset(ctx, args, &cmd).await
}

/// 注入防护：无自由文本参数（仅布尔 tcp_only），模板完全固定。
pub(crate) async fn tool_port_listen(
    ctx: &McpToolContext,
    args: &Map<String, serde_json::Value>,
) -> Result<CallToolResult, String> {
    args.get("asset_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("缺少 asset_id 参数")?;
    let tcp_only = args.get("tcpOnly").and_then(|v| v.as_bool()).unwrap_or(false);
    let proto_flag = if tcp_only { "t" } else { "tun" };

    // 三级降级链：ss 不可用（127）→ netstat；两者皆无 → lsof；全无 → 明确报错。
    // rc 语义：整链最后的 echo rc_chain=$? 反映实际成功的那一段。
    let cmd = format!(
        "if command -v ss >/dev/null 2>&1; then \
           echo '[ss]'; env LC_ALL=C ss -{proto_flag}lp; \
         elif command -v netstat >/dev/null 2>&1; then \
           echo '[netstat]'; env LC_ALL=C netstat -{proto_flag}p 2>/dev/null || env LC_ALL=C netstat -{proto_flag}; \
         elif command -v lsof >/dev/null 2>&1; then \
           echo '[lsof]'; env LC_ALL=C lsof -nP -i{proto_lsof} | env LC_ALL=C grep LISTEN; \
         else \
           echo 'no ss/netstat/lsof available'; exit 127; \
         fi; echo rc_chain=$?",
        proto_lsof = if tcp_only { "T" } else { "" },
    );
    exec_on_asset(ctx, args, &cmd).await
}

/// 注入防护：sortBy 枚举白名单（cpu/mem）、limit 数字——无自由文本。
/// 三层降级：procps --no-headers → procps 带表头（老 procps）→ busybox 列集
/// （无 %cpu/%mem 列，sortBy 一律按 RSS 降序并注明——Alpine 等真机验收发现
/// busybox 对 -o %cpu 整条失败导致空输出+rc=0 伪装成功，v0.20 修复）。
pub(crate) async fn tool_process_list(
    ctx: &McpToolContext,
    args: &Map<String, serde_json::Value>,
) -> Result<CallToolResult, String> {
    args.get("asset_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("缺少 asset_id 参数")?;
    let sort_by = match args.get("sortBy").and_then(|v| v.as_str()) {
        Some("cpu") => "4",        // %cpu 列
        Some("mem") | None => "5", // %mem 列（默认）
        Some(other) => {
            return Ok(error_result(&format!(
                "sortBy 仅支持 cpu / mem（收到 {other:?}）"
            )))
        }
    };
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20).min(200);

    let cmd = format!(
        "__t=$(mktemp 2>/dev/null || echo /tmp/.myshelltool-ps.$$); \
         if env LC_ALL=C ps -eo pid,ppid,user,%cpu,%mem,rss,stat,etime,comm --no-headers >$__t 2>/dev/null && grep -q . $__t; then \
           env LC_ALL=C sort -k{sort_by} -nr <$__t | head -{limit}; \
         elif env LC_ALL=C ps -eo pid,ppid,user,%cpu,%mem,rss,stat,etime,comm >$__t 2>/dev/null && grep -q . $__t; then \
           env LC_ALL=C sort -k{sort_by} -nr <$__t | head -{limit}; \
         elif env LC_ALL=C ps -eo pid,ppid,user,rss,stat,comm >$__t 2>/dev/null && grep -q . $__t; then \
           echo '# busybox ps 无 %cpu/%mem 列：sortBy 已按 RSS(kB) 降序替代'; \
           env LC_ALL=C sort -k4 -nr <$__t | head -{limit}; \
         else \
           echo 'process_list: 所有 ps 形态均失败（procps 与 busybox 列集都不可用）'; false; \
         fi; rc=$?; rm -f $__t 2>/dev/null; echo rc_chain=$rc"
    );
    exec_on_asset(ctx, args, &cmd).await
}

/// `-printf` 排序，故非 name 排序时退化为无排序 + 提示 AI 收窄）。
pub(crate) async fn tool_file_search(
    ctx: &McpToolContext,
    args: &Map<String, serde_json::Value>,
) -> Result<CallToolResult, String> {
    args.get("asset_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("缺少 asset_id 参数")?;
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("缺少 path 参数（起始目录，如 /var/log）")?;
    let name = args.get("name").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
    let min_size = args.get("minSizeMb").and_then(|v| v.as_u64());
    let mtime_days = args.get("mtimeDays").and_then(|v| v.as_i64());
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(100).min(1000);

    // path/name 校验：禁单引号/反斜杠（拼入单引号字面量）；path 额外禁分号与空格
    for (label, val, strict) in [("path", Some(path), true), ("name", name, false)] {
        if let Some(v) = val {
            if v.contains('\'') || v.contains('\\') {
                return Ok(error_result(&format!("{label} 参数不允许包含单引号或反斜杠")));
            }
            if strict && (v.contains(';') || v.contains(' ')) {
                return Ok(error_result(&format!(
                    "{label} 参数不允许包含分号或空格（含空格子目录请逐层进入）"
                )));
            }
        }
    }

    // __t 先行声明（mktemp 失败退化为 /tmp 专名文件——同 CMD_SYSTEM_STATUS 的 top 段口径）
    let mut cmd = format!(
        "__t=$(mktemp 2>/dev/null || echo /tmp/.myshelltool-find.$$); env LC_ALL=C find '{}' ",
        path.trim_end_matches('/')
    );
    if let Some(n) = name {
        cmd.push_str(&format!("-name '{}' ", n));
    }
    if let Some(mb) = min_size {
        cmd.push_str(&format!("-size +{}M ", mb));
    }
    if let Some(days) = mtime_days {
        // 负值 = 最近 N 天内修改（-mtime -N）；正值 = 超过 N 天未改（-mtime +N）
        if days < 0 {
            cmd.push_str(&format!("-mtime -{} ", -days));
        } else {
            cmd.push_str(&format!("-mtime +{} ", days));
        }
    }
    // find 先落临时文件再截断（管道后的 rc 恒为 head 的——no-pipeline-rc-echo
    // 事故口径；落文件让 rc_find 如实反映 find 本身，权限错误/起始目录不存在
    // 才不会被 head 的 0 掩盖）
    cmd.push_str(&format!(
        ">\"$__t\" 2>/dev/null; __r=$?; echo rc_find=$__r; head -{limit} \"$__t\"; rm -f \"$__t\""
    ));
    exec_on_asset(ctx, args, &cmd).await
}
