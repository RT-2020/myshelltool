/**
 * crossWindowSyncRuntime — 跨窗口同步监听实现（v0.21 B1）。
 *
 * 从 workbench store 抽出：workbench.ts 已贴 500 行硬上限，这里用结构化最小
 * 接口接收 store，避免把监听逻辑继续堆进编排壳。
 *
 * 职责：
 * - 主题广播 → applyExternalTheme（非自己发出）；
 * - 资产变更广播 → reloadAssets({ broadcast: false })（非自己发出），
 *   资产窗口且本资产已不在列表时置 defunct 走关窗；
 * - 资产窗口打开/关闭广播 → 主窗口维护 localStorage 恢复列表。
 *
 * 监听句柄由本模块自持，disposeCrossWindowSync() 统一解绑。
 */
import { listenBackendEvent } from '@/services/backend';
import { CROSS_WINDOW_EVENTS, isCrossWindowEventFromSelf } from './crossWindowEvents';
import { markAssetWindowClosed, markAssetWindowOpened } from './windowState';

export interface CrossWindowSyncUiStore {
  applyExternalTheme(theme: string): void;
  markAssetWindowDefunct(): void;
}

export interface CrossWindowSyncAssetsStore {
  assets: Array<{ id?: string }>;
  reloadAssets(options?: { broadcast?: boolean }): Promise<void>;
}

export type CrossWindowSyncAnnounce = (message: string, options?: { level?: string }) => unknown;

let unlistens: Array<() => void> = [];

/** 当前 webview 是否是资产窗口，以及其 assetId（模块加载即定，无响应式需求）。 */
function assetWindowId(): string | null {
  try {
    return new URLSearchParams(location.search).get('assetId');
  } catch {
    /* 非浏览器环境（不应发生）：按主窗口处理 */
    return null;
  }
}

export async function installCrossWindowSync(
  ui: CrossWindowSyncUiStore,
  assets: CrossWindowSyncAssetsStore,
  announce: CrossWindowSyncAnnounce,
  assetMode: boolean
): Promise<void> {
  const myAssetId = assetMode ? assetWindowId() : null;

  const themeUn = await listenBackendEvent(CROSS_WINDOW_EVENTS.THEME_CHANGED, event => {
    if (isCrossWindowEventFromSelf(event.payload)) return;
    const next = (event.payload as { theme?: unknown } | null)?.theme;
    if (typeof next === 'string') ui.applyExternalTheme(next);
  });

  const assetsUn = await listenBackendEvent(CROSS_WINDOW_EVENTS.ASSETS_CHANGED, event => {
    if (isCrossWindowEventFromSelf(event.payload)) return;
    assets
      .reloadAssets({ broadcast: false })
      .then(() => {
        // 资产窗口：本资产已被删除 → 壳组件走关窗流程（在途传输/未保存编辑器
        // 守卫仍生效，不静默销毁丢数据）。
        if (assetMode && myAssetId && !assets.assets.some(asset => asset.id === myAssetId)) {
          ui.markAssetWindowDefunct();
        }
      })
      .catch(error => announce('跨窗口资产同步失败：' + String(error)));
  });

  const openedUn = await listenBackendEvent(CROSS_WINDOW_EVENTS.ASSET_WINDOW_OPENED, event => {
    const id = (event.payload as { assetId?: unknown } | null)?.assetId;
    if (!assetMode && typeof id === 'string' && id) markAssetWindowOpened(id);
  });

  const closedUn = await listenBackendEvent(CROSS_WINDOW_EVENTS.ASSET_WINDOW_CLOSED, event => {
    const id = (event.payload as { assetId?: unknown } | null)?.assetId;
    if (!assetMode && typeof id === 'string' && id) markAssetWindowClosed(id);
  });

  unlistens.push(themeUn, assetsUn, openedUn, closedUn);
}

/** 解绑全部跨窗监听（幂等；重复调用安全）。 */
export function disposeCrossWindowSync(): void {
  for (const unlisten of unlistens.splice(0)) {
    try {
      unlisten();
    } catch {
      /* 监听已失效时 unlisten 可能抛错：清理阶段忽略，不影响其余解绑 */
    }
  }
}
