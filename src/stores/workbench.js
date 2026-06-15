import { defineStore } from 'pinia';
import { computed, watch } from 'vue';
import {
  invokeBackend,
  isTauriRuntime,
  normalizeAsset,
  normalizeTunnelStatus
} from '../services/backend.js';
import { applyTheme } from '../composables/useTheme.js';
import { useSessionsStore } from './sessions.js';
import { useFilesStore } from './files.js';
import { useTunnelsStore } from './tunnels.js';
import { useAssetsStore } from './assets.js';
import { useUiStore } from './ui.js';

export const useWorkbenchStore = defineStore('workbench', () => {
  // ============================================================
  // Child store 实例化
  // sessions store 用 detached effectScope 注册 hostKeyPrompt watcher，
  // 因此 workbench setup 期间调用 useSessionsStore() 不再触发
  // "Cannot read properties of null (reading 'effect')"。
  // files / tunnels / assets / ui store 没有 effect scope 副作用，可放心实例化。
  // ============================================================
  const sessionsStore = useSessionsStore();
  const filesStore = useFilesStore();
  const tunnelsStore = useTunnelsStore();
  const assetsStore = useAssetsStore();
  const uiStore = useUiStore();

  // ============================================================
  // 公共 announce — 委托到 uiStore（statusMessage 的 owner）
  // ============================================================
  function announce(message) {
    uiStore.statusMessage = message;
  }

  // ============================================================
  // 跨域 Computed — warningCount 同时依赖 assets 与 tunnels
  // ============================================================
  const warningCount = computed(() =>
    assetsStore.assets.filter(asset => normalizeStatus(asset.status).label === 'warning').length
    + tunnelsStore.tunnels.filter(tunnel => tunnel.error).length
  );

  // ============================================================
  // selectAsset — 转发到 assetsStore
  // ============================================================
  function selectAsset(id, announceSelection = true) {
    return assetsStore.selectAsset(id, announceSelection);
  }

  // ============================================================
  // Initialize — orchestrate all 5 sub-stores
  // ============================================================
  async function initialize() {
    // 1. UI 主题初始化（applyTheme + systemThemeListener + dataset.assets）
    uiStore.initializeTheme();

    // 2. 主题 watcher → applyTheme + 终端主题同步
    // 注意：Pinia store 解构后 effectiveTheme 会被解包成普通值，必须用 getter
    // 函数包裹才能被 watch 正确监听（否则警告 "Invalid watch source"）。
    watch(() => uiStore.effectiveTheme, next => { applyTheme(next); });
    watch(() => uiStore.effectiveTheme, () => { sessionsStore.updateAllTerminalThemes(); });
    watch(() => uiStore.systemPrefersDark, v => { sessionsStore.systemPrefersDark = v; });
    sessionsStore.systemPrefersDark = uiStore.systemPrefersDark;

    // 3. backend + assets + tunnels 启动加载
    try {
      const [status, assetResult, credential, tunnelResult] = await Promise.all([
        invokeBackend('backend_status'),
        invokeBackend('list_connection_assets'),
        invokeBackend('get_credential_status', { id: 'github-pat' }).catch(() => ({ exists: false })),
        invokeBackend('tunnel_list').catch(() => [])
      ]);
      uiStore.backendStatus = status;
      assetsStore.assetSource = { ...assetResult, count: assetResult.count ?? assetResult.assets?.length ?? 0 };
      assetsStore.assets = (assetResult.assets || []).map(normalizeAsset);
      assetsStore.githubPatConfigured = Boolean(credential.exists);
      tunnelsStore.tunnels = (tunnelResult || []).map(normalizeTunnelStatus);
    } catch (error) {
      uiStore.backendStatus = { ready: false, mode: 'fallback' };
      assetsStore.assetSource = { source: 'unavailable', count: 0 };
      assetsStore.assets = [];
      announce('后端桥接初始化失败：' + error.message);
    }
    if (!assetsStore.selectedAssetId && assetsStore.assets.length) {
      assetsStore.selectedAssetId = assetsStore.assets[0].id;
    }

    // 4. 事件监听编排 — sessions + files 各管各的；ui 在 initializeTheme 内已注册 systemThemeListener
    setupEventListeners().catch(error => {
      announce('后端事件监听初始化失败：' + error.message);
    });
  }

  // workbench.setupEventListeners 仅做转发：
  // - sessions store 处理 host key / keyboard 监听
  // - files store 处理 sftp-transfer-progress 监听
  // - ui store 处理 system theme 监听（在 initializeTheme 中初始化）
  async function setupEventListeners() {
    if (!isTauriRuntime()) return;
    await Promise.all([
      sessionsStore.setupEventListeners(),
      filesStore.setupEventListeners()
    ]);
  }

  async function disposeEventListeners() {
    uiStore.disposeSystemThemeListener();
    await Promise.all([
      sessionsStore.disposeEventListeners(),
      filesStore.disposeEventListeners()
    ]);
  }

  // ============================================================
  // Bridge 注入：所有 workbench state/actions 都已声明完毕。
  // sessions store 需要 selectedAsset / effectiveTheme / modal / selectedAssetId /
  // announce / setTab / updateTransferProgress（转发到 files）。
  // files store 需要 announce / selectedAsset / setTab / modal + sessionsStore 引用。
  // tunnels store 需要 announce / modal + sessionsStore 引用。
  // assets store 需要 announce / modal + clearFileSelection（转发到 files）。
  // ui store 需要 filesStore / tunnelsStore / sessionsStore / saveAsset / selectAsset 引用
  //   用于 setTab 切到 files/tunnels 时的跨 store 编排 + 搜索 suggestion。
  // ============================================================
  sessionsStore.attachWorkbench({
    get selectedAsset() { return assetsStore.selectedAsset; },
    get effectiveTheme() { return uiStore.effectiveTheme; },
    get modal() { return uiStore.modal; },
    set modal(v) { uiStore.modal = v; },
    get selectedAssetId() { return assetsStore.selectedAssetId; },
    set selectedAssetId(v) { assetsStore.selectedAssetId = v; },
    announce,
    setTab: uiStore.setTab,
    updateTransferProgress: filesStore.updateTransferProgress
  });

  filesStore.attachWorkbench({
    announce,
    get selectedAsset() { return assetsStore.selectedAsset; },
    setTab: uiStore.setTab,
    get modal() { return uiStore.modal; },
    set modal(v) { uiStore.modal = v; },
    sessionsStore: () => sessionsStore
  });

  tunnelsStore.attachWorkbench({
    announce,
    get modal() { return uiStore.modal; },
    set modal(v) { uiStore.modal = v; },
    sessionsStore: () => sessionsStore
  });

  assetsStore.attachWorkbench({
    announce,
    get modal() { return uiStore.modal; },
    set modal(v) { uiStore.modal = v; },
    clearFileSelection: filesStore.clearSelection
  });

  uiStore.attachWorkbench({
    announce,
    filesStore: () => filesStore,
    tunnelsStore: () => tunnelsStore,
    sessionsStore: () => sessionsStore,
    assets: () => assetsStore.assets,
    selectAsset,
    saveAsset: assetsStore.saveAsset
  });

  return {
    // --- workbench-owned computed ---
    warningCount,
    // --- ui re-export ---
    backendStatus: computed(() => uiStore.backendStatus),
    activeTab: computed(() => uiStore.activeTab),
    theme: computed(() => uiStore.theme),
    effectiveTheme: computed(() => uiStore.effectiveTheme),
    themeLabel: computed(() => uiStore.themeLabel),
    systemPrefersDark: computed(() => uiStore.systemPrefersDark),
    assetsCollapsed: computed(() => uiStore.assetsCollapsed),
    statusMessage: computed(() => uiStore.statusMessage),
    modal: computed(() => uiStore.modal),
    searchState: computed(() => uiStore.searchState),
    backendStatusText: computed(() => uiStore.backendStatusText),
    backendMode: computed(() => uiStore.backendMode),
    // --- assets re-export ---
    assetSource: computed(() => assetsStore.assetSource),
    assets: computed(() => assetsStore.assets),
    selectedAssetId: computed(() => assetsStore.selectedAssetId),
    selectedAsset: computed(() => assetsStore.selectedAsset),
    groupedAssets: computed(() => assetsStore.groupedAssets),
    githubPatConfigured: computed(() => assetsStore.githubPatConfigured),
    assetSourceText: computed(() => assetsStore.assetSourceText),
    syncText: computed(() => assetsStore.syncText),
    // --- files re-export ---
    remotePath: computed(() => filesStore.remotePath),
    remoteEntries: computed(() => filesStore.remoteEntries),
    remotePathHistory: computed(() => filesStore.remotePathHistory),
    transferQueue: computed(() => filesStore.transferQueue),
    localPath: computed(() => filesStore.localPath),
    localEntries: computed(() => filesStore.localEntries),
    localViewMode: computed(() => filesStore.localViewMode),
    selectedRemotePaths: computed(() => filesStore.selectedRemotePaths),
    selectedLocalPaths: computed(() => filesStore.selectedLocalPaths),
    remoteSortKey: computed(() => filesStore.remoteSortKey),
    remoteSortDir: computed(() => filesStore.remoteSortDir),
    remoteFilter: computed(() => filesStore.remoteFilter),
    remoteListMode: computed(() => filesStore.remoteListMode),
    manualRemotePathInput: computed(() => filesStore.manualRemotePathInput),
    contextMenu: computed(() => filesStore.contextMenu),
    transferDrawerOpen: computed(() => filesStore.transferDrawerOpen),
    activeTransfers: computed(() => filesStore.activeTransfers),
    completedTransfers: computed(() => filesStore.completedTransfers),
    filteredRemoteEntries: computed(() => filesStore.filteredRemoteEntries),
    sortedRemoteEntries: computed(() => filesStore.sortedRemoteEntries),
    selectedRemoteEntries: computed(() => filesStore.selectedRemoteEntries),
    remoteBreadcrumb: computed(() => filesStore.remoteBreadcrumb),
    // --- tunnels re-export ---
    tunnels: computed(() => tunnelsStore.tunnels),
    runningTunnels: computed(() => tunnelsStore.runningTunnels),
    // --- workbench-owned actions ---
    initialize,
    disposeEventListeners,
    setupEventListeners,
    announce,
    selectAsset,
    setTab: uiStore.setTab,
    toggleTheme: uiStore.toggleTheme,
    toggleAssets: uiStore.toggleAssets,
    openGlobalSearch: uiStore.openGlobalSearch,
    closeGlobalSearch: uiStore.closeGlobalSearch,
    setGlobalSearchQuery: uiStore.setGlobalSearchQuery,
    activateSuggestion: uiStore.activateSuggestion,
    // --- assets re-export actions ---
    saveAsset: assetsStore.saveAsset,
    saveToken: assetsStore.saveToken,
    deleteToken: assetsStore.deleteToken,
    // --- files re-export actions ---
    refreshRemoteFiles: filesStore.refreshRemoteFiles,
    navigateRemotePath: filesStore.navigateRemotePath,
    navigateRemoteUp: filesStore.navigateRemoteUp,
    uploadFiles: filesStore.uploadFiles,
    downloadEntry: filesStore.downloadEntry,
    mkdirRemote: filesStore.mkdirRemote,
    renameRemote: filesStore.renameRemote,
    removeRemote: filesStore.removeRemote,
    statRemote: filesStore.statRemote,
    refreshLocalFiles: filesStore.refreshLocalFiles,
    navigateLocalPath: filesStore.navigateLocalPath,
    navigateLocalUp: filesStore.navigateLocalUp,
    localMkdir: filesStore.localMkdir,
    localDelete: filesStore.localDelete,
    localRename: filesStore.localRename,
    setLocalViewMode: filesStore.setLocalViewMode,
    toggleRemoteSelection: filesStore.toggleRemoteSelection,
    selectAllRemote: filesStore.selectAllRemote,
    clearRemoteSelection: filesStore.clearRemoteSelection,
    setRemoteSort: filesStore.setRemoteSort,
    setRemoteFilter: filesStore.setRemoteFilter,
    setRemoteListMode: filesStore.setRemoteListMode,
    setManualRemotePath: filesStore.setManualRemotePath,
    goToManualRemotePath: filesStore.goToManualRemotePath,
    openContextMenu: filesStore.openContextMenu,
    closeContextMenu: filesStore.closeContextMenu,
    batchRemoteDelete: filesStore.batchRemoteDelete,
    batchRemoteDownload: filesStore.batchRemoteDownload,
    copyRemotePath: filesStore.copyRemotePath,
    toggleLocalSelection: filesStore.toggleLocalSelection,
    selectAllLocal: filesStore.selectAllLocal,
    clearLocalSelection: filesStore.clearLocalSelection,
    toggleTransferDrawer: filesStore.toggleTransferDrawer,
    // --- tunnels re-export actions ---
    refreshTunnels: tunnelsStore.refreshTunnels,
    createTunnel: tunnelsStore.createTunnel,
    toggleTunnel: tunnelsStore.toggleTunnel,
    toggleTunnelAutoStart: tunnelsStore.toggleTunnelAutoStart,
    deleteTunnel: tunnelsStore.deleteTunnel,
    // --- session-related (re-export from useSessionsStore) ---
    sessions: computed(() => sessionsStore.sessions),
    activeSessionId: computed(() => sessionsStore.activeSessionId),
    activeSession: computed(() => sessionsStore.activeSession),
    activeSessions: computed(() => sessionsStore.activeSessions),
    hostKeyPrompt: computed(() => sessionsStore.hostKeyPrompt),
    keyboardPrompt: computed(() => sessionsStore.keyboardPrompt),
    terminalFontSize: computed(() => sessionsStore.terminalFontSize),
    terminalAsideOpen: computed(() => sessionsStore.terminalAsideOpen),
    terminalSearch: computed(() => sessionsStore.terminalSearch),
    connectSelected: sessionsStore.connectSelected,
    setActiveSession: sessionsStore.setActiveSession,
    disconnectSession: sessionsStore.disconnectSession,
    reconnectSession: sessionsStore.reconnectSession,
    runTerminalAction: sessionsStore.runTerminalAction,
    resolveHostKeyPrompt: sessionsStore.resolveHostKeyPrompt,
    resolveKeyboardPrompt: sessionsStore.resolveKeyboardPrompt,
    openTerminalSearchInline: sessionsStore.openTerminalSearchInline,
    closeTerminalSearchInline: sessionsStore.closeTerminalSearchInline,
    setTerminalSearchQuery: sessionsStore.setTerminalSearchQuery,
    findTerminalNext: sessionsStore.findTerminalNext,
    setTerminalContainer: sessionsStore.setTerminalContainer,
    setTerminalFontSize: sessionsStore.setTerminalFontSize,
    resetTerminalFontSize: sessionsStore.resetTerminalFontSize,
    toggleTerminalAside: sessionsStore.toggleTerminalAside,
    writeToActiveTerminal: sessionsStore.writeToActiveTerminal
  };
});

export function normalizeStatus(status) {
  if (status === 'Connected' || status === 'connected') return { label: 'connected', dotClass: ' running' };
  if (status === 'Warning' || status === 'warning') return { label: 'warning', dotClass: ' warn' };
  return { label: 'idle', dotClass: '' };
}

export function remotePathForAsset(asset) {
  if (!asset) return '/srv/app/releases';
  if (asset.tags.includes('backup')) return '/backup';
  if (asset.tags.includes('redis')) return '/var/lib/redis';
  if (asset.tags.includes('web') || asset.tags.includes('release')) return '/srv/app/releases';
  return '/home/' + (asset.username || 'user');
}

export function parentPath(path) {
  if (!path || path === '/' || path === '') return '/';
  const trimmed = path.replace(/\/+$/, '');
  const idx = trimmed.lastIndexOf('/');
  if (idx <= 0) return '/';
  return trimmed.slice(0, idx);
}

export function joinPath(base, name) {
  if (!base || base === '/') return '/' + name;
  return base.replace(/\/+$/, '') + '/' + name;
}

// 本地路径 helper：处理 Windows 反斜杠和 Unix 正斜杠。
// 前端只做拼接/解析，所有真实 IO 走 Rust fs_local_* 命令。
export function joinLocalPath(base, name) {
  if (!base) return name;
  const sep = base.includes('\\') && !base.includes('/') ? '\\' : '/';
  const trimmed = base.replace(/[\\/]+$/, '');
  return trimmed + sep + name;
}

export function parentLocalPath(path) {
  if (!path) return '';
  // 同时支持 Windows 和 Unix 风格
  const match = path.match(/^(.*?)[\\/]+[^\\/]+[\\/]*$/);
  if (!match) return path;
  const parent = match[1];
  if (!parent) return path; // 根目录
  // Windows 盘符根：D: -> D:\
  if (/^[a-zA-Z]:$/.test(parent)) return parent + '\\';
  return parent || path;
}
