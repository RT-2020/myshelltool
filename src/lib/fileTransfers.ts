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
  entry: RemoteFileEntry | null;
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
 * 取消上传：置 cancelled 标记（UI 即时反馈）+ 调 `sftp_upload_cancel` 置后端
 * 共享旗标——流式上传循环在下一个块边界（≤1 MiB）看到旗标即中止，并尽力
 * 删除远端半截文件。后端 reject 后由 runPathUpload 按 cancelled 标记收敛状态。
 * 下载仍不可取消——`sftp_download_to_file` 是单次 invoke、后端没有中断通道，
 * TransferDrawer 对下载行不渲染取消按钮（勿造假按钮）。
 */
export function cancelTransfer(id: string) {
  const item = tc().transferQueue.value.find(entry => entry.id === id);
  if (!item) return;
  if (item.direction !== 'upload') return;
  if (item.status !== 'running' && item.status !== 'pending') return;
  item.cancelled = true;
  tc().announce('正在取消上传：' + item.name, { level: 'warn' });
  // 取消命令失败（会话已断等）可忽略：上传随即会因写失败以错误态收尾，
  // 状态同样由 runPathUpload 收敛，不会因为取消命令丢失而卡 running。
  invokeBackend('sftp_upload_cancel', { transferId: id }).catch(() => null);
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
        await runPathUpload(item, session);
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
export function askFileOverwrite(entry: RemoteFileEntry, remoteTarget: string) {
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
  // 无论是否中止，都刷新「当前面板」（当前资产的目录）：本批可能已传完一部分，
  // 列表要如实反映落盘结果。不能拿 baseDir 去刷——那可能已不属于当前面板。
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
