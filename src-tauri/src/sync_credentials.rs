//! 凭据与托管私钥同步辅助层（Full Sync Vault）
//!
//! 负责从本地 SecretStore（Windows DPAPI 保护）中安全读取与资产关联的凭据，
//! 以及在从 Gist 拉取解密后，将凭据逐一通过本机 DPAPI 重新加密写入 SecretStore。

use std::collections::HashSet;
use log::{info, warn};
use myshelltool_core::sync::SyncCredentialItem;
use myshelltool_core::ConnectionAsset;
use crate::AppState;

/// 凭据收集结果：成功项 + 失败项（id 与原因）。
///
/// 为什么不能只返回成功项：推送时会用收集结果**整体覆盖** Gist 备份，若某条凭据
/// 读不出来却被静默跳过，远端备份就少一项（换机恢复后该资产没密码），而调用方仍
/// 报「已推送」。对照 v2.5 对资产 JSON 的同类处理（损坏即中止同步），凭据方向
/// 必须同样显式失败——**宁可不同步，也不能用不完整的备份覆盖完整的备份**。
#[derive(Debug, Default)]
pub struct CollectOutcome {
    pub items: Vec<SyncCredentialItem>,
    /// (credential_id, 原因)：读取失败（DPAPI 解不开、文件损坏、被占用）。
    pub failed: Vec<(String, String)>,
    /// 资产引用了但本地根本没有的凭据（可能已被清理）——不算失败，但要可见。
    pub missing: Vec<String>,
}

impl CollectOutcome {
    /// 是否有「本应同步却没同步」的项（读取失败）。
    pub fn has_failures(&self) -> bool {
        !self.failed.is_empty()
    }

    /// 人类可读的失败摘要（给 Err 消息用）。
    pub fn failure_summary(&self) -> String {
        self.failed
            .iter()
            .map(|(id, reason)| format!("{id}: {reason}"))
            .collect::<Vec<_>>()
            .join("；")
    }
}

/// 收集与当前资产列表关联的所有凭据（密码 / passphrase / 托管私钥内容）。
/// 仅读取已存在的凭据项，不泄露给任何日志或非加密通道。
///
/// 三态：读到 → items；**读失败 → failed**（调用方必须据此中止同步）；
/// 不存在 → missing（仅提示，不算失败）。
pub fn collect_sync_credentials(
    state: &AppState,
    assets: &[ConnectionAsset],
) -> Result<CollectOutcome, String> {
    let mut needed_ids = HashSet::new();
    for asset in assets {
        if let Some(ref id) = asset.credential_id {
            if !id.trim().is_empty() {
                needed_ids.insert(id.clone());
            }
        }
        if let Some(ref id) = asset.passphrase_credential_id {
            if !id.trim().is_empty() {
                needed_ids.insert(id.clone());
            }
        }
        if let Some(ref id) = asset.private_key_credential_id {
            if !id.trim().is_empty() {
                needed_ids.insert(id.clone());
            }
        }
    }

    if needed_ids.is_empty() {
        return Ok(CollectOutcome::default());
    }

    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );

    let mut outcome = CollectOutcome::default();
    for id in needed_ids {
        match store.read(&id) {
            Ok(Some(secret)) => {
                if secret.is_empty() {
                    // 文件存在但内容为空：等同不可用（不能把空口令推上远端）
                    outcome.failed.push((id, "凭据文件为空".to_string()));
                } else {
                    outcome.items.push(SyncCredentialItem { id, secret });
                }
            }
            Ok(None) => outcome.missing.push(id),
            Err(e) => {
                warn!("collect_sync_credentials: 读取凭据 '{id}' 失败: {e}");
                outcome.failed.push((id, e));
            }
        }
    }

    info!(
        "collect_sync_credentials: 收集 {} 项；读取失败 {} 项；本地缺失 {} 项",
        outcome.items.len(),
        outcome.failed.len(),
        outcome.missing.len()
    );
    Ok(outcome)
}

/// 将从 Gist 解密得到的凭据批量写入新机器的 SecretStore（自动经本机 DPAPI 保护）。
///
/// 返回 `(成功数, 失败项)`：失败项**必须上报调用方**——曾只 `warn!` 后返回计数，
/// 用户看到「✓ 已拉取」，随后连接以「认证失败」表现，极难归因到「凭据没恢复」。
pub fn restore_sync_credentials(
    state: &AppState,
    credentials: &[SyncCredentialItem],
) -> Result<(usize, Vec<(String, String)>), String> {
    if credentials.is_empty() {
        return Ok((0, vec![]));
    }

    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );

    let mut restored_count = 0;
    let mut failed: Vec<(String, String)> = Vec::new();
    for item in credentials {
        if item.id.trim().is_empty() || item.secret.is_empty() {
            failed.push((item.id.clone(), "载荷中该凭据为空".to_string()));
            continue;
        }
        match store.save(&item.id, &item.secret) {
            Ok(()) => {
                restored_count += 1;
            }
            Err(e) => {
                warn!("restore_sync_credentials: 写入凭据 '{}' 失败: {e}", item.id);
                failed.push((item.id.clone(), e));
            }
        }
    }

    info!(
        "restore_sync_credentials: 成功恢复并加密落盘 {}/{} 项凭据（失败 {} 项）",
        restored_count,
        credentials.len(),
        failed.len()
    );
    Ok((restored_count, failed))
}
