/**
 * fileTransfers — 传输执行域：上传/下载执行体、覆盖确认链、传输队列元操作
 * 与进度事件监听（从 stores/files.ts 按域拆出，v2.8 第五轮）。
 * 绑定式 context（先例 lib/terminalLifecycle）；面板浏览在 lib/filePanel。
 */
import { ref, type Ref } from 'vue';
import type { NormalizedConnectionAsset, NotifyOptions, RemoteFileEntry, TransferProgressPayload } from '@/types/domain';
import { invokeBackend, isTauriRuntime, listenBackendEvent } from '@/services/backend';
import { joinLocalPath, joinPath, parentLocalPath } from '@/stores/workbench';
import { buildTransferItem, formatBytes } from '@/lib/transferUtils';
import { errorMessage } from '@/lib/errorMessage';
import { bindFileOverwrite, requireRemotePath, probeRemoteTarget, askFileOverwrite } from '@/lib/fileOverwrite';
// v0.20（S2 刀）：队列管理域拆至 lib/transferQueueOps.ts（进度/状态机/取消/重试）
import {
  bindTransferQueueOps, setupEventListeners, disposeEventListeners, updateTransferProgress,
  pruneFinishedTransfers, markTransferError, withTransfer, cancelTransfer, retryTransfer
} from '@/lib/transferQueueOps';

// v0.20（S2 刀）：覆盖确认链已拆至 lib/fileOverwrite.ts，调用方路径不变（re-export）。
export { handleFileOverwrite, confirmFileOverwrite, cancelFileOverwrite } from './fileOverwrite';
export { setupEventListeners, disposeEventListeners, cancelTransfer, retryTransfer, pruneFinishedTransfers } from './transferQueueOps';
// files store 直接 import 的队列域符号（值 import 供 runPathUpload/runDownload 消费，转发保导出）
export { updateTransferProgress, markTransferError, withTransfer };
export { probeRemoteTarget, askFileOverwrite };
import { openLocalPath, revealLocalItem } from '@/lib/openLocalFile';
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

/** 上传覆盖确认暂存（原 files store 的 PendingFileOverwrite，随域迁移）。 */
export interface PendingOverwrite {
  entry: RemoteFileEntry | null;
  remoteTarget: string;
  resolve: (choice: boolean) => void;
  settled: boolean;
}

let bound: FileTransfersContext | null = null;

export function bindFileTransfersContext(ctx: FileTransfersContext): void {
  bound = ctx;
  // v0.20（S2 刀）：级联注入覆盖确认链与队列管理域（同一 ctx；重试执行器
  // 传函数引用——runPathUpload/runDownload 为函数声明，提升可用，惰性调用）
  bindFileOverwrite(ctx);
  bindTransferQueueOps(ctx, { runPathUpload, runDownload });
}

function tc(): FileTransfersContext {
  if (!bound) throw new Error('fileTransfers 未绑定 context（files store 未初始化）');
  return bound;
}

/** 上传入口①：本机路径批量上传（原生文件对话框 / OS 拖入，FileSurface 接线）。
 *  逐条 fs_local_stat 拿名称/大小/类型——目录在此过滤跳过（与旧 File 路径的
 *  目录跳过 UX 一致），大小作队列进度分母初始值（真实分母以进度事件为准）。 */
export async function uploadLocalPaths(paths: string[]) {
  const session = tc().getActiveSession();
  if (!session) {
    tc().announce(tc().noSessionMessage('上传'), { level: 'warn' });
    tc().wb().setTab('terminal');
    await tc().wb().sessionsStore()?.connectSelected();
    return;
  }
  const list = (paths || []).filter(p => typeof p === 'string' && p);
  if (!list.length) return;
  const fileEntries: RemoteFileEntry[] = [];
  let dirCount = 0;
  for (const p of list) {
    try {
      const st = await invokeBackend<RemoteFileEntry>('fs_local_stat', { path: p });
      if (st && st.kind === 'file') fileEntries.push(st);
      else dirCount += 1;
    } catch (error) {
      // 单条 stat 失败（权限/系统目录黑名单/不存在）不拖垮整批：报错跳过
      tc().announce('无法读取本地文件：' + p + '：' + errorMessage(error), { level: 'error' });
    }
  }
  if (dirCount) tc().announce('已跳过 ' + dirCount + ' 个目录（暂不支持目录上传）', { level: 'warn' });
  if (fileEntries.length) await uploadLocalFileEntries(fileEntries, session);
}

/** 上传入口②：本地文件面板条目（FileColumn 双击 / 右键「上传到远程」/ 栏间拖拽）。 */
export async function uploadLocalEntry(entry: RemoteFileEntry | null) {
  const session = tc().getActiveSession();
  if (!session) {
    tc().announce(tc().noSessionMessage('上传'), { level: 'warn' });
    tc().wb().setTab('terminal');
    return;
  }
  if (!entry || entry.kind !== 'file') {
    tc().announce('只能上传本地文件', { level: 'warn' });
    return;
  }
  await uploadLocalFileEntries([entry], session);
}

/**
 * 共用批次体（两入口汇合）：顺序处理，每个文件先做覆盖检查，取消则跳过继续
 * 下一批。基准目录/资产快照 + 逐条复核的守卫与旧 uploadFiles 一致——覆盖确认
 * 弹窗会长时间挂起本循环，期间用户切换资产（resetRemotePanel 清空 remotePath）
 * 或终端 cd（OSC 7）都会改写实时值，用实时值拼路径会让剩余文件落到远端根目录。
 */
async function uploadLocalFileEntries(entries: RemoteFileEntry[], session: FilesSessionLike) {
  if (!requireRemotePath()) return;
  const baseDir = tc().remotePath.value;
  const assetId = tc().wb().selectedAsset?.id ?? null;
  // 批次 id：UploadProgressStrip（上传区下方进度条）按它聚合「本次上传」的
  // 总体进度与逐文件分项；关闭提示按批次记忆，新批次自动重新出现。
  const batchId = 'upbatch-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8);
  // 已完成处理（传完或被用户跳过）的文件数：中止时 entries.length - handledCount
  // 就是「未处理」的剩余文件数。
  let handledCount = 0;
  let aborted = false;
  for (const entry of entries) {
    // 每轮开始先校验归属：资产已变（或面板被清空）就中止整批剩余文件，
    // 绝不把用户给 A 资产选的文件传到 B 资产/根目录。
    if (aborted || (tc().wb().selectedAsset?.id ?? null) !== assetId || !baseDir) {
      aborted = true;
      break;
    }
    const remoteTarget = joinPath(baseDir, entry.name);
    // 本次文件自己的覆盖确认句柄：中止收尾时只结算这一条（不碰并发批次/其它文件的 pending）。
    let overwrite: { promise: Promise<boolean>; settle: () => void } | null = null;
    // null（无法确认）按「可能存在」处理：宁可多问一次，不无提示覆盖远端文件
    if ((await probeRemoteTarget(remoteTarget)) !== false) {
      overwrite = askFileOverwrite(entry, remoteTarget);
      if (!(await overwrite.promise)) {
        handledCount += 1;
        continue;
      }
    }
    // 弹窗期间资产可能已切换：判定放在 await 之后，避免把当前文件传错资产
    if ((tc().wb().selectedAsset?.id ?? null) !== assetId) {
      // 只结算「本次文件这一条」确认：handleFileOverwrite 会 pop 队首，若此刻队首
      // 属于并发批次或队列里更早的历史项，就会把别人的确认静默当成「取消」。
      overwrite?.settle();
      aborted = true;
      break;
    }
    const transferId = 'up-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8);
    const item = buildTransferItem({
      id: transferId,
      direction: 'upload',
      name: entry.name,
      assetId,
      batchId,
      remotePath: remoteTarget,
      total: Number(entry.size) || 0,
      op: { kind: 'localPath', localPath: entry.path, remoteTarget }
    });
    tc().transferQueue.value.push(item);
    await withTransfer(transferId, () => runPathUpload(item, session));
    handledCount += 1;
  }
  if (aborted) {
    // 中止时的确认已由 break 前的 overwrite.settle() 单独结算（只碰本次这一条）。
    // 这里刻意**不**清空整个 overwriteQueue：队列里可能有并发批次或其它资产的
    // pending，清空会把它们静默判成「取消」。另一条 break（循环头守卫）发生在
    // 请求确认之前，本条目根本没有 pending，因此不存在遗漏。
    tc().announce('已切换资产，剩余 ' + (entries.length - handledCount) + ' 个文件未上传', { level: 'warn' });
  }
  // 无论是否中止，都刷新「当前面板」：本批可能已传完一部分，列表要如实反映落盘
  // 结果。null = 刷新面板当前目录（filePanel 契约，未加载时才回落服务器默认目录）
  // ——不能拿 baseDir 去刷（用户可能已导航走，会把别的目录错标成本批落点），
  // 更不能把面板拽回登录家目录（修复前 null 直通空 path，canonicalize 即家目录）。
  await tc().refreshRemoteFiles(null).catch(() => null);
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
    tc().announce('已上传：' + item.name, { level: 'success' });
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

export async function downloadEntry(entry: RemoteFileEntry, destDir?: string | null) {
  const session = tc().getActiveSession();
  if (!session) {
    tc().announce(tc().noSessionMessage('下载'), { level: 'warn' });
    return;
  }
  if (entry.kind === 'directory') {
    tc().announce('暂不支持递归下载目录：' + entry.name, { level: 'warn' });
    return;
  }
  const transferId = 'down-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8);
  const item = buildTransferItem({
    id: transferId,
    direction: 'download',
    name: entry.name,
    assetId: tc().wb().selectedAsset?.id ?? null,
    remotePath: entry.path,
    total: entry.size || 0,
    op: { kind: 'download', entry, localPath: null, destDir: destDir ?? null }
  });
  tc().transferQueue.value.push(item);
  await withTransfer(transferId, () => runDownload(item, session));
}

// ============================================================
// 下载完成通知（右下角 toast + 点击打开）
//
// 聚合：同一目录 1.5s 窗口内的完成合并为一条——批量下载逐文件弹会把
// toast 上限 5 条顶掉，用户根本来不及点「打开」。这里的 setTimeout 是
// 纯 UI 去抖（只 flush，不做「赌 N ms 后状态就绪」的探测等待）。
// 时长 10s：比带按钮默认 8s 再宽一点，给足移动鼠标点击的窗口。
// ============================================================
interface DownloadDoneRecord {
  name: string;
  localPath: string;
  size: number;
}

const DOWNLOAD_DONE_WINDOW_MS = 1500;
const DOWNLOAD_TOAST_DURATION_MS = 10_000;
const downloadDoneBuffer = new Map<string, DownloadDoneRecord[]>();
const downloadDoneFlushTimers = new Map<string, ReturnType<typeof setTimeout>>();

function queueDownloadDoneToast(destDir: string, record: DownloadDoneRecord) {
  const list = downloadDoneBuffer.get(destDir) ?? [];
  list.push(record);
  downloadDoneBuffer.set(destDir, list);
  const existing = downloadDoneFlushTimers.get(destDir);
  if (existing) clearTimeout(existing);
  downloadDoneFlushTimers.set(
    destDir,
    setTimeout(() => flushDownloadDoneToasts(destDir), DOWNLOAD_DONE_WINDOW_MS)
  );
}

function flushDownloadDoneToasts(destDir: string) {
  downloadDoneFlushTimers.delete(destDir);
  const records = downloadDoneBuffer.get(destDir) ?? [];
  downloadDoneBuffer.delete(destDir);
  if (records.length === 0) return;
  if (records.length === 1) {
    const rec = records[0]!;
    const sizeLabel = rec.size > 0 ? `（${formatBytes(rec.size)}）` : '';
    tc().announce(`已下载：${rec.name}${sizeLabel} → ${destDir}`, {
      level: 'success',
      duration: DOWNLOAD_TOAST_DURATION_MS,
      actions: [
        { label: '打开', run: () => void runLocalOpenAction('file', rec.localPath) },
        { label: '所在文件夹', run: () => void runLocalOpenAction('reveal', rec.localPath) }
      ]
    });
    return;
  }
  tc().announce(`已下载 ${records.length} 个文件 → ${destDir}`, {
    level: 'success',
    duration: DOWNLOAD_TOAST_DURATION_MS,
    actions: [{ label: '打开文件夹', run: () => void runLocalOpenAction('folder', destDir) }]
  });
}

/** toast 按钮 → plugin-opener；浏览器预览模式降级提示，opener 失败如实报原因。 */
async function runLocalOpenAction(kind: 'file' | 'reveal' | 'folder', path: string) {
  try {
    const result =
      kind === 'reveal' ? await revealLocalItem(path) : await openLocalPath(path);
    if (result === 'unsupported') {
      tc().announce('浏览器预览模式没有本地文件系统能力，请在桌面版中使用该操作', { level: 'warn' });
    }
  } catch (error) {
    const what = kind === 'reveal' ? '打开所在文件夹' : '打开';
    tc().announce(`${what}失败：${errorMessage(error)}`, { level: 'error' });
  }
}

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
