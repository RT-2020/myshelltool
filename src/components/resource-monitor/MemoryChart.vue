<script setup lang="ts">
import { computed } from 'vue';
import { CHART_H, CHART_W, buildAreaPath, buildLinePath, formatBytes } from './chart-utils';

const props = withDefaults(defineProps<{
  points?: number[];
  memTotal?: number;
  memUsed?: number;
  hasData?: boolean;
}>(), {
  points: () => [],
  memTotal: 0,
  memUsed: 0,
  hasData: true
});

const GRAD_ID = 'rm-mem-grad';
const usedPct = computed(() => {
  if (!props.memTotal) return 0;
  return Math.min(100, Math.max(0, (props.memUsed / props.memTotal) * 100));
});
const path = computed(() => buildLinePath(props.points, 100));
const areaPath = computed(() => buildAreaPath(path.value));
const valueText = computed(() => {
  if (!props.hasData) return '—';
  if (!props.memTotal) return formatBytes(props.memUsed);
  return usedPct.value.toFixed(1);
});
const capacityText = computed(() => (props.hasData ? `${formatBytes(props.memUsed)} / ${formatBytes(props.memTotal)}` : '暂无数据'));
</script>

<template>
  <article class="metric-card">
    <div class="metric-head">
      <span class="metric-name">内存</span>
      <span class="metric-readout" :title="capacityText" :aria-label="capacityText">
        <span class="num">{{ valueText }}</span>
        <span v-if="hasData && memTotal" class="unit">%</span>
      </span>
    </div>

    <svg class="spark" :viewBox="`0 0 ${CHART_W} ${CHART_H}`" preserveAspectRatio="none" aria-hidden="true">
      <defs>
        <linearGradient :id="GRAD_ID" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stop-color="var(--success)" stop-opacity="0.3" />
          <stop offset="100%" stop-color="var(--success)" stop-opacity="0" />
        </linearGradient>
      </defs>
      <line class="grid-line" x1="0" :y1="CHART_H * 0.25" :x2="CHART_W" :y2="CHART_H * 0.25" />
      <line class="grid-line" x1="0" :y1="CHART_H * 0.5" :x2="CHART_W" :y2="CHART_H * 0.5" />
      <line class="grid-line" x1="0" :y1="CHART_H * 0.75" :x2="CHART_W" :y2="CHART_H * 0.75" />
      <path v-if="hasData && areaPath" :d="areaPath" :fill="`url(#${GRAD_ID})`" stroke="none" />
      <path v-if="hasData && path" :d="path" class="line-data" />
      <path v-else class="line-empty" :d="`M0,${CHART_H - 4} L${CHART_W},${CHART_H - 4}`" />
      <line class="baseline" x1="0" :y1="CHART_H - 1" :x2="CHART_W" :y2="CHART_H - 1" />
    </svg>

    <!-- 容量读数（已用/总量）：取代旧 mem-bar——面积图画的正是同一百分比序列，进度条成了冗余 -->
    <div class="metric-foot">{{ capacityText }}</div>
  </article>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.metric-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
  min-width: 0;
  margin-bottom: 6px;
}

.metric-name {
  flex: 0 0 auto;
  color: var(--app-muted);
  font: 500 9.5px var(--font-mono);
  letter-spacing: 0.06em;
}

.metric-readout {
  display: inline-flex;
  align-items: baseline;
  gap: 2px;
  min-width: 0;
  color: var(--app-strong);
  font: 500 16px/1 var(--font-mono);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.metric-readout .unit {
  color: var(--app-subtle);
  font-size: 10.5px;
}

.spark {
  display: block;
  width: 100%;
  height: 44px;
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

.metric-foot {
  margin-top: 4px;
  color: var(--app-muted);
  font: 10px var(--font-mono);
  font-variant-numeric: tabular-nums;
  text-align: right;
}
</style>
