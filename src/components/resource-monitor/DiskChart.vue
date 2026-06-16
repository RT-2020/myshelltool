<script setup>
import { computed } from 'vue';
import { HardDrive } from 'lucide-vue-next';
import { CHART_H, CHART_W, formatBytes, formatRate, buildLinePath } from './chart-utils.js';

const props = defineProps({
  readPoints: { type: Array, default: () => [] },
  writePoints: { type: Array, default: () => [] },
  readRate: { type: Number, default: 0 },
  writeRate: { type: Number, default: 0 },
  // 根分区容量（来自 df，字节）
  diskTotal: { type: Number, default: 0 },
  diskUsed: { type: Number, default: 0 }
});

const allPoints = computed(() => [...props.readPoints, ...props.writePoints]);
const yMax = computed(() => {
  const m = allPoints.value.length ? Math.max(...allPoints.value) : 0;
  return m > 0 ? m : 1;
});
const readPath = computed(() => buildLinePath(props.readPoints, yMax.value));
const writePath = computed(() => buildLinePath(props.writePoints, yMax.value));

// 容量百分比 + 进度条宽度
const diskPct = computed(() => {
  if (!props.diskTotal) return 0;
  return Math.min(100, Math.round((props.diskUsed / props.diskTotal) * 100));
});
const hasCapacity = computed(() => props.diskTotal > 0);
</script>

<template>
  <section class="rm-chart rm-chart--disk">
    <header class="rm-chart-head">
      <span class="rm-chart-label"><HardDrive :size="12" /> 磁盘 I/O</span>
      <span class="rm-chart-value">
        <strong class="mono num rd">R{{ formatRate(readRate) }}</strong>
        <strong class="mono num wr">W{{ formatRate(writeRate) }}</strong>
      </span>
    </header>
    <svg :viewBox="`0 0 ${CHART_W} ${CHART_H}`" preserveAspectRatio="none" class="rm-chart-svg">
      <path v-if="readPath" :d="readPath" fill="none" stroke="var(--info, var(--accent))" stroke-width="1.2" />
      <path v-if="writePath" :d="writePath" fill="none" stroke="var(--warn)" stroke-width="1.2" />
    </svg>

    <!-- 根分区容量（来自 df）：used / total (pct%) + 进度条 -->
    <div v-if="hasCapacity" class="disk-capacity">
      <div class="disk-capacity-head">
        <span class="rm-chart-label">容量</span>
        <span class="mono num disk-capacity-text">
          {{ formatBytes(diskUsed) }} / {{ formatBytes(diskTotal) }}
          <span class="disk-capacity-pct" :class="{ 'is-high': diskPct >= 85, 'is-warn': diskPct >= 70 }">{{ diskPct }}%</span>
        </span>
      </div>
      <div class="disk-capacity-bar" role="progressbar" :aria-valuenow="diskPct" aria-valuemin="0" aria-valuemax="100">
        <div class="disk-capacity-fill" :class="{ 'is-high': diskPct >= 85, 'is-warn': diskPct >= 70 }" :style="{ width: diskPct + '%' }"></div>
      </div>
    </div>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.rm-chart {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  padding: var(--space-2) var(--space-3);
}
.rm-chart-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: var(--text-xs);
}
.rm-chart-label {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--app-muted);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}
.rm-chart-value {
  display: inline-flex;
  align-items: baseline;
  gap: 8px;
  color: var(--app-strong);
}
.rm-chart-value .rd { color: var(--info, var(--accent)); }
.rm-chart-value .wr { color: var(--warn); }
.rm-chart-svg {
  width: 100%;
  height: 60px;
  display: block;
}

// 容量行
.disk-capacity {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-block-start: 2px;
}
.disk-capacity-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: var(--text-xs);
}
.disk-capacity-text {
  color: var(--app-strong);
}
.disk-capacity-pct {
  margin-inline-start: 4px;
  color: var(--app-muted);
  &.is-warn { color: var(--warn); }
  &.is-high { color: var(--danger); }
}
.disk-capacity-bar {
  width: 100%;
  height: 4px;
  border-radius: var(--radius-pill);
  background: var(--app-control);
  overflow: hidden;
}
.disk-capacity-fill {
  height: 100%;
  border-radius: var(--radius-pill);
  background: var(--accent);
  transition: width var(--motion-base, 0.3s) var(--ease-standard);
  &.is-warn { background: var(--warn); }
  &.is-high { background: var(--danger); }
}
</style>
