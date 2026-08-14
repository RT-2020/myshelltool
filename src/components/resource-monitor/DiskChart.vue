<script setup>
import { computed } from 'vue';
import { CHART_H, CHART_W, buildLinePath, formatBytes, formatCompactRate, formatRate } from './chart-utils.js';

const props = defineProps({
  readPoints: { type: Array, default: () => [] },
  writePoints: { type: Array, default: () => [] },
  readRate: { type: Number, default: 0 },
  writeRate: { type: Number, default: 0 },
  diskTotal: { type: Number, default: 0 },
  diskUsed: { type: Number, default: 0 },
  hasData: { type: Boolean, default: true }
});

const allPoints = computed(() => [...props.readPoints, ...props.writePoints]);
const yMax = computed(() => {
  const max = allPoints.value.length ? Math.max(...allPoints.value) : 0;
  return max > 0 ? max : 1;
});
const readPath = computed(() => buildLinePath(props.readPoints, yMax.value));
const writePath = computed(() => buildLinePath(props.writePoints, yMax.value));
const diskPct = computed(() => {
  if (!props.diskTotal) return 0;
  return Math.min(100, Math.round((props.diskUsed / props.diskTotal) * 100));
});
const hasCapacity = computed(() => props.diskTotal > 0);
const valueText = computed(() => {
  if (!props.hasData) return '—';
  return `读${formatCompactRate(props.readRate)}写${formatCompactRate(props.writeRate)}`;
});
const detailText = computed(() => (props.hasData ? `读取 ${formatRate(props.readRate)} · 写入 ${formatRate(props.writeRate)}` : '暂无数据'));
</script>

<template>
  <article class="metric-card">
    <div class="metric-head">
      <span class="metric-name">磁盘</span>
      <span class="metric-value compact-rate" :class="{ 'has-data': hasData }" :title="detailText" :aria-label="detailText">
        <span class="num">{{ valueText }}</span>
      </span>
    </div>

    <svg class="spark" :viewBox="`0 0 ${CHART_W} ${CHART_H}`" preserveAspectRatio="none" aria-hidden="true">
      <line class="grid-line" x1="0" :y1="CHART_H * 0.25" :x2="CHART_W" :y2="CHART_H * 0.25" />
      <line class="grid-line" x1="0" :y1="CHART_H * 0.5" :x2="CHART_W" :y2="CHART_H * 0.5" />
      <line class="grid-line" x1="0" :y1="CHART_H * 0.75" :x2="CHART_W" :y2="CHART_H * 0.75" />
      <path v-if="hasData && readPath" :d="readPath" class="line-rd-data" />
      <path v-if="hasData && writePath" :d="writePath" class="line-wr-data" />
      <path v-if="!hasData" class="line-empty" :d="`M0,${CHART_H - 4} L${CHART_W},${CHART_H - 4}`" />
      <line class="baseline" x1="0" :y1="CHART_H - 1" :x2="CHART_W" :y2="CHART_H - 1" />
    </svg>

    <div class="disk-row">
      <span class="label">根分区</span>
      <span class="val">{{ hasCapacity ? `${formatBytes(diskUsed)} / ${formatBytes(diskTotal)}` : '— / —' }}</span>
      <div class="disk-bar">
        <div
          v-if="hasCapacity"
          class="disk-bar-fill"
          :class="{ 'is-high': diskPct >= 85, 'is-warn': diskPct >= 70 }"
          :style="{ width: diskPct + '%' }"
        ></div>
      </div>
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
.line-rd-data,
.line-wr-data {
  fill: none;
  stroke-linecap: round;
  stroke-linejoin: round;
  stroke-width: 1.2;
}

.line-empty { stroke: var(--app-border-strong); }
.line-rd-data { stroke: var(--info); }
.line-wr-data {
  stroke: var(--warn);
  stroke-dasharray: 4 3;
}

.disk-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: center;
  gap: 6px 10px;
  margin-top: 6px;
  color: var(--app-subtle);
  font: 10px var(--font-mono);
}

.disk-row .val {
  justify-self: end;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.disk-bar {
  grid-column: 1 / -1;
  height: 4px;
  overflow: hidden;
  border-radius: var(--radius-pill);
  background: var(--app-panel-2);
}

.disk-bar-fill {
  height: 100%;
  border-radius: inherit;
  background: var(--success);
}

.disk-bar-fill.is-warn { background: var(--warn); }
.disk-bar-fill.is-high { background: var(--danger); }
</style>
