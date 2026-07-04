// xterm.js 主题：深/浅两套完整 ANSI 16 色。
//
// UI 整体重构（open-design）：深色色板已同步设计稿 --terminal-* token
// （见 src/styles/_tokens.scss 与 tokens.css）：
//   --terminal-bg #0c0f17  --terminal-text #c9d1d9  --terminal-prompt #6cb6ff
//   --terminal-success #4ec9b0  --terminal-warn #d29922  --terminal-danger #f06292
//   --terminal-accent #6cb6ff
//
// ANSI 16 色映射策略：
//   - 设计稿显式定义的（bg/fg/blue/green/yellow/red）→ 用设计稿值
//   - 设计稿未定义的（black/magenta/cyan/bright*）→ 保留原 VS Code One Dark 风值，
//     与设计稿冷调底色协调，并在下方注释标明来源

export const darkTheme = {
  // —— 设计稿契约色（--terminal-* 同源）——
  background: '#0c0f17',        // --terminal-bg（原 #0b0e14，设计稿更冷调）
  foreground: '#c9d1d9',        // --terminal-text
  cursor: '#c9d1d9',
  selectionBackground: '#264f78aa',
  // —— ANSI 16 色 ——
  black: '#1b1f27',             // 设计稿未定义，保留（与 bg 协调的近黑暗底）
  red: '#f06292',               // --terminal-danger（原 #ff6b6b，设计稿粉红更克制）
  green: '#4ec9b0',             // --terminal-success（原 #7ee787，设计稿青绿 VS Code 风）
  yellow: '#d29922',            // --terminal-warn（原 #f2cc60，设计稿琥珀黄）
  blue: '#6cb6ff',              // --terminal-prompt / accent（原 #79c0ff）
  magenta: '#d2a8ff',           // 设计稿未定义，保留（与冷调底色协调）
  cyan: '#56d4dd',              // 设计稿未定义，保留（与 green/success 青调呼应）
  white: '#c9d1d9',             // 同 foreground
  brightBlack: '#6e7681',       // 设计稿未定义，保留（注释/行号灰）
  brightRed: '#ffa198',         // 设计稿未定义，保留
  brightGreen: '#56d4dd',       // 向 cyan 靠拢，与 success 青调一致
  brightYellow: '#e3b341',      // 设计稿未定义，保留
  brightBlue: '#58a6ff',        // 设计稿未定义，保留（与 blue 同系）
  brightMagenta: '#bc8cff',     // 设计稿未定义，保留
  brightCyan: '#39c5cf',        // 设计稿未定义，保留
  brightWhite: '#f0f6fc'        // 设计稿未定义，保留
};

// 浅色主题色板（用于浅色主题下的终端；保留浅底供用户选择）。
// 注意：open-design 设计稿的契约是「终端始终深色」，但现有行为是浅色主题用浅色终端。
// 若后续要严格执行「终端始终深色」契约，让 pickTerminalTheme 无条件返回 darkTheme 即可。
export const lightTheme = {
  background: '#fbfbfc',
  foreground: '#1b1f27',
  cursor: '#1b1f27',
  selectionBackground: '#cae3ffaa',
  black: '#1b1f27',
  red: '#cf222e',
  green: '#1a7f37',
  yellow: '#9a6700',
  blue: '#0969da',
  magenta: '#8250df',
  cyan: '#1b7c83',
  white: '#57606a',
  brightBlack: '#6e7681',
  brightRed: '#a40e26',
  brightGreen: '#2da44e',
  brightYellow: '#bf8700',
  brightBlue: '#218bff',
  brightMagenta: '#a475f4',
  brightCyan: '#3197aa',
  brightWhite: '#1b1f27'
};

export function pickTerminalTheme(mode) {
  if (mode === 'light') return lightTheme;
  return darkTheme;
}
