//! MCP 授权范围（scope）判定内核（MCP服务设计-v3 §3.2，阶段 B1）。
//!
//! ## 为什么在 core
//!
//! `level` 回答「这个动作要不要确认」；scope 回答「**这台机器允不允许碰**」——
//! 运维不会把生产库钥匙交给 AI，此前只有全给或全不给。判定是安全判据
//! （涉及审批/授权的拒绝语义），按项目纪律放 core 真跑单测（src-tauri
//! 测试二进制本机跑不起来）；config.rs / tools.rs 只做 IO 与收口接线。
//!
//! ## 判定语义（红线：形态 E——不靠形状猜测）
//!
//! - 分组匹配**必须按路径组件**比较：`allowed_groups = ["生产"]` 不得匹配
//!   `生产测试`（字符串前缀会撞出「生产测试」这种前缀子串组）。`asset.group`
//!   本就是 `/` 分隔的多级路径，按组件逐级比对，**前缀层级即子树授权**
//!   （`["生产"]` 授权 `生产/数据库`——运维语义：授权整个生产组）。
//! - 优先级：`deny_all` > `denied_asset_ids` > (`allowed_groups` ∪ `allowed_tags`
//!   并集；两者都空 = 不限制) > 默认拒绝（配置了 allow 列表但资产既不在组
//!   也不带标签 → 拒）。
//! - 默认（无任何配置）= 全部允许：scope 是**收紧**维度，用户没表达偏好时
//!   不替用户拒绝（与 McpConfig::default 的产品哲学一致；fail-closed 只用于
//!   「配置存在但不可信」——由 config.rs 在读盘层置 deny_all）。

use serde::{Deserialize, Serialize};

use crate::asset_store::ConnectionAsset;

/// 授权范围配置（mcp-config.json 的 scope 字段）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpScope {
    /// 允许访问的资产分组前缀（按组件匹配，父组授权含子组）；空 = 不按分组限制。
    #[serde(default)]
    pub allowed_groups: Vec<String>,
    /// 允许访问的标签（资产 tags 命中任一即可）；空 = 不按标签限制。
    #[serde(default)]
    pub allowed_tags: Vec<String>,
    /// 显式拒绝的资产 id（优先级最高）。
    #[serde(default)]
    pub denied_asset_ids: Vec<String>,
    /// 是否允许 sftp_upload / sftp_download 触及本机文件系统。
    #[serde(default)]
    pub allow_local_fs: bool,
    /// 配置不可信时置 true：拒绝一切资产访问（fail-closed 收敛态）。
    #[serde(default)]
    pub deny_all: bool,
}

impl McpScope {
    /// 不限制任何访问的空 scope（= 默认全允许）。
    pub fn unrestricted() -> Self {
        Self::default()
    }

    /// 收敛态：拒绝一切（配置不可信时由读盘层构造，不让默认值悄悄放行）。
    pub fn deny_all() -> Self {
        Self {
            deny_all: true,
            ..Self::default()
        }
    }

    /// 是否配置了任何资产维度的限制（全空 = 未启用 scope，一切照旧）。
    /// 注意**不含** allow_local_fs：本机 FS 闸门只在 scope 已配置（is_restricted）
    /// 时才生效——完全未配置的老用户不受影响；用户一旦开始限制资产访问，
    /// 本机文件触及默认也收紧（需显式 allow_local_fs=true 打开）。
    pub fn is_restricted(&self) -> bool {
        self.deny_all
            || !self.denied_asset_ids.is_empty()
            || !self.allowed_groups.is_empty()
            || !self.allowed_tags.is_empty()
    }
}

/// 判定结果（执行日志 scopeResult 字段值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeVerdict {
    /// 允许（未启用限制 / 命中组或标签）。
    Allowed,
    /// deny_all 收敛态。
    DeniedAll,
    /// 命中显式拒绝的资产 id。
    DeniedId,
    /// 配置了 allow 列表但资产既不在授权组也不带授权标签。
    DeniedNoMatch,
    /// 本机 FS 触及未被授权（allow_local_fs=false）。
    DeniedLocalFs,
}

impl ScopeVerdict {
    /// 执行日志 scopeResult 字段值。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::DeniedAll => "deny_all",
            Self::DeniedId => "denied_id",
            Self::DeniedNoMatch => "denied_no_match",
            Self::DeniedLocalFs => "denied_local_fs",
        }
    }

    pub fn is_allowed(self) -> bool {
        self == Self::Allowed
    }
}

/// 判定某资产是否可访问。
///
/// 优先级：deny_all > denied_id > (groups ∪ tags 并集，两者都空 = 不限制) > 默认拒绝。
pub fn evaluate(scope: &McpScope, asset: &ConnectionAsset) -> ScopeVerdict {
    if scope.deny_all {
        return ScopeVerdict::DeniedAll;
    }
    if scope.denied_asset_ids.iter().any(|id| id == &asset.id) {
        return ScopeVerdict::DeniedId;
    }
    let groups_empty = scope.allowed_groups.is_empty();
    let tags_empty = scope.allowed_tags.is_empty();
    if groups_empty && tags_empty {
        // 未按组/标签限制（denied 列表已在上面处理）
        return ScopeVerdict::Allowed;
    }
    let group_hit = !groups_empty
        && scope
            .allowed_groups
            .iter()
            .any(|g| group_matches(g, &asset.group));
    let tag_hit = !tags_empty && asset.tags.iter().any(|t| scope.allowed_tags.contains(t));
    if group_hit || tag_hit {
        ScopeVerdict::Allowed
    } else {
        ScopeVerdict::DeniedNoMatch
    }
}

/// 本机文件系统触及判定（sftp_upload 读本机 / sftp_download 写本机）。
///
/// 语义（向后兼容的关键）：scope 完全未配置 = 不收紧（Allowed）；一旦配置了
/// 任何资产维度限制（is_restricted），本机 FS 默认收紧、需显式
/// `allow_local_fs: true` 打开——用户开始圈资产范围时，本机文件触及不应静默保持敞开。
pub fn evaluate_local_fs(scope: &McpScope) -> ScopeVerdict {
    if scope.deny_all {
        // deny_all 收敛态对一切工具生效（含本机 FS 触及）
        ScopeVerdict::DeniedAll
    } else if !scope.is_restricted() {
        // 未启用 scope：老用户零配置 = 行为与 B1 之前完全一致
        ScopeVerdict::Allowed
    } else if scope.allow_local_fs {
        ScopeVerdict::Allowed
    } else {
        ScopeVerdict::DeniedLocalFs
    }
}

/// 拒绝文案（AI host 直接可见）：明确说「范围未授权」而非泛化「失败」——
/// AI 需要知道这是**配置边界**（换资产或让用户改 scope），不是可重试的故障。
pub fn denied_message(verdict: ScopeVerdict) -> String {
    match verdict {
        ScopeVerdict::DeniedAll => {
            "访问被拒绝：MCP 配置不可信（deny_all 收敛态），所有资产访问均已禁止。请让用户在 myshelltool 的 MCP 面板重新配置授权范围后重试。".to_string()
        }
        ScopeVerdict::DeniedId => {
            "访问被拒绝：该资产被 MCP 授权范围显式排除（denied asset）。请换用 list_assets 查看可访问的资产。".to_string()
        }
        ScopeVerdict::DeniedNoMatch => {
            "访问被拒绝：该资产不在 MCP 授权范围内（不在允许的分组/标签内）。请换用 list_assets 查看可访问的资产，或让用户在 MCP 面板调整授权范围。".to_string()
        }
        ScopeVerdict::DeniedLocalFs => {
            "访问被拒绝：MCP 授权范围未开放本机文件系统触及（allowLocalFs=false）。上传/下载需要让用户在 MCP 面板显式开放。".to_string()
        }
        ScopeVerdict::Allowed => String::new(),
    }
}

/// 分组前缀匹配（按路径组件）：`allowed="生产"` 匹配 `生产` 与 `生产/数据库`，
/// **不匹配** `生产测试`（防形状猜测/前缀子串碰撞）。空 allowed 组串不匹配任何
/// 分组（显式空串是配置噪音，按不命中处理，不 panic 不放行）。
pub fn group_matches(allowed_group: &str, asset_group: &str) -> bool {
    if allowed_group.is_empty() {
        return false;
    }
    let allowed: Vec<&str> = allowed_group.split('/').collect();
    let actual: Vec<&str> = asset_group.split('/').collect();
    if actual.len() < allowed.len() {
        return false;
    }
    allowed
        .iter()
        .zip(actual.iter())
        .all(|(a, b)| component_eq(a, b))
}

/// 单个组件比较：去首尾空白后全等（分组名含 `/` 只能作为层级分隔符，这是
/// 资产分组模型的自有约定）。不做大小写归一——分组名是用户定义的标识符，
/// 静默归一会让「Prod」与「prod」两个不同组被判同组。
fn component_eq(a: &str, b: &str) -> bool {
    a.trim() == b.trim() && !a.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_store::{AuthMethod, ConnectionStatus};

    fn asset(id: &str, group: &str, tags: &[&str]) -> ConnectionAsset {
        ConnectionAsset {
            id: id.to_string(),
            name: format!("资产{id}"),
            host: "10.0.0.1".to_string(),
            port: 22,
            username: "root".to_string(),
            auth_method: AuthMethod::Password,
            private_key_path: None,
            group: group.to_string(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            status: ConnectionStatus::Idle,
            last_connected: String::new(),
            credential_id: None,
            passphrase_credential_id: None,
            private_key_credential_id: None,
            jump_host: None,
            connect_timeout_secs: None,
            keepalive_interval_secs: None,
        }
    }

    // ── 组件级分组匹配（B1 验收①）──
    #[test]
    fn group_prefix_collision_is_rejected() {
        // 「生产」不得匹配「生产测试」——验收标准的正例
        assert!(!group_matches("生产", "生产测试"));
        assert!(!group_matches("prod", "production"));
    }

    #[test]
    fn group_prefix_subtree_is_authorized() {
        // 父组授权含子组（运维语义：授权整个生产组）
        assert!(group_matches("生产", "生产"));
        assert!(group_matches("生产", "生产/数据库"));
        assert!(group_matches("生产/数据库", "生产/数据库/主库"));
    }

    #[test]
    fn group_partial_depth_is_not_authorized() {
        // 深前缀不匹配浅分组（授权了 生产/数据库，不能访问 生产 顶层的其他资产）
        assert!(!group_matches("生产/数据库", "生产"));
        assert!(!group_matches("生产/数据库", "生产/缓存"));
    }

    #[test]
    fn group_empty_component_never_matches() {
        assert!(!group_matches("", "生产"));
        assert!(!group_matches("生产//x", "生产//x")); // 空组件按不匹配处理（配置噪音）
    }

    // ── 判定优先级 ──
    #[test]
    fn deny_all_beats_everything() {
        let scope = McpScope {
            deny_all: true,
            allowed_groups: vec!["生产".to_string()],
            ..McpScope::default()
        };
        assert_eq!(evaluate(&scope, &asset("a1", "生产", &[])), ScopeVerdict::DeniedAll);
    }

    #[test]
    fn denied_id_beats_allow_lists() {
        let scope = McpScope {
            denied_asset_ids: vec!["prod-1".to_string()],
            allowed_groups: vec!["生产".to_string()],
            ..McpScope::default()
        };
        // 资产在授权组里但被显式拒绝 → 拒
        assert_eq!(evaluate(&scope, &asset("prod-1", "生产", &[])), ScopeVerdict::DeniedId);
        // 同组其他资产不受影响
        assert_eq!(evaluate(&scope, &asset("prod-2", "生产", &[])), ScopeVerdict::Allowed);
    }

    #[test]
    fn groups_and_tags_are_union() {
        let scope = McpScope {
            allowed_groups: vec!["生产".to_string()],
            allowed_tags: vec!["sandbox".to_string()],
            ..McpScope::default()
        };
        // 命中组即可
        assert_eq!(evaluate(&scope, &asset("a", "生产/数据库", &[])), ScopeVerdict::Allowed);
        // 或命中标签（组不命中）
        assert_eq!(evaluate(&scope, &asset("b", "测试", &["sandbox"])), ScopeVerdict::Allowed);
        // 都不命中 → 拒（默认拒绝，不是默认放行）
        assert_eq!(evaluate(&scope, &asset("c", "测试", &["dev"])), ScopeVerdict::DeniedNoMatch);
    }

    #[test]
    fn empty_scope_means_unrestricted() {
        // 用户从未配置 scope = 不收紧（产品默认，非 fail-closed 场景）
        let scope = McpScope::unrestricted();
        assert_eq!(evaluate(&scope, &asset("any", "任意组", &[])), ScopeVerdict::Allowed);
        assert!(scope.is_restricted() == false);
    }

    #[test]
    fn configured_lists_default_deny() {
        // 配置了 allow 列表但资产不命中 → 默认拒绝（不能静默放行圈外资产）
        let scope = McpScope {
            allowed_tags: vec!["prod".to_string()],
            ..McpScope::default()
        };
        assert_eq!(evaluate(&scope, &asset("x", "生产", &["dev"])), ScopeVerdict::DeniedNoMatch);
    }

    // ── 本机 FS 判定 ──
    #[test]
    fn local_fs_gate() {
        // ① 完全未配置 scope：本机传输不受影响（老用户零行为变更）
        let unconfigured = McpScope::unrestricted();
        assert_eq!(evaluate_local_fs(&unconfigured), ScopeVerdict::Allowed);
        // ② 配置了资产限制但没开本机 FS → 收紧
        let mut scope = McpScope {
            allowed_groups: vec!["生产".to_string()],
            ..McpScope::default()
        };
        assert_eq!(evaluate_local_fs(&scope), ScopeVerdict::DeniedLocalFs);
        // ③ 显式打开
        scope.allow_local_fs = true;
        assert_eq!(evaluate_local_fs(&scope), ScopeVerdict::Allowed);
        // ④ deny_all 收敛态对本机 FS 同样生效
        scope.deny_all = true;
        assert_eq!(evaluate_local_fs(&scope), ScopeVerdict::DeniedAll);
    }

    // ── serde 兼容 ──
    #[test]
    fn legacy_config_without_scope_field_deserializes() {
        // 旧 mcp-config.json（只有 level）读出的 scope = 全默认（不限制）
        let json = r#"{"level":"minimal"}"#;
        #[derive(serde::Deserialize)]
        struct Wrap {
            level: String,
            #[serde(default)]
            scope: McpScope,
        }
        let w: Wrap = serde_json::from_str(json).expect("旧配置必须可读");
        assert_eq!(w.scope, McpScope::unrestricted());
    }

    #[test]
    fn scope_serializes_camel_case() {
        let json = serde_json::to_string(&McpScope::deny_all()).unwrap();
        assert!(json.contains("\"denyAll\":true"), "camelCase 字段缺失: {json}");
    }
}
