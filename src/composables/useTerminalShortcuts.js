// 集中键盘快捷键：保留现有 Ctrl+Shift+F/C/V、Alt+Enter、Ctrl+=/-/0、Ctrl+Tab、Ctrl+W、Ctrl+Shift+T
// 新增：`?` 弹出快捷键速查、Ctrl+Shift+P 命令面板
// 焦点守卫：activeElement 是 input/textarea/contenteditable 时跳过全局快捷键（避免抢输入框焦点）

const TEXT_INPUT_TAGS = ['INPUT', 'TEXTAREA', 'SELECT'];

function isTextInput(el) {
  if (!el) return false;
  if (TEXT_INPUT_TAGS.includes(el.tagName)) return true;
  if (el.isContentEditable) return true;
  return false;
}

// handlers: 一个 map，键是快捷键标识（如 'ctrl+shift+f'），值是 () => void。
// 文字键（如 '?'）单独处理，不在 ctrl/alt 修饰下。
export function useTerminalShortcuts(handlers) {
  function onKeyDown(e) {
    // 焦点守卫：在输入框中不拦截任何全局快捷键
    if (isTextInput(document.activeElement)) {
      // 仅放行 Esc（关闭搜索框由搜索框自己处理）
      return;
    }

    const key = e.key;
    const ctrl = e.ctrlKey || e.metaKey;

    // Ctrl+Shift+xxx
    if (ctrl && e.shiftKey) {
      const k = key.toLowerCase();
      if (k === 'f' && handlers.search) { e.preventDefault(); handlers.search(); return; }
      if (k === 'c' && handlers.copy) { e.preventDefault(); handlers.copy(); return; }
      if (k === 'v' && handlers.paste) { e.preventDefault(); handlers.paste(); return; }
      if (k === 't' && handlers.connect) { e.preventDefault(); handlers.connect(); return; }
      if (k === 'p' && handlers.commandPalette) { e.preventDefault(); handlers.commandPalette(); return; }
      return;
    }

    // Ctrl+xxx（无 shift）
    if (ctrl && !e.shiftKey) {
      const k = key.toLowerCase();
      if (key === 'Tab' && handlers.nextSession) { e.preventDefault(); handlers.nextSession(); return; }
      if (k === 'w' && handlers.closeActive) { e.preventDefault(); handlers.closeActive(); return; }
      if ((k === '=' || k === '+') && handlers.fontInc) { e.preventDefault(); handlers.fontInc(); return; }
      if (k === '-' && handlers.fontDec) { e.preventDefault(); handlers.fontDec(); return; }
      if (k === '0' && handlers.fontReset) { e.preventDefault(); handlers.fontReset(); return; }
      return;
    }

    // Alt+Enter（全屏）
    if (e.altKey && key === 'Enter' && handlers.fullscreen) {
      e.preventDefault();
      handlers.fullscreen();
      return;
    }

    // `?` 弹出快捷键速查（无修饰，仅当 shift 时 key 才是 '?'）
    if (key === '?' && handlers.showCheatsheet) {
      e.preventDefault();
      handlers.showCheatsheet();
      return;
    }

    // Esc（关闭浮层，仅当不在输入框时）
    if (key === 'Escape' && handlers.dismissOverlay) {
      e.preventDefault();
      handlers.dismissOverlay();
      return;
    }
  }

  function setup() {
    window.addEventListener('keydown', onKeyDown, true);
  }

  function teardown() {
    window.removeEventListener('keydown', onKeyDown, true);
  }

  return { setup, teardown };
}
