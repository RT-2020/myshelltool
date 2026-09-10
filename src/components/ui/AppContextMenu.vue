<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from 'vue';

/** 右键菜单条目契约（FileSurface 等消费方按此形状传入）。 */
interface ContextMenuItem {
  label?: string;
  action?: () => void;
  danger?: boolean;
  separator?: boolean;
  disabled?: boolean;
}

const props = withDefaults(
  defineProps<{
    items?: ContextMenuItem[];
    open?: boolean;
    x?: number;
    y?: number;
  }>(),
  {
    items: () => [],
    open: false,
    x: 0,
    y: 0
  }
);
const emit = defineEmits<{ close: [] }>();

const menuRef = ref<HTMLElement | null>(null);
const pos = ref<{ left: number; top: number }>({ left: 0, top: 0 });
const activeIndex = ref(0);

// 可交互项（跳过 separator / disabled），activeIndex 指向该数组下标
const selectable = computed(() => {
  const res: Array<{ item: ContextMenuItem; index: number }> = [];
  props.items.forEach((item, i) => {
    if (!item.separator && !item.disabled) res.push({ item, index: i });
  });
  return res;
});

function close() {
  emit('close');
}

function onItemClick(item: ContextMenuItem) {
  if (item.separator || item.disabled) return;
  if (typeof item.action === 'function') item.action();
  close();
}

function moveHighlight(dir: number) {
  const len = selectable.value.length;
  if (!len) return;
  let next = activeIndex.value + dir;
  if (next < 0) next = len - 1;
  if (next >= len) next = 0;
  activeIndex.value = next;
}

function onDocClick() {
  if (props.open) close();
}

function onKeydown(e: KeyboardEvent) {
  if (!props.open) return;
  if (e.key === 'Escape') {
    e.preventDefault();
    close();
    return;
  }
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault();
    moveHighlight(e.key === 'ArrowDown' ? 1 : -1);
    return;
  }
  if (e.key === 'Enter') {
    e.preventDefault();
    const cur = selectable.value[activeIndex.value];
    if (cur) onItemClick(cur.item);
  }
}

function onMenuClick(e: MouseEvent) {
  e.stopPropagation();
}

watch(
  () => props.open,
  (v) => {
    if (v) {
      activeIndex.value = 0;
      pos.value = { left: props.x, top: props.y };
      nextTick(() => {
        if (!menuRef.value) return;
        // 视口翻转：以实际渲染尺寸为准，避免溢出屏幕
        const rect = menuRef.value.getBoundingClientRect();
        const margin = 4;
        let left = pos.value.left;
        let top = pos.value.top;
        if (left + rect.width > window.innerWidth) {
          left = Math.max(margin, window.innerWidth - rect.width - margin);
        }
        if (top + rect.height > window.innerHeight) {
          top = Math.max(margin, window.innerHeight - rect.height - margin);
        }
        pos.value = { left, top };
        const first = menuRef.value.querySelector<HTMLElement>('.app-context-menu-item:not(.disabled)');
        if (first) first.focus();
      });
    }
  }
);

watch(
  () => props.items,
  () => {
    if (props.open) activeIndex.value = 0;
  }
);

onMounted(() => {
  document.addEventListener('click', onDocClick);
  document.addEventListener('keydown', onKeydown);
});
onBeforeUnmount(() => {
  document.removeEventListener('click', onDocClick);
  document.removeEventListener('keydown', onKeydown);
});
</script>

<template>
  <Teleport to="body">
    <ul
      v-if="open"
      ref="menuRef"
      class="app-context-menu"
      role="menu"
      :style="{ left: pos.left + 'px', top: pos.top + 'px' }"
      @click="onMenuClick"
      @contextmenu.prevent
    >
      <template v-for="(item, idx) in items" :key="idx">
        <li v-if="item.separator" class="app-context-menu-separator"></li>
        <li
          v-else
          class="app-context-menu-item"
          :class="{
            danger: item.danger,
            disabled: item.disabled,
            'is-active': activeIndex >= 0 && selectable[activeIndex] && selectable[activeIndex].index === idx
          }"
          role="menuitem"
          tabindex="-1"
          :aria-disabled="item.disabled ? 'true' : undefined"
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
  // body 级瞬态浮层（Teleport to body）：必须压住 app 内的 sticky 表头，否则文件区
  // 右键菜单会被文件列头/列头行（--z-sticky = 200）盖掉；用 --z-popover(250) 而非
  // --z-dropdown(100)，同时保持低于抽屉(300)/模态(400)——抽屉与模态是用户显式打开的
  // 面板，理应在菜单之上。
  z-index: var(--z-popover);
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
.app-context-menu-item.is-active {
  background: var(--app-hover);
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
