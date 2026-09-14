import type { Terminal } from '@xterm/xterm';

// 终端输入守卫的纯函数工厂（与 store 解耦，便于单测）。
//
// createNativePasteGuard — xterm attachCustomKeyEventHandler 的 handler 工厂。
// 拦截原生 Ctrl/Cmd+V：异步读剪贴板后走统一危险粘贴守卫（requestDangerousPaste），
// 命中危险命令弹确认、未命中直接写入；clipboard API 不可用时放行浏览器默认粘贴。
// 返回 true = 放行 xterm 默认行为；返回 false = 本 handler 已接管该按键。

/** 危险粘贴守卫所需的 session 最小形状（真实 session 的结构子集，协变兼容）。 */
export interface GuardSession {
  term: Terminal;
  allowedPastePatterns?: Set<string>;
}

/** store 持有的危险粘贴确认弹窗 reactive 状态。 */
export interface DangerousPastePrompt {
  open: boolean;
  sessionId: string | null;
  command: string;
  matchedPattern: string;
}

export interface NativePasteGuardOptions {
  getSessionId: () => string;
  requestDangerousPaste: (sessionId: string, text: string) => void;
  announce: (message: string, options?: { level?: string }) => void;
}

export function createNativePasteGuard({ getSessionId, requestDangerousPaste, announce }: NativePasteGuardOptions) {
  return (event: KeyboardEvent): boolean => {
    const mod = event.ctrlKey || event.metaKey;
    if (!mod || event.shiftKey || (event.key !== 'v' && event.key !== 'V')) return true;

    // clipboard API 不可用（非安全上下文 / 旧 webview / 未授权）：放行默认粘贴。
    // 注意：promise 拒绝发生在 handler 返回之后，无法再"返回 true"，
    // 此时事件已拦截，只能提示用户改用工具栏粘贴（含 fallback 链）。
    if (typeof navigator === 'undefined' || !navigator.clipboard || typeof navigator.clipboard.readText !== 'function') {
      return true;
    }
    event.preventDefault();
    navigator.clipboard.readText()
      .then((text) => {
        if (text) requestDangerousPaste(getSessionId(), text);
      })
      .catch(() => {
        announce('剪贴板读取失败，请使用工具栏粘贴', { level: 'warn' });
      });
    return false;
  };
}

// ============================================================
// 应用快捷键组合识别（单一信息源）
// ============================================================
// TerminalSurface.handleKeydown（动作分发）与 xterm 放行器（createAppShortcutRelease）
// 共用本判定——两处各自维护一份组合表必然漂移（Ctrl+K 就是先例）。
// 只收录带 Ctrl/Cmd 修饰键的组合；Escape 与可打印字符（如 ?）必须留给终端本体。
// altKey 一律排除：欧洲键盘 AltGr+字符会带上 ctrlKey，不能当快捷键放行。

export type AppShortcutAction =
  | 'global-search'    // Ctrl+K（App.vue 全局层，不限 terminal tab）
  | 'connect-selected' // Ctrl+Shift+T
  | 'terminal-search'  // Ctrl+F
  | 'command-palette'  // Ctrl+Shift+P
  | 'terminal-copy'    // Ctrl+Shift+C
  | 'terminal-paste'   // Ctrl+Shift+V
  | 'terminal-clear'   // Ctrl+Shift+L
  | 'font-inc'         // Ctrl+= / Ctrl++
  | 'font-dec'         // Ctrl+-
  | 'font-reset'       // Ctrl+0
  | 'session-next'     // Ctrl+Tab
  | 'session-prev'     // Ctrl+Shift+Tab
  | 'session-close';   // Ctrl+W

export function matchAppShortcut(event: KeyboardEvent): AppShortcutAction | null {
  const mod = event.ctrlKey || event.metaKey;
  if (!mod || event.altKey) return null;
  if (event.key === 'Tab') return event.shiftKey ? 'session-prev' : 'session-next';
  const key = typeof event.key === 'string' ? event.key.toLowerCase() : '';
  if (event.shiftKey) {
    if (key === 'p') return 'command-palette';
    if (key === 'c') return 'terminal-copy';
    if (key === 'v') return 'terminal-paste';
    if (key === 'l') return 'terminal-clear';
    if (key === 't') return 'connect-selected';
    return null;
  }
  if (key === 'k') return 'global-search';
  if (key === 'f') return 'terminal-search';
  if (key === 'w') return 'session-close';
  if (key === '=' || key === '+') return 'font-inc';
  if (key === '-') return 'font-dec';
  if (key === '0') return 'font-reset';
  return null;
}

// createAppShortcutRelease — xterm 按键放行器工厂。
// 焦点在终端时 xterm 会把 Ctrl 组合键消费成 VT 序列直接发给远端（Ctrl+K→\x0b、
// Ctrl+F→\x06、Ctrl+W→\x17、Ctrl+Tab→\t）且拦截冒泡，window 上的应用监听收不到
// → 终端快捷键在「终端焦点」这一主场景下整体失效（实测矩阵见 v2.8）。
// 返回 false = xterm 对该键完全放行：keydown 不消费（事件冒泡到 window 分发），
// keyup 也不处理（xterm 的 _keyUp 路径会 this.focus() 抢回焦点，故一并拦下）。
// enabled=false（资产独立窗口，无全局搜索/命令面板）时全放行，保留终端原生行为。
// 注意：Ctrl+V（无 shift）不在放行之列——那是 createNativePasteGuard 的领域。
export interface AppShortcutReleaseOptions {
  enabled: boolean;
}

export function createAppShortcutRelease({ enabled }: AppShortcutReleaseOptions) {
  return (event: KeyboardEvent): boolean => {
    if (!enabled) return true;
    return matchAppShortcut(event) === null;
  };
}

// composeKeyHandlers — 组合多个 customKeyEventHandler。attachCustomKeyEventHandler
// 只能挂一个（后挂覆盖前挂），多守卫必须经此组合；首个返回 false 即短路。
export function composeKeyHandlers(...handlers: Array<(event: KeyboardEvent) => boolean>) {
  return (event: KeyboardEvent): boolean => {
    for (const handler of handlers) if (!handler(event)) return false;
    return true;
  };
}

// createDangerousPasteGuard — 危险粘贴守卫状态机（依赖注入，纯逻辑可单测）。
// getSession(sessionId) 返回含 term 的 session（无则 null）；prompt 是 store 持有的
// reactive 对象；detect 是 detectDangerousCommand。request 返回 true = 已拦截等确认。
export interface DangerousPasteGuardOptions {
  getSession: (sessionId: string | null) => GuardSession | null;
  prompt: DangerousPastePrompt;
  detect: (text: string) => { pattern: string } | null;
}

export function createDangerousPasteGuard({ getSession, prompt, detect }: DangerousPasteGuardOptions) {
  function request(sessionId: string, text: string): boolean {
    const session = getSession(sessionId);
    if (!session || !text) return false;
    const danger = detect(text);
    if (danger) {
      // 本会话已放行的规则（approve 记录）直接放行
      if (session.allowedPastePatterns && session.allowedPastePatterns.has(danger.pattern)) {
        session.term.paste(text);
        return false;
      }
      prompt.open = true;
      prompt.sessionId = sessionId;
      prompt.command = text;
      prompt.matchedPattern = danger.pattern;
      return true; // 已拦截，等待用户确认
    }
    session.term.paste(text);
    return false; // 未拦截，已直接写入
  }

  function approve(allowedPattern: string) {
    const session = getSession(prompt.sessionId);
    const command = prompt.command;
    if (session && command) {
      if (allowedPattern) {
        if (!session.allowedPastePatterns) session.allowedPastePatterns = new Set();
        session.allowedPastePatterns.add(allowedPattern);
      }
      // 只对 prompt 记录的 sessionId 写入，杜绝串会话
      session.term.paste(command);
    }
    cancel();
  }

  function cancel() {
    prompt.open = false;
    prompt.sessionId = null;
    prompt.command = '';
    prompt.matchedPattern = '';
  }

  return { request, approve, cancel };
}
