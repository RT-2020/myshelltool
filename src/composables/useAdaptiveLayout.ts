/**
 * useAdaptiveLayout — 源头自适应布局：窗口变窄时布局自我重组（栏位折叠），
 * 替代整体缩放（字号不变、信息密度按断点降级）。
 *
 * 断点（视口 CSS px，每个窗口独立判定）：
 *   bp2 (max-width: 1023.5px) — 自动折叠右栏（280→0）
 *   bp3 (max-width: 859.5px)  — 侧栏自动收成 44px 图标栏（资产窗口无侧栏，跳过）
 * 窄档（1024–1279）的标题栏/状态栏细节降档是纯 CSS
 * （workbench-shell.narrow.scss），本 composable 只负责需要 store 联动的折叠。
 *
 * 不变量（勿破坏）：
 *   1. 自动折叠一律走 setRightCollapsed/setAssetsCollapsed（persist=false、
 *      不 announce）——用户手动折叠偏好（localStorage）永不被覆盖；
 *   2. 退出断点时只恢复「被自动折叠的」栏（autoCollapsed 标记），用户手动
 *      折叠的保持原样；
 *   3. 用户在自动折叠期间手动展开某栏 → watch 发现实际值与自动预期不符，
 *      清除该栏标记，此后断点不再代管该栏（用户手势优先）。
 *
 * 为什么不能用纯 CSS 断点（勘察结论）：scss 媒体查询改 --sidebar-w 会与
 * usePanelResize 写入的内联变量、store/dataset 状态、rail 渲染（依赖 store
 * ref）脱节——44px 列里塞完整侧栏内容会破版。折叠必须走 store action。
 * matchMedia 单阈值无迟滞：窗口在阈值附近徘徊会各切换一次，200ms grid
 * 过渡吸收，实测若觉抖动再考虑 40px 迟滞带。
 */
import { onBeforeUnmount, watch } from 'vue';
import { useUiStore } from '../stores/ui';
import { isAssetWindowMode } from '../lib/assetWindows';
import { isTauriRuntime, maximizeTauriWindow } from '../services/backend';

const BP2_QUERY = '(max-width: 1023.5px)'; // 紧凑档：折右栏
const BP3_QUERY = '(max-width: 859.5px)';  // 极限档：侧栏收 44px rail

export function useAdaptiveLayout() {
  const ui = useUiStore();
  const autoCollapsed = { right: false, sidebar: false };
  const isAssetWindow = isAssetWindowMode();

  function applyBp2(matches: boolean) {
    if (matches) {
      if (!ui.rightCollapsed) {
        autoCollapsed.right = true;
        ui.setRightCollapsed(true);
      }
    } else if (autoCollapsed.right) {
      autoCollapsed.right = false;
      ui.setRightCollapsed(false);
    }
  }

  function applyBp3(matches: boolean) {
    if (isAssetWindow) return; // 资产窗口无侧栏
    if (matches) {
      if (!ui.assetsCollapsed) {
        autoCollapsed.sidebar = true;
        ui.setAssetsCollapsed(true);
      }
    } else if (autoCollapsed.sidebar) {
      autoCollapsed.sidebar = false;
      ui.setAssetsCollapsed(false);
    }
  }

  // 用户手势优先：自动折叠的栏被手动展开后，退出断点不再代管恢复
  const stopWatchRight = watch(() => ui.rightCollapsed, collapsed => {
    if (autoCollapsed.right && !collapsed) autoCollapsed.right = false;
  });
  const stopWatchSidebar = watch(() => ui.assetsCollapsed, collapsed => {
    if (autoCollapsed.sidebar && !collapsed) autoCollapsed.sidebar = false;
  });

  const mql2 = window.matchMedia(BP2_QUERY);
  const mql3 = window.matchMedia(BP3_QUERY);
  const onBp2Change = (event: MediaQueryListEvent) => applyBp2(event.matches);
  const onBp3Change = (event: MediaQueryListEvent) => applyBp3(event.matches);
  mql2.addEventListener('change', onBp2Change);
  mql3.addEventListener('change', onBp3Change);

  // 首开窗口比屏幕工作区还宽时（默认 1366 落在 1280 屏/高 DPI 小屏上）直接
  // 最大化，避免「首开即越界」。平台假设：screen.availWidth 为主屏工作区
  // （CSS px），窗口开在副屏可能误触发——代价只是一次最大化，可接受。
  if (isTauriRuntime() && window.screen.availWidth > 0 && window.innerWidth > window.screen.availWidth) {
    maximizeTauriWindow().catch(error => console.warn('[adaptiveLayout] maximize failed:', error));
  }

  // 初始评估（窗口一开就在断点内的场景：恢复的小窗、小屏最大化）
  applyBp2(mql2.matches);
  applyBp3(mql3.matches);

  onBeforeUnmount(() => {
    mql2.removeEventListener('change', onBp2Change);
    mql3.removeEventListener('change', onBp3Change);
    stopWatchRight();
    stopWatchSidebar();
  });
}
