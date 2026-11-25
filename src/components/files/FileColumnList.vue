<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import { File as FileIcon, Folder, Link2, Loader2, AlertCircle, RefreshCw } from 'lucide-vue-next';
import { formatBytes } from '@/lib/transferUtils';
import { FILE_DRAG_MIME } from '@/lib/fileTypes';
import {
  formatFileEntryOwner,
  formatFileEntryPermissions,
  formatFileEntryTime,
  inferFileEntryType,
  FILE_COLUMN_GRID_ROW,
  FILE_COLUMN_GRID_ROW_NARROW,
  FILE_COLUMN_GRID_COMPACT
} from './fileColumnUtils';
import type { RemoteFileEntry } from '@/types/domain';

const props = withDefaults(defineProps<{
  entries?: RemoteFileEntry[];
  selectionSet: Set<string>;
  listMode?: string;
  isLocal?: boolean;
  disabledHint?: string;
  currentPath?: string;
  remoteEntriesLength?: number;
  filterText?: string;
  remoteFilter?: string;
  remoteError?: string | null;
  remoteLoaded?: boolean;
  isBusy?: boolean;
  busyMessage?: string;
}>(), {
  entries: () => [],
  listMode: 'detailed',
  isLocal: false,
  disabledHint: '',
  currentPath: '',
  remoteEntriesLength: 0,
  filterText: '',
  remoteFilter: '',
  remoteError: '',
  remoteLoaded: false,
  isBusy: false,
  busyMessage: ''
});

const emit = defineEmits<{
  'list-click': [];
  'row-click': [event: MouseEvent, entry: RemoteFileEntry];
  'row-double-click': [entry: RemoteFileEntry];
  'row-context-menu': [event: MouseEvent, entry: RemoteFileEntry];
  'row-select-key': [entry: RemoteFileEntry, opts: { additive?: boolean; range?: boolean }];
  'clear-filter': [];
  retry: [];
  'drag-start': [count: number];
}>();

const listRef = ref<HTMLElement | null>(null);
const focusedIndex = ref(0);

watch(
  () => props.entries.length,
  (len) => {
    if (len <= 0) focusedIndex.value = 0;
    else if (focusedIndex.value >= len) focusedIndex.value = len - 1;
  }
);

function focusRow(idx: number, entry: RemoteFileEntry, opts: { additive?: boolean; range?: boolean; select?: boolean } = {}) {
  focusedIndex.value = idx;
  if (opts.select) {
    emit('row-select-key', entry, { additive: opts.additive, range: opts.range });
  }
  nextTick(() => {
    const el = listRef.value?.querySelector<HTMLElement>(`[data-row-idx="${idx}"]`);
    el?.focus();
    el?.scrollIntoView({ block: 'nearest' });
  });
}

function onRowKeydown(e: KeyboardEvent, idx: number, entry: RemoteFileEntry) {
  if (props.isBusy || !props.entries.length) return;
  if (e.key === 'ArrowDown') {
    e.preventDefault();
    const nextIdx = Math.min(idx + 1, props.entries.length - 1);
    focusRow(nextIdx, props.entries[nextIdx]!, { range: e.shiftKey, select: true });
    return;
  }
  if (e.key === 'ArrowUp') {
    e.preventDefault();
    const prevIdx = Math.max(idx - 1, 0);
    focusRow(prevIdx, props.entries[prevIdx]!, { range: e.shiftKey, select: true });
    return;
  }
  if (e.key === 'Home') {
    e.preventDefault();
    focusRow(0, props.entries[0]!, { range: e.shiftKey, select: true });
    return;
  }
  if (e.key === 'End') {
    e.preventDefault();
    const lastIdx = props.entries.length - 1;
    focusRow(lastIdx, props.entries[lastIdx]!, { range: e.shiftKey, select: true });
    return;
  }
  if (e.key === ' ') {
    e.preventDefault();
    emit('row-select-key', entry, { additive: true });
    return;
  }
  if (e.key === 'Enter') {
    e.preventDefault();
    emit('row-double-click', entry);
  }
}

// ============================================================
// 栏间拖拽上传（本地 → 远程）：仅本地行可拖，dragstart 写自定义 MIME。
// 与 ConnectionSidebar 的 DRAG_MIME 同模式；drop 侧判定在 FileSurface。
// v0.18：常量上移 lib/fileTypes（单一事实源，EditorSurface 拖拽打开共用）。
// ============================================================

function onRowDragStart(event: DragEvent, entry: RemoteFileEntry) {
  if (!props.isLocal || !event.dataTransfer) return;
  // 该行在选中集内 → 携带整个选中集（当前可见条目）；否则仅该行。
  // 目录也允许拖（drop 侧过滤并提示，uploadLocalEntry 仅支持文件）。
  const payloadEntries = props.selectionSet.has(entry.path)
    ? props.entries.filter((e) => props.selectionSet.has(e.path))
    : [entry];
  event.dataTransfer.setData(FILE_DRAG_MIME, JSON.stringify({ entries: payloadEntries }));
  event.dataTransfer.effectAllowed = 'copy';
  event.dataTransfer.dropEffect = 'copy';
  emit('drag-start', payloadEntries.length);
}

const activeFilterText = computed(() => (props.filterText || props.remoteFilter || '').trim());

// ============================================================
// 空态四分支（远程侧，S2）：error → 加载失败+重试；未加载 → 尚未加载远程目录；
// 加载过且空 → 该目录为空；有过滤词 → 无匹配。本地侧同构支持过滤无匹配。
// ============================================================
const emptyTitle = computed(() => {
  if (activeFilterText.value) return '无匹配';
  if (props.isLocal) return props.currentPath ? '该目录为空' : '尚未选择本地目录';
  if (props.remoteError) return '加载失败';
  if (!props.remoteEntriesLength && !props.remoteLoaded) return '尚未加载远程目录';
  return '该目录为空';
});

const emptyDesc = computed(() => {
  if (activeFilterText.value) return `无匹配「${activeFilterText.value}」的条目`;
  if (props.isLocal) {
    return props.disabledHint || (props.currentPath ? '' : '点击刷新加载本地目录');
  }
  if (props.remoteError) return props.remoteError;
  // 从未加载：空态不能是死胡同——说明「接下来会发生什么 / 该点哪里」
  if (!props.remoteEntriesLength && !props.remoteLoaded) {
    return props.currentPath ? '点击右上角刷新加载' : '连接会话后自动加载远程目录';
  }
  return '';
});
</script>

<template>
  <div
    ref="listRef"
    class="file-list file-column-list"
    :class="{ compact: listMode === 'compact' }"
    role="listbox"
    aria-multiselectable="true"
    :aria-label="isLocal ? '本地文件列表' : '远程文件列表'"
    @click.self="emit('list-click')"
  >
    <div
      v-for="(entry, idx) in entries"
      :key="entry.path"
      class="file-row"
      :class="{
        selected: selectionSet.has(entry.path),
        compact: listMode === 'compact',
        dir: entry.kind === 'directory',
        symlink: entry.kind === 'symlink'
      }"
      role="option"
      :aria-selected="selectionSet.has(entry.path) ? 'true' : 'false'"
      :tabindex="idx === focusedIndex ? 0 : -1"
      :data-row-idx="idx"
      :data-path="entry.path"
      :data-kind="entry.kind"
      :draggable="isLocal ? 'true' : 'false'"
      @focus="focusedIndex = idx"
      @keydown="onRowKeydown($event, idx, entry)"
      @dragstart="onRowDragStart($event, entry)"
      @click="focusedIndex = idx; emit('row-click', $event, entry)"
      @dblclick="emit('row-double-click', entry)"
      @contextmenu="focusedIndex = idx; emit('row-context-menu', $event, entry)"
    >
      <span class="file-row-icon">
        <Folder v-if="entry.kind === 'directory'" :size="14" aria-hidden="true" />
        <Link2 v-else-if="entry.kind === 'symlink'" :size="14" aria-hidden="true" />
        <FileIcon v-else :size="14" aria-hidden="true" />
      </span>
      <span class="col-name file-row-name" :title="entry.name">{{ entry.name }}</span>
      <template v-if="listMode === 'detailed'">
        <span
          class="col-perm file-row-perm"
          :title="entry.permissions || ''"
        >{{ formatFileEntryPermissions(entry) }}</span>
        <span
          class="col-owner file-row-owner"
          :title="formatFileEntryOwner(entry)"
        >{{ formatFileEntryOwner(entry) }}</span>
        <span class="col-size file-row-size">{{ formatBytes(entry.size) }}</span>
        <span class="col-type file-row-type" :title="inferFileEntryType(entry)">
          {{ inferFileEntryType(entry) }}
        </span>
        <span class="col-mtime file-row-time">{{ formatFileEntryTime(entry) }}</span>
      </template>
    </div>

    <div v-if="!entries.length" class="col-empty file-column-empty">
      <div class="col-empty-icon" aria-hidden="true">
        <Folder v-if="isLocal && !activeFilterText" :size="22" />
        <AlertCircle v-else-if="remoteError" :size="22" />
        <FileIcon v-else :size="22" />
      </div>
      <div class="col-empty-title">{{ emptyTitle }}</div>
      <p v-if="emptyDesc" class="col-empty-desc muted">{{ emptyDesc }}</p>
      <button
        v-if="remoteError"
        class="empty-retry"
        type="button"
        @click="emit('retry')"
      >
        <RefreshCw :size="12" aria-hidden="true" />
        <span>重试</span>
      </button>
      <button
        v-else-if="activeFilterText"
        class="empty-retry"
        type="button"
        @click="emit('clear-filter')"
      >
        <span>清除过滤</span>
      </button>
    </div>

    <div v-if="isBusy" class="file-loading-overlay" role="status" aria-live="polite">
      <Loader2 class="spin" :size="16" />
      <span>{{ busyMessage }}</span>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.file-list,
.file-column-list {
  position: relative;
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
  display: flex;
  flex-direction: column;
  background: var(--app-window);
}

.file-loading-overlay {
  position: absolute;
  inset: 0;
  z-index: var(--z-sticky);
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  padding: var(--space-3);
  background: color-mix(in oklab, var(--app-window), transparent 22%);
  color: var(--app-text);
  font-size: var(--text-xs);
  // pointer-events: none — 遮罩不拦截交互，防传输期间列表被锁死的兜底（S2）
  pointer-events: none;
}

.file-loading-overlay span {
  min-width: 0;
  max-width: calc(100% - 42px);
  padding: 7px 12px 7px 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.file-loading-overlay svg {
  padding: 7px 0 7px 12px;
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

.file-column-list.compact .file-row {
  grid-template-columns: v-bind(FILE_COLUMN_GRID_COMPACT);
  padding-block: 2px;
}

.file-row {
  display: grid;
  grid-template-columns: v-bind(FILE_COLUMN_GRID_ROW);
  align-items: center;
  gap: var(--space-2);
  min-height: 30px;
  padding: 0 12px;
  font-size: var(--text-xs);
  color: var(--app-text);
  cursor: pointer;
  user-select: none;
  border-block-end: 1px solid var(--app-border-soft);
  transition: background var(--dur-fast) var(--ease-standard);
}

.file-row:last-child {
  border-block-end: none;
}

.file-row:hover:not(.selected) {
  background: var(--app-hover);
}

.file-row:active:not(.selected) {
  background: var(--app-active);
}

.file-row:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

.file-row.selected {
  background: var(--app-selected);
}

.file-row.selected:hover {
  background: var(--accent-soft-strong);
}

.file-row.compact {
  grid-template-columns: v-bind(FILE_COLUMN_GRID_COMPACT);
}

// 窄栏回退：与 FileColumnColumns 的 @container 规则同阈值同步（去掉权限/属主组列 + 换模板）。
// 选择器带 .file-row 前缀提高特异性，防跨组件打包顺序影响（同 .col-header .col-perm）。
@container (max-width: 660px) {
  .file-row .col-perm,
  .file-row .col-owner {
    display: none;
  }

  .file-row {
    grid-template-columns: v-bind(FILE_COLUMN_GRID_ROW_NARROW);
  }
}

.file-row-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--app-subtle);
}

.file-row.dir .file-row-icon {
  color: var(--accent);
}

.file-row.symlink .file-row-icon {
  color: var(--app-muted);
}

.file-row-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--app-text);
}

.file-row.dir .file-row-name {
  font-weight: 500;
}

.file-row.selected .file-row-name {
  color: var(--app-strong);
}

.file-row-size {
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
  text-align: center;
}

// 权限：等宽 + 定长（10 字符），列内放得下 76px = 12px 等宽 × 10。与其余元数据列一起居中。
.file-row-perm {
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
  text-align: center;
  white-space: nowrap;
}

// 属主/组：与权限列同款等宽字体；用户/组名可能很长，超宽省略号收尾（title 兜底全文）。
.file-row-owner {
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
  text-align: center;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.file-row-type {
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  text-align: center;
}

.file-row-time {
  font-size: var(--text-xs);
  color: var(--app-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  text-align: center;
}

.col-empty,
.file-column-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  // 占满列表剩余空间并允许收缩（不设 min-height 下限）：
  // 固定分栏等矮容器里空态不再把列表撑出滚动条。
  // overflow hidden：容器被极端 ratio 压到低于内容 min-content（图标+标题+描述
  // ≈146px）时，内容在本框内收敛裁剪，不溢出到面板外（--terminal-h 已按视口
  // 夹紧保底文件区 120px，此条只是拖拽极限下的兜底）。
  overflow: hidden;
  flex: 1 1 auto;
  min-height: 0;
  padding: var(--space-6);
  margin: var(--space-3);
  text-align: center;
  border: 1px dashed var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel);
}

.col-empty-icon {
  width: 34px;
  height: 34px;
  display: grid;
  place-items: center;
  color: var(--app-subtle);
}

.col-empty-icon svg {
  width: 100%;
  height: 100%;
  stroke-width: 1.4;
}

.col-empty-title {
  font-size: var(--text-xs);
  font-weight: 500;
  color: var(--app-text);
}

.col-empty-desc {
  max-width: 260px;
  font-size: 11px;
  color: var(--app-muted);
  line-height: 1.55;
}

.file-column-empty .muted {
  margin: 0;
  color: var(--app-muted);
  font-size: 11px;
}

// S2：远程加载失败后的「重试」按钮
.empty-retry {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 4px 12px;
  background: var(--app-control);
  color: var(--app-text);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-pill);
  font-size: var(--text-xs);
  cursor: pointer;
  transition:
    background var(--dur-fast) var(--ease-standard),
    border-color var(--dur-fast) var(--ease-standard);
}
.empty-retry:hover {
  background: var(--app-hover);
  border-color: var(--accent);
  color: var(--accent);
}
.empty-retry:active {
  background: var(--app-active);
}
</style>
