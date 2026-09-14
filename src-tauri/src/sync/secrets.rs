//! 同步链路的本机凭据管理（会话密钥 + 恢复密码，从 sync.rs 按域拆出）。
//! 安全边界见 AGENTS.md §8：会话密钥 DPAPI 存储；只保存应用生成的恢复密码。
use super::*;

const SESSION_KEY_ID: &str = "sync-session-key";

/// 【v2.7】自动生成的恢复密码 credential id。
///
/// 只保存**应用自己生成**的恢复密码（见 `recovery_code` 模块的安全定位）：
/// 用户手输的主密码一律不落盘 —— 那可能是他在别处复用的口令，属于 §8 凭据红线的范围。
const RECOVERY_PASSWORD_ID: &str = "sync-recovery-password";

/// 生成一个高熵恢复密码并存进本机安全存储（DPAPI），返回明文供界面填入。
///
/// 为什么返回明文：用户需要**看到**它（并在换机时能抄/能复制），否则"自动生成"等于
/// 给他一个他自己都不知道的密钥 —— 换机恢复能力就静默消失了。明文只回到本机 webview，
/// 不落日志、不进资产 JSON。
#[tauri::command]
pub async fn sync_generate_recovery_password(state: State<'_, AppState>) -> Result<String, String> {
    let password = myshelltool_core::recovery_code::generate_recovery_password(
        myshelltool_core::recovery_code::DEFAULT_LEN,
    );
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    store.save(RECOVERY_PASSWORD_ID, &password)?;
    Ok(password)
}

/// 读回已保存的自动生成恢复密码（供「换机恢复 → 查看恢复密码」）。
/// 没有保存过（用户手输自己的密码）返回 None —— 不假装有。
#[tauri::command]
pub async fn sync_reveal_recovery_password(
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    store.read(RECOVERY_PASSWORD_ID)
}

/// 若本次使用的主密码与已保存的恢复密码不同，删掉后者。
///
/// 必要性：用户先用「生成强密码」建了备份，随后又改成自己的密码（或换机恢复时填了
/// 原密码）—— 那份旧恢复密码**已经打不开当前备份**了。留着它会让「查看恢复密码」显示
/// 一个错误的值，用户抄走后在换机时得到"密码错误"，且无从归因。宁可没有，不可给错。
pub fn drop_stale_recovery_password(state: &AppState, used_password: &str) {
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    match store.read(RECOVERY_PASSWORD_ID) {
        Ok(Some(saved)) if saved != used_password => {
            if let Err(e) = store.delete(RECOVERY_PASSWORD_ID) {
                log::warn!("sync: 清理已失效的恢复密码失败（下次仍会显示旧值）: {e}");
            } else {
                log::info!("sync: 本次主密码与已保存的恢复密码不同，已清除后者（它会打开不当前备份）");
            }
        }
        Ok(_) => {}
        Err(e) => log::warn!("sync: 读取已保存的恢复密码失败，跳过一致性清理: {e}"),
    }
}

/// 是否已保存自动生成的恢复密码（面板据此显示「查看恢复密码」入口）。
pub fn recovery_password_saved(state: &AppState) -> bool {
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    matches!(store.read(RECOVERY_PASSWORD_ID), Ok(Some(_)))
}

/// 派生会话密钥并 DPAPI 加密存盘（v1.6 启用自动同步）。
///
/// 用主密码 + **新的随机 salt** 派生：适用于「本机已在用某份载荷，现在补一个免密密钥」
/// （此后推送会按这把 key + salt 重写载荷，自洽）。换机恢复/首次初始化请改用
/// `session_key_for_payload` + `store_session_key`，**按载荷的 salt** 重建，否则两条路径
/// 会得到不同的 key。
///
/// 返回 `()`：副作用（落盘）即本函数全部职责，派生出的内存 key 副本无需回传给调用方——
/// 后续解密需要 key 时统一从 SecretStore 读（见 `read_session_key`）。
pub fn save_session_key(state: &AppState, master_password: &str) -> Result<(), String> {
    use myshelltool_core::crypto;
    let mut salt = vec![0u8; crypto::SALT_LEN];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
    let key = crypto::derive_session_key(master_password, &salt)?;
    store_session_key(state, &key, &salt)
}

/// 会话密钥落盘（`base64(key):base64(salt)`，整体交 SecretStore 的 DPAPI 加密）。
///
/// salt 必须一并存：写载荷时要用它（见 `pack_local_vault`），缺了就写不出可用主密码
/// 恢复的备份 —— 因此空 salt 直接拒绝（fail-closed）。
pub fn store_session_key(state: &AppState, key: &[u8; 32], salt: &[u8]) -> Result<(), String> {
    if salt.is_empty() {
        return Err("会话密钥缺少派生 salt：拒绝写出无法用主密码恢复的备份".to_string());
    }
    let payload = format!("{}:{}", b64(key), b64(salt));
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    store.save(SESSION_KEY_ID, &payload)
}

/// 开启/恢复同步时**顺手记住本机密钥**（DPAPI）：这是「登录 → 初始化一次 → 之后推拉与
/// 自动同步全免密」的关键一步。
///
/// 按**载荷自己的 salt** 重建 key（`session_key_for_payload`），保证免密路径与主密码路径
/// 解的是同一把 key。存盘失败**不阻断 setup**（免密只是便利，主密码路径照常可用），但
/// 留 warn 且不置 `auto_sync_enabled` —— 前端会退回「输主密码」模式，用户可再点一次
/// 「本机免密 & 自动同步」重试，不会被静默糊弄。
pub fn remember_session_key(
    state: &AppState,
    master_password: &str,
    payload: &SyncPayload,
    sync_state: &mut SyncState,
) {
    match myshelltool_core::sync::session_key_for_payload(payload, master_password) {
        Ok((key, salt)) => match store_session_key(state, &key, &salt) {
            Ok(()) => {
                sync_state.auto_sync_enabled = true;
                log::info!("sync_setup: 已记住本机会话密钥（DPAPI）——后续推拉/自动同步免密");
            }
            Err(e) => log::warn!("sync_setup: 会话密钥存盘失败，免密未启用（可稍后手动启用）: {e}"),
        },
        Err(e) => log::warn!("sync_setup: 建立会话密钥失败，免密未启用: {e}"),
    }
}

/// 从 SecretStore 读会话密钥 + 其派生 salt（DPAPI 解密）。未配置返回 None（调用方回退手动模式）。
///
/// **两者必须成对取用**：salt 要随每次加密的载荷写进 `blob.salt`，否则换机/重装后
/// 主密码无法重建同一把 key（v2.7 修复的可恢复性缺陷，见 `crypto::encrypt_with_key`）。
pub fn read_session_key_material(state: &AppState) -> Result<Option<([u8; 32], Vec<u8>)>, String> {
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    let payload = match store.read(SESSION_KEY_ID)? {
        Some(s) => s,
        None => return Ok(None), // 未启用自动同步
    };
    let (key_b64, salt_b64) = payload
        .split_once(':')
        .ok_or_else(|| "会话密钥载荷格式损坏".to_string())?;
    let key_bytes = b64_decode(key_b64)?;
    let key: [u8; 32] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "会话密钥长度异常".to_string())?;
    let salt = b64_decode(salt_b64)?;
    // fail-closed：salt 缺失/为空时**拒绝**继续（否则会写出「只有本机能解」的载荷，
    // 正是本次修复要根治的形态）。修法是关闭再重新启用自动同步以重新派生。
    if salt.is_empty() {
        return Err(
            "本机会话密钥缺少派生 salt（无法写出可用主密码恢复的备份）：请关闭并重新启用自动同步".to_string(),
        );
    }
    Ok(Some((key, salt)))
}

/// 只要会话密钥的便捷包装（解密/探测用，不需要 salt）。
pub fn read_session_key(state: &AppState) -> Result<Option<[u8; 32]>, String> {
    Ok(read_session_key_material(state)?.map(|(key, _salt)| key))
}

/// 删除会话密钥（关闭自动同步 / 重置密码 / 清空同步）。
pub fn delete_session_key(state: &AppState) -> Result<(), String> {
    let store = myshelltool_core::SecretStore::new(
        &state.secret_store_dir,
        Box::new(crate::dpapi_codec::DpapiCodec),
    );
    store.delete(SESSION_KEY_ID).map(|_| ())
}

// ─── base64 小工具（与 core::crypto 内部实现一致，sync 命令层用于会话密钥存盘）───
pub fn b64(data: &[u8]) -> String {
    // 复用标准 base64（与 core crypto 的 base64_encode 同算法，避免在命令层再依赖 core 私有函数）
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(CHARS[((n >> 18) & 63) as usize] as char);
        out.push(CHARS[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { CHARS[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { CHARS[(n & 63) as usize] as char } else { '=' });
    }
    out
}

pub fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if s.len() % 4 != 0 {
        return Err(format!("invalid base64 length: {}", s.len()));
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    for chunk in s.as_bytes().chunks(4) {
        let mut vals = [0u8; 4];
        let mut pad = 0;
        for (i, &b) in chunk.iter().enumerate() {
            vals[i] = if b == b'=' {
                pad += 1;
                0
            } else {
                CHARS.iter().position(|&c| c == b)
                    .ok_or_else(|| format!("invalid base64 char: {}", b as char))? as u8
            };
        }
        let n = ((vals[0] as u32) << 18) | ((vals[1] as u32) << 12)
            | ((vals[2] as u32) << 6) | (vals[3] as u32);
        out.push((n >> 16) as u8);
        if pad < 2 { out.push((n >> 8) as u8); }
        if pad < 1 { out.push(n as u8); }
    }
    Ok(out)
}
