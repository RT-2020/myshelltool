import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import {
  invokeBackend,
  normalizeTunnelConfig,
  normalizeTunnelStatus
} from '../services/backend.js';

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
export const useTunnelsStore = defineStore('tunnels', () => {
  // ============================================================
  // State（原 workbench.js:52）
  // ============================================================
  const tunnels = ref([]);

  // ============================================================
  // 跨 store 桥接（lazy）
  // ============================================================
  let workbenchBridge = null;
  function attachWorkbench(store) {
    workbenchBridge = store;
  }
  function wb() {
    if (!workbenchBridge) {
      throw new Error('tunnels store: workbench bridge not attached. Call tunnelsStore.attachWorkbench(workbenchStore) at App.vue init.');
    }
    return workbenchBridge;
  }
  function announce(message) {
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
    tunnels.value = (await invokeBackend('tunnel_list')).map(normalizeTunnelStatus);
  }

  async function createTunnel(form) {
    const sessionsStore = wb().sessionsStore();
    const sessionId = sessionsStore.activeSession?.sessionId || '';
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

  async function toggleTunnel(tunnel) {
    if (tunnel.active) await invokeBackend('tunnel_stop', { tunnelId: tunnel.id });
    else await invokeBackend('tunnel_start', { sessionId: tunnel.config.session_id, tunnelId: tunnel.id });
    await refreshTunnels();
    announce((tunnel.active ? '已停止：' : '已启动：') + tunnel.config.name);
  }

  async function toggleTunnelAutoStart(tunnel) {
    const config = normalizeTunnelConfig({ ...tunnel.config, auto_start: !tunnel.config.auto_start });
    await invokeBackend('tunnel_delete', { tunnelId: tunnel.id });
    await invokeBackend('tunnel_create', { config });
    if (tunnel.active && config.auto_start) {
      await invokeBackend('tunnel_start', { sessionId: config.session_id, tunnelId: config.id });
    }
    await refreshTunnels();
    announce((config.auto_start ? '已启用自动启动：' : '已关闭自动启动：') + config.name);
  }

  async function deleteTunnel(tunnel) {
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
