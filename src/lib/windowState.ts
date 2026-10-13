/**
 * windowState — 独立资产窗口的「上次打开列表」持久化（v0.21 B1）。
 *
 * 只存 assetId；重启后主窗口 boot 完成、资产列表就绪时按 id 恢复窗口。
 * 存储放主窗口 localStorage：跨窗口实时性由 asset-window-opened/closed
 * 事件保证（主窗口收到事件后写入自己的 localStorage），单窗口重启持久性
 * 由 localStorage 保证。
 *
 * 不存窗口几何（后续独立项）：本批只解决「窗口不持久化恢复」。
 */

const OPEN_WINDOWS_KEY = 'myshelltool-open-asset-windows:v1';

function readIds(): string[] {
  try {
    const raw = localStorage.getItem(OPEN_WINDOWS_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return [...new Set(parsed.map(String).filter(Boolean))];
  } catch {
    /* localStorage 不可用/JSON 损坏：按「没有需要恢复的窗口」处理 */
    return [];
  }
}

function writeIds(ids: string[]): void {
  try {
    localStorage.setItem(OPEN_WINDOWS_KEY, JSON.stringify([...new Set(ids.filter(Boolean))]));
  } catch {
    /* localStorage 不可用：恢复能力降级，不影响当前会话 */
  }
}

/** 读取待恢复的资产 id 列表（主窗口 boot 用）。 */
export function readOpenAssetWindows(): string[] {
  return readIds();
}

/** 记录一个已打开的资产窗口（幂等）。 */
export function markAssetWindowOpened(assetId: string): void {
  if (!assetId) return;
  const ids = readIds();
  if (ids.includes(assetId)) return;
  writeIds([...ids, assetId]);
}

/** 记录一个已关闭的资产窗口（幂等）。 */
export function markAssetWindowClosed(assetId: string): void {
  if (!assetId) return;
  writeIds(readIds().filter(id => id !== assetId));
}
