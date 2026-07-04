/**
 * usePanelResize — 面板拖拽调整 composable（Wave: 布局自由化）
 *
 * 管理三个布局 CSS 变量：
 *   --sidebar-w     左侧连接资产列宽（与 ui.js 的折叠态共用变量名）
 *   --right-w       右侧资源监控/运维摘要列宽
 *   --terminal-h    open-design 中间终端区高度
 *   --center-top-h  旧 shell 兼容变量（与 --terminal-h 同步）
 *
 * 拖拽通过 pointer 事件实现（pointerdown 在 divider 上 → 绑定 document 的
 * pointermove/pointerup）。拖拽结束写 localStorage，刷新后保持。
 *
 * 折叠优先级：当某列处于折叠态（dataset.assets/data-right = collapsed）时，
 * 对应拖拽不响应——折叠态由 CSS dataset 选择器强制覆盖变量，拖拽逻辑跳过，
 * 避免内联 style 与折叠 44px/0px 打架（内联优先级高于选择器）。
 *
 * CSS 变量经 document.documentElement.style.setProperty 写入内联，
 * AppShellLayout 的 grid-template-columns/rows 用 var() 读取。
 * xterm 自适应：现有 ResizeObserver 监听 termDiv，拖拽改变容器尺寸会自动 fit，
 * 无需本 composable 手动调用。
 */
import { onBeforeUnmount, ref } from 'vue';

const LAYOUT_STORAGE_KEY = 'myshelltool:layout:v1';
const LEGACY_LAYOUT_STORAGE_KEY = 'myshelltool-layout';

// 列/行宽高的 clamp 范围（px）。中心区行高用百分比表述，但内部统一按 px 计算。
const SIDEBAR_MIN = 180;
const SIDEBAR_MAX = 480;
const RIGHT_MIN = 220;
const RIGHT_MAX = 520;
const CENTER_TOP_MIN = 120;   // 终端区最小高度
const CENTER_BOTTOM_MIN = 120; // 文件区最小高度

// 默认值（resetLayout / 首次加载无持久化时用）
const DEFAULTS = {
  sidebarW: 260,
  rightW: 280,
  terminalRatio: 0.55,
  centerTopH: null // 兼容旧存储：null = 按 terminalRatio 计算
};

function readStoredLayout() {
  try {
    const raw = localStorage.getItem(LAYOUT_STORAGE_KEY);
    const legacyRaw = raw || localStorage.getItem(LEGACY_LAYOUT_STORAGE_KEY);
    if (!legacyRaw) return null;
    const parsed = JSON.parse(legacyRaw);
    return parsed && typeof parsed === 'object' ? parsed : null;
  } catch {
    return null;
  }
}

function clamp(v, min, max) {
  return Math.min(max, Math.max(min, v));
}

/**
 * 读取某列是否处于折叠态（dataset 在 <html> 上由 ui.js toggleAssets/toggleRight 写入）。
 * 折叠态下对应列的拖拽跳过。
 */
function isRegionCollapsed(which) {
  if (typeof document === 'undefined') return false;
  if (which === 'sidebar') return document.documentElement.dataset.assets === 'collapsed';
  if (which === 'right') return document.documentElement.dataset.right === 'collapsed';
  return false;
}

function getMainHeight() {
  if (typeof window === 'undefined') return 720;
  return Math.max(CENTER_TOP_MIN + CENTER_BOTTOM_MIN, window.innerHeight - 52 - 28);
}

export function usePanelResize() {
  // 响应式值（供模板绑定 title/aria，也供 reset）
  const sidebarW = ref(DEFAULTS.sidebarW);
  const rightW = ref(DEFAULTS.rightW);
  const centerTopH = ref(DEFAULTS.centerTopH);
  const terminalRatio = ref(DEFAULTS.terminalRatio);
  // 拖拽中状态（驱动 divider 视觉态）
  const resizing = ref(null); // 'sidebar' | 'right' | 'center-row' | null

  // 从 localStorage 恢复 + 应用内联变量
  const stored = readStoredLayout();
  if (stored) {
    if (typeof stored.sidebarW === 'number') sidebarW.value = stored.sidebarW;
    if (typeof stored.rightW === 'number') rightW.value = stored.rightW;
    if (typeof stored.centerTopH === 'number') centerTopH.value = stored.centerTopH;
    if (typeof stored.terminalRatio === 'number') terminalRatio.value = clamp(stored.terminalRatio, 0.18, 0.85);
    else if (typeof stored.centerTopH === 'number') {
      terminalRatio.value = clamp(stored.centerTopH / getMainHeight(), 0.18, 0.85);
      centerTopH.value = null;
    }
  }
  applyCssVars();

  let activeDrag = null; // { which, startX, startY, startVal, viewportH }

  function getTerminalHeightFromRatio() {
    return Math.round(getMainHeight() * terminalRatio.value);
  }

  function setTerminalHeight(height, root = document.documentElement) {
    root.style.setProperty('--terminal-h', `${height}px`);
    root.style.setProperty('--center-top-h', `${height}px`);
  }

  /**
   * 启动拖拽。divider 的 pointerdown 调用。
   * @param {PointerEvent} event
   * @param {'sidebar'|'right'|'center-row'} which
   */
  function startResize(event, which) {
    // 折叠态跳过对应列（sidebar/right）。center-row 无折叠态。
    if (isRegionCollapsed(which)) return;
    event.preventDefault();
    activeDrag = {
      which,
      startX: event.clientX,
      startY: event.clientY,
      viewportH: window.innerHeight,
      startVal: which === 'center-row'
        ? (centerTopH.value ?? getTerminalHeightFromRatio())
        : (which === 'sidebar' ? sidebarW.value : rightW.value)
    };
    resizing.value = which;
    document.addEventListener('pointermove', onPointerMove);
    document.addEventListener('pointerup', onPointerUp);
    // 拖拽中禁用文本选择，避免拖过文字产生选中
    document.body.style.userSelect = 'none';
  }

  function onPointerMove(event) {
    if (!activeDrag) return;
    const { which, startX, startY, startVal, viewportH } = activeDrag;

    if (which === 'sidebar') {
      // 向右拖 → 列变宽。delta = clientX - startX
      const w = clamp(startVal + (event.clientX - startX), SIDEBAR_MIN, SIDEBAR_MAX);
      sidebarW.value = w;
      setVar('--sidebar-w', `${w}px`);
    } else if (which === 'right') {
      // 右侧列：向左拖（减小 clientX）→ 列变宽。delta 取反。
      const w = clamp(startVal - (event.clientX - startX), RIGHT_MIN, RIGHT_MAX);
      rightW.value = w;
      setVar('--right-w', `${w}px`);
    } else if (which === 'center-row') {
      // 向下拖 → 终端区变高。clamp 保证文件区至少 CENTER_BOTTOM_MIN。
      const mainH = Math.max(CENTER_TOP_MIN + CENTER_BOTTOM_MIN, viewportH - 52 /*titlebar*/ - 28 /*statusbar*/);
      const maxTop = mainH - CENTER_BOTTOM_MIN;
      const h = clamp(startVal + (event.clientY - startY), CENTER_TOP_MIN, maxTop);
      centerTopH.value = h;
      terminalRatio.value = clamp(h / mainH, 0.18, 0.85);
      setTerminalHeight(h);
    }
  }

  function onPointerUp() {
    if (!activeDrag) return;
    persist();
    if (activeDrag.which === 'center-row') {
      centerTopH.value = null;
      applyCssVars();
    }
    activeDrag = null;
    resizing.value = null;
    document.removeEventListener('pointermove', onPointerMove);
    document.removeEventListener('pointerup', onPointerUp);
    document.body.style.userSelect = '';
  }

  function handleResizeKeydown(event, which) {
    const step = event.shiftKey ? 32 : 8;
    let handled = true;
    if (which === 'sidebar') {
      if (event.key === 'ArrowLeft') sidebarW.value = clamp(sidebarW.value - step, SIDEBAR_MIN, SIDEBAR_MAX);
      else if (event.key === 'ArrowRight') sidebarW.value = clamp(sidebarW.value + step, SIDEBAR_MIN, SIDEBAR_MAX);
      else handled = false;
    } else if (which === 'right') {
      if (event.key === 'ArrowLeft') rightW.value = clamp(rightW.value + step, RIGHT_MIN, RIGHT_MAX);
      else if (event.key === 'ArrowRight') rightW.value = clamp(rightW.value - step, RIGHT_MIN, RIGHT_MAX);
      else handled = false;
    } else if (which === 'center-row') {
      if (event.key === 'ArrowUp') terminalRatio.value = clamp(terminalRatio.value - 0.02, 0.18, 0.85);
      else if (event.key === 'ArrowDown') terminalRatio.value = clamp(terminalRatio.value + 0.02, 0.18, 0.85);
      else handled = false;
      centerTopH.value = null;
    } else {
      handled = false;
    }
    if (!handled) return;
    event.preventDefault();
    applyCssVars();
    persist();
  }

  function resetPane(which) {
    if (which === 'sidebar') sidebarW.value = DEFAULTS.sidebarW;
    else if (which === 'right') rightW.value = DEFAULTS.rightW;
    else if (which === 'center-row') {
      centerTopH.value = null;
      terminalRatio.value = DEFAULTS.terminalRatio;
    }
    applyCssVars();
    persist();
  }

  function setVar(name, value) {
    if (typeof document !== 'undefined' && document.documentElement) {
      document.documentElement.style.setProperty(name, value);
    }
  }

  function applyCssVars() {
    const root = (typeof document !== 'undefined') ? document.documentElement : null;
    if (!root) return;
    // 折叠态：清除内联变量，让 dataset 选择器的 44px/0 生效（内联优先级高于选择器，
    // 必须主动清除，否则折叠后仍是上次拖拽宽度）。
    // 展开态：写入内联变量恢复上次拖拽宽度。
    if (isRegionCollapsed('sidebar')) {
      root.style.removeProperty('--sidebar-w');
    } else {
      root.style.setProperty('--sidebar-w', `${sidebarW.value}px`);
    }
    if (isRegionCollapsed('right')) {
      root.style.removeProperty('--right-w');
    } else {
      root.style.setProperty('--right-w', `${rightW.value}px`);
    }
    if (centerTopH.value != null) {
      setTerminalHeight(centerTopH.value, root);
    } else {
      setTerminalHeight(getTerminalHeightFromRatio(), root);
    }
  }

  function persist() {
    try {
      localStorage.setItem(LAYOUT_STORAGE_KEY, JSON.stringify({
        sidebarW: sidebarW.value,
        rightW: rightW.value,
        terminalRatio: terminalRatio.value
      }));
    } catch {
      /* localStorage 不可用，静默 */
    }
  }

  /**
   * 恢复默认布局：清 localStorage + 重置内联变量为默认 + 重置响应式值。
   */
  function resetLayout() {
    sidebarW.value = DEFAULTS.sidebarW;
    rightW.value = DEFAULTS.rightW;
    centerTopH.value = DEFAULTS.centerTopH;
    terminalRatio.value = DEFAULTS.terminalRatio;
    // 清除内联变量（让 grid 回到 fallback 值：sidebar 260, right 280, center-top 1fr）
    if (typeof document !== 'undefined' && document.documentElement) {
      document.documentElement.style.removeProperty('--sidebar-w');
      document.documentElement.style.removeProperty('--right-w');
      document.documentElement.style.removeProperty('--center-top-h');
      document.documentElement.style.removeProperty('--terminal-h');
    }
    try {
      localStorage.removeItem(LAYOUT_STORAGE_KEY);
      localStorage.removeItem(LEGACY_LAYOUT_STORAGE_KEY);
    } catch { /* ignore */ }
    // 折叠态不覆盖：折叠列保持折叠（reset 不展开折叠态，用户可单独点展开）。
    applyCssVars();
  }

  onBeforeUnmount(() => {
    document.removeEventListener('pointermove', onPointerMove);
    document.removeEventListener('pointerup', onPointerUp);
    if (typeof window !== 'undefined') window.removeEventListener('resize', applyCssVars);
  });

  if (typeof window !== 'undefined') window.addEventListener('resize', applyCssVars);

  return {
    sidebarW,
    rightW,
    centerTopH,
    terminalRatio,
    resizing,
    startResize,
    handleResizeKeydown,
    resetPane,
    resetLayout,
    // 折叠态变化后调用：重新同步内联变量（折叠列清除内联、展开列恢复宽度）。
    // App.vue 的 onToggleAssets/onToggleRight 切换 store 后调一次。
    syncCollapse: applyCssVars
  };
}
