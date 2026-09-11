<script setup lang="ts">
import { computed, onBeforeUnmount, watch } from 'vue';
import { AlertTriangle, MonitorOff } from 'lucide-vue-next';
import { useResourceMonitorStore } from '@/stores/resourceMonitor';
import { useSessionsStore } from '@/stores/sessions';
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
  // 后端推送的采样失败（resource-monitor-error）：比前端调用失败更具体，优先展示
  if (rm.monitorError) return 'monitor-error';
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

async function onRetry() {
  await rm.retry().catch(() => {});
}
</script>

<template>
  <section class="rs-section rm-section" data-region="resource-monitor">
    <div v-if="placeholder === 'monitor-error'" class="rm-error-banner" role="alert">
      <AlertTriangle :size="14" />
      <div class="rm-error-body">
        <span class="rm-error-title">资源监控不可用</span>
        <span class="rm-error-msg">{{ rm.monitorError }}</span>
      </div>
      <button type="button" class="rm-retry-btn" @click="onRetry">重试</button>
    </div>

    <div v-else-if="placeholder === 'error'" class="rm-error-banner" role="alert">
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

    <div v-if="!placeholder && rm.degradedNotice" class="rm-degraded-note" role="status">
      部分指标不可用：{{ rm.degradedNotice }}
    </div>

    <div class="metric-grid">
      <CpuChart
        :points="rm.cpuHistoryPoints"
        :current="snapshot?.cpuUsage || 0"
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
        :disks="snapshot?.disks || []"
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

// 降级轻提示（快照 degraded，如磁盘数据不可用）：非错误，指标继续展示
.rm-degraded-note {
  margin-bottom: var(--space-3);
  padding: 6px 10px;
  border: 1px solid var(--app-border-soft);
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
  color: var(--app-muted);
  font: 10.5px var(--font-display);
}

.rs-empty-banner svg {
  width: 14px;
  height: 14px;
  stroke-width: 1.6;
  flex-shrink: 0;
}

// 纵排单列仪表带（参考 FinalShell 信息层级 × 本项目 token 纪律）：
// 右栏窄（~280px），2×2 网格把曲线压到 ~120px 宽；单列让每张图全宽。
// 区块不再各自带卡框——直接坐在右栏 panel 底上，hairline 分隔（见 .metric-row）。
// head/读数/图高的样式回归各 Chart 组件自治，这里只管布局与分隔。
.metric-grid {
  display: flex;
  flex-direction: column;
  gap: 0;
}

:deep(.metric-card) {
  min-width: 0;
  padding: 10px 2px 12px;
  border: none;
  border-bottom: 1px solid var(--app-border-soft);
  border-radius: 0;
  background: transparent;
}

:deep(.metric-card:last-child) {
  border-bottom: none;
  padding-bottom: 2px;
}

:deep(.spark) {
  height: 44px;
}

:deep(.spark.network) {
  height: 48px;
}
</style>
