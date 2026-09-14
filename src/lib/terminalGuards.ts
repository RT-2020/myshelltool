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

// createGlobalSearchHotkeyRelease — Ctrl+K 放行器工厂。
// 焦点在终端时 xterm 会把 Ctrl+K 消费成 VT kill-line（\x0b 直接发给远端），
// 事件不冒泡，App.vue 的 window keydown 监听收不到 → 全局搜索快捷键在终端
// 焦点下失效。返回 false = xterm 对该键完全放行：keydown 不消费（事件冒泡到
// window 由 handleGlobalKeydown preventDefault + 打开搜索），keyup 也不处理
// （xterm 的 _keyUp 路径会 this.focus() 抢回焦点，故一并拦下）。
// enabled=false（资产独立窗口不注册全局搜索）时全放行，保留终端 kill-line。
export interface GlobalSearchHotkeyReleaseOptions {
  enabled: boolean;
}

export function createGlobalSearchHotkeyRelease({ enabled }: GlobalSearchHotkeyReleaseOptions) {
  return (event: KeyboardEvent): boolean => {
    if (!enabled) return true;
    const isCtrlK = (event.ctrlKey || event.metaKey) && !event.shiftKey && !event.altKey
      && typeof event.key === 'string' && event.key.toLowerCase() === 'k';
    return !isCtrlK;
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
