import { pickTerminalTheme } from '../lib/terminalThemes.js';

// xterm Terminal options 工厂：集中所有终端配置，方便 store 在 connectSession 时调用。
// 人机工程学选项：cursorStyle bar / scrollback 10000 / lineHeight 1.3 / minimumContrastRatio 4.5
//   / fastScrollModifier alt
// 注：rightClickSelectsWord 已移除——右键改由 TerminalPane 的 @contextmenu 弹复制粘贴菜单，
// 双击仍可选词（xterm 默认行为）。
export function buildTerminalOptions({ fontSize, themeMode }) {
  return {
    cursorBlink: true,
    cursorStyle: 'bar',
    fontSize: fontSize || 14,
    // 字体优先级与设计稿 --font-mono 同源（JetBrains Mono 优先，需在 index.html 加载）。
    // 回退链：JetBrains Mono → Cascadia Code（Win11 自带）→ Consolas（Win10）→ 系统等宽
    fontFamily: '"JetBrains Mono", "Cascadia Code", Consolas, "Courier New", ui-monospace, monospace',
    theme: pickTerminalTheme(themeMode),
    allowProposedApi: true,
    scrollback: 10000,
    lineHeight: 1.3,
    letterSpacing: 0,
    minimumContrastRatio: 4.5,
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
