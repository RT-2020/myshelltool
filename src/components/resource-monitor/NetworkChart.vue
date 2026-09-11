<script setup lang="ts">
import { computed } from 'vue';
import { CHART_H, CHART_W, buildAreaPath, buildLinePath, formatRate } from './chart-utils';

const props = withDefaults(defineProps<{
  rxPoints?: number[];
  txPoints?: number[];
  rxRate?: number;
  txRate?: number;
  hasData?: boolean;
}>(), {
  rxPoints: () => [],
  txPoints: () => [],
  rxRate: 0,
  txRate: 0,
  hasData: true
});

const GRAD_ID = 'rm-net-grad';
const allPoints = computed(() => [...props.rxPoints, ...props.txPoints]);
const yMax = computed(() => {
  const max = allPoints.value.length ? Math.max(...allPoints.value) : 0;
  return max > 0 ? max : 1;
});
const rxPath = computed(() => buildLinePath(props.rxPoints, yMax.value));
const txPath = computed(() => buildLinePath(props.txPoints, yMax.value));
const rxArea = computed(() => buildAreaPath(rxPath.value));
const rxText = computed(() => (props.hasData ? formatRate(props.rxRate) : '—'));
const txText = computed(() => (props.hasData ? formatRate(props.txRate) : '—'));
const detailText = computed(() => (props.hasData ? `接收 ${formatRate(props.rxRate)} · 发送 ${formatRate(props.txRate)}` : '暂无数据'));
</script>

<template>
  <article class="metric-card">
    <!-- 读数行兼任图例：↓接收（info 实线+面积）/ ↑发送（success 虚线），与曲线色一一对应 -->
    <div class="metric-head">
      <span class="metric-name">网络</span>
      <span class="metric-readout" :title="detailText" :aria-label="detailText">
        <span class="rate rx"><i class="key rx-key" aria-hidden="true"></i>↓{{ rxText }}</span>
        <span class="rate tx"><i class="key tx-key" aria-hidden="true"></i>↑{{ txText }}</span>
      </span>
    </div>

    <svg class="spark network" :viewBox="`0 0 ${CHART_W} ${CHART_H}`" preserveAspectRatio="none" aria-hidden="true">
      <defs>
        <linearGradient :id="GRAD_ID" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stop-color="var(--info)" stop-opacity="0.3" />
          <stop offset="100%" stop-color="var(--info)" stop-opacity="0" />
        </linearGradient>
      </defs>
      <line class="grid-line" x1="0" :y1="CHART_H * 0.25" :x2="CHART_W" :y2="CHART_H * 0.25" />
      <line class="grid-line" x1="0" :y1="CHART_H * 0.5" :x2="CHART_W" :y2="CHART_H * 0.5" />
      <line class="grid-line" x1="0" :y1="CHART_H * 0.75" :x2="CHART_W" :y2="CHART_H * 0.75" />
      <path v-if="hasData && rxArea" :d="rxArea" :fill="`url(#${GRAD_ID})`" stroke="none" />
      <path v-if="hasData && rxPath" :d="rxPath" class="line-rx-data" />
      <path v-if="hasData && txPath" :d="txPath" class="line-tx-data" />
      <path v-if="!hasData" class="line-empty line-rx" :d="`M0,${CHART_H - 6} L${CHART_W},${CHART_H - 6}`" />
      <path v-if="!hasData" class="line-empty line-tx" :d="`M0,${CHART_H - 3} L${CHART_W},${CHART_H - 3}`" />
      <line class="baseline" x1="0" :y1="CHART_H - 1" :x2="CHART_W" :y2="CHART_H - 1" />
    </svg>
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

// 双速率读数：与 CPU/内存的大读数同层级；色点即图例（点色 = 曲线色）
.metric-readout {
  display: inline-flex;
  align-items: baseline;
  justify-content: flex-end;
  gap: 10px;
  min-width: 0;
  color: var(--app-strong);
  font: 500 13px/1 var(--font-mono);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.metric-readout .rate {
  display: inline-flex;
  align-items: baseline;
  gap: 4px;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

// 图例键：短线段形态（实/虚）与曲线线型一致，色弱下仍可区分 rx/tx
.metric-readout .key {
  align-self: center;
  width: 10px;
  height: 0;
  border-top: 2px solid var(--info);
}

.metric-readout .tx-key {
  border-top-style: dashed;
  border-top-color: var(--success);
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
</style>
