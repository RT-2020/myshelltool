//! 端口覆盖环境变量的三态解析（`MYSHELLTOOL_MCP_PORT`）。
//!
//! 为什么放 core：src-tauri 的测试二进制受 Tauri runtime DLL 限制跑不起来
//! （AGENTS.md 已知边界），解析判据要在 `npm run test:core` 里真跑——与
//! dangerous_commands / shell 分段器迁入 core 同一纪律。
//!
//! 三态口径对齐 `lib.rs::mcp_data_dir` 对 `MYSHELLTOOL_DATA_DIR` 的校验纪律：
//! **区分「未设置」与「设置了但不可用」**。空串/纯空白/非数字/0/超范围都是
//! 「用户显式配置了但值不可用」，调用方必须 warn 让用户看见后回退默认，
//! 不得静默折叠成「未配置」——否则用户以为端口改了，实际监听还在老端口上。

use std::ffi::OsStr;

/// 端口覆盖的解析结果（三态，供调用方区分 warn 文案与默认回退）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortOverride {
    /// 环境变量未设置——用调用方自己的默认值
    NotSet,
    /// 设置了但不是可用端口（回传原值字符串，供日志展示；非 UTF-8 值经
    /// lossy 替换为 U+FFFD，同样落这里而不是被当成「未设置」丢弃）
    Invalid(String),
    /// 合法端口（1-65535）
    Port(u16),
}

/// 解析端口覆盖值。
///
/// - `None`（变量未设置）→ `NotSet`
/// - 空串/纯空白 → `Invalid`（空值不算「未配置」）
/// - trim 后可解析为 1-65535 整数 → `Port`（前后空白容忍，与
///   `MYSHELLTOOL_DATA_DIR` 的 trim 口径一致）
/// - 其余（非数字 / `0` / 超出 u16 / 内嵌空白 / 负号）→ `Invalid`。
///   `0` 在 u16 解析上合法，但监听 0 = 内核随机分配端口，不是用户
///   「指定端口」的意图，按无效处理。
pub fn parse_port(raw: Option<&OsStr>) -> PortOverride {
    let Some(raw) = raw else {
        return PortOverride::NotSet;
    };
    let text = raw.to_string_lossy();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return PortOverride::Invalid(text.into_owned());
    }
    match trimmed.parse::<u16>() {
        Ok(p) if p != 0 => PortOverride::Port(p),
        _ => PortOverride::Invalid(text.into_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_set_when_env_missing() {
        assert_eq!(parse_port(None), PortOverride::NotSet);
    }

    #[test]
    fn valid_port_parsed_with_surrounding_whitespace_tolerated() {
        assert_eq!(parse_port(Some(OsStr::new("41500"))), PortOverride::Port(41500));
        assert_eq!(parse_port(Some(OsStr::new(" 41235 "))), PortOverride::Port(41235));
        // 边界值：1 与 65535 均为合法端口
        assert_eq!(parse_port(Some(OsStr::new("1"))), PortOverride::Port(1));
        assert_eq!(
            parse_port(Some(OsStr::new("65535"))),
            PortOverride::Port(65535)
        );
    }

    #[test]
    fn blank_is_invalid_not_unset() {
        // 空值不算「未配置」：调用方要 warn 让用户看见，而不是静默用默认
        assert_eq!(
            parse_port(Some(OsStr::new(""))),
            PortOverride::Invalid(String::new())
        );
        assert_eq!(
            parse_port(Some(OsStr::new("   "))),
            PortOverride::Invalid("   ".to_string())
        );
    }

    #[test]
    fn zero_is_invalid() {
        assert_eq!(
            parse_port(Some(OsStr::new("0"))),
            PortOverride::Invalid("0".to_string())
        );
    }

    #[test]
    fn garbage_overflow_and_embedded_whitespace_are_invalid() {
        for bad in ["abc", "65536", "-1", "41 500", "41.5"] {
            assert_eq!(
                parse_port(Some(OsStr::new(bad))),
                PortOverride::Invalid(bad.to_string()),
                "expected {bad:?} to be Invalid"
            );
        }
    }
}
