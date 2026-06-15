<script setup>
import { computed } from 'vue';
import { Network } from 'lucide-vue-next';
import { CHART_H, CHART_W, formatRate, buildLinePath } from './chart-utils.js';

const props = defineProps({
  rxPoints: { type: Array, default: () => [] },
  txPoints: { type: Array, default: () => [] },
  rxRate: { type: Number, default: 0 },
  txRate: { type: Number, default: 0 }
});

const allPoints = computed(() => [...props.rxPoints, ...props.txPoints]);
const yMax = computed(() => {
  const m = allPoints.value.length ? Math.max(...allPoints.value) : 0;
  return m > 0 ? m : 1;
});
const rxPath = computed(() => buildLinePath(props.rxPoints, yMax.value));
const txPath = computed(() => buildLinePath(props.txPoints, yMax.value));
</script>

<template>
  <section class="rm-chart rm-chart--net">
    <header class="rm-chart-head">
      <span class="rm-chart-label"><Network :size="12" /> 网络</span>
      <span class="rm-chart-value">
        <strong class="mono num rx">↓{{ formatRate(rxRate) }}</strong>
        <strong class="mono num tx">↑{{ formatRate(txRate) }}</strong>
      </span>
    </header>
    <svg :viewBox="`0 0 ${CHART_W} ${CHART_H}`" preserveAspectRatio="none" class="rm-chart-svg">
      <path v-if="rxPath" :d="rxPath" fill="none" stroke="var(--info, var(--accent))" stroke-width="1.2" />
      <path v-if="txPath" :d="txPath" fill="none" stroke="var(--success)" stroke-width="1.2" />
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
  gap: 8px;
  color: var(--app-strong);
}
.rm-chart-value .rx { color: var(--info, var(--accent)); }
.rm-chart-value .tx { color: var(--success); }
.rm-chart-svg {
  width: 100%;
  height: 60px;
  display: block;
}
</style>
