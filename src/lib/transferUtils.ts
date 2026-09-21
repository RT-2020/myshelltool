/**
 * transferUtils — S2 传输队列工具函数
 *
 * 从 files.js 抽出的纯函数：速度/ETA 格式化 + 传输队列项构造。
 * files.js 只负责调用，不内联这些计算（控制 store 行数增长，见 AGENTS.md 文件上限）。
 */

/** retryTransfer 复用的原始操作参数（见 buildTransferItem 注释）。 */
export type TransferOp =
  // 上传（本机路径）：字节不经 IPC，后端 sftp_upload_from_file 读盘直写 SFTP。
  | { kind: 'localPath'; localPath: string; remoteTarget: string }
  // 下载：下载已改为「后端流式写入本地文件」，因此需要记住用户选定的落盘路径。
  // localPath 为 null 表示「尚未选定」——首次执行时弹系统目录选择框，
  // 重试时复用已解析路径（避免重试让用户重选一次）。
  // destDir：本次选定的保存目录。批量下载时由第一个文件选定、同批其余文件复用，
  // 因此整批只弹一次目录框（不是每个文件一次）。
  | { kind: 'download'; entry: unknown; localPath: string | null; destDir?: string | null };

/** 传输队列项（files store 的 transferQueue 元素形状）。 */
export interface TransferQueueItem {
  id: string;
  direction: string; // 'upload' | 'download'
  name: string;
  /**
   * 发起本次传输时所属资产的 id（Wave: 跨资产归属修复）。
   *
   * 为什么必须有它：重试（retryTransfer）原先用「当前活跃会话」解析目标，
   * 用户在 A 上失败了传输、切到 B 再点重试，重试会**在 B 上按 A 的路径继续**——
   * 路径与文件名都来自原传输项，落到另一台服务器上（可能静默成功）。
   * 记下发起时的资产 id，重试时按它解析会话，解析不到就明确拒绝而不是换机器。
   *
   * 可选：历史队列项（该字段引入前构造的）没有此值，调用方需容忍 undefined。
   */
  assetId?: string | null;
  remotePath: string;
  op: TransferOp | null;
  transferred: number;
  total: number;
  percent: number;
  status: string;
  startedAt: number;
  finishedAt: number | null;
  speed: number;      // B/s 瞬时速度（updateTransferProgress 维护）
  eta: number | null; // 剩余秒数（null = 未知/已完成）
  cancelled: boolean;
  error: string | null;
}

export interface BuildTransferItemArgs {
  id: string;
  direction: string;
  name: string;
  remotePath: string;
  total?: number | null;
  op?: TransferOp | null;
  /** 发起时的资产 id（见 TransferQueueItem.assetId）。 */
  assetId?: string | null;
}

/**
 * 格式化字节数（B → "12 KB" / "3 MB"）。
 * 从 TransferDrawer 的私有实现上移至此（下载完成 toast 也要展示大小，
 * 避免第三份拷贝）；0/非法值返回 "0 B"。
 */
export function formatBytes(bytes: number | null | undefined): string {
  const size = Number(bytes) || 0;
  if (size >= 1024 * 1024) return Math.round(size / 1024 / 1024) + ' MB';
  if (size >= 1024) return Math.round(size / 1024) + ' KB';
  return size + ' B';
}

/**
 * 格式化瞬时速度（B/s → "1.2 MB/s" / "350 KB/s"）。
 * <=0 或非法值返回 ''，调用方据此隐藏速度行。
 */
export function formatSpeed(bytesPerSec: number | null | undefined): string {
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
export function formatEta(seconds: number | null | undefined): string {
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
 *   - 上传（本机路径，服务端流式）: { kind: 'localPath', localPath, remoteTarget }
 *   - 下载                        : { kind: 'download', entry, localPath, destDir }
 */
export function buildTransferItem({ id, direction, name, remotePath, total, op, assetId }: BuildTransferItemArgs): TransferQueueItem {
  return {
    id,
    direction,
    name,
    assetId: assetId ?? null,
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
