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
import { buildTransferItem } from '../lib/transferUtils.js';

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
  const manualLocalPathInput = ref('');
  const lastSelectedRemoteIndex = ref(-1);
  const contextMenu = ref({ visible: false, x: 0, y: 0, side: 'remote', entry: null });
  // 传输队列抽屉默认收起：用户有传输时状态栏「传输」或文件区 trigger 可展开。
  const transferDrawerOpen = ref(false);
  // 本地/远程双栏：app.html 设计稿默认双栏（localPaneVisible=true）。
  // 用户可点 view-pills 的「仅远程」收起本地面板。
  const localPaneVisible = ref(true);
  const fileOperationStack = ref({ remote: [], local: [] });
  // 删除确认链：removeRemote / localDelete / batchRemoteDelete 只组装 pending 并弹
  // confirmFileDelete modal，真正删除在 confirmFileDelete()（用户点「删除」后）执行。
  const pendingFileDelete = ref(null); // { kind: 'remote'|'local', paths: string[], names: string[] }
  // 上传覆盖保护：同名目标存在时挂起上传循环，等 confirmFileOverwrite / cancelFileOverwrite resolve。
  const pendingFileOverwrite = ref(null); // { entry, remoteTarget, resolve }
  // 远程目录加载错误（列表空态显示「加载失败 + 重试」）。
  const remoteError = ref('');
  // 远程目录是否成功加载过（区分「尚未加载」与「该目录为空」）。
  const remoteLoaded = ref(false);
  // 本地列表模式独立于 remoteListMode（本地表头不再被远程 listMode 绑架）。
  const localListMode = ref('detailed');
  const lastSelectedLocalIndex = ref(-1);

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

  const filteredRemoteEntries = computed(() => {
    const q = remoteFilter.value.trim().toLowerCase();
    if (!q) return remoteEntries.value;
    return remoteEntries.value.filter(e => e.name.toLowerCase().includes(q));
  });
  const sortedRemoteEntries = computed(() => {
    const key = remoteSortKey.value;
    const dir = remoteSortDir.value === 'asc' ? 1 : -1;
    // 类型推断：目录→DIR / 符号链接→LNK / 普通文件→扩展名大写（无扩展名→FILE）。
    const typeOf = (e) => {
      if (e.kind === 'directory') return 'DIR';
      if (e.kind === 'symlink') return 'LNK';
      const dot = e.name.lastIndexOf('.');
      if (dot <= 0 || dot === e.name.length - 1) return 'FILE';
      return e.name.slice(dot + 1).toUpperCase();
    };
    // 用户:组 组合串用于排序。
    const ownerOf = (e) => [e.user || '', e.group || ''].join(':');
    const cmp = (a, b) => {
      const aDir = a.kind === 'directory' ? 0 : 1;
      const bDir = b.kind === 'directory' ? 0 : 1;
      if (aDir !== bDir) return aDir - bDir;
      let av, bv;
      if (key === 'size') { av = a.size || 0; bv = b.size || 0; }
      else if (key === 'modified') { av = Number(a.modified) || 0; bv = Number(b.modified) || 0; }
      else if (key === 'type') { av = typeOf(a); bv = typeOf(b); }
      else if (key === 'permissions') {
        // 权限按八进制数值排（缺权限当作 0）。
        av = a.permissions ? parseInt(a.permissions, 8) || 0 : 0;
        bv = b.permissions ? parseInt(b.permissions, 8) || 0 : 0;
      }
      else if (key === 'owner') { av = ownerOf(a); bv = ownerOf(b); }
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
    // 终态（done/error/cancelled）忽略迟到进度事件，避免状态回跳
    if (item.status !== 'running' && item.status !== 'pending') return;
    const now = Date.now();
    const deltaBytes = transferred - (item.transferred || 0);
    const deltaMs = now - (item._lastProgressAt || now);
    // 瞬时速度：本次增量/耗时（简单滑动）；无增量时保留上次速度，避免事件间隔抖动归零
    if (deltaMs > 0 && deltaBytes > 0) {
      item.speed = (deltaBytes / deltaMs) * 1000;
    }
    item._lastProgressAt = now;
    item.transferred = transferred;
    item.total = total;
    item.percent = total > 0 ? Math.min(100, Math.round((transferred / total) * 100)) : 0;
    item.eta = item.speed > 0 && total > transferred ? Math.round((total - transferred) / item.speed) : null;
    if (transferred >= total && total > 0) {
      item.status = 'done';
      item.finishedAt = Date.now();
      item.speed = 0;
      item.eta = null;
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
    item.speed = 0;
    item.eta = null;
    // 与完成路径一致：错误项同样在 60s 后从队列清理（避免残留堆积）
    pruneFinishedTransfers();
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

  function beginFileOperation(side, message) {
    const token = Symbol(side);
    fileOperationStack.value[side] = [
      ...fileOperationStack.value[side],
      { token, message }
    ];
    return () => {
      fileOperationStack.value[side] = fileOperationStack.value[side].filter(item => item.token !== token);
    };
  }

  async function withFileOperation(side, message, task) {
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
  async function withTransfer(itemId, task) {
    const item = transferQueue.value.find(entry => entry.id === itemId);
    if (item) item.status = 'running';
    return task();
  }

  /**
   * 取消上传：置 cancelled 标记，分块循环在下一块前检查并停止。
   * 下载不可取消——sftp_download_with_progress 是整块 invoke（后端无中断通道），
   * TransferDrawer 对下载行不渲染取消按钮（勿造假按钮）。
   */
  function cancelTransfer(id) {
    const item = transferQueue.value.find(entry => entry.id === id);
    if (!item) return;
    if (item.direction !== 'upload') return;
    if (item.status !== 'running' && item.status !== 'pending') return;
    item.cancelled = true;
    announce('正在取消上传：' + item.name, { level: 'warn' });
  }

  /**
   * 重试失败/取消的传输：复用队列项保存的原始参数（op），重新走上传/下载流程。
   * 上传重试直接续传语义（覆盖检查在上传入口已做过，此处不再弹窗）。
   */
  async function retryTransfer(id) {
    const item = transferQueue.value.find(entry => entry.id === id);
    if (!item) return;
    if (item.status !== 'error' && item.status !== 'cancelled') return;
    const session = getActiveSession();
    if (!session) {
      announce('重试需要先建立 SSH 会话', { level: 'warn' });
      return;
    }
    // 重置为运行态（复用原 id，进度事件按 id 续接）
    item.status = 'running';
    item.error = null;
    item.cancelled = false;
    item.transferred = 0;
    item.percent = 0;
    item.speed = 0;
    item.eta = null;
    item.startedAt = Date.now();
    item.finishedAt = null;
    try {
      if (item.direction === 'upload') {
        if (item.op?.kind === 'localEntry') {
          await runLocalEntryUpload(item, session);
        } else if (item.op?.kind === 'file') {
          await runFileUpload(item, session);
        } else {
          // op 缺失（异常数据/旧队列项）：无法重试
          markTransferError(id, '传输参数缺失');
          announce('传输参数缺失，无法重试：' + item.name, { level: 'warn' });
        }
      } else {
        await runDownload(item, session);
      }
    } catch (error) {
      markTransferError(id, error.message);
      announce('传输失败：' + item.name + '：' + error.message, {
        level: 'error',
        action: { label: '重试', run: () => retryTransfer(id) }
      });
    }
  }

  // ============================================================
  // Sessions lazy 解析（避免循环 import）
  // ============================================================
  // 解析指定资产当前应使用的会话：优先活跃会话（若同资产），否则回退首个匹配会话。
  // 同资产多会话场景下，文件浏览跟随当前聚焦的 tab。
  function resolveSessionForAsset(asset) {
    const sessionsStore = wb().sessionsStore();
    const active = sessionsStore.activeSession;
    if (active && active.asset?.id === asset.id) return active;
    return sessionsStore.sessions.find(item => item.asset.id === asset.id) || null;
  }
  function getActiveSession() {
    const sessionsStore = wb().sessionsStore();
    const active = sessionsStore.activeSession;
    if (active) return active;
    const asset = wb().selectedAsset;
    if (!asset) return null;
    return resolveSessionForAsset(asset);
  }

  // ============================================================
  // Remote SFTP 浏览 / 操作
  // ============================================================
  async function refreshRemoteFiles(path = null) {
    try {
      await withFileOperation('remote', '正在读取远程目录...', async () => {
        const asset = wb().selectedAsset;
        if (!asset) return;
        const targetPath = path || remotePathForAsset(asset);
        const activeSession = resolveSessionForAsset(asset);
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
      });
      remoteError.value = '';
    } catch (error) {
      // 失败：写入 remoteError（列表空态显示「加载失败 + 重试」），announce 升级 error
      remoteError.value = error.message || String(error);
      announce('远程目录加载失败：' + error.message, { level: 'error' });
    }
  }

  function applyRemoteListing(path, entries) {
    if (remotePath.value !== path) {
      remotePathHistory.value.push(remotePath.value);
    }
    remotePath.value = path;
    remoteEntries.value = entries;
    remoteLoaded.value = true;
  }

  async function navigateRemotePath(target) {
    if (!target) return;
    await refreshRemoteFiles(target).catch(error => announce('进入目录失败：' + error.message));
  }

  async function navigateRemoteUp() {
    const parent = parentPath(remotePath.value);
    await refreshRemoteFiles(parent).catch(error => announce('返回上级失败：' + error.message));
  }

  // ------------------------------------------------------------
  // 上传/下载传输（S2：覆盖保护 + 取消/重试 + 速度 ETA + busy 解耦）
  // ------------------------------------------------------------

  /** sftp_stat 路径版：上传覆盖检查复用（statRemote 的「无消费」状态由此消除）。 */
  async function statRemotePath(path) {
    const session = getActiveSession();
    if (!session) return null;
    return invokeBackend('sftp_stat', { sessionId: session.sessionId, path });
  }

  async function statRemote(entry) {
    if (!entry?.path) return null;
    return statRemotePath(entry.path);
  }

  /** 目标路径是否已存在（stat 成功 = 存在；失败/无会话 = 不存在，视为可覆盖）。 */
  async function remoteTargetExists(remoteTarget) {
    try {
      const result = await statRemotePath(remoteTarget);
      return !!result;
    } catch {
      // stat 失败 = 目标不存在（或后端不支持），可覆盖
      return false;
    }
  }

  /**
   * 上传前同名确认：挂起上传循环直到用户选择。
   * resolve(true) 继续上传；resolve(false) 跳过该文件（不中断整批）。
   */
  function askFileOverwrite(entry, remoteTarget) {
    return new Promise(resolve => {
      pendingFileOverwrite.value = { entry, remoteTarget, resolve };
      wb().modal = { type: 'confirmFileOverwrite', payload: { path: remoteTarget } };
    }).then(choice => {
      pendingFileOverwrite.value = null;
      return choice;
    });
  }

  function confirmFileOverwrite() {
    const pending = pendingFileOverwrite.value;
    if (pending) pending.resolve(true);
    pendingFileOverwrite.value = null;
    wb().modal = { type: null };
  }

  function cancelFileOverwrite() {
    const pending = pendingFileOverwrite.value;
    if (pending) {
      pending.resolve(false);
      announce('已跳过同名文件' + (pending.entry?.name ? '：' + pending.entry.name : ''), { level: 'warn' });
    }
    pendingFileOverwrite.value = null;
    wb().modal = { type: null };
  }

  /** 上传浏览器 File（input/drag-drop）。顺序处理：每个文件先做覆盖检查，取消则跳过继续下一批。 */
  async function uploadFiles(fileList) {
    const session = getActiveSession();
    if (!session) {
      announce('上传需要先建立 SSH 会话', { level: 'warn' });
      wb().setTab('terminal');
      await wb().sessionsStore().connectSelected();
      return;
    }
    const files = Array.from(fileList || []);
    if (!files.length) return;
    for (const file of files) {
      const remoteTarget = joinPath(remotePath.value, file.name);
      if (await remoteTargetExists(remoteTarget)) {
        if (!(await askFileOverwrite(file, remoteTarget))) continue;
      }
      const transferId = 'up-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8);
      const item = buildTransferItem({
        id: transferId,
        direction: 'upload',
        name: file.name,
        remotePath: remoteTarget,
        total: file.size,
        op: { kind: 'file', file, remoteTarget }
      });
      transferQueue.value.push(item);
      await withTransfer(transferId, () => runFileUpload(item, session));
    }
    await refreshRemoteFiles(remotePath.value).catch(() => null);
  }

  /** 分块上传执行体（uploadFiles 与 retryTransfer 共用）。 */
  async function runFileUpload(item, session) {
    const transferId = item.id;
    try {
      await invokeBackend('sftp_upload_start', {
        sessionId: session.sessionId,
        remotePath: item.remotePath,
        transferId
      });
      const CHUNK_SIZE = 8 * 1024 * 1024;
      let transferred = 0;
      while (transferred < item.total) {
        if (item.cancelled) break; // cancelTransfer 标记 → 停在当前 chunk 边界
        const end = Math.min(transferred + CHUNK_SIZE, item.total);
        const slice = item.op.file.slice(transferred, end);
        const chunk = new Uint8Array(await slice.arrayBuffer());
        await invokeBackend('sftp_upload_chunk', {
          sessionId: session.sessionId,
          chunk,
          transferId,
          bytesTransferred: end,
          totalBytes: item.total
        });
        transferred = end;
      }
      if (item.cancelled) {
        item.status = 'cancelled';
        item.finishedAt = Date.now();
        item.speed = 0;
        item.eta = null;
        pruneFinishedTransfers();
        announce('已取消上传：' + item.name, { level: 'warn' });
        // 后端无 abort 通道：finalize 兜底释放分块会话（可能落盘部分数据，best-effort）
        try {
          await invokeBackend('sftp_upload_finalize', { transferId });
        } catch {
          // best-effort cleanup
        }
        return;
      }
      await invokeBackend('sftp_upload_finalize', { transferId });
      item.status = 'done';
      item.percent = 100;
      item.transferred = item.total;
      item.eta = null;
      item.finishedAt = Date.now();
      pruneFinishedTransfers();
      announce('已上传：' + item.name, { level: 'success' });
    } catch (error) {
      markTransferError(transferId, error.message);
      announce('上传失败：' + item.name + '：' + error.message, {
        level: 'error',
        action: { label: '重试', run: () => retryTransfer(transferId) }
      });
      try {
        await invokeBackend('sftp_upload_finalize', { transferId });
      } catch {
        // best-effort cleanup
      }
    }
  }

  /** 上传本地条目（FileColumn 双击本地文件 / 右键「上传到远程」）。 */
  async function uploadLocalEntry(entry) {
    const session = getActiveSession();
    if (!session) {
      announce('上传需要先建立 SSH 会话', { level: 'warn' });
      wb().setTab('terminal');
      return;
    }
    if (!entry || entry.kind !== 'file') {
      announce('只能上传本地文件', { level: 'warn' });
      return;
    }
    const remoteTarget = joinPath(remotePath.value, entry.name);
    if (await remoteTargetExists(remoteTarget)) {
      if (!(await askFileOverwrite(entry, remoteTarget))) return;
    }
    const transferId = 'up-local-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8);
    const item = buildTransferItem({
      id: transferId,
      direction: 'upload',
      name: entry.name,
      remotePath: remoteTarget,
      total: Number(entry.size) || 0,
      op: { kind: 'localEntry', entry, remoteTarget }
    });
    transferQueue.value.push(item);
    await withTransfer(transferId, () => runLocalEntryUpload(item, session));
  }

  /** 本地条目分块上传执行体（uploadLocalEntry 与 retryTransfer 共用）。 */
  async function runLocalEntryUpload(item, session) {
    const transferId = item.id;
    try {
      await invokeBackend('sftp_upload_start', {
        sessionId: session.sessionId,
        remotePath: item.remotePath,
        transferId
      });
      const chunkSize = 8 * 1024 * 1024;
      let offset = 0;
      while (offset < item.total || (item.total === 0 && offset === 0)) {
        if (item.cancelled) break;
        const chunk = await invokeBackend('fs_local_read_chunk', {
          path: item.op.entry.path,
          offset,
          length: chunkSize
        });
        const bytes = Array.isArray(chunk) ? chunk : Array.from(chunk || []);
        if (!bytes.length && item.total > 0) break;
        offset += bytes.length;
        await invokeBackend('sftp_upload_chunk', {
          sessionId: session.sessionId,
          chunk: bytes,
          transferId,
          bytesTransferred: offset,
          totalBytes: item.total
        });
        if (item.total === 0 || bytes.length < chunkSize) break;
      }
      if (item.cancelled) {
        item.status = 'cancelled';
        item.finishedAt = Date.now();
        item.speed = 0;
        item.eta = null;
        pruneFinishedTransfers();
        announce('已取消上传：' + item.name, { level: 'warn' });
        try {
          await invokeBackend('sftp_upload_finalize', { transferId });
        } catch {
          // best-effort cleanup
        }
        return;
      }
      await invokeBackend('sftp_upload_finalize', { transferId });
      item.status = 'done';
      item.percent = 100;
      item.transferred = item.total;
      item.eta = null;
      item.finishedAt = Date.now();
      pruneFinishedTransfers();
      announce('已上传本地文件：' + item.name, { level: 'success' });
      await refreshRemoteFiles(remotePath.value).catch(() => null);
    } catch (error) {
      markTransferError(transferId, error.message);
      announce('上传本地文件失败：' + item.name + '：' + error.message, {
        level: 'error',
        action: { label: '重试', run: () => retryTransfer(transferId) }
      });
      try {
        await invokeBackend('sftp_upload_finalize', { transferId });
      } catch {
        // best-effort cleanup
      }
    }
  }

  async function downloadEntry(entry) {
    const session = getActiveSession();
    if (!session) {
      announce('下载需要先建立 SSH 会话', { level: 'warn' });
      return;
    }
    if (entry.kind === 'directory') {
      announce('暂不支持递归下载目录：' + entry.name, { level: 'warn' });
      return;
    }
    const transferId = 'down-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8);
    const item = buildTransferItem({
      id: transferId,
      direction: 'download',
      name: entry.name,
      remotePath: entry.path,
      total: entry.size || 0,
      op: { kind: 'download', entry }
    });
    transferQueue.value.push(item);
    await withTransfer(transferId, () => runDownload(item, session));
  }

  /** 下载执行体（downloadEntry 与 retryTransfer 共用）。整块 invoke，不可中断。 */
  async function runDownload(item, session) {
    const transferId = item.id;
    try {
      const bytes = await invokeBackend('sftp_download_with_progress', {
        sessionId: session.sessionId,
        remotePath: item.remotePath,
        transferId
      });
      // 进度事件通常已把 item 置 done；invoke 成功但事件缺失时兜底
      const entry = transferQueue.value.find(e => e.id === transferId);
      if (entry && entry.status === 'running') {
        entry.status = 'done';
        entry.percent = 100;
        entry.transferred = entry.total || bytes.length;
        entry.speed = 0;
        entry.eta = null;
        entry.finishedAt = Date.now();
        pruneFinishedTransfers();
      }
      triggerBrowserDownload(item.name, bytes);
      announce('已下载：' + item.name, { level: 'success' });
    } catch (error) {
      markTransferError(transferId, error.message);
      announce('下载失败：' + item.name + '：' + error.message, {
        level: 'error',
        action: { label: '重试', run: () => retryTransfer(transferId) }
      });
    }
  }

  async function mkdirRemote(name) {
    const session = getActiveSession();
    if (!session) {
      announce('需要先建立 SSH 会话', { level: 'warn' });
      return;
    }
    if (!name?.trim()) {
      announce('目录名不能为空', { level: 'warn' });
      return;
    }
    try {
      await withFileOperation('remote', '正在创建远程目录...', async () => {
        const target = joinPath(remotePath.value, name.trim());
        await invokeBackend('sftp_mkdir', { sessionId: session.sessionId, path: target });
        await refreshRemoteFiles(remotePath.value);
      });
      announce('已创建目录：' + name.trim(), { level: 'success' });
    } catch (error) {
      announce('创建目录失败：' + error.message, { level: 'error' });
      throw error; // 抛给 GlobalModals submitModal：失败保持弹窗打开，可修正重试
    }
  }

  async function renameRemote(entry, newName) {
    const session = getActiveSession();
    if (!session) {
      announce('需要先建立 SSH 会话', { level: 'warn' });
      return;
    }
    if (!newName?.trim()) {
      announce('新名称不能为空', { level: 'warn' });
      return;
    }
    try {
      await withFileOperation('remote', '正在重命名远程文件...', async () => {
        const target = joinPath(parentPath(entry.path), newName.trim());
        await invokeBackend('sftp_rename', { sessionId: session.sessionId, oldPath: entry.path, newPath: target });
        await refreshRemoteFiles(remotePath.value);
      });
      announce('已重命名：' + entry.name + ' → ' + newName.trim(), { level: 'success' });
    } catch (error) {
      announce('重命名失败：' + error.message, { level: 'error' });
      throw error;
    }
  }

  // ------------------------------------------------------------
  // 删除确认链（统一走 GlobalModals，禁止 window.confirm）
  // removeRemote / localDelete / batchRemoteDelete 只组装 pendingFileDelete
  // 并弹 confirmFileDelete modal；confirmFileDelete 执行真正删除。
  // ------------------------------------------------------------
  function openFileDeleteConfirm(kind, paths, names) {
    pendingFileDelete.value = { kind, paths, names };
    wb().modal = { type: 'confirmFileDelete' };
  }

  function removeRemote(entry) {
    const session = getActiveSession();
    if (!session) {
      announce('需要先建立 SSH 会话', { level: 'warn' });
      return;
    }
    openFileDeleteConfirm('remote', [entry.path], [entry.name]);
  }

  function localDelete(paths) {
    if (!paths?.length) return;
    const names = paths.map(p => localEntries.value.find(e => e.path === p)?.name || p);
    openFileDeleteConfirm('local', [...paths], names);
  }

  function batchRemoteDelete() {
    const paths = Array.from(selectedRemotePaths.value);
    if (!paths.length) return;
    const session = getActiveSession();
    if (!session) {
      announce('需要先建立 SSH 会话', { level: 'warn' });
      return;
    }
    const names = paths.map(p => remoteEntries.value.find(e => e.path === p)?.name || p);
    openFileDeleteConfirm('remote', paths, names);
  }

  async function confirmFileDelete() {
    const pending = pendingFileDelete.value;
    if (!pending) return;
    const { kind, paths } = pending;
    try {
      if (kind === 'remote') {
        const session = getActiveSession();
        if (!session) {
          announce('需要先建立 SSH 会话', { level: 'warn' });
          return;
        }
        await withFileOperation('remote', '正在删除远程文件...', async () => {
          for (const path of paths) {
            const entry = remoteEntries.value.find(e => e.path === path);
            if (!entry) continue;
            await invokeBackend('sftp_remove', { sessionId: session.sessionId, path, kind: entry.kind });
          }
          if (paths.length > 1) selectedRemotePaths.value = new Set();
          await refreshRemoteFiles(remotePath.value);
        });
        announce('已删除远程 ' + paths.length + ' 项', { level: 'success' });
      } else {
        await withFileOperation('local', '正在删除本地文件...', async () => {
          for (const path of paths) {
            const entry = localEntries.value.find(e => e.path === path);
            if (!entry) continue;
            await invokeBackend('fs_local_delete', { path, kind: entry.kind });
          }
          await refreshLocalFiles(localPath.value);
        });
        announce('已删除本地 ' + paths.length + ' 项', { level: 'success' });
      }
      pendingFileDelete.value = null;
      wb().modal = { type: null };
    } catch (error) {
      // 删除失败：保留 pending 与弹窗，用户可直接重试（不做静默失败）
      announce('删除失败：' + error.message, { level: 'error' });
    }
  }

  function cancelFileDelete() {
    pendingFileDelete.value = null;
    wb().modal = { type: null };
  }

  // ============================================================
  // Local fs_local_* 操作
  // ============================================================
  async function refreshLocalFiles(path = null) {
    if (!isTauriRuntime()) {
      announce('本地浏览需要桌面客户端（npm run tauri:dev）', { level: 'warn' });
      return;
    }
    try {
      await withFileOperation('local', '正在读取本地目录...', async () => {
        const target = path !== null ? path : (localPath.value || await invokeBackend('fs_local_home_dir'));
        const result = await invokeBackend('fs_local_list_dir', { path: target });
        localPath.value = result.path;
        localEntries.value = result.entries || [];
      });
    } catch (error) {
      announce('本地目录读取失败：' + error.message, { level: 'error' });
    }
  }

  async function navigateLocalPath(target) {
    if (!target) return;
    await refreshLocalFiles(target);
  }

  async function navigateLocalUp() {
    if (!localPath.value) return;
    try {
      const result = await withFileOperation('local', '正在读取本地目录...', () =>
        invokeBackend('fs_local_list_dir', { path: localPath.value })
      );
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
    if (!name?.trim()) {
      announce('目录名不能为空', { level: 'warn' });
      return;
    }
    try {
      await withFileOperation('local', '正在创建本地目录...', async () => {
        const target = joinLocalPath(localPath.value, name.trim());
        await invokeBackend('fs_local_mkdir', { path: target });
        await refreshLocalFiles(localPath.value);
      });
      announce('已创建本地目录：' + name.trim(), { level: 'success' });
    } catch (error) {
      announce('创建本地目录失败：' + error.message, { level: 'error' });
      throw error;
    }
  }

  async function localRename(oldPath, newName) {
    if (!newName?.trim()) {
      announce('新名称不能为空', { level: 'warn' });
      return;
    }
    try {
      await withFileOperation('local', '正在重命名本地文件...', async () => {
        const newPath = joinLocalPath(parentLocalPath(oldPath), newName.trim());
        await invokeBackend('fs_local_rename', { oldPath, newPath });
        await refreshLocalFiles(localPath.value);
      });
      announce('已重命名：' + newName.trim(), { level: 'success' });
    } catch (error) {
      announce('重命名失败：' + error.message, { level: 'error' });
      throw error;
    }
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

  function toggleLocalSelection(path, { additive = false, range = false } = {}) {
    const list = localEntries.value;
    const idx = list.findIndex(e => e.path === path);
    if (range && lastSelectedLocalIndex.value >= 0 && idx >= 0) {
      // shift 范围多选：与远程 range 逻辑同构（参照 toggleRemoteSelection）
      const [start, end] = [lastSelectedLocalIndex.value, idx].sort((a, b) => a - b);
      const next = new Set(selectedLocalPaths.value);
      for (let i = start; i <= end; i++) next.add(list[i].path);
      selectedLocalPaths.value = next;
      return;
    }
    const next = new Set(additive ? selectedLocalPaths.value : []);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    selectedLocalPaths.value = next;
    lastSelectedLocalIndex.value = idx;
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

  function setLocalListMode(mode) {
    if (mode === 'compact' || mode === 'detailed') localListMode.value = mode;
  }

  function setManualRemotePath(value) {
    manualRemotePathInput.value = value;
  }

  async function goToManualRemotePath() {
    const target = manualRemotePathInput.value.trim();
    if (!target) return;
    await navigateRemotePath(target);
  }

  function setManualLocalPath(value) {
    manualLocalPathInput.value = value;
  }

  async function goToManualLocalPath() {
    const target = manualLocalPathInput.value.trim();
    if (!target) return;
    await navigateLocalPath(target);
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

  async function batchRemoteDownload() {
    const paths = Array.from(selectedRemotePaths.value);
    if (!paths.length) return;
    // 下载不走 withFileOperation：传输 busy 与浏览 busy 解耦，不锁列表
    for (const path of paths) {
      const entry = remoteEntries.value.find(e => e.path === path);
      if (!entry || entry.kind !== 'file') continue;
      await downloadEntry(entry).catch(error =>
        announce('下载失败：' + entry.name + '：' + error.message, { level: 'error' })
      );
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
  function toggleLocalPane() { localPaneVisible.value = !localPaneVisible.value; }
  function setLocalPaneVisible(value) { localPaneVisible.value = !!value; }

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
    triggerBrowserDownload,
    cancelTransfer,
    retryTransfer,
    // remote sftp
    refreshRemoteFiles,
    applyRemoteListing,
    navigateRemotePath,
    navigateRemoteUp,
    uploadFiles,
    uploadLocalEntry,
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
