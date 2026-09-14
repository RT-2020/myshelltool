//! 同步数据流命令（setup/push/pull/resolve/reset/clear/自动同步，
//! 从 sync.rs 按域拆出）。加密内核在 core::sync，Gist 客户端在 super::gist，
//! 本机凭据在 super::secrets。
use super::*;

#[tauri::command]
pub async fn sync_setup(
    state: State<'_, AppState>,
    master_password: String,
    gist_id: Option<String>,
) -> Result<SyncSetupResult, String> {
    if master_password.trim().is_empty() {
        return Err("主密码不能为空".to_string());
    }
    let pat = read_github_pat(&state)?;

    // 检查是否已配置
    let mut sync_state = load_sync_state(&state)?;
    if sync_state.gist_id.is_some() {
        let masked = mask_gist_id(sync_state.gist_id.as_deref().unwrap());
        return Ok(SyncSetupResult::AlreadyConfigured { gist_id_masked: masked });
    }

    if let Some(existing_gist_id) = gist_id.filter(|s| !s.trim().is_empty()) {
        // 换机器/导入场景：拉取已有 Gist
        let remote = gist_get(&pat, &existing_gist_id)
            .await?
            .ok_or_else(|| format!("Gist {existing_gist_id} 不存在"))?;
        let (content, _) = remote;
        let payload: SyncPayload = serde_json::from_str(&content)
            .map_err(|e| format!("Gist 内容非合法同步载荷: {e}"))?;
        // 统一走 decrypt_vault（给了密码 → 密码路径）。只有「salt 随载荷」的新格式能解开，
        // 旧格式由 core 明确拒绝并提示重建。
        let vault = decrypt_vault(&state, &master_password, &payload)?;
        let assets_json = serde_json::to_string_pretty(&vault.assets_store)
            .map_err(|e| format!("序列化资产失败: {e}"))?;

        // 换机拉取：自动将远端解密出的密码与私钥通过本机 DPAPI 写入本地 SecretStore。
        // 恢复失败不阻断导入（资产 JSON 已就绪、必须返回前端），但**逐个失败都要有痕**：
        // 曾只打一条 warn，用户看到「已拉取」却连不上（认证失败），归因成本极高。
        if !vault.credentials.is_empty() {
            match crate::sync_credentials::restore_sync_credentials(&state, &vault.credentials) {
                Ok((restored, failed)) => {
                    if !failed.is_empty() {
                        log::warn!(
                            "sync_setup: {} 项凭据恢复失败（成功 {restored} 项）：{}",
                            failed.len(),
                            failed
                                .iter()
                                .map(|(id, r)| format!("{id}: {r}"))
                                .collect::<Vec<_>>()
                                .join("；")
                        );
                    }
                }
                Err(e) => log::warn!("sync_setup: 恢复同步凭据到本机 SecretStore 失败: {e}"),
            }
        }
        // 资产文件写回失败必须中止（对照 sync_pull 的写法）：否则本地仍是旧内容，
        // 而下方 last_synced_at 照常前移，此后 pull 会以「安全拉取」误判并覆盖。
        // 原子写：资产 JSON 是本地唯一副本，半截即整个列表不可读。
        myshelltool_core::write_atomic(&state.asset_store_path, &assets_json).map_err(|e| {
            format!(
                "写回 connection-assets.json（{}）失败: {e}",
                state.asset_store_path.display()
            )
        })?;

        // 记录 sync state。last_synced_at 记本地完成时刻（与刚落盘的资产文件
        // mtime 同钟域）；记 Gist 的 updated_at（远端历史时刻）会让此后每次
        // has_local_changes_since_last_sync 都误判「本地有变更」。
        sync_state.gist_id = Some(existing_gist_id.clone());
        sync_state.local_rev = Some(payload.remote_rev);
        sync_state.last_synced_at = Some(chrono::Utc::now().to_rfc3339());
        // 按**这份载荷的 salt** 记住本机密钥：以后推拉/自动同步免密
        remember_session_key(&state, &master_password, &payload, &mut sync_state);
        save_sync_state(&state, &sync_state)?;
        // 换机恢复用的是原机主密码：若本机还存着一份**不同**的自动生成密码，它已打不开
        // 当前备份 —— 清掉，避免「查看恢复密码」给出错值
        drop_stale_recovery_password(&state, &master_password);

        Ok(SyncSetupResult::PulledRemote { assets_json })
    } else {
        // 首次推送：全量打包当前本地资产与凭据 → 创建 Gist
        let payload = pack_local_vault(&state, &sync_state, &master_password, 1)?;
        let payload_json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
        let (new_gist_id, _) = gist_create(&pat, &payload_json).await?;

        sync_state.gist_id = Some(new_gist_id.clone());
        sync_state.local_rev = Some(1);
        // 与 push 同理：记本地完成时刻，与本地文件 mtime 保持同钟域
        sync_state.last_synced_at = Some(chrono::Utc::now().to_rfc3339());
        // 按刚推上去的载荷的 salt 记住本机密钥（同一把 key 两条路都能解）
        remember_session_key(&state, &master_password, &payload, &mut sync_state);
        save_sync_state(&state, &sync_state)?;
        // 用户可能先点了「生成强密码」又改用自己的密码：清掉那份已失效的
        drop_stale_recovery_password(&state, &master_password);

        Ok(SyncSetupResult::Created {
            gist_id_masked: mask_gist_id(&new_gist_id),
        })
    }
}

/// sync_push 返回结果。
#[derive(Debug, Clone, Serialize)]
pub struct SyncPushResult {
    pub success: bool,
    pub message: String,
    pub new_rev: Option<u64>,
}

/// 辅助函数：根据当前本地资产及关联凭据打包加密载荷（优先会话密钥路径，否则走主密码）。
fn pack_local_vault(
    state: &AppState,
    sync_state: &SyncState,
    master_password: &str,
    new_rev: u64,
) -> Result<SyncPayload, String> {
    let local_json = read_local_assets(state)?;
    // 文件可读但 JSON 损坏 → 直接中止（push / setup 首推 / resolve_conflict(local)
    // 都经此打包）。旧版静默折叠成空 vault 再加密推 Gist，会无声清空远端备份
    // 且 last_synced_at 照常前移（用户看到「已推送」）。read_local_assets 已单独
    // 放行 NotFound（首次使用合法为空），这里只拦「可读但损坏」。
    let store: myshelltool_core::ConnectionAssetStore = serde_json::from_str(&local_json)
        .map_err(|e| {
            format!(
                "本地资产文件损坏，已中止同步以免覆盖远端备份（{}）：{e}。\
                 请检查该文件内容，或改用 sync_pull 从 Gist 恢复。",
                state.asset_store_path.display()
            )
        })?;

    let credentials = if sync_state.sync_credentials {
        // 三态处置（v2.6）：读到 → 上传；**读失败 → 中止本次推送**；本地缺失 → 提示但继续。
        // 曾用 `unwrap_or_default()` + 内部 warn-and-skip：读不出的凭据被静默丢弃，
        // 却仍把「不完整的 vault」推上 Gist 覆盖完整备份并报「已推送」——换机恢复后
        // 该资产没密码且无从归因。宁可本次不同步，也不能用残缺备份覆盖好备份。
        let outcome = crate::sync_credentials::collect_sync_credentials(state, &store.assets)?;
        if outcome.has_failures() {
            return Err(format!(
                "同步已中止：{} 项凭据读取失败（{}）。这些凭据不会进入备份，若继续推送会用不完整的备份覆盖 Gist 上的完整备份。\
                 请先修复（例如确认以同一 Windows 账户运行、凭据文件未被占用）后重试。",
                outcome.failed.len(),
                outcome.failure_summary()
            ));
        }
        if !outcome.missing.is_empty() {
            log::warn!(
                "sync_push: 资产引用了本地不存在的凭据 {} 项（可能已被清理），本次备份不含这些项",
                outcome.missing.len()
            );
        }
        outcome.items
    } else {
        vec![]
    };

    let vault = myshelltool_core::sync::SyncVaultData::new(store, credentials);

    if master_password.trim().is_empty() {
        // 免密路径（自动同步）：**连 salt 一起取**，写进载荷 —— 这样换机后主密码
        // 仍能重建同一把 key 解开这份备份（v2.7 可恢复性修复）。
        let (key, salt) = read_session_key_material(state)?
            .ok_or_else(|| "未启用自动同步，需提供主密码".to_string())?;
        myshelltool_core::sync::pack_vault_with_key(&vault, &key, &salt, new_rev)
    } else {
        myshelltool_core::sync::pack_vault(&vault, master_password, new_rev)
    }
}

/// 统一解密 helper：按调用方是否给了主密码选路径，解析为 SyncVaultData。
///
/// - 没给密码 → 免密路径（自动同步，必须本机有会话密钥）；
/// - 给了密码 → 密码路径。会话密钥加密的载荷也随带派生 salt，因此主密码能重建同一把
///   key —— 换机恢复即依赖此点；
/// - 旧格式载荷（`blob.salt` 缺省）由 core 统一明确拒绝（v2.7：不做旧版适配），
///   这里不再有任何回退分支。
fn decrypt_vault(
    state: &AppState,
    master_password: &str,
    payload: &SyncPayload,
) -> Result<myshelltool_core::sync::SyncVaultData, String> {
    if master_password.trim().is_empty() {
        let key = read_session_key(state)?
            .ok_or_else(|| "未启用自动同步，需提供主密码".to_string())?;
        return myshelltool_core::sync::unpack_vault_with_key(payload, &key);
    }
    myshelltool_core::sync::unpack_vault(payload, master_password)
}

/// 推送本地资产及关联凭据到 Gist（端到端加密）。
///
/// `master_password` 为空 → 会话密钥路径（需已启用自动同步）；非空 → 主密码路径。
/// 两条路径产出的都是同一种载荷格式（都带派生 salt），因此备份始终可用主密码恢复。
#[tauri::command]
pub async fn sync_push(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<SyncPushResult, String> {
    let mut sync_state = load_sync_state(&state)?;
    let gist_id = sync_state
        .gist_id
        .clone()
        .ok_or_else(|| "未配置同步（请先 sync_setup）".to_string())?;
    let pat = read_github_pat(&state)?;

    // v2.6：推送前先读远端 rev，与本地记录的 local_rev 对齐（乐观并发检查）。
    //
    // 事故形态：两台机器共用同一 Gist，A 推 rev5 后 B 推自己的 rev5'，`gist_update`
    // 是无条件 PATCH → **A 的改动在远端被静默抹掉**，而双方都显示「已推送」；此后
    // B 再 pull 时 decide() 见 rev 相等 → NoChange「已是最新」，两边都以为同步健康。
    // 这里不引入强制冲突弹窗（推送语义就是「让远端等于本地」），但**不允许在不知情
    // 的情况下覆盖别人的提交**：远端 rev 与本地认知不一致即中止并给出可操作提示。
    match gist_get(&pat, &gist_id).await? {
        Some((content, _updated_at)) => {
            let local_rev = sync_state.local_rev.unwrap_or(0);
            // 远端 rev 在载荷里（明文元数据，无需解密）。解析失败说明远端不是本应用的
            // 载荷格式 → 同样不能盲推覆盖，按「状态异常」中止。
            let remote_rev = match serde_json::from_str::<myshelltool_core::sync::SyncPayload>(&content) {
                Ok(payload) => payload.remote_rev,
                Err(e) => {
                    return Err(format!(
                        "推送已中止：远端备份内容不是可识别的同步载荷（解析失败: {e}）。\
                         继续推送会覆盖远端文件，请先确认该 Gist 是否被其他工具/版本改写。"
                    ));
                }
            };
            if remote_rev != local_rev {
                return Err(format!(
                    "推送已中止：远端备份已被其他设备/会话更新（远端 rev {remote_rev}，本地记录 rev {local_rev}）。\
                     继续推送会用本地内容覆盖数据、丢掉远端那次保存。请先执行「拉取」确认远端改动后再推送。"
                ));
            }
        }
        None => {
            // Gist 被删（手动清理/权限变化）：不是「远端为空可以随便写」，而是状态异常。
            return Err(
                "推送已中止：Gist 不存在（可能已被手动删除或失去访问权限）。请重新配置同步（sync_setup）后再推送。"
                    .to_string(),
            );
        }
    }

    let new_rev = sync_state.local_rev.unwrap_or(0) + 1;
    let payload = pack_local_vault(&state, &sync_state, &master_password, new_rev)?;
    let payload_json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;

    gist_update(&pat, &gist_id, &payload_json).await?;

    sync_state.local_rev = Some(new_rev);
    // push 后本地文件 mtime ≤ 此刻；last_synced_at 记本地完成时刻与 mtime 同钟域，
    // 才不会被 Gist 服务器钟与本机钟的偏差误报成「本地有变更」
    sync_state.last_synced_at = Some(chrono::Utc::now().to_rfc3339());
    // 【v2.7】用主密码推成功、且本机还没有免密密钥时，顺手记住这把 key：
    // 「输一次主密码」就足够把免密打开（已免密的老用户不受影响，不做 key churn）。
    if !master_password.trim().is_empty() && !sync_state.auto_sync_enabled {
        remember_session_key(&state, &master_password, &payload, &mut sync_state);
    }
    save_sync_state(&state, &sync_state)?;

    Ok(SyncPushResult {
        success: true,
        message: format!("已推送（rev {new_rev}）"),
        new_rev: Some(new_rev),
    })
}

/// sync_pull 返回结果（含冲突决策）。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "decision")]
pub enum SyncPullResult {
    /// 无需操作（双方都没变）。
    NoChange,
    /// 安全拉取：远端更新，本地没变。assets_json 是解密后的资产，前端直接导入。
    /// `credentials_failed`（v2.6）：本次有 N 项凭据未能写入本机 SecretStore——
    /// 资产已拉取成功，但这些资产的密码/私钥仍不可用，前端必须提示（曾静默只 warn，
    /// 用户看到「已拉取」却连不上，归因困难）。
    Pulled {
        assets_json: String,
        new_rev: u64,
        #[serde(default)]
        credentials_failed: usize,
    },
    /// 本地比远端新，建议 push 而非 pull。
    LocalNewer,
    /// 冲突：双方都变了。返回本地+远端资产 JSON，前端弹窗让用户选。
    Conflict {
        local_json: String,
        remote_json: String,
        remote_rev: u64,
    },
}

/// 拉取 Gist + 冲突检测。
#[tauri::command]
pub async fn sync_pull(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<SyncPullResult, String> {
    let sync_state = load_sync_state(&state)?;
    let gist_id = sync_state
        .gist_id
        .clone()
        .ok_or_else(|| "未配置同步（请先 sync_setup）".to_string())?;
    let pat = read_github_pat(&state)?;

    // 1. 拉远端
    let remote_raw = gist_get(&pat, &gist_id)
        .await?
        .ok_or_else(|| "Gist 不存在（可能被手动删除）".to_string())?;
    let (content, _) = remote_raw;
    let remote_payload: SyncPayload = serde_json::from_str(&content)
        .map_err(|e| format!("Gist 内容非合法同步载荷: {e}"))?;

    // 2. 本地是否有变更：比较本地资产 mtime vs 上次同步时间
    let local_has_changes = has_local_changes_since_last_sync(&state, &sync_state);

    // 3. 冲突判定
    let decision = sync::decide(local_has_changes, Some(&remote_payload), &sync_state);

    match decision {
        SyncDecision::NoChange => Ok(SyncPullResult::NoChange),
        SyncDecision::PullRemote => {
            // 安全拉取：解密远端 vault，覆盖本地资产并恢复凭据到 SecretStore
            let vault = decrypt_vault(&state, &master_password, &remote_payload)?;
            let remote_json = serde_json::to_string_pretty(&vault.assets_store)
                .map_err(|e| format!("序列化资产失败: {e}"))?;
            myshelltool_core::write_atomic(&state.asset_store_path, &remote_json)
                .map_err(|e| format!("写回 connection-assets.json: {e}"))?;

            // 自动将远端解密的密码与托管私钥存入本地 SecretStore（DPAPI 重新加密）。
            // 资产已落盘 → 不阻断 pull；但失败项数要回传前端（toast 里显式提示），
            // 否则用户看到「已拉取」却连不上（凭据没恢复），归因困难。
            let mut credentials_failed = 0usize;
            if !vault.credentials.is_empty() {
                match crate::sync_credentials::restore_sync_credentials(&state, &vault.credentials) {
                    Ok((restored, failed)) => {
                        credentials_failed = failed.len();
                        if credentials_failed > 0 {
                            log::warn!(
                                "sync_pull: {} 项凭据恢复失败（成功 {restored} 项）：{}",
                                credentials_failed,
                                failed
                                    .iter()
                                    .map(|(id, r)| format!("{id}: {r}"))
                                    .collect::<Vec<_>>()
                                    .join("；")
                            );
                        }
                    }
                    Err(e) => {
                        credentials_failed = vault.credentials.len();
                        log::warn!("sync_pull: 恢复同步凭据到本机 SecretStore 失败: {e}");
                    }
                }
            }

            let mut new_state = sync_state;
            new_state.local_rev = Some(remote_payload.remote_rev);
            // last_synced_at 记本地完成时刻（与刚落盘的资产文件 mtime 同钟域）：
            // 记 Gist 的 updated_at（远端历史时刻）会导致此后每次
            // has_local_changes_since_last_sync 都误判「本地有变更」
            new_state.last_synced_at = Some(chrono::Utc::now().to_rfc3339());
            // 【v2.7】用主密码拉成功、且本机还没有免密密钥时顺手记住（同 push）：
            // 按**远端载荷的 salt** 重建，免密路径与主密码路径才是同一把 key。
            if !master_password.trim().is_empty() && !new_state.auto_sync_enabled {
                remember_session_key(&state, &master_password, &remote_payload, &mut new_state);
            }
            save_sync_state(&state, &new_state)?;

            Ok(SyncPullResult::Pulled {
                assets_json: remote_json,
                new_rev: remote_payload.remote_rev,
                credentials_failed,
            })
        }
        SyncDecision::PushLocal => Ok(SyncPullResult::LocalNewer),
        SyncDecision::Conflict => {
            // 冲突：返回双方 JSON，前端弹窗让用户选
            let local_json = read_local_assets(&state)?;
            let vault = decrypt_vault(&state, &master_password, &remote_payload)?;
            let remote_json = serde_json::to_string_pretty(&vault.assets_store)
                .map_err(|e| format!("序列化远端资产失败: {e}"))?;
            Ok(SyncPullResult::Conflict {
                local_json,
                remote_json,
                remote_rev: remote_payload.remote_rev,
            })
        }
    }
}

/// 用户在冲突对话框选择后，强制用某一方的数据覆盖。
///
/// `choice`: "local" | "remote"
/// - local：加密本地数据与凭据推送（覆盖远端）
/// - remote：用 remote_json 覆盖本地，并拉取凭据恢复
#[tauri::command]
pub async fn sync_resolve_conflict(
    state: State<'_, AppState>,
    master_password: String,
    choice: String,
    remote_json: String,
    remote_rev: u64,
) -> Result<(), String> {
    let pat = read_github_pat(&state)?;
    let sync_state = load_sync_state(&state)?;
    let gist_id = sync_state
        .gist_id
        .clone()
        .ok_or_else(|| "未配置同步".to_string())?;

    match choice.as_str() {
        "local" => {
            // 用本地覆盖远端：打包全量本地资产及凭据推送
            let new_rev = remote_rev + 1;
            let payload = pack_local_vault(&state, &sync_state, &master_password, new_rev)?;
            let payload_json = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
            gist_update(&pat, &gist_id, &payload_json).await?;

            let mut new_state = sync_state;
            new_state.local_rev = Some(new_rev);
            // 与 sync_push 同理：记本地完成时刻，与 mtime 同钟域
            new_state.last_synced_at = Some(chrono::Utc::now().to_rfc3339());
            save_sync_state(&state, &new_state)?;
        }
        "remote" => {
            // 用远端覆盖本地：写入资产文件，并尝试解密远端凭据恢复
            myshelltool_core::write_atomic(&state.asset_store_path, &remote_json)
                .map_err(|e| format!("写回 connection-assets.json: {e}"))?;

            // 尽力恢复远端凭据（remote_json 只含资产，凭据在 Gist 载荷里）。
            // 任何一步失败都不阻断「用远端覆盖本地」的结果，但逐层留日志——
            // 用户选了 remote 通常预期凭据也一并恢复，静默跳过会掩盖这一点。
            match gist_get(&pat, &gist_id).await {
                Ok(Some((content, _))) => {
                    match serde_json::from_str::<SyncPayload>(&content) {
                        Ok(payload) => match decrypt_vault(&state, &master_password, &payload) {
                            Ok(vault) => {
                                if !vault.credentials.is_empty() {
                                    match crate::sync_credentials::restore_sync_credentials(
                                        &state,
                                        &vault.credentials,
                                    ) {
                                        Ok((_, failed)) if !failed.is_empty() => log::warn!(
                                            "sync_resolve_conflict: {} 项凭据恢复失败：{}",
                                            failed.len(),
                                            failed
                                                .iter()
                                                .map(|(id, r)| format!("{id}: {r}"))
                                                .collect::<Vec<_>>()
                                                .join("；")
                                        ),
                                        Ok(_) => {}
                                        Err(e) => log::warn!(
                                            "sync_resolve_conflict: 恢复同步凭据失败: {e}"
                                        ),
                                    }
                                }
                            }
                            Err(e) => log::warn!(
                                "sync_resolve_conflict: 解密远端载荷以恢复凭据失败: {e}"
                            ),
                        },
                        Err(e) => log::warn!(
                            "sync_resolve_conflict: 解析 Gist 载荷以恢复凭据失败: {e}"
                        ),
                    }
                }
                Ok(None) => {
                    log::warn!("sync_resolve_conflict: Gist 不存在，跳过凭据恢复");
                }
                Err(e) => {
                    log::warn!("sync_resolve_conflict: 重新拉取 Gist 以恢复凭据失败: {e}");
                }
            }

            let mut new_state = sync_state;
            new_state.local_rev = Some(remote_rev);
            new_state.last_synced_at = Some(chrono::Utc::now().to_rfc3339());
            save_sync_state(&state, &new_state)?;
        }
        other => return Err(format!("无效的冲突选择: {other}")),
    }
    Ok(())
}

/// 重置主密码（需验证旧密码）。
///
/// 重新加密当前本地数据并用新密码推送（gist_id 不变）。
#[tauri::command]
pub async fn sync_reset_master_password(
    state: State<'_, AppState>,
    old_password: String,
    new_password: String,
) -> Result<(), String> {
    if new_password.trim().is_empty() {
        return Err("新主密码不能为空".to_string());
    }
    if old_password.trim().is_empty() {
        return Err("旧主密码不能为空".to_string());
    }
    let sync_state = load_sync_state(&state)?;
    let gist_id = sync_state
        .gist_id
        .clone()
        .ok_or_else(|| "未配置同步".to_string())?;
    let pat = read_github_pat(&state)?;

    // 验证旧密码：拉远端用旧密码解密
    let remote_raw = gist_get(&pat, &gist_id)
        .await?
        .ok_or_else(|| "Gist 不存在".to_string())?;
    let (content, _) = remote_raw;
    let payload: SyncPayload = serde_json::from_str(&content)
        .map_err(|e| format!("Gist 内容非合法载荷: {e}"))?;
    // 验证旧密码 + 取出**完整保管库（含凭据）**。
    //
    // v2.7 修两处缺陷：
    // ① 原实现用已删除的 `sync::unpack()` 取明文再 `sync::pack()` 回写 —— 那个接口在明文是
    //    SyncVaultData 时**只返回 assets_store、丢弃 credentials**，于是「重置主密码」
    //    会把 Gist 备份里的密码/私钥整段抹掉，且 UI 只提示「✓ 主密码已重置」（静默丢数据）。
    //    现在走保管库级 `unpack_vault` / `pack_vault`，原样搬运凭据（旧接口已删，从类型上杜绝）。
    // ② 旧格式载荷曾恒报「旧主密码错误」（正确密码也被判错）→ 现在 core 统一明确拒绝该格式，
    //    文案直接说明「旧格式不再支持、请重推一次」，不再伪装成密码错误。
    let vault = decrypt_vault(&state, &old_password, &payload)?;

    // 用新密码重新加密同一份保管库并推送
    let new_rev = sync_state.local_rev.unwrap_or(0) + 1;
    let new_payload = myshelltool_core::sync::pack_vault(&vault, &new_password, new_rev)?;
    let payload_json = serde_json::to_string(&new_payload).map_err(|e| e.to_string())?;
    gist_update(&pat, &gist_id, &payload_json).await?;

    let mut new_state = sync_state;
    new_state.local_rev = Some(new_rev);
    // 与 sync_push 同理：记本地完成时刻，与 mtime 同钟域
    new_state.last_synced_at = Some(chrono::Utc::now().to_rfc3339());
    // v1.6：若已启用自动同步，用新密码重新派生会话密钥（保持一致性，旧密钥失效）
    if new_state.auto_sync_enabled {
        save_session_key(&state, &new_password)?;
    }
    // 换了主密码 → 旧主密码（含可能保存过的自动生成密码）已打不开备份，清掉免得给错值
    drop_stale_recovery_password(&state, &new_password);
    save_sync_state(&state, &new_state)?;
    Ok(())
}

/// 清空同步配置（忘了主密码的逃生口）。
///
/// 删除本地 sync-state.json + 会话密钥。Gist 上的数据保留（用户可手动去 GitHub 删）。
#[tauri::command]
pub async fn sync_clear(state: State<'_, AppState>) -> Result<(), String> {
    let path = sync_state_path(&state)?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| format!("删除 sync-state.json: {e}"))?;
    }
    // v1.6：连同会话密钥一起清。
    // 失败必须可见：曾用 `let _ =` 吞掉删除错误，UI 显示「已清空同步」而等价于
    // 主密码的会话密钥仍留在 credentials/ 里——用户以为这个「忘了主密码的逃生口」
    // 已生效（形态 C 静默兜底）。delete_session_key 内部已把「本来就不存在」当成功。
    if let Err(e) = delete_session_key(&state) {
        log::warn!("sync_clear: 会话密钥删除失败: {e}");
        return Err(format!(
            "sync-state 已清除，但会话密钥删除失败：{e}。请手动删除 credentials/sync-session-key.cred 后重试"
        ));
    }
    Ok(())
}

// ─── v1.6 自动同步命令 ───

/// sync_enable_auto_sync 返回结果。
#[derive(Debug, Clone, Serialize)]
pub struct AutoSyncResult {
    pub enabled: bool,
    pub message: String,
}

/// v1.6 启用自动同步：验证主密码 → 派生会话密钥 → DPAPI 加密存 SecretStore。
///
/// 必须先完成 sync_setup（有 gist_id）。主密码派生后即丢弃，仅 DPAPI 密文持久化。
/// 会话密钥的加解密能力与主密码等价（同一 Argon2id 派生），但绑定本机 Windows 用户。
#[tauri::command]
pub async fn sync_enable_auto_sync(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<AutoSyncResult, String> {
    if master_password.trim().is_empty() {
        return Err("主密码不能为空".to_string());
    }
    let mut sync_state = load_sync_state(&state)?;
    let gist_id = sync_state
        .gist_id
        .clone()
        .ok_or_else(|| "未配置同步（请先完成 sync_setup）".to_string())?;

    // 验证主密码正确性：拉远端用主密码解密。
    //
    // v2.7：唯一载荷格式都带派生 salt，主密码**一律**能验（不再有「旧格式跳过验证」的分支）；
    // 旧格式载荷由 core 明确拒绝，用户会看到「旧格式不再支持，请重推一次」而不是被静默放行。
    let pat = read_github_pat(&state)?;
    if let Some((content, _)) = gist_get(&pat, &gist_id).await? {
        let payload: SyncPayload = serde_json::from_str(&content)
            .map_err(|e| format!("Gist 内容非合法载荷: {e}"))?;
        myshelltool_core::sync::unpack_vault(&payload, &master_password)
            .map_err(|e| format!("主密码验证失败：{e}"))?;
    }

    // 派生会话密钥 + DPAPI 加密存盘
    save_session_key(&state, &master_password)?;

    sync_state.auto_sync_enabled = true;
    save_sync_state(&state, &sync_state)?;

    Ok(AutoSyncResult {
        enabled: true,
        message: "自动同步已启用（会话密钥已用 DPAPI 保护）".to_string(),
    })
}

/// v1.6 关闭自动同步：删除会话密钥。
#[tauri::command]
pub async fn sync_disable_auto_sync(state: State<'_, AppState>) -> Result<AutoSyncResult, String> {
    let mut sync_state = load_sync_state(&state)?;
    delete_session_key(&state)?;
    sync_state.auto_sync_enabled = false;
    save_sync_state(&state, &sync_state)?;
    Ok(AutoSyncResult {
        enabled: false,
        message: "自动同步已关闭（会话密钥已删除）".to_string(),
    })
}
