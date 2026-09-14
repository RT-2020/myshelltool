/**
 * fileTransfers — 传输执行域：上传/下载执行体、覆盖确认链、传输队列元操作
 * 与进度事件监听（从 stores/files.ts 按域拆出，v2.8 第五轮）。
 * 绑定式 context（先例 lib/terminalLifecycle）；面板浏览在 lib/filePanel。
 */
import { ref, type Ref } from 'vue';
import type { NormalizedConnectionAsset, NotifyOptions, RemoteFileEntry, TransferProgressPayload } from '@/types/domain';
import { invokeBackend, isTauriRuntime, listenBackendEvent } from '@/services/backend';
import { joinLocalPath, joinPath } from '@/stores/workbench';
import { buildTransferItem } from '@/lib/transferUtils';
import { errorMessage } from '@/lib/errorMessage';
import type { FilesSessionLike, FilesWorkbenchBridge, QueueItem } from '@/lib/fileTypes';

const TRANSFER_PROGRESS_EVENT = 'sftp-transfer-progress';

let progressUnlisten: TauriUnlistenFn | null = null;

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
  entry: File | RemoteFileEntry | null;
  remoteTarget: string;
  resolve: (choice: boolean) => void;
  settled: boolean;
}

let bound: FileTransfersContext | null = null;

export function bindFileTransfersContext(ctx: FileTransfersContext): void {
  bound = ctx;
}

function tc(): FileTransfersContext {
  if (!bound) throw new Error('fileTransfers 未绑定 context（files store 未初始化）');
  return bound;
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
 * 取消上传：置 cancelled 标记，分块循环在下一块前检查并停止。
 * 下载仍不可取消——流式化后字节不再经过 IPC（后端直接写本地文件），但
 * `sftp_download_to_file` 依旧是单次 invoke、后端没有中断通道，前端置标记也
 * 无法让后端停下。TransferDrawer 对下载行不渲染取消按钮（勿造假按钮）。
 */
export function cancelTransfer(id: string) {
  const item = tc().transferQueue.value.find(entry => entry.id === id);
  if (!item) return;
  if (item.direction !== 'upload') return;
  if (item.status !== 'running' && item.status !== 'pending') return;
  item.cancelled = true;
  tc().announce('正在取消上传：' + item.name, { level: 'warn' });
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
      if (item.op?.kind === 'localEntry') {
        await runLocalEntryUpload(item, session);
      } else if (item.op?.kind === 'file') {
        await runFileUpload(item, session);
      } else {
        // op 缺失（异常数据/旧队列项）：无法重试
        markTransferError(id, '传输参数缺失');
        tc().announce('传输参数缺失，无法重试：' + item.name, { level: 'warn' });
      }
    } else {
      await runDownload(item, session);
    }
  } catch (error) {
    markTransferError(id, errorMessage(error));
    tc().announce('传输失败：' + item.name + '：' + errorMessage(error), {
      level: 'error',
      action: { label: '重试', run: () => retryTransfer(id) }
    });
  }
}

/**
 * 三态探测目标路径是否已存在：true = 明确存在；false = 明确不存在（stat
 * 返回 NoSuchFile）；null = 无法确认（无会话 / stat 其他错误）。调用方对
 * null 一律按「可能存在」处理（走覆盖确认）——stat 报错 ≠ 不存在，一律当
 * false 放行会绕过覆盖确认、无提示覆盖远端已有文件。
 */
export async function probeRemoteTarget(remoteTarget: string): Promise<boolean | null> {
  const session = tc().getActiveSession();
  if (!session) return null;
  try {
    const result = await tc().statRemotePath(remoteTarget);
    return !!result;
  } catch (error) {
    // russh-sftp 的 StatusCode::NoSuchFile Display 为 "No such file"（后端
    // 包装成 "SFTP stat failed: No such file: ..."），据此区分「明确不存在」
    // 与「无法确认」；其余错误（权限/超时/连接断）保守返回 null。
    const message = error instanceof Error ? error.message : String(error || '');
    return /no such file/i.test(message) ? false : null;
  }
}

/**
 * 上传前同名确认：入队并返回一个 Promise，由 confirmFileOverwrite /
 * cancelFileOverwrite 逐个（串行）resolve。
 * resolve(true) 继续上传；resolve(false) 跳过该文件（不中断整批）。
 * 队列保证每个 Promise 都会 settle（用户选择、或批次中止时的收尾清理），
 * 不存在永久挂起的上传循环。
 */
export function askFileOverwrite(entry: File | RemoteFileEntry, remoteTarget: string) {
  let settleOwn: (() => void) | null = null;
  const promise = new Promise<boolean>(resolve => {
    tc().overwriteQueue.value.push({ entry, remoteTarget, resolve, settled: false });
    settleOwn = () => settleOwnFileOverwrite(resolve);
    // 队列为空时本次即队首，立即展示；否则当前弹窗仍归上一项，等它 settle 后推进。
    const head = tc().overwriteQueue.value[0];
    if (head && head.resolve === resolve) {
      tc().wb().modal = { type: 'confirmFileOverwrite', payload: { path: remoteTarget } };
    }
  });
  return {
    promise,
    /**
     * 只结算「本次这一条」确认（批次中止时用）。返回的句柄绑定的是本次的
     * resolve，因此不会碰到并发批次或其他文件留下的 pending。
     */
    settle: () => settleOwn?.()
  };
}

/**
 * 只 settle「自己发起的那条」覆盖确认（批次中止时用）。
 *
 * 为什么不能直接 `handleFileOverwrite(false)`：那只 pop 队首，若此刻队首属于
 * 另一个并发批次，就会把**别人的**确认当成「取消」静默掉。这里按 resolve 引用
 * 定位自己的条目，已 settle 则跳过；命中的若不是队首也不影响弹窗（队列推进由
 * confirm/cancel 负责），只保证自己那个 Promise 不会永久悬空。
 */
function settleOwnFileOverwrite(pendingResolve: (choice: boolean) => void) {
  const mine = tc().overwriteQueue.value.find(item => item.resolve === pendingResolve);
  if (!mine || mine.settled) return;
  mine.settled = true;
  mine.resolve(false);
  // 若自己这条正是队首（弹窗显示的也是它），必须把弹窗推进到下一项或关掉，
  // 否则会出现「队列已 settle 但弹窗还停在一条已失效的确认上」。
  if (tc().overwriteQueue.value[0] === mine) {
    tc().overwriteQueue.value.shift();
    const next = tc().overwriteQueue.value[0];
    tc().wb().modal =
      tc().overwriteQueue.value.length && next
        ? { type: 'confirmFileOverwrite', payload: { path: next.remoteTarget } }
        : { type: null };
  } else {
    tc().overwriteQueue.value = tc().overwriteQueue.value.filter(item => item !== mine);
  }
}

/**
 * settle 队首并出队（confirm/cancel 的公共收尾）：队列还有下一项就继续显示
 * 下一个确认弹窗，空了才关闭弹窗。
 */
export function handleFileOverwrite(choice: boolean) {
  const pending = tc().overwriteQueue.value.shift();
  if (pending) {
    pending.settled = true;
    pending.resolve(choice);
  }
  const next = tc().overwriteQueue.value[0];
  // 用 length 判定而不是 `next ? ... : ...`：三元在 false 分支返回 undefined，
  // 会被 vue-tsc（exactOptionalPropertyTypes）判为不符合 ModalState。
  tc().wb().modal =
    tc().overwriteQueue.value.length && next
      ? { type: 'confirmFileOverwrite', payload: { path: next.remoteTarget } }
      : { type: null };
}

export function confirmFileOverwrite() {
  handleFileOverwrite(true);
}

export function cancelFileOverwrite() {
  // 提示必须取「被取消的那一项」的名字，不是随手一项
  const pending = tc().overwriteQueue.value[0];
  if (pending) {
    tc().announce('已跳过同名文件' + (pending.entry?.name ? '：' + pending.entry.name : ''), { level: 'warn' });
  }
  handleFileOverwrite(false);
}

/** 远程写操作前置守卫：remotePath 未加载（空串）时 joinPath('', name) 会折叠到
 * 文件系统根（`/name`），把文件传到 /. 未加载一律中止并提示，不猜测目标目录。 */
function requireRemotePath(): boolean {
  if (tc().remotePath.value) return true;
  tc().announce('远程目录尚未加载，请先刷新后再操作', { level: 'warn' });
  return false;
}

/** 上传浏览器 File（input/drag-drop）。顺序处理：每个文件先做覆盖检查，取消则跳过继续下一批。 */
export async function uploadFiles(fileList?: FileList | File[] | null) {
  const session = tc().getActiveSession();
  if (!session) {
    tc().announce(tc().noSessionMessage('上传'), { level: 'warn' });
    tc().wb().setTab('terminal');
    await tc().wb().sessionsStore()?.connectSelected();
    return;
  }
  const files = Array.from(fileList || []);
  if (!files.length) return;
  if (!requireRemotePath()) return;
  // 基准目录快照：remotePath 是全局单例，而覆盖确认弹窗会长时间挂起本循环——
  // 期间用户切换资产（handleAssetSelected → resetRemotePanel 清空 remotePath）
  // 或终端 cd（OSC 7）都会改写它。循环内读实时值会让剩余文件 joinPath('', name)
  // 折叠成 '/name'，整批静默落到远端根目录。基准目录 + 资产归属校验一起兜住。
  const baseDir = tc().remotePath.value;
  const assetId = tc().wb().selectedAsset?.id ?? null;
  // 已完成处理（传完或被用户跳过）的文件数：中止时 files.length - handledCount
  // 就是「未处理」的剩余文件数。
  let handledCount = 0;
  let aborted = false;
  for (const file of files) {
    // 每轮开始先校验归属：资产已变（或面板被清空）就中止整批剩余文件，
    // 绝不把用户给 A 资产选的文件传到 B 资产/根目录。
    if (aborted || (tc().wb().selectedAsset?.id ?? null) !== assetId || !baseDir) {
      aborted = true;
      break;
    }
    const remoteTarget = joinPath(baseDir, file.name);
    // 本次文件自己的覆盖确认句柄：中止收尾时只结算这一条（不碰并发批次/其它文件的 pending）。
    let overwrite: { promise: Promise<boolean>; settle: () => void } | null = null;
    // null（无法确认）按「可能存在」处理：宁可多问一次，不无提示覆盖远端文件
    if ((await probeRemoteTarget(remoteTarget)) !== false) {
      overwrite = askFileOverwrite(file, remoteTarget);
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
      name: file.name,
      assetId,
      remotePath: remoteTarget,
      total: file.size,
      op: { kind: 'file', file, remoteTarget }
    });
    tc().transferQueue.value.push(item);
    await withTransfer(transferId, () => runFileUpload(item, session));
    handledCount += 1;
  }
  if (aborted) {
    // 中止时的确认已由 break 前的 overwrite.settle() 单独结算（只碰本次这一条）。
    // 这里刻意**不**清空整个 overwriteQueue：队列里可能有并发批次或其它资产的
    // pending，清空会把它们静默判成「取消」。另一条 break（循环头守卫）发生在
    // 请求确认之前，本条目根本没有 pending，因此不存在遗漏。
    tc().announce('已切换资产，剩余 ' + (files.length - handledCount) + ' 个文件未上传', { level: 'warn' });
  }
  // 无论是否中止，都刷新「当前面板」（当前资产的目录）：本批可能已传完一部分，
  // 列表要如实反映落盘结果。不能拿 baseDir 去刷——那可能已不属于当前面板。
  await tc().refreshRemoteFiles(null).catch(() => null);
}

/** 分块上传执行体（uploadFiles 与 retryTransfer 共用）。 */
async function runFileUpload(item: QueueItem, session: FilesSessionLike) {
  const transferId = item.id;
  // 调用契约：仅 op.kind === 'file' 的队列项进入本函数（uploadFiles/retryTransfer 分发保证）
  const uploadFile = (item.op as { file: File } | null)?.file;
  try {
    await invokeBackend('sftp_upload_start', {
      sessionId: session.sessionId,
      remotePath: item.remotePath,
      transferId
    });
    const CHUNK_SIZE = 8 * 1024 * 1024;
    // 以「短块(<CHUNK_SIZE)/0 字节」为真 EOF，不信任 drop 时刻的 file.size：
    // 文件传输期间增长（日志/导出中）不截断，照 runLocalEntryUpload 契约。
    let offset = 0;
    while (true) {
      if (item.cancelled) break; // cancelTransfer 标记 → 停在当前 chunk 边界
      const bytes = new Uint8Array(await uploadFile!.slice(offset, offset + CHUNK_SIZE).arrayBuffer());
      if (!bytes.length) break; // 0 字节 = 空文件或已读完
      offset += bytes.length;
      await invokeBackend('sftp_upload_chunk', {
        sessionId: session.sessionId,
        chunk: bytes,
        transferId,
        bytesTransferred: offset,
        // 进度条分母（item.total 仅作显示基准）：文件可能已增长，取 max 防超 100%
        totalBytes: Math.max(item.total, offset)
      });
      if (bytes.length < CHUNK_SIZE) break; // 短块 = 真 EOF
    }
    if (item.cancelled) {
      item.status = 'cancelled';
      item.finishedAt = Date.now();
      item.speed = 0;
      item.eta = null;
      pruneFinishedTransfers();
      tc().announce('已取消上传：' + item.name, { level: 'warn' });
      // 后端无 abort 通道：finalize 兜底释放分块会话（可能落盘部分数据，best-effort）
      try {
        await invokeBackend('sftp_upload_finalize', { transferId });
      } catch {
        // best-effort cleanup
      }
      return;
    }
    await invokeBackend('sftp_upload_finalize', { transferId });
    item.status = 'done';
    item.percent = 100;
    item.transferred = offset;
    // 文件大小在 drop 后变化（增长/收缩）：内容已按实际读到的完整上传，状态仍
    // done，仅 warn 对账差异（item.total > 0 时才比对，0 起步的增长不弹）
    if (item.total > 0 && offset !== item.total) {
      tc().announce('文件在传输期间发生变化：预期 ' + item.total + ' 字节，实际 ' + offset + ' 字节：' + item.name, { level: 'warn' });
    }
    item.total = Math.max(item.total, offset);
    item.eta = null;
    item.finishedAt = Date.now();
    pruneFinishedTransfers();
    tc().announce('已上传：' + item.name, { level: 'success' });
  } catch (error) {
    markTransferError(transferId, errorMessage(error));
    tc().announce('上传失败：' + item.name + '：' + errorMessage(error), {
      level: 'error',
      action: { label: '重试', run: () => retryTransfer(transferId) }
    });
    try {
      await invokeBackend('sftp_upload_finalize', { transferId });
    } catch {
      // best-effort cleanup
    }
  }
}

/** 上传本地条目（FileColumn 双击本地文件 / 右键「上传到远程」）。 */
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
  if (!requireRemotePath()) return;
  // 与 uploadFiles 同源：目标路径与所属资产必须取同一时刻的快照。
  // 下面两个 await（stat 探测、同名确认）可能让用户切换资产或让终端 cd 触发
  // resetRemotePanel 清空 remotePath —— 用实时 remotePath 继续拼路径会把文件
  // 送到远端根（joinPath('', name) = '/name'），用实时会话则可能送到另一台机器。
  const baseDir = tc().remotePath.value;
  const assetId = tc().wb().selectedAsset?.id ?? null;
  const remoteTarget = joinPath(baseDir, entry.name);
  let overwrite: { promise: Promise<boolean>; settle: () => void } | null = null;
  // null（无法确认）按「可能存在」处理：宁可多问一次，不无提示覆盖远端文件
  if ((await probeRemoteTarget(remoteTarget)) !== false) {
    overwrite = askFileOverwrite(entry, remoteTarget);
    if (!(await overwrite.promise)) return;
  }
  // 迟到守卫：确认弹窗期间用户可能已切换资产，此时本次上传已不属于当前面板
  if ((tc().wb().selectedAsset?.id ?? null) !== assetId) {
    // 只结算「本次这一条」确认（句柄绑定的是本次的 resolve）。不能用
    // handleFileOverwrite：它 pop 的是队首，若队首属于并发批次，就会把别人的
    // 确认静默判成取消；同理也不能按路径字符串比对面首（并发同路径会撞上）。
    overwrite?.settle();
    tc().announce('已切换资产，已取消本次上传：' + entry.name, { level: 'warn' });
    return;
  }
  const transferId = 'up-local-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8);
  const item = buildTransferItem({
    id: transferId,
    direction: 'upload',
    name: entry.name,
    assetId,
    remotePath: remoteTarget,
    total: Number(entry.size) || 0,
    op: { kind: 'localEntry', entry, remoteTarget }
  });
  tc().transferQueue.value.push(item);
  await withTransfer(transferId, () => runLocalEntryUpload(item, session));
}

/** 本地条目分块上传执行体（uploadLocalEntry 与 retryTransfer 共用）。 */
async function runLocalEntryUpload(item: QueueItem, session: FilesSessionLike) {
  const transferId = item.id;
  // 调用契约：仅 op.kind === 'localEntry' 的队列项进入本函数
  const localEntry = (item.op as { entry: RemoteFileEntry } | null)?.entry;
  try {
    await invokeBackend('sftp_upload_start', {
      sessionId: session.sessionId,
      remotePath: item.remotePath,
      transferId
    });
    const chunkSize = 8 * 1024 * 1024;
    // 以「短块」为 EOF 判据，不信任目录列表时刻的 item.total：文件在列表后
    // 增长（日志/导出中）不截断，size 记 0 的非空文件也能传完整。后端契约：
    // fs_local_read_chunk 返回长度 < 请求长度 ⟺ 真 EOF（读满或读到 0）。
    let offset = 0;
    while (true) {
      if (item.cancelled) break;
      const chunk = await invokeBackend<number[]>('fs_local_read_chunk', {
        path: localEntry!.path,
        offset,
        length: chunkSize
      });
      const bytes = Array.isArray(chunk) ? chunk : Array.from(chunk || []);
      if (!bytes.length) break; // 0 字节 = 空文件或已读完
      offset += bytes.length;
      await invokeBackend('sftp_upload_chunk', {
        sessionId: session.sessionId,
        chunk: bytes,
        transferId,
        bytesTransferred: offset,
        // 进度条分母（item.total 仅作显示基准）：文件可能已增长，取 max 防超 100%
        totalBytes: Math.max(item.total, offset)
      });
      if (bytes.length < chunkSize) break; // 短块 = 真 EOF
    }
    if (item.cancelled) {
      item.status = 'cancelled';
      item.finishedAt = Date.now();
      item.speed = 0;
      item.eta = null;
      pruneFinishedTransfers();
      tc().announce('已取消上传：' + item.name, { level: 'warn' });
      try {
        await invokeBackend('sftp_upload_finalize', { transferId });
      } catch {
        // best-effort cleanup
      }
      return;
    }
    await invokeBackend('sftp_upload_finalize', { transferId });
    item.status = 'done';
    item.percent = 100;
    item.transferred = offset;
    // 文件大小在列表后变化（增长/收缩）：内容已按当前读到的完整上传，状态仍
    // done，仅 warn 提示差异（item.total > 0 时才比对，0 起步的增长不弹）
    if (item.total > 0 && offset !== item.total) {
      tc().announce('文件在上传期间大小已变化（列目录时 ' + item.total + ' 字节，实际上传 ' + offset + ' 字节）：' + item.name, { level: 'warn' });
    }
    item.total = Math.max(item.total, offset);
    item.eta = null;
    item.finishedAt = Date.now();
    pruneFinishedTransfers();
    tc().announce('已上传本地文件：' + item.name, { level: 'success' });
    await tc().refreshRemoteFiles(tc().remotePath.value).catch(() => null);
  } catch (error) {
    markTransferError(transferId, errorMessage(error));
    tc().announce('上传本地文件失败：' + item.name + '：' + errorMessage(error), {
      level: 'error',
      action: { label: '重试', run: () => retryTransfer(transferId) }
    });
    try {
      await invokeBackend('sftp_upload_finalize', { transferId });
    } catch {
      // best-effort cleanup
    }
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
    tc().announce('已下载：' + item.name + ' → ' + tc().localPath.value, { level: 'success' });
  } catch (error) {
    markTransferError(transferId, errorMessage(error));
    tc().announce('下载失败：' + item.name + '：' + errorMessage(error), {
      level: 'error',
      action: { label: '重试', run: () => retryTransfer(transferId) }
    });
  }
}
