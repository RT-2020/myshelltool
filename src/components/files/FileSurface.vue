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
const { remoteListMode, contextMenu, selectedRemotePaths, localPaneVisible } = storeToRefs(filesStore);

const isTauriCore = computed(() => isTauriRuntime());

// Hidden file input for upload (triggered via context-menu or drag-drop).
const fileInput = ref(null);

function triggerFileUpload() {
  fileInput.value?.click();
}
function onFilePick(event) {
  const files = event.target.files;
  if (files?.length) filesStore.uploadFiles(files);
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
  if (files?.length) filesStore.uploadFiles(files);
}
function onDragOver(event) {
  // 必须 preventDefault 才能触发 drop；仅含文件时才显提示。
  event.preventDefault();
  if (event.dataTransfer?.types?.includes('Files')) {
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
      else filesStore.announce('单文件上传待接线：' + entry.name);
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
    class="file-surface"
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

    <!-- Dual-pane：默认折叠本地列（grid 1fr），展开时 1fr 1fr。
         远程列右上角浮一个「本地」切换按钮（折叠态常显，展开态可收起）。 -->
    <div class="file-surface-dual">
      <FileColumn
        v-if="localPaneVisible"
        kind="local"
        :disabled-hint="isTauriCore ? '' : '桌面客户端运行时才支持本地浏览（npm run tauri:dev）'"
        class="file-surface-col file-surface-col--local"
      />
      <FileColumn
        kind="remote"
        class="file-surface-col file-surface-col--remote"
      >
        <!-- 本地列切换按钮：塞进远程 header actions 最前（非悬浮，不遮挡过滤图标）。 -->
        <template #actions-leading>
          <button
            type="button"
            class="local-toggle"
            :class="{ active: localPaneVisible }"
            :title="localPaneVisible ? '隐藏本地面板' : '显示本地面板（双栏）'"
            @click="toggleLocalPane"
          >
            <component :is="localPaneVisible ? FolderOpen : PanelLeft" :size="14" />
            <span>本地</span>
          </button>
        </template>
      </FileColumn>
    </div>

    <!-- 拖拽上传视觉提示浮层：松开上传到当前远程目录。pointer-events:none 不拦截 drop。 -->
    <div v-if="dragging" class="drag-overlay">
      <div class="drag-overlay-inner">
        <FolderOpen :size="28" />
        <strong>松开以上传到当前远程目录</strong>
        <span class="drag-overlay-sub">{{ filesStore.remotePath || '/' }}</span>
      </div>
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

// Surface container — 无圆角无框：与 grid region 贴合，靠 region 间 1px 分隔。
.file-surface {
  position: relative; // 浮动按钮 / 拖拽浮层的定位参照系
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

// Dual-pane：默认单栏（1fr），localPaneVisible 时 1fr 1fr。
.file-surface-dual {
  display: grid;
  grid-template-columns: 1fr;
  flex: 1 1 auto;
  min-height: 0;
  overflow: hidden;
  transition: grid-template-columns var(--motion-base) var(--ease-standard);
}
.file-surface.local-open .file-surface-dual {
  grid-template-columns: 1fr 1fr;
}
.file-surface-col {
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}
// 双栏时本地列右侧 1px 分隔。
.file-surface.local-open .file-surface-col--local {
  border-inline-end: 1px solid var(--app-border);
}

// 本地切换按钮：内联在远程 header actions 里（非悬浮，不遮挡过滤图标）。
// 折叠态低调灰边，展开态 accent 高亮表示当前双栏。
.local-toggle {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  padding-inline: 7px;
  background: transparent;
  color: var(--app-muted);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-pill);
  font-size: var(--text-xs);
  font-family: var(--font-body);
  cursor: pointer;
  user-select: none;
  flex: 0 0 auto;
  transition: background var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard),
    border-color var(--motion-fast) var(--ease-standard);
}
.local-toggle:hover {
  background: var(--app-hover);
  color: var(--app-strong);
  border-color: var(--app-border-strong);
}
.local-toggle.active {
  color: var(--accent);
  border-color: color-mix(in oklab, var(--accent), transparent 30%);
  background: color-mix(in oklab, var(--accent), transparent 90%);
}

// 拖拽视觉提示：整面半透明 accent 边框 + 居中浮层。pointer-events:none 不拦截 drop。
.drag-overlay {
  position: absolute;
  inset: 0;
  z-index: var(--z-drawer);
  display: flex;
  align-items: center;
  justify-content: center;
  background: color-mix(in oklab, var(--accent), transparent 90%);
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
  box-shadow: var(--app-shadow);
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
