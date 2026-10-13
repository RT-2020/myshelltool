/**
 * crossWindowEvents — 跨窗口事件总线封装（v0.21 B1：多窗口同步）。
 *
 * Tauri 2 的 event.emit 是全局广播：主窗口与所有资产窗口都会收到。事件全部
 * 携带 source（当前窗口 label），接收方据此忽略自己发出的事件，避免回环。
 *
 * 失败姿态：非 Tauri runtime / emit 失败只影响「实时同步」这项增强，不影响
 * 主流程（窗口仍能独立工作），故静默降级为无同步，不向用户抛错。
 */
import { emitBackendEvent, getTauriWindow } from '../services/backend';

export const CROSS_WINDOW_EVENTS = {
  /** 主题切换（payload: { theme: ThemeMode }） */
  THEME_CHANGED: 'app-theme-changed',
  /** 资产库变更（增删改/分组/同步拉取；payload: { count?: number }） */
  ASSETS_CHANGED: 'assets-changed',
  /** 资产窗口打开（payload: { assetId: string }），主窗口据此记入恢复列表 */
  ASSET_WINDOW_OPENED: 'asset-window-opened',
  /** 资产窗口关闭（payload: { assetId: string }），主窗口据此移出恢复列表 */
  ASSET_WINDOW_CLOSED: 'asset-window-closed'
};

/** 当前窗口 label（非 Tauri/取不到时为空串；空串事件不会被当成「自己发的」）。 */
export function currentWindowLabel(): string {
  return getTauriWindow()?.label || '';
}

/** 广播跨窗口事件（自动带 source）。失败静默降级，不阻断调用方。 */
export async function emitCrossWindow(
  eventName: string,
  payload: Record<string, unknown> = {}
): Promise<void> {
  try {
    await emitBackendEvent(eventName, { ...payload, source: currentWindowLabel() });
  } catch {
    /* 非 Tauri runtime 或事件通道失败：跨窗同步是增强能力，不影响本窗口主流程 */
  }
}

/** 事件是否由当前窗口自己发出（避免自己 reload/apply 自己的广播）。 */
export function isCrossWindowEventFromSelf(payload: unknown): boolean {
  const source = (payload as { source?: unknown } | null)?.source;
  const self = currentWindowLabel();
  return Boolean(source && self && source === self);
}
