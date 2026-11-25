<script setup lang="ts">
import type { Component } from 'vue';

const props = withDefaults(
  defineProps<{
    id?: string | number;
    label?: string;
    /** lucide-vue-next 图标组件（对象式或函数式组件）。 */
    icon?: Component | null;
    active?: boolean;
    disabled?: boolean;
  }>(),
  {
    id: '',
    label: '',
    icon: null,
    active: false,
    disabled: false
  }
);
const emit = defineEmits<{ click: [] }>();

function onClick() {
  if (props.disabled) return;
  emit('click');
}
</script>

<template>
  <button
    class="app-tab"
    :class="{ active, 'is-disabled': disabled }"
    role="tab"
    :disabled="disabled"
    :aria-selected="active"
    @click="onClick"
  >
    <component v-if="icon" :is="icon" :size="14" class="app-tab-icon" />
    <span class="app-tab-label">{{ label }}</span>
  </button>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.app-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 14px;
  background: transparent;
  border: none;
  border-radius: var(--radius-sm);
  color: var(--app-muted);
  font-size: var(--text-sm);
  font-family: var(--font-body);
  cursor: pointer;
  position: relative;
  transition: color var(--dur-fast) var(--ease-standard),
    background-color var(--dur-fast) var(--ease-standard);
}
.app-tab:hover:not(:disabled) {
  color: var(--app-strong);
  background-color: var(--app-hover);
}
.app-tab:active:not(:disabled) {
  background-color: var(--app-active);
}
.app-tab.is-disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.app-tab.active {
  color: var(--app-strong);
}
.app-tab-icon {
  flex: 0 0 auto;
}
.app-tab-label {
  white-space: nowrap;
}
</style>
