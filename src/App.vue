<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue';
import { storeToRefs } from 'pinia';
import { normalizeStatus, remotePathForAsset, useWorkbenchStore } from './stores/workbench.js';
import { isTauriRuntime } from './services/backend.js';

const desktopRuntimeAvailable = ref(isTauriRuntime());

const store = useWorkbenchStore();
const {
  groupedAssets,
  selectedAsset,
  activeTab,
  theme,
  themeLabel,
  assetsCollapsed,
  statusMessage,
  remotePath,
  remoteEntries,
  transferQueue,
  tunnels,
  sessions,
  activeSession,
  githubPatConfigured,
  activeSessions,
  runningTunnels,
  warningCount,
  backendStatusText,
  assetSourceText,
  syncText,
  hostKeyPrompt,
  keyboardPrompt,
  searchState
} = storeToRefs(store);

const connectionFilter = ref('');
const tokenInput = ref('');
const terminalMount = ref(null);
const fileInput = ref(null);
const editingAsset = reactive(emptyAsset());
const editingCredential = reactive(emptyCredential());
const tunnelForm = reactive(emptyTunnelForm());
const terminalSearchQuery = ref('');
const keyboardResponses = reactive({});
const mkdirName = ref('');
const renameTarget = reactive({ path: '', current: '', next: '' });
const warningListOpen = ref(false);
const globalSearchInput = ref(null);

onMounted(() => {
  store.setTerminalContainer(terminalMount.value);
  store.initialize();
  window.addEventListener('keydown', handleGlobalKeydown);
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', handleGlobalKeydown);
  store.disposeEventListeners();
});

function handleGlobalKeydown(event) {
  if ((event.ctrlKey || event.metaKey) && (event.key === 'k' || event.key === 'K')) {
    event.preventDefault();
    store.openGlobalSearch();
    setTimeout(() => globalSearchInput.value?.focus(), 30);
  }
  if (event.key === 'Escape') {
    if (searchState.value.open) store.closeGlobalSearch();
    else if (warningListOpen.value) warningListOpen.value = false;
  }
}

watch(() => store.modal.type, type => {
  if (type === 'assetEditor') {
    Object.assign(editingAsset, store.modal.asset ? cloneAsset(store.modal.asset) : emptyAsset());
    Object.assign(editingCredential, emptyCredential());
  }
  if (type === 'tunnelCreate') {
    Object.assign(tunnelForm, emptyTunnelForm());
  }
  if (type === 'mkdir') {
    mkdirName.value = '';
  }
  if (type === 'rename') {
    Object.assign(renameTarget, {
      path: store.modal.entry?.path || '',
      current: store.modal.entry?.name || '',
      next: store.modal.entry?.name || ''
    });
  }
  if (type === 'terminalSearch') {
    terminalSearchQuery.value = '';
  }
  if (type === 'keyboardInteractive') {
    Object.keys(keyboardResponses).forEach(key => delete keyboardResponses[key]);
  }
});

const titleChip = computed(() => {
  const suffix = activeSessions.value === 1 ? 'session' : 'sessions';
  return `${store.backendStatus.ready ? 'online' : 'preview'} · ${activeSessions.value} ${suffix}`;
});

const filteredGroups = computed(() => {
  const query = connectionFilter.value.trim().toLowerCase();
  if (!query) return groupedAssets.value;
  return groupedGroupsFiltered(query);
});

function groupedGroupsFiltered(query) {
  return groupedAssets.value
    .map(group => ({
      ...group,
      items: group.items.filter(asset => [
        asset.name,
        asset.host,
        asset.username,
        asset.group,
        asset.auth_method,
        ...asset.tags
      ].join(' ').toLowerCase().includes(query))
    }))
    .filter(group => group.items.length);
}

const selectedRemotePath = computed(() => remotePathForAsset(selectedAsset.value));

const overviewRows = computed(() => {
  const rows = [];
  if (selectedAsset.value) {
    rows.push({
      title: `${selectedAsset.value.name} · ${selectedAsset.value.auth_method}`,
      body: `${selectedAsset.value.username}@${selectedAsset.value.host} · ${selectedRemotePath.value}`,
      action: 'terminal',
      actionLabel: '连接'
    });
  }
  if (remoteEntries.value.length) {
    rows.push({
      title: `${remoteEntries.value.length} 个远程条目`,
      body: `当前路径 ${remotePath.value}`,
      action: 'files',
      actionLabel: '查看'
    });
  }
  if (tunnels.value.length) {
    rows.push({
      title: `${runningTunnels.value}/${tunnels.value.length} 个隧道运行中`,
      body: tunnels.value.map(tunnel => tunnel.config.name).join(' · '),
      action: 'tunnels',
      actionLabel: '管理'
    });
  }
  return rows.length ? rows : [{
    title: '选择连接资产开始工作',
    body: '新增连接后即可打开终端、文件和隧道。',
    action: 'files',
    actionLabel: '打开文件'
  }];
});

const attentionRows = computed(() => {
  const rows = [];
  if (!githubPatConfigured.value) {
    rows.push({
      title: 'GitHub token 未配置',
      body: '同步功能只显示本地安全存储状态，不展示明文。',
      modal: 'tokenConfig',
      actionLabel: '配置'
    });
  }
  for (const asset of store.assets.filter(item => normalizeStatus(item.status).label === 'warning').slice(0, 2)) {
    rows.push({
      title: `${asset.name} 状态为 warning`,
      body: `${asset.host} · ${asset.last_connected}`,
      action: 'files',
      actionLabel: '查看'
    });
  }
  for (const tunnel of tunnels.value.filter(item => item.error).slice(0, 2)) {
    rows.push({
      title: `${tunnel.config.name} 隧道异常`,
      body: tunnel.error,
      action: 'tunnels',
      actionLabel: '处理'
    });
  }
  for (const transfer of transferQueue.value.filter(item => item.status === 'error').slice(0, 2)) {
    rows.push({
      title: `${transfer.direction === 'upload' ? '上传' : '下载'}失败：${transfer.name}`,
      body: transfer.error || '未知错误',
      action: 'files',
      actionLabel: '重试'
    });
  }
  return rows.length ? rows : [{
    title: '暂无阻塞项',
    body: '当前没有来自资产、凭据或隧道状态的待处理事项。',
    action: 'overview',
    actionLabel: '刷新'
  }];
});

const terminalSubtitle = computed(() => {
  if (activeSession.value) return `${activeSession.value.asset.username}@${activeSession.value.asset.host}`;
  if (selectedAsset.value) return `${selectedAsset.value.username}@${selectedAsset.value.host}`;
  return '点击左侧主机连接';
});

const modalTitle = computed(() => {
  switch (store.modal.type) {
    case 'assetEditor': return editingAsset.id ? '编辑连接资产' : '新增连接资产';
    case 'tunnelCreate': return '新增隧道';
    case 'tokenConfig': return '配置 / 更新 GitHub token';
    case 'hostKeyVerify': return '主机密钥验证';
    case 'keyboardInteractive': return '键盘交互认证';
    case 'mkdir': return '新建远程目录';
    case 'rename': return '重命名远程条目';
    case 'terminalSearch': return '终端搜索';
    default: return '提示';
  }
});

const assetCredentialHint = computed(() => {
  if (!editingAsset.id) return '新连接，密码/passphrase 可在下方填入';
  if (editingAsset.credential_id) return '密码已存储（重新输入会覆盖）';
  return '尚未存储密码';
});

function openAssetEditor(asset = null) {
  store.modal = { type: 'assetEditor', asset };
}

function openTunnelDialog() {
  store.modal = { type: 'tunnelCreate', asset: null };
}

function openTokenConfig() {
  tokenInput.value = '';
  store.modal = { type: 'tokenConfig', asset: null };
}

function openMkdir() {
  store.modal = { type: 'mkdir', entry: null };
}

function openRename(entry) {
  store.modal = { type: 'rename', entry };
}

function closeModal() {
  store.modal = { type: null, asset: null };
}

function submitModal() {
  switch (store.modal.type) {
    case 'assetEditor':
      if (editingAsset.auth_method === 'Password' && !editingAsset.credential_id && !editingCredential.password) {
        window.alert('Password 认证需要密码：首次保存请填写密码字段，否则无法连接');
        return;
      }
      store.saveAsset(
        { ...editingAsset, tags: splitTags(editingAsset.tags) },
        {
          password: editingCredential.password,
          passphrase: editingCredential.passphrase
        }
      );
      return;
    case 'tokenConfig':
      store.saveToken(tokenInput.value).then(saved => {
        if (saved) tokenInput.value = '';
      });
      return;
    case 'tunnelCreate':
      store.createTunnel({
        ...tunnelForm,
        local_port: Number(tunnelForm.local_port),
        remote_port: Number(tunnelForm.remote_port)
      });
      return;
    case 'mkdir':
      store.mkdirRemote(mkdirName.value).then(() => closeModal());
      return;
    case 'rename':
      store.renameRemote({ path: renameTarget.path, name: renameTarget.current }, renameTarget.next).then(() => closeModal());
      return;
    case 'hostKeyVerify':
      store.resolveHostKeyPrompt(hostKeyPrompt.value.request_id, true);
      closeModal();
      return;
    case 'keyboardInteractive':
      store.resolveKeyboardPrompt(keyboardPrompt.value.request_id, Object.values(keyboardResponses));
      closeModal();
      return;
    case 'terminalSearch':
      if (store.modal.payload?.sessionId) {
        store.executeTerminalSearch(store.modal.payload.sessionId, terminalSearchQuery.value);
      }
      return;
    default:
      closeModal();
  }
}

function denyHostKey() {
  store.resolveHostKeyPrompt(hostKeyPrompt.value.request_id, false);
  closeModal();
}

function tabClick(tab) {
  if (tab === 'terminal') store.connectSelected();
  else store.setTab(tab);
}

function handleGlobalSearchKeydown(event) {
  if (event.key === 'Enter') {
    event.preventDefault();
    const first = searchState.value.suggestions[0];
    if (first) store.activateSuggestion(first);
  }
}

function triggerFileUpload() {
  fileInput.value?.click();
}

function handleFilePick(event) {
  const files = event.target.files;
  store.uploadFiles(files);
  event.target.value = '';
}

function handleFileDrop(event) {
  event.preventDefault();
  const files = event.dataTransfer?.files;
  if (files && files.length) store.uploadFiles(files);
}

function handleFileDragOver(event) {
  event.preventDefault();
}

function openRemoteEntry(entry) {
  if (entry.kind === 'directory') {
    store.navigateRemotePath(entry.path);
  } else if (entry.kind === 'symlink') {
    store.navigateRemotePath(entry.path);
  } else {
    store.downloadEntry(entry);
  }
}

function remoteEntryActionLabel(entry) {
  if (entry.kind === 'directory') return '进入';
  return '下载';
}

function toggleWarningList() {
  warningListOpen.value = !warningListOpen.value;
}

function formatBytes(bytes) {
  const size = Number(bytes) || 0;
  if (size >= 1024 * 1024) return Math.round(size / 1024 / 1024) + ' MB';
  if (size >= 1024) return Math.round(size / 1024) + ' KB';
  return size + ' B';
}

function statusClass(status) {
  return normalizeStatus(status).dotClass.trim();
}

function tunnelKindLabel(kind) {
  if (kind === 'local') return 'Local';
  if (kind === 'remote') return 'Remote (不可用)';
  return 'Dynamic SOCKS';
}

function emptyAsset() {
  return {
    id: '',
    name: '',
    host: '',
    port: 22,
    username: '',
    auth_method: 'Password',
    private_key_path: '',
    group: '未分组',
    tags: '',
    status: 'Idle',
    credential_id: null,
    passphrase_credential_id: null
  };
}

function emptyCredential() {
  return { password: '', passphrase: '' };
}

function emptyTunnelForm() {
  return {
    name: '',
    kind: 'local',
    local_addr: '127.0.0.1',
    local_port: '',
    remote_addr: '',
    remote_port: '',
    auto_start: false
  };
}

function cloneAsset(asset) {
  return {
    ...asset,
    tags: asset.tags.join(', '),
    private_key_path: asset.private_key_path || ''
  };
}

function splitTags(tags) {
  return Array.isArray(tags) ? tags : String(tags || '').split(/[·,，\s]+/).filter(Boolean);
}
</script>

<template>
  <section class="app-shell">
    <div v-if="!desktopRuntimeAvailable" class="desktop-only-banner">
      <strong>桌面客户端模式未启动。</strong>
      SSH/SFTP/隧道功能需要 Tauri 桌面运行时。请运行 <code>npm run tauri:dev</code>（开发）或 <code>npm run tauri:build</code>（打包）。
    </div>
    <div class="window">
      <header class="titlebar">
        <div class="brand-lockup">
          <div class="app-mark">mst</div>
          <div class="brand-title">
            <strong>myshelltool</strong>
            <span>Windows SSH 客户端</span>
          </div>
        </div>
        <label class="quick-search" aria-label="全局搜索和快速连接">
          <span class="magnifier">/</span>
          <input
            id="globalSearch"
            ref="globalSearchInput"
            v-model="searchState.query"
            type="search"
            placeholder="搜索主机、标签、命令；输入 ssh user@host 快速连接"
            @input="store.setGlobalSearchQuery($event.target.value)"
            @keydown="handleGlobalSearchKeydown"
          />
          <kbd>Ctrl K</kbd>
          <ul v-if="searchState.open && searchState.suggestions.length" class="search-suggestions">
            <li v-for="(item, idx) in searchState.suggestions" :key="idx" @mousedown.prevent="store.activateSuggestion(item)">
              <strong v-if="item.kind === 'asset'">{{ item.asset.name }}</strong>
              <strong v-else>{{ item.label }}</strong>
              <span class="muted">{{ item.kind === 'asset' ? `${item.asset.username}@${item.asset.host}` : '回车建立连接' }}</span>
            </li>
          </ul>
        </label>
        <div class="title-actions">
          <span class="chip"><span class="dot" :class="{ running: store.backendStatus.ready }"></span>{{ titleChip }}</span>
          <button class="btn" id="themeToggle" type="button" :aria-pressed="theme === 'light'" @click="store.toggleTheme" :title="'点击切换主题（当前：' + themeLabel + '）'">
            <span data-theme-label>{{ themeLabel }}</span>
          </button>
          <button class="btn" data-modal="settingsHub" @click="openTokenConfig">同步 / 安全</button>
          <button class="btn" data-modal="connectionFailed" @click="toggleWarningList">{{ warningCount }} warning
            <div v-if="warningListOpen" class="warning-list">
              <p class="muted" v-if="!attentionRows.length">无待处理项</p>
              <div v-for="row in attentionRows" :key="row.title">
                <strong>{{ row.title }}</strong>
                <p class="muted">{{ row.body }}</p>
              </div>
            </div>
          </button>
        </div>
      </header>

      <div class="layout">
        <aside class="sidebar" aria-label="连接资产树">
          <div class="side-top">
            <div class="side-title">
              <h2>连接资产</h2>
              <div class="side-actions">
                <button class="icon-btn asset-toggle" id="assetToggle" type="button" :aria-label="assetsCollapsed ? '展开连接资产' : '收起连接资产'" :aria-expanded="String(!assetsCollapsed)" @click="store.toggleAssets">
                  <span class="when-expanded">‹</span><span class="when-collapsed">›</span>
                </button>
                <button class="icon-btn" data-asset-create aria-label="新增连接" @click="openAssetEditor()">＋</button>
              </div>
            </div>
            <input id="connectionFilter" v-model="connectionFilter" class="filter" type="search" placeholder="筛选分组、标签、主机、IP" />
          </div>
          <div class="tree" id="connectionTree">
            <div v-for="group in filteredGroups" :key="group.name" class="tree-section">
              <div class="tree-title"><span>{{ group.name }}</span><span>{{ group.items.length }}</span></div>
              <div
                v-for="asset in group.items"
                :key="asset.id"
                class="host-node"
                :class="{ active: selectedAsset?.id === asset.id }"
                :data-host="[asset.name, asset.host, asset.username, asset.group, asset.auth_method, ...asset.tags].join(' ')"
                :data-asset-id="asset.id"
                data-host-action
                @click="store.selectAsset(asset.id)"
                @dblclick="store.connectSelected"
              >
                <span class="dot" :class="statusClass(asset.status)"></span>
                <div><div class="host-name">{{ asset.name }}</div><div class="host-meta">{{ asset.host }} · {{ asset.username }}</div></div>
                <span class="host-label">{{ asset.tags[0] || asset.auth_method }}</span>
              </div>
            </div>
          </div>
        </aside>

        <main class="workbench" aria-label="主工作区">
          <div class="tabbar" role="tablist" aria-label="工作区标签" id="tabbar">
            <button class="workspace-tab" role="tab" :aria-selected="String(activeTab === 'overview')" data-tab="overview" @click="store.setTab('overview')">工作区总览</button>
            <button
              v-for="session in sessions"
              :key="session.sessionId"
              class="workspace-tab"
              role="tab"
              :aria-selected="String(activeTab === 'terminal' && activeSession?.sessionId === session.sessionId)"
              :data-tab="'session-' + session.sessionId"
              @click="store.setActiveSession(session.sessionId); store.setTab('terminal')"
            >
              {{ session.asset.name }} · SSH <span class="tab-close" title="关闭标签" @click.stop="store.disconnectSession(session.sessionId)">×</span>
            </button>
            <button class="workspace-tab" role="tab" :aria-selected="String(activeTab === 'terminal')" data-tab="terminal" @click="store.connectSelected">终端</button>
            <button class="workspace-tab" role="tab" :aria-selected="String(activeTab === 'files')" data-tab="files" @click="store.setTab('files')">文件 <span class="tab-close" title="关闭标签">×</span></button>
            <button class="workspace-tab" role="tab" :aria-selected="String(activeTab === 'tunnels')" data-tab="tunnels" @click="store.setTab('tunnels')">隧道管理</button>
          </div>

          <div class="panel-stage">
            <section class="screen-panel" :class="{ active: activeTab === 'overview' }" data-panel="overview">
              <div class="screen-header">
                <div>
                  <h1>工作区总览</h1>
                </div>
                <div class="toolbar">
                  <button class="btn primary" data-tab-target="terminal" @click="store.connectSelected">打开终端</button>
                  <button class="btn" data-tab-target="files" @click="store.setTab('files')">打开文件</button>
                  <button class="btn" data-modal="settingsHub" @click="openTokenConfig">配置中心</button>
                </div>
              </div>
              <div class="metric-grid">
                <div class="card metric-card"><span class="label">活跃 SSH</span><span class="value num">{{ activeSessions }}</span><span class="hint">来自运行时会话状态</span></div>
                <div class="card metric-card"><span class="label">传输队列</span><span class="value num">{{ transferQueue.filter(item => item.status === 'running').length }}/{{ transferQueue.length }}</span><span class="hint">上传 / 下载任务</span></div>
                <div class="card metric-card"><span class="label">隧道健康</span><span class="value num">{{ runningTunnels }}/{{ tunnels.length }}</span><span class="hint">来自 tunnel_list</span></div>
                <div class="card metric-card"><span class="label">同步状态</span><span class="value num">{{ githubPatConfigured ? 'on' : 'off' }}</span><span class="hint">{{ syncText }}</span></div>
              </div>
              <div class="focus-layout">
                <div class="stack">
                  <div class="card">
                    <div class="row-between"><h3>继续工作</h3><span class="status-pill running"><span class="dot running"></span>状态已接入</span></div>
                    <div style="margin-top: var(--space-3);">
                      <div v-for="row in overviewRows" :key="row.title" class="session-row">
                        <div><strong>{{ row.title }}</strong><p class="muted">{{ row.body }}</p></div>
                        <button class="btn" :data-tab-target="row.action" @click="tabClick(row.action)">{{ row.actionLabel }}</button>
                      </div>
                    </div>
                  </div>
                  <div class="card">
                    <div class="row-between"><h3>需要处理</h3><span class="status-pill warn"><span class="dot warn"></span>{{ attentionRows.length }} 项</span></div>
                    <div style="margin-top: var(--space-3);">
                      <div v-for="row in attentionRows" :key="row.title" class="activity-row">
                        <div><strong>{{ row.title }}</strong><p class="muted">{{ row.body }}</p></div>
                        <button class="btn" @click="row.modal ? openTokenConfig() : tabClick(row.action)">{{ row.actionLabel }}</button>
                      </div>
                    </div>
                  </div>
                </div>
                <aside class="stack">
                  <div class="card">
                    <h3>快速连接</h3>
                    <label class="stack" style="margin-top: var(--space-3);">
                      <input class="input" :value="selectedAsset ? `ssh ${selectedAsset.username}@${selectedAsset.host}` : ''" aria-label="快速连接命令" readonly />
                      <button class="btn primary" data-tab-target="terminal" @click="store.connectSelected">连接并打开终端</button>
                    </label>
                  </div>
                  <div class="callout warn">
                    <strong>安全提醒</strong>
                    <p class="muted">密码、私钥、token、passphrase 只显示本地安全存储状态；危险操作都从弹窗确认进入。</p>
                  </div>
                </aside>
              </div>
            </section>

            <section class="screen-panel" :class="{ active: activeTab === 'terminal' }" data-panel="terminal">
              <div class="screen-header">
                <div>
                  <h1>{{ activeSession?.asset.name || selectedAsset?.name || '未选择连接' }} · SSH</h1>
                </div>
                <div class="toolbar">
                  <button class="btn" @click="store.runTerminalAction('search')">搜索</button>
                  <button class="btn" @click="store.runTerminalAction('copy')">复制</button>
                  <button class="btn" @click="store.runTerminalAction('paste')">粘贴</button>
                  <button class="btn" @click="store.runTerminalAction('clear')">清屏</button>
                  <button class="btn" @click="store.runTerminalAction('fullscreen')">全屏</button>
                </div>
              </div>
              <div class="split-work">
                <div class="card terminal-pane">
                  <div class="pane-toolbar">
                    <div><strong id="terminalHost">{{ activeSession?.asset.name || selectedAsset?.name || '未连接' }}</strong> <span class="muted" id="terminalMeta">{{ terminalSubtitle }}</span></div>
                    <span class="status-pill" id="terminalStatus" :class="{ running: activeSession }"><span class="dot" :class="{ running: activeSession }"></span>{{ activeSession ? 'connected' : 'idle' }}</span>
                  </div>
                  <div id="terminalContainer" aria-label="终端区域">
                    <div ref="terminalMount" style="height:100%;"></div>
                    <div v-if="!activeSession" style="padding:var(--space-4);color:var(--muted)">
                      <span v-if="store.backendStatus.mode !== 'tauri-core'">SSH 终端需要桌面客户端。当前为浏览器预览模式。</span>
                      <span v-else>请点击左侧主机建立 SSH 连接。</span>
                    </div>
                  </div>
                </div>
                <aside class="stack">
                  <div class="card">
                    <div class="row-between"><h3>当前路径文件</h3><button class="btn" data-tab-target="files" @click="store.setTab('files')">完整文件管理</button></div>
                    <div style="margin-top: var(--space-3);">
                      <div v-for="entry in remoteEntries.slice(0, 3)" :key="entry.path" class="file-row">
                        <div><strong>{{ entry.name }}</strong><p class="muted">{{ entry.kind }} · {{ formatBytes(entry.size) }}</p></div>
                        <button class="btn" @click="store.setTab('files')">查看</button>
                      </div>
                      <p v-if="!remoteEntries.length" class="muted" style="padding:var(--space-3)">打开文件页后加载远程目录。</p>
                    </div>
                  </div>
                  <div class="card">
                    <h3>会话信息</h3>
                    <dl class="context-grid" style="margin-top: var(--space-3);">
                      <dt>认证</dt><dd>{{ selectedAsset?.auth_method || '-' }}</dd>
                      <dt>目标</dt><dd>{{ selectedAsset ? `${selectedAsset.host}:${selectedAsset.port}` : '-' }}</dd>
                      <dt>路径</dt><dd class="num">{{ selectedRemotePath }}</dd>
                      <dt>状态</dt><dd>{{ activeSession ? 'connected' : 'idle' }}</dd>
                    </dl>
                  </div>
                </aside>
              </div>
            </section>

            <section class="screen-panel" :class="{ active: activeTab === 'files' }" data-panel="files">
              <div class="screen-header">
                <div>
                  <h1>{{ selectedAsset?.name || '未选择连接' }} · 文件管理</h1>
                </div>
                <div class="toolbar">
                  <button class="btn primary" data-upload-trigger @click="triggerFileUpload">上传</button>
                  <button class="btn" data-mkdir @click="openMkdir">新建目录</button>
                  <button class="btn" @click="store.navigateRemoteUp">上级目录</button>
                  <button class="btn" data-refresh-remote-files @click="store.refreshRemoteFiles()">刷新</button>
                </div>
              </div>
              <input ref="fileInput" type="file" multiple style="display:none" @change="handleFilePick" />
              <div class="dual-pane">
                <div class="card file-pane" @drop="handleFileDrop" @dragover="handleFileDragOver">
                  <div class="pane-toolbar"><strong>本地</strong><span class="path-bar">上传队列 · 拖拽文件到此处</span></div>
                  <div class="file-list">
                    <div v-if="!transferQueue.length" class="file-row">
                      <div><strong>暂无传输任务</strong><p class="muted">选择文件后开始上传。</p></div>
                      <button class="btn" @click="triggerFileUpload">选择文件</button>
                    </div>
                    <div v-for="item in transferQueue.slice(0, 5)" :key="item.id" class="file-row">
                      <div><strong>{{ item.name }}</strong><p class="muted">{{ item.direction === 'upload' ? '上传' : '下载' }} · {{ formatBytes(item.transferred) }} / {{ formatBytes(item.total) }} · {{ item.status }}</p></div>
                      <span class="status-pill" :class="{ running: item.status === 'running', error: item.status === 'error' }">{{ item.percent }}%</span>
                    </div>
                  </div>
                </div>
                <div class="card file-pane">
                  <div class="pane-toolbar">
                    <strong>远程</strong>
                    <span class="path-bar" id="remotePathBar">{{ remotePath }}</span>
                  </div>
                  <div class="file-list" id="remoteFileList" aria-label="远程文件列表">
                    <div v-for="entry in remoteEntries" :key="entry.path" class="file-row" :data-remote-path="entry.path">
                      <div @click="entry.kind !== 'file' ? openRemoteEntry(entry) : null" :style="entry.kind !== 'file' ? 'cursor:pointer;flex:1' : 'flex:1'">
                        <strong>{{ entry.name }}</strong>
                        <p class="muted">{{ entry.kind }} · {{ formatBytes(entry.size) }} · {{ entry.modified || 'unknown' }}</p>
                      </div>
                      <button class="btn" @click="openRemoteEntry(entry)">{{ remoteEntryActionLabel(entry) }}</button>
                      <button class="btn" @click="openRename(entry)">重命名</button>
                      <button class="btn danger" @click="store.removeRemote(entry)">删除</button>
                    </div>
                    <div v-if="!remoteEntries.length" class="file-row"><div><strong>尚未加载</strong><p class="muted">点击刷新读取远程目录</p></div><span class="status-pill">idle</span></div>
                  </div>
                </div>
              </div>
              <div class="grid-2" style="margin-top: var(--space-4);">
                <div class="card">
                  <div class="row-between"><h3>传输队列</h3><span class="status-pill">{{ store.activeTransfers.length }} 进行中</span></div>
                  <div id="transferQueueList" style="margin-top: var(--space-3);">
                    <p v-if="!transferQueue.length" class="muted" style="text-align:center;padding:var(--space-3)">暂无传输任务</p>
                    <div v-for="item in transferQueue" :key="item.id" class="file-row transfer-row" :data-transfer-id="item.id">
                      <div>
                        <strong>{{ item.name }}</strong>
                        <p class="muted">
                          {{ item.direction === 'upload' ? '上传' : '下载' }} ·
                          {{ formatBytes(item.transferred) }} / {{ formatBytes(item.total) }} ·
                          {{ item.status }}
                        </p>
                        <div class="progress-bar">
                          <div class="progress-fill" :class="{ error: item.status === 'error', done: item.status === 'done' }" :style="{ width: item.percent + '%' }"></div>
                        </div>
                      </div>
                      <span class="status-pill" :class="{ running: item.status === 'running', error: item.status === 'error', warn: item.status === 'done' }">{{ item.percent }}%</span>
                    </div>
                  </div>
                </div>
              </div>
            </section>

            <section class="screen-panel" :class="{ active: activeTab === 'tunnels' }" data-panel="tunnels">
              <div class="screen-header">
                <div>
                  <h1>隧道管理</h1>
                </div>
                <div class="toolbar"><button class="btn primary" data-create-tunnel @click="openTunnelDialog">新增隧道</button><button class="btn" data-refresh-tunnels @click="store.refreshTunnels">刷新</button></div>
              </div>
              <div class="card">
                <div class="table-wrap">
                  <table class="data-table" id="tunnelTable">
                    <thead><tr><th>名称</th><th>类型</th><th>本地</th><th>远程</th><th>目标主机</th><th>状态</th><th>自动启动</th><th>健康检查</th><th>操作</th></tr></thead>
                    <tbody>
                      <tr v-for="tunnel in tunnels" :key="tunnel.id" data-tunnel-row :data-tunnel-id="tunnel.id" :data-session-id="tunnel.config.session_id">
                        <td>{{ tunnel.config.name }}</td>
                        <td>{{ tunnelKindLabel(tunnel.config.kind) }}</td>
                        <td class="num">{{ tunnel.config.local_addr }}:{{ tunnel.config.local_port }}</td>
                        <td class="num">{{ tunnel.config.kind === 'dynamic' ? '—' : `${tunnel.config.remote_addr}:${tunnel.config.remote_port}` }}</td>
                        <td>{{ tunnel.config.session_id }}</td>
                        <td><span class="status-pill" :class="{ running: tunnel.active, error: tunnel.error }" data-status-pill>{{ tunnel.active ? 'Running' : (tunnel.error || 'Stopped') }}</span></td>
                        <td><button class="switch" :class="{ on: tunnel.config.auto_start }" :aria-pressed="String(tunnel.config.auto_start)" data-tunnel-autostart @click="store.toggleTunnelAutoStart(tunnel)"><span></span></button></td>
                        <td>{{ tunnel.active ? '活跃' : (tunnel.error ? '错误' : '未运行') }}</td>
                        <td>
                          <button class="btn" data-tunnel-toggle @click="store.toggleTunnel(tunnel)">{{ tunnel.active ? '停止' : '启动' }}</button>
                          <button class="btn danger" data-tunnel-delete @click="store.deleteTunnel(tunnel)">删除</button>
                        </td>
                      </tr>
                      <tr v-if="!tunnels.length"><td colspan="9" style="text-align:center;color:var(--app-muted)">暂无隧道配置，点击"新增隧道"创建</td></tr>
                    </tbody>
                  </table>
                </div>
              </div>
            </section>
          </div>
        </main>

        <aside class="context-panel" aria-label="上下文详情">
          <div class="context-head">
            <div class="row">
              <div>
                <h2 id="contextTitle">{{ selectedAsset?.name || '未选择连接' }}</h2>
                <p class="muted" id="contextSubtitle">{{ selectedAsset ? `${selectedAsset.host} · ${selectedAsset.username} · ${normalizeStatus(selectedAsset.status).label}` : '选择左侧资产' }}</p>
              </div>
              <span class="dot" id="contextDot" :class="selectedAsset ? statusClass(selectedAsset.status) : ''"></span>
            </div>
            <div class="action-list"><button class="btn primary" data-tab-target="terminal" @click="store.connectSelected">连接</button><button class="btn" data-tab-target="files" @click="store.setTab('files')">文件</button><button class="btn" data-tab-target="tunnels" @click="store.setTab('tunnels')">隧道</button></div>
          </div>
          <div class="context-body stack">
            <div class="context-card">
              <strong>主机摘要</strong>
              <dl class="context-grid" style="margin-top: var(--space-3);">
                <dt>地址</dt><dd id="contextAddress" class="num">{{ selectedAsset ? `${selectedAsset.host}:${selectedAsset.port}` : '-' }}</dd>
                <dt>用户</dt><dd id="contextUser">{{ selectedAsset?.username || '-' }}</dd>
                <dt>标签</dt><dd id="contextTags">{{ selectedAsset?.tags.join(' · ') || '-' }}</dd>
                <dt>最近连接</dt><dd id="contextLast">{{ selectedAsset?.last_connected || '-' }}</dd>
                <dt>凭据</dt><dd>{{ selectedAsset?.credential_id ? '本地安全存储（已存）' : '未存储' }}</dd>
              </dl>
            </div>
            <div class="context-card">
              <div class="row-between"><strong>同步 / 安全</strong><button class="btn" data-modal="settingsHub" @click="openTokenConfig">打开</button></div>
              <div class="security-row"><div><p class="strong">Git 配置同步</p><p class="muted">{{ syncText }} · 不展示明文</p></div><span class="status-pill" :class="{ running: githubPatConfigured }">{{ githubPatConfigured ? '正常' : '未配置' }}</span></div>
              <div class="security-row"><div><p class="strong">known_hosts</p><p class="muted">桌面端连接时由后端校验</p></div><span class="status-pill">后端</span></div>
            </div>
            <div class="context-card" aria-label="后端桥接状态">
              <strong>后端桥接</strong>
              <dl class="context-grid" style="margin-top: var(--space-3);">
                <dt>状态</dt><dd id="backendStatus">{{ backendStatusText }}</dd>
                <dt>资产来源</dt><dd id="assetSource">{{ assetSourceText }}</dd>
              </dl>
            </div>
            <div class="context-card">
              <div class="row-between"><strong>快捷操作</strong><span class="status-pill warn">{{ warningCount }} warning</span></div>
              <div class="tagline" style="margin-top: var(--space-3);"><button class="btn" data-modal="tokenConfig" @click="openTokenConfig">更新 token</button><button class="btn" @click="openAssetEditor(selectedAsset)">编辑连接</button></div>
            </div>
          </div>
        </aside>
      </div>

      <footer class="statusbar">
        <div class="status-left"><span><span class="dot" :class="{ running: activeSessions > 0 }"></span> SSH {{ activeSessions > 0 ? 'connected' : 'idle' }}</span><span>backend <span class="num">{{ store.backendStatus.mode }}</span></span><span class="status-message" data-status-message>{{ statusMessage }}</span></div>
        <div class="status-right"><span>{{ syncText }}</span><span>tunnels {{ runningTunnels }}/{{ tunnels.length }}</span><span><span class="dot warn"></span> {{ warningCount }} warning</span></div>
      </footer>
    </div>
  </section>

  <div class="modal-layer" id="modalLayer" :class="{ open: store.modal.type }" :aria-hidden="String(!store.modal.type)">
    <div class="modal" role="dialog" aria-modal="true" aria-labelledby="modalTitle">
      <div class="modal-head"><h2 id="modalTitle">{{ modalTitle }}</h2><button class="icon-btn" id="modalClose" aria-label="关闭" @click="closeModal">×</button></div>
      <div class="modal-body" id="modalBody">
        <div v-if="store.modal.type === 'assetEditor'" class="stack">
          <div class="grid-2">
            <label class="stack"><span class="muted">名称</span><input v-model="editingAsset.name" class="input" data-asset-field="name" /></label>
            <label class="stack"><span class="muted">主机</span><input v-model="editingAsset.host" class="input" data-asset-field="host" /></label>
            <label class="stack"><span class="muted">端口</span><input v-model="editingAsset.port" class="input" type="number" min="1" max="65535" data-asset-field="port" /></label>
            <label class="stack"><span class="muted">用户名</span><input v-model="editingAsset.username" class="input" data-asset-field="username" /></label>
            <label class="stack"><span class="muted">分组</span><input v-model="editingAsset.group" class="input" data-asset-field="group" /></label>
            <label class="stack"><span class="muted">标签</span><input v-model="editingAsset.tags" class="input" data-asset-field="tags" placeholder="prod, app" /></label>
            <label class="stack"><span class="muted">认证方式</span><select v-model="editingAsset.auth_method" class="select" data-asset-field="auth_method"><option>Password</option><option>PrivateKey</option></select></label>
            <label class="stack"><span class="muted">私钥路径</span><input v-model="editingAsset.private_key_path" class="input" data-asset-field="private_key_path" placeholder="~/.ssh/id_ed25519" :disabled="editingAsset.auth_method !== 'PrivateKey'" /></label>
            <label class="stack"><span class="muted">状态</span><select v-model="editingAsset.status" class="select" data-asset-field="status"><option>Connected</option><option>Warning</option><option>Idle</option></select></label>
          </div>
          <div class="callout">
            <strong>凭据</strong>
            <p class="muted">{{ assetCredentialHint }}</p>
          </div>
          <div class="grid-2">
            <label v-if="editingAsset.auth_method === 'Password'" class="stack"><span class="muted">密码（明文不会回显，仅保存到本地安全存储）<span v-if="!editingAsset.credential_id" style="color:var(--danger,#dc2626)"> · 首次保存必填</span></span><input v-model="editingCredential.password" class="input" type="password" data-asset-field="password" placeholder="首次保存必填；编辑时留空保留既有密码" /></label>
            <label v-if="editingAsset.auth_method === 'PrivateKey'" class="stack"><span class="muted">Passphrase（可选）</span><input v-model="editingCredential.passphrase" class="input" type="password" data-asset-field="passphrase" placeholder="无加密私钥留空" /></label>
          </div>
        </div>
        <div v-else-if="store.modal.type === 'tunnelCreate'" class="stack">
          <label class="stack"><span>名称</span><input v-model="tunnelForm.name" class="input" id="tunnelName" placeholder="mysql-local" /></label>
          <label class="stack"><span>类型</span><select v-model="tunnelForm.kind" class="input" id="tunnelKind"><option value="local">Local（本地端口转发）</option><option value="dynamic">Dynamic SOCKS</option><option value="remote">Remote（暂未实现，将提示错误）</option></select></label>
          <label class="stack"><span>本地地址</span><input v-model="tunnelForm.local_addr" class="input" id="tunnelLocalAddr" /></label>
          <label class="stack"><span>本地端口</span><input v-model="tunnelForm.local_port" class="input" id="tunnelLocalPort" type="number" placeholder="13306" /></label>
          <div v-if="tunnelForm.kind !== 'dynamic'" id="tunnelRemoteFields">
            <label class="stack"><span>远程地址</span><input v-model="tunnelForm.remote_addr" class="input" id="tunnelRemoteAddr" placeholder="10.10.9.32" /></label>
            <label class="stack"><span>远程端口</span><input v-model="tunnelForm.remote_port" class="input" id="tunnelRemotePort" type="number" placeholder="3306" /></label>
          </div>
          <label><input v-model="tunnelForm.auto_start" type="checkbox" id="tunnelAutoStart" /> 自动启动</label>
        </div>
        <div v-else-if="store.modal.type === 'hostKeyVerify'" class="stack">
          <p class="muted">检测到主机密钥，请确认是否信任该主机。</p>
          <dl class="context-grid">
            <dt>主机</dt><dd>{{ hostKeyPrompt?.host_port }}</dd>
            <dt>密钥类型</dt><dd>{{ hostKeyPrompt?.key_type }}</dd>
            <dt>指纹</dt><dd class="num" style="word-break:break-all">{{ hostKeyPrompt?.fingerprint }}</dd>
            <dt>状态</dt><dd>{{ hostKeyPrompt?.is_changed ? '密钥已变更（警告）' : '首次连接' }}</dd>
          </dl>
          <p class="muted">确认后将会保存到本地 known_hosts，下次连接不再提示。</p>
        </div>
        <div v-else-if="store.modal.type === 'keyboardInteractive'" class="stack">
          <p class="muted">服务器需要键盘交互认证，请根据提示输入：</p>
          <p v-if="keyboardPrompt?.name"><strong>{{ keyboardPrompt.name }}</strong></p>
          <p v-if="keyboardPrompt?.instructions" class="muted">{{ keyboardPrompt.instructions }}</p>
          <label v-for="(prompt, idx) in (keyboardPrompt?.prompts || [])" :key="idx" class="stack">
            <span class="muted">{{ prompt }}</span>
            <input v-model="keyboardResponses[idx]" class="input" type="password" :placeholder="prompt" />
          </label>
        </div>
        <div v-else-if="store.modal.type === 'mkdir'" class="stack">
          <label class="stack"><span>目录名</span><input v-model="mkdirName" class="input" placeholder="new-folder" /></label>
          <p class="muted">将在当前远程路径下创建：{{ remotePath }}</p>
        </div>
        <div v-else-if="store.modal.type === 'rename'" class="stack">
          <label class="stack"><span>新名称</span><input v-model="renameTarget.next" class="input" /></label>
          <p class="muted">原名称：{{ renameTarget.current }}</p>
        </div>
        <div v-else-if="store.modal.type === 'terminalSearch'" class="stack">
          <label class="stack"><span>搜索关键字</span><input v-model="terminalSearchQuery" class="input" placeholder="error" @keydown.enter="submitModal" /></label>
          <p class="muted">点击确认定位到下一个匹配。</p>
        </div>
        <div v-else class="stack">
          <p>token 仅写入本地安全存储。界面提交后只展示"已配置"或"未配置"。</p>
          <label class="stack"><span class="muted">Personal Access Token</span><input v-model="tokenInput" class="input" type="password" data-sync-token placeholder="粘贴 token，保存后立即隐藏" /></label>
          <p class="muted" data-token-storage-status>本地安全存储：{{ githubPatConfigured ? '已配置' : '未配置' }}</p>
          <button class="btn danger" data-delete-credential @click="store.deleteToken">清除已保存的 token</button>
        </div>
      </div>
      <div class="modal-actions">
        <button v-if="store.modal.type === 'hostKeyVerify'" class="btn danger" @click="denyHostKey">拒绝</button>
        <button class="btn" id="modalSecondary" @click="closeModal">取消</button>
        <button class="btn primary" id="modalPrimary" @click="submitModal">确认</button>
      </div>
    </div>
  </div>
</template>

<style>
.progress-bar {
  width: 100%;
  height: 4px;
  background: var(--app-bg-elevated, rgba(255,255,255,0.06));
  border-radius: 2px;
  margin-top: 6px;
  overflow: hidden;
}
.progress-fill {
  height: 100%;
  background: linear-gradient(90deg, #2f6feb, #6aa6ff);
  transition: width 0.2s ease;
}
.progress-fill.done { background: #2ea043; }
.progress-fill.error { background: #da3633; }

.search-suggestions {
  position: absolute;
  top: 100%;
  left: 0;
  right: 0;
  background: var(--app-bg, #1a1a1a);
  border: 1px solid var(--app-border, rgba(255,255,255,0.12));
  border-radius: 8px;
  margin-top: 6px;
  padding: 4px;
  list-style: none;
  z-index: 20;
  max-height: 280px;
  overflow-y: auto;
}
.search-suggestions li {
  padding: 8px 10px;
  display: flex;
  justify-content: space-between;
  gap: 8px;
  border-radius: 6px;
  cursor: pointer;
}
.search-suggestions li:hover { background: rgba(255,255,255,0.05); }

.warning-list {
  position: absolute;
  top: 100%;
  right: 0;
  width: 280px;
  background: var(--app-bg, #1a1a1a);
  border: 1px solid var(--app-border, rgba(255,255,255,0.12));
  border-radius: 8px;
  margin-top: 6px;
  padding: 10px;
  z-index: 20;
  text-align: left;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.transfer-row { align-items: flex-start; }
.transfer-row > div { flex: 1; }

.desktop-only-banner {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  z-index: 9999;
  padding: 14px 24px;
  background: #fef3c7;
  color: #78350f;
  border-bottom: 1px solid #f59e0b;
  font-size: 14px;
  text-align: center;
}
.desktop-only-banner code {
  background: rgba(0,0,0,0.08);
  padding: 2px 6px;
  border-radius: 3px;
  font-family: ui-monospace, SFMono-Regular, monospace;
}
</style>
