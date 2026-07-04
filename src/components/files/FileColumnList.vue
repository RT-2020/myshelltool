<script setup>
import { File as FileIcon, Folder, Link2, Loader2 } from 'lucide-vue-next';
import {
  formatFileEntryOwner,
  formatFileEntrySize,
  formatFileEntryTime,
  inferFileEntryType
} from './fileColumnUtils.js';

defineProps({
  entries: { type: Array, default: () => [] },
  selectionSet: { type: Object, required: true },
  listMode: { type: String, default: 'detailed' },
  isLocal: { type: Boolean, default: false },
  disabledHint: { type: String, default: '' },
  currentPath: { type: String, default: '' },
  remoteEntriesLength: { type: Number, default: 0 },
  remoteFilter: { type: String, default: '' },
  isBusy: { type: Boolean, default: false },
  busyMessage: { type: String, default: '' }
});

const emit = defineEmits([
  'list-click',
  'row-click',
  'row-double-click',
  'row-context-menu'
]);
</script>

<template>
  <div
    class="file-list file-column-list"
    :class="{ compact: listMode === 'compact' }"
    @click.self="emit('list-click')"
  >
    <div
      v-for="entry in entries"
      :key="entry.path"
      class="file-row"
      :class="{
        selected: selectionSet.has(entry.path),
        compact: listMode === 'compact',
        dir: entry.kind === 'directory',
        symlink: entry.kind === 'symlink'
      }"
      :data-path="entry.path"
      :data-kind="entry.kind"
      @click="emit('row-click', $event, entry)"
      @dblclick="emit('row-double-click', entry)"
      @contextmenu="emit('row-context-menu', $event, entry)"
    >
      <span class="file-row-icon">
        <Folder v-if="entry.kind === 'directory'" :size="14" />
        <Link2 v-else-if="entry.kind === 'symlink'" :size="14" />
        <FileIcon v-else :size="14" />
      </span>
      <span class="col-name file-row-name" :title="entry.name">{{ entry.name }}</span>
      <template v-if="listMode === 'detailed'">
        <span class="col-size file-row-size">{{ formatFileEntrySize(entry.size) }}</span>
        <span class="col-type file-row-type" :title="inferFileEntryType(entry)">
          {{ inferFileEntryType(entry) }}
        </span>
        <span class="col-mtime file-row-time">{{ formatFileEntryTime(entry) }}</span>
        <span class="col-perm file-row-perm">{{ entry.permissions || '—' }}</span>
        <span class="col-owner file-row-owner" :title="formatFileEntryOwner(entry)">
          {{ formatFileEntryOwner(entry) }}
        </span>
      </template>
    </div>

    <div v-if="!entries.length" class="col-empty file-column-empty">
      <div class="col-empty-icon" aria-hidden="true">
        <Folder v-if="isLocal" :size="22" />
        <FileIcon v-else :size="22" />
      </div>
      <div class="col-empty-title">{{ isLocal ? '尚未选择本地目录' : '未连接到远程主机' }}</div>
      <p class="col-empty-desc muted">
        <template v-if="isLocal">
          {{ disabledHint || (currentPath ? '该目录为空' : '点击刷新加载本地目录') }}
        </template>
        <template v-else>
          {{ remoteEntriesLength ? '无匹配「' + remoteFilter + '」的条目' : '尚未加载远程目录' }}
        </template>
      </p>
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
  pointer-events: auto;
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
  grid-template-columns: 16px minmax(0, 1fr);
  padding-block: 2px;
}

.file-row {
  display: grid;
  grid-template-columns: 16px minmax(0, 1fr) 64px 56px 130px 56px 92px;
  align-items: center;
  gap: var(--space-2);
  min-height: 30px;
  padding: 0 12px;
  font-size: 12.5px;
  color: var(--app-text);
  cursor: pointer;
  user-select: none;
  border-block-end: 1px solid var(--app-border-soft);
  transition: background var(--motion-fast) var(--ease-standard);
}

.file-row:last-child {
  border-block-end: none;
}

.file-row:hover:not(.selected) {
  background: var(--app-hover);
}

.file-row.selected {
  background: var(--app-selected);
}

.file-row.selected:hover {
  background: var(--accent-soft-strong);
}

.file-row.compact {
  grid-template-columns: 16px minmax(0, 1fr);
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
  text-align: end;
}

.file-row-type {
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.file-row-time {
  font-size: var(--text-xs);
  color: var(--app-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.file-row-perm {
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
  text-align: end;
  white-space: nowrap;
}

.file-row-owner {
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.col-empty,
.file-column-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: var(--space-6);
  margin: var(--space-3);
  min-height: 180px;
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
  font-size: 12.5px;
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
</style>
