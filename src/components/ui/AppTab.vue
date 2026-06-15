<script setup>
defineProps({
  id: { type: [String, Number], default: '' },
  label: { type: String, default: '' },
  icon: { type: [Object, Function], default: null },
  active: { type: Boolean, default: false }
});
const emit = defineEmits(['click']);

function onClick() {
  emit('click');
}
</script>

<template>
  <button
    class="app-tab"
    :class="{ active }"
    role="tab"
    :aria-selected="String(active)"
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
  color: var(--app-muted);
  font-size: var(--text-sm);
  font-family: var(--font-body);
  cursor: pointer;
  position: relative;
  transition: color var(--motion-fast) var(--ease-standard);
}
.app-tab:hover {
  color: var(--app-strong);
}
.app-tab.active {
  color: var(--app-strong);
  box-shadow: inset 0 -2px 0 var(--accent);
}
.app-tab-icon {
  flex: 0 0 auto;
}
.app-tab-label {
  white-space: nowrap;
}
</style>
