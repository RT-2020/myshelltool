import { defineStore } from 'pinia';
import { computed, markRaw, ref, watch } from 'vue';
import { invokeBackend, isTauriRuntime, listenBackendEvent, normalizeAsset, normalizeTunnelConfig, normalizeTunnelStatus, slugify } from '../services/backend.js';

const THEME_STORAGE_KEY = 'myshelltool-theme';
const THEME_ORDER = ['system', 'light', 'dark'];
const THEME_LABELS = { system: '跟随系统', light: '浅色', dark: '深色' };

const TRANSFER_PROGRESS_EVENT = 'sftp-transfer-progress';
const HOST_KEY_VERIFY_EVENT = 'ssh-host-key-verify';
const KEYBOARD_INTERACTIVE_EVENT = 'ssh-keyboard-interactive';

export const useWorkbenchStore = defineStore('workbench', () => {
  const backendStatus = ref({ ready: false, mode: 'loading' });
  const assetSource = ref({ source: 'loading', count: 0 });
  const assets = ref([]);
  const selectedAssetId = ref(null);
  const activeTab = ref('overview');
  const theme = ref(normalizeStoredTheme(readStored('myshelltool-theme')));
  const systemPrefersDark = ref(readSystemPrefersDark());
  const effectiveTheme = computed(() => {
    if (theme.value === 'system') return systemPrefersDark.value ? 'dark' : 'light';
    return theme.value;
  });
  const themeLabel = computed(() => THEME_LABELS[theme.value] || theme.value);
  let systemThemeUnlisten = null;
  const assetsCollapsed = ref(readStored('myshelltool-assets') === 'collapsed');
  const statusMessage = ref('就绪：连接资产可收起，双击主机打开 SSH 会话。');
  const remotePath = ref('/srv/app/releases');
  const remoteEntries = ref([]);
  const remotePathHistory = ref([]);
  const transferQueue = ref([]);
  const tunnels = ref([]);
  const sessions = ref([]);
  const activeSessionId = ref(null);
  const githubPatConfigured = ref(false);
  const modal = ref({ type: null, asset: null });
  const hostKeyPrompt = ref(null);
  const keyboardPrompt = ref(null);
  const searchState = ref({ open: false, query: '', suggestions: [] });
  let terminalContainer = null;
  let terminalModules = null;
  let progressUnlisten = null;
  let hostKeyUnlisten = null;
  let keyboardUnlisten = null;
  let resizeObserver = null;
  let hostKeyTimeout = null;

  async function ensureHostKeyListeners() {
    if (typeof window === 'undefined') return;
    try {
      if (!hostKeyUnlisten) {
        hostKeyUnlisten = await listenBackendEvent(HOST_KEY_VERIFY_EVENT, event => {
          hostKeyPrompt.value = event.payload;
          modal.value = { type: 'hostKeyVerify', asset: selectedAsset.value };
        });
      }
      if (!keyboardUnlisten) {
        keyboardUnlisten = await listenBackendEvent(KEYBOARD_INTERACTIVE_EVENT, event => {
          keyboardPrompt.value = event.payload;
          modal.value = { type: 'keyboardInteractive', asset: selectedAsset.value };
        });
      }
    } catch (error) {
      console.warn('host key / keyboard listener registration deferred:', error.message);
    }
  }

  // 立即注册 host key / keyboard 监听器（不等 initialize 调用）
  if (typeof window !== 'undefined') {
    ensureHostKeyListeners();
  }

  // hostKeyPrompt 65s 自动清理（与后端 60s 超时对齐 + 5s 缓冲）
  watch(hostKeyPrompt, prompt => {
    if (hostKeyTimeout) {
      clearTimeout(hostKeyTimeout);
      hostKeyTimeout = null;
    }
    if (prompt) {
      hostKeyTimeout = setTimeout(() => {
        if (hostKeyPrompt.value) {
          hostKeyPrompt.value = null;
          modal.value = { type: null, asset: null };
          announce('主机密钥验证超时（65秒未响应），请重新连接');
        }
      }, 65000);
    }
  });

  const selectedAsset = computed(() => assets.value.find(asset => asset.id === selectedAssetId.value) || assets.value[0] || null);
  const groupedAssets = computed(() => {
    const groups = new Map();
    for (const asset of assets.value) {
      const group = asset.group || '未分组';
      if (!groups.has(group)) groups.set(group, []);
      groups.get(group).push(asset);
    }
    return [...groups.entries()].map(([name, items]) => ({ name, items }));
  });
  const activeSessions = computed(() => sessions.value.length);
  const activeSession = computed(() => sessions.value.find(session => session.sessionId === activeSessionId.value) || null);
  const runningTunnels = computed(() => tunnels.value.filter(tunnel => tunnel.active).length);
  const warningCount = computed(() => assets.value.filter(asset => normalizeStatus(asset.status).label === 'warning').length + tunnels.value.filter(tunnel => tunnel.error).length);
  const backendMode = computed(() => backendStatus.value?.mode || 'unknown');
  const assetSourceText = computed(() => `${assetSource.value.source || assetSource.value.mode || 'unknown'} · ${assetSource.value.count ?? assets.value.length} 项`);
  const backendStatusText = computed(() => backendStatus.value.ready ? `已连接 · ${backendMode.value}` : `未就绪 · ${backendMode.value}`);
  const syncText = computed(() => githubPatConfigured.value ? 'PAT 已配置' : 'PAT 未配置');
  const activeTransfers = computed(() => transferQueue.value.filter(item => item.status === 'running' || item.status === 'pending'));
  const completedTransfers = computed(() => transferQueue.value.filter(item => item.status === 'done' || item.status === 'error'));

  async function initialize() {
    applyTheme(effectiveTheme.value);
    startSystemThemeListener();
    watch(effectiveTheme, next => applyTheme(next));
    applyAssetsState(assetsCollapsed.value, false);
    try {
      const [status, assetResult, credential, tunnelResult] = await Promise.all([
        invokeBackend('backend_status'),
        invokeBackend('list_connection_assets'),
        invokeBackend('get_credential_status', { id: 'github-pat' }).catch(() => ({ exists: false })),
        invokeBackend('tunnel_list').catch(() => [])
      ]);
      backendStatus.value = status;
      assetSource.value = { ...assetResult, count: assetResult.count ?? assetResult.assets?.length ?? 0 };
      assets.value = (assetResult.assets || []).map(normalizeAsset);
      githubPatConfigured.value = Boolean(credential.exists);
      tunnels.value = (tunnelResult || []).map(normalizeTunnelStatus);
    } catch (error) {
      backendStatus.value = { ready: false, mode: 'fallback' };
      assetSource.value = { source: 'unavailable', count: 0 };
      assets.value = [];
      announce('后端桥接初始化失败：' + error.message);
    }
    if (!selectedAssetId.value && assets.value.length) selectedAssetId.value = assets.value[0].id;
    setupEventListeners().catch(error => {
      announce('后端事件监听初始化失败：' + error.message);
    });
  }

  async function setupEventListeners() {
    if (!isTauriRuntime()) return;

    if (!progressUnlisten) {
      progressUnlisten = await listenBackendEvent(TRANSFER_PROGRESS_EVENT, event => {
        const { transfer_id, bytes_transferred, total_bytes } = event.payload || {};
        updateTransferProgress(transfer_id, bytes_transferred, total_bytes);
      });
    }
    if (!hostKeyUnlisten) {
      hostKeyUnlisten = await listenBackendEvent(HOST_KEY_VERIFY_EVENT, event => {
        hostKeyPrompt.value = event.payload;
        modal.value = { type: 'hostKeyVerify', asset: selectedAsset.value };
      });
    }
    if (!keyboardUnlisten) {
      keyboardUnlisten = await listenBackendEvent(KEYBOARD_INTERACTIVE_EVENT, event => {
        keyboardPrompt.value = event.payload;
        modal.value = { type: 'keyboardInteractive', asset: selectedAsset.value };
      });
    }
  }

  async function disposeEventListeners() {
    if (hostKeyTimeout) { clearTimeout(hostKeyTimeout); hostKeyTimeout = null; }
    if (typeof progressUnlisten === 'function') { await progressUnlisten(); progressUnlisten = null; }
    if (typeof hostKeyUnlisten === 'function') { await hostKeyUnlisten(); hostKeyUnlisten = null; }
    if (typeof keyboardUnlisten === 'function') { await keyboardUnlisten(); keyboardUnlisten = null; }
    if (typeof systemThemeUnlisten === 'function') { systemThemeUnlisten(); systemThemeUnlisten = null; }
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

  function announce(message) {
    statusMessage.value = message;
  }

  function selectAsset(id, announceSelection = true) {
    if (!assets.value.some(asset => asset.id === id)) return;
    selectedAssetId.value = id;
    if (announceSelection && selectedAsset.value) announce('已选择连接：' + selectedAsset.value.name);
  }

  function setTab(tab) {
    activeTab.value = tab;
    if (tab === 'files') refreshRemoteFiles().catch(error => announce('远程文件刷新失败：' + error.message));
    if (tab === 'tunnels') refreshTunnels();
    announce('已切换到 ' + tabLabel(tab));
  }

  function toggleTheme() {
    const current = THEME_ORDER.indexOf(theme.value);
    theme.value = THEME_ORDER[(current + 1) % THEME_ORDER.length];
    applyTheme(effectiveTheme.value);
    localStorage.setItem(THEME_STORAGE_KEY, theme.value);
    announce('主题已切换：' + (THEME_LABELS[theme.value] || theme.value));
  }

  function startSystemThemeListener() {
    if (systemThemeUnlisten || typeof window === 'undefined' || !window.matchMedia) return;
    const mql = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = event => { systemPrefersDark.value = event.matches; };
    if (typeof mql.addEventListener === 'function') {
      mql.addEventListener('change', handler);
      systemThemeUnlisten = () => mql.removeEventListener('change', handler);
    } else if (typeof mql.addListener === 'function') {
      mql.addListener(handler);
      systemThemeUnlisten = () => mql.removeListener(handler);
    }
  }

  function toggleAssets() {
    assetsCollapsed.value = !assetsCollapsed.value;
    applyAssetsState(assetsCollapsed.value, true);
    announce(assetsCollapsed.value ? '连接资产已收起，主工作区已扩展' : '连接资产已展开');
  }

  async function saveAsset(input, credentials = {}) {
    const previous = assets.value.find(asset => asset.id === input.id);
    const id = input.id || uniqueAssetId(input.name, input.host);
    const item = normalizeAsset({
      ...input,
      id,
      last_connected: previous?.last_connected || '从未'
    });

    if (credentials.password) {
      await invokeBackend('save_credential', { id: credentialIdFor(id, 'password'), secret: credentials.password });
      item.credential_id = credentialIdFor(id, 'password');
    } else if (previous?.credential_id) {
      item.credential_id = previous.credential_id;
    }
    if (credentials.passphrase) {
      await invokeBackend('save_credential', { id: credentialIdFor(id, 'passphrase'), secret: credentials.passphrase });
      item.passphrase_credential_id = credentialIdFor(id, 'passphrase');
    } else if (previous?.passphrase_credential_id) {
      item.passphrase_credential_id = previous.passphrase_credential_id;
    }

    const result = await invokeBackend('save_connection_asset', { asset: item });
    assets.value = (result.assets || []).map(normalizeAsset);
    assetSource.value = { ...result, count: result.count ?? result.assets?.length ?? assets.value.length };
    selectAsset(item.id, false);
    modal.value = { type: null, asset: null };
    announce('连接资产已保存：' + item.name);
  }

  function credentialIdFor(assetId, kind) {
    return `${assetId}:${kind}`;
  }

  async function saveToken(secret) {
    if (!secret.trim()) {
      announce('token 不能为空');
      return false;
    }
    await invokeBackend('save_credential', { id: 'github-pat', secret });
    const status = await invokeBackend('get_credential_status', { id: 'github-pat' });
    githubPatConfigured.value = Boolean(status.exists);
    announce('同步配置已保存，本地安全存储：' + (githubPatConfigured.value ? '已配置' : '未配置'));
    return true;
  }

  async function deleteToken() {
    await invokeBackend('delete_credential', { id: 'github-pat' });
    githubPatConfigured.value = false;
    announce('已清除本地安全存储中的 token');
  }

  async function refreshRemoteFiles(path = null) {
    const asset = selectedAsset.value;
    if (!asset) return;
    const targetPath = path || remotePathForAsset(asset);

    const activeSession = sessions.value.find(session => session.asset.id === asset.id);
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
    const session = activeSession.value || sessions.value.find(item => item.asset.id === selectedAsset.value?.id);
    if (!session) {
      announce('上传需要先建立 SSH 会话');
      setTab('terminal');
      await connectSelected();
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
        } catch (cleanupError) {
          // best-effort cleanup
        }
      }
    }));
    await refreshRemoteFiles(remotePath.value).catch(() => null);
  }

  async function downloadEntry(entry) {
    const session = activeSession.value || sessions.value.find(item => item.asset.id === selectedAsset.value?.id);
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

  async function mkdirRemote(name) {
    const session = activeSession.value || sessions.value.find(item => item.asset.id === selectedAsset.value?.id);
    if (!session) return announce('需要先建立 SSH 会话');
    if (!name?.trim()) return announce('目录名不能为空');
    const target = joinPath(remotePath.value, name.trim());
    await invokeBackend('sftp_mkdir', { sessionId: session.sessionId, path: target });
    announce('已创建目录：' + name);
    await refreshRemoteFiles(remotePath.value);
  }

  async function renameRemote(entry, newName) {
    const session = activeSession.value || sessions.value.find(item => item.asset.id === selectedAsset.value?.id);
    if (!session) return announce('需要先建立 SSH 会话');
    if (!newName?.trim()) return announce('新名称不能为空');
    const target = joinPath(parentPath(entry.path), newName.trim());
    await invokeBackend('sftp_rename', { sessionId: session.sessionId, oldPath: entry.path, newPath: target });
    announce('已重命名：' + entry.name + ' → ' + newName);
    await refreshRemoteFiles(remotePath.value);
  }

  async function removeRemote(entry) {
    const session = activeSession.value || sessions.value.find(item => item.asset.id === selectedAsset.value?.id);
    if (!session) return announce('需要先建立 SSH 会话');
    await invokeBackend('sftp_remove', { sessionId: session.sessionId, path: entry.path, kind: entry.kind });
    announce('已删除：' + entry.name);
    await refreshRemoteFiles(remotePath.value);
  }

  async function statRemote(entry) {
    const session = activeSession.value || sessions.value.find(item => item.asset.id === selectedAsset.value?.id);
    if (!session) return null;
    return invokeBackend('sftp_stat', { sessionId: session.sessionId, path: entry.path });
  }

  async function refreshTunnels() {
    tunnels.value = (await invokeBackend('tunnel_list')).map(normalizeTunnelStatus);
  }

  async function createTunnel(form) {
    const sessionId = activeSession.value?.sessionId || '';
    const config = normalizeTunnelConfig({
      id: 'tunnel-' + Date.now(),
      session_id: sessionId,
      ...form
    });
    await invokeBackend('tunnel_create', { config });
    if (config.auto_start) await invokeBackend('tunnel_start', { sessionId, tunnelId: config.id });
    await refreshTunnels();
    modal.value = { type: null, asset: null };
    announce('隧道已创建：' + config.name);
  }

  async function toggleTunnel(tunnel) {
    if (tunnel.active) await invokeBackend('tunnel_stop', { tunnelId: tunnel.id });
    else await invokeBackend('tunnel_start', { sessionId: tunnel.config.session_id, tunnelId: tunnel.id });
    await refreshTunnels();
    announce((tunnel.active ? '已停止：' : '已启动：') + tunnel.config.name);
  }

  async function toggleTunnelAutoStart(tunnel) {
    const config = normalizeTunnelConfig({ ...tunnel.config, auto_start: !tunnel.config.auto_start });
    await invokeBackend('tunnel_delete', { tunnelId: tunnel.id });
    await invokeBackend('tunnel_create', { config });
    if (tunnel.active && config.auto_start) {
      await invokeBackend('tunnel_start', { sessionId: config.session_id, tunnelId: config.id });
    }
    await refreshTunnels();
    announce((config.auto_start ? '已启用自动启动：' : '已关闭自动启动：') + config.name);
  }

  async function deleteTunnel(tunnel) {
    await invokeBackend('tunnel_delete', { tunnelId: tunnel.id });
    await refreshTunnels();
    announce('隧道已删除：' + tunnel.config.name);
  }

  async function resolveHostKeyPrompt(requestId, accepted) {
    try {
      await invokeBackend('ssh_confirm_host_key', { requestId, accepted });
    } catch (error) {
      announce('主机密钥响应失败：' + error.message);
    }
    hostKeyPrompt.value = null;
    if (!accepted) modal.value = { type: null, asset: null };
  }

  async function resolveKeyboardPrompt(requestId, responses) {
    try {
      await invokeBackend('ssh_keyboard_response', { requestId, responses });
    } catch (error) {
      announce('键盘交互响应失败：' + error.message);
    }
    keyboardPrompt.value = null;
  }

  async function connectSelected() {
    const asset = selectedAsset.value;
    if (!asset) return;
    if (!isTauriRuntime()) {
      setTab('terminal');
      announce('SSH 需要桌面客户端：' + asset.name);
      return;
    }
    if (asset.auth_method === 'Password' && !asset.credential_id) {
      announce('该连接未保存密码：请编辑连接，填写密码后再连接');
      modal.value = { type: 'assetEditor', asset };
      return;
    }
    if (asset.auth_method === 'PrivateKey' && !asset.private_key_path) {
      announce('该连接未配置私钥路径：请编辑连接');
      modal.value = { type: 'assetEditor', asset };
      return;
    }
    const existing = sessions.value.find(session => session.asset.id === asset.id);
    if (existing) {
      setActiveSession(existing.sessionId);
      setTab('terminal');
      announce('已切换到会话：' + asset.name);
      return;
    }
    // 确保 host key / keyboard 监听器已注册，防止 emit 事件先于 listener 到达
    await ensureHostKeyListeners();
    setTab('terminal');
    announce('正在连接：' + asset.name);
    try {
      await ensureTerminalModules();
    } catch (error) {
      announce('终端模块加载失败：' + error.message);
      return;
    }
    if (!terminalContainer) throw new Error('terminal container is not mounted');

    const termDiv = document.createElement('div');
    termDiv.style.cssText = 'display:none;height:100%;';
    terminalContainer.appendChild(termDiv);

    const term = markRaw(new terminalModules.Terminal({
      cursorBlink: true,
      fontSize: 14,
      fontFamily: 'Consolas, "Courier New", monospace',
      theme: getTerminalTheme()
    }));
    const fit = markRaw(new terminalModules.FitAddon());
    const search = markRaw(new terminalModules.SearchAddon());
    term.loadAddon(fit);
    term.loadAddon(search);
    term.open(termDiv);
    term.writeln('\x1b[36mmyshelltool SSH\x1b[0m - connecting to ' + asset.host + '...\r\n');

    let connectedSessionId = null;
    try {
      const result = await invokeBackend('ssh_connect', {
        host: asset.host,
        port: asset.port,
        username: asset.username,
        password: '',
        credentialId: asset.credential_id || null,
        authMethod: asset.auth_method,
        privateKeyPath: asset.private_key_path,
        passphrase: null,
        passphraseCredentialId: asset.passphrase_credential_id || null
      });
      if (!result.connected) {
        term.writeln('\x1b[31mConnection failed: ' + (result.error || 'unknown') + '\x1b[0m\r\n');
        term.dispose();
        termDiv.remove();
        announce('连接失败：' + asset.name);
        return;
      }
      connectedSessionId = result.session_id;
      const session = {
        sessionId: connectedSessionId,
        asset,
        term,
        fit,
        search,
        termDiv,
        unlisten: null,
        resizeObserver: null
      };
      term.onData(data => {
        const encoder = new TextEncoder();
        invokeBackend('ssh_write', { sessionId: session.sessionId, data: Array.from(encoder.encode(data)) });
      });
      term.onResize(({ cols, rows }) => {
        invokeBackend('ssh_resize', { sessionId: session.sessionId, cols, rows }).catch(() => null);
      });
      attachResizeObserver(session);
      session.unlisten = await listenBackendEvent('ssh-output-' + session.sessionId, event => {
        if (event.payload && event.payload.length > 0) {
          const decoder = new TextDecoder();
          term.write(decoder.decode(new Uint8Array(event.payload)));
        } else {
          term.writeln('\x1b[31m\r\nConnection closed.\x1b[0m');
        }
      });
      sessions.value.push(session);
      activeSessionId.value = session.sessionId;
      showOnlyActiveTerminal();
      announce('已连接：' + asset.name);
    } catch (error) {
      if (connectedSessionId) await invokeBackend('ssh_disconnect', { sessionId: connectedSessionId }).catch(() => null);
      term.writeln('\x1b[31mError: ' + error.message + '\x1b[0m\r\n');
      term.dispose();
      termDiv.remove();
      announce('连接失败：' + error.message);
    }
  }

  function attachResizeObserver(session) {
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => {
      try {
        session.fit.fit();
      } catch {}
    });
    observer.observe(session.termDiv);
    session.resizeObserver = observer;
  }

  function runTerminalAction(action) {
    const session = activeSession.value;
    if (!session) {
      announce('请先连接主机');
      return;
    }
    if (action === 'clear') {
      session.term.clear();
      announce('终端已清屏');
      return;
    }
    if (action === 'search') {
      openTerminalSearch(session);
      return;
    }
    if (action === 'copy') {
      copyTerminalSelection(session);
      return;
    }
    if (action === 'paste') {
      pasteToTerminal(session);
      return;
    }
    if (action === 'fullscreen') {
      toggleTerminalFullscreen();
      return;
    }
  }

  function openTerminalSearch(session) {
    modal.value = { type: 'terminalSearch', asset: session.asset, payload: { sessionId: session.sessionId } };
  }

  async function executeTerminalSearch(sessionId, query, direction = 'next') {
    const session = sessions.value.find(item => item.sessionId === sessionId);
    if (!session || !query) return;
    const result = await session.search.findNext(query);
    if (!result) announce('未找到匹配：' + query);
  }

  async function copyTerminalSelection(session) {
    const selection = session.term.getSelection();
    if (!selection) {
      announce('终端无选中内容');
      return;
    }
    try {
      await navigator.clipboard.writeText(selection);
      announce('已复制终端选中文本');
    } catch {
      announce('剪贴板不可用');
    }
  }

  async function pasteToTerminal(session) {
    try {
      const text = await navigator.clipboard.readText();
      if (!text) return;
      const encoder = new TextEncoder();
      await invokeBackend('ssh_write', { sessionId: session.sessionId, data: Array.from(encoder.encode(text)) });
      announce('已粘贴到终端');
    } catch {
      announce('剪贴板读取失败');
    }
  }

  function toggleTerminalFullscreen() {
    document.documentElement.dataset.terminalFullscreen = document.documentElement.dataset.terminalFullscreen === 'true' ? 'false' : 'true';
    const session = activeSession.value;
    if (session) setTimeout(() => { try { session.fit.fit(); } catch {} }, 30);
    announce(document.documentElement.dataset.terminalFullscreen === 'true' ? '终端已全屏' : '终端已退出全屏');
  }

  function setTerminalContainer(element) {
    terminalContainer = element;
    showOnlyActiveTerminal();
  }

  function setActiveSession(sessionId) {
    if (!sessions.value.some(session => session.sessionId === sessionId)) return;
    activeSessionId.value = sessionId;
    showOnlyActiveTerminal();
  }

  async function disconnectSession(sessionId) {
    const session = sessions.value.find(item => item.sessionId === sessionId);
    if (!session) return;
    await invokeBackend('ssh_disconnect', { sessionId }).catch(() => null);
    if (typeof session.unlisten === 'function') session.unlisten();
    if (session.resizeObserver) session.resizeObserver.disconnect();
    session.term.dispose();
    session.termDiv.remove();
    sessions.value = sessions.value.filter(item => item.sessionId !== sessionId);
    activeSessionId.value = sessions.value.at(-1)?.sessionId || null;
    showOnlyActiveTerminal();
    announce('已断开：' + session.asset.name);
  }

  async function ensureTerminalModules() {
    if (terminalModules) return terminalModules;
    const [{ Terminal }, { FitAddon }, { SearchAddon }] = await Promise.all([
      import('@xterm/xterm'),
      import('@xterm/addon-fit'),
      import('@xterm/addon-search')
    ]);
    terminalModules = { Terminal, FitAddon, SearchAddon };
    return terminalModules;
  }

  function showOnlyActiveTerminal() {
    for (const session of sessions.value) {
      session.termDiv.style.display = session.sessionId === activeSessionId.value ? '' : 'none';
      if (session.sessionId === activeSessionId.value) {
        setTimeout(() => {
          try { session.fit.fit(); } catch {}
        }, 20);
      }
    }
  }

  function getTerminalTheme() {
    return effectiveTheme.value === 'light'
      ? { background: '#f7f7f7', foreground: '#1f2328', cursor: '#2f6feb', selectionBackground: '#c9d7f0' }
      : { background: '#111111', foreground: '#e8e8e8', cursor: '#6aa6ff', selectionBackground: '#2f415f' };
  }

  function openGlobalSearch() {
    searchState.value = { ...searchState.value, open: true };
  }

  function closeGlobalSearch() {
    searchState.value = { ...searchState.value, open: false };
  }

  function setGlobalSearchQuery(query) {
    searchState.value.query = query;
    const trimmed = query.trim();
    const sshMatch = trimmed.match(/^ssh\s+([^\s@]+)@([^\s:]+)(?::(\d+))?$/);
    if (sshMatch) {
      searchState.value.suggestions = [{
        kind: 'quick-connect',
        username: sshMatch[1],
        host: sshMatch[2],
        port: Number(sshMatch[3] || 22),
        label: `快速连接 ${sshMatch[1]}@${sshMatch[2]}`
      }];
      return;
    }
    const lower = trimmed.toLowerCase();
    searchState.value.suggestions = assets.value
      .filter(asset => [asset.name, asset.host, asset.username, asset.group, ...asset.tags].join(' ').toLowerCase().includes(lower))
      .slice(0, 6)
      .map(asset => ({ kind: 'asset', asset }));
  }

  async function activateSuggestion(suggestion) {
    if (!suggestion) return;
    if (suggestion.kind === 'asset') {
      selectAsset(suggestion.asset.id);
      closeGlobalSearch();
      return;
    }
    if (suggestion.kind === 'quick-connect') {
      closeGlobalSearch();
      const tempId = slugify(suggestion.host);
      const item = normalizeAsset({
        id: tempId,
        name: `${suggestion.username}@${suggestion.host}`,
        host: suggestion.host,
        port: suggestion.port,
        username: suggestion.username,
        auth_method: 'Password',
        group: '快速连接',
        tags: ['quick']
      });
      await saveAsset(item);
      await connectSelected();
    }
  }

  function uniqueAssetId(name, host) {
    const base = slugify(name || host || 'asset');
    let candidate = base;
    let index = 2;
    while (assets.value.some(item => item.id === candidate)) {
      candidate = base + '-' + index;
      index += 1;
    }
    return candidate;
  }

  return {
    backendStatus,
    assetSource,
    assets,
    selectedAssetId,
    selectedAsset,
    groupedAssets,
    activeTab,
    theme,
    effectiveTheme,
    themeLabel,
    assetsCollapsed,
    statusMessage,
    remotePath,
    remoteEntries,
    remotePathHistory,
    transferQueue,
    tunnels,
    sessions,
    activeSessionId,
    activeSession,
    githubPatConfigured,
    modal,
    hostKeyPrompt,
    keyboardPrompt,
    searchState,
    activeSessions,
    runningTunnels,
    warningCount,
    backendStatusText,
    assetSourceText,
    syncText,
    activeTransfers,
    completedTransfers,
    initialize,
    disposeEventListeners,
    announce,
    selectAsset,
    setTab,
    toggleTheme,
    toggleAssets,
    saveAsset,
    saveToken,
    deleteToken,
    refreshRemoteFiles,
    navigateRemotePath,
    navigateRemoteUp,
    uploadFiles,
    downloadEntry,
    mkdirRemote,
    renameRemote,
    removeRemote,
    statRemote,
    refreshTunnels,
    createTunnel,
    toggleTunnel,
    toggleTunnelAutoStart,
    deleteTunnel,
    resolveHostKeyPrompt,
    resolveKeyboardPrompt,
    connectSelected,
    runTerminalAction,
    executeTerminalSearch,
    setTerminalContainer,
    setActiveSession,
    disconnectSession,
    openGlobalSearch,
    closeGlobalSearch,
    setGlobalSearchQuery,
    activateSuggestion
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

function readStored(key) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function applyTheme(theme) {
  document.documentElement.dataset.theme = theme === 'light' ? 'light' : 'dark';
}

function normalizeStoredTheme(value) {
  return THEME_ORDER.includes(value) ? value : 'system';
}

function readSystemPrefersDark() {
  if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  }
  return true;
}

function applyAssetsState(collapsed, persist) {
  document.documentElement.dataset.assets = collapsed ? 'collapsed' : 'expanded';
  if (persist) localStorage.setItem('myshelltool-assets', collapsed ? 'collapsed' : 'expanded');
}

function tabLabel(tab) {
  return {
    overview: '工作区总览',
    terminal: '终端',
    files: '文件',
    tunnels: '隧道管理',
    editor: '编辑器'
  }[tab] || tab;
}
