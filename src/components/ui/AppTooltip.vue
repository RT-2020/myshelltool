<script setup>
import { ref, onBeforeUnmount } from 'vue';

const props = defineProps({
  content: { type: String, default: '' },
  placement: { type: String, default: 'top' }, // top | right | bottom | left
  delay: { type: Number, default: 300 }
});

const visible = ref(false);
let showTimer = null;

function onMouseEnter() {
  if (showTimer) clearTimeout(showTimer);
  showTimer = setTimeout(() => {
    visible.value = true;
  }, props.delay);
}

function onMouseLeave() {
  if (showTimer) {
    clearTimeout(showTimer);
    showTimer = null;
  }
  visible.value = false;
}

onBeforeUnmount(() => {
  if (showTimer) clearTimeout(showTimer);
});
</script>

<template>
  <span
    class="app-tooltip-host"
    @mouseenter="onMouseEnter"
    @mouseleave="onMouseLeave"
    @focus="onMouseEnter"
    @blur="onMouseLeave"
  >
    <slot />
    <Transition name="app-tooltip">
      <span
        v-if="visible && content"
        class="app-tooltip"
        :class="`app-tooltip--${placement}`"
        role="tooltip"
      >{{ content }}</span>
    </Transition>
  </span>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.app-tooltip-host {
  position: relative;
  display: inline-flex;
}

.app-tooltip {
  position: absolute;
  z-index: var(--z-tooltip);
  background: var(--app-strong);
  color: var(--app-bg);
  padding: 4px 8px;
  font-size: var(--text-xs);
  border-radius: var(--radius-sm);
  white-space: nowrap;
  pointer-events: none;
  box-shadow: var(--app-shadow);
  line-height: 1.4;
}

.app-tooltip--top {
  bottom: 100%;
  left: 50%;
  transform: translateX(-50%);
  margin-bottom: 6px;
}
.app-tooltip--bottom {
  top: 100%;
  left: 50%;
  transform: translateX(-50%);
  margin-top: 6px;
}
.app-tooltip--left {
  right: 100%;
  top: 50%;
  transform: translateY(-50%);
  margin-right: 6px;
}
.app-tooltip--right {
  left: 100%;
  top: 50%;
  transform: translateY(-50%);
  margin-left: 6px;
}

.app-tooltip-enter-active,
.app-tooltip-leave-active {
  transition: opacity var(--motion-fast) var(--ease-standard);
}
.app-tooltip-enter-from,
.app-tooltip-leave-to {
  opacity: 0;
}
</style>
