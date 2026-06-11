const panels = [...document.querySelectorAll('[data-panel]')];
    const tabs = [...document.querySelectorAll('.workspace-tab')];
    const statusMessage = document.querySelector('[data-status-message]');
    const backendStatus = document.getElementById('backendStatus');
    const assetSource = document.getElementById('assetSource');
    const connectionTree = document.getElementById('connectionTree');
    const contextMenu = document.getElementById('contextMenu');
    const modalLayer = document.getElementById('modalLayer');
    const modalTitle = document.getElementById('modalTitle');
    const modalBody = document.getElementById('modalBody');
    const modalPrimary = document.getElementById('modalPrimary');
    const modalSecondary = document.getElementById('modalSecondary');
    const modalClose = document.getElementById('modalClose');
    const themeToggle = document.getElementById('themeToggle');
    const themeLabel = document.querySelector('[data-theme-label]');
    const assetToggle = document.getElementById('assetToggle');
    const compactAssetsQuery = window.matchMedia('(max-width: 1180px)');
    let assetPreferenceLocked = false;
    const contextTitle = document.getElementById('contextTitle');
    const contextSubtitle = document.getElementById('contextSubtitle');
    const contextDot = document.getElementById('contextDot');
    const contextAddress = document.getElementById('contextAddress');
    const contextUser = document.getElementById('contextUser');
    const contextTags = document.getElementById('contextTags');
    const contextLast = document.getElementById('contextLast');
    const browserAssetsKey = 'myshelltool-connection-assets';
    let connectionAssets = [];
    let selectedAssetId = null;
    let activeModal = null;
    let editingAssetId = null;

    const sessions = new Map();
    let activeSessionId = null;
    let currentRemotePath = '/';
    let pendingPasswordResolve = null;
    let pendingHostKeyResolve = null;

    const fallbackAssets = [
      asset('prod-bastion', 'prod-bastion', '10.10.4.8', 'root', '收藏', ['favorite', 'ProxyJump'], 'Connected', '15 分钟前', 'PrivateKey'),
      asset('web-01', 'web-01', '10.10.8.21', 'deploy', '收藏', ['web', 'release'], 'Connected', '1 小时前', 'PrivateKey'),
      asset('db-readonly', 'db-readonly', '10.10.9.32', 'audit', '收藏', ['db', 'readonly'], 'Warning', '昨天', 'Password'),
      asset('app-cluster-01', 'app-cluster-01', '172.18.1.44', 'ubuntu', '生产环境', ['prod', 'app'], 'Connected', '今天', 'PrivateKey'),
      asset('cache-redis-02', 'cache-redis-02', '172.18.2.19', 'redis', '生产环境', ['redis', 'idle'], 'Idle', '3 天前', 'Password'),
      asset('ops-jump-gateway', 'ops-jump-gateway', '172.18.0.10', 'ops', '生产环境', ['jump', 'proxy'], 'Connected', '刚刚', 'PrivateKey'),
      asset('lab-windows-dev', 'lab-windows-dev', '192.168.31.70', 'administrator', '最近连接', ['win', 'dev'], 'Connected', '刚刚', 'Password'),
      asset('nas-backup', 'nas-backup', '192.168.31.9', 'backup', '最近连接', ['sftp', 'backup'], 'Warning', '昨天', 'PrivateKey')
    ];

    async function invokeBackend(command, args = {}) {
      const tauriInvoke = window.__TAURI__?.core?.invoke;
      if (typeof tauriInvoke === 'function') {
        return tauriInvoke(command, args);
      }

      if (command === 'backend_status') {
        return { ready: true, mode: 'browser-preview' };
      }
      if (command === 'list_connection_assets') {
        const assets = readBrowserAssets();
        return { source: 'browser-preview local assets', count: assets.length, assets };
      }
      if (command === 'save_connection_asset') {
        const assets = readBrowserAssets();
        const savedAsset = normalizeAsset(args.asset);
        const index = assets.findIndex(item => item.id === savedAsset.id);
        if (index >= 0) {
          assets[index] = savedAsset;
        } else {
          assets.push(savedAsset);
        }
        writeBrowserAssets(assets);
        return { source: 'browser-preview local assets', count: assets.length, assets };
      }
      if (command === 'save_sync_settings') {
        const token = args.token?.token;
        return {
          enabled: Boolean(args.settings?.enabled),
          endpoint: args.settings?.endpoint || '',
          interval_minutes: Number(args.settings?.interval_minutes) || 0,
          token_status: { configured: typeof token === 'string' && token.trim().length > 0 }
        };
      }
      if (command === 'save_credential') {
        const id = String(args.id || 'default');
        const secret = String(args.secret || '');
        if (!secret.trim()) throw new Error('secret must not be empty');
        try { sessionStorage.setItem('cred-' + id, 'x'); } catch {}
        return { id, exists: true, label: id };
      }
      if (command === 'get_credential_status') {
        const id = String(args.id || 'default');
        let exists = false;
        try { exists = sessionStorage.getItem('cred-' + id) !== null; } catch {}
        return { id, exists, label: id };
      }
      if (command === 'delete_credential') {
        const id = String(args.id || 'default');
        try { sessionStorage.removeItem('cred-' + id); } catch {}
        return true;
      }
      if (command === 'ssh_connect') {
        return { session_id: '', connected: false, error: 'SSH requires desktop client' };
      }
      if (command === 'ssh_list_directory') {
        const path = args.path || '/srv/app/releases';
        return {
          host: args.host || 'browser-preview',
          path,
          entries: [
            { name: 'current', path: path + '/current', kind: 'symlink', size: 0, modified: 'preview' },
            { name: 'config.toml', path: path + '/config.toml', kind: 'file', size: 8192, modified: 'preview' },
            { name: 'logs', path: path + '/logs', kind: 'directory', size: 0, modified: 'preview' },
            { name: 'backup.tar', path: path + '/backup.tar', kind: 'file', size: 100663296, modified: 'preview' }
          ]
        };
      }
      if (command === 'ssh_confirm_host_key') {
        return null;
      }
      if (command === 'ssh_write' || command === 'ssh_resize' || command === 'ssh_disconnect') {
        return null;
      }
      throw new Error('Unsupported backend command: ' + command);
    }

    window.invokeBackend = invokeBackend;

    const modals = {
      settingsHub: {
        title: '同步 / 安全配置中心',
        body: `<div class="grid-2"><div class="stack"><h3>GitHub / Git 同步</h3><label class="stack"><span class="muted">仓库地址</span><input class="input" value="git@github.com:private/myshell-config.git" /></label><label class="stack"><span class="muted">分支</span><input class="input" value="main" /></label><div class="callout"><strong>安全边界</strong><p class="muted">token 仅保存在本地安全存储；不会写入 Git 仓库；不会同步明文密码、私钥或 passphrase。</p></div></div><div class="stack"><h3>本地安全</h3><div class="security-row"><div><strong>known_hosts</strong><p class="muted">3 个已信任，1 个待确认</p></div><button class="btn" data-modal="fingerprint">确认</button></div><div class="security-row"><div><strong>SSH agent</strong><p class="muted">OpenSSH Agent 已启用，Pageant 未启用</p></div><button class="switch on"><span></span></button></div><div class="security-row"><div><strong>FIDO2 / 硬件密钥</strong><p class="muted">第一阶段预留入口</p></div><button class="btn">预留配置</button></div></div></div>`,
        primary: '保存设置',
        secondary: '取消'
      },
      fingerprint: {
        title: '首次连接主机指纹确认',
        body: `<p>这是第一次连接 prod-bastion。请确认主机指纹来自可信来源后再继续。</p><div class="fingerprint" style="margin-top: var(--space-3);">ED25519 · SHA256:Wm7kQ3y2p9s9GxK8qT0NfLocalSecurePreview</div><ul><li>默认动作：取消连接</li><li>信任后写入 known_hosts</li><li>后续变更会触发高危警告</li></ul>`,
        primary: '信任并连接',
        secondary: '取消连接'
      },
      hostKeyWarning: {
        title: 'host key 变更高危警告',
        body: `<p>web-01 的主机指纹与 known_hosts 记录不一致。可能是服务器重装，也可能是中间人攻击。</p><div class="fingerprint" style="margin-top: var(--space-3);">旧指纹 SHA256:F4m2...2Q<br>新指纹 SHA256:P9r8...7A</div><ul><li>建议先联系管理员核对</li><li>继续前必须输入主机名称确认</li><li>默认不允许自动覆盖 known_hosts</li></ul>`,
        primary: '我已核对，更新指纹',
        secondary: '取消连接'
      },
      deleteFile: {
        title: '删除远程文件确认',
        body: `<p>将删除远程路径 /srv/app/releases/backup.tar。该操作不可从 myshelltool 内直接撤销。</p><ul><li>需要再次确认目标路径</li><li>批量删除时展示文件数量和总大小</li><li>危险按钮保持红色，默认焦点不落在确认上</li></ul>`,
        primary: '确认删除',
        secondary: '取消'
      },
      fileConflict: {
        title: '覆盖文件冲突处理',
        body: `<p>远程已存在 config.toml，且修改时间晚于本地版本。</p><div class="grid-3" style="margin-top: var(--space-3);"><button class="btn">覆盖</button><button class="btn">跳过</button><button class="btn">重命名</button></div><label class="stack" style="margin-top: var(--space-3);"><span class="muted">应用范围</span><select class="select"><option>仅当前文件</option><option>应用到全部冲突</option></select></label>`,
        primary: '应用选择',
        secondary: '取消'
      },
      editConflict: {
        title: '远程编辑保存冲突',
        body: `<p>远端文件已在编辑期间发生变化。建议先查看差异，再决定覆盖保存或取消。</p><ul><li>查看差异：打开只读 diff</li><li>覆盖保存：写入本地编辑版本</li><li>取消：保留编辑缓冲区，不上传</li></ul>`,
        primary: '查看差异',
        secondary: '取消'
      },
      gitConflict: {
        title: 'Git 同步冲突处理',
        body: `<p>connections/prod-bastion.toml 同时存在本地和远程修改。</p><div class="grid-3" style="margin-top: var(--space-3);"><button class="btn">使用本地版本</button><button class="btn">使用远程版本</button><button class="btn">手动合并</button></div><p class="muted" style="margin-top: var(--space-3);">敏感引用不会参与 diff，token 和私钥不会显示。</p>`,
        primary: '继续处理',
        secondary: '稍后处理'
      },
      tokenConfig: {
        title: '配置 / 更新 GitHub token',
        body: `<p>token 仅写入本地安全存储。界面提交后只展示”已配置”或”未配置”。</p><label class=”stack” style=”margin-top: var(--space-3);”><span class=”muted”>Personal Access Token</span><input class=”input” type=”password” data-sync-token placeholder=”粘贴 token，保存后立即隐藏” /></label><p class=”muted” data-token-storage-status>本地安全存储：检测中</p><button class=”btn danger” style=”margin-top: var(--space-2);” data-delete-credential>清除已保存的 token</button><ul><li>不会写入 Git 仓库</li><li>不会同步到远程</li><li>可以随时清除并重新测试连接</li></ul>`,
        primary: '保存到本地安全存储',
        secondary: '取消'
      },
      connectionFailed: {
        title: '连接失败错误详情',
        body: `<p>prod-bastion 连接失败：ProxyJump 链路中的 ops-jump-gateway 响应超时。</p><div class="fingerprint" style="margin-top: var(--space-3);">ssh -J ops-jump-gateway root@10.10.4.8<br>error: connect timeout after 30s</div><ul><li>重试连接</li><li>查看 SSH 日志</li><li>编辑跳板配置</li></ul>`,
        primary: '重试',
        secondary: '查看日志'
      }
    };

    function asset(id, name, host, username, group, tags, status, lastConnected, authMethod) {
      return {
        id,
        name,
        host,
        port: 22,
        username,
        auth_method: authMethod,
        private_key_path: null,
        group,
        tags,
        status,
        last_connected: lastConnected
      };
    }

    function announce(message) {
      statusMessage.textContent = message;
    }

    function readStoredTheme() {
      try {
        return localStorage.getItem('myshelltool-theme') || 'dark';
      } catch {
        return 'dark';
      }
    }

    function storeTheme(theme) {
      try {
        localStorage.setItem('myshelltool-theme', theme);
      } catch {
        return;
      }
    }

    function applyTheme(theme) {
      const nextTheme = theme === 'light' ? 'light' : 'dark';
      document.documentElement.dataset.theme = nextTheme;
      themeToggle.setAttribute('aria-pressed', String(nextTheme === 'light'));
      themeLabel.textContent = nextTheme === 'light' ? '深色模式' : '浅色模式';
      storeTheme(nextTheme);
    }

    function readStoredAssetsState() {
      try {
        return localStorage.getItem('myshelltool-assets');
      } catch {
        return null;
      }
    }

    function storeAssetsState(collapsed) {
      try {
        localStorage.setItem('myshelltool-assets', collapsed ? 'collapsed' : 'expanded');
      } catch {
        return;
      }
    }

    function readBrowserAssets() {
      try {
        const stored = localStorage.getItem(browserAssetsKey);
        if (stored) return JSON.parse(stored).map(normalizeAsset);
      } catch {
        return fallbackAssets.map(normalizeAsset);
      }
      return fallbackAssets.map(normalizeAsset);
    }

    function writeBrowserAssets(assets) {
      try {
        localStorage.setItem(browserAssetsKey, JSON.stringify(assets.map(normalizeAsset)));
      } catch {
        return;
      }
    }

    function applyAssetsState(collapsed, persist = true) {
      document.documentElement.dataset.assets = collapsed ? 'collapsed' : 'expanded';
      assetToggle.setAttribute('aria-expanded', String(!collapsed));
      assetToggle.setAttribute('aria-label', collapsed ? '展开连接资产' : '收起连接资产');
      assetToggle.title = collapsed ? '展开连接资产' : '收起连接资产';
      if (persist) storeAssetsState(collapsed);
    }

    function activateTab(id) {
      const isSessionTab = id.startsWith('session-');
      const panelId = isSessionTab ? 'terminal' : id;
      const allTabs = document.querySelectorAll('.workspace-tab');
      allTabs.forEach(tab => tab.setAttribute('aria-selected', String(tab.dataset.tab === id)));
      panels.forEach(panel => panel.classList.toggle('active', panel.dataset.panel === panelId));
      const targetTab = [...allTabs].find(tab => tab.dataset.tab === id);
      if (targetTab) targetTab.hidden = false;
      if (isSessionTab) {
        const sessionId = id.replace('session-', '');
        switchToSession(sessionId);
      }
      if (id === 'files') refreshRemoteFiles().catch(err => announce('远程文件刷新失败：' + err.message));
      try {
        localStorage.setItem('myshelltool-active-tab', id);
      } catch {}
      announce('已切换到 ' + (targetTab?.textContent.replace('×', '').trim() || id));
    }

    function renderBackendStatus(status) {
      const mode = status.mode || 'unknown';
      backendStatus.textContent = status.ready ? '已连接 · ' + mode : '未就绪 · ' + mode;
    }

    function renderAssetSource(result) {
      const source = result.source || result.mode || 'unknown';
      const count = Number.isFinite(result.count) ? result.count : (Array.isArray(result.assets) ? result.assets.length : 0);
      assetSource.textContent = source + ' · ' + count + ' 项';
    }

    async function initializeBackendBridge() {
      try {
        const [status, assets] = await Promise.all([
          invokeBackend('backend_status'),
          invokeBackend('list_connection_assets')
        ]);
        connectionAssets = (assets.assets || []).map(normalizeAsset);
        renderBackendStatus(status);
        renderAssetSource(assets);
        renderConnectionTree();
      } catch {
        backendStatus.textContent = '不可用';
        assetSource.textContent = 'fallback 未加载';
        connectionAssets = fallbackAssets.map(normalizeAsset);
        renderConnectionTree();
      }
    }

    function normalizeAsset(item) {
      const tags = Array.isArray(item?.tags) ? item.tags : String(item?.tags || '').split(/[·,，\s]+/).filter(Boolean);
      return {
        id: String(item?.id || slugify(item?.name || item?.host || 'asset')),
        name: String(item?.name || '未命名连接'),
        host: String(item?.host || item?.address || ''),
        port: Number(item?.port) || 22,
        username: String(item?.username || item?.user || ''),
        auth_method: item?.auth_method || 'Password',
        private_key_path: item?.private_key_path || null,
        group: String(item?.group || '未分组'),
        tags,
        status: item?.status || 'Idle',
        last_connected: String(item?.last_connected || item?.lastConnected || '从未')
      };
    }

    function renderConnectionTree() {
      const groups = new Map();
      for (const item of connectionAssets) {
        const group = item.group || '未分组';
        if (!groups.has(group)) groups.set(group, []);
        groups.get(group).push(item);
      }

      connectionTree.innerHTML = [...groups.entries()].map(([group, assets]) => `
        <div class="tree-section">
          <div class="tree-title"><span>${escapeHtml(group)}</span><span>${assets.length}</span></div>
          ${assets.map(renderAssetNode).join('')}
        </div>
      `).join('');

      const selected = connectionAssets.find(item => item.id === selectedAssetId) || connectionAssets[0];
      if (selected) selectAsset(selected.id, false);
    }

    function renderAssetNode(item) {
      const status = normalizeStatus(item.status);
      const label = item.tags[0] || item.auth_method;
      const searchable = [item.name, item.host, item.username, item.group, item.auth_method, ...item.tags].join(' ');
      return `
        <div class="host-node" data-host="${escapeAttr(searchable)}" data-asset-id="${escapeAttr(item.id)}" data-host-action>
          <span class="dot${status.dotClass}"></span>
          <div><div class="host-name">${escapeHtml(item.name)}</div><div class="host-meta">${escapeHtml(item.host)} · ${escapeHtml(item.username)}</div></div>
          <span class="host-label">${escapeHtml(label)}</span>
        </div>
      `;
    }

    function selectAsset(id, announceSelection = true) {
      const item = connectionAssets.find(asset => asset.id === id);
      if (!item) return;
      selectedAssetId = item.id;
      document.querySelectorAll('[data-host-action]').forEach(node => {
        node.classList.toggle('active', node.dataset.assetId === item.id);
      });
      updateContext(item);
      if (announceSelection) announce('已选择连接：' + item.name);
    }

    function updateContext(asset) {
      const status = normalizeStatus(asset.status);
      contextTitle.textContent = asset.name;
      contextSubtitle.textContent = asset.host + ' · ' + asset.username + ' · ' + status.label;
      contextAddress.textContent = asset.host + ':' + asset.port;
      contextUser.textContent = asset.username;
      contextTags.textContent = asset.tags.join(' · ') || asset.group;
      contextLast.textContent = asset.last_connected;
      contextDot.className = 'dot' + status.dotClass;
    }

    function remotePathForAsset(asset) {
      if (!asset) return '/srv/app/releases';
      if (asset.tags.includes('backup')) return '/backup';
      if (asset.tags.includes('redis')) return '/var/lib/redis';
      if (asset.tags.includes('web') || asset.tags.includes('release')) return '/srv/app/releases';
      return '/home/' + (asset.username || 'user');
    }

    function formatBytes(bytes) {
      const size = Number(bytes) || 0;
      if (size >= 1024 * 1024) return Math.round(size / 1024 / 1024) + ' MB';
      if (size >= 1024) return Math.round(size / 1024) + ' KB';
      return size + ' B';
    }

    function fileIcon(kind) {
      if (kind === 'directory') return '📁';
      if (kind === 'symlink') return '🔗';
      return '📄';
    }

    function renderRemoteFiles(result) {
      const pathBar = document.getElementById('remotePathBar');
      const list = document.getElementById('remoteFileList');
      if (!pathBar || !list) return;
      currentRemotePath = result.path || '/';
      pathBar.innerHTML = '';
      const parts = currentRemotePath.split('/').filter(Boolean);
      const breadcrumb = document.createDocumentFragment();
      const rootLink = document.createElement('span');
      rootLink.className = 'breadcrumb-link';
      rootLink.textContent = '/';
      rootLink.onclick = () => refreshRemoteFiles('/');
      breadcrumb.appendChild(rootLink);
      let accumulated = '';
      parts.forEach((part, i) => {
        accumulated += '/' + part;
        const sep = document.createElement('span');
        sep.className = 'breadcrumb-sep';
        sep.textContent = ' / ';
        breadcrumb.appendChild(sep);
        if (i < parts.length - 1) {
          const link = document.createElement('span');
          link.className = 'breadcrumb-link';
          link.textContent = part;
          const targetPath = accumulated;
          link.onclick = () => refreshRemoteFiles(targetPath);
          breadcrumb.appendChild(link);
        } else {
          const current = document.createElement('span');
          current.className = 'breadcrumb-current';
          current.textContent = part;
          breadcrumb.appendChild(current);
        }
      });
      pathBar.appendChild(breadcrumb);

      const entries = Array.isArray(result.entries) ? result.entries : [];
      if (!entries.length) {
        list.innerHTML = '<div class="file-row"><div><strong>空目录</strong><p class="muted">没有远程文件</p></div><span class="status-pill">empty</span></div>';
        return;
      }
      list.innerHTML = entries.map(entry => {
        const kindLabel = entry.kind === 'directory' ? '目录' : entry.kind === 'symlink' ? '链接' : '文件';
        const icon = fileIcon(entry.kind);
        const clickAttr = entry.kind === 'directory'
          ? `data-open-remote-dir`
          : '';
        const actions = entry.kind === 'directory'
          ? `<button class="btn" data-open-remote-dir>打开</button><button class="btn" data-file-rename="${escapeAttr(entry.path)}">重命名</button><button class="btn danger" data-file-delete="${escapeAttr(entry.path)}&kind=directory">删除</button>`
          : `<button class="btn" data-file-download="${escapeAttr(entry.path)}">下载</button><button class="btn" data-file-edit="${escapeAttr(entry.path)}">编辑</button><button class="btn" data-file-rename="${escapeAttr(entry.path)}">重命名</button><button class="btn danger" data-file-delete="${escapeAttr(entry.path)}&kind=file">删除</button>`;
        return `<div class="file-row" data-remote-path="${escapeAttr(entry.path)}" ${clickAttr}><div><span style="margin-right:6px">${icon}</span><strong>${escapeHtml(entry.name)}</strong><p class="muted">${kindLabel} · ${formatBytes(entry.size)} · ${escapeHtml(entry.modified || 'unknown')}</p></div>${actions}</div>`;
      }).join('');
    }

    async function resolveAssetAuth(asset) {
      const isTauri = typeof window.__TAURI__?.core?.invoke === 'function';
      let password = '';
      let credentialId = null;
      let passphrase = null;
      let passphraseCredId = null;
      const authMethod = asset.auth_method || 'Password';

      if (authMethod === 'Password') {
        const credId = 'ssh-pw-' + asset.id;
        if (isTauri) {
          try {
            const status = await invokeBackend('get_credential_status', { id: credId });
            if (status.exists) credentialId = credId;
          } catch {}
        }
        if (isTauri && !credentialId) {
          const result = await openCredentialPrompt(asset);
          if (!result) return null;
          password = result.password;
          if (result.savePassword) {
            try { await invokeBackend('save_credential', { id: credId, secret: password }); } catch {}
          }
        }
      } else if (authMethod === 'PrivateKey') {
        const credId = 'ssh-key-' + asset.id;
        if (isTauri) {
          try {
            const status = await invokeBackend('get_credential_status', { id: credId });
            if (status.exists) passphraseCredId = credId;
          } catch {}
        }
        if (isTauri && !passphraseCredId) {
          const result = await openCredentialPrompt(asset, {
            title: '输入私钥 Passphrase — ' + asset.name,
            label: 'Passphrase',
            placeholder: '留空表示无 passphrase',
            saveLabel: '保存 passphrase 到本地安全存储',
            note: '如果私钥有 passphrase 请输入，无 passphrase 可直接点连接。',
            allowEmpty: true
          });
          if (!result) return null;
          passphrase = result.password;
          if (result.savePassword && passphrase) {
            try { await invokeBackend('save_credential', { id: credId, secret: passphrase }); } catch {}
          }
        }
      }

      return { password, credentialId, passphrase, passphraseCredId, authMethod };
    }

    function getSessionForAsset(assetId) {
      for (const [, s] of sessions) {
        if (s.asset.id === assetId) return s;
      }
      return null;
    }

    async function refreshRemoteFiles(path = null) {
      const asset = connectionAssets.find(item => item.id === selectedAssetId) || connectionAssets[0];
      if (!asset) return;
      const remotePath = path || remotePathForAsset(asset);
      const list = document.getElementById('remoteFileList');
      const pathBar = document.getElementById('remotePathBar');
      if (pathBar) pathBar.textContent = remotePath;
      if (list) list.innerHTML = '<div class="file-row"><div><strong>加载中</strong><p class="muted">正在读取远程目录</p></div><span class="status-pill warn">loading</span></div>';

      const activeSession = getSessionForAsset(asset.id);
      if (activeSession) {
        try {
          const result = await invokeBackend('sftp_list_dir', {
            session_id: activeSession.sessionId,
            path: remotePath
          });
          renderRemoteFiles(result);
          announce('远程文件已刷新（SFTP）：' + asset.name);
          return;
        } catch (e) {
          announce('SFTP 列表失败，回退到 SSH exec');
        }
      }

      const auth = await resolveAssetAuth(asset);
      if (!auth) return;
      const result = await invokeBackend('ssh_list_directory', {
        host: asset.host,
        port: asset.port,
        username: asset.username,
        password: auth.password,
        credential_id: auth.credentialId,
        auth_method: auth.authMethod,
        private_key_path: asset.private_key_path,
        passphrase: auth.passphrase,
        passphrase_credential_id: auth.passphraseCredId,
        path: remotePath
      });
      renderRemoteFiles(result);
      announce('远程文件已刷新：' + asset.name);
    }

    async function createRemoteDir() {
      const name = prompt('新建目录名称：');
      if (!name) return;
      const session = getSessionForAsset(selectedAssetId);
      if (!session) { announce('需要先连接 SSH'); return; }
      const fullPath = currentRemotePath === '/' ? '/' + name : currentRemotePath + '/' + name;
      await invokeBackend('sftp_mkdir', { session_id: session.sessionId, path: fullPath });
      announce('已创建目录：' + name);
      refreshRemoteFiles(currentRemotePath);
    }

    async function renameRemoteFile(oldPath) {
      const newName = prompt('新名称：', oldPath.split('/').pop());
      if (!newName) return;
      const session = getSessionForAsset(selectedAssetId);
      if (!session) { announce('需要先连接 SSH'); return; }
      const dir = oldPath.substring(0, oldPath.lastIndexOf('/')) || '/';
      const newPath = dir + '/' + newName;
      await invokeBackend('sftp_rename', { session_id: session.sessionId, old_path: oldPath, new_path: newPath });
      announce('已重命名为：' + newName);
      refreshRemoteFiles(currentRemotePath);
    }

    async function deleteRemoteFile(filePath, kind) {
      const name = filePath.split('/').pop();
      if (!confirm('确定要删除 ' + kind === 'directory' ? '目录' : '文件' + ' "' + name + '"？此操作不可撤销。')) return;
      const session = getSessionForAsset(selectedAssetId);
      if (!session) { announce('需要先连接 SSH'); return; }
      await invokeBackend('sftp_remove', { session_id: session.sessionId, path: filePath, kind });
      announce('已删除：' + name);
      refreshRemoteFiles(currentRemotePath);
    }

    async function uploadFile(file) {
      const session = getSessionForAsset(selectedAssetId);
      if (!session) { announce('需要先连接 SSH'); return; }
      const remotePath = currentRemotePath === '/' ? '/' + file.name : currentRemotePath + '/' + file.name;
      announce('正在上传：' + file.name);
      const content = await file.text();
      await invokeBackend('sftp_write_file', { session_id: session.sessionId, path: remotePath, content });
      announce('已上传：' + file.name);
      refreshRemoteFiles(currentRemotePath);
    }

    function openModal(key) {
      const item = modals[key];
      if (!item) return;
      activeModal = key;
      editingAssetId = null;
      modalTitle.textContent = item.title;
      modalBody.innerHTML = item.body;
      modalPrimary.textContent = item.primary;
      modalSecondary.textContent = item.secondary;
      modalLayer.classList.add('open');
      modalLayer.setAttribute('aria-hidden', 'false');
      modalPrimary.focus();
      if (key === 'tokenConfig') {
        const storageStatus = modalBody.querySelector('[data-token-storage-status]');
        invokeBackend('get_credential_status', { id: 'github-pat' }).then(status => {
          if (storageStatus) storageStatus.textContent = '本地安全存储：' + (status.exists ? '已配置' : '未配置');
        }).catch(() => {
          if (storageStatus) storageStatus.textContent = '本地安全存储：未配置';
        });
        const deleteBtn = modalBody.querySelector('[data-delete-credential]');
        if (deleteBtn) {
          deleteBtn.addEventListener('click', async () => {
            await invokeBackend('delete_credential', { id: 'github-pat' });
            const storageStatus = modalBody.querySelector('[data-token-storage-status]');
            if (storageStatus) storageStatus.textContent = '本地安全存储：未配置';
            announce('已清除本地安全存储中的 token');
          });
        }
      }
      announce('已打开弹窗：' + item.title);
    }

    function openAssetEditor(assetToEdit = null) {
      activeModal = 'assetEditor';
      editingAssetId = assetToEdit?.id || null;
      const item = assetToEdit || asset('', '', '', '', '未分组', [], 'Idle', '从未', 'Password');
      modalTitle.textContent = assetToEdit ? '编辑连接资产' : '新增连接资产';
      modalBody.innerHTML = `<div class="grid-2">
        <label class="stack"><span class="muted">名称</span><input class="input" data-asset-field="name" value="${escapeAttr(item.name)}" /></label>
        <label class="stack"><span class="muted">主机</span><input class="input" data-asset-field="host" value="${escapeAttr(item.host)}" /></label>
        <label class="stack"><span class="muted">端口</span><input class="input" type="number" min="1" max="65535" data-asset-field="port" value="${escapeAttr(String(item.port || 22))}" /></label>
        <label class="stack"><span class="muted">用户名</span><input class="input" data-asset-field="username" value="${escapeAttr(item.username)}" /></label>
        <label class="stack"><span class="muted">分组</span><input class="input" data-asset-field="group" value="${escapeAttr(item.group)}" /></label>
        <label class="stack"><span class="muted">标签</span><input class="input" data-asset-field="tags" value="${escapeAttr(item.tags.join(', '))}" placeholder="prod, app" /></label>
        <label class="stack"><span class="muted">认证方式</span><select class="select" data-asset-field="auth_method"><option>Password</option><option>PrivateKey</option><option>Token</option></select></label>
        <label class="stack"><span class="muted">私钥路径</span><input class="input" data-asset-field="private_key_path" value="${escapeAttr(item.private_key_path || '')}" placeholder="~/.ssh/id_ed25519" /></label>
        <label class="stack"><span class="muted">状态</span><select class="select" data-asset-field="status"><option>Connected</option><option>Warning</option><option>Idle</option></select></label>
      </div><div class="callout" style="margin-top: var(--space-3);"><strong>安全边界</strong><p class="muted">这里只保存连接资产元数据；密码、私钥、passphrase 和 token 不会写入资产 JSON。</p></div>`;
      modalBody.querySelector('[data-asset-field="auth_method"]').value = item.auth_method;
      modalBody.querySelector('[data-asset-field="status"]').value = item.status;
      modalPrimary.textContent = assetToEdit ? '保存连接资产' : '创建连接资产';
      modalSecondary.textContent = '取消';
      modalLayer.classList.add('open');
      modalLayer.setAttribute('aria-hidden', 'false');
      modalBody.querySelector('[data-asset-field="name"]').focus();
      announce(assetToEdit ? '正在编辑连接资产' : '正在新增连接资产');
    }

    function closeModal() {
      if (activeModal === 'hostKeyPrompt' && pendingHostKeyResolve) {
        pendingHostKeyResolve(false);
        pendingHostKeyResolve = null;
      }
      if (activeModal === 'passwordPrompt' && pendingPasswordResolve) {
        pendingPasswordResolve(null);
        pendingPasswordResolve = null;
      }
      modalLayer.classList.remove('open');
      modalLayer.setAttribute('aria-hidden', 'true');
      activeModal = null;
      editingAssetId = null;
      announce('弹窗已关闭');
    }

    async function saveAssetEditor() {
      const field = name => modalBody.querySelector('[data-asset-field="' + name + '"]')?.value.trim() || '';
      const name = field('name');
      const host = field('host');
      const username = field('username');
      const port = Number(field('port')) || 22;
      if (!name || !host || !username || port < 1 || port > 65535) {
        announce('连接资产需要名称、主机、用户名和有效端口');
        return;
      }
      const previous = connectionAssets.find(item => item.id === editingAssetId);
      const item = normalizeAsset({
        id: previous?.id || uniqueAssetId(name, host),
        name,
        host,
        port,
        username,
        auth_method: field('auth_method') || 'Password',
        private_key_path: field('private_key_path') || null,
        group: field('group') || '未分组',
        tags: field('tags').split(/[·,，\s]+/).filter(Boolean),
        status: field('status') || 'Idle',
        last_connected: previous?.last_connected || '从未'
      });
      const result = await invokeBackend('save_connection_asset', { asset: item });
      connectionAssets = (result.assets || []).map(normalizeAsset);
      renderAssetSource(result);
      renderConnectionTree();
      selectAsset(item.id);
      closeModal();
      announce('连接资产已保存：' + item.name);
    }

    async function saveTokenConfig() {
      const tokenInput = modalBody.querySelector('[data-sync-token]');
      const storageStatus = modalBody.querySelector('[data-token-storage-status]');
      const secret = tokenInput?.value || '';
      if (!secret.trim()) {
        announce('token 不能为空');
        return;
      }
      await invokeBackend('save_credential', { id: 'github-pat', secret });
      if (tokenInput) tokenInput.value = '';
      const status = await invokeBackend('get_credential_status', { id: 'github-pat' });
      const configured = Boolean(status.exists);
      if (storageStatus) storageStatus.textContent = '本地安全存储：' + (configured ? '已配置' : '未配置');
      announce('同步配置已保存，本地安全存储：' + (configured ? '已配置' : '未配置'));
    }

    function normalizeStatus(status) {
      if (status === 'Connected' || status === 'connected') return { label: 'connected', dotClass: ' running' };
      if (status === 'Warning' || status === 'warning') return { label: 'warning', dotClass: ' warn' };
      return { label: 'idle', dotClass: '' };
    }

    function uniqueAssetId(name, host) {
      const base = slugify(name || host || 'asset');
      let candidate = base;
      let index = 2;
      while (connectionAssets.some(item => item.id === candidate)) {
        candidate = base + '-' + index;
        index += 1;
      }
      return candidate;
    }

    function slugify(value) {
      const slug = String(value).trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
      return slug || 'asset';
    }

    function escapeHtml(value) {
      return String(value).replace(/[&<>"]/g, char => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[char]));
    }

    function escapeAttr(value) {
      return escapeHtml(value).replace(/'/g, '&#39;');
    }

    function openCredentialPrompt(asset, opts = {}) {
      const title = opts.title || ('输入 SSH 密码 — ' + asset.name);
      const label = opts.label || '密码';
      const placeholder = opts.placeholder || '输入 SSH 密码';
      const saveLabel = opts.saveLabel || '保存密码到本地安全存储';
      const allowEmpty = Boolean(opts.allowEmpty);
      const note = opts.note || `用户 ${escapeHtml(asset.username)} 的凭据未保存在本地安全存储中。`;
      return new Promise(resolve => {
        pendingPasswordResolve = resolve;
        activeModal = 'passwordPrompt';
        editingAssetId = null;
        modalBody.dataset.allowEmpty = allowEmpty ? 'true' : 'false';
        modalTitle.textContent = title;
        modalBody.innerHTML = `<p>正在连接 <strong>${escapeHtml(asset.name)}</strong>（${escapeHtml(asset.host)}:${asset.port}）</p><p class="muted">${note}</p><label class="stack" style="margin-top: var(--space-3);"><span class="muted">${label}</span><input class="input" type="password" data-ssh-password placeholder="${placeholder}" /></label><label style="margin-top: var(--space-3); display: flex; align-items: center; gap: var(--space-2);"><input type="checkbox" data-save-password /><span class="muted">${saveLabel}</span></label>`;
        modalPrimary.textContent = '连接';
        modalSecondary.textContent = '取消';
        modalLayer.classList.add('open');
        modalLayer.setAttribute('aria-hidden', 'false');
        const pwInput = modalBody.querySelector('[data-ssh-password]');
        if (pwInput) {
          pwInput.focus();
          pwInput.addEventListener('keydown', e => { if (e.key === 'Enter') modalPrimary.click(); });
        }
        announce(opts.announce || ('请输入凭据'));
      });
    }

    function updateTerminalToolbar(asset) {
      const terminalHost = document.getElementById('terminalHost');
      const terminalMeta = document.getElementById('terminalMeta');
      const terminalStatus = document.getElementById('terminalStatus');
      if (asset) {
        terminalHost.textContent = asset.name;
        terminalMeta.textContent = asset.username + '@' + asset.host + ' · xterm-256color';
        terminalStatus.innerHTML = '<span class="dot running"></span>connected';
      } else {
        terminalHost.textContent = '未连接';
        terminalMeta.textContent = '点击左侧主机连接';
        terminalStatus.innerHTML = '<span class="dot"></span>idle';
      }
    }

    function switchToSession(sessionId) {
      sessions.forEach((s, id) => {
        s.termDiv.style.display = id === sessionId ? '' : 'none';
      });
      activeSessionId = sessionId;
      const session = sessions.get(sessionId);
      if (session) {
        setTimeout(() => { try { session.fit.fit(); } catch {} }, 10);
        updateTerminalToolbar(session.asset);
      }
    }

    function removeSession(sessionId) {
      const session = sessions.get(sessionId);
      if (!session) return;
      session.term.dispose();
      session.termDiv.remove();
      session.tab.remove();
      sessions.delete(sessionId);
      if (activeSessionId === sessionId) {
        activeSessionId = null;
        const remaining = [...sessions.keys()];
        if (remaining.length > 0) {
          activateTab('session-' + remaining[remaining.length - 1]);
        } else {
          updateTerminalToolbar(null);
          activateTab('overview');
        }
      }
    }

    function showHostKeyDialog(eventData) {
      return new Promise(resolve => {
        pendingHostKeyResolve = resolve;
        activeModal = 'hostKeyPrompt';
        editingAssetId = null;
        const { host_port, key_type, fingerprint, is_changed } = eventData;
        if (is_changed) {
          modalTitle.textContent = 'host key 变更高危警告';
          modalBody.innerHTML = `<p>${escapeHtml(host_port)} 的主机指纹与 known_hosts 记录不一致。可能是服务器重装，也可能是中间人攻击。</p><div class="fingerprint" style="margin-top: var(--space-3);">新指纹 ${escapeHtml(key_type)} · ${escapeHtml(fingerprint)}</div><ul><li>建议先联系管理员核对</li><li>默认不允许自动覆盖 known_hosts</li></ul>`;
          modalPrimary.textContent = '我已核对，更新指纹';
        } else {
          modalTitle.textContent = '首次连接主机指纹确认';
          modalBody.innerHTML = `<p>这是第一次连接 ${escapeHtml(host_port)}。请确认主机指纹来自可信来源后再继续。</p><div class="fingerprint" style="margin-top: var(--space-3);">${escapeHtml(key_type)} · ${escapeHtml(fingerprint)}</div><ul><li>默认动作：取消连接</li><li>信任后写入 known_hosts</li><li>后续变更会触发高危警告</li></ul>`;
          modalPrimary.textContent = '信任并连接';
        }
        modalSecondary.textContent = '取消连接';
        modalLayer.classList.add('open');
        modalLayer.setAttribute('aria-hidden', 'false');
        modalPrimary.focus();
        announce(is_changed ? '主机指纹已变更，请确认' : '首次连接，请确认主机指纹');
      });
    }

    function getTerminalTheme() {
      return document.documentElement.dataset.theme === 'light'
        ? { background: '#ffffff', foreground: '#1e1e1e', cursor: '#333333' }
        : { background: '#1e1e1e', foreground: '#d4d4d4', cursor: '#cccccc' };
    }

    async function initTerminal() {
      const container = document.getElementById('terminalContainer');
      if (!container) return;
      const isTauri = typeof window.__TAURI__?.core?.invoke === 'function';
      if (isTauri) {
        window.__TAURI__.core.getCurrentWindow().listen('ssh-host-key-verify', async event => {
          const accepted = await showHostKeyDialog(event.payload);
          invokeBackend('ssh_confirm_host_key', {
            request_id: event.payload.request_id,
            accepted
          });
        });
      } else {
        container.innerHTML = '<div style="padding:var(--space-4);color:var(--muted)">SSH 终端需要桌面客户端。当前为浏览器预览模式。</div>';
      }
    }

    async function connectSsh(assetId) {
      const asset = connectionAssets.find(a => a.id === assetId);
      if (!asset) return;
      const isTauri = typeof window.__TAURI__?.core?.invoke === 'function';
      if (!isTauri) { announce('SSH 需要桌面客户端'); return; }

      const existing = [...sessions.values()].find(s => s.asset.id === assetId);
      if (existing) {
        activateTab('session-' + existing.sessionId);
        return;
      }

      let password = '';
      let credentialId = null;
      let authMethod = asset.auth_method || 'Password';
      let privateKeyPath = asset.private_key_path || null;
      let passphrase = null;
      let passphraseCredId = null;

      if (authMethod === 'Password') {
        const credId = 'ssh-pw-' + asset.id;
        try {
          const status = await invokeBackend('get_credential_status', { id: credId });
          if (status.exists) credentialId = credId;
        } catch {}
        if (!credentialId) {
          const result = await openCredentialPrompt(asset);
          if (!result) return;
          password = result.password;
          if (result.savePassword) {
            try { await invokeBackend('save_credential', { id: credId, secret: password }); } catch {}
          }
        }
      } else if (authMethod === 'PrivateKey') {
        if (!privateKeyPath) privateKeyPath = '~/.ssh/id_ed25519';
        const credId = 'ssh-key-' + asset.id;
        try {
          const status = await invokeBackend('get_credential_status', { id: credId });
          if (status.exists) passphraseCredId = credId;
        } catch {}
        if (!passphraseCredId) {
          const result = await openCredentialPrompt(asset, {
            title: '输入私钥 Passphrase — ' + asset.name,
            label: 'Passphrase',
            placeholder: '留空表示无 passphrase',
            saveLabel: '保存 passphrase 到本地安全存储',
            note: '如果私钥有 passphrase 请输入，无 passphrase 可直接点连接。',
            allowEmpty: true
          });
          if (!result) return;
          passphrase = result.password;
          if (result.savePassword && passphrase) {
            try { await invokeBackend('save_credential', { id: credId, secret: passphrase }); } catch {}
          }
        }
      }

      let Terminal, FitAddon;
      try {
        ({ Terminal } = await import('../node_modules/@xterm/xterm/lib/xterm.mjs'));
        ({ FitAddon } = await import('../node_modules/@xterm/addon-fit/lib/addon-fit.mjs'));
      } catch (err) {
        announce('终端模块加载失败: ' + err.message);
        return;
      }

      const tab = document.createElement('button');
      tab.className = 'workspace-tab';
      tab.setAttribute('role', 'tab');
      tab.setAttribute('aria-selected', 'false');
      tab.dataset.tab = 'session-pending';
      tab.dataset.sessionTab = 'true';
      tab.innerHTML = escapeHtml(asset.name) + ' · SSH <span class="tab-close" title="关闭标签">×</span>';
      const filesTab = document.querySelector('[data-tab="files"]');
      filesTab.parentElement.insertBefore(tab, filesTab);

      const container = document.getElementById('terminalContainer');
      const termDiv = document.createElement('div');
      termDiv.style.cssText = 'display:none;height:100%;';
      container.appendChild(termDiv);

      const term = new Terminal({
        cursorBlink: true,
        fontSize: 14,
        fontFamily: 'Consolas, "Courier New", monospace',
        theme: getTerminalTheme()
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      term.open(termDiv);
      term.writeln('\x1b[36mmyshelltool SSH\x1b[0m — connecting to ' + asset.host + '...\r\n');

      activateTab('terminal');

      try {
        const result = await invokeBackend('ssh_connect', {
          host: asset.host,
          port: asset.port,
          username: asset.username,
          password,
          credential_id: credentialId,
          auth_method: authMethod,
          private_key_path: privateKeyPath,
          passphrase,
          passphrase_credential_id: passphraseCredId,
        });

        if (result.connected) {
          const sessionId = result.session_id;
          tab.dataset.tab = 'session-' + sessionId;
          tab.dataset.sessionId = sessionId;

          sessions.set(sessionId, { term, fit, termDiv, tab, asset, sessionId });

          term.onData(data => {
            const encoder = new TextEncoder();
            invokeBackend('ssh_write', { sessionId, data: Array.from(encoder.encode(data)) });
          });

          window.__TAURI__.core.getCurrentWindow().listen('ssh-output-' + sessionId, event => {
            if (event.payload && event.payload.length > 0) {
              const decoder = new TextDecoder();
              term.write(decoder.decode(new Uint8Array(event.payload)));
            } else {
              term.writeln('\x1b[31m\r\nConnection closed.\x1b[0m');
            }
          });

          switchToSession(sessionId);
          const allTabs = document.querySelectorAll('.workspace-tab');
          allTabs.forEach(t => t.setAttribute('aria-selected', String(t.dataset.tab === 'session-' + sessionId)));
          announce('已连接: ' + asset.name);
        } else {
          term.writeln('\x1b[31mConnection failed: ' + (result.error || 'unknown') + '\x1b[0m\r\n');
          term.dispose();
          termDiv.remove();
          tab.remove();
          activateTab('overview');
          announce('连接失败: ' + asset.name);
        }
      } catch (err) {
        term.writeln('\x1b[31mError: ' + escapeHtml(err.message) + '\x1b[0m\r\n');
        term.dispose();
        termDiv.remove();
        tab.remove();
        activateTab('overview');
      }
    }

    applyTheme(readStoredTheme());
    initializeBackendBridge();
    initTerminal();
    const storedAssetsState = readStoredAssetsState();
    assetPreferenceLocked = storedAssetsState === 'collapsed' || storedAssetsState === 'expanded';
    applyAssetsState(assetPreferenceLocked ? storedAssetsState === 'collapsed' : compactAssetsQuery.matches, false);

    themeToggle.addEventListener('click', () => {
      const currentTheme = document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
      const nextTheme = currentTheme === 'light' ? 'dark' : 'light';
      applyTheme(nextTheme);
      const theme = getTerminalTheme();
      sessions.forEach(s => { s.term.options.theme = theme; });
      announce('已切换到' + (nextTheme === 'light' ? '浅色' : '深色') + '主题');
    });

    assetToggle.addEventListener('click', () => {
      const collapsed = document.documentElement.dataset.assets !== 'collapsed';
      assetPreferenceLocked = true;
      applyAssetsState(collapsed);
      announce(collapsed ? '连接资产已收起，主工作区已扩展' : '连接资产已展开');
    });

    compactAssetsQuery.addEventListener('change', event => {
      if (!assetPreferenceLocked) applyAssetsState(event.matches, false);
    });

    window.addEventListener('resize', () => {
      if (activeSessionId) {
        const session = sessions.get(activeSessionId);
        if (session) { try { session.fit.fit(); } catch {} }
      }
    });

    document.querySelector('.tabbar').addEventListener('click', event => {
      const close = event.target.closest('.tab-close');
      const tab = event.target.closest('.workspace-tab');
      if (!tab) return;
      if (close) {
        event.stopPropagation();
        if (tab.dataset.sessionId) {
          const sessionId = tab.dataset.sessionId;
          invokeBackend('ssh_disconnect', { sessionId });
          removeSession(sessionId);
          announce('已断开并关闭：' + tab.textContent.replace('×', '').trim());
          return;
        }
        const wasActive = tab.getAttribute('aria-selected') === 'true';
        tab.hidden = true;
        const panel = document.querySelector('[data-panel="' + tab.dataset.tab + '"]');
        if (panel) panel.classList.remove('active');
        announce('已关闭标签：' + tab.textContent.replace('×', '').trim());
        if (wasActive) activateTab('overview');
        return;
      }
      activateTab(tab.dataset.tab);
    });

    document.addEventListener('click', event => {
      const tabTarget = event.target.closest('[data-tab-target]');
      const createAsset = event.target.closest('[data-asset-create]');
      const editAsset = event.target.closest('[data-asset-edit]');
      const modalTarget = event.target.closest('[data-modal]');
      const refreshRemote = event.target.closest('[data-refresh-remote-files]');
      const openRemoteDir = event.target.closest('[data-open-remote-dir]');
      const fileRename = event.target.closest('[data-file-rename]');
      const fileDelete = event.target.closest('[data-file-delete]');
      const fileEdit = event.target.closest('[data-file-edit]');
      const fileDownload = event.target.closest('[data-file-download]');
      const newDirBtn = event.target.closest('[data-create-remote-dir]');
      const uploadBtn = event.target.closest('[data-upload-file]');
      if (refreshRemote) refreshRemoteFiles().catch(err => announce('远程文件刷新失败：' + err.message));
      if (newDirBtn) createRemoteDir().catch(err => announce('创建目录失败：' + err.message));
      if (uploadBtn) document.getElementById('fileUploadInput')?.click();
      if (openRemoteDir) {
        const row = openRemoteDir.closest('[data-remote-path]');
        if (row) refreshRemoteFiles(row.dataset.remotePath).catch(err => announce('远程目录打开失败：' + err.message));
      }
      if (fileRename) renameRemoteFile(fileRename.dataset.fileRename).catch(err => announce('重命名失败：' + err.message));
      if (fileDelete) deleteRemoteFile(fileDelete.dataset.fileDelete, fileDelete.dataset.kind || 'file').catch(err => announce('删除失败：' + err.message));
      if (fileEdit) announce('远程编辑（Monaco Editor）将在 Phase D 实现');
      if (fileDownload) announce('文件下载将在 Phase C 实现');
      if (tabTarget) {
        if (tabTarget.dataset.tabTarget === 'terminal') {
          connectSsh(selectedAssetId);
        } else {
          activateTab(tabTarget.dataset.tabTarget);
        }
      }
      if (createAsset) openAssetEditor();
      if (editAsset) openAssetEditor(connectionAssets.find(item => item.id === selectedAssetId));
      if (modalTarget) openModal(modalTarget.dataset.modal);
      if (!event.target.closest('#contextMenu') && !event.target.closest('[data-host-action]')) contextMenu.classList.remove('open');
    });

    document.getElementById('connectionFilter').addEventListener('input', event => {
      const value = event.target.value.trim().toLowerCase();
      document.querySelectorAll('[data-host]').forEach(node => {
        node.style.display = node.dataset.host.toLowerCase().includes(value) ? '' : 'none';
      });
      announce(value ? '连接树筛选：' + value : '连接树筛选已清空');
    });

    document.getElementById('fileUploadInput')?.addEventListener('change', event => {
      const files = event.target.files;
      if (!files || !files.length) return;
      for (const file of files) {
        uploadFile(file).catch(err => announce('上传失败：' + err.message));
      }
      event.target.value = '';
    });

    const remoteFileList = document.getElementById('remoteFileList');
    if (remoteFileList) {
      remoteFileList.addEventListener('dragover', event => {
        event.preventDefault();
        remoteFileList.classList.add('drag-over');
      });
      remoteFileList.addEventListener('dragleave', () => {
        remoteFileList.classList.remove('drag-over');
      });
      remoteFileList.addEventListener('drop', event => {
        event.preventDefault();
        remoteFileList.classList.remove('drag-over');
        const files = event.dataTransfer?.files;
        if (!files || !files.length) return;
        for (const file of files) {
          uploadFile(file).catch(err => announce('上传失败：' + err.message));
        }
      });
    }

    connectionTree.addEventListener('click', event => {
      const node = event.target.closest('[data-host-action]');
      if (node) selectAsset(node.dataset.assetId);
    });

    connectionTree.addEventListener('dblclick', event => {
      const node = event.target.closest('[data-host-action]');
      if (node) connectSsh(node.dataset.assetId);
    });

    connectionTree.addEventListener('contextmenu', event => {
      const node = event.target.closest('[data-host-action]');
      if (!node) return;
      event.preventDefault();
      selectAsset(node.dataset.assetId);
      const item = connectionAssets.find(asset => asset.id === node.dataset.assetId);
      contextMenu.style.left = Math.min(event.clientX, window.innerWidth - 236) + 'px';
      contextMenu.style.top = Math.min(event.clientY, window.innerHeight - 260) + 'px';
      contextMenu.classList.add('open');
      announce('已打开 ' + item.name + ' 的右键菜单');
    });

    modalClose.addEventListener('click', closeModal);
    modalSecondary.addEventListener('click', () => {
      if (activeModal === 'hostKeyPrompt' && pendingHostKeyResolve) {
        pendingHostKeyResolve(false);
        pendingHostKeyResolve = null;
      }
      if (activeModal === 'passwordPrompt' && pendingPasswordResolve) {
        pendingPasswordResolve(null);
        pendingPasswordResolve = null;
      }
      closeModal();
    });
    modalPrimary.addEventListener('click', async () => {
      if (activeModal === 'hostKeyPrompt' && pendingHostKeyResolve) {
        pendingHostKeyResolve(true);
        pendingHostKeyResolve = null;
        closeModal();
        return;
      }
      if (activeModal === 'passwordPrompt' && pendingPasswordResolve) {
        const password = modalBody.querySelector('[data-ssh-password]')?.value || '';
        const allowEmpty = modalBody.dataset.allowEmpty === 'true';
        if (!password && !allowEmpty) { announce('密码不能为空'); return; }
        const savePassword = modalBody.querySelector('[data-save-password]')?.checked || false;
        pendingPasswordResolve({ password, savePassword });
        pendingPasswordResolve = null;
        closeModal();
        return;
      }
      if (activeModal === 'assetEditor') {
        await saveAssetEditor();
        return;
      }
      if (modalBody.querySelector('[data-sync-token]')) {
        await saveTokenConfig();
        return;
      }
      closeModal();
    });
    modalLayer.addEventListener('click', event => { if (event.target === modalLayer) closeModal(); });
    document.addEventListener('keydown', event => { if (event.key === 'Escape') { closeModal(); contextMenu.classList.remove('open'); } });

    document.querySelectorAll('[data-toggle-tunnel]').forEach(button => {
      button.addEventListener('click', () => {
        const row = button.closest('[data-tunnel-row]');
        const pill = row.querySelector('[data-status-pill]');
        const isRunning = pill.textContent.trim() === 'Running' || pill.textContent.trim() === 'Reconnecting';
        pill.classList.remove('running', 'warn', 'error', 'reconnecting');
        if (isRunning) {
          pill.textContent = 'Stopped';
          button.textContent = '启动';
          announce(row.cells[0].textContent + ' 已停止');
        } else {
          pill.textContent = 'Running';
          pill.classList.add('running');
          button.textContent = '停止';
          announce(row.cells[0].textContent + ' 已启动，健康检查正常');
        }
      });
    });

    document.querySelectorAll('.switch').forEach(toggle => {
      toggle.addEventListener('click', () => toggle.classList.toggle('on'));
    });

    const lastTab = (() => {
      try {
        return localStorage.getItem('myshelltool-active-tab');
      } catch {
        return null;
      }
    })();
    if (lastTab && document.querySelector('[data-panel="' + lastTab + '"]')) activateTab(lastTab);
