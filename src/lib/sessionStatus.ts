/**
 * sessionStatus — 连接/会话状态的权威六态映射（02 #1）。
 *
 * 同时兼容两套输入值域：
 *   1. 运行时会话状态（sessions store 小写态：connected/connecting/reconnecting/disconnected/error/idle）
 *   2. 后端资产持久化初始态（PascalCase：Connected/Warning/Idle/Disconnected/Error）
 * 返回 `_utilities.scss` `.dot` 权威修饰符与中文状态词，供 ConnectionSidebar / AssetGroupNode /
 * workbench.normalizeStatus 等多方共享。
 */

export interface SessionDotStatus {
  /** 中文状态词（用于 title / aria-label / 可访问文本通道）。 */
  label: string;
  /** 全局 `.dot` 修饰符类名（idle | connected | connecting | reconnecting | disconnected | error | warn）。 */
  dotClass: string;
}

export function normalizeSessionStatus(status: string | null | undefined): SessionDotStatus {
  const s = (status || '').trim();
  switch (s) {
    case 'connected':
    case 'Connected':
    case 'running':
      return { label: '已连接', dotClass: 'connected' };
    case 'connecting':
      return { label: '连接中', dotClass: 'connecting' };
    case 'reconnecting':
      return { label: '重连中', dotClass: 'reconnecting' };
    case 'disconnected':
    case 'Disconnected':
      return { label: '已断开', dotClass: 'disconnected' };
    case 'error':
    case 'Error':
      return { label: '连接异常', dotClass: 'error' };
    case 'warning':
    case 'Warning':
    case 'warn':
      return { label: '警告', dotClass: 'warn' };
    default:
      return { label: '未连接', dotClass: 'idle' };
  }
}

/** 从同资产多个活跃/历史会话中挑出最具代表性的状态（在线 > 进行中 > 异常 > 断开）。 */
const STATUS_PRIORITY: Record<string, number> = {
  connected: 5,
  connecting: 4,
  reconnecting: 3,
  error: 2,
  disconnected: 1
};

export function pickDominantSessionStatus(statuses: Array<string | null | undefined>): string | null {
  let best: string | null = null;
  let bestRank = -1;
  for (const raw of statuses) {
    if (!raw) continue;
    const rank = STATUS_PRIORITY[raw] ?? 0;
    if (rank > bestRank) {
      bestRank = rank;
      best = raw;
    }
  }
  return best;
}
