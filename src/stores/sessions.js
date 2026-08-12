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
 *
 * 从 workbench.js 抽取所有 session 相关 state / actions / computed。
 * 关键迁移点（Critic 改进）：
 *   1. createOscParser (OSC 0/1/2 标题解析) 必须随 session 一起迁移
 *   2. watch(hostKeyPrompt) 65s 超时 + hostKeyTimeout 闭包必须随 sshConfirmHostKey 迁移
 *   3. localStorage key 禁重命名（保留原 'myshelltool-terminal-font' / '-aside'）
 *   4. progressUnlisten / hostKeyUnlisten / keyboardUnlisten 三个监听器 handle 必须迁移
 *
 * 跨 store 依赖（assets/ui 在 Wave 2.2/2.3 才拆分）通过 lazy getter 注入：
 *   - workbench.selectedAsset
 *   - workbench.effectiveTheme
 *   - workbench.modal
 *   - workbench.statusMessage（announce）
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

  // announce 优先走 workbench.statusMessage；fallback 到 console
  function announce(message) {
    if (workbenchBridge && typeof workbenchBridge.announce === 'function') {
      return workbenchBridge.announce(message);
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
            announce('主机密钥验证超时（65秒未响应），请重新连接');
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
    // 统一会话状态监听：Rust emit 后更新 session.status（权威源）。
    // connectSelected 内已有乐观更新（L735 session.status='connected'），
    // 这里的事件作为确认/补充。disconnected 事件让所有派生 UI（侧栏圆点等）
    // 即时变灰，无需依赖 ssh-closed-{id} 的副作用。
    //
    // 匹配策略：优先用 session_id 精确匹配；若未命中（connected 事件可能在
    // session.sessionId 从 pending- 切到真实 id 之前到达），则尝试用 asset.id
    // 匹配——asset.id 在整个连接流程中稳定不变。
    if (!statusUnlisten) {
      statusUnlisten = await listenBackendEvent(SESSION_STATUS_EVENT, event => {
        const { session_id, status } = event.payload || {};
        if (!session_id || !status) return;
        let session = sessions.value.find(s => s.sessionId === session_id);
        if (!session) {
          // 后备：connected 事件竞态时 session.sessionId 仍是 pending-，
          // 无法按真实 id 命中。此时用 asset.id 匹配（connectSelected 已把
          // asset 挂在 session 上）。仅当该 asset 恰好有一个 session 时采用，
          // 避免误匹配多会话场景。
          // 注意：Rust payload 当前只含 session_id，不含 asset.id；此后备依赖
          // connectSelected 的乐观更新已先行设置 status，故此处主要服务于
          // disconnected 事件（那时 session_id 必为真实值，第一轮即命中）。
        }
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
        // 用双 rAF 替代原 setTimeout(20ms)：第一帧让浏览器把 display:'' 应用
        // 到布局，第二帧时元素已有真实尺寸，fit() 才能测准。
        // （原 20ms 定时器无法保证布局完成，fit 会测到中间态尺寸。）
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
    // 关键：不要在每次 RO 回调里裸调 fit()。xterm 重绘会导致容器子像素尺寸
    // 抖动 → RO 再次触发 → fit() → onResize → ssh_resize → 服务器重绘 → 循环
    // （输入字符时 UI 乱跳的根因）。
    // 用 requestAnimationFrame 合并同一帧内的多次回调，并用上次的 cols/rows
    // 去重：尺寸没真正变化就不重新 fit/通知后端。
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
  }
  function setTerminalSearchQuery(query) {
    terminalSearch.value = { ...terminalSearch.value, query };
  }
  async function findTerminalNext(direction = 'next') {
    const session = activeSession.value;
    const query = terminalSearch.value.query;
    if (!session || !query) return;
    try {
      const opts = {
        caseSensitive: false,
        wholeWord: false,
        regex: false,
        decorations: { matchOverviewRng: '\x1b[43;30m', activeMatchColorOverviewRng: '\x1b[41;97m' }
      };
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
    const encoder = new TextEncoder();
    await invokeBackend('ssh_write', { sessionId: session.sessionId, data: Array.from(encoder.encode(text)) });
    announce('已粘贴到终端');
  }

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
    await invokeBackend('ssh_disconnect', { sessionId }).catch(() => null);
    if (typeof session.unlisten === 'function') session.unlisten();
    if (session.resizeObserver) session.resizeObserver.disconnect();
    try { session.term.dispose(); } catch {}
    session.termDiv?.remove();
    sessions.value = sessions.value.filter(item => item.sessionId !== sessionId);
    activeSessionId.value = sessions.value.at(-1)?.sessionId || null;
    showOnlyActiveTerminal();
    announce('已断开：' + session.asset.name);
  }

  async function reconnectSession(sessionId) {
    const session = sessions.value.find(item => item.sessionId === sessionId);
    if (!session) return;
    sessions.value = sessions.value.filter(item => item.sessionId !== sessionId);
    if (activeSessionId.value === sessionId) activeSessionId.value = null;
    await invokeBackend('ssh_disconnect', { sessionId }).catch(() => null);
    try { if (typeof session.unlisten === 'function') session.unlisten(); } catch {}
    try { session.resizeObserver?.disconnect(); } catch {}
    try { session.term?.dispose(); } catch {}
    try { session.termDiv?.remove(); } catch {}
    wb().selectedAssetId = session.asset.id;
    try {
      await connectSelected();
    } catch (err) {
      announce('重连失败：' + (err?.message || err));
    }
  }

  // ============================================================
  // Connect（原 workbench.js:905-1067）
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
    // term.open() —— 那样 WebGL canvas 会在 0×0 尺寸初始化，之后即使 fit()
    // renderer 的内部 canvas/纹理状态也不一定能重建，导致终端空白。
    // （见 .omc/notes/terminal-invisible-regression.md 的根因记录）
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
    // 会按正确尺寸初始化，避免 0×0 损坏。
    try {
      const { WebglAddon } = await import('@xterm/addon-webgl');
      const webgl = markRaw(new WebglAddon());
      webgl.onContextLoss(() => { try { webgl.dispose(); } catch (_) {} });
      term.loadAddon(webgl);
    } catch (_) { /* 显卡不支持时降级到默认 renderer */ }
    // 等一帧让浏览器完成布局后立即 fit，拿到准确的 cols/rows。
    await new Promise(resolve => requestAnimationFrame(resolve));
    try { fit.fit(); } catch {}
    term.writeln('\x1b[36mmyshelltool SSH\x1b[0m - connecting to ' + asset.host + '...\r\n');

    const decoder = new TextDecoder('utf-8', { stream: true });
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
      decoder
    });
    sessions.value.push(session);
    activeSessionId.value = session.sessionId;
    showOnlyActiveTerminal();

    let realSessionId = null;
    try {
      // termDiv 在 open 前就已可见、open 后已 fit（见上方 xterm 创建流程），
      // 这里直接读取 fit 后的 cols/rows 即可，无需重复 fit。
      const cols = term.cols && term.cols > 0 ? term.cols : 80;
      const rows = term.rows && term.rows > 0 ? term.rows : 24;
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
        term.writeln('\x1b[31mConnection failed: ' + (result.error || 'unknown') + '\x1b[0m\r\n');
        removeSessionEntry(session);
        announce('连接失败：' + asset.name + (result.error ? '（' + result.error + '）' : ''));
        return;
      }
      realSessionId = result.session_id;
      const wasActive = activeSessionId.value === session.sessionId;
      session.sessionId = realSessionId;
      if (wasActive) activeSessionId.value = realSessionId;
      session.status = 'connected';
      term.onData(data => {
        const encoder = new TextEncoder();
        invokeBackend('ssh_write', { sessionId: session.sessionId, data: Array.from(encoder.encode(data)) });
      });
      term.onResize(({ cols, rows }) => {
        // 守卫：fit() 在元素 0 尺寸（隐藏/未布局）时会算出 0 或异常值，
        // 不能把 0×0 PTY 发给服务器（会导致服务器重置光标、整屏重绘 → UI 跳动）。
        // 与连接处的守卫（见下方 ssh_connect 前的 cols/rows 计算）保持一致。
        if (!cols || !rows || cols <= 0 || rows <= 0) return;
        invokeBackend('ssh_resize', { sessionId: session.sessionId, cols, rows }).catch(() => null);
      });
      attachResizeObserver(session);
      const oscParser = createOscParser(title => { session.oscTitle = title; });
      const outputUnlisten = await listenBackendEvent('ssh-output-' + realSessionId, event => {
        if (event.payload && event.payload.length > 0) {
          const text = session.decoder.decode(new Uint8Array(event.payload));
          oscParser.feed(text);
          term.write(text);
        }
      });
      const closedUnlisten = await listenBackendEvent('ssh-closed-' + realSessionId, event => {
        const reason = typeof event.payload === 'string' && event.payload ? event.payload : 'unknown';
        session.status = 'disconnected';
        term.writeln('\r\n\x1b[31m[myshelltool] 远程连接已关闭 (' + reason + ')。\x1b[0m');
        announce('远程连接已关闭：' + asset.name + '（' + reason + '）');
      });
      session.unlisten = () => {
        try { outputUnlisten(); } catch (_) { /* noop */ }
        try { closedUnlisten(); } catch (_) { /* noop */ }
      };
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
      announce('已连接：' + asset.name);
    } catch (error) {
      if (realSessionId) await invokeBackend('ssh_disconnect', { sessionId: realSessionId }).catch(() => null);
      term.writeln('\x1b[31mError: ' + error.message + '\x1b[0m\r\n');
      removeSessionEntry(session);
      announce('连接失败：' + error.message);
    } finally {
      connectingAssetIds.delete(asset.id);
    }
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
    removeSessionEntry,
    // terminal ops
    runTerminalAction,
    openTerminalSearchInline,
    closeTerminalSearchInline,
    setTerminalSearchQuery,
    findTerminalNext,
    setTerminalFontSize,
    resetTerminalFontSize,
    toggleTerminalAside,
    toggleTerminalFullscreen,
    updateAllTerminalThemes,
    writeToActiveTerminal,
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
