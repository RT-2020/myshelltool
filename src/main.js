const panels = [...document.querySelectorAll('[data-panel]')];
    const tabs = [...document.querySelectorAll('.workspace-tab')];
    const statusMessage = document.querySelector('[data-status-message]');
    const backendStatus = document.getElementById('backendStatus');
    const assetSource = document.getElementById('assetSource');
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

    async function invokeBackend(command, args = {}) {
      const tauriInvoke = window.__TAURI__?.core?.invoke;
      if (typeof tauriInvoke === 'function') {
        return tauriInvoke(command, args);
      }

      if (command === 'backend_status') {
        return { ready: true, mode: 'browser-preview' };
      }
      if (command === 'list_connection_assets') {
        return { source: 'browser-preview fallback', count: 8, assets: [] };
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
        body: `<p>token 仅写入本地安全存储。界面提交后只展示“已配置”或“未配置”。</p><label class="stack" style="margin-top: var(--space-3);"><span class="muted">Personal Access Token</span><input class="input" type="password" data-sync-token placeholder="粘贴 token，保存后立即隐藏" /></label><p class="muted" data-token-storage-status>本地安全存储：未配置</p><ul><li>不会写入 Git 仓库</li><li>不会同步到远程</li><li>可以随时清除并重新测试连接</li></ul>`,
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

    function applyAssetsState(collapsed, persist = true) {
      document.documentElement.dataset.assets = collapsed ? 'collapsed' : 'expanded';
      assetToggle.setAttribute('aria-expanded', String(!collapsed));
      assetToggle.setAttribute('aria-label', collapsed ? '展开连接资产' : '收起连接资产');
      assetToggle.title = collapsed ? '展开连接资产' : '收起连接资产';
      if (persist) storeAssetsState(collapsed);
    }

    function activateTab(id) {
      const targetTab = tabs.find(tab => tab.dataset.tab === id);
      if (targetTab) targetTab.hidden = false;
      tabs.forEach(tab => tab.setAttribute('aria-selected', String(tab.dataset.tab === id)));
      panels.forEach(panel => panel.classList.toggle('active', panel.dataset.panel === id));
      try {
        localStorage.setItem('myshelltool-active-tab', id);
      } catch {
        return;
      }
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
        renderBackendStatus(status);
        renderAssetSource(assets);
      } catch {
        backendStatus.textContent = '不可用';
        assetSource.textContent = 'fallback 未加载';
      }
    }

    function updateContext(node) {
      const status = node.dataset.status || 'connected';
      contextTitle.textContent = node.dataset.name;
      contextSubtitle.textContent = node.dataset.address + ' · ' + node.dataset.user + ' · ' + status;
      contextAddress.textContent = node.dataset.address;
      contextUser.textContent = node.dataset.user;
      contextTags.textContent = node.dataset.tags;
      contextLast.textContent = node.dataset.last;
      contextDot.className = 'dot' + (status === 'connected' ? ' running' : status === 'warning' ? ' warn' : '');
      announce('已选择连接：' + node.dataset.name);
    }

    function openModal(key) {
      const item = modals[key];
      if (!item) return;
      modalTitle.textContent = item.title;
      modalBody.innerHTML = item.body;
      modalPrimary.textContent = item.primary;
      modalSecondary.textContent = item.secondary;
      modalLayer.classList.add('open');
      modalLayer.setAttribute('aria-hidden', 'false');
      modalPrimary.focus();
      announce('已打开弹窗：' + item.title);
    }

    function closeModal() {
      modalLayer.classList.remove('open');
      modalLayer.setAttribute('aria-hidden', 'true');
      announce('弹窗已关闭');
    }

    async function saveTokenConfig() {
      const tokenInput = modalBody.querySelector('[data-sync-token]');
      const storageStatus = modalBody.querySelector('[data-token-storage-status]');
      const result = await invokeBackend('save_sync_settings', {
        settings: {
          enabled: true,
          endpoint: 'git@github.com:private/myshell-config.git',
          interval_minutes: 15
        },
        token: { token: tokenInput?.value || '' }
      });
      if (tokenInput) tokenInput.value = '';
      const configured = Boolean(result.token_status?.configured);
      if (storageStatus) storageStatus.textContent = '本地安全存储：' + (configured ? '已配置' : '未配置');
      announce('同步配置已保存，本地安全存储：' + (configured ? '已配置' : '未配置'));
    }

    applyTheme(readStoredTheme());
    initializeBackendBridge();
    const storedAssetsState = readStoredAssetsState();
    assetPreferenceLocked = storedAssetsState === 'collapsed' || storedAssetsState === 'expanded';
    applyAssetsState(assetPreferenceLocked ? storedAssetsState === 'collapsed' : compactAssetsQuery.matches, false);

    themeToggle.addEventListener('click', () => {
      const currentTheme = document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
      const nextTheme = currentTheme === 'light' ? 'dark' : 'light';
      applyTheme(nextTheme);
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

    document.querySelector('.tabbar').addEventListener('click', event => {
      const close = event.target.closest('.tab-close');
      const tab = event.target.closest('.workspace-tab');
      if (!tab) return;
      if (close) {
        event.stopPropagation();
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
      const modalTarget = event.target.closest('[data-modal]');
      if (tabTarget) activateTab(tabTarget.dataset.tabTarget);
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

    document.querySelectorAll('[data-host-action]').forEach(node => {
      node.addEventListener('click', () => {
        document.querySelectorAll('[data-host-action]').forEach(item => item.classList.remove('active'));
        node.classList.add('active');
        updateContext(node);
      });
      node.addEventListener('contextmenu', event => {
        event.preventDefault();
        document.querySelectorAll('[data-host-action]').forEach(item => item.classList.remove('active'));
        node.classList.add('active');
        updateContext(node);
        contextMenu.style.left = Math.min(event.clientX, window.innerWidth - 236) + 'px';
        contextMenu.style.top = Math.min(event.clientY, window.innerHeight - 260) + 'px';
        contextMenu.classList.add('open');
        announce('已打开 ' + node.dataset.name + ' 的右键菜单');
      });
    });

    modalClose.addEventListener('click', closeModal);
    modalSecondary.addEventListener('click', closeModal);
    modalPrimary.addEventListener('click', async () => {
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
