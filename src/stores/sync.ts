import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import type {
  RemoteUpdateStatus,
  SyncConflictStash,
  SyncPullResult,
  SyncPushResult,
  SyncSetupResult,
  SyncStatusResult
} from '@/types/domain';
import { invokeBackend, isTauriRuntime } from '../services/backend';

/** sync store 实际消费的 workbench bridge 最小结构。
 *  assetsStore：拉取/冲突解决后必须重载前端资产内存态（见 reloadAssetsFromBackend）。 */
interface SyncWorkbenchBridge {
  announce?(message: string): unknown;
  assetsStore?(): { reloadAssets(): Promise<void> } | null;
}

/**
 * useSyncStore — v1.3 Gist 同步前端状态 + actions（v1.6 增自动同步）。
 *
 * 对接 src-tauri/src/sync.rs 的 10 个命令（v1.6 +3：auto_sync 三件套）。负责：
 * - 同步状态展示（sync_status → 状态栏真实同步状态）
 * - push/pull/冲突解决/重置密码/清空 的 action 封装
 * - 冲突暂存（pull 返回 Conflict 时存双方 JSON，供冲突框展示后选择）
 * - v1.6 自动同步：enable/disable + 会话密钥路径 push/pull（无需主密码）+ 远端更新探测
 *
 * 与 assets store 的关系：assets 的写操作完成后调 maybeAutoPush（经 workbench bridge），
 * 触发本 store 的 autoPushIfEnabled —— 若 autoSyncEnabled 则后台 push（不弹窗，失败静默 announce）。
 */
export const useSyncStore = defineStore('sync', () => {
  // ============================================================
  // State
  // ============================================================
  const status = ref<SyncStatusResult | null>(null); // sync_status 返回
  const loading = ref(false); // 操作进行中（防重复）
  /**
   * 【v2.7】当前在跑的是哪个动作（'push' | 'pull' | null）。
   * 面板据此把进行方向画出来（rail 上箭头朝外/朝内）并给对应按钮"推送中…"文案 ——
   * 只有一个 loading 布尔值时，界面只能含糊地说「同步中…」，用户看不出数据在往哪走。
   */
  const activeOp = ref<'push' | 'pull' | null>(null);
  const lastMessage = ref(''); // 最近操作结果文案（成功/失败）
  // 冲突暂存：pull 返回 Conflict 时存，冲突框据此展示 + 用户选择后清空
  const conflict = ref<SyncConflictStash | null>(null);
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
  // 【v2.7】本机是否有未推送改动（后端保守判定）——面板据此决定"该按哪个按钮"
  const localHasChanges = computed(() => Boolean(status.value?.local_has_changes));
  // 【v2.7】本机是否保存了自动生成的恢复密码（用户手输的密码一律不保存）
  const recoveryPasswordSaved = computed(() => Boolean(status.value?.recovery_password_saved));
  // 是否同步凭据与私钥（默认 true）
  const syncCredentialsEnabled = computed(() => status.value?.sync_credentials ?? true);
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

  /**
   * 后端已把 connection-assets.json 整体改写（pull / 远端覆盖本地冲突 / setup 拉取
   * 远端）后，重载前端资产内存态——**后端写盘 ≠ 前端状态已同步**。
   *
   * 不重载的后果不只是「列表显示旧数据」：uniqueAssetId 在旧列表里查重，会生成
   * 一个磁盘上已存在的 id，随后的 save_connection_asset upsert 会静默覆盖刚拉取
   * 来的资产（主机/用户名被改，凭据引用也可能错位）。
   *
   * 桥未注入时（单元测试/启动序列未完成）不静默吞：打日志留痕，且不假装成功。
   */
  async function reloadAssetsFromBackend() {
    const assetsStore = workbenchBridge?.assetsStore?.();
    if (!assetsStore) {
      // 不静默吞：toast 已说「拉取成功」而列表是旧的，正是本轮要修的缺陷形态
      flashMessage('✗ 已拉取远端数据，但资产列表未重载（assets bridge 未注入），请重新打开应用', true);
      return;
    }
    try {
      await assetsStore.reloadAssets();
    } catch (error) {
      flashMessage(`✗ 已拉取远端数据，但本地资产列表重载失败：${(error as Error | undefined)?.message || error}`, true);
    }
  }

  /** 刷新同步状态（sync_status）。浏览器预览模式静默跳过。 */
  async function refreshStatus() {
    if (!isTauriRuntime()) return;
    try {
      status.value = await invokeBackend<SyncStatusResult>('sync_status');
    } catch (error) {
      // eslint-disable-next-line no-console
      console.warn('[sync] refreshStatus failed:', (error as Error | undefined)?.message || error);
    }
  }

  /**
   * 首次设置同步。
   * @param masterPassword 主密码
   * @param gistId 可选已有 gist_id（换机器场景）
   * @returns SyncSetupResult（Created/PulledRemote/AlreadyConfigured）；loading 中或失败返回 null
   */
  async function setup(masterPassword: string, gistId?: string): Promise<SyncSetupResult | null> {
    if (loading.value) return null;
    loading.value = true;
    lastMessage.value = '';
    try {
      const result = await invokeBackend<SyncSetupResult>('sync_setup', {
        masterPassword,
        gistId: gistId || null
      });
      await refreshStatus();
      // PulledRemote：后端已用远端数据覆盖本地 connection-assets.json，必须重载前端列表
      if (result.kind === 'PulledRemote') await reloadAssetsFromBackend();
      flashMessage(result.kind === 'Created' ? '✓ 同步已配置（新 Gist）'
        : result.kind === 'PulledRemote' ? '✓ 已拉取远端数据'
        : '同步已配置，无需重复设置');
      return result;
    } catch (error) {
      flashMessage(`✗ ${(error as Error | undefined)?.message || error}`, true);
      return null;
    } finally {
      loading.value = false;
    }
  }

  /**
   * 推送本地资产到 Gist。
   * @param masterPassword 主密码。v1.6：留空时走会话密钥路径（自动同步）。
   */
  async function push(masterPassword = ''): Promise<SyncPushResult | null> {
    if (loading.value) return null;
    loading.value = true;
    activeOp.value = 'push';
    lastMessage.value = '';
    try {
      const result = await invokeBackend<SyncPushResult>('sync_push', { masterPassword });
      await refreshStatus();
      flashMessage(result.message);
      // v1.6：push 成功后远端已是最新的，清除更新提示
      remoteHasUpdates.value = false;
      return result;
    } catch (error) {
      flashMessage(`✗ 推送失败：${(error as Error | undefined)?.message || error}`, true);
      return null;
    } finally {
      loading.value = false;
      activeOp.value = null;
    }
  }

  /**
   * 拉取 Gist + 冲突检测。
   * @param masterPassword 主密码。v1.6：留空时走会话密钥路径。
   * 返回 Conflict 时存入 conflict 暂存区（供冲突框展示）。
   * @returns SyncPullResult 的 decision；loading 中或失败返回 null
   */
  async function pull(masterPassword = ''): Promise<SyncPullResult | null> {
    if (loading.value) return null;
    loading.value = true;
    activeOp.value = 'pull';
    lastMessage.value = '';
    try {
      const result = await invokeBackend<SyncPullResult>('sync_pull', { masterPassword });
      await refreshStatus();
      switch (result.decision) {
        case 'NoChange':
          flashMessage('已是最新（双方都无变更）');
          remoteHasUpdates.value = false;
          break;
        case 'Pulled':
          // 后端已写盘（connection-assets.json = 远端数据），必须重载前端内存态，
          // 否则「✓ 已拉取」toast 与陈旧列表并存，后续 upsert 会覆盖刚拉取的资产
          await reloadAssetsFromBackend();
          // 凭据未完全恢复时不能只说「已拉取」：用户会以为可以直接连，
          // 实际连接以「认证失败」表现（后端已在日志写明是哪几项）。
          if (result.credentials_failed) {
            flashMessage(
              `✓ 已拉取远端数据（rev ${result.new_rev}），但 ${result.credentials_failed} 项凭据未能恢复到本机（这些资产连接时会认证失败，请重新输入密码）`,
              true
            );
          } else {
            flashMessage(`✓ 已拉取远端数据（rev ${result.new_rev}）`);
          }
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
      flashMessage(`✗ 拉取失败：${(error as Error | undefined)?.message || error}`, true);
      return null;
    } finally {
      loading.value = false;
      activeOp.value = null;
    }
  }

  /**
   * 解决冲突（用户在冲突框选择后调）。
   * @param masterPassword 主密码。v1.6：留空时走会话密钥路径。
   * @param choice 'local' | 'remote'
   */
  async function resolveConflict(masterPassword = '', choice: string) {
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
      // 选「远端覆盖本地」时后端已改写 connection-assets.json：必须重载前端列表。
      // （choice='local' 时本地数据未变，重载是幂等的，不做分支以免漏掉未来语义变化）
      await reloadAssetsFromBackend();
      flashMessage(choice === 'local' ? '✓ 已用本地覆盖远端' : '✓ 已用远端覆盖本地');
    } catch (error) {
      flashMessage(`✗ 冲突解决失败：${(error as Error | undefined)?.message || error}`, true);
      return;
    } finally {
      loading.value = false;
    }
  }

  /** 重置主密码（需旧密码验证）。重置后若已启用自动同步，会话密钥自动重新派生。 */
  async function resetMasterPassword(oldPassword: string, newPassword: string) {
    if (loading.value) return;
    loading.value = true;
    try {
      await invokeBackend('sync_reset_master_password', {
        oldPassword,
        newPassword
      });
      flashMessage('✓ 主密码已重置');
    } catch (error) {
      flashMessage(`✗ ${(error as Error | undefined)?.message || error}`, true);
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
      flashMessage(`✗ ${(error as Error | undefined)?.message || error}`, true);
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
  // v1.6 自动同步 actions（恢复密码的生成/查看在 useSyncRecoveryPassword：
  // 那两个动作只服务首次设置与换机恢复两处界面，且要保证明文不常驻 store）
  // ============================================================

  /**
   * 启用自动同步：验证主密码 → 派生会话密钥 → DPAPI 加密存盘。
   *
   * 【v2.7】启用成功后**立即用会话密钥推送一次**：把远端载荷升级成「带派生 salt」的新格式，
   * 主密码于是能在任何机器上恢复这份备份（v2.7 之前自动同步推的载荷 salt 缺省，只有
   * 原机那个 Windows 用户能解 —— 备份实为不可恢复）。推送失败**不阻断启用**（可稍后在
   * 「立即同步」里手动推），但必须如实告知，不能让「已启用自动同步」掩盖一份解不开的备份。
   *
   * @param masterPassword 主密码（一次性，验证后派生密钥即丢弃）
   * @returns 是否成功
   */
  async function enableAutoSync(masterPassword: string) {
    if (loading.value) return false;
    loading.value = true;
    lastMessage.value = '';
    try {
      await invokeBackend('sync_enable_auto_sync', { masterPassword });
      await refreshStatus();
      let note = '';
      if (configured.value) {
        try {
          // 不调 push()：loading 已置位，push() 会在「操作进行中」守卫处直接返回 null
          await invokeBackend('sync_push', { masterPassword: '' });
          await refreshStatus();
          remoteHasUpdates.value = false;
          note = '，并已把云端备份升级为主密码可恢复的格式';
        } catch (error) {
          note = `；但升级云端备份失败（${(error as Error | undefined)?.message || error}），请稍后在「立即同步」里手动推送一次`;
        }
      }
      flashMessage('✓ 自动同步已启用' + note);
      return true;
    } catch (error) {
      flashMessage(`✗ ${(error as Error | undefined)?.message || error}`, true);
      return false;
    } finally {
      loading.value = false;
    }
  }

  // ============================================================
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
      flashMessage(`✗ ${(error as Error | undefined)?.message || error}`, true);
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
      const result = await invokeBackend<RemoteUpdateStatus>('sync_check_remote_updates');
      remoteHasUpdates.value = Boolean(result.has_updates);
      if (result.has_updates) {
        // 经 workbench bridge announce（若已注入）
        workbenchBridge?.announce?.('远端 Gist 有更新，点击同步面板拉取最新');
      }
    } catch (error) {
      // 探测失败不阻塞启动，静默
      // eslint-disable-next-line no-console
      console.warn('[sync] checkRemoteUpdates failed:', (error as Error | undefined)?.message || error);
    }
  }

  /**
   * 资产写操作后自动推送（v1.6 核心：经 workbench bridge 由 assets store 调用）。
   *
   * - 仅在 autoSyncEnabled 时触发；走会话密钥路径（masterPassword 留空）
   * - **不弹窗、不阻塞**：不 await（调用方 fire-and-forget），不打断用户资产操作
   * - 串行化（enqueueAutoPush）：并发 push 会让后端各自 load_sync_state 算出**同一个
   *   new_rev**（sync.rs 无 CAS，Gist PATCH 也无），晚到的旧载荷覆盖新载荷 → 远端
   *   少一次保存，而本地 local_rev/last_synced_at 已记为「已同步」，换机器也看不出冲突。
   *   因此自动 push 绝不并发：在途时只置 pending 标记，当前 push 结束后重放一次
   *   （合并多次资产写为一次 push）。
   */
  async function autoPushIfEnabled() {
    if (!isTauriRuntime()) return;
    if (!autoSyncEnabled.value) return;
    if (conflict.value) return; // 有未解决冲突，不自动 push（避免覆盖用户待决策的数据）
    enqueueAutoPush();
  }

  /** auto-push 在途标记 + 重放标记（本 store 内串行队列，见 autoPushIfEnabled 注释）。 */
  let autoPushInFlight = false;
  let autoPushPending = false;

  function enqueueAutoPush() {
    if (autoPushInFlight) {
      // 合并：在途 push 结束后重放一次，不并发、也不静默丢弃
      autoPushPending = true;
      return;
    }
    autoPushInFlight = true;
    void (async () => {
      try {
        let rerun = true;
        while (rerun) {
          rerun = false;
          try {
            await invokeBackend('sync_push', { masterPassword: '' });
            await refreshStatus();
            remoteHasUpdates.value = false;
          } catch (error) {
            // 自动同步失败 announce，不打断用户（典型：冲突，让用户手动处理）
            workbenchBridge?.announce?.('自动同步失败：' + ((error as Error | undefined)?.message || error) + '（请到同步面板处理）');
          }
          if (autoPushPending) { autoPushPending = false; rerun = true; }
        }
      } finally {
        autoPushInFlight = false;
      }
    })();
  }

  // ============================================================
  // 跨 store 桥接（lazy）—— assets store / workbench 调用
  // ============================================================
  let workbenchBridge: SyncWorkbenchBridge | null = null;
  function attachWorkbench(bridge: SyncWorkbenchBridge) {
    workbenchBridge = bridge;
  }

  function flashMessage(msg: string, isError = false) {
    lastMessage.value = msg;
    if (!isError) setTimeout(() => { if (lastMessage.value === msg) lastMessage.value = ''; }, 4000);
  }

  /**
   * 切换是否同步凭据与托管私钥。
   * @param enabled 是否开启
   */
  async function setSyncCredentialsEnabled(enabled: boolean) {
    if (!isTauriRuntime()) return;
    try {
      await invokeBackend('sync_set_credentials_enabled', { enabled });
      await refreshStatus();
      flashMessage(enabled ? '✓ 已开启凭据与私钥同步' : '已关闭凭据与私钥同步（仅同步资产元数据）');
    } catch (error) {
      flashMessage(`✗ 操作失败：${(error as Error | undefined)?.message || error}`, true);
    }
  }

  return {
    // state
    status, loading, lastMessage, conflict, remoteHasUpdates, activeOp,
    // computed
    configured, patConfigured, lastSyncedAt, gistIdMasked, autoSyncEnabled, localHasChanges, recoveryPasswordSaved, syncCredentialsEnabled, syncText,
    // bridge
    attachWorkbench,
    // actions
    refreshStatus, setup, push, pull, resolveConflict,
    resetMasterPassword, clearSync, dismissConflict,
    // v1.6 自动同步
    enableAutoSync, disableAutoSync, checkRemoteUpdates, autoPushIfEnabled,
    // 凭据同步开关
    setSyncCredentialsEnabled
  };
});
