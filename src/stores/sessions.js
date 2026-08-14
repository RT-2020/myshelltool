import { defineStore } from 'pinia';
import { computed, effectScope, markRaw, reactive, ref, watch } from 'vue';
import {
  getTauriWindow,
  invokeBackend,
  isTauriRuntime,
  listenBackendEvent
} from '../services/backend.js';
import { buildTerminalOptions } from '../composables/useTerminalConfig.js';
import { useClipboard } from '../composables/useClipboard.js';
import { useAutoReconnect } from '../composables/useAutoReconnect.js';
import { detectDangerousCommand } from '../lib/dangerousCommands.js';
import { createDangerousPasteGuard, createNativePasteGuard } from '../lib/terminalGuards.js';
import { pickTerminalTheme } from '../lib/terminalThemes.js';

// 事件 channel 常量（原 workbench.js:12-14）
const TRANSFER_PROGRESS_EVENT = 'sftp-transfer-progress';
const HOST_KEY_VERIFY_EVENT = 'ssh-host-key-verify';
const KEYBOARD_INTERACTIVE_EVENT = 'ssh-keyboard-interactive';
// 统一会话状态事件：Rust 在 连接成功 / 远端关闭 / 用户断开 三处 emit。
// 前端在这里维护 session.status（唯一权威源），所有状态 UI 从它派生。
const SESSION_STATUS_EVENT = 'ssh-session-status';

// localStorage key（CRITICAL: do NOT rename — Critic 改进 3）
const TERMINAL_FONT_KEY = 'myshelltool-terminal-font';
const TERMINAL_ASIDE_KEY = 'myshelltool-terminal-aside';

function readStored(key) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/**
 * useSessionsStore — Wave 2 Step 2.1
 * 从 workbench.js 抽取所有 session 相关 state / actions / computed。
 * 关键迁移点（Critic 改进）：createOscParser 随 session 迁移；hostKeyPrompt
 * 65s 超时 watcher + hostKeyTimeout 闭包随 sshConfirmHostKey 迁移；localStorage
 * key 禁重命名；progress/hostKey/keyboard 三个 unlisten handle 必须迁移。
 * 跨 store 依赖（assets/ui）通过 lazy getter（attachWorkbench）注入。
 */
export const useSessionsStore = defineStore('sessions', () => {
  // ============================================================
  // State
  // ============================================================
  const sessions = ref([]);
  const activeSessionId = ref(null);
  const hostKeyPrompt = ref(null);
  const keyboardPrompt = ref(null);
  const terminalFontSize = ref(Number(readStored(TERMINAL_FONT_KEY)) || 14);
  const terminalAsideOpen = ref(readStored(TERMINAL_ASIDE_KEY) === 'open');
  const terminalSearch = ref({ open: false, query: '', direction: 'next', result: null });

  // 用于驱动终端主题更新（原 workbench.js:24，仅保留与终端主题相关的部分）
  const systemPrefersDark = ref(false);

  // ============================================================
  // Module-level closure（原 workbench.js:64-71）
  // ============================================================
  const connectingAssetIds = new Set();
  let terminalContainer = null;
  let terminalModules = null;
  let progressUnlisten = null;
  let hostKeyUnlisten = null;
  let keyboardUnlisten = null;
  let statusUnlisten = null;
  let resizeObserver = null;
  let hostKeyTimeout = null;

  // ============================================================
  // 跨 store 桥接（lazy）
  // ============================================================
  // workbench.selectedAsset / workbench.modal / workbench.effectiveTheme / announce
  // 在 store 实例化时注入；App.vue 通过 sessionsStore.attachWorkbench(store) 注册
  let workbenchBridge = null;
  function attachWorkbench(store) {
    workbenchBridge = store;
  }
  function wb() {
    if (!workbenchBridge) {
      throw new Error('sessions store: workbench bridge not attached. Call sessionsStore.attachWorkbench(workbenchStore) at App.vue init.');
    }
    return workbenchBridge;
  }

  // announce 优先走 workbench.statusMessage（notify：opts.level 决定是否进 toast 队列）；
  // fallback 到 console
  function announce(message, opts) {
    if (workbenchBridge && typeof workbenchBridge.announce === 'function') {
      return workbenchBridge.announce(message, opts);
    }
    // 桥未注入时（极少出现，如单元测试）直接打 log
    // eslint-disable-next-line no-console
    console.log('[sessions] announce:', message);
  }

  // ============================================================
  // Computed
  // ============================================================
  const activeSession = computed(() =>
    sessions.value.find(session => session.sessionId === activeSessionId.value) || null
  );
  // 历史别名：返回 sessions.length；workbench.js 仍 re-export 此符号以兼容 App.vue
  const activeSessions = computed(() => sessions.value.length);

  function effectiveTheme() {
    return wb().effectiveTheme;
  }

  // ============================================================
  // Listeners — 三个 unlisten 句柄（CRITICAL Critic 改进 2/4）
  // ============================================================
  // 用 detached effect scope 注册 hostKeyPrompt watcher + 立即触发的
  // ensureHostKeyListeners 调用。当 sessions store 被嵌套实例化（如
  // workbench setup 期间调用 useSessionsStore()）时，外层 effect scope
  // 还未激活，Vue 的 watch() 会因 activeEffect === null 而崩溃。
  // detached scope 把这些副作用从父 scope 解绑，避免 reactivity 报错。
  let _ensureHostKeyListeners = null;

  function ensureHostKeyListeners() {
    if (_ensureHostKeyListeners) return _ensureHostKeyListeners();
  }

  const sessionScope = effectScope(true);
  sessionScope.run(() => {
    _ensureHostKeyListeners = async function ensureHostKeyListeners() {
      if (typeof window === 'undefined') return;
      try {
        if (!hostKeyUnlisten) {
          hostKeyUnlisten = await listenBackendEvent(HOST_KEY_VERIFY_EVENT, event => {
            hostKeyPrompt.value = event.payload;
            wb().modal = { type: 'hostKeyVerify', asset: wb().selectedAsset };
          });
        }
        if (!keyboardUnlisten) {
          keyboardUnlisten = await listenBackendEvent(KEYBOARD_INTERACTIVE_EVENT, event => {
            keyboardPrompt.value = event.payload;
            wb().modal = { type: 'keyboardInteractive', asset: wb().selectedAsset };
          });
        }
      } catch (error) {
        // eslint-disable-next-line no-console
        console.warn('host key / keyboard listener registration deferred:', error.message);
      }
    };

    // 立即注册（原 workbench.js:94-96）
    if (typeof window !== 'undefined') {
      _ensureHostKeyListeners();
    }

    // hostKeyPrompt 65s 自动清理（与后端 60s 超时对齐 + 5s 缓冲）
    // CRITICAL Critic 改进 2：watcher + hostKeyTimeout 闭包必须随 sshConfirmHostKey 一起迁移
    watch(hostKeyPrompt, prompt => {
      if (hostKeyTimeout) {
        clearTimeout(hostKeyTimeout);
        hostKeyTimeout = null;
      }
      if (prompt) {
        hostKeyTimeout = setTimeout(() => {
          if (hostKeyPrompt.value) {
            hostKeyPrompt.value = null;
            wb().modal = { type: null, asset: null };
            announce('主机密钥验证超时（65秒未响应），请重新连接', { level: 'warn' });
          }
        }, 65000);
      }
    });
  });

  async function setupEventListeners() {
    if (!isTauriRuntime()) return;

    if (!progressUnlisten) {
      progressUnlisten = await listenBackendEvent(TRANSFER_PROGRESS_EVENT, event => {
        const { transfer_id, bytes_transferred, total_bytes } = event.payload || {};
        // updateTransferProgress 留在 workbench（transferQueue 在 Wave 2.2 才拆）
        if (workbenchBridge && typeof workbenchBridge.updateTransferProgress === 'function') {
          workbenchBridge.updateTransferProgress(transfer_id, bytes_transferred, total_bytes);
        }
      });
    }
    if (!hostKeyUnlisten) {
      hostKeyUnlisten = await listenBackendEvent(HOST_KEY_VERIFY_EVENT, event => {
        hostKeyPrompt.value = event.payload;
        wb().modal = { type: 'hostKeyVerify', asset: wb().selectedAsset };
      });
    }
    if (!keyboardUnlisten) {
      keyboardUnlisten = await listenBackendEvent(KEYBOARD_INTERACTIVE_EVENT, event => {
        keyboardPrompt.value = event.payload;
        wb().modal = { type: 'keyboardInteractive', asset: wb().selectedAsset };
      });
    }
    // 统一会话状态监听：Rust emit 后更新 session.status（权威源，乐观更新外
    // 的确认/补充；disconnected 让侧栏圆点等派生 UI 即时变灰）。
    if (!statusUnlisten) {
      statusUnlisten = await listenBackendEvent(SESSION_STATUS_EVENT, event => {
        const { session_id, status } = event.payload || {};
        if (!session_id || !status) return;
        // 按真实 session_id 匹配；connected 事件早于 pending→真实 id 切换到达时
        // 无法命中（此时 connectSelected 的乐观更新已先行设置 status，无需补刀）。
        const session = sessions.value.find(s => s.sessionId === session_id);
        if (session) session.status = status;
      });
    }
  }

  async function disposeEventListeners() {
    if (hostKeyTimeout) { clearTimeout(hostKeyTimeout); hostKeyTimeout = null; }
    if (typeof progressUnlisten === 'function') { await progressUnlisten(); progressUnlisten = null; }
    if (typeof hostKeyUnlisten === 'function') { await hostKeyUnlisten(); hostKeyUnlisten = null; }
    if (typeof keyboardUnlisten === 'function') { await keyboardUnlisten(); keyboardUnlisten = null; }
    if (typeof statusUnlisten === 'function') { await statusUnlisten(); statusUnlisten = null; }
  }

  // ============================================================
  // OSC parser（CRITICAL Critic 改进 1 — 必须迁移）
  // ============================================================
  // OSC 0/1/2 标题序列解析（用于更新 Tab 标签）
  function createOscParser(onTitle) {
    let buffer = '';
    return {
      feed(text) {
        // 拆分：寻找 OSC 序列起始 (\x1b])，匹配到 ST (\x07 或 \x1b\\)
        buffer += text;
        // 仅处理最近一段缓冲，避免无限增长
        if (buffer.length > 4096) buffer = buffer.slice(-4096);
        const regex = /\x1b\](\d+);([^\x07\x1b]*)\x07|\x1b\](\d+);([^\x07\x1b]*)\x1b\\/g;
        let match;
        let lastIndex = 0;
        while ((match = regex.exec(buffer)) !== null) {
          const code = Number(match[1] || match[3]);
          const title = match[2] || match[4];
          if ((code === 0 || code === 1 || code === 2) && title) {
            onTitle(title.slice(0, 200));
          }
          lastIndex = regex.lastIndex;
        }
        if (lastIndex > 0) {
          // 保留未消费的尾部，但丢弃开头非序列部分
          const lastEscape = buffer.lastIndexOf('\x1b]', lastIndex);
          buffer = lastEscape >= 0 && lastEscape < lastIndex ? buffer.slice(lastEscape) : '';
        }
      }
    };
  }

  // ============================================================
  // Terminal mount / lifecycle
  // ============================================================
  function setTerminalContainer(element) {
    terminalContainer = element;
    showOnlyActiveTerminal();
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
      const isActive = session.sessionId === activeSessionId.value;
      session.termDiv.style.display = isActive ? '' : 'none';
      if (isActive) {
        // 双 rAF 替代原 setTimeout(20ms)：第一帧应用 display:'' 到布局，
        // 第二帧元素已有真实尺寸，fit() 才能测准（20ms 定时器测到中间态）。
        requestAnimationFrame(() => {
          requestAnimationFrame(() => {
            try { session.fit.fit(); } catch {}
            try { session.term.focus(); } catch {}
          });
        });
      }
    }
  }

  function setActiveSession(sessionId) {
    if (!sessions.value.some(session => session.sessionId === sessionId)) return;
    activeSessionId.value = sessionId;
    showOnlyActiveTerminal();
  }

  function attachResizeObserver(session) {
    if (typeof ResizeObserver === 'undefined') return;
    // 关键：不要在 RO 回调里裸调 fit()（xterm 重绘 → 容器子像素抖动 → RO 再触发
    // → fit → onResize → ssh_resize → 服务器重绘 → UI 乱跳的循环）。用 rAF 合并
    // 同一帧回调 + cols/rows 去重：尺寸没变就不重新 fit/通知后端。
    let rafId = 0;
    let lastCols = -1;
    let lastRows = -1;
    const runFit = () => {
      rafId = 0;
      try {
        session.fit.fit();
      } catch {}
      // 仅当 xterm 实际尺寸变化时才让 onResize 链路生效（见下方 onResize 守卫，
      // 这里只负责稳定测量，不再直接触发 ssh_resize）。
      const c = session.term.cols;
      const r = session.term.rows;
      if (c === lastCols && r === lastRows) return;
      lastCols = c;
      lastRows = r;
    };
    const observer = new ResizeObserver(() => {
      if (rafId) return;          // 本帧已排队，合并掉
      rafId = requestAnimationFrame(runFit);
    });
    observer.observe(session.termDiv);
    session.resizeObserver = observer;
  }

  function removeSessionEntry(session) {
    // 统一清理：取消自动重连定时器 + 标记手动关闭，防止 closed 事件/重连回调竞态
    session.autoReconnect?.cancel();
    session.manualDisconnect = true;
    try { session.searchResultsDisposable?.dispose(); } catch (_) { /* noop */ }
    try { session.search?.clearDecorations(); } catch (_) { /* noop */ }
    const idx = sessions.value.indexOf(session);
    if (idx >= 0) sessions.value.splice(idx, 1);
    if (activeSessionId.value === session.sessionId) {
      activeSessionId.value = sessions.value.at(-1)?.sessionId || null;
    }
    try { session.term.dispose(); } catch {}
    session.termDiv?.remove();
  }

  // ============================================================
  // Theme
  // ============================================================
  function getTerminalTheme() {
    return pickTerminalTheme(effectiveTheme().value);
  }

  // 主题变更时同步 xterm theme（原 workbench.js:484-489）
  function updateAllTerminalThemes() {
    const next = getTerminalTheme();
    for (const session of sessions.value) {
      try { session.term.options.theme = next; } catch {}
    }
  }

  // ============================================================
  // Font size（原 workbench.js:492-516）
  // ============================================================
  function applyTerminalFontSizeAll() {
    for (const session of sessions.value) {
      try {
        session.term.options.fontSize = terminalFontSize.value;
        if (session.term.rows > 0) session.term.refresh(0, session.term.rows - 1);
        session.fit.fit();
      } catch {}
    }
  }

  function setTerminalFontSize(delta) {
    const next = Math.min(28, Math.max(9, terminalFontSize.value + delta));
    if (next === terminalFontSize.value) return;
    terminalFontSize.value = next;
    localStorage.setItem(TERMINAL_FONT_KEY, String(next));
    applyTerminalFontSizeAll();
    announce('终端字号：' + next + 'px');
  }

  function resetTerminalFontSize() {
    if (terminalFontSize.value === 14) return;
    terminalFontSize.value = 14;
    localStorage.setItem(TERMINAL_FONT_KEY, '14');
    applyTerminalFontSizeAll();
    announce('终端字号已重置：14px');
  }

  // ============================================================
  // Terminal aside（原 workbench.js:555-562）
  // ============================================================
  function toggleTerminalAside() {
    terminalAsideOpen.value = !terminalAsideOpen.value;
    localStorage.setItem(TERMINAL_ASIDE_KEY, terminalAsideOpen.value ? 'open' : 'closed');
    setTimeout(() => {
      const session = activeSession.value;
      if (session) try { session.fit.fit(); } catch {}
    }, 60);
  }

  // ============================================================
  // Inline search（原 workbench.js:519-552）
  // ============================================================
  function openTerminalSearchInline() {
    const session = activeSession.value;
    if (!session) {
      announce('请先连接主机');
      return;
    }
    terminalSearch.value = { open: true, query: terminalSearch.value.query || '', direction: 'next', result: null };
  }
  function closeTerminalSearchInline() {
    terminalSearch.value = { ...terminalSearch.value, open: false };
    const session = activeSession.value;
    try { session?.search?.clearDecorations(); } catch (_) { /* noop */ }
  }
  function setTerminalSearchQuery(query) {
    terminalSearch.value = { ...terminalSearch.value, query };
    if (!query) {
      const session = activeSession.value;
      try { session?.search?.clearDecorations(); } catch (_) { /* noop */ }
    }
  }
  // per-session 搜索选项（searchOpts 在 session 创建时初始化）；合并而非整体替换
  function setTerminalSearchOpts(sessionId, opts) {
    const session = sessions.value.find(item => item.sessionId === sessionId);
    if (!session || !session.searchOpts) return;
    Object.assign(session.searchOpts, opts || {});
  }
  async function findTerminalNext(direction = 'next') {
    const session = activeSession.value;
    const query = terminalSearch.value.query;
    if (!session || !query) return;
    const opts = {
      caseSensitive: session.searchOpts?.caseSensitive ?? false,
      wholeWord: session.searchOpts?.wholeWord ?? false,
      regex: session.searchOpts?.regex ?? false,
      // 装饰色必须是十六进制色值（@xterm/addon-search decorations 契约）
      decorations: { matchOverviewRuler: '#f5c518', activeMatchColorOverviewRuler: '#ef4444' }
    };
    try {
      const found = direction === 'prev'
        ? await session.search.findPrevious(query, opts)
        : await session.search.findNext(query, opts);
      terminalSearch.value = { ...terminalSearch.value, result: found ? 'hit' : 'miss' };
      if (!found) announce('未找到匹配：' + query);
    } catch {
      terminalSearch.value = { ...terminalSearch.value, result: 'miss' };
    }
  }

  // ============================================================
  // Clipboard / write
  // ============================================================
  const { copy: clipboardCopy, paste: clipboardPaste } = useClipboard();

  async function copyTerminalSelection(session) {
    const selection = session.term.getSelection();
    if (!selection) {
      announce('终端无选中内容');
      return;
    }
    const ok = await clipboardCopy(selection);
    if (ok) announce('已复制终端选中文本');
    else announce('剪贴板不可用');
  }

  async function pasteToTerminal(session) {
    const text = await clipboardPaste();
    if (!text) { announce('剪贴板为空或不可用'); return; }
    // 统一走危险粘贴守卫：命中弹确认、未命中直接写入
    requestDangerousPaste(session.sessionId, text);
  }

  // ============================================================
  // 危险粘贴守卫（单一入口：命令面板 / 右键菜单 / 原生 Ctrl+V；状态机在 terminalGuards.js）
  // ============================================================
  const dangerousPastePrompt = reactive({ open: false, sessionId: null, command: '', matchedPattern: '' });
  const pasteGuard = createDangerousPasteGuard({
    getSession: sessionId => sessions.value.find(item => item.sessionId === sessionId) || null,
    prompt: dangerousPastePrompt,
    detect: detectDangerousCommand
  });

  function requestDangerousPaste(sessionId, text) { return pasteGuard.request(sessionId, text); }
  function approveDangerousPaste(allowedPattern) { pasteGuard.approve(allowedPattern); }
  function cancelDangerousPaste() { pasteGuard.cancel(); }

  async function writeToActiveTerminal(text) {
    if (!text) return;
    const sessionId = activeSessionId.value;
    if (!sessionId) { announce('当前无活跃会话'); return; }
    const encoder = new TextEncoder();
    await invokeBackend('ssh_write', { sessionId, data: Array.from(encoder.encode(text)) });
  }

  // ============================================================
  // Fullscreen（原 workbench.js:1208-1231）
  // ============================================================
  async function toggleTerminalFullscreen() {
    const win = getTauriWindow();
    if (win && typeof win.setFullscreen === 'function') {
      try {
        let isFull = false;
        try { isFull = await win.isFullscreen(); } catch { /* 读权限失败时假设 false */ }
        await win.setFullscreen(!isFull);
        const session = activeSession.value;
        if (session) setTimeout(() => { try { session.fit.fit(); } catch {} }, 120);
        announce(isFull ? '已退出全屏' : '已进入全屏');
        return;
      } catch (error) {
        announce('全屏切换失败：' + error.message);
        return;
      }
    }
    // Fallback：浏览器预览模式
    document.documentElement.dataset.terminalFullscreen = document.documentElement.dataset.terminalFullscreen === 'true' ? 'false' : 'true';
    const session = activeSession.value;
    if (session) setTimeout(() => { try { session.fit.fit(); } catch {} }, 30);
    announce(document.documentElement.dataset.terminalFullscreen === 'true' ? '终端已全屏（预览模式）' : '终端已退出全屏');
  }

  // ============================================================
  // Run terminal action dispatcher
  // ============================================================
  function runTerminalAction(action) {
    if (action === 'connect') {
      connectSelected();
      return;
    }
    // 字号是全局设置，空态（无活跃会话）也放行
    if (action === 'font-inc') {
      setTerminalFontSize(1);
      return;
    }
    if (action === 'font-dec') {
      setTerminalFontSize(-1);
      return;
    }
    if (action === 'font-reset') {
      resetTerminalFontSize();
      return;
    }
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
      openTerminalSearchInline();
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
    if (action === 'reconnect') {
      reconnectSession(session.sessionId);
      return;
    }
    if (action === 'toggle-aside') {
      toggleTerminalAside();
      return;
    }
  }

  // ============================================================
  // Host key / keyboard resolve
  // ============================================================
  async function resolveHostKeyPrompt(requestId, accepted) {
    try {
      await invokeBackend('ssh_confirm_host_key', { requestId, accepted });
    } catch (error) {
      announce('主机密钥响应失败：' + error.message);
    }
    hostKeyPrompt.value = null;
    if (!accepted) wb().modal = { type: null, asset: null };
  }

  async function resolveKeyboardPrompt(requestId, responses) {
    try {
      await invokeBackend('ssh_keyboard_response', { requestId, responses });
    } catch (error) {
      announce('键盘交互响应失败：' + error.message);
    }
    keyboardPrompt.value = null;
  }

  // ============================================================
  // Disconnect / reconnect
  // ============================================================
  async function disconnectSession(sessionId) {
    const session = sessions.value.find(item => item.sessionId === sessionId);
    if (!session) return;
    // 用户主动断开：先标记 manualDisconnect，后端 closed 事件到达时不再触发自动重连
    session.manualDisconnect = true;
    session.autoReconnect?.cancel();
    await invokeBackend('ssh_disconnect', { sessionId }).catch(() => null);
    if (typeof session.unlisten === 'function') session.unlisten();
    if (session.resizeObserver) session.resizeObserver.disconnect();
    try { session.searchResultsDisposable?.dispose(); } catch (_) { /* noop */ }
    try { session.term.dispose(); } catch {}
    session.termDiv?.remove();
    sessions.value = sessions.value.filter(item => item.sessionId !== sessionId);
    activeSessionId.value = sessions.value.at(-1)?.sessionId || null;
    showOnlyActiveTerminal();
    announce('已断开：' + session.asset.name);
  }

  // 重连：复用同一 session/termDiv 重走 ssh_connect（不销毁）；手动/自动重连共用
  async function reconnectSession(sessionId) {
    const session = sessions.value.find(item => item.sessionId === sessionId);
    if (!session) return;
    session.autoReconnect?.cancel();
    session.manualDisconnect = false; // 重连是主动行为，不算用户断开
    const ok = await attachSessionStream(session);
    if (ok) session.reconnectAttempt = 0;
  }

  // ============================================================
  // Connect（原 workbench.js:905-1067）
  // 拆两步：connectSelected（创建终端 + session）与 attachSessionStream
  // （复用已有 session 走 ssh_connect + 事件接线）。自动/手动重连共用后者，
  // 保持会话与终端不销毁。
  // ============================================================
  async function connectSelected() {
    const asset = wb().selectedAsset;
    if (!asset) return;
    // 双击竞态守卫
    if (connectingAssetIds.has(asset.id)) {
      wb().setTab('terminal');
      announce('正在连接：' + asset.name);
      return;
    }
    if (!isTauriRuntime()) {
      wb().setTab('terminal');
      announce('SSH 需要桌面客户端：' + asset.name);
      return;
    }
    if (asset.auth_method === 'Password' && !asset.credential_id) {
      announce('该连接未保存密码：请编辑连接，填写密码后再连接');
      wb().modal = { type: 'assetEditor', asset };
      return;
    }
    if (asset.auth_method === 'PrivateKey' && !asset.private_key_path) {
      announce('该连接未配置私钥路径：请编辑连接');
      wb().modal = { type: 'assetEditor', asset };
      return;
    }
    await ensureHostKeyListeners();
    wb().setTab('terminal');
    announce('正在连接：' + asset.name);
    connectingAssetIds.add(asset.id);
    try {
      await ensureTerminalModules();
    } catch (error) {
      announce('终端模块加载失败：' + error.message);
      connectingAssetIds.delete(asset.id);
      return;
    }
    if (!terminalContainer) {
      connectingAssetIds.delete(asset.id);
      throw new Error('terminal container is not mounted');
    }

    const termDiv = document.createElement('div');
    // 关键修复：termDiv 一开始就可见、占满容器。绝不能在 display:none 上调
    // term.open()（WebGL canvas 会 0×0 初始化，fit 后也不重建，终端空白）。
    termDiv.style.cssText = 'display:block;width:100%;height:100%;';
    terminalContainer.appendChild(termDiv);

    const term = markRaw(new terminalModules.Terminal(buildTerminalOptions({
      fontSize: terminalFontSize.value,
      themeMode: effectiveTheme().value
    })));
    const fit = markRaw(new terminalModules.FitAddon());
    const search = markRaw(new terminalModules.SearchAddon());
    term.loadAddon(fit);
    term.loadAddon(search);
    // 可选 addon：URL 可点击。失败静默回退
    try {
      const { WebLinksAddon } = await import('@xterm/addon-web-links');
      term.loadAddon(markRaw(new WebLinksAddon()));
    } catch (_) { /* addon 不可用时降级 */ }
    // 在已可见的 termDiv 上 open（元素此刻有真实尺寸）。
    term.open(termDiv);
    // WebGL addon 在 open 之后加载：此时 termDiv 已有真实尺寸，WebGL canvas
    // 会按正确尺寸初始化，避免 0×0 损坏。降级时提示用户（info 级 toast）。
    try {
      const { WebglAddon } = await import('@xterm/addon-webgl');
      const webgl = markRaw(new WebglAddon());
      webgl.onContextLoss(() => {
        try { webgl.dispose(); } catch (_) { /* noop */ }
        announce('显卡渲染不可用，已降级为软件渲染', { level: 'info' });
      });
      term.loadAddon(webgl);
    } catch (_) {
      announce('显卡渲染不可用，已降级为软件渲染', { level: 'info' });
    }
    // 等一帧让浏览器完成布局后立即 fit，拿到准确的 cols/rows。
    await new Promise(resolve => requestAnimationFrame(resolve));
    try { fit.fit(); } catch {}
    term.writeln('\x1b[36mmyshelltool SSH\x1b[0m - connecting to ' + asset.host + '...\r\n');

    // 用 reactive() 包裹 session 对象：connectSelected 后续会通过本地 session
    // 变量多次修改其属性（status / sessionId / oscTitle / unlisten 等）。若 push
    // 的是普通对象，本地引用指向【原始对象】，对其属性的赋值不经过代理 set trap，
    // 不触发响应式更新——这曾导致终端区域 status 永远停在 'connecting'（侧栏圆点
    // 靠 computed 重算碰巧更新，但终端组件的细粒度依赖收不到通知）。
    // reactive() 让本地 session 引用本身成为代理，所有属性变更都可靠触发更新。
    // term/fit/search 已 markRaw，reactive 不会再深代理它们。
    const session = reactive({
      sessionId: 'pending-' + asset.id + '-' + Date.now(),
      asset,
      term,
      fit,
      search,
      termDiv,
      unlisten: null,
      resizeObserver: null,
      status: 'connecting',
      oscTitle: '',
      connectError: null,
      manualDisconnect: false,
      reconnectAttempt: 0,
      reconnectTotal: 0,
      searchOpts: reactive({ caseSensitive: false, regex: false, wholeWord: false }),
      searchMatch: { index: 0, total: 0 },
      allowedPastePatterns: new Set(),
      decoder: new TextDecoder('utf-8', { stream: true })
    });
    // 每个 session 一个自动重连实例（退避 1s/2s/5s/15s 共 4 次）
    session.autoReconnect = useAutoReconnect({
      onAttempt: ({ attempt, total, delay }) => {
        session.reconnectAttempt = attempt;
        session.reconnectTotal = total;
        announce(`连接断开，${Math.round(delay / 1000)} 秒后重连 (${attempt}/${total})`, { level: 'warn' });
      },
      onExhausted: () => {
        session.status = 'error';
        session.connectError = '自动重连失败，请手动重连';
        announce('自动重连失败：' + session.asset.name + '，请手动重连', { level: 'error' });
      }
    });
    // 原生 Ctrl+V 粘贴守卫：markRaw(term) 之后挂 handler；闭包经 getSessionId
    // 取 session 当前 sessionId（重连期间 sessionId 会切换，不能写死初始值）。
    term.attachCustomKeyEventHandler(createNativePasteGuard({
      getSessionId: () => session.sessionId,
      requestDangerousPaste,
      announce
    }));
    // 搜索计数订阅：findNext/findPrevious 结果变化时更新 session.searchMatch
    session.searchResultsDisposable = search.onDidChangeResults(({ resultIndex, resultCount }) => {
      session.searchMatch = { index: resultIndex, total: resultCount };
    });

    sessions.value.push(session);
    activeSessionId.value = session.sessionId;
    showOnlyActiveTerminal();

    // 输入/尺寸/观察器与连接状态无关，创建即挂；重连复用同一 session 无需重挂
    term.onData(data => {
      const encoder = new TextEncoder();
      // catch null：pending- 占位 id 期间（重连中）键入会失败，静默丢弃
      invokeBackend('ssh_write', { sessionId: session.sessionId, data: Array.from(encoder.encode(data)) }).catch(() => null);
    });
    term.onResize(({ cols, rows }) => {
      // 守卫：fit() 在元素 0 尺寸（隐藏/未布局）时会算出 0 或异常值，
      // 不能把 0×0 PTY 发给服务器（会导致服务器重置光标、整屏重绘 → UI 跳动）。
      // 与连接处的守卫（见 attachSessionStream 的 cols/rows 计算）保持一致。
      if (!cols || !rows || cols <= 0 || rows <= 0) return;
      invokeBackend('ssh_resize', { sessionId: session.sessionId, cols, rows }).catch(() => null);
    });
    attachResizeObserver(session);

    try {
      const ok = await attachSessionStream(session);
      if (!ok) return; // 失败已置 status='error' + connectError，错误卡片展示
    } finally {
      connectingAssetIds.delete(asset.id);
    }
  }

  // 对已有 session 建立 SSH 连接 + 事件接线（首次连接 / 自动重连 / 手动重连共用）。
  // 复用 termDiv/term 不销毁；失败返回 false 并置 status='error' + connectError。
  async function attachSessionStream(session) {
    const asset = session.asset;
    // 清理上一次连接的监听（重连时旧 ssh-output-/ssh-closed- 监听必须先解绑）
    if (typeof session.unlisten === 'function') {
      try { session.unlisten(); } catch (_) { /* noop */ }
      session.unlisten = null;
    }
    const prevRealId = String(session.sessionId).startsWith('pending-') ? null : session.sessionId;
    session.sessionId = 'pending-' + asset.id + '-' + Date.now();
    if (prevRealId) await invokeBackend('ssh_disconnect', { sessionId: prevRealId }).catch(() => null);
    session.status = 'connecting';
    session.connectError = null;
    const cols = session.term.cols && session.term.cols > 0 ? session.term.cols : 80;
    const rows = session.term.rows && session.term.rows > 0 ? session.term.rows : 24;
    let realSessionId = null;
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
        passphraseCredentialId: asset.passphrase_credential_id || null,
        cols,
        rows
      });
      if (!result.connected) {
        // 失败保留 session：status='error' + connectError，错误卡片展示后可重试/编辑/关闭
        session.status = 'error';
        session.connectError = result.error || 'unknown';
        session.term.writeln('\x1b[31mConnection failed: ' + (result.error || 'unknown') + '\x1b[0m\r\n');
        announce('连接失败：' + asset.name + (result.error ? '（' + result.error + '）' : ''), { level: 'error' });
        return false;
      }
      realSessionId = result.session_id;
      // 竞态守卫：连接期间会话可能已被 cancelConnect/removeSessionEntry 移除，
      // 成功后立即断开新会话，避免后端遗留孤儿 SSH 会话（无监听器可清理）。
      if (!sessions.value.includes(session)) {
        await invokeBackend('ssh_disconnect', { sessionId: realSessionId }).catch(() => null);
        return false;
      }
      const wasActive = activeSessionId.value === session.sessionId;
      session.sessionId = realSessionId;
      if (wasActive) activeSessionId.value = realSessionId;
      session.status = 'connected';
      await registerSessionStream(session);
      showOnlyActiveTerminal();
      // 更新对应 asset 的 last_connected（纯显示元数据，无竞态——不写 status，
      // 运行时连接态一律从 session.status 派生）。workbench bridge 暴露
      // assets() 返回 assetsStore.assets 数组。
      try {
        const assetsList = workbenchBridge && typeof workbenchBridge.assets === 'function'
          ? workbenchBridge.assets()
          : null;
        const target = Array.isArray(assetsList) ? assetsList.find(a => a && a.id === asset.id) : null;
        if (target) target.last_connected = new Date().toLocaleString('zh-CN');
      } catch (_) { /* 显示元数据更新失败不影响连接 */ }
      announce('已连接：' + asset.name, { level: 'success' });
      return true;
    } catch (error) {
      if (realSessionId) await invokeBackend('ssh_disconnect', { sessionId: realSessionId }).catch(() => null);
      session.status = 'error';
      session.connectError = error.message;
      session.term.writeln('\x1b[31mError: ' + error.message + '\x1b[0m\r\n');
      announce('连接失败：' + error.message, { level: 'error' });
      return false;
    }
  }

  // 注册 output / closed 监听（每次连接建立时调用；decoder/oscParser 新建，
  // 避免上一次连接遗留的流式解码状态污染新连接）。
  async function registerSessionStream(session) {
    const { asset, term } = session;
    const realSessionId = session.sessionId;
    const decoder = new TextDecoder('utf-8', { stream: true });
    session.decoder = decoder;
    const oscParser = createOscParser(title => { session.oscTitle = title; });
    const outputUnlisten = await listenBackendEvent('ssh-output-' + realSessionId, event => {
      if (event.payload && event.payload.length > 0) {
        const text = decoder.decode(new Uint8Array(event.payload));
        oscParser.feed(text);
        term.write(text);
      }
    });
    const closedUnlisten = await listenBackendEvent('ssh-closed-' + realSessionId, event => {
      const reason = typeof event.payload === 'string' && event.payload ? event.payload : 'unknown';
      session.status = 'disconnected';
      term.writeln('\r\n\x1b[31m[myshelltool] 远程连接已关闭 (' + reason + ')。\x1b[0m');
      announce('远程连接已关闭：' + asset.name + '（' + reason + '）', { level: 'warn' });
      scheduleSessionReconnect(session, reason);
    });
    session.unlisten = () => {
      try { outputUnlisten(); } catch (_) { /* noop */ }
      try { closedUnlisten(); } catch (_) { /* noop */ }
    };
  }

  // 自动重连调度：仅对远端异常关闭（非 disconnected-by-user）触发；复用同一 session
  // 重走 ssh_connect。manualDisconnect / 会话已移除双守卫防与 cancelConnect 竞态。
  function scheduleSessionReconnect(session, reason) {
    if (session.manualDisconnect || !session.autoReconnect) return;
    if (reason === 'disconnected-by-user') return;
    const doReconnect = async () => {
      if (session.manualDisconnect || !sessions.value.includes(session)) return;
      const ok = await attachSessionStream(session);
      if (ok) {
        session.autoReconnect.reset();
        session.reconnectAttempt = 0;
        announce('已恢复连接：' + session.asset.name, { level: 'success' });
      } else if (!session.manualDisconnect && sessions.value.includes(session)) {
        // 本次失败 → 排队下一次尝试（onAttempt 更新计数，4 次后 onExhausted）
        session.autoReconnect.schedule(doReconnect);
      }
    };
    session.autoReconnect.schedule(doReconnect);
  }

  // 取消进行中的连接：仅 connecting 态可取消。不设自动超时——hostkey 等待有
  // 65s 后端流程，超时会误杀正常等待。
  async function cancelConnect(sessionId) {
    const session = sessions.value.find(item => item.sessionId === sessionId);
    if (!session || session.status !== 'connecting') return;
    session.manualDisconnect = true;
    session.autoReconnect?.cancel();
    const realId = String(session.sessionId).startsWith('pending-') ? null : session.sessionId;
    if (realId) await invokeBackend('ssh_disconnect', { sessionId: realId }).catch(() => null);
    removeSessionEntry(session);
    announce('已取消连接', { level: 'info' });
  }

  // 关闭错误卡片：这是失败会话唯一被 remove 的路径（区别于用户主动断开/关标签）
  function dismissSessionError(sessionId) {
    const session = sessions.value.find(item => item.sessionId === sessionId);
    if (!session) return;
    removeSessionEntry(session);
  }

  return {
    // state
    sessions,
    activeSessionId,
    hostKeyPrompt,
    keyboardPrompt,
    terminalFontSize,
    terminalAsideOpen,
    terminalSearch,
    dangerousPastePrompt,
    systemPrefersDark,
    // computed
    activeSession,
    activeSessions,
    // bridge
    attachWorkbench,
    // lifecycle
    setupEventListeners,
    disposeEventListeners,
    ensureHostKeyListeners,
    setTerminalContainer,
    setActiveSession,
    disconnectSession,
    reconnectSession,
    connectSelected,
    cancelConnect,
    dismissSessionError,
    removeSessionEntry,
    // terminal ops
    runTerminalAction,
    openTerminalSearchInline,
    closeTerminalSearchInline,
    setTerminalSearchQuery,
    setTerminalSearchOpts,
    findTerminalNext,
    setTerminalFontSize,
    resetTerminalFontSize,
    toggleTerminalAside,
    toggleTerminalFullscreen,
    updateAllTerminalThemes,
    writeToActiveTerminal,
    // 危险粘贴守卫（单一入口）
    requestDangerousPaste,
    approveDangerousPaste,
    cancelDangerousPaste,
    // host key / keyboard
    resolveHostKeyPrompt,
    resolveKeyboardPrompt,
    // exposed helpers（部分 internal 供 workbench 复用）
    createOscParser,
    applyTerminalFontSizeAll,
    attachResizeObserver,
    showOnlyActiveTerminal,
    ensureTerminalModules,
    getTerminalTheme,
    copyTerminalSelection,
    pasteToTerminal
  };
});
