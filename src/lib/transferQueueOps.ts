/**
 * transferQueueOps — 传输队列管理域（v0.20 自 fileTransfers 拆出，S2 的
 * 一刀；逻辑原样迁移）：进度事件监听接线、队列项状态机（进度/错误/裁剪）、
 * withTransfer 运行守卫、取消（上传旗标通道）与重试（按原资产解析会话——
 * 绝不用当前面板会话去猜，防「A 失败 B 重试静默落错服务器」）。
 *
 * 依赖方向：fileTransfers → 本模块（值 import：markTransferError 等被
 * runPathUpload/runDownload 消费）；本模块的重试执行器（runPathUpload/
 * runDownload）经 bindTransferQueueOps 注入（函数声明提升，惰性调用）。
 */
import type { TransferProgressPayload } from '@/types/domain';
import { invokeBackend, isTauriRuntime, listenBackendEvent } from '@/services/backend';
import { errorMessage } from '@/lib/errorMessage';
import type { FileTransfersContext, } from '@/lib/fileTransfers';
import type { QueueItem } from '@/lib/fileTypes';

const TRANSFER_PROGRESS_EVENT = 'sftp-transfer-progress';

let progressUnlisten: TauriUnlistenFn | null = null;
let ctx: FileTransfersContext | null = null;
let executors: {
  runPathUpload(item: QueueItem, session: { sessionId: string }): Promise<unknown>;
  runDownload(item: QueueItem, session: { sessionId: string }): Promise<unknown>;
} | null = null;

/** fileTransfers.bindFileTransfersContext 级联注入。 */
export function bindTransferQueueOps(
  context: FileTransfersContext,
  exec: {
    runPathUpload(item: QueueItem, session: { sessionId: string }): Promise<unknown>;
    runDownload(item: QueueItem, session: { sessionId: string }): Promise<unknown>;
  }
): void {
  ctx = context;
  executors = exec;
}

function tc(): FileTransfersContext {
  if (!ctx) throw new Error('transferQueueOps: context not bound');
  return ctx;
}

// ============================================================
// 传输进度事件监听（原 workbench.js:177-187）
// files store 自管 progressUnlisten，workbench.setupEventListeners 不再处理。
// ============================================================
export async function setupEventListeners() {
  if (!isTauriRuntime()) return;
  if (!progressUnlisten) {
    progressUnlisten = await listenBackendEvent(TRANSFER_PROGRESS_EVENT, event => {
      const { transfer_id, bytes_transferred, total_bytes } = (event.payload || {}) as TransferProgressPayload;
      updateTransferProgress(transfer_id, bytes_transferred, total_bytes);
    });
  }
}

export async function disposeEventListeners() {
  if (typeof progressUnlisten === 'function') {
    await progressUnlisten();
    progressUnlisten = null;
  }
}

export function updateTransferProgress(transferId: string, transferred: number, total: number) {
  const item = tc().transferQueue.value.find(entry => entry.id === transferId);
  if (!item) return;
  // 终态（done/error/cancelled）忽略迟到进度事件，避免状态回跳
  if (item.status !== 'running' && item.status !== 'pending') return;
  const now = Date.now();
  const deltaBytes = transferred - (item.transferred || 0);
  const deltaMs = now - (item._lastProgressAt || now);
  // 瞬时速度：本次增量/耗时（简单滑动）；无增量时保留上次速度，避免事件间隔抖动归零
  if (deltaMs > 0 && deltaBytes > 0) {
    item.speed = (deltaBytes / deltaMs) * 1000;
  }
  item._lastProgressAt = now;
  item.transferred = transferred;
  item.total = total;
  item.percent = total > 0 ? Math.min(100, Math.round((transferred / total) * 100)) : 0;
  item.eta = item.speed > 0 && total > transferred ? Math.round((total - transferred) / item.speed) : null;
  if (transferred >= total && total > 0) {
    item.status = 'done';
    item.finishedAt = Date.now();
    item.speed = 0;
    item.eta = null;
    pruneFinishedTransfers();
  }
}

export function pruneFinishedTransfers() {
  const cutoff = Date.now() - 60_000;
  tc().transferQueue.value = tc().transferQueue.value.filter(item => {
    if (item.status === 'running' || item.status === 'pending') return true;
    return (item.finishedAt || 0) > cutoff;
  });
}

export function markTransferError(transferId: string, message: string) {
  const item = tc().transferQueue.value.find(entry => entry.id === transferId);
  if (!item) return;
  item.status = 'error';
  item.error = message;
  item.finishedAt = Date.now();
  item.speed = 0;
  item.eta = null;
  // 与完成路径一致：错误项同样在 60s 后从队列清理（避免残留堆积）
  pruneFinishedTransfers();
}

export async function withTransfer<T>(itemId: string, task: () => Promise<T>) {
  const item = tc().transferQueue.value.find(entry => entry.id === itemId);
  if (item) item.status = 'running';
  return task();
}

/**
 * 取消上传：置 cancelled 标记（UI 即时反馈）+ 调 `sftp_upload_cancel` 置后端
 * 共享旗标——流式上传循环在下一个块边界（≤1 MiB）看到旗标即中止，并尽力
 * 删除远端半截文件。后端 reject 后由 runPathUpload 按 cancelled 标记收敛状态。
 * 下载仍不可取消——`sftp_download_to_file` 是单次 invoke、后端没有中断通道，
 * TransferDrawer 对下载行不渲染取消按钮（勿造假按钮）。
 */
export function cancelTransfer(id: string) {
  const item = tc().transferQueue.value.find(entry => entry.id === id);
  if (!item) return;
  // v0.20（S9）：下载也可取消（transfer_cancels 共用旗标通道）
  if (item.status !== 'running' && item.status !== 'pending') return;
  item.cancelled = true;
  tc().announce('正在取消' + (item.direction === 'upload' ? '上传' : '下载') + '：' + item.name, { level: 'warn' });
  // 取消命令失败（会话已断等）可忽略：上传随即会因写失败以错误态收尾，
  // 状态同样由 runPathUpload 收敛，不会因为取消命令丢失而卡 running。
  const cancelCmd = item.direction === 'upload' ? 'sftp_upload_cancel' : 'sftp_download_cancel';
  invokeBackend(cancelCmd, { transferId: id }).catch(() => null);
}

/**
 * 重试失败/取消的传输：复用队列项保存的原始参数（op），重新走上传/下载流程。
 * 上传重试直接续传语义（覆盖检查在上传入口已做过，此处不再弹窗）。
 */
export async function retryTransfer(id: string) {
  const item = tc().transferQueue.value.find(entry => entry.id === id);
  if (!item) return;
  if (item.status !== 'error' && item.status !== 'cancelled') return;
  // 会话必须按【发起时所属资产】解析，绝不用当前面板/活跃会话去猜。
  // 用 tc().getActiveSession() 的旧行为：在 A 上传输失败、切到 B 再点重试，重试就会
  // 带着 A 的路径在 B 上继续跑（路径与文件名都来自原队列项），可能静默成功——
  // 数据落到错误的服务器上，比报错危险得多。
  if (!item.assetId) {
    // 旧队列项（assetId 字段引入前构造的）：无法确定原资产，宁可不重试
    tc().announce('该传输项缺少所属资产信息，无法重试；请重新发起传输', { level: 'warn' });
    return;
  }
  const asset = (tc().wb().assets?.() || []).find(entry => entry?.id === item.assetId) || null;
  const session = tc().resolveSessionForAsset(asset);
  if (!session) {
    const name = asset?.name || item.assetId;
    tc().announce(`原资产「${name}」当前没有可用会话，已取消重试（不会改用其它服务器）`, { level: 'warn' });
    return;
  }
  // 重置为运行态（复用原 id，进度事件按 id 续接）
  item.status = 'running';
  item.error = null;
  item.cancelled = false;
  item.transferred = 0;
  item.percent = 0;
  item.speed = 0;
  item.eta = null;
  item.startedAt = Date.now();
  item.finishedAt = null;
  try {
    if (item.direction === 'upload') {
      if (item.op?.kind === 'localPath') {
        await executors!.runPathUpload(item, session);
      } else {
        // op 缺失（异常数据/旧队列项）：无法重试
        markTransferError(id, '传输参数缺失');
        tc().announce('传输参数缺失，无法重试：' + item.name, { level: 'warn' });
      }
    } else {
      await executors!.runDownload(item, session);
    }
  } catch (error) {
    markTransferError(id, errorMessage(error));
    tc().announce('传输失败：' + item.name + '：' + errorMessage(error), {
      level: 'error',
      action: { label: '重试', run: () => retryTransfer(id) }
    });
  }
}
