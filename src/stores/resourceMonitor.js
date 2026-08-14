import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { invokeBackend, isTauriRuntime, listenBackendEvent } from '../services/backend.js';

const MAX_HISTORY = 60;
const INTERVAL_MS = 2000;

// 供 UI（ResourceMonitorPanel 头部 meta）派生展示，避免硬编码「2秒 · 60点」
export const RESOURCE_MONITOR_INTERVAL_MS = INTERVAL_MS;
export const RESOURCE_MONITOR_MAX_HISTORY = MAX_HISTORY;

export const useResourceMonitorStore = defineStore('resourceMonitor', () => {
  const activeSessionId = ref(null);
  const snapshot = ref(null);
  const history = ref([]);
  const enabled = ref(false);
  const error = ref(null);

  let unlisten = null;
  // 最近一次尝试采样的 sessionId：stop() 清空 activeSessionId 后 retry() 仍可重连
  let lastSessionId = null;
  const prevNetRx = ref(0);
  const prevNetTx = ref(0);
  const prevDiskRead = ref(0);
  const prevDiskWrite = ref(0);
  const prevTimestamp = ref(0);

  const isDesktopRuntime = computed(() => isTauriRuntime());

  function computeRate(cur, prev, dtMs) {
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
    const out = [];
    for (let i = 0; i < history.value.length; i += 1) {
      const s = history.value[i];
      const prev = i > 0 ? history.value[i - 1] : null;
      const dt = prev ? s.timestamp - prev.timestamp : 0;
      out.push(computeRate(s.netRxBytes, prev?.netRxBytes ?? 0, dt));
    }
    return out;
  });
  const netTxHistoryPoints = computed(() => {
    const out = [];
    for (let i = 0; i < history.value.length; i += 1) {
      const s = history.value[i];
      const prev = i > 0 ? history.value[i - 1] : null;
      const dt = prev ? s.timestamp - prev.timestamp : 0;
      out.push(computeRate(s.netTxBytes, prev?.netTxBytes ?? 0, dt));
    }
    return out;
  });
  const diskReadHistoryPoints = computed(() => {
    const out = [];
    for (let i = 0; i < history.value.length; i += 1) {
      const s = history.value[i];
      const prev = i > 0 ? history.value[i - 1] : null;
      const dt = prev ? s.timestamp - prev.timestamp : 0;
      out.push(computeRate(s.diskReadBytes, prev?.diskReadBytes ?? 0, dt));
    }
    return out;
  });
  const diskWriteHistoryPoints = computed(() => {
    const out = [];
    for (let i = 0; i < history.value.length; i += 1) {
      const s = history.value[i];
      const prev = i > 0 ? history.value[i - 1] : null;
      const dt = prev ? s.timestamp - prev.timestamp : 0;
      out.push(computeRate(s.diskWriteBytes, prev?.diskWriteBytes ?? 0, dt));
    }
    return out;
  });

  function applySnapshot(s) {
    if (!s) return;
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
    history.value.push(s);
    if (history.value.length > MAX_HISTORY) history.value.shift();
  }

  async function start(sessionId, intervalMs = INTERVAL_MS) {
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
    try {
      await invokeBackend('resource_monitor_start', { sessionId, intervalMs });
    } catch (e) {
      error.value = e?.message || String(e);
      enabled.value = false;
      return;
    }
    if (!unlisten) {
      try {
        unlisten = await listenBackendEvent('resource-monitor-snapshot', payload => {
          const next = payload?.payload || payload;
          applySnapshot(next);
        });
      } catch (e) {
        error.value = e?.message || String(e);
      }
    }
  }

  async function stop() {
    if (!activeSessionId.value) return;
    const id = activeSessionId.value;
    try { await invokeBackend('resource_monitor_stop', { sessionId: id }); }
    catch (e) { /* session may already be gone */ }
    if (unlisten) {
      try { unlisten(); } catch (e) { /* noop */ }
      unlisten = null;
    }
    activeSessionId.value = null;
    snapshot.value = null;
    history.value = [];
    enabled.value = false;
    prevNetRx.value = 0;
    prevNetTx.value = 0;
    prevDiskRead.value = 0;
    prevDiskWrite.value = 0;
    prevTimestamp.value = 0;
  }

  async function snapshotOnce(sessionId) {
    if (!isTauriRuntime()) return null;
    return invokeBackend('resource_monitor_snapshot', { sessionId });
  }

  // 重试最近一次采样：先干净停掉（含 unlisten/历史清零），再按 lastSessionId 重启。
  async function retry() {
    if (!lastSessionId) return;
    await stop().catch(() => {});
    await start(lastSessionId);
  }

  async function listActive() {
    if (!isTauriRuntime()) return [];
    return invokeBackend('resource_monitor_list_active');
  }

  function dispose() { return stop(); }

  return {
    activeSessionId,
    snapshot,
    history,
    enabled,
    error,
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
