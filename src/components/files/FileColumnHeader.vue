<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import {
  ArrowUp,
  ChevronRight,
  Filter as FilterIcon,
  Loader2,
  Pencil,
  RefreshCw,
  X
} from 'lucide-vue-next';
import type { PathCrumb } from './fileColumnUtils';

const props = withDefaults(defineProps<{
  kind: 'local' | 'remote';
  columnTitle: string;
  selectionCount?: number;
  entryCount?: number;
  crumbs?: PathCrumb[];
  pathEditing?: boolean;
  manualPathInput?: string;
  filterValue?: string;
  showFilterClear?: boolean;
  isBusy?: boolean;
  disableUp?: boolean;
  disableRefresh?: boolean;
}>(), {
  selectionCount: 0,
  entryCount: 0,
  crumbs: () => [],
  pathEditing: false,
  manualPathInput: '',
  filterValue: '',
  showFilterClear: false,
  isBusy: false,
  disableUp: false,
  disableRefresh: false
});

const emit = defineEmits<{
  'enter-path-editing': [];
  'manual-path-input': [event: Event];
  'path-input-keydown': [event: KeyboardEvent];
  'blur-path-input': [];
  'crumb-click': [seg: PathCrumb];
  'toggle-filter-input': [event: Event];
  'clear-filter': [];
  'go-up': [];
  'refresh': [];
}>();

const pathInputRef = ref<HTMLInputElement | null>(null);

// 编辑态挂载即聚焦并全选：HTML 的 autofocus 属性对动态插入的元素不生效
// （只认文档初次加载），且全选让「整路径覆盖输入」一键可达（antd Editable
// 常规形态）。
watch(() => props.pathEditing, editing => {
  if (!editing) return;
  nextTick(() => {
    pathInputRef.value?.focus();
    pathInputRef.value?.select();
  });
});

const filterOpen = ref(false);
const filterPopoverRef = ref<HTMLElement | null>(null);
const filterBtnRef = ref<HTMLButtonElement | null>(null);
const filterInputRef = ref<HTMLInputElement | null>(null);
const breadcrumbRef = ref<HTMLElement | null>(null);

function scrollBreadcrumbsToEnd() {
  const el = breadcrumbRef.value;
  if (!el) return;
  el.scrollLeft = el.scrollWidth;
}

watch(
  () => [props.crumbs, props.pathEditing] as const,
  ([, editing]) => {
    if (!editing) nextTick(scrollBreadcrumbsToEnd);
  },
  { flush: 'post', immediate: true }
);

function toggleFilter(event: MouseEvent) {
  event.stopPropagation();
  filterOpen.value = !filterOpen.value;
  if (filterOpen.value) {
    requestAnimationFrame(() => filterInputRef.value?.focus());
  }
}

function closeFilterAndRestoreFocus() {
  if (!filterOpen.value) return;
  filterOpen.value = false;
  nextTick(() => filterBtnRef.value?.focus());
}

function onWindowClick(event: MouseEvent) {
  if (!filterOpen.value) return;
  const pop = filterPopoverRef.value;
  const btn = filterBtnRef.value;
  const target = event.target as Node | null;
  if (pop && !pop.contains(target) && btn && !btn.contains(target)) {
    filterOpen.value = false;
  }
}

let crumbResizeObserver: ResizeObserver | null = null;
onMounted(() => {
  window.addEventListener('click', onWindowClick);
  if (typeof ResizeObserver !== 'undefined') {
    crumbResizeObserver = new ResizeObserver(() => scrollBreadcrumbsToEnd());
    if (breadcrumbRef.value) crumbResizeObserver.observe(breadcrumbRef.value);
  }
});
onBeforeUnmount(() => {
  window.removeEventListener('click', onWindowClick);
  crumbResizeObserver?.disconnect();
  crumbResizeObserver = null;
});
</script>

<template>
  <header class="pane-header file-column-head">
    <span class="pane-tag" :class="kind">{{ columnTitle }}</span>
    <div class="file-column-head-meta">
      <strong class="file-column-title">{{ columnTitle }}</strong>
      <span v-if="selectionCount" class="file-column-count">{{ selectionCount }} 项已选</span>
      <span v-else class="file-column-count">{{ entryCount }} 项</span>
    </div>

    <div class="file-column-path">
      <nav
        v-if="!pathEditing"
        ref="breadcrumbRef"
        class="breadcrumb file-column-breadcrumb"
        aria-label="当前路径"
        @dblclick="emit('enter-path-editing')"
      >
        <template v-if="crumbs.length">
          <button
            v-for="(seg, idx) in crumbs"
            :key="seg.path + idx"
            type="button"
            class="crumb"
            :class="{ active: idx === crumbs.length - 1 }"
            :title="seg.path"
            @click="emit('crumb-click', seg)"
          >
            <span class="crumb-label">{{ seg.label }}</span>
            <ChevronRight v-if="idx < crumbs.length - 1" :size="11" class="crumb-sep" aria-hidden="true" />
          </button>
          <button
            type="button"
            class="crumb-edit"
            title="编辑路径"
            aria-label="编辑路径"
            @click.stop="emit('enter-path-editing')"
          >
            <Pencil :size="11" aria-hidden="true" />
          </button>
        </template>
        <button
          v-else
          type="button"
          class="crumb-empty"
          aria-label="输入路径"
          @click="emit('enter-path-editing')"
        >点此输入路径...</button>
      </nav>

      <input
        v-else
        ref="pathInputRef"
        class="file-column-manual-path"
        aria-label="编辑目录路径"
        :value="manualPathInput"
        placeholder="路径（回车跳转，Esc 取消）"
        spellcheck="false"
        @input="emit('manual-path-input', $event)"
        @keydown="emit('path-input-keydown', $event)"
        @blur="emit('blur-path-input')"
      />
    </div>

    <div class="pane-tools file-column-head-actions">
      <slot name="actions-leading" />

      <div class="filter-host">
        <button
          ref="filterBtnRef"
          class="icon-btn"
          :class="{ active: filterValue !== '' || filterOpen }"
          type="button"
          title="过滤当前目录"
          aria-label="过滤当前目录"
          aria-haspopup="dialog"
          :aria-expanded="filterOpen"
          @click="toggleFilter"
        >
          <FilterIcon :size="14" aria-hidden="true" />
        </button>
        <div
          v-if="filterOpen"
          ref="filterPopoverRef"
          class="filter-popover"
          @click.stop
          @keydown.escape.stop.prevent="closeFilterAndRestoreFocus"
        >
          <span class="filter-popover-icon" aria-hidden="true"><FilterIcon :size="12" /></span>
          <input
            ref="filterInputRef"
            class="filter-popover-input"
            aria-label="过滤当前目录"
            :value="filterValue"
            placeholder="过滤当前目录..."
            spellcheck="false"
            @input="emit('toggle-filter-input', $event)"
          />
          <button
            v-if="showFilterClear"
            class="filter-popover-clear"
            type="button"
            title="清空过滤"
            aria-label="清空过滤"
            @click="emit('clear-filter')"
          >
            <X :size="12" aria-hidden="true" />
          </button>
        </div>
      </div>

      <button
        class="icon-btn"
        type="button"
        title="上级目录"
        aria-label="上级目录"
        :disabled="disableUp"
        @click="emit('go-up')"
      >
        <ArrowUp :size="14" aria-hidden="true" />
      </button>
      <button
        class="icon-btn"
        type="button"
        title="刷新"
        aria-label="刷新目录"
        :disabled="disableRefresh"
        @click="emit('refresh')"
      >
        <Loader2 v-if="isBusy" class="spin" :size="14" aria-hidden="true" />
        <RefreshCw v-else :size="14" aria-hidden="true" />
      </button>
    </div>
  </header>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.pane-header,
.file-column-head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 0 8px;
  position: sticky;
  top: 0;
  z-index: var(--z-sticky);
  background: var(--app-panel-2);
  border-block-end: 1px solid var(--app-border);
  min-height: 34px;
}

.pane-tag {
  display: inline-flex;
  align-items: center;
  height: 18px;
  padding: 0 7px;
  border-radius: var(--radius-pill);
  font: 500 10px var(--font-display);
  letter-spacing: 0.04em;
  white-space: nowrap;
}

.pane-tag.local {
  color: var(--info);
  background: var(--info-soft);
}

.pane-tag.remote {
  color: var(--accent);
  background: var(--accent-soft);
}

.file-column-head-meta {
  display: inline-flex;
  align-items: baseline;
  gap: var(--space-2);
  flex: 0 0 auto;
  min-width: 0;
}

@container (max-width: 660px) {
  .file-column-head-meta {
    display: none;
  }
}

.file-column-title {
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--app-text);
  white-space: nowrap;
}

.file-column-count {
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.file-column-path {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  align-items: center;
}

.pane-tools,
.file-column-head-actions {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  flex: 0 0 auto;
}

.file-column-breadcrumb {
  display: flex;
  align-items: center;
  gap: 1px;
  min-width: 0;
  flex: 1;
  overflow-x: auto;
  scrollbar-width: none;

  &::-webkit-scrollbar {
    display: none;
  }
}

.crumb {
  display: inline-flex;
  align-items: center;
  gap: 1px;
  background: transparent;
  border: none;
  padding: 2px 4px;
  color: var(--app-muted);
  cursor: pointer;
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  font-family: var(--font-mono);
  white-space: nowrap;
  flex: 0 0 auto;
  transition:
    background var(--dur-fast) var(--ease-standard),
    color var(--dur-fast) var(--ease-standard);
}

.crumb:hover {
  background: var(--app-hover);
  color: var(--app-strong);
}

.crumb:active {
  background: var(--app-active);
}

.crumb.active {
  color: var(--app-strong);
  font-weight: 600;
}

.crumb-sep {
  color: var(--app-subtle);
  flex-shrink: 0;
}

.crumb-edit {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  background: transparent;
  border: none;
  color: var(--app-subtle);
  cursor: pointer;
  border-radius: var(--radius-sm);
  flex: 0 0 auto;
  margin-inline-start: 2px;
  transition:
    background var(--dur-fast) var(--ease-standard),
    color var(--dur-fast) var(--ease-standard);
}

.crumb-edit::after {
  content: '';
  position: absolute;
  inset: -5px;
}

.crumb-edit:hover {
  background: var(--app-hover);
  color: var(--app-strong);
}

.crumb-edit:active {
  background: var(--app-active);
}

.crumb-empty {
  border: none;
  background: transparent;
  border-radius: var(--radius-sm);
  color: var(--app-subtle);
  font: inherit;
  font-size: var(--text-xs);
  cursor: pointer;
  padding: 2px 4px;
  text-align: left;
  transition: background var(--dur-fast) var(--ease-standard), color var(--dur-fast) var(--ease-standard);
}

.crumb-empty:hover {
  background: var(--app-hover);
  color: var(--app-text);
}

.crumb-empty:active {
  background: var(--app-active);
}

.crumb-empty:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

// 通用 icon-btn（含 .active 激活态）已收敛为全局类（_utilities.scss 单一权威实现）

.file-column-manual-path {
  flex: 1 1 auto;
  min-width: 60px;
  padding: 2px 6px;
  height: 22px;
  background: var(--app-control);
  color: var(--app-text);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  font-family: var(--font-mono);
  outline: none;
  transition:
    border-color var(--dur-fast) var(--ease-standard),
    box-shadow var(--dur-fast) var(--ease-standard);
}

// 光环跟随本输入框自身的圆角（区别于 _base.scss 根治的裸 input 直角光环）
.file-column-manual-path:focus {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

.filter-host {
  position: relative;
  display: inline-flex;
}

.filter-popover {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  z-index: var(--z-dropdown);
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 6px 8px;
  min-width: 200px;
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  box-shadow: var(--app-shadow);
}

.filter-popover-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--app-subtle);
  pointer-events: none;
  flex-shrink: 0;
}

.filter-popover-input {
  flex: 1;
  min-width: 0;
  padding: 3px 6px;
  height: 22px;
  background: var(--app-control);
  color: var(--app-text);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  outline: none;
  font-size: var(--text-xs);
  font-family: var(--font-body);
  transition:
    border-color var(--dur-fast) var(--ease-standard),
    box-shadow var(--dur-fast) var(--ease-standard);
}

.filter-popover-input::placeholder {
  color: var(--app-subtle);
}

.filter-popover-input:focus {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

.filter-popover-clear {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  background: transparent;
  border: none;
  color: var(--app-muted);
  cursor: pointer;
  border-radius: var(--radius-sm);
  flex-shrink: 0;
}

.filter-popover-clear::after {
  content: '';
  position: absolute;
  inset: -5px;
}

.filter-popover-clear:hover {
  background: var(--app-hover);
  color: var(--app-strong);
}

.filter-popover-clear:active {
  background: var(--app-active);
}

.spin {
  flex: 0 0 auto;
  animation: file-loading-spin 0.9s linear infinite;
}

@keyframes file-loading-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
