// 终端输入守卫的纯函数工厂（与 store 解耦，便于单测）。
//
// createNativePasteGuard — xterm attachCustomKeyEventHandler 的 handler 工厂。
// 拦截原生 Ctrl/Cmd+V：异步读剪贴板后走统一危险粘贴守卫（requestDangerousPaste），
// 命中危险命令弹确认、未命中直接写入；clipboard API 不可用时放行浏览器默认粘贴。
// 返回 true = 放行 xterm 默认行为；返回 false = 本 handler 已接管该按键。

export function createNativePasteGuard({ getSessionId, requestDangerousPaste, announce }) {
  return (event) => {
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

// createDangerousPasteGuard — 危险粘贴守卫状态机（依赖注入，纯逻辑可单测）。
// getSession(sessionId) 返回含 term 的 session（无则 null）；prompt 是 store 持有的
// reactive 对象；detect 是 detectDangerousCommand。request 返回 true = 已拦截等确认。
export function createDangerousPasteGuard({ getSession, prompt, detect }) {
  function request(sessionId, text) {
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

  function approve(allowedPattern) {
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
