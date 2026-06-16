<script setup>
import { computed, watch, onBeforeUnmount } from 'vue';
import { Activity, Cpu, MonitorOff } from 'lucide-vue-next';
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

onBeforeUnmount(() => { rm.stop().catch(() => {}); });

const showPlaceholder = computed(() => {
  if (!rm.isDesktopRuntime) return 'desktop-required';
  if (!sessions.activeSessionId) return 'no-session';
  if (!rm.snapshot) return 'waiting';
  return '';
});

const snapshot = computed(() => rm.snapshot);
</script>

<template>
  <section class="rm-panel" data-region="resource-monitor">
    <header class="rm-panel-head">
      <span class="rm-panel-title"><Activity :size="14" /> 服务器资源监控</span>
    </header>

    <div v-if="showPlaceholder === 'desktop-required'" class="rm-empty">
      <MonitorOff :size="32" />
      <strong>需要桌面端</strong>
      <p class="muted">资源监控仅在 Tauri 桌面运行时可用，请运行 <code>npm run tauri:dev</code>。</p>
    </div>

    <div v-else-if="showPlaceholder === 'no-session'" class="rm-empty">
      <Cpu :size="32" />
      <strong>未连接 SSH 主机</strong>
      <p class="muted">从左侧资产树选择主机并连接，即可查看实时 CPU / 内存 / 网络 / 磁盘。</p>
    </div>

    <div v-else-if="showPlaceholder === 'waiting'" class="rm-empty rm-empty--sm">
      <Activity :size="20" />
      <span class="muted">等待采样数据…</span>
    </div>

    <div v-else class="rm-panel-body">
      <CpuChart
        :points="rm.cpuHistoryPoints"
        :current="snapshot?.cpuUsage || 0"
        :cores="snapshot?.cpuCores || 0"
      />
      <MemoryChart
        :points="rm.memHistoryPoints"
        :mem-total="snapshot?.memTotal || 0"
        :mem-used="snapshot?.memUsed || 0"
      />
      <NetworkChart
        :rx-points="rm.netRxHistoryPoints"
        :tx-points="rm.netTxHistoryPoints"
        :rx-rate="rm.netRxRate"
        :tx-rate="rm.netTxRate"
      />
      <DiskChart
        :read-points="rm.diskReadHistoryPoints"
        :write-points="rm.diskWriteHistoryPoints"
        :read-rate="rm.diskReadRate"
        :write-rate="rm.diskWriteRate"
        :disk-total="snapshot?.diskTotal || 0"
        :disk-used="snapshot?.diskUsed || 0"
      />
    </div>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.rm-panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1 1 60%;
  border-block-end: 1px solid var(--app-border);
  overflow: hidden;
}
.rm-panel-head {
  display: flex;
  align-items: center;
  flex: 0 0 auto; // 固定 header，body 滚动时不被压缩
  padding: var(--space-2) var(--space-3);
  border-block-end: 1px solid var(--app-border);
  background: var(--app-chrome);
}
.rm-panel-title {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-xs);
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--app-muted);
}
.rm-panel-body {
  flex: 1;
  overflow-y: auto;
}
.rm-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  padding: var(--space-4);
  text-align: center;
  color: var(--app-muted);
}
.rm-empty--sm {
  flex-direction: row;
  padding: var(--space-3);
  font-size: var(--text-xs);
}
.rm-empty strong {
  color: var(--app-strong);
  font-size: var(--text-sm);
}
.rm-empty code {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  padding: 2px 6px;
  background: var(--app-subtle);
  border-radius: var(--radius-sm);
}
.rm-empty .muted {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.5;
}
</style>
