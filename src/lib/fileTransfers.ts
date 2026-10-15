/**
 * fileTransfers — 传输执行域：上传/下载执行体、覆盖确认链、传输队列元操作
 * 与进度事件监听（从 stores/files.ts 按域拆出，v2.8 第五轮）。
 * 绑定式 context（先例 lib/terminalLifecycle）；面板浏览在 lib/filePanel。
 */
import type { Ref } from 'vue';
import type { NormalizedConnectionAsset, NotifyOptions, RemoteFileEntry, TransferProgressPayload } from '@/types/domain';
import { invokeBackend, isTauriRuntime, listenBackendEvent } from '@/services/backend';
import { joinLocalPath, joinPath, parentLocalPath, parentPath } from '@/lib/pathUtils';
import { buildTransferItem } from '@/lib/transferUtils';
import { errorMessage } from '@/lib/errorMessage';
import { bindFileOverwrite, requireRemotePath, probeRemoteTarget, probeLocalTarget, askFileOverwrite } from '@/lib/fileOverwrite';
import { bindTransferDoneToasts, queueDownloadDoneToast, queueUploadDoneToast } from '@/lib/transferDoneToasts';
import type { OverwriteDecision, PendingOverwrite } from '@/lib/fileTypes';
// v0.20（S2 刀）：队列管理域拆至 lib/transferQueueOps.ts（进度/状态机/取消/重试）
import {
  bindTransferQueueOps, setupEventListeners, disposeEventListeners, updateTransferProgress,
  pruneFinishedTransfers, markTransferError, withTransfer, cancelTransfer, retryTransfer
} from '@/lib/transferQueueOps';
import {
  getTransferConcurrency, runWithConcurrencyPool, serializeOnTarget, transferTargetLockKey
} from '@/lib/transferConcurrency';

// v0.20（S2 刀）：覆盖确认链已拆至 lib/fileOverwrite.ts，调用方路径不变（re-export）。
// B2：renameFileOverwrite（三选之重命名）随迁；PendingOverwrite 权威定义在
// lib/fileTypes.ts（原先的双份同形定义已收敛）。
export { handleFileOverwrite, confirmFileOverwrite, cancelFileOverwrite, renameFileOverwrite } from './fileOverwrite';
export { setupEventListeners, disposeEventListeners, cancelTransfer, retryTransfer, pruneFinishedTransfers } from './transferQueueOps';
// files store 直接 import 的队列域符号（值 import 供 runPathUpload/runDownload 消费，转发保导出）
export { updateTransferProgress, markTransferError, withTransfer };
export { probeRemoteTarget, askFileOverwrite };
export { queueDownloadDoneToast } from './transferDoneToasts';
import type { FilesSessionLike, FilesWorkbenchBridge, QueueItem } from '@/lib/fileTypes';

export interface FileTransfersContext {
  wb(): FilesWorkbenchBridge;
  announce(message: string, opts?: NotifyOptions): void;
  transferQueue: Ref<QueueItem[]>;
  transferDrawerOpen: Ref<boolean>;
  overwriteQueue: Ref<PendingOverwrite[]>;
  remotePath: Ref<string>;
  localPath: Ref<string>;
  remoteEntries: Ref<RemoteFileEntry[]>;
  localEntries: Ref<RemoteFileEntry[]>;
  getActiveSession(): FilesSessionLike | null;
  resolveSessionForAsset(asset: NormalizedConnectionAsset | null): FilesSessionLike | null;
  statRemotePath(path: string): Promise<Record<string, unknown> | null>;
  noSessionMessage(action: string): string;
  requireRemotePath(): boolean;
  refreshRemoteFiles(path?: string | null, opts?: { silent?: boolean }): Promise<void>;
  refreshLocalFiles(path?: string | null): Promise<void>;
}

/** 上传覆盖确认暂存（权威定义 v0.20 随域迁移至 lib/fileTypes.ts，此处 import）。 */

let bound: FileTransfersContext | null = null;

export function bindFileTransfersContext(ctx: FileTransfersContext): void {
  bound = ctx;
  // v0.20（S2 刀）：级联注入覆盖确认链与队列管理域（同一 ctx；重试执行器
  // 传函数引用——runPathUpload/runDownload 为函数声明，提升可用，惰性调用）；
  // B2 后续：完成通知聚合域（transferDoneToasts）同 ctx 级联
  bindFileOverwrite(ctx);
  bindTransferQueueOps(ctx, { runPathUpload, runDownload });
  bindTransferDoneToasts(ctx);
}

function tc(): FileTransfersContext {
  if (!bound) throw new Error('fileTransfers 未绑定 context（files store 未初始化）');
  return bound;
}

// 上传/下载的公开入口（uploadLocalPaths / uploadLocalEntry / uploadDirectory /
// downloadDirectory）在 lib/dirTransfer.ts（B2：入口域与目录递归同域，且它需要
// 本模块的批次体与下载执行器——依赖方向 dirTransfer → fileTransfers 单向）。

/** 上传批次单元：文件 + 它在远端的落点目录（目录整传时为对应子目录）。 */
export interface UploadSpec {
  entry: RemoteFileEntry;
  targetDir: string;
}

/** 上传批次结果摘要（目录整传的完成提示与测试断言用）。failed = 传输执行体
 *  收敛为 error 态的文件（B2 并发修复：失败不再并入 uploaded 虚报完成数）。 */
export interface UploadBatchSummary {
  total: number;
  uploaded: number;
  failed: number;
  skipped: number;
  aborted: boolean;
}

/**
 * 共用批次体（各上传入口汇合）：顺序处理，每个文件先做冲突三选检查，跳过则
 * 继续下一批。基准目录/资产快照 + 逐条复核的守卫与旧 uploadFiles 一致——覆盖
 * 确认弹窗会长时间挂起本循环，期间用户切换资产（resetRemotePanel 清空 remotePath）
 * 或终端 cd（OSC 7）都会改写实时值，用实时值拼路径会让剩余文件落到远端根目录。
 * isCancelled（目录批次传入）：旗标置位后剩余文件不再入队（整批取消）。
 */
export async function uploadLocalFileEntries(
  specs: UploadSpec[],
  session: FilesSessionLike,
  opts: { batchId?: string; isCancelled?: () => boolean; assumedEmptyDirs?: Set<string> } = {}
): Promise<UploadBatchSummary> {
  if (!requireRemotePath()) return { total: specs.length, uploaded: 0, failed: 0, skipped: 0, aborted: true };
  const baseDir = tc().remotePath.value;
  const assetId = tc().wb().selectedAsset?.id ?? null;
  // 批次 id：UploadProgressStrip（上传区下方进度条）按它聚合「本次上传」的
  // 总体进度与逐文件分项；关闭提示按批次记忆，新批次自动重新出现。
  // 目录整传由调用方传入整批共用的 batchId（跨目录聚合为一次批次）。
  const batchId = opts.batchId || 'upbatch-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8);
  // 已完成处理（传完/跳过/失败收敛）的文件数：中止时 specs.length - handledCount
  // 就是「未处理」的剩余文件数。
  let handledCount = 0;
  let skippedCount = 0;
  let failedCount = 0;
  let aborted = false;
  let cancelledByUser = false;

  const concurrency = getTransferConcurrency();

  async function processOne(spec: UploadSpec): Promise<void> {
    // 开始先校验：批次被取消或资产已变（或面板被清空）就中止整批剩余文件，
    // 绝不把用户给 A 资产选的文件传到 B 资产/根目录。
    if (aborted) return;
    if (opts.isCancelled?.()) {
      cancelledByUser = true;
      aborted = true;
      return;
    }
    if ((tc().wb().selectedAsset?.id ?? null) !== assetId || !baseDir) {
      aborted = true;
      return;
    }
    const entry = spec.entry;
    let finalName = entry.name;
    let remoteTarget = joinPath(spec.targetDir, entry.name);
    // 本次文件自己的覆盖确认句柄：中止收尾时只结算这一条（不碰并发批次/其它文件的 pending）。
    let overwrite: { promise: Promise<OverwriteDecision>; settle: () => void } | null = null;
    // 目录整传的性能优化（B2 后续）：目标目录由本次批次的 mkdir 新建（首次成功
    // = 之前不存在 → 必为空目录）时跳过冲突探测——省 1 次 SFTP stat 往返/文件
    // （公网 RTT 下 N 文件 = N×RTT 的纯等待）。复用已存在目录的仍逐文件探测。
    const assumeFree = opts.assumedEmptyDirs?.has(spec.targetDir) ?? false;
    // null（无法确认）按「可能存在」处理：宁可多问一次，不无提示覆盖远端文件
    if (!assumeFree && (await probeRemoteTarget(remoteTarget)) !== false) {
      if (aborted || (opts.isCancelled?.() ?? false)) return;
      overwrite = askFileOverwrite({ entry, target: remoteTarget, dir: spec.targetDir, direction: 'upload', batchId });
      const decision = await overwrite.promise;
      if (decision.kind === 'skip') {
        handledCount += 1;
        skippedCount += 1;
        return;
      }
      if (decision.kind === 'rename') {
        finalName = decision.name;
        remoteTarget = joinPath(spec.targetDir, decision.name);
      }
    }
    // 弹窗期间资产可能已切换：判定放在 await 之后，避免把当前文件传错资产
    if ((tc().wb().selectedAsset?.id ?? null) !== assetId) {
      // 只结算「本次文件这一条」确认：handleFileOverwrite 会 pop 队首，若此刻队首
      // 属于并发批次或队列里更早的历史项，就会把别人的确认静默当成「取消」。
      overwrite?.settle();
      aborted = true;
      return;
    }
    if (aborted || (opts.isCancelled?.() ?? false)) {
      overwrite?.settle();
      return;
    }
    const transferId = 'up-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8);
    const item = buildTransferItem({
      id: transferId,
      direction: 'upload',
      name: finalName,
      assetId,
      batchId,
      remotePath: remoteTarget,
      total: Number(entry.size) || 0,
      op: { kind: 'localPath', localPath: entry.path, remoteTarget }
    });
    tc().transferQueue.value.push(item);
    try {
      await withTransfer(transferId, () => runPathUpload(item, session));
    } finally {
      handledCount += 1;
    }
    // runPathUpload 各路径均收敛终态（done/error/cancelled）：非 done 即失败
    // 口径（错误已逐文件 announce + 挂重试；cancelled 仅在批次中止时出现，
    // 中止分支不读 failedCount）。
    if (item.status !== 'done') failedCount += 1;
  }

  await runWithConcurrencyPool(
    specs,
    concurrency,
    spec => {
      // 同目标互斥（B2 并发修复，P0）：并发池把「探测→决策→写入」拆开后，
      // 同批/跨批对同一远端目标的两个同名文件会在彼此 create（O_TRUNC）前
      // 完成探测，双双放行 → 交错写坏远端文件。按**初始**目标路径串行化：
      // 后到者的探测必然看到先到者的落盘状态（rename 改道的不影响——原坑
      // 位要么被占要么空着，语义都正确）。不同目标照常并发。
      const initialTarget = joinPath(spec.targetDir, spec.entry.name);
      return serializeOnTarget(
        transferTargetLockKey(session.sessionId, initialTarget),
        () => processOne(spec)
      );
    },
    () => aborted || (opts.isCancelled?.() ?? false)
  );
  if (aborted) {
    // 中止时的确认已由 break 前的 overwrite.settle() 单独结算（只碰本次这一条）。
    // 这里刻意**不**清空整个 overwriteQueue：队列里可能有并发批次或其它资产的
    // pending，清空会把它们静默判成「取消」。循环头的取消/资产守卫发生在请求
    // 确认之前，本条目没有 pending，不存在遗漏。
    tc().announce(
      (cancelledByUser ? '已取消整批，剩余 ' : '已切换资产，剩余 ') + (specs.length - handledCount) + ' 个文件未上传',
      { level: 'warn' }
    );
  }
  // 无论是否中止，都刷新「当前面板」：本批可能已传完一部分，列表要如实反映落盘
  // 结果。null = 刷新面板当前目录（filePanel 契约，未加载时才回落服务器默认目录）
  // ——不能拿 baseDir 去刷（用户可能已导航走，会把别的目录错标成本批落点），
  // 更不能把面板拽回登录家目录（修复前 null 直通空 path，canonicalize 即家目录）。
  await tc().refreshRemoteFiles(null).catch(() => null);
  return {
    total: specs.length,
    uploaded: handledCount - skippedCount - failedCount,
    failed: failedCount,
    skipped: skippedCount,
    aborted
  };
}

/**
 * 流式上传执行体（uploadLocalFileEntries 与 retryTransfer 共用）。
 *
 * 单条 `sftp_upload_from_file` invoke 跑全程：后端读盘直写 SFTP，字节不经 IPC
 * （旧分块链路 8 MiB 块经 JSON 数字数组序列化约 4 倍膨胀）。进度由
 * `sftp-transfer-progress` 事件驱动（含最终一条）；取消经 cancelTransfer →
 * `sftp_upload_cancel` 旗标，后端中止并删远端半截后以错误 reject，这里按
 * item.cancelled 标记归到取消态而非错误态。
 */
async function runPathUpload(item: QueueItem, session: FilesSessionLike) {
  const transferId = item.id;
  // 调用契约：仅 op.kind === 'localPath' 的队列项进入本函数
  const localPath = (item.op as { localPath?: string } | null)?.localPath;
  if (!localPath) {
    markTransferError(transferId, '传输参数缺失');
    return;
  }
  try {
    await invokeBackend('sftp_upload_from_file', {
      sessionId: session.sessionId,
      localPath,
      remotePath: item.remotePath,
      transferId
    });
    // 进度事件通常已把 item 置 done；invoke 成功但事件缺失时兜底（同 runDownload）
    const entry = tc().transferQueue.value.find(e => e.id === transferId);
    if (entry && entry.status === 'running') {
      entry.status = 'done';
      entry.percent = 100;
      entry.transferred = Math.max(entry.transferred, entry.total);
      entry.speed = 0;
      entry.eta = null;
      entry.finishedAt = Date.now();
      pruneFinishedTransfers();
    }
    // 大小对账：stat 时刻的大小 vs 实际上传字节（文件传输期间增长不截断，
    // 内容完整上传，仅 warn 提示差异；progress 事件缺失时 transferred 不可靠，不比）
    const finalItem = entry || item;
    if (finalItem.total > 0 && finalItem.transferred > 0 && finalItem.transferred !== finalItem.total) {
      tc().announce('文件在传输期间发生变化：预期 ' + finalItem.total + ' 字节，实际 ' + finalItem.transferred + ' 字节：' + item.name, { level: 'warn' });
    }
    // B2 后续：完成通知走聚合器（同目标目录 1.5s 窗口合并——目录整传 N 文件
    // 不再 N 条 toast 刷屏；单文件 1.5s 后仍显示原「已上传：name」文案）
    queueUploadDoneToast(parentPath(item.remotePath), {
      name: item.name,
      size: finalItem.total || 0
    });
  } catch (error) {
    if (item.cancelled) {
      // 取消经后端旗标生效（中止 + 删远端半截），reject 的错误串是「已取消」
      // 而非真失败——归取消态，不挂重试按钮。
      item.status = 'cancelled';
      item.finishedAt = Date.now();
      item.speed = 0;
      item.eta = null;
      pruneFinishedTransfers();
      tc().announce('已取消上传：' + item.name, { level: 'warn' });
      return;
    }
    markTransferError(transferId, errorMessage(error));
    tc().announce('上传失败：' + item.name + '：' + errorMessage(error), {
      level: 'error',
      action: { label: '重试', run: () => retryTransfer(transferId) }
    });
  }
}

/**
 * 下载入口（单文件；目录走 dirTransfer.downloadDirectory）。B2 重构：
 * 目录对话框前移到入口（runDownload 不再首弹）；落盘目标先过本地冲突三选门
 * ——skip 即取消本条，rename 换名换路径；决策后的最终路径写入 op.localPath，
 * runDownload 直接使用（重试沿用同一路径，不再重复探测——对齐上传重试语义）。
 * B2 并发修复：返回所建队列项（终态由调用方读 item.status 判成败——传输
 * 错误在 runDownload 内部已 announce，不靠抛错传递）；null=未入队（无会话/
 * 非文件/用户取消选择/冲突弹窗选了跳过）。同目标互斥见 runDownloadEntry。
 */
export async function downloadEntry(entry: RemoteFileEntry, destDir?: string | null): Promise<QueueItem | null> {
  const session = tc().getActiveSession();
  if (!session) {
    tc().announce(tc().noSessionMessage('下载'), { level: 'warn' });
    return null;
  }
  if (entry.kind !== 'file') {
    tc().announce('目录请使用右键「下载目录」', { level: 'warn' });
    return null;
  }
  let dir = destDir ?? null;
  if (!dir) {
    dir = await invokeBackend<string | null>('plugin:dialog|open', {
      options: {
        title: '选择下载保存目录',
        directory: true,
        multiple: false,
        defaultPath: tc().localPath.value || undefined
      }
    });
    if (!dir) {
      // 用户取消选择：不是失败，不进队列不留痕迹
      tc().announce('已取消下载：' + entry.name, { level: 'warn' });
      return null;
    }
  }
  // 同目标互斥（B2 并发修复，P0）：按初始本地落点串行化「探测→决策→写入」，
  // 与上传侧对称——后到者的探测才能看到先到者的落盘（本地文件 create+
  // truncate 打开即存在），否则两个并发下载（手动双击/批次交错）写坏本地
  // 文件。目录对话框在锁外：落点未定时先问用户，不占锁等待。
  const finalDir = dir;
  return serializeOnTarget(
    transferTargetLockKey(session.sessionId, joinLocalPath(finalDir, entry.name)),
    () => runDownloadEntry(entry, finalDir, session)
  );
}

/** downloadEntry 的链化主体（同目标互斥锁内执行；并发下同批/跨批同落点排队）。 */
async function runDownloadEntry(entry: RemoteFileEntry, dir: string, session: FilesSessionLike): Promise<QueueItem | null> {
  let finalName = entry.name;
  let localTarget = joinLocalPath(dir, finalName);
  // null（无法确认）按「可能存在」处理：宁可多问一次，不无提示覆盖本地文件
  if ((await probeLocalTarget(localTarget)) !== false) {
    const decision = await askFileOverwrite({
      entry,
      target: localTarget,
      dir,
      direction: 'download'
    }).promise;
    if (decision.kind === 'skip') return null;
    if (decision.kind === 'rename') {
      finalName = decision.name;
      localTarget = joinLocalPath(dir, finalName);
    }
  }
  const transferId = 'down-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8);
  const item = buildTransferItem({
    id: transferId,
    direction: 'download',
    name: finalName,
    assetId: tc().wb().selectedAsset?.id ?? null,
    remotePath: entry.path,
    total: entry.size || 0,
    op: { kind: 'download', entry, localPath: localTarget, destDir: dir }
  });
  tc().transferQueue.value.push(item);
  await withTransfer(transferId, () => runDownload(item, session));
  return item;
}

// ============================================================
// 传输完成通知聚合（上传+下载）已拆至 lib/transferDoneToasts.ts（B2 后续：
// 上传目录整传 N 文件 = N 条 toast 刷屏，镜像下载侧 v0.18 的 1.5s 聚合先例）。
// runPathUpload/runDownload 经顶部 import 调 queueUploadDoneToast/queueDownloadDoneToast。
// ============================================================

/**
 * 下载执行体（downloadEntry 与 retryTransfer 共用）。
 *
 * v-next：下载改为「后端流式写入本地文件」。旧实现由后端把整个文件塞进
 * `Vec<u8>` 经 JSON IPC 送回前端再触发浏览器下载——1 GiB 会膨胀成约 4 GiB
 * 的 JSON 文本，前后端各驻留一份，且后端全程持 SFTP 锁。现在前端只负责
 * 选路径与展示进度，字节不经过 IPC。
 */
async function runDownload(item: QueueItem, session: FilesSessionLike) {
  const transferId = item.id;
  try {
    const op = item.op?.kind === 'download' ? item.op : null;
    // 重试复用已解析的落盘路径；首次执行（或旧队列项）让用户选一个**目录**。
    //
    // 为什么选目录而不是「保存文件」对话框：批量下载时逐个弹保存框会变成
    // 每文件一次弹窗（旧实现是静默落盘，用户本来没有这个负担）。选目录只弹
    // 一次，批次内所有文件落进同一目录、文件名沿用远端名，也省掉逐次命名。
    // 代价是不能在保存前改名 —— 对文件管理器场景可接受。
    let resolvedPath = op?.localPath ?? null;
    if (!resolvedPath) {
      const dir = op?.destDir ?? (await invokeBackend<string | null>('plugin:dialog|open', {
        options: {
          title: '选择下载保存目录',
          directory: true,
          multiple: false,
          // 默认定位到面板当前的本地目录（全局 ref 未被遮蔽，直接用）
          defaultPath: tc().localPath.value || undefined
        }
      }));
      if (!dir) {
        // 用户取消选择：不是失败，按取消处理，不留失败态与重试按钮
        const cancelled = tc().transferQueue.value.find(e => e.id === transferId);
        if (cancelled) {
          cancelled.status = 'cancelled';
          cancelled.speed = 0;
          cancelled.eta = null;
          cancelled.finishedAt = Date.now();
          pruneFinishedTransfers();
        }
        tc().announce('已取消下载：' + item.name, { level: 'warn' });
        return;
      }
      resolvedPath = joinLocalPath(dir, item.name);
      // 记住完整路径，供重试复用（op 是队列项的原始参数容器）
      if (op) {
        op.localPath = resolvedPath;
        op.destDir = dir;
      }
    }
    await invokeBackend('sftp_download_to_file', {
      sessionId: session.sessionId,
      remotePath: item.remotePath,
      localPath: resolvedPath,
      transferId
    });
    // 进度事件通常已把 item 置 done；invoke 成功但事件缺失时兜底
    const entry = tc().transferQueue.value.find(e => e.id === transferId);
    if (entry && entry.status === 'running') {
      entry.status = 'done';
      entry.percent = 100;
      entry.transferred = entry.total;
      entry.speed = 0;
      entry.eta = null;
      entry.finishedAt = Date.now();
      pruneFinishedTransfers();
    }
    // 完成通知走聚合器（同目录合并 + 「打开/所在文件夹」按钮）；
    // 目标路径用真实落盘位置，不再误显示本地面板当前目录
    const destDir = op?.destDir ?? parentLocalPath(resolvedPath);
    queueDownloadDoneToast(destDir, {
      name: item.name,
      localPath: resolvedPath,
      size: item.total || 0
    });
  } catch (error) {
    // v0.20（S9）：下载取消——后端返回 [download:cancelled] 前缀，收敛为
    // cancelled 态（用户主动行为，不是 error；不弹重试 toast）
    const msg = errorMessage(error);
    if (msg.includes(String.fromCharCode(91) + 'download:cancelled' + String.fromCharCode(93))) {
      item.status = 'cancelled';
      item.speed = 0;
      item.eta = null;
      item.finishedAt = Date.now();
      pruneFinishedTransfers();
      tc().announce('已取消下载：' + item.name, { level: 'warn' });
      return;
    }
    markTransferError(transferId, msg);
    tc().announce('下载失败：' + item.name + '：' + msg, {
      level: 'error',
      action: { label: '重试', run: () => retryTransfer(transferId) }
    });
  }
}
