<script setup>
import { computed } from 'vue';
import { HardDrive } from 'lucide-vue-next';
import { CHART_H, CHART_W, formatRate, buildLinePath } from './chart-utils.js';

const props = defineProps({
  readPoints: { type: Array, default: () => [] },
  writePoints: { type: Array, default: () => [] },
  readRate: { type: Number, default: 0 },
  writeRate: { type: Number, default: 0 }
});

const allPoints = computed(() => [...props.readPoints, ...props.writePoints]);
const yMax = computed(() => {
  const m = allPoints.value.length ? Math.max(...allPoints.value) : 0;
  return m > 0 ? m : 1;
});
const readPath = computed(() => buildLinePath(props.readPoints, yMax.value));
const writePath = computed(() => buildLinePath(props.writePoints, yMax.value));
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
</style>
