<script setup lang="ts">
import { computed } from 'vue';
import { CHART_H, CHART_W, buildAreaPath, buildLinePath, formatBytes, formatRate } from './chart-utils';
import type { DiskMountInfo } from '@/types/domain';

const props = withDefaults(defineProps<{
  readPoints?: number[];
  writePoints?: number[];
  readRate?: number;
  writeRate?: number;
  diskTotal?: number;
  diskUsed?: number;
  disks?: DiskMountInfo[];
  hasData?: boolean;
}>(), {
  readPoints: () => [],
  writePoints: () => [],
  readRate: 0,
  writeRate: 0,
  diskTotal: 0,
  diskUsed: 0,
  disks: () => [],
  hasData: true
});

const GRAD_ID = 'rm-disk-grad';
const allPoints = computed(() => [...props.readPoints, ...props.writePoints]);
const yMax = computed(() => {
  const max = allPoints.value.length ? Math.max(...allPoints.value) : 0;
  return max > 0 ? max : 1;
});
const readPath = computed(() => buildLinePath(props.readPoints, yMax.value));
const readArea = computed(() => buildAreaPath(readPath.value));
const writePath = computed(() => buildLinePath(props.writePoints, yMax.value));
const readText = computed(() => (props.hasData ? formatRate(props.readRate) : '—'));
const writeText = computed(() => (props.hasData ? formatRate(props.writeRate) : '—'));
const detailText = computed(() => (props.hasData ? `读取 ${formatRate(props.readRate)} · 写入 ${formatRate(props.writeRate)}` : '暂无数据'));
const hasCapacity = computed(() => props.diskTotal > 0);
// 多挂载点明细（后端已过滤伪文件系统，上限 12 行）；空则回退根分区单行
// （diskTotal/diskUsed——df 段降级或旧后端快照的场景）。
interface MountRow {
  mount: string;
  total: number;
  used: number;
  pct: number;
}
function toRow(m: { mount: string; total: number; used: number }): MountRow {
  const pct = m.total > 0 ? Math.min(100, Math.round((m.used / m.total) * 100)) : 0;
  return { ...m, pct };
}
const mountRows = computed<MountRow[]>(() => props.disks.map(toRow));
const fallbackRow = computed<MountRow | null>(() => (hasCapacity.value ? toRow({ mount: '根分区', total: props.diskTotal, used: props.diskUsed }) : null));
function thresholdClass(pct: number): string {
  if (pct >= 85) return 'is-high';
  if (pct >= 70) return 'is-warn';
  return '';
}
</script>

<template>
  <article class="metric-card">
    <!-- 读数行兼任图例：读（info 实线+面积）/ 写（warn 虚线），与曲线色一一对应 -->
    <div class="metric-head">
      <span class="metric-name">磁盘</span>
      <span class="metric-readout" :title="detailText" :aria-label="detailText">
        <span class="rate rd"><i class="key rd-key" aria-hidden="true"></i>读{{ readText }}</span>
        <span class="rate wr"><i class="key wr-key" aria-hidden="true"></i>写{{ writeText }}</span>
      </span>
    </div>

    <svg class="spark" :viewBox="`0 0 ${CHART_W} ${CHART_H}`" preserveAspectRatio="none" aria-hidden="true">
      <defs>
        <linearGradient :id="GRAD_ID" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stop-color="var(--info)" stop-opacity="0.3" />
          <stop offset="100%" stop-color="var(--info)" stop-opacity="0" />
        </linearGradient>
      </defs>
      <line class="grid-line" x1="0" :y1="CHART_H * 0.25" :x2="CHART_W" :y2="CHART_H * 0.25" />
      <line class="grid-line" x1="0" :y1="CHART_H * 0.5" :x2="CHART_W" :y2="CHART_H * 0.5" />
      <line class="grid-line" x1="0" :y1="CHART_H * 0.75" :x2="CHART_W" :y2="CHART_H * 0.75" />
      <path v-if="hasData && readArea" :d="readArea" :fill="`url(#${GRAD_ID})`" stroke="none" />
      <path v-if="hasData && readPath" :d="readPath" class="line-rd-data" />
      <path v-if="hasData && writePath" :d="writePath" class="line-wr-data" />
      <path v-if="!hasData" class="line-empty" :d="`M0,${CHART_H - 4} L${CHART_W},${CHART_H - 4}`" />
      <line class="baseline" x1="0" :y1="CHART_H - 1" :x2="CHART_W" :y2="CHART_H - 1" />
    </svg>

    <!-- 多挂载点容量列表（FinalShell 式：每行 挂载点 + 用量/总量 + 细条）；
         disks 为空（df 降级/旧后端）时回退根分区单行 -->
    <div v-if="mountRows.length" class="disk-list">
      <div v-for="row in mountRows" :key="row.mount" class="disk-row">
        <span class="label" :title="`${row.mount} · ${row.pct}%`">{{ row.mount }}</span>
        <span class="val">{{ formatBytes(row.used) }} / {{ formatBytes(row.total) }}</span>
        <div class="disk-bar">
          <div class="disk-bar-fill" :class="thresholdClass(row.pct)" :style="{ width: row.pct + '%' }"></div>
        </div>
      </div>
    </div>
    <div v-else-if="fallbackRow" class="disk-row">
      <span class="label">根分区</span>
      <span class="val">{{ formatBytes(fallbackRow.used) }} / {{ formatBytes(fallbackRow.total) }}</span>
      <div class="disk-bar">
        <div class="disk-bar-fill" :class="thresholdClass(fallbackRow.pct)" :style="{ width: fallbackRow.pct + '%' }"></div>
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

// 双速率读数：与网络区块同构；色点即图例（点色 = 曲线色）
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

// 图例键：短线段形态（实/虚）与曲线线型一致
.metric-readout .key {
  align-self: center;
  width: 10px;
  height: 0;
  border-top: 2px solid var(--info);
}

.metric-readout .wr-key {
  border-top-style: dashed;
  border-top-color: var(--warn);
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

// 多挂载点列表：紧凑行距 + 限高滚动（后端上限 12 行，正常服务器 1~4 行不触发滚动）
.disk-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 168px;
  margin-top: 2px;
  overflow-y: auto;
}

// 单行 = 挂载点(可截断) + 读数(右) + 全宽细条。flex-wrap 防窄栏叠字：
// 读数放不下时整行下移而不是压住挂载点名（同前一轮根分区行修复）。
.disk-row {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: baseline;
  gap: 2px 8px;
  margin-top: 4px;
  color: var(--app-subtle);
  font: 10px var(--font-mono);
}

.disk-row .label {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.disk-row .val {
  flex: 0 0 auto;
  max-width: 60%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

.disk-bar {
  flex: 0 0 100%;
  height: 3px;
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
