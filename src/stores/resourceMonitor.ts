import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type { ResourceSnapshot } from '@/types/domain';
import { invokeBackend, isTauriRuntime, listenBackendEvent } from '../services/backend';

const MAX_HISTORY = 60;
const INTERVAL_MS = 2000;

/** 后端 resource-monitor-error 事件 payload（契约：与快照同机制按 sessionId 路由）。 */
interface MonitorErrorPayload {
  sessionId: string;
  reason: string;
}

// 供 UI（ResourceMonitorPanel 头部 meta）派生展示，避免硬编码「2秒 · 60点」
export const RESOURCE_MONITOR_INTERVAL_MS = INTERVAL_MS;
export const RESOURCE_MONITOR_MAX_HISTORY = MAX_HISTORY;

export const useResourceMonitorStore = defineStore('resourceMonitor', () => {
  const activeSessionId = ref<string | null>(null);
  const snapshot = ref<ResourceSnapshot | null>(null);
  const history = ref<ResourceSnapshot[]>([]);
  const enabled = ref(false);
  const error = ref<string | null>(null);
  // 后端推送的采样失败（resource-monitor-error 事件）：与 error（前端调用
  // 失败）区分，非空时面板展示错误态而非全 0 曲线
  const monitorError = ref<string | null>(null);
  // 快照降级提示（degraded 字段，如磁盘数据不可用）：非错误，数据继续展示
  const degradedNotice = ref<string | null>(null);

  let unlisten: TauriUnlistenFn | null = null;
  let errorUnlisten: TauriUnlistenFn | null = null;
  // 最近一次尝试采样的 sessionId：stop() 清空 activeSessionId 后 retry() 仍可重连
  let lastSessionId: string | null = null;
  const prevNetRx = ref(0);
  const prevNetTx = ref(0);
  const prevDiskRead = ref(0);
  const prevDiskWrite = ref(0);
  const prevTimestamp = ref(0);

  const isDesktopRuntime = computed(() => isTauriRuntime());

  function computeRate(cur: number, prev: number, dtMs: number) {
    if (!prev || !dtMs || dtMs <= 0) return 0;
    const delta = cur > prev ? cur - prev : 0;
    return Math.round((delta / dtMs) * 1000);
  }

  const netRxRate = computed(() => {
    const s = snapshot.value;
    if (!s) return 0;
    return computeRate(s.netRxBytes, prevNetRx.value, s.timestamp - prevTimestamp.value);
  });
  const netTxRate = computed(() => {
    const s = snapshot.value;
    if (!s) return 0;
    return computeRate(s.netTxBytes, prevNetTx.value, s.timestamp - prevTimestamp.value);
  });
  const diskReadRate = computed(() => {
    const s = snapshot.value;
    if (!s) return 0;
    return computeRate(s.diskReadBytes, prevDiskRead.value, s.timestamp - prevTimestamp.value);
  });
  const diskWriteRate = computed(() => {
    const s = snapshot.value;
    if (!s) return 0;
    return computeRate(s.diskWriteBytes, prevDiskWrite.value, s.timestamp - prevTimestamp.value);
  });

  const memUsedPct = computed(() => {
    const s = snapshot.value;
    if (!s || !s.memTotal) return 0;
    return Math.min(100, Math.max(0, (s.memUsed / s.memTotal) * 100));
  });

  const cpuHistoryPoints = computed(() => history.value.map(s => s.cpuUsage));
  const memHistoryPoints = computed(() => history.value.map(s => {
    if (!s.memTotal) return 0;
    return (s.memUsed / s.memTotal) * 100;
  }));
  const netRxHistoryPoints = computed(() => {
    const out: number[] = [];
    for (let i = 0; i < history.value.length; i += 1) {
      const s = history.value[i];
      const prev = i > 0 ? history.value[i - 1] : null;
      const dt = prev ? s.timestamp - prev.timestamp : 0;
      out.push(computeRate(s.netRxBytes, prev?.netRxBytes ?? 0, dt));
    }
    return out;
  });
  const netTxHistoryPoints = computed(() => {
    const out: number[] = [];
    for (let i = 0; i < history.value.length; i += 1) {
      const s = history.value[i];
      const prev = i > 0 ? history.value[i - 1] : null;
      const dt = prev ? s.timestamp - prev.timestamp : 0;
      out.push(computeRate(s.netTxBytes, prev?.netTxBytes ?? 0, dt));
    }
    return out;
  });
  const diskReadHistoryPoints = computed(() => {
    const out: number[] = [];
    for (let i = 0; i < history.value.length; i += 1) {
      const s = history.value[i];
      const prev = i > 0 ? history.value[i - 1] : null;
      const dt = prev ? s.timestamp - prev.timestamp : 0;
      out.push(computeRate(s.diskReadBytes, prev?.diskReadBytes ?? 0, dt));
    }
    return out;
  });
  const diskWriteHistoryPoints = computed(() => {
    const out: number[] = [];
    for (let i = 0; i < history.value.length; i += 1) {
      const s = history.value[i];
      const prev = i > 0 ? history.value[i - 1] : null;
      const dt = prev ? s.timestamp - prev.timestamp : 0;
      out.push(computeRate(s.diskWriteBytes, prev?.diskWriteBytes ?? 0, dt));
    }
    return out;
  });

  function applySnapshot(s: ResourceSnapshot | null | undefined) {
    // 跨窗口路由守卫：Rust emit 是全局广播（每个 WebviewWindow 都收到），
    // 只接受本窗口正在监控的会话快照（sessionId 为 ResourceSnapshot 经
    // rename_all=camelCase 后的字段名）；activeSessionId 为 null（stop 后
    // 残留事件）时一并拦截，防污染下一次 start 的历史。
    if (!s || !s.sessionId || s.sessionId !== activeSessionId.value) return;
    if (prevTimestamp.value) {
      prevNetRx.value = snapshot.value?.netRxBytes ?? 0;
      prevNetTx.value = snapshot.value?.netTxBytes ?? 0;
      prevDiskRead.value = snapshot.value?.diskReadBytes ?? 0;
      prevDiskWrite.value = snapshot.value?.diskWriteBytes ?? 0;
    } else {
      prevNetRx.value = s.netRxBytes;
      prevNetTx.value = s.netTxBytes;
      prevDiskRead.value = s.diskReadBytes;
      prevDiskWrite.value = s.diskWriteBytes;
    }
    prevTimestamp.value = s.timestamp;
    snapshot.value = s;
    // 降级提示（非错误）：随最新快照更新/清除，数据继续正常展示
    degradedNotice.value = s.degraded || null;
    history.value.push(s);
    if (history.value.length > MAX_HISTORY) history.value.shift();
  }

  async function start(sessionId: string, intervalMs = INTERVAL_MS) {
    if (!isTauriRuntime()) {
      enabled.value = false;
      activeSessionId.value = null;
      return;
    }
    if (!sessionId) {
      error.value = 'resourceMonitor.start: sessionId required';
      return;
    }
    lastSessionId = sessionId;
    if (activeSessionId.value === sessionId && enabled.value) return;
    if (activeSessionId.value && activeSessionId.value !== sessionId) {
      await stop().catch(() => {});
    }
    activeSessionId.value = sessionId;
    enabled.value = true;
    error.value = null;
    monitorError.value = null;
    degradedNotice.value = null;
    try {
      await invokeBackend('resource_monitor_start', { sessionId, intervalMs });
    } catch (e) {
      const msg = (e as Error | undefined)?.message || String(e);
      if (msg.includes('already monitored')) {
        // 跨窗口迁移竞态：Rust 侧仍有旧窗口（asset 独立窗口）的监控任务，
        // 先 stop 再重试（静默）。asset 窗口 onBeforeUnmount 的 stop 可能晚于主窗口 adopt。
        try {
          await invokeBackend('resource_monitor_stop', { sessionId });
          await invokeBackend('resource_monitor_start', { sessionId, intervalMs });
        } catch (e2) {
          error.value = (e2 as Error | undefined)?.message || String(e2);
          enabled.value = false;
        }
        return;
      }
      error.value = msg;
      enabled.value = false;
      return;
    }
    if (!unlisten) {
      try {
        unlisten = await listenBackendEvent('resource-monitor-snapshot', payload => {
          // 防御性解包：兼容 payload 直接是快照与被再包一层两种到达形状（原 JS 行为）
          const next = (payload?.payload || payload) as ResourceSnapshot | null | undefined;
          applySnapshot(next);
        });
      } catch (e) {
        error.value = (e as Error | undefined)?.message || String(e);
      }
    }
    if (!errorUnlisten) {
      try {
        // 后端采样失败事件（与快照同机制按 sessionId 路由）：置错误态并停止
        // 该会话的数据展示（清残留快照/历史，防图表渲染全 0 假曲线）。后端
        // 任务失败即终止，前端不再补发 resource_monitor_stop（retry 会走
        // stop→start 完整重启）。
        errorUnlisten = await listenBackendEvent('resource-monitor-error', payload => {
          const next = (payload?.payload || payload) as MonitorErrorPayload | null | undefined;
          if (!next?.sessionId || next.sessionId !== activeSessionId.value) return;
          monitorError.value = next.reason || '未知原因';
          snapshot.value = null;
          history.value = [];
          enabled.value = false;
        });
      } catch (e) {
        error.value = (e as Error | undefined)?.message || String(e);
      }
    }
  }

  async function stop() {
    if (!activeSessionId.value) return;
    const id = activeSessionId.value;
    try { await invokeBackend('resource_monitor_stop', { sessionId: id }); }
    catch { /* session may already be gone */ }
    if (unlisten) {
      try { unlisten(); } catch { /* noop */ }
      unlisten = null;
    }
    if (errorUnlisten) {
      try { errorUnlisten(); } catch { /* noop */ }
      errorUnlisten = null;
    }
    activeSessionId.value = null;
    snapshot.value = null;
    history.value = [];
    enabled.value = false;
    monitorError.value = null;
    degradedNotice.value = null;
    prevNetRx.value = 0;
    prevNetTx.value = 0;
    prevDiskRead.value = 0;
    prevDiskWrite.value = 0;
    prevTimestamp.value = 0;
  }

  async function snapshotOnce(sessionId: string): Promise<ResourceSnapshot | null> {
    if (!isTauriRuntime()) return null;
    return invokeBackend<ResourceSnapshot | null>('resource_monitor_snapshot', { sessionId });
  }

  // 重试最近一次采样：先干净停掉（含 unlisten/历史清零），再按 lastSessionId 重启。
  async function retry() {
    if (!lastSessionId) return;
    await stop().catch(() => {});
    await start(lastSessionId);
  }

  async function listActive(): Promise<unknown[]> {
    if (!isTauriRuntime()) return [];
    return invokeBackend<unknown[]>('resource_monitor_list_active');
  }

  function dispose() { return stop(); }

  return {
    activeSessionId,
    snapshot,
    history,
    enabled,
    error,
    monitorError,
    degradedNotice,
    isDesktopRuntime,
    netRxRate,
    netTxRate,
    diskReadRate,
    diskWriteRate,
    memUsedPct,
    cpuHistoryPoints,
    memHistoryPoints,
    netRxHistoryPoints,
    netTxHistoryPoints,
    diskReadHistoryPoints,
    diskWriteHistoryPoints,
    start,
    stop,
    retry,
    snapshotOnce,
    listActive,
    dispose,
    applySnapshot
  };
});
