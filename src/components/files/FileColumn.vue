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
} from 'lucide-vue-next';
import { useFilesStore } from '@/stores/files.js';
import { useUiStore } from '@/stores/ui.js';
import FileColumnHeader from './FileColumnHeader.vue';
import FileColumnList from './FileColumnList.vue';
import {
  buildPathCrumbs,
  inferFileEntryType
} from './fileColumnUtils.js';

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
  manualLocalPathInput,
  selectedRemotePaths,
  // local-side refs
  localPath,
  localEntries,
  localViewMode,
  selectedLocalPaths,
  remoteBusy,
  localBusy,
  remoteBusyMessage,
  localBusyMessage
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

const selectionSet = computed(() => (isLocal.value ? selectedLocalPaths.value : selectedRemotePaths.value));
const selectionCount = computed(() => selectionSet.value.size);
const sortKey = computed(() => (isLocal.value ? localSortKey.value : remoteSortKey.value));
const sortDir = computed(() => (isLocal.value ? localSortDir.value : remoteSortDir.value));
const isBusy = computed(() => (isLocal.value ? localBusy.value : remoteBusy.value));
const busyMessage = computed(() => (isLocal.value ? localBusyMessage.value : remoteBusyMessage.value));

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

// 类型推断（排序用，与模板内 inferType 同义；定义在 computed 之前避免前向引用）。
function typeOfEntry(e) {
  return inferFileEntryType(e);
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
  const ownerOf = (e) => [e.user || '', e.group || ''].join(':');
  return list.sort((a, b) => {
    const aDir = a.kind === 'directory' ? 0 : 1;
    const bDir = b.kind === 'directory' ? 0 : 1;
    if (aDir !== bDir) return aDir - bDir;
    let av, bv;
    if (key === 'size') { av = a.size || 0; bv = b.size || 0; }
    else if (key === 'modified') { av = Number(a.modified) || 0; bv = Number(b.modified) || 0; }
    else if (key === 'type') { av = typeOfEntry(a); bv = typeOfEntry(b); }
    else if (key === 'permissions') {
      av = a.permissions ? parseInt(a.permissions, 8) || 0 : 0;
      bv = b.permissions ? parseInt(b.permissions, 8) || 0 : 0;
    }
    else if (key === 'owner') { av = ownerOf(a); bv = ownerOf(b); }
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
  if (isBusy.value) return;
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
  if (isBusy.value) return;
  if (isLocal.value) {
    if (entry.kind === 'directory') {
      filesStore.navigateLocalPath(entry.path);
    } else {
      filesStore.uploadLocalEntry(entry);
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
  if (isBusy.value) return;
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
  if (isBusy.value) return;
  if (isLocal.value) filesStore.clearLocalSelection();
  else filesStore.clearRemoteSelection();
}

// ============================================================
// Toolbar actions
// ============================================================
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
  if (isLocal.value) filesStore.setManualLocalPath(event.target.value);
  else filesStore.setManualRemotePath(event.target.value);
}
function onManualPathEnter() {
  if (isBusy.value) return;
  if (isLocal.value) filesStore.goToManualLocalPath();
  else filesStore.goToManualRemotePath();
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

const filterValue = computed(() => (isLocal.value ? localFilterQuery.value : remoteFilter.value));
const showFilterClear = computed(() => filterValue.value !== '');

// ============================================================
// 路径面包屑 + 可编辑切换
// 默认渲染面包屑（每段可点跳转上级），点末端「✎」或双击切换为原始路径 input（回车跳转）。
// ============================================================
const pathEditing = ref(false);

// 把路径切成累积段：/srv/app/release → [{/, /}, {srv, /srv}, {app, /srv/app}, ...]。
// 同时支持 Unix '/' 与 Windows '\'。
const crumbs = computed(() => {
  return buildPathCrumbs(currentPath.value);
});

const manualPathInput = computed(() => (
  isLocal.value ? manualLocalPathInput.value : manualRemotePathInput.value
));

function enterPathEditing() {
  // 切到编辑态时把当前路径填进输入框（避免空白）。
  if (isLocal.value) filesStore.setManualLocalPath(currentPath.value || '');
  else filesStore.setManualRemotePath(currentPath.value || '');
  pathEditing.value = true;
}
function exitPathEditing() {
  pathEditing.value = false;
}
function onPathInputKeydown(event) {
  if (event.key === 'Escape') {
    event.preventDefault();
    exitPathEditing();
  } else if (event.key === 'Enter') {
    event.preventDefault();
    onManualPathEnter();
    exitPathEditing();
  }
}
function crumbClick(seg) {
  if (isBusy.value) return;
  if (isLocal.value) filesStore.navigateLocalPath(seg.path);
  else filesStore.navigateRemotePath(seg.path);
}

// ============================================================
// 过滤浮动窗（挂在列表右上角）
// click-outside 关闭；filterValue 非空时图标高亮。
// ============================================================
</script>

<template>
  <section
    ref="columnRef"
    class="file-column"
    :class="[`file-column--${kind}`, { 'is-local-disabled': isLocal && disabledHint }]"
    @mouseenter="onColumnMouseEnter"
    @mouseleave="onColumnMouseLeave"
  >
    <!-- Header: 单行 title+count / 路径(面包屑或编辑input) / 过滤按钮 / 上级目录+刷新。
         过滤框已移到列表右上角浮动小窗，路径框内部显示可点击面包屑（点✎切原始 input）。 -->
    <FileColumnHeader
      :kind="kind"
      :column-title="columnTitle"
      :selection-count="selectionCount"
      :entry-count="effectiveEntries().length"
      :crumbs="crumbs"
      :path-editing="pathEditing"
      :manual-path-input="manualPathInput"
      :filter-value="filterValue"
      :show-filter-clear="showFilterClear"
      :is-busy="isBusy"
      :disable-up="isBusy || (isLocal && !localPath)"
      :disable-refresh="isBusy || (isLocal && !!disabledHint)"
      @enter-path-editing="enterPathEditing"
      @manual-path-input="onManualPathInput"
      @path-input-keydown="onPathInputKeydown"
      @crumb-click="crumbClick"
      @toggle-filter-input="onFilterInput"
      @clear-filter="clearFilter"
      @go-up="goUp"
      @refresh="refresh"
    >
      <template #actions-leading>
        <slot name="actions-leading" />
      </template>
    </FileColumnHeader>

    <!-- Column headers (sortable) — only in detailed list mode.
         列：名称 / 大小 / 类型 / 修改时间 / 权限 / 用户组。-->
    <div v-if="remoteListMode === 'detailed'" class="col-header file-column-cols">
      <button
        class="col-name col-sort"
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
        class="col-size col-sort col-sort--num"
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
        class="col-type col-sort"
        :class="{ active: sortKey === 'type' }"
        type="button"
        @click="setSort('type')"
      >
        <span>类型</span>
        <component
          v-if="sortKey === 'type'"
          :is="sortDir === 'asc' ? ChevronUp : ChevronDown"
          :size="12"
        />
      </button>
      <button
        class="col-mtime col-sort"
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
      <button
        class="col-perm col-sort col-sort--num"
        :class="{ active: sortKey === 'permissions' }"
        type="button"
        @click="setSort('permissions')"
      >
        <span>权限</span>
        <component
          v-if="sortKey === 'permissions'"
          :is="sortDir === 'asc' ? ChevronUp : ChevronDown"
          :size="12"
        />
      </button>
      <button
        class="col-owner col-sort"
        :class="{ active: sortKey === 'owner' }"
        type="button"
        @click="setSort('owner')"
      >
        <span>用户组</span>
        <component
          v-if="sortKey === 'owner'"
          :is="sortDir === 'asc' ? ChevronUp : ChevronDown"
          :size="12"
        />
      </button>
    </div>

    <FileColumnList
      :entries="effectiveEntries()"
      :selection-set="selectionSet"
      :list-mode="remoteListMode"
      :is-local="isLocal"
      :disabled-hint="disabledHint"
      :current-path="currentPath"
      :remote-entries-length="remoteEntries.length"
      :remote-filter="remoteFilter"
      :is-busy="isBusy"
      :busy-message="busyMessage"
      @list-click="onListClickSelf"
      @row-click="onRowClick"
      @row-double-click="onRowDblClick"
      @row-context-menu="onContextMenu"
    />
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// Single-pane column. Low visual weight: 1px chrome edge, sticky headers,
// hairline row separators, no card stacking. Two columns separated by a
// 1px border-inline-end on the first column (applied by FileSurface).
.file-column,
.file-pane {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  height: 100%;
  background: var(--app-window);
  color: var(--app-text);
  font-family: var(--font-body);
  font-size: var(--text-sm);
  overflow: hidden;
}

.file-column.is-local-disabled {
  opacity: 0.6;
}

// Sortable column headers. 列：名称(flex) / 大小 / 类型 / 修改时间 / 权限 / 用户组。
.col-header,
.file-column-cols {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 64px 56px 130px 56px 92px;
  gap: 8px;
  padding: 0 12px;
  min-height: 30px;
  align-items: center;
  background: var(--app-panel);
  border-block-end: 1px solid var(--app-border);
  font: 500 10.5px var(--font-display);
  color: var(--app-muted);
  position: sticky;
  top: 34px;
  z-index: calc(var(--z-sticky) - 1);
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
  min-width: 0;
}
// 数值列标题右对齐（与数值单元格一致）。
.col-sort--num { justify-content: flex-end; text-align: end; }
.col-sort:hover { color: var(--app-strong); }
.col-sort.active { color: var(--accent); }

</style>
