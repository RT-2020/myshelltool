import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { invokeBackend, isTauriRuntime } from '../services/backend.js';

/**
 * useSyncStore — v1.3 Gist 同步前端状态 + actions。
 *
 * 对接 src-tauri/src/sync.rs 的 7 个命令。负责：
 * - 同步状态展示（sync_status → 状态栏真实同步状态）
 * - push/pull/冲突解决/重置密码/清空 的 action 封装
 * - 冲突暂存（pull 返回 Conflict 时存双方 JSON，供冲突框展示后选择）
 *
 * 与 assets store 的关系：assets.js 的 syncText/saveToken 仍负责 PAT 配置
 *（PAT 是同步的前置——没 PAT 不能同步）。本 store 负责 PAT 之后的同步逻辑。
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

  // ============================================================
  // Computed
  // ============================================================
  const configured = computed(() => Boolean(status.value?.configured));
  const patConfigured = computed(() => Boolean(status.value?.pat_configured));
  const lastSyncedAt = computed(() => status.value?.last_synced_at ?? null);
  const gistIdMasked = computed(() => status.value?.gist_id_masked ?? null);
  // 状态栏同步文案：优先真实同步状态，回退 PAT 配置状态
  const syncText = computed(() => {
    if (loading.value) return '同步中…';
    if (lastMessage.value) return lastMessage.value;
    if (configured.value) return '同步已配置';
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
      throw error;
    } finally {
      loading.value = false;
    }
  }

  /** 推送本地资产到 Gist。 */
  async function push(masterPassword) {
    if (loading.value) return null;
    loading.value = true;
    lastMessage.value = '';
    try {
      const result = await invokeBackend('sync_push', { masterPassword });
      await refreshStatus();
      flashMessage(result.message);
      return result;
    } catch (error) {
      flashMessage(`✗ 推送失败：${error?.message || error}`, true);
      throw error;
    } finally {
      loading.value = false;
    }
  }

  /**
   * 拉取 Gist + 冲突检测。
   * 返回 Conflict 时存入 conflict 暂存区（供冲突框展示）。
   * @returns {Promise<object>} SyncPullResult 的 decision
   */
  async function pull(masterPassword) {
    if (loading.value) return null;
    loading.value = true;
    lastMessage.value = '';
    try {
      const result = await invokeBackend('sync_pull', { masterPassword });
      await refreshStatus();
      switch (result.decision) {
        case 'NoChange':
          flashMessage('已是最新（双方都无变更）');
          break;
        case 'Pulled':
          flashMessage(`✓ 已拉取远端数据（rev ${result.new_rev}）`);
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
      throw error;
    } finally {
      loading.value = false;
    }
  }

  /**
   * 解决冲突（用户在冲突框选择后调）。
   * @param {string} choice 'local' | 'remote'
   */
  async function resolveConflict(masterPassword, choice) {
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
      throw error;
    } finally {
      loading.value = false;
    }
  }

  /** 重置主密码（需旧密码验证）。 */
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
      throw error;
    } finally {
      loading.value = false;
    }
  }

  /** 清空同步配置（忘了主密码的逃生口）。 */
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
      throw error;
    } finally {
      loading.value = false;
    }
  }

  /** 清空冲突暂存区（用户在冲突框点"取消"）。 */
  function dismissConflict() {
    conflict.value = null;
  }

  function flashMessage(msg, isError = false) {
    lastMessage.value = msg;
    if (!isError) setTimeout(() => { if (lastMessage.value === msg) lastMessage.value = ''; }, 4000);
  }

  return {
    // state
    status, loading, lastMessage, conflict,
    // computed
    configured, patConfigured, lastSyncedAt, gistIdMasked, syncText,
    // actions
    refreshStatus, setup, push, pull, resolveConflict,
    resetMasterPassword, clearSync, dismissConflict
  };
});
