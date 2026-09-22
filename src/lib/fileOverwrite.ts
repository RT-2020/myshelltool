/**
 * fileOverwrite — 上传同名覆盖确认链（v0.20 自 fileTransfers 拆出，S2 的
 * 一刀；逻辑原样迁移）。队列语义见各函数注释（串行 settle / settleOwn 防跨
 * 批次误结算 / requireRemotePath 防目标目录折叠到根）。
 * ctx 与 fileTransfers 共享 FileTransfersContext（bindFileTransfersContext
 * 级联注入）；调用方仍经 fileTransfers 的 re-export 取函数。
 */
import type { RemoteFileEntry } from '@/types/domain';
import type { FileTransfersContext } from '@/lib/fileTransfers';

let ctx: FileTransfersContext | null = null;

/** fileTransfers.bindFileTransfersContext 级联注入。 */
export function bindFileOverwrite(context: FileTransfersContext): void {
  ctx = context;
}

function tc(): FileTransfersContext {
  if (!ctx) throw new Error('fileOverwrite: context not bound');
  return ctx;
}

/**
 * 上传前探测远端同名目标。返回三态：
 * true=已存在（需确认） / false=明确不存在（No such file） / null=无法确认
 * （权限/超时/连接断——保守走确认路径；false 放行会绕过覆盖确认、无提示覆盖
 * 远端已有文件）。
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
export function requireRemotePath(): boolean {
  if (tc().remotePath.value) return true;
  tc().announce('远程目录尚未加载，请先刷新后再操作', { level: 'warn' });
  return false;
}
