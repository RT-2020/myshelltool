import { defineStore } from 'pinia';
import { computed, reactive, ref } from 'vue';
import type {
  HostKeyVerifyPayload,
  KeyboardInteractivePayload,
  ModalState,
  NormalizedConnectionAsset,
  NotifyOptions,
  SshConnectResult,
  TerminalSearchState
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
import { errorMessage } from '../lib/errorMessage';
import { createDangerousPasteGuard, createNativePasteGuard } from '../lib/terminalGuards';
import type { DangerousPastePrompt } from '../lib/terminalGuards';
import { pickTerminalTheme } from '../lib/terminalThemes';
import { createOscParser } from '@/lib/oscParser';
import {
  applyTerminalFontSizeAll,
  bindTerminalAppearanceContext,
  getTerminalTheme,
  initTerminalFontSize,
  initTerminalLineHeight,
  resetTerminalFontSize,
  setTerminalFontSize,
  setTerminalLineHeight,
  updateAllTerminalThemes
} from '@/lib/terminalAppearance';
import {
  approveDangerousPaste,
  bindTerminalOpsContext,
  cancelDangerousPaste,
  closeTerminalSearchInline,
  copyTerminalSelection,
  dangerousPastePrompt,
  findTerminalNext,
  openTerminalSearchInline,
  pasteToTerminal,
  requestDangerousPaste,
  runTerminalAction,
  setTerminalSearchOpts,
  setTerminalSearchQuery,
  writeToActiveTerminal
} from '@/lib/terminalOps';
import {
  bindSessionEventsContext,
  disposeEventListeners,
  ensureHostKeyListeners,
  ownsConnectPrompt,
  resolveHostKeyPrompt,
  resolveKeyboardPrompt,
  setupEventListeners
} from '@/lib/sessionEvents';
import {
  adoptSession,
  attachResizeObserver,
  bindTerminalLifecycleContext,
  createTerminalForAsset,
  ensureTerminalModules,
  hasTerminalContainer,
  setTerminalContainer,
  showOnlyActiveTerminal,
  type TerminalLifecycleContext
} from '@/lib/terminalLifecycle';
import {
  attachSessionStream,
  bindTerminalStreamContext,
  registerSessionStream,
  type TerminalStreamContext
} from '@/lib/terminalStream';
import type {
  AutoReconnectController,
  OscParser,
  SessionEntry,
  TerminalModules
} from '@/lib/terminalTypes';
import type { Terminal } from '@xterm/xterm';
import type { FitAddon } from '@xterm/addon-fit';
import type { SearchAddon } from '@xterm/addon-search';
import type { SerializeAddon } from '@xterm/addon-serialize';

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
}

/**
 * useSessionsStore — Wave 2 Step 2.1
 * 从 workbench.js 抽取所有 session 相关 state / actions / computed。
 * 关键迁移点（Critic 改进）：createOscParser 随 session 迁移；hostKeyPrompt
 * 65s 超时 watcher + hostKeyTimeout 闭包随 sshConfirmHostKey 迁移；localStorage
 * key 禁重命名；hostKey/keyboard/status 三个 unlisten handle 必须迁移
 * （传输进度 handle 不在此列，归 files store）。
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
  const terminalFontSize = ref(initTerminalFontSize());
  // 行高（xterm lineHeight）：1.0 紧凑 ~ 2.0 宽松，默认 1.0；设置面板可调。
  const terminalLineHeight = ref(initTerminalLineHeight());
  const terminalSearch = ref<TerminalSearchState>({ open: false, query: '', direction: 'next', result: null });

  // 用于驱动终端主题更新（原 workbench.js:24，仅保留与终端主题相关的部分）
  const systemPrefersDark = ref(false);

  // ============================================================
  // Module-level closure（原 workbench.js:64-71）
  // ============================================================
  const connectingAssetIds = new Set<string>();

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
    try { session.term.dispose(); } catch { /* 已 dispose 的 xterm 重复 dispose 抛错，销毁流程继续 */ }
    session.termDiv?.remove();
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
    try { session.term.dispose(); } catch { /* 已 dispose 的 xterm 重复 dispose 抛错，销毁流程继续 */ }
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
      announce('终端模块加载失败：' + errorMessage(error));
      connectingAssetIds.delete(asset.id);
      return;
    }
    if (!hasTerminalContainer()) {
      connectingAssetIds.delete(asset.id);
      throw new Error('terminal container is not mounted');
    }

    const session = await createTerminalForAsset(asset);
    session.term.writeln('\x1b[36mmyshelltool SSH\x1b[0m - connecting to ' + asset.host + '...\r\n');

    try {
      const ok = await attachSessionStream(session);
      if (!ok) return; // 失败已置 status='error' + connectError，错误卡片展示
      // v2.6：首次连接成功同样启动稳定窗口——建连后立刻被关的服务器不该被当成
      // 「稳定连接」（否则用户看到一次成功提示后立刻又断，重连计数也不准）。
      session.autoReconnect?.markConnected();
    } finally {
      connectingAssetIds.delete(asset.id);
    }
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

  // —— 终端生命周期 / 流接线 lib 的 context（绑定式：lib 导出保持拆分前原签名，
  // 本 store setup 时绑定一次；成员为 refs 或箭头回调，取值恒新）——
  const lifecycleCtx: TerminalLifecycleContext = {
    sessions,
    activeSessionId,
    terminalFontSize,
    terminalLineHeight,
    effectiveTheme,
    announce,
    requestDangerousPaste: (sessionId, text) => requestDangerousPaste(sessionId, text),
    setActiveSession,
    registerSessionStream,
    onSessionConnected: assetId => { workbenchBridge?.onSessionConnected?.(assetId); },
    setTab: tab => { workbenchBridge?.setTab?.(tab); },
    getAssets: () => (workbenchBridge && typeof workbenchBridge.assets === 'function' ? workbenchBridge.assets() : null)
  };
  const streamCtx: TerminalStreamContext = {
    sessions,
    activeSessionId,
    getAssets: lifecycleCtx.getAssets,
    onSessionConnected: lifecycleCtx.onSessionConnected,
    onSessionClosed: assetId => { workbenchBridge?.onSessionClosed?.(assetId); },
    syncTerminalCwd: (assetId, path) => { workbenchBridge?.syncTerminalCwd?.(assetId, path); },
    announce,
    showOnlyActiveTerminal
  };
  // 事件接线 ctx（wb().modal 读写与 selectedAsset 经回调，保持 bridge 惰性）
  bindSessionEventsContext({
    sessions,
    hostKeyPrompt,
    keyboardPrompt,
    setModal: modal => { wb().modal = modal; },
    selectedAsset: () => wb().selectedAsset,
    announce
  });
  // 外观 ctx
  bindTerminalAppearanceContext({
    sessions,
    terminalFontSize,
    terminalLineHeight,
    effectiveTheme,
    announce
  });
  // 终端操作 ctx（connect/reconnect/fontSize 经回调回 store 的会话管理层）
  bindTerminalOpsContext({
    sessions,
    activeSessionId,
    activeSession,
    terminalSearch,
    announce,
    connect: () => { connectSelected(); },
    reconnect: sessionId => { reconnectSession(sessionId); },
    setFontSize: delta => setTerminalFontSize(delta),
    resetFontSize: () => resetTerminalFontSize()
  });
  bindTerminalLifecycleContext(lifecycleCtx);
  bindTerminalStreamContext(streamCtx);

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
    ownsConnectPrompt,
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
