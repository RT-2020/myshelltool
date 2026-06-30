import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { invokeBackend, isTauriRuntime } from '../services/backend.js';

/**
 * useSyncStore — v1.3 Gist 同步前端状态 + actions（v1.6 增自动同步）。
 *
 * 对接 src-tauri/src/sync.rs 的 10 个命令（v1.6 +3：auto_sync 三件套）。负责：
 * - 同步状态展示（sync_status → 状态栏真实同步状态）
 * - push/pull/冲突解决/重置密码/清空 的 action 封装
 * - 冲突暂存（pull 返回 Conflict 时存双方 JSON，供冲突框展示后选择）
 * - v1.6 自动同步：enable/disable + 会话密钥路径 push/pull（无需主密码）+ 远端更新探测
 *
 * 与 assets store 的关系：assets.js 的写操作完成后调 maybeAutoPush（经 workbench bridge），
 * 触发本 store 的 autoPushIfEnabled —— 若 autoSyncEnabled 则后台 push（不弹窗，失败静默 announce）。
 */
export const useSyncStore = defineStore('sync', () => {
  // ============================================================
  // State
  // ============================================================
  const status = ref(null); // sync_status 返回
  const loading = ref(false); // 操作进行中（防重复）
  const lastMessage = ref(''); // 最近操作结果文案（成功/失败）
  // 冲突暂存：pull 返回 Conflict 时存，冲突框据此展示 + 用户选择后清空
  const conflict = ref(null); // { localJson, remoteJson, remoteRev } | null
  // v1.6：远端更新探测结果（启动时 checkRemoteUpdates 写入）
  const remoteHasUpdates = ref(false);

  // ============================================================
  // Computed
  // ============================================================
  const configured = computed(() => Boolean(status.value?.configured));
  const patConfigured = computed(() => Boolean(status.value?.pat_configured));
  const lastSyncedAt = computed(() => status.value?.last_synced_at ?? null);
  const gistIdMasked = computed(() => status.value?.gist_id_masked ?? null);
  // v1.6：是否启用自动同步（会话密钥已派生）
  const autoSyncEnabled = computed(() => Boolean(status.value?.auto_sync_enabled));
  // 状态栏同步文案：优先真实同步状态，回退 PAT 配置状态
  const syncText = computed(() => {
    if (loading.value) return '同步中…';
    if (lastMessage.value) return lastMessage.value;
    if (configured.value) return autoSyncEnabled.value ? '同步已配置（自动）' : '同步已配置';
    if (patConfigured.value) return 'PAT 已配置';
    return '未配置同步';
  });

  // ============================================================
  // Actions
  // ============================================================

  /** 刷新同步状态（sync_status）。浏览器预览模式静默跳过。 */
  async function refreshStatus() {
    if (!isTauriRuntime()) return;
    try {
      status.value = await invokeBackend('sync_status');
    } catch (error) {
      // eslint-disable-next-line no-console
      console.warn('[sync] refreshStatus failed:', error?.message || error);
    }
  }

  /**
   * 首次设置同步。
   * @param {string} masterPassword 主密码
   * @param {string} [gistId] 可选已有 gist_id（换机器场景）
   * @returns {Promise<object>} SyncSetupResult（Created/PulledRemote/AlreadyConfigured）
   */
  async function setup(masterPassword, gistId) {
    if (loading.value) return null;
    loading.value = true;
    lastMessage.value = '';
    try {
      const result = await invokeBackend('sync_setup', {
        masterPassword,
        gistId: gistId || null
      });
      await refreshStatus();
      flashMessage(result.kind === 'Created' ? '✓ 同步已配置（新 Gist）'
        : result.kind === 'PulledRemote' ? '✓ 已拉取远端数据'
        : '同步已配置，无需重复设置');
      return result;
    } catch (error) {
      flashMessage(`✗ ${error?.message || error}`, true);
      return null;
    } finally {
      loading.value = false;
    }
  }

  /**
   * 推送本地资产到 Gist。
   * @param {string} [masterPassword] 主密码。v1.6：留空时走会话密钥路径（自动同步）。
   */
  async function push(masterPassword = '') {
    if (loading.value) return null;
    loading.value = true;
    lastMessage.value = '';
    try {
      const result = await invokeBackend('sync_push', { masterPassword });
      await refreshStatus();
      flashMessage(result.message);
      // v1.6：push 成功后远端已是最新的，清除更新提示
      remoteHasUpdates.value = false;
      return result;
    } catch (error) {
      flashMessage(`✗ 推送失败：${error?.message || error}`, true);
      return null;
    } finally {
      loading.value = false;
    }
  }

  /**
   * 拉取 Gist + 冲突检测。
   * @param {string} [masterPassword] 主密码。v1.6：留空时走会话密钥路径。
   * 返回 Conflict 时存入 conflict 暂存区（供冲突框展示）。
   * @returns {Promise<object>} SyncPullResult 的 decision
   */
  async function pull(masterPassword = '') {
    if (loading.value) return null;
    loading.value = true;
    lastMessage.value = '';
    try {
      const result = await invokeBackend('sync_pull', { masterPassword });
      await refreshStatus();
      switch (result.decision) {
        case 'NoChange':
          flashMessage('已是最新（双方都无变更）');
          remoteHasUpdates.value = false;
          break;
        case 'Pulled':
          flashMessage(`✓ 已拉取远端数据（rev ${result.new_rev}）`);
          remoteHasUpdates.value = false;
          break;
        case 'LocalNewer':
          flashMessage('本地比远端新，建议推送');
          break;
        case 'Conflict':
          // 存入冲突暂存区，前端据此弹冲突框
          conflict.value = {
            localJson: result.local_json,
            remoteJson: result.remote_json,
            remoteRev: result.remote_rev
          };
          break;
      }
      return result;
    } catch (error) {
      flashMessage(`✗ 拉取失败：${error?.message || error}`, true);
      return null;
    } finally {
      loading.value = false;
    }
  }

  /**
   * 解决冲突（用户在冲突框选择后调）。
   * @param {string} [masterPassword] 主密码。v1.6：留空时走会话密钥路径。
   * @param {string} choice 'local' | 'remote'
   */
  async function resolveConflict(masterPassword = '', choice) {
    if (!conflict.value) return;
    if (loading.value) return;
    loading.value = true;
    try {
      await invokeBackend('sync_resolve_conflict', {
        masterPassword,
        choice,
        remoteJson: conflict.value.remoteJson,
        remoteRev: conflict.value.remoteRev
      });
      conflict.value = null;
      await refreshStatus();
      flashMessage(choice === 'local' ? '✓ 已用本地覆盖远端' : '✓ 已用远端覆盖本地');
    } catch (error) {
      flashMessage(`✗ 冲突解决失败：${error?.message || error}`, true);
      return;
    } finally {
      loading.value = false;
    }
  }

  /** 重置主密码（需旧密码验证）。重置后若已启用自动同步，会话密钥自动重新派生。 */
  async function resetMasterPassword(oldPassword, newPassword) {
    if (loading.value) return;
    loading.value = true;
    try {
      await invokeBackend('sync_reset_master_password', {
        oldPassword,
        newPassword
      });
      flashMessage('✓ 主密码已重置');
    } catch (error) {
      flashMessage(`✗ ${error?.message || error}`, true);
      return null;
    } finally {
      loading.value = false;
    }
  }

  /** 清空同步配置（忘了主密码的逃生口）。同时清除会话密钥。 */
  async function clearSync() {
    if (loading.value) return;
    loading.value = true;
    try {
      await invokeBackend('sync_clear');
      conflict.value = null;
      await refreshStatus();
      flashMessage('已清空同步配置');
    } catch (error) {
      flashMessage(`✗ ${error?.message || error}`, true);
      return null;
    } finally {
      loading.value = false;
    }
  }

  /** 清空冲突暂存区（用户在冲突框点"取消"）。 */
  function dismissConflict() {
    conflict.value = null;
  }

  // ============================================================
  // v1.6 自动同步 actions
  // ============================================================

  /**
   * 启用自动同步：验证主密码 → 派生会话密钥 → DPAPI 加密存盘。
   * @param {string} masterPassword 主密码（一次性，验证后派生密钥即丢弃）
   * @returns {Promise<boolean>} 是否成功
   */
  async function enableAutoSync(masterPassword) {
    if (loading.value) return false;
    loading.value = true;
    lastMessage.value = '';
    try {
      await invokeBackend('sync_enable_auto_sync', { masterPassword });
      await refreshStatus();
      flashMessage('✓ 自动同步已启用');
      return true;
    } catch (error) {
      flashMessage(`✗ ${error?.message || error}`, true);
      return false;
    } finally {
      loading.value = false;
    }
  }

  /** 关闭自动同步：删除会话密钥。 */
  async function disableAutoSync() {
    if (loading.value) return false;
    loading.value = true;
    try {
      await invokeBackend('sync_disable_auto_sync');
      await refreshStatus();
      flashMessage('已关闭自动同步');
      return true;
    } catch (error) {
      flashMessage(`✗ ${error?.message || error}`, true);
      return false;
    } finally {
      loading.value = false;
    }
  }

  /**
   * 启动时探测远端是否有更新（轻量，只读 rev，不解密）。
   * 有更新时写 remoteHasUpdates=true 供 UI 显示徽章，并 announce 提示。
   */
  async function checkRemoteUpdates() {
    if (!isTauriRuntime() || !configured.value) return;
    try {
      const result = await invokeBackend('sync_check_remote_updates');
      remoteHasUpdates.value = Boolean(result.has_updates);
      if (result.has_updates) {
        // 经 workbench bridge announce（若已注入）
        if (workbenchBridge?.announce) {
          workbenchBridge.announce('远端 Gist 有更新，点击同步面板拉取最新');
        }
      }
    } catch (error) {
      // 探测失败不阻塞启动，静默
      // eslint-disable-next-line no-console
      console.warn('[sync] checkRemoteUpdates failed:', error?.message || error);
    }
  }

  /**
   * 资产写操作后自动推送（v1.6 核心：经 workbench bridge 由 assets store 调用）。
   *
   * - 仅在 autoSyncEnabled 且无 pending 操作时触发
   * - 走会话密钥路径（masterPassword 留空）
   * - **不弹窗、不阻塞**：失败仅静默 announce，不打断用户资产操作
   * - 防抖：若已在 loading，跳过（避免连续保存触发并发 push）
   */
  async function autoPushIfEnabled() {
    if (!isTauriRuntime()) return;
    if (!autoSyncEnabled.value) return;
    if (loading.value) return; // 防并发：上一次操作未完成
    if (conflict.value) return; // 有未解决冲突，不自动 push（避免覆盖）
    try {
      await invokeBackend('sync_push', { masterPassword: '' });
      await refreshStatus();
      remoteHasUpdates.value = false;
    } catch (error) {
      // 自动同步失败静默 announce，不打断用户（典型：冲突，让用户手动处理）
      if (workbenchBridge?.announce) {
        workbenchBridge.announce('自动同步失败：' + (error?.message || error) + '（请到同步面板处理）');
      }
    }
  }

  // ============================================================
  // 跨 store 桥接（lazy）—— assets store / workbench 调用
  // ============================================================
  let workbenchBridge = null;
  function attachWorkbench(bridge) {
    workbenchBridge = bridge;
  }

  function flashMessage(msg, isError = false) {
    lastMessage.value = msg;
    if (!isError) setTimeout(() => { if (lastMessage.value === msg) lastMessage.value = ''; }, 4000);
  }

  return {
    // state
    status, loading, lastMessage, conflict, remoteHasUpdates,
    // computed
    configured, patConfigured, lastSyncedAt, gistIdMasked, autoSyncEnabled, syncText,
    // bridge
    attachWorkbench,
    // actions
    refreshStatus, setup, push, pull, resolveConflict,
    resetMasterPassword, clearSync, dismissConflict,
    // v1.6 自动同步
    enableAutoSync, disableAutoSync, checkRemoteUpdates, autoPushIfEnabled
  };
});
