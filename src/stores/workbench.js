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
import { useMcpStore } from './mcp.js';
import { useSyncStore } from './sync.js';

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
  const mcpStore = useMcpStore();
  const syncStore = useSyncStore();

  // ============================================================
  // 公共 announce — 委托到 uiStore（statusMessage 的 owner）
  // ============================================================
  function announce(message) {
    uiStore.statusMessage = message;
  }

  // ============================================================
  // 跨域 Computed — warningCount 同时依赖 assets 与 tunnels
  // ============================================================
  // warningCount 反映运行时真实告警：异常会话（status='error'）+ 隧道错误。
  // 旧实现从 asset.status 过滤，但 asset.status 在连接流程中从不更新（永远是
  // 'Idle'），导致计数恒为 tunnel error 数。改为派生 session + tunnel。
  const warningCount = computed(() =>
    sessionsStore.sessions.filter(session => session.status === 'error').length
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
      assetsStore.declaredGroups = assetResult.groups || [];
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

    // 5. 连接成功 → 后台预刷新远程 + 本地文件。不切 tab（保留「连接后看终端」的常规体验），
    //    仅让用户切到 files tab 时即可看到内容。切 tab 时 ui.js 的 setTab('files')
    //    会再补一次刷新（幂等，无副作用）。守卫 prev !== 'connected' 防重连/抖动重复刷新。
    watch(
      () => sessionsStore.activeSession?.status,
      (status, prev) => {
        if (status === 'connected' && prev !== 'connected') {
          filesStore.refreshRemoteFiles().catch(() => null);
          filesStore.refreshLocalFiles().catch(() => null);
        }
      }
    );
  }

  // workbench.setupEventListeners 仅做转发：
  // - sessions store 处理 host key / keyboard 监听
  // - files store 处理 sftp-transfer-progress 监听
  // - ui store 处理 system theme 监听（在 initializeTheme 中初始化）
  // - mcp store 处理 mcp-tool-approval 监听（v1.5 GUI 弹窗审批）
  async function setupEventListeners() {
    if (!isTauriRuntime()) return;
    // v1.2：MCP 探测是无状态按需调用，启动时主动探测一次，
    // 让状态灯打开程序即可见结果。
    mcpStore.refresh();
    // v1.3：启动时刷新同步状态（供状态栏展示真实同步配置状态）
    syncStore.refreshStatus();
    await Promise.all([
      sessionsStore.setupEventListeners(),
      filesStore.setupEventListeners(),
      mcpStore.setupEventListeners()
    ]);
  }

  async function disposeEventListeners() {
    uiStore.disposeSystemThemeListener();
    await Promise.all([
      sessionsStore.disposeEventListeners(),
      filesStore.disposeEventListeners(),
      mcpStore.disposeEventListeners()
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

  // v1.5：mcp store 需 modal setter（弹/关审批窗）+ announce（超时提示）。
  mcpStore.attachWorkbench({
    announce,
    get modal() { return uiStore.modal; },
    set modal(v) { uiStore.modal = v; }
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
    rightCollapsed: computed(() => uiStore.rightCollapsed),
    statusMessage: computed(() => uiStore.statusMessage),
    // modal 必须可写：App.vue onCreateAsset / GlobalModals closeModal 等通过
    // `store.modal = {...}` 赋值。纯 computed 是只读的，赋值静默失败（曾导致
    // "新建连接打不开" / modal 无法关闭）。这里加 setter 转发到 uiStore.modal，
    // 与 workbench bridge setter（L130/143/150/157）行为一致。
    modal: computed({
      get: () => uiStore.modal,
      set: (v) => { uiStore.modal = v; }
    }),
    searchState: computed(() => uiStore.searchState),
    backendStatusText: computed(() => uiStore.backendStatusText),
    backendMode: computed(() => uiStore.backendMode),
    // --- assets re-export ---
    assetSource: computed(() => assetsStore.assetSource),
    assets: computed(() => assetsStore.assets),
    selectedAssetId: computed(() => assetsStore.selectedAssetId),
    selectedAsset: computed(() => assetsStore.selectedAsset),
    groupedAssets: computed(() => assetsStore.groupedAssets),
    declaredGroups: computed(() => assetsStore.declaredGroups),
    githubPatConfigured: computed(() => assetsStore.githubPatConfigured),
    assetSourceText: computed(() => assetsStore.assetSourceText),
    // v1.3：syncText 改为真实同步状态（来自 syncStore，含 push/pull 结果），
    // 而非 assetsStore 的 PAT 配置状态。
    syncText: computed(() => syncStore.syncText),
    // --- v1.3 sync re-export ---
    syncConfigured: computed(() => syncStore.configured),
    syncLastSyncedAt: computed(() => syncStore.lastSyncedAt),
    syncGistIdMasked: computed(() => syncStore.gistIdMasked),
    syncConflict: computed(() => syncStore.conflict),
    syncLoading: computed(() => syncStore.loading),
    syncRefreshStatus: () => syncStore.refreshStatus(),
    syncSetup: syncStore.setup,
    syncPush: syncStore.push,
    syncPull: syncStore.pull,
    syncResolveConflict: syncStore.resolveConflict,
    syncResetMasterPassword: syncStore.resetMasterPassword,
    syncClear: syncStore.clearSync,
    syncDismissConflict: syncStore.dismissConflict,
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
    manualLocalPathInput: computed(() => filesStore.manualLocalPathInput),
    contextMenu: computed(() => filesStore.contextMenu),
    transferDrawerOpen: computed(() => filesStore.transferDrawerOpen),
    localPaneVisible: computed(() => filesStore.localPaneVisible),
    activeTransfers: computed(() => filesStore.activeTransfers),
    completedTransfers: computed(() => filesStore.completedTransfers),
    filteredRemoteEntries: computed(() => filesStore.filteredRemoteEntries),
    sortedRemoteEntries: computed(() => filesStore.sortedRemoteEntries),
    selectedRemoteEntries: computed(() => filesStore.selectedRemoteEntries),
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
    toggleRight: uiStore.toggleRight,
    openGlobalSearch: uiStore.openGlobalSearch,
    closeGlobalSearch: uiStore.closeGlobalSearch,
    setGlobalSearchQuery: uiStore.setGlobalSearchQuery,
    activateSuggestion: uiStore.activateSuggestion,
    // --- assets re-export actions ---
    saveAsset: assetsStore.saveAsset,
    deleteAsset: assetsStore.deleteAsset,
    duplicateAsset: assetsStore.duplicateAsset,
    moveAsset: assetsStore.moveAsset,
    renameGroup: assetsStore.renameGroup,
    dissolveGroup: assetsStore.dissolveGroup,
    createGroup: assetsStore.createGroup,
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
    setManualLocalPath: filesStore.setManualLocalPath,
    goToManualLocalPath: filesStore.goToManualLocalPath,
    openContextMenu: filesStore.openContextMenu,
    closeContextMenu: filesStore.closeContextMenu,
    batchRemoteDelete: filesStore.batchRemoteDelete,
    batchRemoteDownload: filesStore.batchRemoteDownload,
    copyRemotePath: filesStore.copyRemotePath,
    toggleLocalSelection: filesStore.toggleLocalSelection,
    selectAllLocal: filesStore.selectAllLocal,
    clearLocalSelection: filesStore.clearLocalSelection,
    toggleTransferDrawer: filesStore.toggleTransferDrawer,
    toggleLocalPane: filesStore.toggleLocalPane,
    setLocalPaneVisible: filesStore.setLocalPaneVisible,
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
    // --- v1.2：MCP 服务可观测性 (re-export from useMcpStore) ---
    mcpStatus: computed(() => mcpStore.status),
    mcpProbe: computed(() => mcpStore.probe),
    mcpClientConnected: computed(() => mcpStore.clientConnected),
    mcpTools: computed(() => mcpStore.tools),
    mcpResources: computed(() => mcpStore.resources),
    mcpPrompts: computed(() => mcpStore.prompts),
    mcpDataDir: computed(() => mcpStore.dataDir),
    mcpServerVersion: computed(() => mcpStore.serverVersion),
    mcpEndpoint: computed(() => mcpStore.endpoint),
    refreshMcpStatus: () => mcpStore.refresh(),
    buildMcpConfig: mcpStore.buildConfig,
    // v1.5：GUI 弹窗审批（GlobalModals 经此读 prompt + 调 resolve）
    mcpApprovalPrompt: computed(() => mcpStore.approvalPrompt),
    resolveMcpApproval: mcpStore.resolveMcpApproval,
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
  // dotClass 不带前导空格（Vue class 绑定会自动处理空格），与
  // ConnectionSidebar.vue 本地副本统一，避免两份实现漂移。
  if (status === 'Connected' || status === 'connected') return { label: 'connected', dotClass: 'running' };
  if (status === 'Warning' || status === 'warning') return { label: 'warning', dotClass: 'warn' };
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
