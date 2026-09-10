import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import type { ModalState, NormalizedTunnelStatus } from '@/types/domain';
import {
  invokeBackend,
  normalizeTunnelConfig,
  normalizeTunnelStatus
} from '../services/backend';

/**
 * useTunnelsStore — Wave 2 Step 2.2
 *
 * 从 workbench.js 抽取隧道相关 state / actions / computed。
 *
 * 跨 store 桥接（lazy getter 注入）：
 *   - workbench.announce(message) / statusMessage
 *   - workbench.modal（createTunnel 完成后清空）
 *   - sessions.activeSession?.sessionId（lazy）
 */

/** tunnels store 实际消费的 workbench bridge 最小结构（勿耦合完整 workbench store 类型）。 */
interface TunnelsWorkbenchBridge {
  announce(message: string): unknown;
  modal: ModalState;
  sessionsStore(): { activeSession: { sessionId: string } | null } | null;
}

export const useTunnelsStore = defineStore('tunnels', () => {
  // ============================================================
  // State（原 workbench.js:52）
  // ============================================================
  const tunnels = ref<NormalizedTunnelStatus[]>([]);

  // ============================================================
  // 跨 store 桥接（lazy）
  // ============================================================
  let workbenchBridge: TunnelsWorkbenchBridge | null = null;
  function attachWorkbench(store: TunnelsWorkbenchBridge) {
    workbenchBridge = store;
  }
  function wb(): TunnelsWorkbenchBridge {
    if (!workbenchBridge) {
      throw new Error('tunnels store: workbench bridge not attached. Call tunnelsStore.attachWorkbench(workbenchStore) at App.vue init.');
    }
    return workbenchBridge;
  }
  function announce(message: string) {
    if (workbenchBridge && typeof workbenchBridge.announce === 'function') {
      return workbenchBridge.announce(message);
    }
    // eslint-disable-next-line no-console
    console.log('[tunnels] announce:', message);
  }

  // ============================================================
  // Computed（原 workbench.js:95）
  // ============================================================
  const runningTunnels = computed(() => tunnels.value.filter(tunnel => tunnel.active).length);

  // ============================================================
  // Actions
  // ============================================================
  async function refreshTunnels() {
    tunnels.value = (await invokeBackend<Record<string, unknown>[]>('tunnel_list')).map(normalizeTunnelStatus);
  }

  async function createTunnel(form: Record<string, unknown>) {
    const sessionsStore = wb().sessionsStore();
    const sessionId = sessionsStore?.activeSession?.sessionId || '';
    const config = normalizeTunnelConfig({
      id: 'tunnel-' + Date.now(),
      session_id: sessionId,
      ...form
    });
    await invokeBackend('tunnel_create', { config });
    if (config.auto_start) await invokeBackend('tunnel_start', { sessionId, tunnelId: config.id });
    await refreshTunnels();
    wb().modal = { type: null, asset: null };
    announce('隧道已创建：' + config.name);
  }

  async function toggleTunnel(tunnel: NormalizedTunnelStatus) {
    if (tunnel.active) await invokeBackend('tunnel_stop', { tunnelId: tunnel.id });
    else await invokeBackend('tunnel_start', { sessionId: tunnel.config.session_id, tunnelId: tunnel.id });
    await refreshTunnels();
    announce((tunnel.active ? '已停止：' : '已启动：') + tunnel.config.name);
  }

  async function toggleTunnelAutoStart(tunnel: NormalizedTunnelStatus) {
    const config = normalizeTunnelConfig({ ...tunnel.config, auto_start: !tunnel.config.auto_start });
    await invokeBackend('tunnel_delete', { tunnelId: tunnel.id });
    await invokeBackend('tunnel_create', { config });
    if (tunnel.active && config.auto_start) {
      await invokeBackend('tunnel_start', { sessionId: config.session_id, tunnelId: config.id });
    }
    await refreshTunnels();
    announce((config.auto_start ? '已启用自动启动：' : '已关闭自动启动：') + config.name);
  }

  async function deleteTunnel(tunnel: NormalizedTunnelStatus) {
    await invokeBackend('tunnel_delete', { tunnelId: tunnel.id });
    await refreshTunnels();
    announce('隧道已删除：' + tunnel.config.name);
  }

  return {
    // state
    tunnels,
    // computed
    runningTunnels,
    // bridge
    attachWorkbench,
    // actions
    refreshTunnels,
    createTunnel,
    toggleTunnel,
    toggleTunnelAutoStart,
    deleteTunnel
  };
});
