//! MCP 危险命令拦截等级配置（v2）。
//!
//! 持久化在 `<data_dir>/mcp-config.json`，GUI 经 mcp_get_config / mcp_set_config
//! 读写。运行时 AppState 与 McpToolContext 共享同一份 `Arc<RwLock<McpConfig>>`，
//! mcp_set_config 更新后已建立的 HTTP 会话**下次工具调用即生效**（每次 call_tool
//! 现读快照，不在会话建立时固化）。
//!
//! 等级语义（用户可配置的产品决策，不是安全兜底）：
//! - `Minimal`（默认）：仅黑名单（超高危，dangerous_commands.rs 16 条正则）
//!   需要用户确认；Unknown / 黄名单命令直接放行。**这是用户明确选择的低摩擦
//!   默认档**——不做 fail-secure 默认拒，放行行为记入执行日志（minimal_allowed）
//!   供事后审计。
//! - `Strict`：非白名单一律确认（Unknown / 黄名单也走 elicitation / GUI 弹窗），
//!   即 v1.x 的原 fail-secure 语义。黑名单在两档下均恒拦截。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 危险命令拦截等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpInterceptLevel {
    /// 仅拦截超高危（黑名单需确认，Unknown/黄名单放行）。默认档。
    Minimal,
    /// 全部需确认（非白名单一律 elicitation/GUI 审批，原 fail-secure 语义）。
    Strict,
}

impl McpInterceptLevel {
    /// 序列化名（"minimal" / "strict"，与 serde camelCase 一致；执行日志快照也用此值）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Strict => "strict",
        }
    }

    /// 解析 mcp_set_config 传入的 level 字符串。无效值返回 None（命令层转 Err）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "minimal" => Some(Self::Minimal),
            "strict" => Some(Self::Strict),
            _ => None,
        }
    }
}

/// MCP 配置（目前只有拦截等级一个字段，后续扩展在此追加）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpConfig {
    pub level: McpInterceptLevel,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            level: McpInterceptLevel::Minimal,
        }
    }
}

/// mcp-config.json 路径。data_dir 由调用方传入（GUI 侧统一从 mcp_data_dir() 取，
/// 与 mcp-endpoint.json / mcp-execution-log.json 同目录）。
pub fn mcp_config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("mcp-config.json")
}

/// 读配置。文件不存在或损坏（含非法 level 值）→ 返回默认值（Minimal），
/// 不向上抛错——配置损坏不应阻断 MCP server 启动。
pub fn load_mcp_config(path: &Path) -> McpConfig {
    if !path.exists() {
        return McpConfig::default();
    }
    match std::fs::read_to_string(path) {
        Ok(json) if !json.trim().is_empty() => {
            serde_json::from_str(&json).unwrap_or_default()
        }
        _ => McpConfig::default(),
    }
}

/// 写配置（serde_json pretty）。简单整文件写——配置文件极小且写入频率极低
/// （用户手动切换等级），无原子性诉求（与执行日志的高频 append 不同）。
pub fn save_mcp_config(path: &Path, config: &McpConfig) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(config).map_err(|e| format!("序列化 mcp-config.json: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("写 mcp-config.json: {e}"))
}
