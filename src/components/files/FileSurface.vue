<script setup>
/**
 * FileSurface — Wave 3 Step 3.4（v3 精简版）
 * Center-bottom container: 默认只渲染远程文件栏占满全宽，点「本地」按钮展开双栏。
 * 极简 chrome：无标题 toolbar，功能全在右键菜单；传输触发在状态栏「传输」胶囊（全局）。
 *
 * v3 变化：
 *  - 默认折叠本地列（localPaneVisible=false），远程栏独享全宽。
 *  - 支持从 Windows 资源管理器拖拽文件到文件区上传（drop/dragover + dragover 视觉提示）。
 *
 * Store-bound: directly reads useFilesStore / useUiStore. No prop drilling.
 */
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { PanelLeft, FolderOpen } from 'lucide-vue-next';
import { useFilesStore } from '@/stores/files.js';
import { useUiStore } from '@/stores/ui.js';
import { isTauriRuntime } from '@/services/backend.js';
import FileColumn from './FileColumn.vue';
import AppContextMenu from '@/components/ui/AppContextMenu.vue';

const filesStore = useFilesStore();
const uiStore = useUiStore();
const { remoteListMode, contextMenu, selectedRemotePaths, localPaneVisible, remoteBusy } = storeToRefs(filesStore);

const isTauriCore = computed(() => isTauriRuntime());

// Hidden file input for upload (triggered via context-menu or drag-drop).
const fileInput = ref(null);

function triggerFileUpload() {
  if (remoteBusy.value) return;
  fileInput.value?.click();
}
function onFilePick(event) {
  const files = event.target.files;
  if (files?.length && !remoteBusy.value) filesStore.uploadFiles(files);
  if (event.target) event.target.value = '';
}

// 拖拽上传：Windows 资源管理器拖文件进来即触发 uploadFiles。
// dragover 时显示半透明 accent 边框 + 「松开上传」浮层（dragging=true）。
const dragging = ref(false);
let dragLeaveTimer = null;

function onDrop(event) {
  event.preventDefault();
  dragging.value = false;
  if (dragLeaveTimer) { clearTimeout(dragLeaveTimer); dragLeaveTimer = null; }
  const files = event.dataTransfer?.files;
  if (files?.length && !remoteBusy.value) filesStore.uploadFiles(files);
}
function onDragOver(event) {
  // 必须 preventDefault 才能触发 drop；仅含文件时才显提示。
  event.preventDefault();
  if (!remoteBusy.value && event.dataTransfer?.types?.includes('Files')) {
    dragging.value = true;
    if (dragLeaveTimer) { clearTimeout(dragLeaveTimer); dragLeaveTimer = null; }
  }
}
function onDragLeave() {
  // 用 timer 延迟隐藏，避免子元素切换触发的误判 dragleave。
  if (dragLeaveTimer) clearTimeout(dragLeaveTimer);
  dragLeaveTimer = setTimeout(() => { dragging.value = false; }, 80);
}

// 本地列折叠切换：首次展开时若本地未加载则触发加载。
function toggleLocalPane() {
  filesStore.toggleLocalPane();
  if (localPaneVisible.value && isTauriCore.value && !filesStore.localPath) {
    filesStore.refreshLocalFiles().catch(() => null);
  }
}

// ============================================================
// Context menu items — 吸收原 toolbar 下沉功能（刷新 / 新建目录 / 上传 /
// 列表模式切换 / 显示本地列）。items: [{ label, action, danger, separator, disabled }]
// ============================================================
const contextMenuItems = computed(() => {
  if (!contextMenu.value.visible) return [];
  const side = contextMenu.value.side;
  const entry = contextMenu.value.entry;
  const isDir = entry?.kind === 'directory' || entry?.kind === 'symlink';
  const make = (label, fn, opts = {}) => ({ label, action: fn, ...opts });

  if (side === 'remote') {
    const items = [];
    // 多选批量操作优先。
    if (selectedRemotePaths.value.size > 1) {
      items.push(make(`批量下载 (${selectedRemotePaths.value.size})`, () => filesStore.batchRemoteDownload()));
      items.push(make(`批量删除 (${selectedRemotePaths.value.size})`, () => filesStore.batchRemoteDelete(), { danger: true }));
      items.push({ separator: true });
    }
    // 单项操作。
    if (entry) {
      items.push(make(isDir ? '进入目录' : '下载', () => {
        if (isDir) filesStore.navigateRemotePath(entry.path);
        else filesStore.downloadEntry(entry);
      }));
      items.push({ separator: true });
      items.push(make('重命名', () => { uiStore.modal = { type: 'rename', entry }; }));
      items.push(make('删除', () => filesStore.removeRemote(entry), { danger: true }));
      items.push({ separator: true });
      items.push(make('复制路径', () => filesStore.copyRemotePath(entry)));
      items.push({ separator: true });
    }
    // 目录级操作（下沉自原 toolbar）。
    items.push(make('上传文件到当前目录', () => triggerFileUpload()));
    items.push(make('新建远程目录', () => { uiStore.modal = { type: 'mkdir', entry: null }; }));
    items.push(make('刷新当前目录', () => filesStore.refreshRemoteFiles()));
    items.push(make(remoteListMode.value === 'detailed' ? '切换为紧凑列表' : '切换为详细列表', () =>
      filesStore.setRemoteListMode(remoteListMode.value === 'detailed' ? 'compact' : 'detailed')
    ));
    items.push({ separator: true });
    items.push(make(localPaneVisible.value ? '隐藏本地面板' : '显示本地面板', () => toggleLocalPane()));
    return items;
  }
  // Local menu
  const items = [];
  if (entry) {
    items.push(make(isDir ? '进入目录' : '上传到远程', () => {
      if (isDir) filesStore.navigateLocalPath(entry.path);
      else filesStore.uploadLocalEntry(entry);
    }));
    items.push({ separator: true });
    items.push(make('重命名', () => { uiStore.modal = { type: 'localRename', entry }; }));
    items.push(make('删除', () => filesStore.localDelete([entry.path]), { danger: true }));
    items.push({ separator: true });
    items.push(make('复制路径', () => {
      navigator.clipboard?.writeText(entry.path).catch(() => null);
    }));
    items.push({ separator: true });
  }
  // 目录级操作（下沉自原 toolbar）。本地浏览需桌面运行时。
  items.push(make('新建本地目录', () => { uiStore.modal = { type: 'localMkdir', entry: null }; }, { disabled: !isTauriCore.value }));
  items.push(make('刷新当前目录', () => filesStore.refreshLocalFiles(), { disabled: !isTauriCore.value }));
  items.push({ separator: true });
  items.push(make('隐藏本地面板', () => toggleLocalPane()));
  return items;
});
</script>

<template>
  <div
    class="region-files"
    :class="{ 'is-dragging': dragging, 'local-open': localPaneVisible }"
    @drop="onDrop"
    @dragover="onDragOver"
    @dragleave="onDragLeave"
  >
    <input
      ref="fileInput"
      type="file"
      multiple
      style="display:none"
      @change="onFilePick"
    />

    <!-- ============ file-header（app.css L694-701：chrome-label + view-pills + file-actions）============ -->
    <header class="file-header">
      <div class="file-title">
        <span class="chrome-label">文件传输</span>
      </div>
      <!-- 视图胶囊：双栏 / 仅远程（app.css view-pills L703-727）-->
      <div class="view-pills" role="tablist" aria-label="文件视图模式">
        <button
          type="button"
          class="view-pill"
          :class="{ active: localPaneVisible }"
          role="tab"
          :aria-selected="String(localPaneVisible)"
          title="本地 / 远程 双栏"
          @click="!localPaneVisible && toggleLocalPane()"
        >本地 / 远程</button>
        <button
          type="button"
          class="view-pill"
          :class="{ active: !localPaneVisible }"
          role="tab"
          :aria-selected="String(!localPaneVisible)"
          title="仅远程"
          @click="localPaneVisible && toggleLocalPane()"
        >仅远程</button>
      </div>
      <div class="file-actions">
        <button class="icon-btn" type="button" title="新建目录" aria-label="新建目录" :disabled="remoteBusy" @click="uiStore.modal = { type: 'mkdir', entry: null }">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true"><path d="M3 7a2 2 0 012-2h3l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z"/><path d="M12 11v5M9.5 13.5h5" stroke-linecap="round" stroke-linejoin="round"/></svg>
        </button>
        <button class="icon-btn" type="button" title="上传文件" aria-label="上传文件到当前远程目录" :disabled="remoteBusy" @click="triggerFileUpload">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 9l5-5 5 5M12 4v12" stroke-linecap="round" stroke-linejoin="round"/></svg>
        </button>
        <button class="icon-btn" type="button" title="刷新" aria-label="刷新远程目录" :disabled="remoteBusy" @click="filesStore.refreshRemoteFiles()">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true"><path d="M21 12a9 9 0 11-3-6.7M21 4v5h-5" stroke-linecap="round" stroke-linejoin="round"/></svg>
        </button>
      </div>
    </header>

    <!-- ============ file-dual（app.css L729-733：grid 1fr 1px 1fr）============ -->
    <div class="file-dual">
      <FileColumn
        v-if="localPaneVisible"
        kind="local"
        :disabled-hint="isTauriCore ? '' : '桌面客户端运行时才支持本地浏览（npm run tauri:dev）'"
        class="file-pane file-pane-local"
      />
      <div v-if="localPaneVisible" class="file-divider" aria-hidden="true"></div>
      <FileColumn
        kind="remote"
        class="file-pane file-pane-remote"
      >
        <!-- 远程列表头弱提示（S2）：常显，最少打扰 -->
        <template #actions-leading>
          <span class="file-column-hint" title="右键文件或空白处查看更多操作">右键查看更多操作</span>
        </template>
      </FileColumn>
    </div>

    <!-- ============ drop-hint（app.css L770-785：底部胶囊提示）============ -->
    <!-- 拖拽上传视觉提示：dragging 时整面 accent 虚线 + 底部胶囊提示目标路径 -->
    <div v-if="dragging" class="drag-overlay">
      <div class="drag-overlay-inner">
        <FolderOpen :size="28" />
        <strong>松开以上传到当前远程目录</strong>
        <span class="drag-overlay-sub">{{ filesStore.remotePath || '/' }}</span>
      </div>
    </div>
    <!-- 常显底部胶囊（无拖拽时也提示，app.html drop-hint）-->
    <div v-else class="drop-hint" aria-hidden="true">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 9l5-5 5 5M12 4v12" stroke-linecap="round" stroke-linejoin="round"/></svg>
      <span>拖拽文件到任意一栏上传</span>
    </div>

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

// ============================================================
// region-files（app.css L688-693：grid 38px 1fr）
// ============================================================
.region-files {
  position: relative;
  display: grid;
  grid-template-rows: 38px 1fr;
  width: 100%;
  height: 100%;
  min-height: 0;
  background: var(--app-window);
  color: var(--app-text);
  font-family: var(--font-body);
  overflow: hidden;
}

// ============================================================
// file-header（app.css L694-701：chrome-label + view-pills + file-actions）
// ============================================================
.file-header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 0 12px;
  background: var(--app-chrome);
  border-bottom: 1px solid var(--app-border);
}
.file-title {
  display: flex;
  align-items: center;
}
.chrome-label {
  font-family: var(--font-display);
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.04em;
  color: var(--app-muted);
}
.file-actions {
  display: flex;
  gap: 2px;
  margin-left: auto;
}

// 通用 icon-btn 已收敛为全局类（_utilities.scss 单一权威实现），模板直接使用

// ============================================================
// view-pills（app.css L703-727：视图胶囊 双栏/仅远程）
// ============================================================
.view-pills {
  display: inline-flex;
  background: var(--app-panel-2);
  border: 1px solid var(--app-border);
  border-radius: 7px;
  padding: 2px;
  gap: 0;
}
.view-pill {
  padding: 4px 12px;
  font-size: 11.5px;
  color: var(--app-muted);
  background: transparent;
  border: 0;
  border-radius: 5px;
  cursor: pointer;
  font-family: var(--font-body);
  transition: background var(--motion-fast), color var(--motion-fast);
}
.view-pill:hover { color: var(--app-text); }
.view-pill.active {
  background: var(--app-panel);
  color: var(--app-text);
  font-weight: 500;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
}

// ============================================================
// file-dual（app.css L729-734：grid 1fr 1px 1fr）
// 仅远程模式时隐藏 local 列与 divider，grid 自动塌缩为 1fr
// ============================================================
.file-dual {
  display: grid;
  grid-template-columns: 1fr;
  min-height: 0;
  overflow: hidden;
}
.region-files.local-open .file-dual {
  grid-template-columns: 1fr 1px 1fr;
}
.file-pane {
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.file-divider {
  background: var(--app-border);
}

// S2：远程列表头弱提示（右键查看更多操作）
.file-column-hint {
  font-size: var(--text-xs);
  color: var(--app-subtle);
  white-space: nowrap;
  padding-inline: 4px;
  user-select: none;
  max-width: 120px;
  overflow: hidden;
  text-overflow: ellipsis;
}

// ============================================================
// drop-hint（app.css L770-785：底部胶囊提示，常显）
// ============================================================
.drop-hint {
  position: absolute;
  bottom: 12px;
  left: 50%;
  transform: translateX(-50%);
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px 12px;
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-pill);
  box-shadow: var(--shadow-pop);
  font-size: 11px;
  color: var(--app-muted);
  pointer-events: none;
  z-index: var(--z-base);
}
.drop-hint svg {
  width: 12px;
  height: 12px;
  stroke-width: 1.7;
  color: var(--app-subtle);
}

// ============================================================
// drag-overlay（拖拽时整面 accent 虚线 + 居中浮层）
// ============================================================
.drag-overlay {
  position: absolute;
  inset: 0;
  z-index: var(--z-drawer);
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--accent-soft);
  border: 2px dashed var(--accent);
  pointer-events: none;
}
.drag-overlay-inner {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: var(--space-4) var(--space-6);
  color: var(--accent);
  background: var(--app-panel);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-pop);
}
.drag-overlay-inner strong {
  font-size: var(--text-sm);
}
.drag-overlay-sub {
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
}
</style>
