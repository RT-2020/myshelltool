<script setup>
import { computed } from 'vue';
import { CHART_H, CHART_W, buildLinePath, formatCompactRate, formatRate } from './chart-utils.js';

const props = defineProps({
  rxPoints: { type: Array, default: () => [] },
  txPoints: { type: Array, default: () => [] },
  rxRate: { type: Number, default: 0 },
  txRate: { type: Number, default: 0 },
  hasData: { type: Boolean, default: true }
});

const allPoints = computed(() => [...props.rxPoints, ...props.txPoints]);
const yMax = computed(() => {
  const max = allPoints.value.length ? Math.max(...allPoints.value) : 0;
  return max > 0 ? max : 1;
});
const rxPath = computed(() => buildLinePath(props.rxPoints, yMax.value));
const txPath = computed(() => buildLinePath(props.txPoints, yMax.value));
const valueText = computed(() => {
  if (!props.hasData) return '—';
  return `收${formatCompactRate(props.rxRate)}发${formatCompactRate(props.txRate)}`;
});
const detailText = computed(() => (props.hasData ? `接收 ${formatRate(props.rxRate)} · 发送 ${formatRate(props.txRate)}` : '暂无数据'));
</script>

<template>
  <article class="metric-card">
    <div class="metric-head">
      <span class="metric-name">网络</span>
      <span class="metric-value compact-rate" :class="{ 'has-data': hasData }" :title="detailText" :aria-label="detailText">
        <span class="num">{{ valueText }}</span>
      </span>
    </div>

    <svg class="spark network" :viewBox="`0 0 ${CHART_W} ${CHART_H}`" preserveAspectRatio="none" aria-hidden="true">
      <line class="grid-line" x1="0" :y1="CHART_H * 0.25" :x2="CHART_W" :y2="CHART_H * 0.25" />
      <line class="grid-line" x1="0" :y1="CHART_H * 0.5" :x2="CHART_W" :y2="CHART_H * 0.5" />
      <line class="grid-line" x1="0" :y1="CHART_H * 0.75" :x2="CHART_W" :y2="CHART_H * 0.75" />
      <path v-if="hasData && rxPath" :d="rxPath" class="line-rx-data" />
      <path v-if="hasData && txPath" :d="txPath" class="line-tx-data" />
      <path v-if="!hasData" class="line-empty line-rx" :d="`M0,${CHART_H - 6} L${CHART_W},${CHART_H - 6}`" />
      <path v-if="!hasData" class="line-empty line-tx" :d="`M0,${CHART_H - 3} L${CHART_W},${CHART_H - 3}`" />
      <line class="baseline" x1="0" :y1="CHART_H - 1" :x2="CHART_W" :y2="CHART_H - 1" />
    </svg>

    <div class="metric-foot">
      <span class="legend">
        <span><i class="rx"></i>接收</span>
        <span><i class="tx"></i>发送</span>
      </span>
      <span>{{ hasData ? `峰值 ${formatRate(Math.max(rxRate, txRate))}` : '峰值 —' }}</span>
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
  gap: 4px;
  color: var(--app-subtle);
  font: 500 13px var(--font-mono);
}

.metric-value.has-data .num { color: var(--info); }

.spark {
  display: block;
  width: 100%;
  height: 52px;
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
.line-rx-data,
.line-tx-data {
  fill: none;
  stroke-linecap: round;
  stroke-linejoin: round;
  stroke-width: 1.2;
}

.line-empty { stroke: var(--app-border-strong); }
.line-rx-data { stroke: var(--info); }
.line-tx-data {
  stroke: var(--success);
  stroke-dasharray: 4 3;
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

.legend,
.legend span {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

// 图例与系列线双编码：颜色 + 线型（rx 实线 / tx 虚线），色弱可区分
.legend i {
  width: 10px;
  height: 0;
  border: 0;
  border-top: 2px solid var(--info);
  border-radius: 0;
  background: transparent;
}

.legend i.tx {
  border-top-style: dashed;
  border-color: var(--success);
}
</style>
