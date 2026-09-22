<script setup lang="ts">
/**
 * FileSurface — Wave 3 Step 3.4（v3 精简版）
 * Center-bottom container: 默认只渲染远程文件栏占满全宽，点「本地」按钮展开双栏。
 * 极简 chrome：无标题 toolbar，功能全在右键菜单；传输触发在状态栏「传输」胶囊（全局）。
 *
 * v3 变化：
 *  - 默认折叠本地列（localPaneVisible=false），远程栏独享全宽。
 *  - 支持从 Windows 资源管理器拖拽文件到文件区上传（Tauri 窗口级 onDragDropEvent + 浮层提示）。
 *
 * Store-bound: directly reads useFilesStore / useUiStore. No prop drilling.
 */
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { PanelLeft, FolderOpen } from 'lucide-vue-next';
import { useFilesStore } from '@/stores/files';
import { useUiStore } from '@/stores/ui';
import { useEditorStore } from '@/stores/editor';
import { useWorkbenchStore } from '@/stores/workbench';
import { getTauriWindow, invokeBackend, isTauriRuntime } from '@/services/backend';
import { isKnownBinaryExtension } from '@/lib/editor/editorLanguages';
import { FILE_DRAG_MIME } from '@/lib/fileTypes';
import { errorMessage } from '@/lib/errorMessage';
import { resolveSessionForAsset } from '@/lib/filePanel';
import FileColumn from './FileColumn.vue';
import UploadProgressStrip from './UploadProgressStrip.vue';
import AppContextMenu from '@/components/ui/AppContextMenu.vue';
import type { ModalState, RemoteFileEntry } from '@/types/domain';

/** 右键菜单条目（与 AppContextMenu 的 ContextMenuItem 同形状）。 */
interface FileMenuItem {
  label?: string;
  action?: () => void;
  danger?: boolean;
  separator?: boolean;
  disabled?: boolean;
}

const filesStore = useFilesStore();
const uiStore = useUiStore();
const workbench = useWorkbenchStore();

// v0.18：双击/右键「编辑」→ 打开内置编辑器（远程挂当前选中资产的会话）
function openInEditor(entry: RemoteFileEntry, side: 'local' | 'remote') {
  void workbench.editorOpenTarget(
    side === 'remote'
      ? { kind: 'remote', assetId: workbench.selectedAsset?.id ?? null, path: entry.path }
      : { kind: 'local', assetId: null, path: entry.path }
  );
}

/** 「编辑」菜单项：已知二进制禁用并说明；未知扩展允许按纯文本尝试（后端嗅探兜底）。 */
function editMenuItem(entry: RemoteFileEntry, side: 'local' | 'remote'): FileMenuItem {
  const binary = isKnownBinaryExtension(entry.name);
  return {
    label: binary ? '编辑（二进制文件）' : '编辑',
    action: () => openInEditor(entry, side),
    disabled: binary
  };
}
const { remoteListMode, contextMenu, selectedRemotePaths, localPaneVisible, remoteBusy } = storeToRefs(filesStore);

const isTauriCore = computed(() => isTauriRuntime());

// 上传走原生文件对话框：webview 的 <input type="file"> 拿不到完整本地路径，
// 而流式上传（sftp_upload_from_file）以后端读盘为准——路径是必需输入。
async function triggerFileUpload() {
  if (remoteBusy.value) return;
  if (!isTauriCore.value) {
    uiStore.notify('浏览器预览不支持上传（需桌面客户端 npm run tauri:dev）', { level: 'warn' });
    return;
  }
  const selected = await invokeBackend<string | string[] | null>('plugin:dialog|open', {
    options: {
      title: '选择要上传的文件',
      multiple: true,
      directory: false,
      // 默认定位到面板当前的本地目录（与下载目录选择框同一约定）
      defaultPath: filesStore.localPath || undefined
    }
  }).catch(err => {
    uiStore.notify('打开文件对话框失败：' + (err?.message || err), { level: 'error' });
    return null;
  });
  const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
  if (paths.length) filesStore.uploadLocalPaths(paths);
}

// OS 拖拽上传：tauri.conf 未设 dragDropEnabled=false，OS 文件拖放由 Tauri 拦截
// 并转发为窗口级事件（HTML5 drop 在 Windows 上不会触发——因此这里必须是
// onDragDropEvent，而不是元素上的 @drop）。事件是窗口级的：拖入任意区域
// （含终端）都会走上传，与旧「整面 dropzone」语义略有扩大但符合拖入意图。
const dragging = ref(false);
let unlistenOsDrag: (() => void) | null = null;

onMounted(async () => {
  if (!isTauriCore.value) return;
  const win = getTauriWindow();
  // 测试 mock（ui-ipc-flows）的窗口句柄没有 onDragDropEvent，直接跳过
  if (typeof win?.onDragDropEvent !== 'function') return;
  unlistenOsDrag = await win.onDragDropEvent(event => {
    const payload = event.payload as { type: string; paths?: string[] };
    if (payload.type === 'enter') {
      if (!remoteBusy.value) dragging.value = true;
    } else if (payload.type === 'leave') {
      dragging.value = false;
    } else if (payload.type === 'drop') {
      dragging.value = false;
      if (remoteBusy.value) return;
      // 目录过滤（含跳过提示）在 store 侧经 fs_local_stat 统一完成
      const paths = (payload.paths || []).filter(Boolean);
      if (paths.length) filesStore.uploadLocalPaths(paths);
    }
  });
});
onUnmounted(() => {
  if (unlistenOsDrag) { unlistenOsDrag(); unlistenOsDrag = null; }
});

// ============================================================
// 栏间拖拽上传（本地 → 远程）：本地行 dragstart（FileColumnList）写自定义 MIME，
// 远程栏 wrapper 判定后放行 drop，逐条走 filesStore.uploadLocalEntry 现有管线
// （分块上传 / 同名覆盖确认 / 传输队列 / toast 均复用，不新增 store 逻辑）。
// v0.18：MIME 常量上移 lib/fileTypes（EditorSurface 拖拽打开也判定它）。
// ============================================================

const columnDragging = ref(false);
const dragEntryCount = ref(0);
let columnDragLeaveTimer: ReturnType<typeof setTimeout> | null = null;

function hasInternalFileDrag(event: DragEvent) {
  return Boolean(event.dataTransfer?.types?.includes(FILE_DRAG_MIME));
}

function onLocalDragStart(count: number) {
  dragEntryCount.value = Number(count) || 1;
}

function onRemoteColumnDragOver(event: DragEvent) {
  if (!hasInternalFileDrag(event)) return; // OS 文件拖拽走整面 dropzone
  event.preventDefault();
  event.dataTransfer!.dropEffect = 'copy';
  if (remoteBusy.value) return;
  columnDragging.value = true;
  if (columnDragLeaveTimer) { clearTimeout(columnDragLeaveTimer); columnDragLeaveTimer = null; }
}

function onRemoteColumnDragLeave() {
  // 照整面 dropzone 的 80ms timer 防抖，避免子元素切换误判。
  if (columnDragLeaveTimer) clearTimeout(columnDragLeaveTimer);
  columnDragLeaveTimer = setTimeout(() => { columnDragging.value = false; }, 80);
}

async function onRemoteColumnDrop(event: DragEvent) {
  if (columnDragLeaveTimer) { clearTimeout(columnDragLeaveTimer); columnDragLeaveTimer = null; }
  columnDragging.value = false;
  if (!hasInternalFileDrag(event)) return;
  event.preventDefault();
  event.stopPropagation(); // 内部拖拽不冒泡到整面 onDrop
  if (remoteBusy.value) return;
  let payload: { entries?: RemoteFileEntry[] } | null = null;
  try {
    payload = JSON.parse(event.dataTransfer!.getData(FILE_DRAG_MIME) || 'null');
  } catch {
    payload = null;
  }
  const entries = Array.isArray(payload?.entries) ? payload.entries.filter((e) => e && e.path) : [];
  if (!entries.length) return;
  // uploadLocalEntry 仅接受 kind='file'（目录在其内部即拒），此处先过滤。
  const fileEntries = entries.filter((e) => e.kind === 'file');
  if (!fileEntries.length) {
    uiStore.notify('暂不支持目录上传', { level: 'warn' });
    return;
  }
  for (const entry of fileEntries) {
    // 逐条 await：同名覆盖确认等交互按序进行，避免并发上传互踩。
    await filesStore.uploadLocalEntry(entry);
  }
  if (fileEntries.length < entries.length) {
    uiStore.notify(`已跳过 ${entries.length - fileEntries.length} 个目录（暂不支持目录上传）`, { level: 'warn' });
  }
}

function onAnyDragEnd() {
  // 拖拽源（本地行）结束（含 Esc 取消）：清栏级拖入态，防 overlay 残留。
  // OS 外部文件拖入不触发本元素 dragend，不影响整面 dropzone 行为。
  if (columnDragLeaveTimer) { clearTimeout(columnDragLeaveTimer); columnDragLeaveTimer = null; }
  columnDragging.value = false;
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
// v0.20（SSH P2）：chmod / readlink——经编辑器通用弹窗（editor store 的 openDialog
// input 形态现成，v0.18 建立的多用途通道）。
const editorStore = useEditorStore();

async function chmodViaDialog(entry: RemoteFileEntry) {
  editorStore.openDialog({
    title: '修改权限',
    message: `为 ${entry.name} 设置八进制权限（3-4 位，如 644 / 0755）。当前：${entry.permissions || '未知'}`,
    input: { label: '八进制权限', placeholder: '644', value: entry.permissions || '' },
    buttons: [
      { label: '取消' },
      {
        label: '应用',
        primary: true,
        action: async (inputValue?: string) => {
          const mode = (inputValue || '').trim();
          if (!/^[0-7]{3,4}$/.test(mode) || (mode.length === 4 && mode[0] !== '0')) {
            return '权限格式无效：期望 3-4 位八进制（如 644 或 0755）';
          }
          const session = resolveSessionForAsset(null);
          // resolveSessionForAsset(null) 取 selectedAsset 的会话——与 sftp 链路一致
          const err = await invokeBackend<string | null>('sftp_chmod', {
            sessionId: session?.sessionId ?? '',
            path: entry.path,
            mode
          }).then(() => null).catch(e => String(e));
          if (err) return `chmod 失败：${err}`;
          filesStore.refreshRemoteFiles();
          return null; // null = 关闭弹窗
        }
      }
    ]
  });
}

async function showSymlinkTarget(entry: RemoteFileEntry) {
  const session = resolveSessionForAsset(null);
  if (!session) {
    workbench.announce?.('readlink 需要活跃会话', { level: 'warn' });
    return;
  }
  try {
    const target = await invokeBackend<string>('sftp_readlink', {
      sessionId: session.sessionId,
      path: entry.path
    });
    editorStore.openDialog({
      title: '链接目标',
      message: `${entry.name} → ${target}`,
      buttons: [{ label: '关闭', primary: true }]
    });
  } catch (error) {
    workbench.announce?.('readlink 失败：' + errorMessage(error), { level: 'error' });
  }
}

const contextMenuItems = computed<FileMenuItem[]>(() => {
  if (!contextMenu.value.visible) return [];
  const side = contextMenu.value.side;
  const entry = contextMenu.value.entry;
  const isDir = entry?.kind === 'directory' || entry?.kind === 'symlink';
  const make = (label: string, fn: () => void, opts: { danger?: boolean; disabled?: boolean } = {}): FileMenuItem => ({ label, action: fn, ...opts });

  if (side === 'remote') {
    const items: FileMenuItem[] = [];
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
      if (!isDir) {
        items.push(editMenuItem(entry, 'remote'));
      }
      items.push({ separator: true });
      items.push(make('重命名', () => { uiStore.modal = { type: 'rename', entry } as ModalState; }));
      items.push(make('修改权限…', () => { void chmodViaDialog(entry); }));
      items.push(make('删除', () => filesStore.removeRemote(entry), { danger: true }));
      if (entry.kind === 'symlink') {
        items.push(make('查看链接目标', () => { void showSymlinkTarget(entry); }));
      }
      items.push({ separator: true });
      items.push(make('复制路径', () => filesStore.copyRemotePath(entry)));
      items.push({ separator: true });
    }
    // 目录级操作（下沉自原 toolbar）。
    items.push(make('上传文件到当前目录', () => triggerFileUpload()));
    items.push(make('新建远程目录', () => { uiStore.modal = { type: 'mkdir', entry: null } as ModalState; }));
    items.push(make('刷新当前目录', () => filesStore.refreshRemoteFiles()));
    items.push(make(remoteListMode.value === 'detailed' ? '切换为紧凑列表' : '切换为详细列表', () =>
      filesStore.setRemoteListMode(remoteListMode.value === 'detailed' ? 'compact' : 'detailed')
    ));
    items.push({ separator: true });
    items.push(make(localPaneVisible.value ? '隐藏本地面板' : '显示本地面板', () => toggleLocalPane()));
    return items;
  }
  // Local menu
  const items: FileMenuItem[] = [];
  if (entry) {
    items.push(make(isDir ? '进入目录' : '上传到远程', () => {
      if (isDir) filesStore.navigateLocalPath(entry.path);
      else filesStore.uploadLocalEntry(entry);
    }));
    if (!isDir) {
      items.push(editMenuItem(entry, 'local'));
    }
    items.push({ separator: true });
    items.push(make('重命名', () => { uiStore.modal = { type: 'localRename', entry } as ModalState; }));
    items.push(make('删除', () => filesStore.localDelete([entry.path]), { danger: true }));
    items.push({ separator: true });
    items.push(make('复制路径', () => { filesStore.copyRemotePath(entry); }));
    items.push({ separator: true });
  }
  // 目录级操作（下沉自原 toolbar）。本地浏览需桌面运行时。
  items.push(make('新建本地目录', () => { uiStore.modal = { type: 'localMkdir', entry: null } as ModalState; }, { disabled: !isTauriCore.value }));
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
    @dragend="onAnyDragEnd"
  >

    <!-- ============ file-header（app.css L694-701：view-pills + file-actions）============ -->
    <header class="file-header">
      <!-- 视图胶囊：仅远程 / 双栏（app.css view-pills L703-727）-->
      <div class="view-pills" role="tablist" aria-label="文件视图模式">
        <button
          type="button"
          class="view-pill"
          :class="{ active: !localPaneVisible }"
          role="tab"
          :aria-selected="!localPaneVisible ? 'true' : 'false'"
          title="仅远程"
          @click="localPaneVisible && toggleLocalPane()"
        >仅远程</button>
        <button
          type="button"
          class="view-pill"
          :class="{ active: localPaneVisible }"
          role="tab"
          :aria-selected="localPaneVisible ? 'true' : 'false'"
          title="本地 / 远程 双栏"
          @click="!localPaneVisible && toggleLocalPane()"
        >双栏</button>
      </div>
      <div class="file-actions">
        <button class="icon-btn" type="button" title="新建目录" aria-label="新建目录" :disabled="remoteBusy" @click="uiStore.modal = ({ type: 'mkdir', entry: null } as ModalState)">
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
        @drag-start="onLocalDragStart"
        @open-text="entry => openInEditor(entry, 'local')"
      />
      <div v-if="localPaneVisible" class="file-divider" aria-hidden="true"></div>
      <!-- 远程栏 wrapper：栏间拖拽（自定义 MIME）的 drop 目标，与整面 OS 文件 dropzone 分离 -->
      <div
        class="file-pane file-pane-remote file-pane-remote-wrap"
        @dragover="onRemoteColumnDragOver"
        @dragleave="onRemoteColumnDragLeave"
        @drop="onRemoteColumnDrop"
      >
        <FileColumn kind="remote" @open-text="entry => openInEditor(entry, 'remote')">
          <!-- 远程列表头弱提示（S2）：常显，最少打扰 -->
          <template #actions-leading>
            <span class="file-column-hint" title="右键文件或空白处查看更多操作">右键查看更多操作</span>
          </template>
        </FileColumn>
        <!-- 栏级拖拽 overlay：仅覆盖远程栏（内缩 8px），区别于 OS 拖入的整面 drag-overlay -->
        <div v-if="columnDragging" class="column-drag-overlay" aria-hidden="true">
          <span>松开：上传 {{ dragEntryCount }} 项到 {{ filesStore.remotePath || '/' }}</span>
        </div>
      </div>
    </div>

    <!-- ============ 上传进度提示条（v0.19）：上传区域正下方，文档流内联不遮挡；
         未上传时不渲染（组件内部 v-if，网格 auto 行塌缩为 0）============ -->
    <UploadProgressStrip />

    <!-- ============ 拖拽上传视觉提示（app.css L770-785）============ -->
    <!-- dragging 时整面 accent 虚线 + 浮层提示目标路径；无拖拽时不渲染常驻提示 -->
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

<style scoped lang="scss" src="./FileSurface.surface.scss"></style>
