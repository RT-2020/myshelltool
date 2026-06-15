// useTheme — 三态主题切换 composable（Wave 1 Step 1.3）
//
// 替代 src/stores/workbench.js 中分散的 applyTheme / readSystemPrefersDark /
// startSystemThemeListener 三个内部函数，模块化以便 Wave 2 拆 useUiStore 时直接复用。
//
// 三态：'system' | 'light' | 'dark'
// - 'system': 跟随 prefers-color-scheme，由 CSS @media (在 _tokens.scss 的
//   [data-theme='system'] selector 内) 处理实际渲染色。
// - 'light' / 'dark': 强制覆盖。
//
// Wave 1 阶段保持 workbench.js 现有行为（effectiveTheme computed 把 'system' 解析
// 成 'light'/'dark' 再写入 dataset.theme），所以 useTheme.applyTheme 只是工具函数，
// 不改变 effectiveTheme 解析逻辑。Wave 2 切换到 useUiStore 时可以改为直接写
// dataset.theme = theme（让 CSS @media 接管 system 分支）。

export const THEME_ORDER = ['system', 'light', 'dark'];

export const THEME_LABELS = {
  system: '跟随系统',
  light: '浅色',
  dark: '深色',
};

/**
 * 把 theme 写入 document.documentElement.dataset.theme。
 * 接受 'system' | 'light' | 'dark'。非法值降级为 'dark'（与历史默认一致）。
 */
export function applyTheme(theme) {
  if (typeof document === 'undefined') return;
  const value = THEME_ORDER.includes(theme) ? theme : 'dark';
  document.documentElement.dataset.theme = value;
}

/**
 * 读 prefers-color-scheme。SSR / 无 matchMedia 时默认 true（dark）。
 */
export function readSystemPrefersDark() {
  if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  }
  return true;
}

/**
 * 监听 prefers-color-scheme 变化，返回 unsubscribe 函数。
 * 兼容新标准 addEventListener 与旧 Safari addListener。
 */
export function startSystemThemeListener(onChange) {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return () => {};
  }
  const mql = window.matchMedia('(prefers-color-scheme: dark)');
  const handler = event => onChange(event.matches);
  if (typeof mql.addEventListener === 'function') {
    mql.addEventListener('change', handler);
    return () => mql.removeEventListener('change', handler);
  }
  if (typeof mql.addListener === 'function') {
    mql.addListener(handler);
    return () => mql.removeListener(handler);
  }
  return () => {};
}

/**
 * Composable 形式（可选；Wave 2 useUiStore 会用）。
 * 返回响应式 theme ref + setTheme/applyTheme 方法。
 * 当前 workbench.js 不使用此 composable，保持 own state；此处仅为未来复用预置。
 */
export function useTheme() {
  // 占位：Wave 2 useUiStore 会用 pinia + ref + watch 实现；此处不引入 vue 依赖
  // 避免 workbench.js 重复实例化导致的副作用。
  return {
    applyTheme,
    readSystemPrefersDark,
    startSystemThemeListener,
    THEME_ORDER,
    THEME_LABELS,
  };
}
