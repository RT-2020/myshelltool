import { pickTerminalTheme } from '../lib/terminalThemes.js';

// xterm Terminal options 工厂：集中所有终端配置，方便 store 在 connectSession 时调用。
// 人机工程学选项：cursorStyle bar / scrollback 10000 / lineHeight 1.3 / minimumContrastRatio 4.5
//   / rightClickSelectsWord true / fastScrollModifier alt
export function buildTerminalOptions({ fontSize, themeMode }) {
  return {
    cursorBlink: true,
    cursorStyle: 'bar',
    fontSize: fontSize || 14,
    fontFamily: 'Consolas, "JetBrains Mono", "Courier New", ui-monospace, monospace',
    theme: pickTerminalTheme(themeMode),
    allowProposedApi: true,
    scrollback: 10000,
    lineHeight: 1.3,
    letterSpacing: 0,
    minimumContrastRatio: 4.5,
    rightClickSelectsWord: true,
    fastScrollModifier: 'alt',
    fastScrollSensitivity: 5
  };
}

// 主题切换时同步所有活跃终端
export function applyThemeToAll(sessions, themeMode) {
  const theme = pickTerminalTheme(themeMode);
  sessions.value.forEach(session => {
    if (session.controller) {
      session.controller.applyTheme(theme);
    } else if (session.term) {
      try { session.term.options.theme = theme; } catch (_) { /* noop */ }
    }
  });
}
