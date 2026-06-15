<script setup>
import { computed } from 'vue';

const props = defineProps({
  percent: { type: Number, default: 0 },
  variant: { type: String, default: 'linear' }, // linear | circular
  status: { type: String, default: 'running' } // running | done | error
});

const clamped = computed(() => Math.max(0, Math.min(100, props.percent)));

const fillColor = computed(() => {
  if (props.status === 'done') return 'var(--success)';
  if (props.status === 'error') return 'var(--danger)';
  return 'var(--accent)';
});

// circular geometry
const radius = 16;
const stroke = 3;
const circumference = 2 * Math.PI * radius;
const dashOffset = computed(() => circumference - (clamped.value / 100) * circumference);
</script>

<template>
  <div class="app-progress" :class="`app-progress--${variant}`">
    <template v-if="variant === 'linear'">
      <div class="app-progress-track">
        <div
          class="app-progress-fill"
          :style="{ width: clamped + '%', background: fillColor }"
        ></div>
      </div>
    </template>
    <template v-else>
      <svg class="app-progress-circular" :width="40" :height="40" viewBox="0 0 40 40">
        <circle
          class="app-progress-circular-bg"
          :cx="20" :cy="20" :r="radius"
          :stroke-width="stroke"
          fill="none"
        />
        <circle
          class="app-progress-circular-fg"
          :cx="20" :cy="20" :r="radius"
          :stroke-width="stroke"
          :stroke="fillColor"
          fill="none"
          stroke-linecap="round"
          :stroke-dasharray="circumference"
          :stroke-dashoffset="dashOffset"
          :transform="'rotate(-90 20 20)'"
        />
      </svg>
    </template>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.app-progress {
  display: inline-flex;
  align-items: center;
}

.app-progress-track {
  width: 100%;
  height: 4px;
  background: var(--app-panel-2);
  border-radius: var(--radius-pill);
  overflow: hidden;
}

.app-progress-fill {
  height: 100%;
  border-radius: var(--radius-pill);
  transition: width 0.2s ease, background var(--motion-fast) var(--ease-standard);
}

.app-progress-circular-bg {
  stroke: var(--app-panel-2);
}

.app-progress-circular-fg {
  transition: stroke-dashoffset 0.2s ease, stroke var(--motion-fast) var(--ease-standard);
}
</style>
