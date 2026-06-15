import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import {
  invokeBackend,
  isTauriRuntime,
  listenBackendEvent
} from '../services/backend.js';
import {
  joinLocalPath,
  joinPath,
  parentLocalPath,
  parentPath,
  remotePathForAsset
} from './workbench.js';

// 事件 channel 常量（原 workbench.js:25）
const TRANSFER_PROGRESS_EVENT = 'sftp-transfer-progress';

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
  const remotePath = ref('/srv/app/releases');
  const remoteEntries = ref([]);
  const remotePathHistory = ref([]);
  const transferQueue = ref([]);
  const localPath = ref('');
  const localEntries = ref([]);
  const localViewMode = ref(isTauriRuntime() ? 'browser' : 'queue');
  const selectedRemotePaths = ref(new Set());
  const selectedLocalPaths = ref(new Set());
  const remoteSortKey = ref('name');
  const remoteSortDir = ref('asc');
  const remoteFilter = ref('');
  const remoteListMode = ref('detailed');
  const manualRemotePathInput = ref('');
  const lastSelectedRemoteIndex = ref(-1);
  const contextMenu = ref({ visible: false, x: 0, y: 0, side: 'remote', entry: null });
  const transferDrawerOpen = ref(true);

  // 进度事件 unlisten handle（必须 init 后保存，不能跨 store 共享）
  let progressUnlisten = null;

  // ============================================================
  // 跨 store 桥接（lazy）
  // ============================================================
  let workbenchBridge = null;
  function attachWorkbench(store) {
    workbenchBridge = store;
  }
  function wb() {
    if (!workbenchBridge) {
      throw new Error('files store: workbench bridge not attached. Call filesStore.attachWorkbench(workbenchStore) at App.vue init.');
    }
    return workbenchBridge;
  }
  function announce(message) {
    if (workbenchBridge && typeof workbenchBridge.announce === 'function') {
      return workbenchBridge.announce(message);
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
  const completedTransfers = computed(() => transferQueue.value.filter(item => item.status === 'done' || item.status === 'error'));

  const filteredRemoteEntries = computed(() => {
    const q = remoteFilter.value.trim().toLowerCase();
    if (!q) return remoteEntries.value;
    return remoteEntries.value.filter(e => e.name.toLowerCase().includes(q));
  });
  const sortedRemoteEntries = computed(() => {
    const key = remoteSortKey.value;
    const dir = remoteSortDir.value === 'asc' ? 1 : -1;
    const cmp = (a, b) => {
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
    };
    return [...filteredRemoteEntries.value].sort(cmp);
  });
  const selectedRemoteEntries = computed(() =>
    remoteEntries.value.filter(e => selectedRemotePaths.value.has(e.path))
  );
  const remoteBreadcrumb = computed(() => {
    const segments = remotePath.value.split('/').filter(Boolean);
    const crumbs = [{ name: '/', path: '/' }];
    let acc = '';
    for (const seg of segments) {
      acc += '/' + seg;
      crumbs.push({ name: seg, path: acc });
    }
    return crumbs;
  });

  // ============================================================
  // 传输进度事件监听（原 workbench.js:177-187）
  // files store 自管 progressUnlisten，workbench.setupEventListeners 不再处理。
  // ============================================================
  async function setupEventListeners() {
    if (!isTauriRuntime()) return;
    if (!progressUnlisten) {
      progressUnlisten = await listenBackendEvent(TRANSFER_PROGRESS_EVENT, event => {
        const { transfer_id, bytes_transferred, total_bytes } = event.payload || {};
        updateTransferProgress(transfer_id, bytes_transferred, total_bytes);
      });
    }
  }

  async function disposeEventListeners() {
    if (typeof progressUnlisten === 'function') {
      await progressUnlisten();
      progressUnlisten = null;
    }
  }

  function updateTransferProgress(transferId, transferred, total) {
    const item = transferQueue.value.find(entry => entry.id === transferId);
    if (!item) return;
    item.transferred = transferred;
    item.total = total;
    item.percent = total > 0 ? Math.min(100, Math.round((transferred / total) * 100)) : 0;
    if (transferred >= total && total > 0) {
      item.status = 'done';
      item.finishedAt = Date.now();
      pruneFinishedTransfers();
    }
  }

  function pruneFinishedTransfers() {
    const cutoff = Date.now() - 60_000;
    transferQueue.value = transferQueue.value.filter(item => {
      if (item.status === 'running' || item.status === 'pending') return true;
      return (item.finishedAt || 0) > cutoff;
    });
  }

  function markTransferError(transferId, message) {
    const item = transferQueue.value.find(entry => entry.id === transferId);
    if (!item) return;
    item.status = 'error';
    item.error = message;
    item.finishedAt = Date.now();
  }

  function triggerBrowserDownload(name, bytes) {
    if (typeof document === 'undefined') return;
    const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    const blob = new Blob([u8], { type: 'application/octet-stream' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = name;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  }

  // ============================================================
  // Sessions lazy 解析（避免循环 import）
  // ============================================================
  function getActiveSession() {
    const sessionsStore = wb().sessionsStore();
    const active = sessionsStore.activeSession;
    if (active) return active;
    const asset = wb().selectedAsset;
    if (!asset) return null;
    return sessionsStore.sessions.find(item => item.asset.id === asset.id) || null;
  }

  // ============================================================
  // Remote SFTP 浏览 / 操作
  // ============================================================
  async function refreshRemoteFiles(path = null) {
    const asset = wb().selectedAsset;
    if (!asset) return;
    const targetPath = path || remotePathForAsset(asset);
    const sessionsStore = wb().sessionsStore();
    const activeSession = sessionsStore.sessions.find(session => session.asset.id === asset.id);
    if (activeSession) {
      const result = await invokeBackend('sftp_list_dir', { sessionId: activeSession.sessionId, path: targetPath });
      applyRemoteListing(targetPath, result.entries || []);
      return;
    }
    const result = await invokeBackend('ssh_list_directory', {
      host: asset.host,
      port: asset.port,
      username: asset.username,
      password: '',
      credential_id: asset.credential_id || null,
      auth_method: asset.auth_method,
      private_key_path: asset.private_key_path,
      passphrase: null,
      passphrase_credential_id: asset.passphrase_credential_id || null,
      path: targetPath
    });
    applyRemoteListing(result.path || targetPath, Array.isArray(result.entries) ? result.entries : []);
    announce('远程文件已刷新：' + asset.name);
  }

  function applyRemoteListing(path, entries) {
    if (remotePath.value !== path) {
      remotePathHistory.value.push(remotePath.value);
    }
    remotePath.value = path;
    remoteEntries.value = entries;
  }

  async function navigateRemotePath(target) {
    if (!target) return;
    await refreshRemoteFiles(target).catch(error => announce('进入目录失败：' + error.message));
  }

  async function navigateRemoteUp() {
    const parent = parentPath(remotePath.value);
    await refreshRemoteFiles(parent).catch(error => announce('返回上级失败：' + error.message));
  }

  async function uploadFiles(fileList) {
    const session = getActiveSession();
    if (!session) {
      announce('上传需要先建立 SSH 会话');
      wb().setTab('terminal');
      await wb().sessionsStore().connectSelected();
      return;
    }
    const files = Array.from(fileList || []);
    if (!files.length) return;
    await Promise.allSettled(files.map(async file => {
      const transferId = 'up-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8);
      const remoteTarget = joinPath(remotePath.value, file.name);
      transferQueue.value.push({
        id: transferId,
        direction: 'upload',
        name: file.name,
        remotePath: remoteTarget,
        transferred: 0,
        total: file.size,
        percent: 0,
        status: 'running',
        startedAt: Date.now()
      });
      try {
        await invokeBackend('sftp_upload_start', {
          sessionId: session.sessionId,
          remotePath: remoteTarget,
          transferId
        });
        const CHUNK_SIZE = 8 * 1024 * 1024;
        let transferred = 0;
        while (transferred < file.size) {
          const end = Math.min(transferred + CHUNK_SIZE, file.size);
          const slice = file.slice(transferred, end);
          const chunk = new Uint8Array(await slice.arrayBuffer());
          await invokeBackend('sftp_upload_chunk', {
            sessionId: session.sessionId,
            chunk,
            transferId,
            bytesTransferred: end,
            totalBytes: file.size
          });
          transferred = end;
        }
        await invokeBackend('sftp_upload_finalize', { transferId });
        announce('已上传：' + file.name);
      } catch (error) {
        markTransferError(transferId, error.message);
        announce('上传失败：' + file.name + '：' + error.message);
        try {
          await invokeBackend('sftp_upload_finalize', { transferId });
        } catch {
          // best-effort cleanup
        }
      }
    }));
    await refreshRemoteFiles(remotePath.value).catch(() => null);
  }

  async function downloadEntry(entry) {
    const session = getActiveSession();
    if (!session) {
      announce('下载需要先建立 SSH 会话');
      return;
    }
    if (entry.kind === 'directory') {
      announce('暂不支持递归下载目录：' + entry.name);
      return;
    }
    const transferId = 'down-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8);
    transferQueue.value.push({
      id: transferId,
      direction: 'download',
      name: entry.name,
      remotePath: entry.path,
      transferred: 0,
      total: entry.size || 0,
      percent: 0,
      status: 'running',
      startedAt: Date.now()
    });
    try {
      const bytes = await invokeBackend('sftp_download_with_progress', {
        sessionId: session.sessionId,
        remotePath: entry.path,
        transferId
      });
      triggerBrowserDownload(entry.name, bytes);
      announce('已下载：' + entry.name);
    } catch (error) {
      markTransferError(transferId, error.message);
      announce('下载失败：' + error.message);
    }
  }

  async function mkdirRemote(name) {
    const session = getActiveSession();
    if (!session) return announce('需要先建立 SSH 会话');
    if (!name?.trim()) return announce('目录名不能为空');
    const target = joinPath(remotePath.value, name.trim());
    await invokeBackend('sftp_mkdir', { sessionId: session.sessionId, path: target });
    announce('已创建目录：' + name);
    await refreshRemoteFiles(remotePath.value);
  }

  async function renameRemote(entry, newName) {
    const session = getActiveSession();
    if (!session) return announce('需要先建立 SSH 会话');
    if (!newName?.trim()) return announce('新名称不能为空');
    const target = joinPath(parentPath(entry.path), newName.trim());
    await invokeBackend('sftp_rename', { sessionId: session.sessionId, oldPath: entry.path, newPath: target });
    announce('已重命名：' + entry.name + ' → ' + newName);
    await refreshRemoteFiles(remotePath.value);
  }

  async function removeRemote(entry) {
    const session = getActiveSession();
    if (!session) return announce('需要先建立 SSH 会话');
    await invokeBackend('sftp_remove', { sessionId: session.sessionId, path: entry.path, kind: entry.kind });
    announce('已删除：' + entry.name);
    await refreshRemoteFiles(remotePath.value);
  }

  async function statRemote(entry) {
    const session = getActiveSession();
    if (!session) return null;
    return invokeBackend('sftp_stat', { sessionId: session.sessionId, path: entry.path });
  }

  // ============================================================
  // Local fs_local_* 操作
  // ============================================================
  async function refreshLocalFiles(path = null) {
    if (!isTauriRuntime()) {
      announce('本地浏览需要桌面客户端（npm run tauri:dev）');
      return;
    }
    try {
      const target = path !== null ? path : (localPath.value || await invokeBackend('fs_local_home_dir'));
      const result = await invokeBackend('fs_local_list_dir', { path: target });
      localPath.value = result.path;
      localEntries.value = result.entries || [];
    } catch (error) {
      announce('本地目录读取失败：' + error.message);
    }
  }

  async function navigateLocalPath(target) {
    if (!target) return;
    await refreshLocalFiles(target);
  }

  async function navigateLocalUp() {
    if (!localPath.value) return;
    try {
      const result = await invokeBackend('fs_local_list_dir', { path: localPath.value });
      if (!result.parent || result.parent === localPath.value) {
        announce('已是根目录');
        return;
      }
      await refreshLocalFiles(result.parent);
    } catch (error) {
      announce('返回上级失败：' + error.message);
    }
  }

  async function localMkdir(name) {
    if (!name?.trim()) return announce('目录名不能为空');
    const target = joinLocalPath(localPath.value, name.trim());
    await invokeBackend('fs_local_mkdir', { path: target });
    announce('已创建本地目录：' + name);
    await refreshLocalFiles(localPath.value);
  }

  async function localDelete(paths) {
    if (!paths?.length) return;
    for (const path of paths) {
      const entry = localEntries.value.find(e => e.path === path);
      if (!entry) continue;
      await invokeBackend('fs_local_delete', { path, kind: entry.kind });
    }
    announce('已删除本地 ' + paths.length + ' 项');
    await refreshLocalFiles(localPath.value);
  }

  async function localRename(oldPath, newName) {
    if (!newName?.trim()) return announce('新名称不能为空');
    const newPath = joinLocalPath(parentLocalPath(oldPath), newName.trim());
    await invokeBackend('fs_local_rename', { oldPath, newPath });
    announce('已重命名：' + newName);
    await refreshLocalFiles(localPath.value);
  }

  function setLocalViewMode(mode) {
    if (mode !== 'queue' && mode !== 'browser') return;
    localViewMode.value = mode;
    if (mode === 'browser' && !localEntries.value.length) refreshLocalFiles().catch(() => null);
  }

  // ============================================================
  // Remote 多选 / 排序 / 过滤
  // ============================================================
  function toggleRemoteSelection(path, { additive = false, range = false } = {}) {
    const list = sortedRemoteEntries.value;
    const idx = list.findIndex(e => e.path === path);
    if (range && lastSelectedRemoteIndex.value >= 0 && idx >= 0) {
      const [start, end] = [lastSelectedRemoteIndex.value, idx].sort((a, b) => a - b);
      const next = new Set(selectedRemotePaths.value);
      for (let i = start; i <= end; i++) next.add(list[i].path);
      selectedRemotePaths.value = next;
      return;
    }
    const next = new Set(additive ? selectedRemotePaths.value : []);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    selectedRemotePaths.value = next;
    lastSelectedRemoteIndex.value = idx;
  }

  function selectAllRemote() {
    selectedRemotePaths.value = new Set(sortedRemoteEntries.value.map(e => e.path));
  }

  function clearRemoteSelection() {
    if (selectedRemotePaths.value.size) selectedRemotePaths.value = new Set();
    lastSelectedRemoteIndex.value = -1;
  }

  function toggleLocalSelection(path, { additive = false } = {}) {
    const next = new Set(additive ? selectedLocalPaths.value : []);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    selectedLocalPaths.value = next;
  }

  function selectAllLocal() {
    selectedLocalPaths.value = new Set(localEntries.value.map(e => e.path));
  }

  function clearLocalSelection() {
    if (selectedLocalPaths.value.size) selectedLocalPaths.value = new Set();
  }

  function setRemoteSort(key) {
    if (remoteSortKey.value === key) {
      remoteSortDir.value = remoteSortDir.value === 'asc' ? 'desc' : 'asc';
    } else {
      remoteSortKey.value = key;
      remoteSortDir.value = 'asc';
    }
  }

  function setRemoteFilter(query) {
    remoteFilter.value = query;
  }

  function setRemoteListMode(mode) {
    if (mode === 'compact' || mode === 'detailed') remoteListMode.value = mode;
  }

  function setManualRemotePath(value) {
    manualRemotePathInput.value = value;
  }

  async function goToManualRemotePath() {
    const target = manualRemotePathInput.value.trim();
    if (!target) return;
    await navigateRemotePath(target);
  }

  // ============================================================
  // 右键菜单 / 批量操作
  // ============================================================
  function openContextMenu({ x, y, side, entry }) {
    contextMenu.value = { visible: true, x, y, side, entry };
  }
  function closeContextMenu() {
    contextMenu.value = { ...contextMenu.value, visible: false };
  }

  async function batchRemoteDelete() {
    const paths = Array.from(selectedRemotePaths.value);
    if (!paths.length) return;
    const session = getActiveSession();
    if (!session) return announce('需要先建立 SSH 会话');
    if (!window.confirm(`确认删除 ${paths.length} 项？`)) return;
    for (const path of paths) {
      const entry = remoteEntries.value.find(e => e.path === path);
      if (!entry) continue;
      try {
        await invokeBackend('sftp_remove', { sessionId: session.sessionId, path, kind: entry.kind });
      } catch (error) {
        announce('删除失败：' + entry.name + '：' + error.message);
      }
    }
    selectedRemotePaths.value = new Set();
    announce('批量删除完成');
    await refreshRemoteFiles(remotePath.value);
  }

  async function batchRemoteDownload() {
    const paths = Array.from(selectedRemotePaths.value);
    if (!paths.length) return;
    for (const path of paths) {
      const entry = remoteEntries.value.find(e => e.path === path);
      if (!entry || entry.kind !== 'file') continue;
      await downloadEntry(entry).catch(error => announce('下载失败：' + entry.name + '：' + error.message));
    }
  }

  async function copyRemotePath(entry) {
    const target = entry?.path || remotePath.value;
    try {
      await navigator.clipboard.writeText(target);
      announce('已复制路径：' + target);
    } catch {
      announce('剪贴板不可用');
    }
  }

  function toggleTransferDrawer() { transferDrawerOpen.value = !transferDrawerOpen.value; }

  // ============================================================
  // clearSelectionOnAssetSwitch — assets store selectAsset 时调用
  // ============================================================
  function clearSelection() {
    selectedRemotePaths.value = new Set();
    selectedLocalPaths.value = new Set();
    lastSelectedRemoteIndex.value = -1;
  }

  return {
    // state
    remotePath,
    remoteEntries,
    remotePathHistory,
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
    manualRemotePathInput,
    lastSelectedRemoteIndex,
    contextMenu,
    transferDrawerOpen,
    // computed
    activeTransfers,
    completedTransfers,
    filteredRemoteEntries,
    sortedRemoteEntries,
    selectedRemoteEntries,
    remoteBreadcrumb,
    // bridge
    attachWorkbench,
    // lifecycle
    setupEventListeners,
    disposeEventListeners,
    // transfer ops
    updateTransferProgress,
    pruneFinishedTransfers,
    markTransferError,
    triggerBrowserDownload,
    // remote sftp
    refreshRemoteFiles,
    applyRemoteListing,
    navigateRemotePath,
    navigateRemoteUp,
    uploadFiles,
    downloadEntry,
    mkdirRemote,
    renameRemote,
    removeRemote,
    statRemote,
    // local fs
    refreshLocalFiles,
    navigateLocalPath,
    navigateLocalUp,
    localMkdir,
    localDelete,
    localRename,
    setLocalViewMode,
    // selection / sort / filter / context menu
    toggleRemoteSelection,
    selectAllRemote,
    clearRemoteSelection,
    toggleLocalSelection,
    selectAllLocal,
    clearLocalSelection,
    setRemoteSort,
    setRemoteFilter,
    setRemoteListMode,
    setManualRemotePath,
    goToManualRemotePath,
    openContextMenu,
    closeContextMenu,
    batchRemoteDelete,
    batchRemoteDownload,
    copyRemotePath,
    toggleTransferDrawer,
    // helpers exposed to assets store
    clearSelection
  };
});
