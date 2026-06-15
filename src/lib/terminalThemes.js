// xterm.js 主题：深/浅两套完整 ANSI 16 色，与 src/styles.css 设计 token 协调

export const darkTheme = {
  background: '#0b0e14',
  foreground: '#c9d1d9',
  cursor: '#c9d1d9',
  selectionBackground: '#264f78aa',
  black: '#1b1f27',
  red: '#ff6b6b',
  green: '#7ee787',
  yellow: '#f2cc60',
  blue: '#79c0ff',
  magenta: '#d2a8ff',
  cyan: '#56d4dd',
  white: '#c9d1d9',
  brightBlack: '#6e7681',
  brightRed: '#ffa198',
  brightGreen: '#56d364',
  brightYellow: '#e3b341',
  brightBlue: '#58a6ff',
  brightMagenta: '#bc8cff',
  brightCyan: '#39c5cf',
  brightWhite: '#f0f6fc'
};

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
