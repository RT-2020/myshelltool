<script setup>
import { computed } from 'vue';
import { MemoryStick } from 'lucide-vue-next';
import { CHART_H, CHART_W, formatBytes, buildLinePath } from './chart-utils.js';

const props = defineProps({
  points: { type: Array, default: () => [] },
  memTotal: { type: Number, default: 0 },
  memUsed: { type: Number, default: 0 }
});

const usedPct = computed(() => {
  if (!props.memTotal) return 0;
  return Math.min(100, Math.max(0, (props.memUsed / props.memTotal) * 100));
});

const path = computed(() => buildLinePath(props.points, 100));
</script>

<template>
  <section class="rm-chart rm-chart--mem">
    <header class="rm-chart-head">
      <span class="rm-chart-label"><MemoryStick :size="12" /> 内存</span>
      <span class="rm-chart-value">
        <strong class="mono num">{{ formatBytes(memUsed) }}</strong>
        <span class="muted mono num">/ {{ formatBytes(memTotal) }}</span>
      </span>
    </header>
    <svg :viewBox="`0 0 ${CHART_W} ${CHART_H}`" preserveAspectRatio="none" class="rm-chart-svg">
      <path v-if="path" :d="path" fill="none" stroke="var(--accent)" stroke-width="1.2" />
    </svg>
    <div class="rm-mem-bar" role="progressbar" :aria-valuenow="usedPct.toFixed(0)" aria-valuemin="0" aria-valuemax="100">
      <div class="rm-mem-bar-fill" :style="{ width: usedPct + '%' }"></div>
    </div>
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
  height: 36px;
  display: block;
}
.rm-mem-bar {
  height: 3px;
  background: var(--app-subtle);
  border-radius: var(--radius-sm);
  overflow: hidden;
}
.rm-mem-bar-fill {
  height: 100%;
  background: var(--accent);
  transition: width var(--motion-fast) var(--ease-standard);
}
</style>
