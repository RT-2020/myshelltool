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
  ChevronRight,
  Folder,
  File as FileIcon,
  Link2,
  ArrowUp,
  RefreshCw,
  Filter as FilterIcon,
  Pencil,
  Loader2,
  X
} from 'lucide-vue-next';
import { useFilesStore } from '@/stores/files.js';
import { useUiStore } from '@/stores/ui.js';

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
  if (e.kind === 'directory') return 'DIR';
  if (e.kind === 'symlink') return 'LNK';
  const dot = e.name.lastIndexOf('.');
  if (dot <= 0 || dot === e.name.length - 1) return 'FILE';
  return e.name.slice(dot + 1).toUpperCase();
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

// ============================================================
// 路径面包屑 + 可编辑切换
// 默认渲染面包屑（每段可点跳转上级），点末端「✎」或双击切换为原始路径 input（回车跳转）。
// ============================================================
const pathEditing = ref(false);

// 把路径切成累积段：/srv/app/release → [{/, /}, {srv, /srv}, {app, /srv/app}, ...]。
// 同时支持 Unix '/' 与 Windows '\'。
const crumbs = computed(() => {
  const raw = currentPath.value || '';
  if (!raw) return [];
  // 规范分隔为 '/' 便于切分；Windows 盘符 D:\ → D:/。
  const norm = raw.replace(/\\/g, '/');
  const isAbs = norm.startsWith('/');
  const segs = norm.split('/').filter(Boolean);
  const result = [];
  if (isAbs) {
    // Unix 根
    result.push({ label: '/', path: '/' });
    let acc = '';
    for (const seg of segs) {
      acc += '/' + seg;
      result.push({ label: seg, path: acc });
    }
  } else {
    // 相对 / Windows 路径（首段可能是盘符 D:）
    let acc = '';
    segs.forEach((seg, idx) => {
      acc = idx === 0 ? seg : acc + '/' + seg;
      // Windows 盘符根：D: → D:\ （navigateLocalPath 能识别）
      const displayPath = /^[a-zA-Z]:$/.test(seg) ? seg + '\\' : acc;
      result.push({ label: seg, path: displayPath });
    });
  }
  return result;
});

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
const filterOpen = ref(false);
const filterPopoverRef = ref(null);
const filterBtnRef = ref(null);
const filterInputRef = ref(null);

function toggleFilter(event) {
  event.stopPropagation();
  filterOpen.value = !filterOpen.value;
  if (filterOpen.value) {
    // 打开后聚焦输入框（nextTick 确保 DOM 已渲染）。
    requestAnimationFrame(() => filterInputRef.value?.focus());
  }
}
function onWindowClick(event) {
  if (!filterOpen.value) return;
  const pop = filterPopoverRef.value;
  const btn = filterBtnRef.value;
  if (pop && !pop.contains(event.target) && btn && !btn.contains(event.target)) {
    filterOpen.value = false;
  }
}
onMounted(() => window.addEventListener('click', onWindowClick));
onBeforeUnmount(() => window.removeEventListener('click', onWindowClick));

// ============================================================
// 新列展示 helper：类型 / 权限 / 用户:组
// ============================================================
function inferType(entry) {
  if (entry.kind === 'directory') return 'DIR';
  if (entry.kind === 'symlink') return 'LNK';
  const dot = entry.name.lastIndexOf('.');
  if (dot <= 0 || dot === entry.name.length - 1) return 'FILE';
  // 扩展名大写，超长截断（如 .tar.gz 取最后段 GZ）。
  const ext = entry.name.slice(dot + 1).toUpperCase();
  return ext.length > 5 ? ext.slice(0, 5) : ext;
}

function formatOwner(entry) {
  const u = entry.user;
  const g = entry.group;
  if (!u && !g) return '—';
  if (u && g) return u + ':' + g;
  return u || g || '—';
}
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
    <header class="pane-header file-column-head">
      <span class="pane-tag" :class="kind">{{ columnTitle }}</span>
      <div class="file-column-head-meta">
        <strong class="file-column-title">{{ columnTitle }}</strong>
        <span class="file-column-count" v-if="selectionCount">{{ selectionCount }} 项已选</span>
        <span class="file-column-count" v-else>{{ effectiveEntries().length }} 项</span>
      </div>
      <div class="file-column-path">
        <!-- 面包屑态：每段可点跳转，末端 ✎ 切编辑 -->
        <nav v-if="!pathEditing" class="breadcrumb file-column-breadcrumb" @dblclick="enterPathEditing">
          <template v-if="crumbs.length">
            <button
              v-for="(seg, idx) in crumbs"
              :key="seg.path + idx"
              type="button"
              class="crumb"
              :class="{ active: idx === crumbs.length - 1 }"
              :title="seg.path"
              @click="crumbClick(seg)"
            >
              <span class="crumb-label">{{ seg.label }}</span>
              <ChevronRight v-if="idx < crumbs.length - 1" :size="11" class="crumb-sep" />
            </button>
            <button
              type="button"
              class="crumb-edit"
              title="编辑路径"
              @click.stop="enterPathEditing"
            >
              <Pencil :size="11" />
            </button>
          </template>
          <span v-else class="crumb-empty" @click="enterPathEditing">点此输入路径…</span>
        </nav>
        <!-- 编辑态：原始路径 input，回车跳转、Esc 退出 -->
        <input
          v-else
          class="file-column-manual-path"
          :value="isLocal ? manualLocalPathInput : manualRemotePathInput"
          :placeholder="isLocal ? '路径（回车跳转，Esc 取消）' : '路径（回车跳转，Esc 取消）'"
          spellcheck="false"
          autofocus
          @input="onManualPathInput"
          @keydown="onPathInputKeydown"
        />
      </div>
      <div class="pane-tools file-column-head-actions">
        <!-- 可选前置按钮插槽（如「本地」列切换），插在过滤按钮之前，避免与悬浮按钮冲突。 -->
        <slot name="actions-leading" />
        <!-- 过滤浮动窗触发按钮（filterValue 非空时高亮） -->
        <div class="filter-host">
          <button
            ref="filterBtnRef"
            class="icon-btn"
            :class="{ active: filterValue !== '' || filterOpen }"
            type="button"
            :title="'过滤当前目录'"
            @click="toggleFilter"
          >
            <FilterIcon :size="14" />
          </button>
          <!-- 浮动过滤面板：绝对定位挂在右上角 -->
          <div v-if="filterOpen" ref="filterPopoverRef" class="filter-popover" @click.stop>
            <span class="filter-popover-icon"><FilterIcon :size="12" /></span>
            <input
              ref="filterInputRef"
              class="filter-popover-input"
              :value="filterValue"
              placeholder="过滤当前目录..."
              spellcheck="false"
              @input="onFilterInput"
            />
            <button
              v-if="showFilterClear"
              class="filter-popover-clear"
              type="button"
              title="清空"
              @click="clearFilter"
            >
              <X :size="12" />
            </button>
          </div>
        </div>
        <button
          class="icon-btn"
          type="button"
          title="上级目录"
          :disabled="isBusy || (isLocal && !localPath)"
          @click="goUp"
        >
          <ArrowUp :size="14" />
        </button>
        <button
          class="icon-btn"
          type="button"
          title="刷新"
          :disabled="isBusy || (isLocal && !!disabledHint)"
          @click="refresh"
        >
          <Loader2 v-if="isBusy" class="spin" :size="14" />
          <RefreshCw v-else :size="14" />
        </button>
      </div>
    </header>

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

    <!-- File list -->
    <div
      class="file-list file-column-list"
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
        <span class="col-name file-row-name" :title="entry.name">{{ entry.name }}</span>
        <template v-if="remoteListMode === 'detailed'">
          <span class="col-size file-row-size">{{ formatBytes(entry.size) }}</span>
          <span class="col-type file-row-type" :title="inferType(entry)">{{ inferType(entry) }}</span>
          <span class="col-mtime file-row-time">{{ formatEntryTime(entry) }}</span>
          <span class="col-perm file-row-perm">{{ entry.permissions || '—' }}</span>
          <span class="col-owner file-row-owner" :title="formatOwner(entry)">{{ formatOwner(entry) }}</span>
        </template>
      </div>

      <div v-if="!effectiveEntries().length" class="col-empty file-column-empty">
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
            {{ remoteEntries.length ? '无匹配「' + remoteFilter + '」的条目' : '尚未加载远程目录' }}
          </template>
        </p>
      </div>

      <div v-if="isBusy" class="file-loading-overlay" role="status" aria-live="polite">
        <Loader2 class="spin" :size="16" />
        <span>{{ busyMessage }}</span>
      </div>
    </div>
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

// Header row: 单行承载 title+count / 路径框+过滤框 / 上级目录/刷新。
// 原独立 pathrow / filterrow 已合并，省两行高度（每列净省 ~46px 让给终端区）。
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
  display: none;
  align-items: baseline;
  gap: var(--space-2);
  flex: 0 0 auto;
  min-width: 0;
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
  white-space: nowrap;
}
// 路径区：吃剩余宽度，承载面包屑或编辑态 input。
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

// 面包屑：每段可点跳转，分隔符 ChevronRight。
.file-column-breadcrumb {
  display: flex;
  align-items: center;
  gap: 1px;
  min-width: 0;
  flex: 1;
  overflow-x: auto;
  scrollbar-width: none;
  &::-webkit-scrollbar { display: none; }
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
  transition: background var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard);
}
.crumb:hover {
  background: var(--app-hover);
  color: var(--app-strong);
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
  transition: background var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard);
}
.crumb-edit:hover {
  background: var(--app-hover);
  color: var(--app-strong);
}
.crumb-empty {
  color: var(--app-subtle);
  font-size: var(--text-xs);
  cursor: text;
  padding: 2px 4px;
}

.icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
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
// 过滤按钮激活态（有过滤值或面板打开时）。
.icon-btn.active {
  color: var(--accent);
  background: color-mix(in oklab, var(--accent), transparent 88%);
}

// 路径编辑态 input（吃满路径区宽度）。
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
  transition: border-color var(--motion-fast) var(--ease-standard),
    box-shadow var(--motion-fast) var(--ease-standard);
}
.file-column-manual-path:focus {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

// 过滤浮动窗宿主：相对定位，popover 绝对定位挂其下。
.filter-host {
  position: relative;
  display: inline-flex;
}
// 浮动过滤面板：绝对定位贴在 header 下方右侧。
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
  transition: border-color var(--motion-fast) var(--ease-standard),
    box-shadow var(--motion-fast) var(--ease-standard);
}
.filter-popover-input::placeholder { color: var(--app-subtle); }
.filter-popover-input:focus {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}
.filter-popover-clear {
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
.filter-popover-clear:hover {
  background: var(--app-hover);
  color: var(--app-strong);
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

// File list — fills remaining column height, scrolls.
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
  to { transform: rotate(360deg); }
}
.file-column-list.compact .file-row {
  grid-template-columns: 16px minmax(0, 1fr);
  padding-block: 2px;
}

// Single file row. Hairline separators via border-block-end on rows.
// 列对齐 .file-column-cols：图标/名(flex) / 大小 / 类型 / 修改时间 / 权限 / 用户组。
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
.file-row.selected { background: var(--app-selected); }
.file-row.selected:hover { background: var(--accent-soft-strong); }

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
// 类型列：扩展名大写，mono 等宽，弱色。
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
// 权限列：八进制串，mono 等宽，右对齐。
.file-row-perm {
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
  text-align: end;
  white-space: nowrap;
}
// 用户:组列：弱色，省略号兜底。
.file-row-owner {
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

// Empty state.
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
