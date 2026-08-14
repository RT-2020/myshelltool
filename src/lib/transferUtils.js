/**
 * transferUtils — S2 传输队列工具函数
 *
 * 从 files.js 抽出的纯函数：速度/ETA 格式化 + 传输队列项构造。
 * files.js 只负责调用，不内联这些计算（控制 store 行数增长，见 AGENTS.md 文件上限）。
 */

/**
 * 格式化瞬时速度（B/s → "1.2 MB/s" / "350 KB/s"）。
 * <=0 或非法值返回 ''，调用方据此隐藏速度行。
 */
export function formatSpeed(bytesPerSec) {
  const bps = Number(bytesPerSec) || 0;
  if (bps <= 0) return '';
  if (bps >= 1024 * 1024) return (bps / 1024 / 1024).toFixed(1) + ' MB/s';
  if (bps >= 1024) return (bps / 1024).toFixed(1) + ' KB/s';
  return Math.round(bps) + ' B/s';
}

/**
 * 格式化剩余时间（秒 → "12s" / "3m 20s" / "1h 5m"）。
 * 非法值（null/undefined/NaN/<=0）返回 ''，调用方据此隐藏 ETA。
 */
export function formatEta(seconds) {
  const sec = Number(seconds);
  if (!Number.isFinite(sec) || sec <= 0) return '';
  if (sec < 60) return Math.ceil(sec) + 's';
  const mins = Math.floor(sec / 60);
  if (mins < 60) {
    const rest = Math.ceil(sec % 60);
    return rest > 0 ? `${mins}m ${rest}s` : `${mins}m`;
  }
  const hours = Math.floor(mins / 60);
  return `${hours}h ${mins % 60}m`;
}

/**
 * 构造传输队列项。op 保存原始操作参数，供 retryTransfer 复用：
 *   - 上传（浏览器 File）  : { kind: 'file', file, remoteTarget }
 *   - 上传（本地条目）     : { kind: 'localEntry', entry, remoteTarget }
 *   - 下载                : { kind: 'download', entry }
 */
export function buildTransferItem({ id, direction, name, remotePath, total, op }) {
  return {
    id,
    direction,
    name,
    remotePath,
    op: op || null,
    transferred: 0,
    total: Number(total) || 0,
    percent: 0,
    status: 'running',
    startedAt: Date.now(),
    finishedAt: null,
    speed: 0,        // B/s 瞬时速度（updateTransferProgress 维护）
    eta: null,       // 剩余秒数（null = 未知/已完成）
    cancelled: false,
    error: null
  };
}
