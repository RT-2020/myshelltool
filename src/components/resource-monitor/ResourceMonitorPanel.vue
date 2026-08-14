<script setup>
import { computed, onBeforeUnmount, watch } from 'vue';
import { AlertTriangle, MonitorOff, Pause, Play } from 'lucide-vue-next';
import {
  useResourceMonitorStore,
  RESOURCE_MONITOR_INTERVAL_MS,
  RESOURCE_MONITOR_MAX_HISTORY
} from '@/stores/resourceMonitor.js';
import { useSessionsStore } from '@/stores/sessions.js';
import CpuChart from './CpuChart.vue';
import MemoryChart from './MemoryChart.vue';
import NetworkChart from './NetworkChart.vue';
import DiskChart from './DiskChart.vue';

const rm = useResourceMonitorStore();
const sessions = useSessionsStore();

watch(
  () => sessions.activeSessionId,
  async (newId, oldId) => {
    if (oldId && oldId !== newId) {
      await rm.stop().catch(() => {});
    }
    if (newId) {
      await rm.start(newId).catch(() => {});
    } else {
      await rm.stop().catch(() => {});
    }
  },
  { immediate: false }
);

onBeforeUnmount(() => {
  rm.stop().catch(() => {});
});

const placeholder = computed(() => {
  if (!rm.isDesktopRuntime) return 'desktop-required';
  if (!sessions.activeSessionId) return 'no-session';
  if (rm.error && !rm.snapshot) return 'error';
  if (!rm.snapshot) return 'waiting';
  return '';
});

const emptyText = computed(() => {
  if (placeholder.value === 'desktop-required') return '需要桌面端 · 监控待机';
  if (placeholder.value === 'waiting') return '等待首次采样 · 指标收集中';
  return '未连接到会话 · 采样待机';
});

const snapshot = computed(() => rm.snapshot);
const hasData = computed(() => Boolean(rm.snapshot));

// 头部 meta 从 store 常量派生（原硬编码「2秒 · 60点」）
const metaText = computed(() =>
  hasData.value
    ? `${RESOURCE_MONITOR_INTERVAL_MS / 1000}秒 · ${RESOURCE_MONITOR_MAX_HISTORY}点`
    : '— · —'
);

// 头部开始/停止：无活跃会话时禁用并说明原因
const toggleTitle = computed(() => {
  if (!sessions.activeSessionId) return '连接后可用';
  return rm.enabled ? '停止采样' : '开始采样';
});

async function onToggleMonitor() {
  if (rm.enabled) {
    await rm.stop().catch(() => {});
    return;
  }
  if (sessions.activeSessionId) {
    await rm.start(sessions.activeSessionId).catch(() => {});
  }
}

async function onRetry() {
  await rm.retry().catch(() => {});
}
</script>

<template>
  <section class="rs-section rm-section" data-region="resource-monitor">
    <div class="rs-section-head">
      <span class="rs-section-title">资源监控</span>
      <div class="rs-head-right">
        <span class="rs-section-meta">{{ metaText }}</span>
        <button
          type="button"
          class="rm-toggle"
          :title="toggleTitle"
          :aria-label="toggleTitle"
          :disabled="!sessions.activeSessionId"
          @click="onToggleMonitor"
        >
          <Pause v-if="rm.enabled" :size="13" />
          <Play v-else :size="13" />
        </button>
      </div>
    </div>

    <div v-if="placeholder === 'error'" class="rm-error-banner" role="alert">
      <AlertTriangle :size="14" />
      <div class="rm-error-body">
        <span class="rm-error-title">监控异常</span>
        <span class="rm-error-msg">{{ rm.error }}</span>
      </div>
      <button type="button" class="rm-retry-btn" @click="onRetry">重试</button>
    </div>

    <div v-else-if="placeholder" class="rs-empty-banner">
      <MonitorOff />
      <span>{{ emptyText }}</span>
    </div>

    <div class="metric-grid">
      <CpuChart
        :points="rm.cpuHistoryPoints"
        :current="snapshot?.cpuUsage || 0"
        :cores="snapshot?.cpuCores || 0"
        :has-data="hasData"
      />
      <MemoryChart
        :points="rm.memHistoryPoints"
        :mem-total="snapshot?.memTotal || 0"
        :mem-used="snapshot?.memUsed || 0"
        :has-data="hasData"
      />
      <NetworkChart
        :rx-points="rm.netRxHistoryPoints"
        :tx-points="rm.netTxHistoryPoints"
        :rx-rate="rm.netRxRate"
        :tx-rate="rm.netTxRate"
        :has-data="hasData"
      />
      <DiskChart
        :read-points="rm.diskReadHistoryPoints"
        :write-points="rm.diskWriteHistoryPoints"
        :read-rate="rm.diskReadRate"
        :write-rate="rm.diskWriteRate"
        :disk-total="snapshot?.diskTotal || 0"
        :disk-used="snapshot?.diskUsed || 0"
        :has-data="hasData"
      />
    </div>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.rs-section {
  padding: var(--space-4) var(--space-3) var(--space-3);
  border-bottom: 1px solid var(--app-border-soft);
}

.rs-section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  margin-bottom: var(--space-3);
}

.rs-section-title {
  color: var(--app-subtle);
  font: 500 10px var(--font-mono);
  letter-spacing: 0.08em;
}

.rs-section-meta {
  color: var(--app-subtle);
  font: 10px var(--font-mono);
  letter-spacing: 0.04em;
}

.rs-head-right {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
}

.rm-toggle {
  display: inline-grid;
  place-items: center;
  width: 22px;
  height: 22px;
  padding: 0;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
  color: var(--app-muted);
  cursor: pointer;
  transition: background var(--motion-fast), color var(--motion-fast), border-color var(--motion-fast);
}

.rm-toggle:hover:not(:disabled) {
  background: var(--app-hover);
  color: var(--app-text);
  border-color: var(--app-border-strong);
}

.rm-toggle:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

.rm-toggle:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.rm-error-banner {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-bottom: var(--space-3);
  padding: 8px 10px;
  border: 1px solid var(--danger-soft);
  border-radius: var(--radius-sm);
  background: color-mix(in oklab, var(--danger), transparent 92%);
  color: var(--danger);
  font: 11px var(--font-display);
}

.rm-error-banner svg {
  width: 14px;
  height: 14px;
  stroke-width: 1.6;
  flex-shrink: 0;
}

.rm-error-body {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.rm-error-title {
  font-weight: 600;
  color: var(--danger);
}

.rm-error-msg {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--app-muted);
  font: 10.5px var(--font-mono);
}

.rm-retry-btn {
  flex-shrink: 0;
  height: 24px;
  padding: 0 10px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
  color: var(--app-text);
  font: 500 11px var(--font-display);
  cursor: pointer;
}

.rm-retry-btn:hover {
  background: var(--app-hover);
  color: var(--app-strong);
}

.rm-retry-btn:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

.rs-empty-banner {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-bottom: var(--space-3);
  padding: 8px 10px;
  border: 1px dashed var(--app-border-strong);
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
  color: var(--app-subtle);
  font: 11px var(--font-display);
}

.rs-empty-banner svg {
  width: 14px;
  height: 14px;
  stroke-width: 1.6;
  flex-shrink: 0;
}

.metric-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}

:deep(.metric-card) {
  min-width: 0;
  padding: 10px 12px;
  border: 1px solid var(--app-border);
  border-radius: 7px;
  background: var(--app-panel);
}

:deep(.metric-head) {
  display: flex;
  min-width: 0;
  gap: 8px;
  margin-bottom: 8px;
}

:deep(.metric-name) {
  flex: 0 0 auto;
  font: 500 9.5px var(--font-display);
  letter-spacing: 0.06em;
}

:deep(.metric-value) {
  flex: 1 1 auto;
  justify-content: flex-end;
  min-width: 0;
  overflow: visible;
  font-size: 13px;
  font-variant-numeric: tabular-nums;
  letter-spacing: 0;
  text-align: right;
  text-overflow: clip;
  white-space: nowrap;
}

:deep(.metric-value.compact-rate) {
  font-size: 10px;
}

:deep(.metric-value.compact-rate .num) {
  font-size: inherit;
}

:deep(.spark) {
  height: 36px;
}

:deep(.spark.network) {
  height: 40px;
}
</style>
