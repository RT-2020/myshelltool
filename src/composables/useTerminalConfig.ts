import type { ITerminalOptions } from '@xterm/xterm';
import { pickTerminalTheme } from '../lib/terminalThemes';

// xterm Terminal options 工厂：集中所有终端配置，方便 store 在 connectSession 时调用。
// 人机工程学选项：cursorStyle bar / scrollback 10000 / lineHeight（可配置，默认 1.0）/
//   minimumContrastRatio 4.5 / fastScrollModifier alt
// 注：rightClickSelectsWord 已移除——右键改由 TerminalPane 的 @contextmenu 弹复制粘贴菜单，
// 双击仍可选词（xterm 默认行为）。
export interface TerminalConfigInput {
  fontSize?: number;
  lineHeight?: number;
  themeMode?: string;
}

// xterm 6 的 ITerminalOptions 已删除 fastScrollModifier（v5 废弃、alt 行为内建）；
// 原 JS 仍透传该选项（运行时多余键无副作用），类型层局部补充以保持行为零变化。
export type TerminalOptions = ITerminalOptions & { fastScrollModifier?: 'alt' | 'ctrl' | 'shift' | null };

export function buildTerminalOptions({ fontSize, lineHeight, themeMode }: TerminalConfigInput): TerminalOptions {
  return {
    cursorBlink: true,
    cursorStyle: 'bar',
    fontSize: fontSize || 12,
    // 字体优先级与设计稿 --font-mono 同源（JetBrains Mono 优先，需在 index.html 加载）。
    // 回退链：JetBrains Mono → Cascadia Code（Win11 自带）→ Consolas（Win10）→ 系统等宽
    fontFamily: '"JetBrains Mono", "Cascadia Code", Consolas, "Courier New", ui-monospace, monospace',
    theme: pickTerminalTheme(themeMode),
    allowProposedApi: true,
    scrollback: 10000,
    lineHeight: lineHeight || 1,
    letterSpacing: 0,
    minimumContrastRatio: 4.5,
    fastScrollModifier: 'alt',
    fastScrollSensitivity: 5
  };
}
