<script setup>
/**
 * FileSurface — Wave 3 Step 3.4
 * Center-bottom container: dual-pane file manager (local + remote) with a
 * bottom-anchored transfer drawer. Tabby/Termius styling — low visual weight,
 * hairline chrome edges, no card stacking.
 *
 * Store-bound: directly reads useFilesStore / useUiStore. No prop drilling.
 * The two FileColumn children are driven by their `kind` prop; selection /
 * sort / filter / context-menu / breadcrumb / upload all live in the store.
 *
 * Right-click on either column writes filesStore.contextMenu (same shape
 * App.vue reads), so batch operations and per-entry actions still work when
 * Wave 3.5 wires FileSurface into App.vue.
 *
 * Not wired into App.vue yet — Step 3.4 ships the components only.
 */
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import {
  Upload,
  FolderPlus,
  RefreshCw,
  HardDrive,
  ChevronsUpDown
} from 'lucide-vue-next';
import { useFilesStore } from '@/stores/files.js';
import { useUiStore } from '@/stores/ui.js';
import { useAssetsStore } from '@/stores/assets.js';
import { isTauriRuntime } from '@/services/backend.js';
import FileColumn from './FileColumn.vue';
import TransferDrawer from './TransferDrawer.vue';
import AppContextMenu from '@/components/ui/AppContextMenu.vue';

const filesStore = useFilesStore();
const uiStore = useUiStore();
const assetsStore = useAssetsStore();
const { selectedAsset } = storeToRefs(assetsStore);
const { remoteListMode, contextMenu, transferDrawerOpen } = storeToRefs(filesStore);

const isTauriCore = computed(() => isTauriRuntime());
const localDisabledHint = computed(() =>
  isTauriCore.value ? '' : '桌面客户端运行时才支持本地浏览（npm run tauri:dev）'
);

// Hidden file input for upload (selected via toolbar button or drag-drop).
const fileInput = ref(null);

function triggerFileUpload() {
  fileInput.value?.click();
}
function onFilePick(event) {
  const files = event.target.files;
  if (files?.length) filesStore.uploadFiles(files);
  if (event.target) event.target.value = '';
}
function onDrop(event) {
  event.preventDefault();
  const files = event.dataTransfer?.files;
  if (files?.length) filesStore.uploadFiles(files);
}
function onDragOver(event) {
  event.preventDefault();
}

// Toolbar actions (delegated to store or central modal).
function openMkdirRemote() {
  uiStore.modal = { type: 'mkdir', entry: null };
}
function openMkdirLocal() {
  uiStore.modal = { type: 'localMkdir', entry: null };
}
function refreshRemote() {
  filesStore.refreshRemoteFiles();
}
function toggleListMode() {
  filesStore.setRemoteListMode(remoteListMode.value === 'detailed' ? 'compact' : 'detailed');
}
function toggleDrawer() {
  filesStore.toggleTransferDrawer();
}

// ============================================================
// Context menu items — same content shape as App.vue contextMenuItems,
// but built here from files store state. items: [{ label, action, danger, separator, disabled }]
// ============================================================
const contextMenuItems = computed(() => {
  if (!contextMenu.value.visible) return [];
  const side = contextMenu.value.side;
  const entry = contextMenu.value.entry;
  const isDir = entry?.kind === 'directory' || entry?.kind === 'symlink';
  const make = (label, fn, opts = {}) => ({ label, action: fn, ...opts });

  if (side === 'remote') {
    const items = [];
    if (selectedRemotePaths.value.size > 1) {
      items.push(make(`批量下载 (${selectedRemotePaths.value.size})`, () => filesStore.batchRemoteDownload()));
      items.push(make(`批量删除 (${selectedRemotePaths.value.size})`, () => filesStore.batchRemoteDelete(), { danger: true }));
      items.push({ separator: true });
    }
    items.push(make(isDir ? '进入目录' : '下载', () => {
      if (isDir) filesStore.navigateRemotePath(entry.path);
      else filesStore.downloadEntry(entry);
    }));
    items.push(make('上传文件到当前目录', () => triggerFileUpload()));
    items.push({ separator: true });
    items.push(make('重命名', () => { uiStore.modal = { type: 'rename', entry }; }));
    items.push(make('删除', () => filesStore.removeRemote(entry), { danger: true }));
    items.push({ separator: true });
    items.push(make('复制路径', () => filesStore.copyRemotePath(entry)));
    return items;
  }
  // Local menu
  return [
    make(isDir ? '进入目录' : '上传到远程', () => {
      if (isDir) filesStore.navigateLocalPath(entry.path);
      else filesStore.announce('单文件上传待接线：' + entry.name);
    }),
    { separator: true },
    make('重命名', () => { uiStore.modal = { type: 'localRename', entry }; }),
    make('删除', () => filesStore.localDelete([entry.path]), { danger: true }),
    { separator: true },
    make('复制路径', () => {
      navigator.clipboard?.writeText(entry.path).catch(() => null);
    })
  ];
});

// Re-expose selectedRemotePaths for template parity.
const { selectedRemotePaths } = storeToRefs(filesStore);
</script>

<template>
  <div class="file-surface" @drop="onDrop" @dragover="onDragOver">
    <!-- Row 1: surface toolbar — upload / mkdir / refresh / list-mode -->
    <header class="file-surface-toolbar">
      <div class="file-surface-toolbar-left">
        <strong class="file-surface-title">
          {{ selectedAsset?.name || '未选择连接' }} · 文件管理
        </strong>
      </div>
      <div class="file-surface-toolbar-right">
        <button class="btn primary" type="button" data-upload-trigger @click="triggerFileUpload">
          <Upload :size="14" />
          <span>上传</span>
        </button>
        <button class="btn" type="button" data-mkdir @click="openMkdirRemote">
          <FolderPlus :size="14" />
          <span>新建远程目录</span>
        </button>
        <button class="btn" type="button" :disabled="!isTauriCore" @click="openMkdirLocal">
          <FolderPlus :size="14" />
          <span>新建本地目录</span>
        </button>
        <button class="btn" type="button" data-refresh-remote-files @click="refreshRemote">
          <RefreshCw :size="14" />
          <span>刷新远程</span>
        </button>
        <button
          class="btn icon-only"
          type="button"
          :title="remoteListMode === 'detailed' ? '切换紧凑列表' : '切换详细列表'"
          @click="toggleListMode"
        >
          <ChevronsUpDown :size="14" />
        </button>
      </div>
    </header>
    <input
      ref="fileInput"
      type="file"
      multiple
      style="display:none"
      @change="onFilePick"
    />

    <!-- Row 2: dual-pane (local | remote). 1px border-inline-end separates them. -->
    <div class="file-surface-dual">
      <FileColumn
        kind="local"
        :disabled-hint="localDisabledHint"
        class="file-surface-col file-surface-col--local"
      />
      <FileColumn
        kind="remote"
        class="file-surface-col file-surface-col--remote"
      />
    </div>

    <!-- Row 3: transfer drawer (always-visible trigger + slide-up sheet). -->
    <footer class="file-surface-drawer">
      <TransferDrawer :open="transferDrawerOpen" @toggle="toggleDrawer" />
    </footer>

    <!-- Right-click context menu (teleported by AppContextMenu). -->
    <AppContextMenu
      :open="contextMenu.visible"
      :items="contextMenuItems"
      :x="contextMenu.x"
      :y="contextMenu.y"
      @close="filesStore.closeContextMenu()"
    />
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// Surface container — 无圆角无外框：与 grid region 贴合平齐，靠 region 间 1px
// 分隔线划分边界，保持整体性。Local/remote 列由 .col--local 的 border-inline-end 分隔。
.file-surface {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-height: 0;
  background: var(--app-panel);
  color: var(--app-text);
  font-family: var(--font-body);
  overflow: hidden;
}

// Row 1: surface toolbar.
.file-surface-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border-block-end: 1px solid var(--app-border);
  background: var(--app-panel-2);
  flex: 0 0 auto;
}
.file-surface-toolbar-left {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}
.file-surface-title {
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--app-strong); // 统一 center 区标题色：与 .terminal-host 一致
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.file-surface-toolbar-right {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  flex-wrap: wrap;
}

// Buttons match App.vue .btn conventions.
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px 10px;
  background: var(--app-control);
  color: var(--app-text);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  font-family: var(--font-body);
  cursor: pointer;
  user-select: none;
  transition: background var(--motion-fast) var(--ease-standard),
    border-color var(--motion-fast) var(--ease-standard);
}
.btn:hover:not(:disabled) {
  background: var(--app-hover);
  border-color: var(--app-border-strong);
}
.btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
.btn.primary {
  background: var(--accent);
  color: var(--accent-on);
  border-color: var(--accent);
}
.btn.primary:hover:not(:disabled) {
  background: var(--accent-hover);
  border-color: var(--accent-hover);
}
.btn.icon-only {
  padding: 5px;
}

// Row 2: dual-pane. Single-pixel separator between columns.
.file-surface-dual {
  display: grid;
  grid-template-columns: 1fr 1fr;
  flex: 1 1 auto;
  min-height: 0;
  overflow: hidden;
}
.file-surface-col {
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}
.file-surface-col--local {
  border-inline-end: 1px solid var(--app-border);
}

// Row 3: bottom drawer trigger + sheet.
.file-surface-drawer {
  flex: 0 0 auto;
}
</style>
