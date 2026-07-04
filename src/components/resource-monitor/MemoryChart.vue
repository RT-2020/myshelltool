<script setup>
import { computed } from 'vue';
import { CHART_H, CHART_W, buildLinePath, formatBytes } from './chart-utils.js';

const props = defineProps({
  points: { type: Array, default: () => [] },
  memTotal: { type: Number, default: 0 },
  memUsed: { type: Number, default: 0 },
  hasData: { type: Boolean, default: true }
});

const usedPct = computed(() => {
  if (!props.memTotal) return 0;
  return Math.min(100, Math.max(0, (props.memUsed / props.memTotal) * 100));
});
const path = computed(() => buildLinePath(props.points, 100));
const valueText = computed(() => {
  if (!props.hasData) return '—';
  if (!props.memTotal) return formatBytes(props.memUsed);
  return `${usedPct.value.toFixed(0)}%`;
});
const detailText = computed(() => (props.hasData ? `${formatBytes(props.memUsed)} / ${formatBytes(props.memTotal)}` : '暂无数据'));
</script>

<template>
  <article class="metric-card">
    <div class="metric-head">
      <span class="metric-name">内存</span>
      <span class="metric-value compact-percent" :title="detailText" :aria-label="detailText">
        <span class="num">{{ valueText }}</span>
      </span>
    </div>

    <svg class="spark" :viewBox="`0 0 ${CHART_W} ${CHART_H}`" preserveAspectRatio="none" aria-hidden="true">
      <line class="grid-line" x1="0" :y1="CHART_H * 0.25" :x2="CHART_W" :y2="CHART_H * 0.25" />
      <line class="grid-line" x1="0" :y1="CHART_H * 0.5" :x2="CHART_W" :y2="CHART_H * 0.5" />
      <line class="grid-line" x1="0" :y1="CHART_H * 0.75" :x2="CHART_W" :y2="CHART_H * 0.75" />
      <path v-if="hasData && path" :d="path" class="line-data" />
      <path v-else class="line-empty" :d="`M0,${CHART_H - 4} L${CHART_W},${CHART_H - 4}`" />
      <line class="baseline" x1="0" :y1="CHART_H - 1" :x2="CHART_W" :y2="CHART_H - 1" />
    </svg>

    <div class="mem-bar" role="progressbar" :aria-valuenow="usedPct.toFixed(0)" aria-valuemin="0" aria-valuemax="100">
      <div class="mem-bar-fill" :style="{ width: usedPct + '%' }"></div>
    </div>

    <div class="metric-foot">
      <span>{{ hasData ? `已用 ${formatBytes(memUsed)}` : '已用 —' }}</span>
      <span>{{ hasData ? `空闲 ${formatBytes(Math.max(0, memTotal - memUsed))}` : '空闲 —' }}</span>
    </div>
  </article>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.metric-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  margin-bottom: 6px;
}

.metric-name {
  color: var(--app-muted);
  font: 500 9.5px var(--font-mono);
  letter-spacing: 0.06em;
}

.metric-value {
  display: inline-flex;
  align-items: baseline;
  gap: 3px;
  color: var(--app-subtle);
  font: 500 13px var(--font-mono);
}

.metric-value .num { font-size: 14px; }

.spark {
  display: block;
  width: 100%;
  height: 42px;
}

.grid-line {
  stroke: var(--app-border-soft);
  stroke-dasharray: 2 3;
  stroke-width: 1;
}

.baseline {
  stroke: var(--app-border);
  stroke-width: 1;
}

.line-empty,
.line-data {
  fill: none;
  stroke-linecap: round;
  stroke-linejoin: round;
  stroke-width: 1.2;
}

.line-empty { stroke: var(--app-border-strong); }
.line-data { stroke: var(--success); }

.mem-bar {
  height: 3px;
  margin-top: 6px;
  overflow: hidden;
  border-radius: var(--radius-pill);
  background: var(--app-panel-2);
}

.mem-bar-fill {
  height: 100%;
  border-radius: inherit;
  background: var(--success);
}

.metric-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 6px;
  color: var(--app-subtle);
  font: 10px var(--font-mono);
  letter-spacing: 0.04em;
}
</style>
