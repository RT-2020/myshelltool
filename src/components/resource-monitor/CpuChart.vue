<script setup>
import { computed } from 'vue';
import { Cpu } from 'lucide-vue-next';
import { CHART_H, CHART_W, buildLinePath, buildAreaPath } from './chart-utils.js';

const props = defineProps({
  points: { type: Array, default: () => [] },
  current: { type: Number, default: 0 },
  cores: { type: Number, default: 0 }
});

const GRAD_ID = 'rm-cpu-grad';

const path = computed(() => buildLinePath(props.points, 100));
const areaPath = computed(() => buildAreaPath(path.value));
</script>

<template>
  <section class="rm-chart rm-chart--cpu">
    <header class="rm-chart-head">
      <span class="rm-chart-label"><Cpu :size="12" /> CPU</span>
      <span class="rm-chart-value">
        <strong class="mono num">{{ current.toFixed(1) }}%</strong>
        <span class="muted mono num" v-if="cores">{{ cores }} cores</span>
      </span>
    </header>
    <svg :viewBox="`0 0 ${CHART_W} ${CHART_H}`" preserveAspectRatio="none" class="rm-chart-svg">
      <defs>
        <linearGradient :id="GRAD_ID" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stop-color="var(--accent)" stop-opacity="0.35" />
          <stop offset="100%" stop-color="var(--accent)" stop-opacity="0" />
        </linearGradient>
      </defs>
      <path v-if="areaPath" :d="areaPath" :fill="`url(#${GRAD_ID})`" stroke="none" />
      <path v-if="path" :d="path" fill="none" stroke="var(--accent)" stroke-width="1.2" />
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
  border-block-end: 1px solid var(--app-border);
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
  gap: 6px;
  color: var(--app-strong);
}
.rm-chart-svg {
  width: 100%;
  height: 60px;
  display: block;
}
</style>
