<script setup>
import { computed, onBeforeUnmount, watch } from 'vue';
import { MonitorOff } from 'lucide-vue-next';
import { useResourceMonitorStore } from '@/stores/resourceMonitor.js';
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
</script>

<template>
  <section class="rs-section rm-section" data-region="resource-monitor">
    <div class="rs-section-head">
      <span class="rs-section-title">资源监控</span>
      <span class="rs-section-meta">{{ hasData ? '2秒 · 60点' : '— · —' }}</span>
    </div>

    <div v-if="placeholder" class="rs-empty-banner">
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

:deep(.metric-foot),
:deep(.mem-bar),
:deep(.disk-row) {
  display: none;
}
</style>
