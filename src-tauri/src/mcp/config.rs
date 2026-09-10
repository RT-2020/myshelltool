//! MCP 危险命令拦截等级配置（v2）。
//!
//! 持久化在 `<data_dir>/mcp-config.json`，GUI 经 mcp_get_config / mcp_set_config
//! 读写。运行时 AppState 与 McpToolContext 共享同一份 `Arc<RwLock<McpConfig>>`，
//! mcp_set_config 更新后已建立的 HTTP 会话**下次工具调用即生效**（每次 call_tool
//! 现读快照，不在会话建立时固化）。
//!
//! 等级语义（用户可配置的产品决策，不是安全兜底；v2.1 语义重构）：
//! - `Minimal`（默认）：仅硬拦毁灭性（catastrophic，dangerous_commands.rs
//!   的 detect_catastrophic_command：mkfs / dd 写块设备 / rm 根级删除等）
//!   命令；其余（含**非毁灭黑名单**如 rm -rf 子路径、reboot）直接放行，
//!   记执行日志（minimal_allowed）供事后审计。**这是用户明确选择的零审批
//!   交互默认档**——不做 fail-secure 默认拒。
//! - `Strict`：非白名单一律审批（Unknown / 黄名单 / 非毁灭黑名单均走
//!   elicitation / GUI 弹窗），即 v1.x 的原 fail-secure 语义。
//! 毁灭性命令在两档下均 HardBlock 直接拒绝（不弹窗不等超时）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 危险命令拦截等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpInterceptLevel {
    /// 仅硬拦毁灭性（catastrophic）命令，其余（含非毁灭黑名单）直接放行记
    /// minimal_allowed 日志。默认档，零审批交互。
    Minimal,
    /// 非白名单一律审批（elicitation/GUI 弹窗），毁灭性命令同样硬拦。
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

impl McpConfig {
    /// 保守档（Strict）：配置**不可信**时的取值。
    ///
    /// 与 `Default::default()`（Minimal）语义刻意分离：Minimal 是「文件不存在 =
    /// 用户从未表达过偏好」的产品默认值，属应用自身契约；而 mcp-config.json
    /// 存在却读不出来/解析不了，说明用户**表达过**偏好但我们读不到它——
    /// 此时回落到零审批的 Minimal 等于替用户把「非白名单一律确认」静默降级为
    /// 「直接执行」，是 fail-open（形态 C 静默兜底）。不可信时取更严的一档，
    /// 用户若要宽松档可在面板里重新选择（那时配置已被重写为可读）。
    pub fn strict() -> Self {
        Self {
            level: McpInterceptLevel::Strict,
        }
    }
}

/// mcp-config.json 路径。data_dir 由调用方传入（GUI 侧统一从 mcp_data_dir() 取，
/// 与 mcp-endpoint.json / mcp-execution-log.json 同目录）。
pub fn mcp_config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("mcp-config.json")
}

/// 读配置。三态分明，**读不出来绝不静默降级到宽松档**：
/// - 文件不存在 → 默认值 Minimal（用户从未表达过偏好，产品默认档，非猜测）；
/// - 文件存在且合法 → 文件里的档位。**带 UTF-8 BOM 也算合法**：BOM 是编码标记
///   而非内容，读入后先剥 `\u{FEFF}` 再解析（Windows 上 PowerShell 5.1
///   `Set-Content -Encoding utf8` 与老编辑器默认写 BOM，不剥就会把一份合法配置
///   误判成损坏 → 隔离 + 保守档）；
/// - 文件存在但读失败 / 空内容 / JSON 非法 / level 非法 → `McpConfig::strict()`
///   + log::error + 改名隔离为 `mcp-config.corrupt-<unix秒>.json`（保留证据）。
///
/// 不向上抛错（配置损坏不应阻断 MCP server 启动），但**降级方向必须是更严的
/// 一档**：曾对全部失败分支 `unwrap_or_default()`（= Minimal），用户显式设的
/// Strict 会因一次断电/半截写/未知 level 名静默变成零审批放行（形态 C 事故）。
pub fn load_mcp_config(path: &Path) -> McpConfig {
    let json = match std::fs::read_to_string(path) {
        Ok(json) => json,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // 首次运行：无偏好文件，取产品默认档
            return McpConfig::default();
        }
        Err(e) => return unusable_config(path, &format!("读取失败: {e}")),
    };

    // 剥 UTF-8 BOM：`str::trim()` 不处理 U+FEFF（它不是 Rust 的空白字符），
    // 而 serde_json 会把 BOM 当成非法首字符 → 一份**合法**配置被判「损坏」并隔离。
    // 只剥前缀（BOM 只允许出现在文件开头），其余内容原样交给解析器。
    let json = json.strip_prefix('\u{feff}').unwrap_or(&json);

    if json.trim().is_empty() {
        return unusable_config(path, "文件为空（写入被截断？）");
    }

    match serde_json::from_str::<McpConfig>(&json) {
        Ok(config) => config,
        Err(e) => unusable_config(path, &format!("解析失败: {e}")),
    }
}

/// 配置不可信时的统一处置：log::error + 改名隔离 + 返回保守档。
///
/// 隔离而非删除：损坏文件是排查依据（人为编辑错格式 / 磁盘问题 / 旧版本写坏），
/// 改名后下次 save 会写出全新的合法文件。改名失败也不阻断（原文件留原地）。
fn unusable_config(path: &Path, reason: &str) -> McpConfig {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let quarantine = path.with_file_name(format!(
        "{}.corrupt-{secs}.json",
        path.file_stem().and_then(|s| s.to_str()).unwrap_or("mcp-config")
    ));
    match std::fs::rename(path, &quarantine) {
        Ok(()) => log::error!(
            "mcp-config.json 不可用（{reason}），已隔离到 {}；本次运行取保守档 Strict（非白名单命令一律人工确认），请在 MCP 面板重新选择拦截等级",
            quarantine.display()
        ),
        Err(rename_err) => log::error!(
            "mcp-config.json 不可用（{reason}），隔离改名失败: {rename_err}；本次运行取保守档 Strict，请在 MCP 面板重新选择拦截等级"
        ),
    }
    McpConfig::strict()
}

/// 写配置（serde_json pretty）。配置极小、写入频率极低，但仍走**原子替换**
/// （同目录临时文件 + rename，实现收敛在 `myshelltool_core::write_atomic`）：
/// 非原子 `fs::write` 在断电/进程被杀/AV 扫描瞬间会留下 0 字节或半截 JSON，而读侧
/// 一旦降级即影响拦截档位——写坏自己的配置正是「不可信 → Strict」要防的场景，
/// 写入侧不该制造它。
pub fn save_mcp_config(path: &Path, config: &McpConfig) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(config).map_err(|e| format!("序列化 mcp-config.json: {e}"))?;
    myshelltool_core::write_atomic(path, json).map_err(|e| format!("写 mcp-config.json: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 临时目录（无 tempfile 依赖）：测试名 + 进程 id 隔离，避免并发互相踩。
    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("myshelltool-cfg-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn missing_file_falls_back_to_product_default_minimal() {
        let dir = temp_dir("missing");
        let cfg = load_mcp_config(&dir.join("mcp-config.json"));
        assert_eq!(cfg.level, McpInterceptLevel::Minimal);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn valid_file_wins_over_default() {
        let dir = temp_dir("valid");
        let path = dir.join("mcp-config.json");
        std::fs::write(&path, r#"{"level":"strict"}"#).unwrap();
        assert_eq!(load_mcp_config(&path).level, McpInterceptLevel::Strict);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn utf8_bom_file_is_parsed_not_quarantined() {
        let dir = temp_dir("bom");
        let path = dir.join("mcp-config.json");
        // PowerShell 5.1 `Set-Content -Encoding utf8` / 老编辑器写出的合法 JSON 带 BOM
        std::fs::write(&path, "\u{feff}{\"level\":\"minimal\"}").unwrap();
        assert_eq!(load_mcp_config(&path).level, McpInterceptLevel::Minimal);
        // 合法配置不得被判损坏：原文件仍在，且没有产生隔离副本
        assert!(path.exists(), "带 BOM 的合法配置被改名隔离了");
        let quarantined = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains(".corrupt-"));
        assert!(!quarantined, "带 BOM 的合法配置不应触发隔离改名");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bom_does_not_make_corrupt_json_trusted() {
        let dir = temp_dir("bom-corrupt");
        let path = dir.join("mcp-config.json");
        // 剥 BOM 只去掉编码标记，不会把损坏内容「修好」
        std::fs::write(&path, "\u{feff}{\"level\":\"str").unwrap();
        assert_eq!(load_mcp_config(&path).level, McpInterceptLevel::Strict);
        assert!(!path.exists(), "带 BOM 的损坏配置同样要隔离保留证据");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_never_silently_degrades_to_minimal() {
        let dir = temp_dir("corrupt");
        let path = dir.join("mcp-config.json");
        // 半截写入形态
        std::fs::write(&path, r#"{"level":"str"#).unwrap();
        assert_eq!(load_mcp_config(&path).level, McpInterceptLevel::Strict);
        // 损坏文件已隔离保留（证据不丢），原路径不再存在
        assert!(!path.exists());
        let quarantined = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains(".corrupt-"));
        assert!(quarantined, "损坏配置必须改名隔离");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_level_value_is_not_treated_as_trusted() {
        let dir = temp_dir("unknown-level");
        let path = dir.join("mcp-config.json");
        // 手改成未知档位（例如旧版本/拼错）→ 不可信 → 保守档，而不是默认宽松
        std::fs::write(&path, r#"{"level":"Loose"}"#).unwrap();
        assert_eq!(load_mcp_config(&path).level, McpInterceptLevel::Strict);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_file_is_treated_as_corrupt_not_as_unset() {
        let dir = temp_dir("empty");
        let path = dir.join("mcp-config.json");
        std::fs::write(&path, "").unwrap();
        assert_eq!(load_mcp_config(&path).level, McpInterceptLevel::Strict);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_roundtrip_is_atomic() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("mcp-config.json");
        save_mcp_config(&path, &McpConfig::strict()).unwrap();
        assert_eq!(load_mcp_config(&path).level, McpInterceptLevel::Strict);
        // 原子写不留 tmp 残留
        assert!(!dir.join("mcp-config.json.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
