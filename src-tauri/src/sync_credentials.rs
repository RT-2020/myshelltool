//! 凭据与托管私钥同步辅助层（Full Sync Vault）
//!
//! 负责从本地 SecretStore（Windows DPAPI 保护）中安全读取与资产关联的凭据，
//! 以及在从 Gist 拉取解密后，将凭据逐一通过本机 DPAPI 重新加密写入 SecretStore。

use std::collections::HashSet;
use log::{info, warn};
use myshelltool_core::sync::SyncCredentialItem;
use myshelltool_core::ConnectionAsset;
use crate::AppState;

/// 收集与当前资产列表关联的所有凭据（密码 / passphrase / 托管私钥内容）。
/// 仅读取已存在的凭据项，不泄露给任何日志或非加密通道。
pub fn collect_sync_credentials(
    state: &AppState,
    assets: &[ConnectionAsset],
) -> Result<Vec<SyncCredentialItem>, String> {
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
        return Ok(vec![]);
    }

    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );

    let mut result = Vec::with_capacity(needed_ids.len());
    for id in needed_ids {
        match store.read(&id) {
            Ok(Some(secret)) => {
                if !secret.is_empty() {
                    result.push(SyncCredentialItem { id, secret });
                }
            }
            Ok(None) => {
                // 本地不存在该凭据文件（可能已被清理或尚未保存），跳过
            }
            Err(e) => {
                warn!("collect_sync_credentials: 读取凭据 '{id}' 失败: {e}");
            }
        }
    }

    info!("collect_sync_credentials: 已收集 {} 项关联凭据用于加密同步", result.len());
    Ok(result)
}

/// 将从 Gist 解密得到的凭据批量写入新机器的 SecretStore（自动经本机 DPAPI 保护）。
pub fn restore_sync_credentials(
    state: &AppState,
    credentials: &[SyncCredentialItem],
) -> Result<usize, String> {
    if credentials.is_empty() {
        return Ok(0);
    }

    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );

    let mut restored_count = 0;
    for item in credentials {
        if item.id.trim().is_empty() || item.secret.is_empty() {
            continue;
        }
        match store.save(&item.id, &item.secret) {
            Ok(()) => {
                restored_count += 1;
            }
            Err(e) => {
                warn!("restore_sync_credentials: 写入凭据 '{}' 失败: {e}", item.id);
            }
        }
    }

    info!("restore_sync_credentials: 成功恢复并加密落盘 {}/{} 项凭据", restored_count, credentials.len());
    Ok(restored_count)
}
