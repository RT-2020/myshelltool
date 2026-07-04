<script setup>
import { computed } from 'vue';
import { CHART_H, CHART_W, buildAreaPath, buildLinePath } from './chart-utils.js';

const props = defineProps({
  points: { type: Array, default: () => [] },
  current: { type: Number, default: 0 },
  cores: { type: Number, default: 0 },
  hasData: { type: Boolean, default: true }
});

const GRAD_ID = 'rm-cpu-grad';
const path = computed(() => buildLinePath(props.points, 100));
const areaPath = computed(() => buildAreaPath(path.value));
const valueText = computed(() => (props.hasData ? props.current.toFixed(1) : '—'));
const footText = computed(() => `${props.cores || 0} 核 · 负载 0`);
</script>

<template>
  <article class="metric-card">
    <div class="metric-head">
      <span class="metric-name">CPU</span>
      <span class="metric-value">
        <span class="num">{{ valueText }}</span>
        <span v-if="hasData" class="unit">%</span>
      </span>
    </div>

    <svg class="spark" :viewBox="`0 0 ${CHART_W} ${CHART_H}`" preserveAspectRatio="none" aria-hidden="true">
      <defs>
        <linearGradient :id="GRAD_ID" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stop-color="var(--accent)" stop-opacity="0.35" />
          <stop offset="100%" stop-color="var(--accent)" stop-opacity="0" />
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

    <div class="metric-foot">
      <span>{{ footText }}</span>
      <span>2秒 · 60点</span>
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
.metric-value .unit {
  color: var(--app-subtle);
  font-size: 10.5px;
}

.spark {
  display: block;
  width: 100%;
  height: 48px;
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

.line-empty {
  fill: none;
  stroke: var(--app-border-strong);
  stroke-linecap: round;
  stroke-width: 1.2;
}

.line-data {
  fill: none;
  stroke: var(--accent);
  stroke-linecap: round;
  stroke-linejoin: round;
  stroke-width: 1.2;
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
