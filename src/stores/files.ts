import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import type { ModalState, NormalizedConnectionAsset, NotifyOptions, RemoteFileEntry } from '@/types/domain';
import { invokeBackend, isTauriRuntime } from '../services/backend';
import { errorMessage } from '../lib/errorMessage';
import { useClipboard } from '@/composables/useClipboard';
import type {
  ContextMenuState,
  FileOperationEntry,
  FilesWorkbenchBridge,
  PendingFileDelete,
  PendingFileOverwrite,
  QueueItem
} from '@/lib/fileTypes';
import {
  bindRemoteListState,
  computeFiltered,
  sortEntries,
  bindFilePanelContext,
  confirmFileDelete,
  cancelFileDelete,
  getActiveSession,
  handleAssetSelected,
  handleSessionClosed,
  handleSessionConnected,
  localDelete,
  localMkdir,
  localRename,
  batchRemoteDelete,
  navigateLocalUp,
  navigateLocalPath,
  navigateRemotePath,
  navigateRemoteUp,
  removeRemote,
  refreshLocalFiles,
  refreshRemoteFiles,
  renameRemote,
  resetRemotePanel,
  resolveSessionForAsset,
  mkdirRemote,
  setLocalViewMode,
  statRemote,
  statRemotePath,
  syncTerminalCwd,
  applyRemoteListing,
  toggleRemoteSelection,
  selectAllRemote,
  clearRemoteSelection,
  toggleLocalSelection,
  selectAllLocal,
  clearLocalSelection,
  setRemoteSort,
  setRemoteFilter,
  setRemoteListMode,
  setLocalListMode,
  setManualRemotePath,
  goToManualRemotePath,
  setManualLocalPath,
  goToManualLocalPath,
  noSessionMessage,
  type FilePanelContext
} from '@/lib/filePanel';
import {
  bindFileTransfersContext,
  cancelFileOverwrite,
  cancelTransfer,
  confirmFileOverwrite,
  disposeEventListeners,
  downloadEntry,
  handleFileOverwrite,
  retryTransfer,
  setupEventListeners,
  updateTransferProgress,
  markTransferError,
  pruneFinishedTransfers,
  uploadLocalEntry,
  uploadLocalPaths,
  withTransfer,
  type FileTransfersContext,
  type PendingOverwrite
} from '@/lib/fileTransfers';
/**
 * useFilesStore — Wave 2 Step 2.2
 *
 * 从 workbench.js 抽取所有 file manager 相关 state / actions / computed：
 *   - transferQueue + 进度事件监听（move from workbench setupEventListeners）
 *   - remote / local 路径 + entries + 浏览
 *   - SFTP 上传/下载/mkdir/rename/remove/stat
 *   - 本地 fs_local_* 命令
 *   - 多选 / 排序 / 过滤 / 右键菜单 / 传输抽屉
 *
 * 跨 store 桥接（lazy getter 注入）：
 *   - workbench.announce(message) / statusMessage
 *   - workbench.selectedAsset (computed ref)
 *   - workbench.setTab(tab) + workbench.modal（modal 用于 createTunnel/mkdir 流；files 不写 modal）
 *   - sessions.activeSession + sessions.sessions + sessions.connectSelected
 *
 * 注意：files store 自己注册 transfer progress 监听器（CRITICAL）
 * workbench.setupEventListeners 不再处理 transfer progress。
 */
export const useFilesStore = defineStore('files', () => {

  // ============================================================
  // State（原 workbench.js:48-69）
  // ============================================================
  // 初始空：真实路径由首次加载（服务器 canonicalize 家目录）或 OSC 7 上报写入
  const remotePath = ref('');
  const remoteEntries = ref<RemoteFileEntry[]>([]);
  const transferQueue = ref<QueueItem[]>([]);
  const localPath = ref('');
  const localEntries = ref<RemoteFileEntry[]>([]);
  const localViewMode = ref(isTauriRuntime() ? 'browser' : 'queue');
  const selectedRemotePaths = ref<Set<string>>(new Set());
  const selectedLocalPaths = ref<Set<string>>(new Set());
  const remoteSortKey = ref('name');
  const remoteSortDir = ref('asc');
  const remoteFilter = ref('');
  const remoteListMode = ref('detailed');
  const manualRemotePathInput = ref('');
  const manualLocalPathInput = ref('');
  const lastSelectedRemoteIndex = ref(-1);
  const contextMenu = ref<ContextMenuState>({ visible: false, x: 0, y: 0, side: 'remote', entry: null });
  // 传输队列抽屉默认收起：用户有传输时状态栏「传输」或文件区 trigger 可展开。
  const transferDrawerOpen = ref(false);
  // 文件区默认「仅远程」（远程栏独享全宽），点 view-pills 的「双栏」展开本地面板。
  const localPaneVisible = ref(false);
  const fileOperationStack = ref<{ remote: FileOperationEntry[]; local: FileOperationEntry[] }>({ remote: [], local: [] });
  // 删除确认链：removeRemote / localDelete / batchRemoteDelete 只组装 pending 并弹
  // confirmFileDelete modal，真正删除在 confirmFileDelete()（用户点「删除」后）执行。
  const pendingFileDelete = ref<PendingFileDelete | null>(null);
  // 上传覆盖保护：同名目标存在时挂起上传循环，等 confirmFileOverwrite / cancelFileOverwrite resolve。
  // 必须用【队列】而不是单例 ref：两个上传批次同时命中同名确认时，单例会被后者
  // 覆盖，前者的 resolve 随之丢失——那个上传循环永久挂起，传输抽屉里永远残留一条
  // 假「传输中」，且旧清理逻辑会把另一条 pending 一起清掉。队列保证每个 pending
  // 都能被 settle，并且确认/取消只作用于队首。
  const overwriteQueue = ref<PendingFileOverwrite[]>([]);
  // 远程目录加载错误（列表空态显示「加载失败 + 重试」）。
  const remoteError = ref('');
  // 远程目录是否成功加载过（区分「尚未加载」与「该目录为空」）。
  const remoteLoaded = ref(false);
  // remoteLoaded 归属的资产 id：remote* 是全局单例状态，切换 selectedAsset 时若
  // 不跟踪归属，面板会残留上一资产的目录（连接 B 成功后仍显示 A 的 /root），
  // handleSessionConnected 也会因 remoteLoaded=true 跳过 B 的自动加载。
  const remoteLoadedAssetId = ref<string | null>(null);
  // 本地列表模式独立于 remoteListMode（本地表头不再被远程 listMode 绑架）。
  const localListMode = ref('detailed');
  const lastSelectedLocalIndex = ref(-1);

  // 进度事件 unlisten handle（必须 init 后保存，不能跨 store 共享）
  let progressUnlisten: TauriUnlistenFn | null = null;

  // ============================================================
  // 跨 store 桥接（lazy）
  // ============================================================
  let workbenchBridge: FilesWorkbenchBridge | null = null;
  function attachWorkbench(store: FilesWorkbenchBridge) {
    workbenchBridge = store;
  }
  function wb(): FilesWorkbenchBridge {
    if (!workbenchBridge) {
      throw new Error('files store: workbench bridge not attached. Call filesStore.attachWorkbench(workbenchStore) at App.vue init.');
    }
    return workbenchBridge;
  }
  function announce(message: string, opts?: NotifyOptions) {
    if (workbenchBridge && typeof workbenchBridge.announce === 'function') {
      return workbenchBridge.announce(message, opts);
    }
    // eslint-disable-next-line no-console
    console.log('[files] announce:', message);
  }
  // selectedAsset / setTab / modal 通过 wb() getter 实时读
  // sessions store 通过 wb().sessionsStore() 拿到（workbench 暴露 sessionsStore 引用）

  // ============================================================
  // Computed（原 workbench.js:101-139）
  // ============================================================
  const activeTransfers = computed(() => transferQueue.value.filter(item => item.status === 'running' || item.status === 'pending'));
  // 「完成」只算真正完成项；失败/取消归入 failedTransfers（状态栏/抽屉计数拆分）。
  const completedTransfers = computed(() => transferQueue.value.filter(item => item.status === 'done'));
  const failedTransfers = computed(() => transferQueue.value.filter(item => item.status === 'error' || item.status === 'cancelled'));
  const remoteBusy = computed(() => fileOperationStack.value.remote.length > 0);
  const localBusy = computed(() => fileOperationStack.value.local.length > 0);
  const remoteBusyMessage = computed(() => {
    const stack = fileOperationStack.value.remote;
    return stack[stack.length - 1]?.message || '正在处理远程文件...';
  });
  const localBusyMessage = computed(() => {
    const stack = fileOperationStack.value.local;
    return stack[stack.length - 1]?.message || '正在处理本地文件...';
  });

  const selectedRemoteEntries = computed(() =>
    remoteEntries.value.filter(e => selectedRemotePaths.value.has(e.path))
  );

  function beginFileOperation(side: 'remote' | 'local', message: string) {
    const token = Symbol(side);
    fileOperationStack.value[side] = [
      ...fileOperationStack.value[side],
      { token, message }
    ];
    return () => {
      fileOperationStack.value[side] = fileOperationStack.value[side].filter(item => item.token !== token);
    };
  }

  async function withFileOperation<T>(side: 'remote' | 'local', message: string, task: () => Promise<T>) {
    const end = beginFileOperation(side, message);
    try {
      return await task();
    } finally {
      end();
    }
  }

  /**
   * 轻量传输包装：只维护队列状态（status），不触碰 fileOperationStack——
   * 传输 busy 与浏览 busy 解耦，上传/下载不再把整列锁进 loading 遮罩。
   * 完成/失败/取消状态由各传输函数与进度事件维护。
   */

  // ============================================================
  // 右键菜单 / 批量操作
  // ============================================================
  function openContextMenu({ x, y, side, entry }: { x: number; y: number; side: string; entry: RemoteFileEntry | null }) {
    contextMenu.value = { visible: true, x, y, side, entry };
  }
  function closeContextMenu() {
    contextMenu.value = { ...contextMenu.value, visible: false };
  }

  async function batchRemoteDownload() {
    const paths = Array.from(selectedRemotePaths.value);
    if (!paths.length) return;
    const targets = paths
      .map(path => remoteEntries.value.find(e => e.path === path))
      .filter((e): e is RemoteFileEntry => !!e && e.kind === 'file');
    if (!targets.length) return;
    // 整批只问一次保存目录，再让每个文件落进同一目录（避免逐文件弹框）。
    // 用户取消则整批取消（与逐文件取消的语义一致：不算失败）。
    const dir = await invokeBackend<string | null>('plugin:dialog|open', {
      options: {
        title: '选择下载保存目录（本批 ' + targets.length + ' 个文件）',
        directory: true,
        multiple: false,
        defaultPath: localPath.value || undefined
      }
    });
    if (!dir) {
      announce('已取消批量下载', { level: 'warn' });
      return;
    }
    // 下载不走 withFileOperation：传输 busy 与浏览 busy 解耦，不锁列表
    for (const entry of targets) {
      await downloadEntry(entry, dir).catch(error =>
        announce('下载失败：' + entry.name + '：' + errorMessage(error), { level: 'error' })
      );
    }
  }

  async function copyRemotePath(entry?: RemoteFileEntry | null) {
    const target = entry?.path || remotePath.value;
    if (!target) {
      announce('路径为空，无法复制', { level: 'warn' });
      return;
    }
    // 走 useClipboard 三级 fallback（navigator → Tauri 插件 → execCommand）：
    // webview 非聚焦时裸用 navigator.clipboard 会静默失败（既没复制也没提示）
    if (await useClipboard().copy(target)) {
      announce('已复制路径：' + target);
    } else {
      announce('剪贴板不可用', { level: 'warn' });
    }
  }

  function toggleTransferDrawer() { transferDrawerOpen.value = !transferDrawerOpen.value; }
  function toggleLocalPane() { localPaneVisible.value = !localPaneVisible.value; }
  function setLocalPaneVisible(value: boolean) { localPaneVisible.value = !!value; }

  // —— 列表视图域（排序/过滤与多选函数在 lib/filePanel；视图状态本 store 持有）——
  const filteredRemoteEntries = computed(() => computeFiltered());
  const sortedRemoteEntries = computed(() => sortEntries(filteredRemoteEntries.value));
  bindRemoteListState({
    filter: remoteFilter,
    sortKey: remoteSortKey,
    sortDir: remoteSortDir,
    lastRemote: lastSelectedRemoteIndex,
    lastLocal: lastSelectedLocalIndex,
    selectedRemote: selectedRemotePaths,
    selectedLocal: selectedLocalPaths,
    remoteEntries,
    listMode: remoteListMode,
    localListMode
  });

  // —— 面板 / 传输 lib 的 context（绑定式，先例 terminalLifecycle；组装一次，
  // 成员为 refs 或回调，取值恒新）——
  const panelCtx: FilePanelContext = {
    wb,
    announce,
    remotePath,
    remoteEntries,
    remoteLoaded,
    remoteLoadedAssetId,
    remoteError,
    localPath,
    localEntries,
    localViewMode,
    manualRemotePathInput,
    manualLocalPathInput,
    selectedRemotePaths,
    pendingFileDelete,
    fileOperationStack,
    requireRemotePath: () => requireRemotePathShell(),
    withFileOperation,
    refreshLocalFilesCb: () => { refreshLocalFiles().catch(() => null); }
  };
  const transfersCtx: FileTransfersContext = {
    wb,
    announce,
    transferQueue,
    transferDrawerOpen,
    overwriteQueue,
    remotePath,
    localPath,
    remoteEntries,
    localEntries,
    getActiveSession,
    noSessionMessage,
    requireRemotePath: () => requireRemotePathShell(),
    refreshRemoteFiles,
    refreshLocalFiles,
    resolveSessionForAsset,
    statRemotePath
  };
  // 远程写前置守卫原在面板段，两域共用 → shell 持有并回调注入
  function requireRemotePathShell(): boolean {
    if (remotePath.value) return true;
    announce('远程目录尚未加载，请先刷新后再操作', { level: 'warn' });
    return false;
  }
  bindFilePanelContext(panelCtx);
  bindFileTransfersContext(transfersCtx);

  // ============================================================
  // clearSelectionOnAssetSwitch — assets store selectAsset 时调用
  // ============================================================
  function clearSelection() {
    selectedRemotePaths.value = new Set();
    selectedLocalPaths.value = new Set();
    lastSelectedRemoteIndex.value = -1;
    lastSelectedLocalIndex.value = -1;
  }

  return {
    // state
    remotePath,
    remoteEntries,
    transferQueue,
    localPath,
    localEntries,
    localViewMode,
    selectedRemotePaths,
    selectedLocalPaths,
    remoteSortKey,
    remoteSortDir,
    remoteFilter,
    remoteListMode,
    localListMode,
    manualRemotePathInput,
    manualLocalPathInput,
    lastSelectedRemoteIndex,
    contextMenu,
    transferDrawerOpen,
    localPaneVisible,
    pendingFileDelete,
    remoteError,
    remoteLoaded,
    // computed
    activeTransfers,
    completedTransfers,
    failedTransfers,
    remoteBusy,
    localBusy,
    remoteBusyMessage,
    localBusyMessage,
    filteredRemoteEntries,
    sortedRemoteEntries,
    selectedRemoteEntries,
    // bridge
    attachWorkbench,
    // lifecycle
    setupEventListeners,
    disposeEventListeners,
    // transfer ops
    updateTransferProgress,
    pruneFinishedTransfers,
    markTransferError,
    cancelTransfer,
    retryTransfer,
    // remote sftp
    refreshRemoteFiles,
    applyRemoteListing,
  toggleRemoteSelection,
  selectAllRemote,
  clearRemoteSelection,
  toggleLocalSelection,
  selectAllLocal,
  clearLocalSelection,
  setRemoteSort,
  setRemoteFilter,
  setRemoteListMode,
  setLocalListMode,
  setManualRemotePath,
  goToManualRemotePath,
  setManualLocalPath,
  goToManualLocalPath,
    navigateRemotePath,
    navigateRemoteUp,
    handleSessionClosed,
    handleSessionConnected,
    handleAssetSelected,
    syncTerminalCwd,
    uploadLocalEntry,
    uploadLocalPaths,
    downloadEntry,
    mkdirRemote,
    renameRemote,
    removeRemote,
    statRemote,
    // 删除确认链
    confirmFileDelete,
    cancelFileDelete,
    // 上传覆盖保护
    confirmFileOverwrite,
    cancelFileOverwrite,
    // local fs
    refreshLocalFiles,
    navigateLocalPath,
    navigateLocalUp,
    localMkdir,
    localDelete,
    localRename,
    setLocalViewMode,
    // selection / sort / filter / context menu
    openContextMenu,
    closeContextMenu,
    batchRemoteDelete,
    batchRemoteDownload,
    copyRemotePath,
    toggleTransferDrawer,
    toggleLocalPane,
    setLocalPaneVisible,
    // helpers exposed to assets store
    clearSelection
  };
});
