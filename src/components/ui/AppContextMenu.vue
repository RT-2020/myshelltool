<script setup>
import { watch, onMounted, onBeforeUnmount } from 'vue';

const props = defineProps({
  items: { type: Array, default: () => [] }, // [{ label, action, danger, separator, disabled }]
  open: { type: Boolean, default: false },
  x: { type: Number, default: 0 },
  y: { type: Number, default: 0 }
});
const emit = defineEmits(['close']);

function close() {
  emit('close');
}

function onItemClick(item) {
  if (item.separator || item.disabled) return;
  if (typeof item.action === 'function') item.action();
  close();
}

function onDocClick() {
  if (props.open) close();
}

function onKeydown(e) {
  if (props.open && e.key === 'Escape') {
    e.preventDefault();
    close();
  }
}

function onMenuClick(e) {
  e.stopPropagation();
}

onMounted(() => {
  document.addEventListener('click', onDocClick);
  document.addEventListener('keydown', onKeydown);
});
onBeforeUnmount(() => {
  document.removeEventListener('click', onDocClick);
  document.removeEventListener('keydown', onKeydown);
});

watch(
  () => props.open,
  () => {}
);
</script>

<template>
  <Teleport to="body">
    <ul
      v-if="open"
      class="app-context-menu"
      :style="{ left: x + 'px', top: y + 'px' }"
      @click="onMenuClick"
      @contextmenu.prevent
    >
      <template v-for="(item, idx) in items" :key="idx">
        <li v-if="item.separator" class="app-context-menu-separator"></li>
        <li
          v-else
          class="app-context-menu-item"
          :class="{ danger: item.danger, disabled: item.disabled }"
          @click="onItemClick(item)"
        >
          {{ item.label }}
        </li>
      </template>
    </ul>
  </Teleport>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.app-context-menu {
  position: fixed;
  z-index: var(--z-dropdown);
  list-style: none;
  margin: 0;
  padding: 4px;
  min-width: 160px;
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  box-shadow: var(--app-shadow);
}

.app-context-menu-item {
  padding: 6px 10px;
  font-size: var(--text-sm);
  color: var(--app-text);
  cursor: pointer;
  border-radius: var(--radius-sm);
  user-select: none;
}
.app-context-menu-item:hover:not(.disabled) {
  background: var(--app-hover);
}
.app-context-menu-item.danger {
  color: var(--danger);
}
.app-context-menu-item.danger:hover:not(.disabled) {
  background: color-mix(in oklab, var(--danger), transparent 85%);
}
.app-context-menu-item.disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.app-context-menu-separator {
  height: 1px;
  background: var(--app-border);
  margin: 4px 0;
}
</style>
