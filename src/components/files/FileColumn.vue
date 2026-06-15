<script setup>
/**
 * FileColumn — Wave 3 Step 3.4
 * Single-pane file list (local or remote) — Tabby-style hairline rows, sticky
 * header, low visual weight.
 *
 * Store-bound via `kind` prop:
 *   - kind='local'  → localPath / localEntries / selectedLocalPaths + fs_local_* ops
 *   - kind='remote' → remotePath / sortedRemoteEntries / selectedRemotePaths + sftp_* ops
 *
 * Selection / sort / filter / breadcrumb all flow through the files store.
 * Right-click opens the central files.contextMenu (read by FileSurface).
 * Rename / mkdir via uiStore.modal (same modal state App.vue reads).
 * Keyboard: F2 rename, Del remove, F5 refresh, Ctrl+A select all, Esc clear.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { storeToRefs } from 'pinia';
import {
  ChevronUp,
  ChevronDown,
  Folder,
  File as FileIcon,
  Link2,
  ArrowUp,
  RefreshCw,
  Filter as FilterIcon,
  X
} from 'lucide-vue-next';
import { useFilesStore } from '@/stores/files.js';
import { useUiStore } from '@/stores/ui.js';
import AppBreadcrumb from '@/components/ui/AppBreadcrumb.vue';

const props = defineProps({
  kind: { type: String, required: true, validator: (v) => v === 'local' || v === 'remote' },
  // Visual heading label ("本地" / "远程"); defaults based on kind.
  title: { type: String, default: '' },
  // Disable interaction when desktop runtime unavailable (local only).
  disabledHint: { type: String, default: '' }
});
const emit = defineEmits([
  'item-click',
  'item-double-click',
  'sort-change',
  'context-menu-open',
  'selection-change'
]);

const filesStore = useFilesStore();
const uiStore = useUiStore();
const {
  // remote-side refs
  remotePath,
  remoteEntries,
  sortedRemoteEntries,
  remoteSortKey,
  remoteSortDir,
  remoteFilter,
  remoteListMode,
  manualRemotePathInput,
  remoteBreadcrumb,
  selectedRemotePaths,
  // local-side refs
  localPath,
  localEntries,
  localViewMode,
  selectedLocalPaths
} = storeToRefs(filesStore);

const columnTitle = computed(() => props.title || (props.kind === 'local' ? '本地' : '远程'));

// ============================================================
// Per-kind computed bindings
// ============================================================
const isLocal = computed(() => props.kind === 'local');

const entries = computed(() => {
  if (isLocal.value) {
    // Local entries: respect current filter (apply same logic as remote sort/filter for parity)
    const q = (localFilterQuery.value || '').trim().toLowerCase();
    if (!q) return localEntries.value;
    return localEntries.value.filter(e => e.name.toLowerCase().includes(q));
  }
  return sortedRemoteEntries.value;
});

const currentPath = computed(() => (isLocal.value ? localPath.value : remotePath.value));

const breadcrumbItems = computed(() => {
  if (isLocal.value) {
    // Build breadcrumb from localPath segments (platform-aware: \\ or /).
    if (!localPath.value) return [];
    const parts = localPath.value.split(/[\\/]/).filter(Boolean);
    const crumbs = [];
    let acc = '';
    for (let i = 0; i < parts.length; i++) {
      acc = acc ? acc + '/' + parts[i] : parts[i];
      // Keep Windows drive prefix on first segment.
      if (i === 0 && /^[a-zA-Z]:$/.test(parts[0])) {
        acc = parts[0] + '\\';
      }
      crumbs.push({ name: parts[i], path: acc });
    }
    return crumbs;
  }
  return remoteBreadcrumb.value;
});

const selectionSet = computed(() => (isLocal.value ? selectedLocalPaths.value : selectedRemotePaths.value));
const selectionCount = computed(() => selectionSet.value.size);
const sortKey = computed(() => (isLocal.value ? localSortKey.value : remoteSortKey.value));
const sortDir = computed(() => (isLocal.value ? localSortDir.value : remoteSortDir.value));

// Local filter/sort kept locally per-column (files store only tracks remote).
// These preserve parity without polluting the store with local equivalents.
const localFilterQuery = ref('');
const localSortKey = ref('name');
const localSortDir = ref('asc');

function setLocalSort(key) {
  if (localSortKey.value === key) {
    localSortDir.value = localSortDir.value === 'asc' ? 'desc' : 'asc';
  } else {
    localSortKey.value = key;
    localSortDir.value = 'asc';
  }
}

// Apply local sort (files store doesn't expose local sort, so re-sort here).
const sortedLocalEntries = computed(() => {
  // entries() already filtered; but localEntries sort needs to happen before filter for stability.
  const q = (localFilterQuery.value || '').trim().toLowerCase();
  const list = q
    ? localEntries.value.filter(e => e.name.toLowerCase().includes(q))
    : localEntries.value.slice();
  const key = localSortKey.value;
  const dir = localSortDir.value === 'asc' ? 1 : -1;
  return list.sort((a, b) => {
    const aDir = a.kind === 'directory' ? 0 : 1;
    const bDir = b.kind === 'directory' ? 0 : 1;
    if (aDir !== bDir) return aDir - bDir;
    let av, bv;
    if (key === 'size') { av = a.size || 0; bv = b.size || 0; }
    else if (key === 'modified') { av = Number(a.modified) || 0; bv = Number(b.modified) || 0; }
    else { av = a.name.toLowerCase(); bv = b.name.toLowerCase(); }
    if (av < bv) return -1 * dir;
    if (av > bv) return 1 * dir;
    return 0;
  });
});

// Override entries() computed: use sortedLocalEntries when local.
function effectiveEntries() {
  return isLocal.value ? sortedLocalEntries.value : sortedRemoteEntries.value;
}

// ============================================================
// Click handlers (migrated from App.vue onRemoteRowClick etc.)
// ============================================================
function onRowClick(event, entry) {
  if (isLocal.value) {
    filesStore.toggleLocalSelection(entry.path, { additive: event.ctrlKey || event.metaKey });
  } else if (event.ctrlKey || event.metaKey) {
    filesStore.toggleRemoteSelection(entry.path, { additive: true });
  } else if (event.shiftKey) {
    filesStore.toggleRemoteSelection(entry.path, { range: true });
  } else {
    filesStore.toggleRemoteSelection(entry.path, { additive: false });
  }
  emit('item-click', entry);
}

function onRowDblClick(entry) {
  if (isLocal.value) {
    if (entry.kind === 'directory') {
      filesStore.navigateLocalPath(entry.path);
    } else {
      // Queue upload of single local file
      uiStore.statusMessage = '本地文件双击：' + entry.name + '（上传功能待接线）';
    }
  } else if (entry.kind === 'directory' || entry.kind === 'symlink') {
    filesStore.navigateRemotePath(entry.path);
  } else {
    filesStore.downloadEntry(entry);
  }
  emit('item-double-click', entry);
}

function onContextMenu(event, entry) {
  event.preventDefault();
  if (isLocal.value) {
    if (!selectedLocalPaths.value.has(entry.path)) {
      filesStore.toggleLocalSelection(entry.path, { additive: false });
    }
  } else if (!selectedRemotePaths.value.has(entry.path)) {
    filesStore.toggleRemoteSelection(entry.path, { additive: false });
  }
  filesStore.openContextMenu({ x: event.clientX, y: event.clientY, side: props.kind, entry });
  emit('context-menu-open', entry, event.clientX, event.clientY);
}

function onListClickSelf() {
  if (isLocal.value) filesStore.clearLocalSelection();
  else filesStore.clearRemoteSelection();
}

// ============================================================
// Toolbar actions
// ============================================================
function navigateTo(path) {
  if (isLocal.value) filesStore.navigateLocalPath(path);
  else filesStore.navigateRemotePath(path);
}
function goUp() {
  if (isLocal.value) filesStore.navigateLocalUp();
  else filesStore.navigateRemoteUp();
}
function refresh() {
  if (isLocal.value) filesStore.refreshLocalFiles();
  else filesStore.refreshRemoteFiles();
}
function clearFilter() {
  if (isLocal.value) localFilterQuery.value = '';
  else filesStore.setRemoteFilter('');
}
function onFilterInput(event) {
  if (isLocal.value) localFilterQuery.value = event.target.value;
  else filesStore.setRemoteFilter(event.target.value);
}
function onManualPathInput(event) {
  // Only remote has a manual path input
  filesStore.setManualRemotePath(event.target.value);
}
function onManualPathEnter() {
  filesStore.goToManualRemotePath();
}
function setSort(field) {
  emit('sort-change', field);
  if (isLocal.value) setLocalSort(field);
  else filesStore.setRemoteSort(field);
}

// ============================================================
// Keyboard shortcuts (F2 / Del / F5 / Ctrl+A / Esc)
// Bound at column mount; only fire when column DOM is focused or files tab active.
// ============================================================
const columnRef = ref(null);

function onKeydown(event) {
  // Active tab must be files; ignore if user is typing in an input/textarea outside the column.
  if (uiStore.activeTab !== 'files') return;
  const tag = event.target?.tagName;
  const isInputLike = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';
  // Allow F2/Del/F5/Esc through input fields only when the focus is inside this column
  // (e.g., manual path input). For simplicity, allow Esc/F2/Del/F5 only when not in an input.
  const mod = event.ctrlKey || event.metaKey;

  // Ctrl+A — select all (only when this column is the "active" one — heuristic: not in input)
  if (!isInputLike && mod && (event.key === 'a' || event.key === 'A')) {
    // Both columns would fight for Ctrl+A; delegate to whichever the user most recently focused.
    // For safety, only handle if this column's root is hovered or focused.
    if (!isColumnActive()) return;
    event.preventDefault();
    if (isLocal.value) filesStore.selectAllLocal();
    else filesStore.selectAllRemote();
    return;
  }
  if (isInputLike) return;

  if (!isColumnActive()) return;

  if (event.key === 'F2') {
    event.preventDefault();
    triggerRename();
    return;
  }
  if (event.key === 'Delete') {
    event.preventDefault();
    triggerDelete();
    return;
  }
  if (event.key === 'F5') {
    event.preventDefault();
    refresh();
    return;
  }
  if (event.key === 'Escape') {
    if (filesStore.contextMenu.visible) filesStore.closeContextMenu();
    else if (isLocal.value) filesStore.clearLocalSelection();
    else filesStore.clearRemoteSelection();
    return;
  }
}

let activeHover = false;
function onColumnMouseEnter() { activeHover = true; }
function onColumnMouseLeave() { activeHover = false; }
function isColumnActive() {
  // Active when hovered OR contains document.activeElement OR no other column has been hovered.
  // Simple heuristic: hovered column wins.
  return activeHover || columnRef.value?.contains(document.activeElement);
}

function triggerRename() {
  if (isLocal.value) {
    const [path] = selectedLocalPaths.value;
    if (!path) return;
    const entry = localEntries.value.find(e => e.path === path);
    if (entry) uiStore.modal = { type: 'localRename', entry };
  } else {
    if (selectedRemoteEntries.value.length !== 1) return;
    const entry = selectedRemoteEntries.value[0];
    uiStore.modal = { type: 'rename', entry };
  }
}

function triggerDelete() {
  if (isLocal.value) {
    const paths = Array.from(selectedLocalPaths.value);
    if (!paths.length) return;
    filesStore.localDelete(paths);
    return;
  }
  if (selectedRemotePaths.value.size > 1) {
    filesStore.batchRemoteDelete();
  } else if (selectedRemoteEntries.value.length === 1) {
    filesStore.removeRemote(selectedRemoteEntries.value[0]);
  }
}

// Importing the computed here would create a duplicate; rely on store helper.
const selectedRemoteEntries = computed(() =>
  remoteEntries.value.filter(e => selectedRemotePaths.value.has(e.path))
);

onMounted(() => {
  window.addEventListener('keydown', onKeydown);
});
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown);
});

// ============================================================
// Display helpers
// ============================================================
function formatBytes(bytes) {
  const size = Number(bytes) || 0;
  if (size >= 1024 * 1024) return Math.round(size / 1024 / 1024) + ' MB';
  if (size >= 1024) return Math.round(size / 1024) + ' KB';
  return size + ' B';
}

function formatEntryTime(entry) {
  if (!entry.modified) return '—';
  if (/^\d+$/.test(entry.modified) && entry.modified.length >= 8) {
    const d = new Date(Number(entry.modified) * 1000);
    if (!isNaN(d.getTime())) return d.toLocaleString();
  }
  return entry.modified;
}

const filterValue = computed(() => (isLocal.value ? localFilterQuery.value : remoteFilter.value));
const showFilterClear = computed(() => filterValue.value !== '');
</script>

<template>
  <section
    ref="columnRef"
    class="file-column"
    :class="[`file-column--${kind}`, { 'is-local-disabled': isLocal && disabledHint }]"
    @mouseenter="onColumnMouseEnter"
    @mouseleave="onColumnMouseLeave"
  >
    <!-- Header: title + count + actions (sticky) -->
    <header class="file-column-head">
      <div class="file-column-head-left">
        <strong class="file-column-title">{{ columnTitle }}</strong>
        <span class="file-column-count" v-if="selectionCount">{{ selectionCount }} 项已选</span>
        <span class="file-column-count" v-else>{{ effectiveEntries().length }} 项</span>
      </div>
      <div class="file-column-head-right">
        <button
          class="icon-btn"
          type="button"
          title="上级目录"
          :disabled="isLocal && !localPath"
          @click="goUp"
        >
          <ArrowUp :size="14" />
        </button>
        <button
          class="icon-btn"
          type="button"
          title="刷新"
          :disabled="isLocal && !!disabledHint"
          @click="refresh"
        >
          <RefreshCw :size="14" />
        </button>
      </div>
    </header>

    <!-- Breadcrumb + path input (remote only) -->
    <div class="file-column-pathrow">
      <AppBreadcrumb
        v-if="breadcrumbItems.length"
        :items="breadcrumbItems"
        @navigate="navigateTo"
      />
      <span v-else class="file-column-path-empty">
        {{ isLocal ? (disabledHint || '点击刷新加载本地目录') : '尚未加载远程目录' }}
      </span>

      <!-- Manual path input: remote only -->
      <input
        v-if="!isLocal"
        class="file-column-manual-path"
        :value="manualRemotePathInput"
        placeholder="输入路径后回车跳转"
        spellcheck="false"
        @input="onManualPathInput"
        @keydown.enter="onManualPathEnter"
      />
    </div>

    <!-- Filter row -->
    <div class="file-column-filterrow">
      <span class="file-column-filter-icon"><FilterIcon :size="12" /></span>
      <input
        class="file-column-filter-input"
        :value="filterValue"
        placeholder="过滤当前目录..."
        spellcheck="false"
        @input="onFilterInput"
      />
      <button
        v-if="showFilterClear"
        class="file-column-filter-clear"
        type="button"
        title="清空"
        @click="clearFilter"
      >
        <X :size="12" />
      </button>
    </div>

    <!-- Column headers (sortable) — only in detailed list mode -->
    <div v-if="remoteListMode === 'detailed'" class="file-column-cols">
      <button
        class="col-sort"
        :class="{ active: sortKey === 'name' }"
        type="button"
        @click="setSort('name')"
      >
        <span>名称</span>
        <component
          v-if="sortKey === 'name'"
          :is="sortDir === 'asc' ? ChevronUp : ChevronDown"
          :size="12"
        />
      </button>
      <button
        class="col-sort"
        :class="{ active: sortKey === 'size' }"
        type="button"
        @click="setSort('size')"
      >
        <span>大小</span>
        <component
          v-if="sortKey === 'size'"
          :is="sortDir === 'asc' ? ChevronUp : ChevronDown"
          :size="12"
        />
      </button>
      <button
        class="col-sort"
        :class="{ active: sortKey === 'modified' }"
        type="button"
        @click="setSort('modified')"
      >
        <span>修改时间</span>
        <component
          v-if="sortKey === 'modified'"
          :is="sortDir === 'asc' ? ChevronUp : ChevronDown"
          :size="12"
        />
      </button>
    </div>

    <!-- File list -->
    <div
      class="file-column-list"
      :class="{ compact: remoteListMode === 'compact' }"
      @click.self="onListClickSelf"
    >
      <div
        v-for="entry in effectiveEntries()"
        :key="entry.path"
        class="file-row"
        :class="{
          selected: selectionSet.has(entry.path),
          compact: remoteListMode === 'compact',
          dir: entry.kind === 'directory',
          symlink: entry.kind === 'symlink'
        }"
        :data-path="entry.path"
        :data-kind="entry.kind"
        @click="onRowClick($event, entry)"
        @dblclick="onRowDblClick(entry)"
        @contextmenu="onContextMenu($event, entry)"
      >
        <span class="file-row-icon">
          <Folder v-if="entry.kind === 'directory'" :size="14" />
          <Link2 v-else-if="entry.kind === 'symlink'" :size="14" />
          <FileIcon v-else :size="14" />
        </span>
        <span class="file-row-name" :title="entry.name">{{ entry.name }}</span>
        <span v-if="remoteListMode === 'detailed'" class="file-row-size">{{ formatBytes(entry.size) }}</span>
        <span v-if="remoteListMode === 'detailed'" class="file-row-time">{{ formatEntryTime(entry) }}</span>
      </div>

      <div v-if="!effectiveEntries().length" class="file-column-empty">
        <p class="muted">
          <template v-if="isLocal">
            {{ disabledHint || (currentPath ? '该目录为空' : '点击刷新加载本地目录') }}
          </template>
          <template v-else>
            {{ remoteEntries.length ? '无匹配「' + remoteFilter + '」的条目' : '尚未加载远程目录' }}
          </template>
        </p>
      </div>
    </div>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// Single-pane column. Low visual weight: 1px chrome edge, sticky headers,
// hairline row separators, no card stacking. Two columns separated by a
// 1px border-inline-end on the first column (applied by FileSurface).
.file-column {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  height: 100%;
  background: var(--app-panel);
  color: var(--app-text);
  font-family: var(--font-body);
  font-size: var(--text-sm);
  overflow: hidden;
}

.file-column.is-local-disabled {
  opacity: 0.6;
}

// Header row: title + count + actions.
.file-column-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  position: sticky;
  top: 0;
  z-index: var(--z-sticky);
  background: transparent;
  border-block-end: 1px solid var(--app-border);
}
.file-column-head-left {
  display: inline-flex;
  align-items: baseline;
  gap: var(--space-2);
  min-width: 0;
}
.file-column-title {
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--app-text);
}
.file-column-count {
  font-size: var(--text-xs);
  color: var(--app-muted);
}
.file-column-head-right {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  background: transparent;
  border: none;
  color: var(--app-muted);
  cursor: pointer;
  border-radius: var(--radius-sm);
  transition: background var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard);
}
.icon-btn:hover:not(:disabled) {
  background: var(--app-hover);
  color: var(--app-strong);
}
.icon-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

// Path row: breadcrumb + manual path input (remote only).
.file-column-pathrow {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-3);
  border-block-end: 1px solid var(--app-border);
  background: var(--app-panel-2);
  min-height: 26px;
}
.file-column-path-empty {
  font-size: var(--text-xs);
  color: var(--app-muted);
}
.file-column-manual-path {
  margin-left: auto;
  width: 220px;
  max-width: 40%;
  padding: 2px 6px;
  background: var(--app-control);
  color: var(--app-text);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  font-family: var(--font-mono);
  outline: none;
}
.file-column-manual-path:focus {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

// Filter row.
.file-column-filterrow {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: var(--space-1) var(--space-3);
  border-block-end: 1px solid var(--app-border);
  background: var(--app-panel);
  position: relative;
}
.file-column-filter-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--app-subtle);
  pointer-events: none;
}
.file-column-filter-input {
  flex: 1;
  min-width: 0;
  padding: 2px 6px;
  background: transparent;
  color: var(--app-text);
  border: none;
  outline: none;
  font-size: var(--text-xs);
  font-family: var(--font-body);
}
.file-column-filter-input::placeholder {
  color: var(--app-subtle);
}
.file-column-filter-clear {
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
}
.file-column-filter-clear:hover {
  background: var(--app-hover);
  color: var(--app-strong);
}

// Sortable column headers.
.file-column-cols {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 80px 160px;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-3);
  background: var(--app-panel-2);
  border-block-end: 1px solid var(--app-border);
  font-size: var(--text-xs);
  color: var(--app-muted);
  position: sticky;
  top: 0;
}
.col-sort {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  background: transparent;
  border: none;
  padding: 0;
  color: var(--app-muted);
  cursor: pointer;
  font-size: var(--text-xs);
  text-align: start;
  user-select: none;
  transition: color var(--motion-fast) var(--ease-standard);
}
.col-sort:hover { color: var(--app-strong); }
.col-sort.active { color: var(--accent); }

// File list — fills remaining column height, scrolls.
.file-column-list {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
  display: flex;
  flex-direction: column;
}
.file-column-list.compact .file-row {
  grid-template-columns: 16px minmax(0, 1fr);
  padding-block: 2px;
}

// Single file row. Hairline separators via border-block-end on rows.
.file-row {
  display: grid;
  grid-template-columns: 16px minmax(0, 1fr) 80px 160px;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-3);
  font-size: var(--text-sm);
  color: var(--app-text);
  cursor: pointer;
  user-select: none;
  border-block-end: 1px solid color-mix(in oklab, var(--app-border), transparent 60%);
  transition: background var(--motion-fast) var(--ease-standard);
}
.file-row:last-child {
  border-block-end: none;
}
.file-row:hover:not(.selected) {
  background: var(--app-hover);
}
.file-row.selected {
  // Accent-tinted selection (color-mix half-opacity); no thick border.
  background: color-mix(in oklab, var(--accent), transparent 84%);
}
.file-row.selected:hover {
  background: color-mix(in oklab, var(--accent), transparent 78%);
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
.file-row.dir .file-row-icon { color: var(--accent); }
.file-row.symlink .file-row-icon { color: var(--app-muted); }

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
.file-row-time {
  font-size: var(--text-xs);
  color: var(--app-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

// Empty state.
.file-column-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-6);
  text-align: center;
}
.file-column-empty .muted {
  margin: 0;
  color: var(--app-muted);
  font-size: var(--text-sm);
}
</style>
