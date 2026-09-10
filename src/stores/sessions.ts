import { defineStore } from 'pinia';
import { computed, effectScope, markRaw, reactive, ref, watch } from 'vue';
import type {
  HostKeyVerifyPayload,
  KeyboardInteractivePayload,
  ModalState,
  NormalizedConnectionAsset,
  NotifyOptions,
  SshConnectResult,
  TerminalSearchState,
  TransferProgressPayload
} from '@/types/domain';
import {
  invokeBackend,
  isTauriRuntime,
  listenBackendEvent
} from '../services/backend';
import { buildTerminalOptions } from '../composables/useTerminalConfig';
import { useClipboard } from '../composables/useClipboard';
import { useAutoReconnect } from '../composables/useAutoReconnect';
import { detectDangerousCommand } from '../lib/dangerousCommands';
import { createDangerousPasteGuard, createNativePasteGuard } from '../lib/terminalGuards';
import type { DangerousPastePrompt } from '../lib/terminalGuards';
import { pickTerminalTheme } from '../lib/terminalThemes';
import type { Terminal } from '@xterm/xterm';
import type { FitAddon } from '@xterm/addon-fit';
import type { SearchAddon } from '@xterm/addon-search';
import type { SerializeAddon } from '@xterm/addon-serialize';

// 事件 channel 常量（原 workbench.js:12-14）
const TRANSFER_PROGRESS_EVENT = 'sftp-transfer-progress';
const HOST_KEY_VERIFY_EVENT = 'ssh-host-key-verify';
const KEYBOARD_INTERACTIVE_EVENT = 'ssh-keyboard-interactive';
// 统一会话状态事件：Rust 在 连接成功 / 远端关闭 / 用户断开 三处 emit。
// 前端在这里维护 session.status（唯一权威源），所有状态 UI 从它派生。
const SESSION_STATUS_EVENT = 'ssh-session-status';

// localStorage key（CRITICAL: do NOT rename — Critic 改进 3）
const TERMINAL_FONT_KEY = 'myshelltool-terminal-font';
const TERMINAL_LINEHEIGHT_KEY = 'myshelltool-terminal-lineheight';

function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** 动态加载的 xterm 模块集（ensureTerminalModules 缓存，类型取自各包 d.ts）。 */
interface TerminalModules {
  Terminal: typeof import('@xterm/xterm').Terminal;
  FitAddon: typeof import('@xterm/addon-fit').FitAddon;
  SearchAddon: typeof import('@xterm/addon-search').SearchAddon;
  SerializeAddon: typeof import('@xterm/addon-serialize').SerializeAddon;
}

/** 每个 session 一个的自动重连控制器（useAutoReconnect 返回值的最小结构，不耦合其完整类型）。 */
interface AutoReconnectController {
  schedule(reconnectFn: (attempt: number) => void): void;
  cancel(): void;
  reset(): void;
}

/**
 * 会话条目（sessions 数组元素）。term/fit/search/serialize 经 markRaw，
 * reactive 只代理普通字段。fontSize/lineHeight/themeMode 是 adopt 路径的
 * 样式快照覆盖（createTerminalForAsset overrides 展开进来的运行时字段）。
 */
interface SessionEntry {
  sessionId: string;
  asset: NormalizedConnectionAsset;
  term: Terminal;
  fit: FitAddon;
  search: SearchAddon;
  serialize: SerializeAddon;
  termDiv: HTMLDivElement;
  unlisten: (() => Promise<void>) | null;
  resizeObserver: ResizeObserver | null;
  status: string;
  oscTitle: string;
  connectError: string | null;
  manualDisconnect: boolean;
  reconnectAttempt: number;
  reconnectTotal: number;
  searchOpts: { caseSensitive: boolean; regex: boolean; wholeWord: boolean };
  searchMatch: { index: number; total: number };
  allowedPastePatterns: Set<string>;
  decoder: TextDecoder;
  autoReconnect?: AutoReconnectController | null;
  searchResultsDisposable?: { dispose(): void } | null;
  fontSize?: number;
  lineHeight?: number;
  themeMode?: string;
}

/** createTerminalForAsset 的 overrides（adoptSession 跨窗口迁移传真实 sessionId/样式快照）。 */
interface SessionOverrides {
  sessionId?: string;
  status?: string;
  oscTitle?: string;
  fontSize?: number;
  lineHeight?: number;
  themeMode?: string;
}

/** adoptSession 入参（sessionHandoff 迁移协议的目标侧 payload）。 */
interface AdoptSessionPayload {
  sessionId?: string | null;
  assetId?: string | null;
  scrollback?: string | null;
  oscTitle?: string | null;
  style?: { fontSize?: number; lineHeight?: number } | null;
}

/** OSC 序列解析器（createOscParser 的返回契约：feed 拆 OSC 0/1/2 标题 + OSC 7 cwd）。 */
interface OscParser {
  feed(text: string): void;
}

/** sessions store 实际消费的 workbench bridge 最小结构（勿耦合完整 workbench store 类型）。 */
interface SessionsWorkbenchBridge {
  selectedAsset: NormalizedConnectionAsset | null;
  assets(): NormalizedConnectionAsset[] | null;
  effectiveTheme: string;
  modal: ModalState;
  selectedAssetId: string | null;
  selectAsset(id: string, announceSelection?: boolean): unknown;
  onSessionClosed?(assetId?: string | null): unknown;
  onSessionConnected?(assetId?: string | null): unknown;
  syncTerminalCwd?(assetId?: string | null, path?: string): unknown;
  announce(message: string, opts?: NotifyOptions): unknown;
  setTab(tab: string): unknown;
  updateTransferProgress?(transferId: string, transferred: number, total: number): unknown;
}

/**
 * useSessionsStore — Wave 2 Step 2.1
 * 从 workbench.js 抽取所有 session 相关 state / actions / computed。
 * 关键迁移点（Critic 改进）：createOscParser 随 session 迁移；hostKeyPrompt
 * 65s 超时 watcher + hostKeyTimeout 闭包随 sshConfirmHostKey 迁移；localStorage
 * key 禁重命名；progress/hostKey/keyboard 三个 unlisten handle 必须迁移。
 * 跨 store 依赖（assets/ui）通过 lazy getter（attachWorkbench）注入。
 *
 * 行数债务：本文件早已超出 Pinia store 500 行硬上限（见 docs/architecture-log.md
 * Baseline snapshot），v2.4 跨窗口会话迁移（createTerminalForAsset + adoptSession）
 * 以共用工厂控制增量；拆分仍是重构候选，勿继续堆叠无关功能。
 */
export const useSessionsStore = defineStore('sessions', () => {
  // ============================================================
  // State
  // ============================================================
  const sessions = ref<SessionEntry[]>([]);
  const activeSessionId = ref<string | null>(null);
  const hostKeyPrompt = ref<HostKeyVerifyPayload | null>(null);
  const keyboardPrompt = ref<KeyboardInteractivePayload | null>(null);
  // 字号：写入路径（setTerminalFontSize）clamp 9–28，读取路径同款校验——
  // 手改/外部写入的坏值（NaN/越界）不 clamp 会令 xterm 越界字号渲染异常。
  // 非有限数或空值（Number('')===0）回落默认 12，与下行行高的读取风格一致。
  const clampFontSize = (v: number) => Math.min(28, Math.max(9, v));
  const storedFontSize = Number(readStored(TERMINAL_FONT_KEY));
  const terminalFontSize = ref(Number.isFinite(storedFontSize) && storedFontSize > 0 ? clampFontSize(storedFontSize) : 12);
  // 行高（xterm lineHeight）：1.0 紧凑 ~ 2.0 宽松，默认 1.0；设置面板可调。
  const clampLineHeight = (v: number) => Math.min(2, Math.max(1, Math.round(v * 10) / 10));
  const storedLineHeight = Number(readStored(TERMINAL_LINEHEIGHT_KEY));
  const terminalLineHeight = ref(Number.isFinite(storedLineHeight) && storedLineHeight >= 1 ? clampLineHeight(storedLineHeight) : 1);
  const terminalSearch = ref<TerminalSearchState>({ open: false, query: '', direction: 'next', result: null });

  // 用于驱动终端主题更新（原 workbench.js:24，仅保留与终端主题相关的部分）
  const systemPrefersDark = ref(false);

  // ============================================================
  // Module-level closure（原 workbench.js:64-71）
  // ============================================================
  const connectingAssetIds = new Set<string>();
  let terminalContainer: HTMLDivElement | null = null;
  let terminalModules: TerminalModules | null = null;
  let progressUnlisten: TauriUnlistenFn | null = null;
  let hostKeyUnlisten: TauriUnlistenFn | null = null;
  let keyboardUnlisten: TauriUnlistenFn | null = null;
  let statusUnlisten: TauriUnlistenFn | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let hostKeyTimeout: ReturnType<typeof setTimeout> | null = null;

  // ============================================================
  // 跨 store 桥接（lazy）
  // ============================================================
  // workbench.selectedAsset / workbench.modal / workbench.effectiveTheme / announce
  // 在 store 实例化时注入；App.vue 通过 sessionsStore.attachWorkbench(store) 注册
  let workbenchBridge: SessionsWorkbenchBridge | null = null;
  function attachWorkbench(store: SessionsWorkbenchBridge) {
    workbenchBridge = store;
  }
  function wb(): SessionsWorkbenchBridge {
    if (!workbenchBridge) {
      throw new Error('sessions store: workbench bridge not attached. Call sessionsStore.attachWorkbench(workbenchStore) at App.vue init.');
    }
    return workbenchBridge;
  }

  // announce 优先走 workbench.statusMessage（notify：opts.level 决定是否进 toast 队列）；
  // fallback 到 console
  function announce(message: string, opts?: NotifyOptions) {
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
  // 历史别名：返回 sessions.length；workbench 仍 re-export 此符号以兼容 App.vue
  const activeSessions = computed(() => sessions.value.length);

  function effectiveTheme() {
    return wb().effectiveTheme;
  }

  // ============================================================
  // Listeners — 三个 unlisten 句柄（CRITICAL Critic 改进 2/4）
  // ============================================================
  // 跨窗口事件路由守卫（多 WebviewWindow / 每窗口独立 Pinia 实例）：
  // Rust 侧 app.emit 是全局广播，所有窗口都会收到同一份 host key /
  // keyboard 事件。只有「本窗口存在 status==='connecting' 的会话」时
  // 该事件才可能属于本窗口——否则静默忽略（有意路由守卫，非可忽略错误）。
  // 守卫对本窗口 connecting 会话恒通过，单窗口使用路径行为不变。
  function ownsConnectingSession(hostPort?: string | null) {
    const connecting = sessions.value.filter(s => s.status === 'connecting');
    if (!connecting.length) return false;
    if (!hostPort) {
      // keyboard 事件 payload 无 host 字段，无法精确路由：本窗口有
      // connecting 会话即认领。残留边界：两窗口同时 connecting（尤其同
      // 一 host）时无法区分归属，可能双弹框——已知边界，暂不修。
      return true;
    }
    // host key 事件按主机路由：payload.host_port 形如 "host:port"（Rust
    // HostKeyVerifyEvent 无 rename_all，序列化字段名即 host_port）。优先
    // host:port 全等（asset.port 经 normalizeAsset 默认 22），回退去端口
    // 等值比较。
    return connecting.some(s => {
      const a = s.asset;
      if (!a?.host) return false;
      if (a.port && hostPort === `${a.host}:${a.port}`) return true;
      return String(hostPort).replace(/:\d+$/, '') === a.host;
    });
  }

  // host key / keyboard 共用 handler：ensureHostKeyListeners 与
  // setupEventListeners 两个注册点引用同一份，路由守卫只写一处。
  function onHostKeyVerifyEvent(event: TauriEvent) {
    const payload = (event?.payload || null) as HostKeyVerifyPayload | null;
    if (!ownsConnectingSession(payload?.host_port)) return;
    hostKeyPrompt.value = payload;
    wb().modal = { type: 'hostKeyVerify', asset: wb().selectedAsset };
  }

  function onKeyboardInteractiveEvent(event: TauriEvent) {
    if (!ownsConnectingSession()) return;
    keyboardPrompt.value = event.payload as KeyboardInteractivePayload;
    wb().modal = { type: 'keyboardInteractive', asset: wb().selectedAsset };
  }

  // 用 detached effect scope 注册 hostKeyPrompt watcher + 立即触发的
  // ensureHostKeyListeners 调用。当 sessions store 被嵌套实例化（如
  // workbench setup 期间调用 useSessionsStore()）时，外层 effect scope
  // 还未激活，Vue 的 watch() 会因 activeEffect === null 而崩溃。
  // detached scope 把这些副作用从父 scope 解绑，避免 reactivity 报错。
  let _ensureHostKeyListeners: (() => Promise<void>) | null = null;

  function ensureHostKeyListeners(): Promise<void> | undefined {
    if (_ensureHostKeyListeners) return _ensureHostKeyListeners();
  }

  const sessionScope = effectScope(true);
  sessionScope.run(() => {
    _ensureHostKeyListeners = async function ensureHostKeyListeners() {
      if (typeof window === 'undefined') return;
      try {
        if (!hostKeyUnlisten) {
          hostKeyUnlisten = await listenBackendEvent(HOST_KEY_VERIFY_EVENT, onHostKeyVerifyEvent);
        }
        if (!keyboardUnlisten) {
          keyboardUnlisten = await listenBackendEvent(KEYBOARD_INTERACTIVE_EVENT, onKeyboardInteractiveEvent);
        }
      } catch (error) {
        // eslint-disable-next-line no-console
        console.warn('host key / keyboard listener registration deferred:', (error as Error).message);
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
        const { transfer_id, bytes_transferred, total_bytes } = (event.payload || {}) as TransferProgressPayload;
        // updateTransferProgress 留在 workbench（transferQueue 在 Wave 2.2 才拆）
        if (workbenchBridge && typeof workbenchBridge.updateTransferProgress === 'function') {
          workbenchBridge.updateTransferProgress(transfer_id, bytes_transferred, total_bytes);
        }
      });
    }
    if (!hostKeyUnlisten) {
      hostKeyUnlisten = await listenBackendEvent(HOST_KEY_VERIFY_EVENT, onHostKeyVerifyEvent);
    }
    if (!keyboardUnlisten) {
      keyboardUnlisten = await listenBackendEvent(KEYBOARD_INTERACTIVE_EVENT, onKeyboardInteractiveEvent);
    }
    // 统一会话状态监听：Rust emit 后更新 session.status（权威源，乐观更新外
    // 的确认/补充；disconnected 让侧栏圆点等派生 UI 即时变灰）。
    if (!statusUnlisten) {
      statusUnlisten = await listenBackendEvent(SESSION_STATUS_EVENT, event => {
        const { session_id, status } = (event.payload || {}) as { session_id?: string; status?: string };
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
  // OSC 0/1/2 标题序列解析（更新 Tab 标签）+ OSC 7（cwd 上报，文件面板跟随终端目录）
  function createOscParser(onTitle: (title: string) => void, onCwd?: (cwd: string) => void): OscParser {
    let buffer = '';
    // OSC 7 payload 形如 file://host/path 或 file:///path（路径可能带 %XX 编码）
    function cwdFromPayload(payload?: string): string | null {
      let raw = String(payload || '');
      if (raw.startsWith('file://')) {
        raw = raw.slice(7);
        const slash = raw.indexOf('/');
        if (slash < 0) return null;
        raw = raw.slice(slash);
      }
      if (!raw.startsWith('/')) return null;
      try {
        return decodeURIComponent(raw);
      } catch {
        return raw;
      }
    }
    return {
      feed(text) {
        // 拆分：寻找 OSC 序列起始 (\x1b])，匹配到 ST (\x07 或 \x1b\\)
        buffer += text;
        // 仅处理最近一段缓冲，避免无限增长
        if (buffer.length > 4096) buffer = buffer.slice(-4096);
        const regex = /\x1b\](\d+);([^\x07\x1b]*)\x07|\x1b\](\d+);([^\x07\x1b]*)\x1b\\/g;
        let match: RegExpExecArray | null;
        let lastIndex = 0;
        while ((match = regex.exec(buffer)) !== null) {
          const code = Number(match[1] || match[3]);
          const title = match[2] || match[4];
          if ((code === 0 || code === 1 || code === 2) && title) {
            onTitle(title.slice(0, 200));
          } else if (code === 7 && onCwd) {
            const cwd = cwdFromPayload(title);
            if (cwd) onCwd(cwd);
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
  function setTerminalContainer(element: HTMLDivElement | null) {
    terminalContainer = element;
    showOnlyActiveTerminal();
  }

  async function ensureTerminalModules(): Promise<TerminalModules> {
    if (terminalModules) return terminalModules;
    const [{ Terminal }, { FitAddon }, { SearchAddon }, { SerializeAddon }] = await Promise.all([
      import('@xterm/xterm'),
      import('@xterm/addon-fit'),
      import('@xterm/addon-search'),
      import('@xterm/addon-serialize')
    ]);
    terminalModules = { Terminal, FitAddon, SearchAddon, SerializeAddon };
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

  // 激活会话变化（点击/键盘切换标签、关闭后自动落到相邻会话）时，
  // 资产树同步选中对应资产（静默，不 announce）。
  // 已选中则跳过，避免重复触发 clearFileSelection 打断文件区选择。
  function syncAssetSelection(sessionId: string | null) {
    const target = sessions.value.find(session => session.sessionId === sessionId);
    if (
      target?.asset?.id &&
      workbenchBridge &&
      workbenchBridge.selectedAssetId !== target.asset.id &&
      typeof workbenchBridge.selectAsset === 'function'
    ) {
      workbenchBridge.selectAsset(target.asset.id, false);
    }
  }

  function setActiveSession(sessionId: string) {
    if (!sessions.value.some(session => session.sessionId === sessionId)) return;
    activeSessionId.value = sessionId;
    syncAssetSelection(sessionId);
    showOnlyActiveTerminal();
  }

  function attachResizeObserver(session: SessionEntry) {
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

  function removeSessionEntry(session: SessionEntry) {
    // 统一清理：取消自动重连定时器 + 标记手动关闭，防止 closed 事件/重连回调竞态
    session.autoReconnect?.cancel();
    session.manualDisconnect = true;
    try { session.searchResultsDisposable?.dispose(); } catch (_) { /* noop */ }
    try { session.search?.clearDecorations(); } catch (_) { /* noop */ }
    const idx = sessions.value.indexOf(session);
    if (idx >= 0) sessions.value.splice(idx, 1);
    if (activeSessionId.value === session.sessionId) {
      activeSessionId.value = sessions.value.at(-1)?.sessionId || null;
      if (activeSessionId.value) syncAssetSelection(activeSessionId.value);
      // tearoff / disconnect 后补刷 DOM 可见性，与 setActiveSession 行为对齐
      showOnlyActiveTerminal();
    }
    try { session.term.dispose(); } catch {}
    session.termDiv?.remove();
  }

  // ============================================================
  // Theme
  // ============================================================
  function getTerminalTheme() {
    // 历史：bridge.effectiveTheme 运行时是已解包 string，沿用的 .value 读取恒为
    // undefined（pickTerminalTheme 忽略参数恒返回 darkTheme，终端主题与 app 主题
    // 已解耦）。双重 cast 仅为通过类型检查，运行时行为与原 JS 一致。
    return pickTerminalTheme((effectiveTheme() as unknown as { value?: string }).value);
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

  function setTerminalFontSize(delta: number) {
    const next = Math.min(28, Math.max(9, terminalFontSize.value + delta));
    if (next === terminalFontSize.value) return;
    terminalFontSize.value = next;
    localStorage.setItem(TERMINAL_FONT_KEY, String(next));
    applyTerminalFontSizeAll();
    announce('终端字号：' + next + 'px');
  }

  function resetTerminalFontSize() {
    if (terminalFontSize.value === 12) return;
    terminalFontSize.value = 12;
    localStorage.setItem(TERMINAL_FONT_KEY, '12');
    applyTerminalFontSizeAll();
    announce('终端字号已重置：12px');
  }

  // ============================================================
  // Line height（设置面板可调，改变即热更新所有终端并持久化）
  // ============================================================
  function applyTerminalLineHeightAll() {
    for (const session of sessions.value) {
      try {
        session.term.options.lineHeight = terminalLineHeight.value;
        if (session.term.rows > 0) session.term.refresh(0, session.term.rows - 1);
        session.fit.fit();
      } catch {}
    }
  }

  function setTerminalLineHeight(value: number) {
    const next = clampLineHeight(Number(value));
    if (!Number.isFinite(next) || next === terminalLineHeight.value) return;
    terminalLineHeight.value = next;
    localStorage.setItem(TERMINAL_LINEHEIGHT_KEY, String(next));
    applyTerminalLineHeightAll();
    announce('终端行间距：' + next.toFixed(1));
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
  function setTerminalSearchQuery(query: string) {
    terminalSearch.value = { ...terminalSearch.value, query };
    if (!query) {
      const session = activeSession.value;
      try { session?.search?.clearDecorations(); } catch (_) { /* noop */ }
    }
  }
  // per-session 搜索选项（searchOpts 在 session 创建时初始化）；合并而非整体替换
  function setTerminalSearchOpts(sessionId: string, opts?: Record<string, boolean>) {
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

  async function copyTerminalSelection(session: SessionEntry) {
    const selection = session.term.getSelection();
    if (!selection) {
      announce('终端无选中内容');
      return;
    }
    const ok = await clipboardCopy(selection);
    if (ok) announce('已复制终端选中文本');
    else announce('剪贴板不可用');
  }

  async function pasteToTerminal(session: SessionEntry) {
    const text = await clipboardPaste();
    if (!text) { announce('剪贴板为空或不可用'); return; }
    // 统一走危险粘贴守卫：命中弹确认、未命中直接写入
    requestDangerousPaste(session.sessionId, text);
  }

  // ============================================================
  // 危险粘贴守卫（单一入口：命令面板 / 右键菜单 / 原生 Ctrl+V；状态机在 terminalGuards）
  // ============================================================
  const dangerousPastePrompt = reactive<DangerousPastePrompt>({ open: false, sessionId: null, command: '', matchedPattern: '' });
  const pasteGuard = createDangerousPasteGuard({
    getSession: sessionId => sessions.value.find(item => item.sessionId === sessionId) || null,
    prompt: dangerousPastePrompt,
    detect: detectDangerousCommand
  });

  function requestDangerousPaste(sessionId: string, text: string) { return pasteGuard.request(sessionId, text); }
  function approveDangerousPaste(allowedPattern: string) { pasteGuard.approve(allowedPattern); }
  function cancelDangerousPaste() { pasteGuard.cancel(); }

  async function writeToActiveTerminal(text: string) {
    if (!text) return;
    const sessionId = activeSessionId.value;
    if (!sessionId) { announce('当前无活跃会话'); return; }
    const encoder = new TextEncoder();
    await invokeBackend('ssh_write', { sessionId, data: Array.from(encoder.encode(text)) });
  }

  // ============================================================
  // Run terminal action dispatcher
  // ============================================================
  function runTerminalAction(action: string) {
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
    if (action === 'reconnect') {
      reconnectSession(session.sessionId);
      return;
    }
  }

  // ============================================================
  // Host key / keyboard resolve
  // ============================================================
  async function resolveHostKeyPrompt(requestId: string, accepted: boolean) {
    try {
      await invokeBackend('ssh_confirm_host_key', { requestId, accepted });
    } catch (error) {
      announce('主机密钥响应失败：' + (error as Error).message);
    }
    hostKeyPrompt.value = null;
    if (!accepted) wb().modal = { type: null, asset: null };
  }

  async function resolveKeyboardPrompt(requestId: string, responses: string[]) {
    try {
      await invokeBackend('ssh_keyboard_response', { requestId, responses });
    } catch (error) {
      announce('键盘交互响应失败：' + (error as Error).message);
    }
    keyboardPrompt.value = null;
  }

  // ============================================================
  // Disconnect / reconnect
  // ============================================================
  async function disconnectSession(sessionId: string) {
    const session = sessions.value.find(item => item.sessionId === sessionId);
    if (!session) return;
    // 用户主动断开：先标记 manualDisconnect，后端 closed 事件到达时不再触发自动重连
    session.manualDisconnect = true;
    session.autoReconnect?.cancel();
    await invokeBackend('ssh_disconnect', { sessionId }).catch(() => null);
    if (typeof session.unlisten === 'function') session.unlisten().catch(() => null);
    if (session.resizeObserver) session.resizeObserver.disconnect();
    try { session.searchResultsDisposable?.dispose(); } catch (_) { /* noop */ }
    try { session.term.dispose(); } catch {}
    session.termDiv?.remove();
    sessions.value = sessions.value.filter(item => item.sessionId !== sessionId);
    activeSessionId.value = sessions.value.at(-1)?.sessionId || null;
    showOnlyActiveTerminal();
    // 通知文件面板清空该资产的目录（不再停留已关闭资产的旧列表）
    try {
      workbenchBridge?.onSessionClosed?.(session.asset?.id);
    } catch (_) { /* noop */ }
    announce('已断开：' + session.asset.name);
  }

  // 连接成功后注入 bash 专属的 cwd 上报钩子（OSC 7）：文件面板跟随终端 cd。
  // 路径编码先 %→%25、再空格→%20（顺序不可换）：与前端 cwdFromPayload 的
  // decodeURIComponent 往返安全——只编码空格时，目录名含字面 % 会令解码端
  // 抛错回退 raw（100%done），目录名恰为 a%20b 会被误解码成 a b。
  // 无痕注入三件套（远端 pty 从建连起 ECHO=0，见 ssh.rs request_pty）：
  // 1) 回显在 tty 层关闭 → 注入行全程不可见（readline 依据 ECHO 位决定是否显示
  //    输入；早前「等首段输出再注入」闸门无效——MOTD 横幅是 sshd 在 shell 启动前
  //    打印的，慢 .bashrc 机器注入仍落在 .bashrc 执行期，被 tty 回显后又被
  //    readline 二次显示，造成 stty -echo 双重显示）。
  // 2) 单行注入，行尾 `stty echo` 恢复回显——置于 eval 之外：fish 等 shell 在
  //    eval 内报语法错也不影响回显恢复；payload 经 eval 包裹，整行拒Parse 时仅
  //    一行 stderr 噪音。前导空格配合 HISTCONTROL=ignoreboth 不进历史；case 守卫
  //    保证重连多次注入幂等；zsh（BASH_VERSION 空）安全跳过。
  // 3) `test -n "$BASH_VERSION" && printf '%b' '\e[1A\e[G\e[J'`：交互 bash 每执行
  //    完一行命令必重画一次 PS1（不受回显开关影响）。本行执行时 readline 已把提示
  //    符画在上方一行、accept 换行后光标在其下方行首——printf 上移一行、清到屏底，
  //    命令结束后 bash 在原位重画提示符 → 用户只看到一条干净提示符。擦除仅对 bash
  //    启用（readline 行接受必输出换行，上移一行精确落在提示符行；zsh/dash/fish
  //    光标行为不确定，宁可留一个空提示符也不冒险擦掉真实内容）。
  function injectShellCwdIntegration(session: SessionEntry) {
    const encoder = new TextEncoder();
    const mainLine =
      " eval 'if [ -n \"$BASH_VERSION\" ]; then" +
      ' __mt_cwd(){ __mt_p="${PWD//%/%25}"; printf \'\\\'\'\\033]7;file://%s\\007\'\\\'\' "${__mt_p// /%20}"; };' +
      ' case ";$PROMPT_COMMAND;" in *"__mt_cwd"*) ;; *) PROMPT_COMMAND="__mt_cwd;$PROMPT_COMMAND";; esac;' +
      ' __mt_cwd; fi\'; stty echo; test -n "$BASH_VERSION" && printf \'%b\' \'\\e[1A\\e[G\\e[J\'';
    invokeBackend('ssh_write', {
      sessionId: session.sessionId,
      data: Array.from(encoder.encode(mainLine + '\n'))
    }).catch(() => null);
  }

  // 重连：复用同一 session/termDiv 重走 ssh_connect（不销毁）；手动/自动重连共用
  async function reconnectSession(sessionId: string) {
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
      announce('终端模块加载失败：' + (error as Error).message);
      connectingAssetIds.delete(asset.id);
      return;
    }
    if (!terminalContainer) {
      connectingAssetIds.delete(asset.id);
      throw new Error('terminal container is not mounted');
    }

    const session = await createTerminalForAsset(asset);
    session.term.writeln('\x1b[36mmyshelltool SSH\x1b[0m - connecting to ' + asset.host + '...\r\n');

    try {
      const ok = await attachSessionStream(session);
      if (!ok) return; // 失败已置 status='error' + connectError，错误卡片展示
    } finally {
      connectingAssetIds.delete(asset.id);
    }
  }

  // 创建终端 + session 的共用工厂（connectSelected 新建连接与 adoptSession 跨窗口
  // 接管共用，消除两份实现漂移风险）：termDiv + Terminal + addons（fit/search/
  // serialize/webLinks/webgl）+ 危险粘贴守卫 + resize observer + autoReconnect 实例 +
  // 搜索结果订阅，最后挂入 sessions 并激活。overrides 覆盖 session 初始字段（adopt
  // 路径传真实 sessionId / status='connected' / oscTitle）。
  async function createTerminalForAsset(asset: NormalizedConnectionAsset, overrides: SessionOverrides = {}): Promise<SessionEntry> {
    // 调用契约：connectSelected / adoptSession 均已 await ensureTerminalModules() 且
    // terminalContainer 非空（非空断言只为通过类型检查，行为与原 JS 一致）
    const mods = terminalModules!;
    const termDiv = document.createElement('div');
    // 关键修复：termDiv 一开始就可见、占满容器。绝不能在 display:none 上调
    // term.open()（WebGL canvas 会 0×0 初始化，fit 后也不重建，终端空白）。
    termDiv.style.cssText = 'display:block;width:100%;height:100%;';
    terminalContainer!.appendChild(termDiv);

    const term = markRaw(new mods.Terminal(buildTerminalOptions({
      // overrides 可携带跨窗口迁移的样式快照（adoptSession）；缺省读本窗口设置
      fontSize: overrides.fontSize ?? terminalFontSize.value,
      lineHeight: overrides.lineHeight ?? terminalLineHeight.value,
      // 历史：effectiveTheme().value 恒 undefined（bridge 返回已解包 string），
      // pickTerminalTheme 忽略参数。cast 仅为通过类型检查（见 getTerminalTheme）。
      themeMode: overrides.themeMode || (effectiveTheme() as unknown as { value?: string }).value
    })));
    const fit = markRaw(new mods.FitAddon());
    const search = markRaw(new mods.SearchAddon());
    // serialize：跨窗口迁移导出带 SGR 颜色/样式的 scrollback（sessionHandoff 消费）
    const serialize = markRaw(new mods.SerializeAddon());
    term.loadAddon(fit);
    term.loadAddon(search);
    term.loadAddon(serialize);
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

    // 用 reactive() 包裹 session 对象：创建方后续会通过本地 session 变量多次
    // 修改其属性（status / sessionId / oscTitle / unlisten 等）。若 push 的是普通
    // 对象，本地引用指向【原始对象】，对其属性的赋值不经过代理 set trap，不触发
    // 响应式更新——这曾导致终端区域 status 永远停在 'connecting'（侧栏圆点靠
    // computed 重算碰巧更新，但终端组件的细粒度依赖收不到通知）。
    // reactive() 让本地 session 引用本身成为代理，所有属性变更都可靠触发更新。
    // term/fit/search 已 markRaw，reactive 不会再深代理它们。
    const session = reactive<SessionEntry>({
      sessionId: 'pending-' + asset.id + '-' + Date.now(),
      asset,
      term,
      fit,
      search,
      serialize,
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
      allowedPastePatterns: new Set<string>(),
      // 原写法 { stream: true }：stream 并非 TextDecoder 构造选项（WebIDL 忽略
      // 未知键，流式语义实际在 decode() 的第二参数），类型化后显式标注保留原样。
      decoder: new TextDecoder('utf-8', { stream: true } as unknown as TextDecoderOptions),
      ...overrides
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
    // 原生 Ctrl+V 粘贴守卫（安全红线，两条创建路径都必须有）：markRaw(term) 之后挂
    // handler；闭包经 getSessionId 取 session 当前 sessionId（重连期间 sessionId
    // 会切换，不能写死初始值）。
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
    return session;
  }

  // 跨窗口会话接管（sessionHandoff 迁移协议的目标侧）：以真实 sessionId 重建终端
  // 并回放 scrollback，不重连（Rust 侧 SSH 连接从未断开，直接复用）。
  // 注意：不调 injectShellCwdIntegration——远端 shell 存活、PROMPT_COMMAND 仍在
  // （注入只发生在最初连接时），无新 shell 可注入。
  async function adoptSession({ sessionId, assetId, scrollback, oscTitle, style }: AdoptSessionPayload) {
    if (!sessionId) return false;
    // 幂等守卫：已接管（或本就是本窗口会话）直接视为成功
    if (sessions.value.some(item => item.sessionId === sessionId)) return true;
    // 资产重解析（与 attachSessionStream 同源逻辑）：迁移期间资产可能被编辑
    const latestAssets = workbenchBridge && typeof workbenchBridge.assets === 'function'
      ? workbenchBridge.assets()
      : null;
    const asset = Array.isArray(latestAssets)
      ? latestAssets.find(item => item && item.id === assetId)
      : null;
    if (!asset) {
      // 跨窗口迁移诊断关键日志：asset 列表未加载完 / bridge 未注入都能在此分辨
      console.warn('[sessions] adoptSession 失败：资产未找到', {
        assetId,
        bridgeAttached: Boolean(workbenchBridge),
        assetCount: latestAssets?.length ?? null
      });
      return false;
    }
    try {
      await ensureTerminalModules();
    } catch (error) {
      announce('终端模块加载失败：' + (error as Error).message);
      return false;
    }
    if (!terminalContainer) {
      console.warn('[sessions] adoptSession 失败：终端容器未就绪（TerminalPane 未挂载）');
      return false;
    }

    // 样式快照（v2.4）：每个窗口独立 webview + 独立 localStorage 缓存（跨窗口
    // 非实时），目标窗口全局设置可能陈旧。快照【只作用于被迁移的终端实例】
    // （createTerminalForAsset overrides），保证会话视觉与源窗口一致；
    // 绝不回写目标窗口的全局字号/行距/主题——那是窗口级用户偏好，迁移不应改写。
    const styleSnapshot = style && typeof style === 'object' ? style : null;

    const session = await createTerminalForAsset(asset, {
      sessionId,
      status: 'connected',
      oscTitle: oscTitle || '',
      fontSize: styleSnapshot?.fontSize,
      lineHeight: styleSnapshot?.lineHeight
      // themeMode 不传：pickTerminalTheme 恒返回 darkTheme，终端主题与 app 主题解耦
    });
    // scrollback 回放：分块 write（每 100 行间隔一帧 rAF），防大文本一次性写入卡顿
    const text = String(scrollback || '');
    if (text) {
      const lines = text.split('\r\n');
      for (let i = 0; i < lines.length; i += 100) {
        if (i > 0) await new Promise(resolve => requestAnimationFrame(resolve));
        const end = Math.min(i + 100, lines.length);
        session.term.write(lines.slice(i, end).join('\r\n') + (end < lines.length ? '\r\n' : ''));
      }
    }
    // 重建 ssh-output/closed 事件接线（闭包私有，直接调）
    await registerSessionStream(session);
    setActiveSession(sessionId);
    // bridge.onSessionConnected 必须在 setActiveSession 之后：files handler 只对
    // 面板当前 selectedAsset 生效，setActiveSession 先经 syncAssetSelection 切过去
    try {
      workbenchBridge?.onSessionConnected?.(asset.id);
    } catch (_) { /* noop */ }
    // merge 回主窗口后切到 terminal tab，让用户立即看到回迁的终端
    try {
      workbenchBridge?.setTab?.('terminal');
    } catch (_) { /* noop */ }
    return true;
  }

  // 对已有 session 建立 SSH 连接 + 事件接线（首次连接 / 自动重连 / 手动重连共用）。
  // 复用 termDiv/term 不销毁；失败返回 false 并置 status='error' + connectError。
  async function attachSessionStream(session: SessionEntry) {
    // saveAsset 成功后会整体重建 assets 数组（新对象），session.asset 是建连时
    // 捕获的旧快照——重连（reauth 换密码 / 编辑连接后重试 / 自动重连）前按 id
    // 重新解析最新资产，否则仍会用旧认证参数连接（如已切 Password 仍走 PrivateKey）。
    const latestAssets = workbenchBridge && typeof workbenchBridge.assets === 'function'
      ? workbenchBridge.assets()
      : null;
    const freshAsset = Array.isArray(latestAssets)
      ? latestAssets.find(item => item && item.id === session.asset.id)
      : null;
    if (freshAsset) session.asset = freshAsset;
    const asset = session.asset;
    // 清理上一次连接的监听（重连时旧 ssh-output-/ssh-closed- 监听必须先解绑）。
    // unlisten 已 async 化：await 完成，rejection 吞掉不阻断重连。
    if (typeof session.unlisten === 'function') {
      try { await session.unlisten().catch(() => null); } catch (_) { /* noop */ }
      session.unlisten = null;
    }
    const prevRealId = String(session.sessionId).startsWith('pending-') ? null : session.sessionId;
    // 覆盖 sessionId 前记录旧值：重连场景 activeSessionId 仍指向旧 id，成功后需
    // 据此恢复激活态，否则 activeSession 严格 find 落空 → 终端区空白/错误卡片消失。
    const previousSessionId = session.sessionId;
    session.sessionId = 'pending-' + asset.id + '-' + Date.now();
    if (prevRealId) await invokeBackend('ssh_disconnect', { sessionId: prevRealId }).catch(() => null);
    session.status = 'connecting';
    session.connectError = null;
    const cols = session.term.cols && session.term.cols > 0 ? session.term.cols : 80;
    const rows = session.term.rows && session.term.rows > 0 ? session.term.rows : 24;
    let realSessionId: string | null = null;
    try {
      const result = await invokeBackend<SshConnectResult>('ssh_connect', {
        host: asset.host,
        port: asset.port,
        username: asset.username,
        password: '',
        credentialId: asset.credential_id || null,
        authMethod: asset.auth_method,
        privateKeyPath: asset.private_key_path,
        passphrase: null,
        passphraseCredentialId: asset.passphrase_credential_id || null,
        privateKeyCredentialId: asset.private_key_credential_id || null,
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
      const wasActive = activeSessionId.value === previousSessionId;
      session.sessionId = realSessionId;
      if (wasActive) activeSessionId.value = realSessionId;
      session.status = 'connected';
      await registerSessionStream(session);
      showOnlyActiveTerminal();
      // 通知文件面板：该资产已连接，空态时自动加载远程目录
      try {
        workbenchBridge?.onSessionConnected?.(session.asset?.id);
      } catch (_) { /* noop */ }
      // 注入 bash cwd 上报（OSC 7），使文件面板跟随终端 cd
      injectShellCwdIntegration(session);
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
      session.connectError = (error as Error).message;
      session.term.writeln('\x1b[31mError: ' + (error as Error).message + '\x1b[0m\r\n');
      announce('连接失败：' + (error as Error).message, { level: 'error' });
      return false;
    }
  }

  // 注册 output / closed 监听（每次连接建立时调用；decoder/oscParser 新建，
  // 避免上一次连接遗留的流式解码状态污染新连接）。
  async function registerSessionStream(session: SessionEntry) {
    const { asset, term } = session;
    const realSessionId = session.sessionId;
    const decoder = new TextDecoder('utf-8', { stream: true } as unknown as TextDecoderOptions);
    session.decoder = decoder;
    const oscParser = createOscParser(
      title => { session.oscTitle = title; },
      cwd => {
        // 终端 cd → 文件面板跟随（仅当面板展示的就是本会话资产时生效）
        try {
          workbenchBridge?.syncTerminalCwd?.(session.asset?.id, cwd);
        } catch (_) { /* noop */ }
      }
    );
    const outputUnlisten = await listenBackendEvent('ssh-output-' + realSessionId, event => {
      const data = event.payload as number[] | null | undefined;
      if (data && data.length > 0) {
        const text = decoder.decode(new Uint8Array(data));
        oscParser.feed(text);
        term.write(text);
      }
    });
    const closedUnlisten = await listenBackendEvent('ssh-closed-' + realSessionId, event => {
      const reason = typeof event.payload === 'string' && event.payload ? event.payload : 'unknown';
      session.status = 'disconnected';
      term.writeln('\r\n\x1b[31m[myshelltool] 远程连接已关闭 (' + reason + ')。\x1b[0m');
      announce('远程连接已关闭：' + asset.name + '（' + reason + '）', { level: 'warn' });
      // 通知文件面板清空该资产的目录（自动重连成功后会自动重新加载）
      try {
        workbenchBridge?.onSessionClosed?.(asset.id);
      } catch (_) { /* noop */ }
      scheduleSessionReconnect(session, reason);
    });
    // 组合句柄 async 化：跨窗口迁移路径需 await 两个 unlisten 完成后再移除
    // session（防旧窗口残留监听造成双写）；现有调用方不 await 也不破坏。
    session.unlisten = async () => {
      await Promise.all([outputUnlisten(), closedUnlisten()]);
    };
  }

  // 自动重连调度：仅对远端异常关闭（非 disconnected-by-user）触发；复用同一 session
  // 重走 ssh_connect。manualDisconnect / 会话已移除双守卫防与 cancelConnect 竞态。
  function scheduleSessionReconnect(session: SessionEntry, reason: string) {
    if (session.manualDisconnect || !session.autoReconnect) return;
    // 局部捕获：闭包内 TS 不继承外层可选链窄化（同一对象引用，行为不变）
    const autoReconnect = session.autoReconnect;
    if (reason === 'disconnected-by-user') return;
    const doReconnect = async () => {
      if (session.manualDisconnect || !sessions.value.includes(session)) return;
      const ok = await attachSessionStream(session);
      if (ok) {
        autoReconnect.reset();
        session.reconnectAttempt = 0;
        announce('已恢复连接：' + session.asset.name, { level: 'success' });
      } else if (!session.manualDisconnect && sessions.value.includes(session)) {
        // 本次失败 → 排队下一次尝试（onAttempt 更新计数，4 次后 onExhausted）
        autoReconnect.schedule(doReconnect);
      }
    };
    autoReconnect.schedule(doReconnect);
  }

  // 取消进行中的连接：仅 connecting 态可取消。不设自动超时——hostkey 等待有
  // 65s 后端流程，超时会误杀正常等待。
  async function cancelConnect(sessionId: string) {
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
  function dismissSessionError(sessionId: string) {
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
    terminalLineHeight,
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
    adoptSession,
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
    setTerminalLineHeight,
    updateAllTerminalThemes,
    writeToActiveTerminal,
    // 危险粘贴守卫（单一入口）
    requestDangerousPaste,
    approveDangerousPaste,
    cancelDangerousPaste,
    // host key / keyboard
    resolveHostKeyPrompt,
    resolveKeyboardPrompt,
    ownsConnectingSession,
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
