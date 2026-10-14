/**
 * fileOverwrite — 上传/下载同名冲突三选链（B2：覆盖/跳过/重命名）。
 * 队列语义自 v0.20 二选版延续（串行 settle / settleOwn 防跨批次误结算 /
 * requireRemotePath 防目标目录折叠到根），决策从 boolean 拓宽为
 * OverwriteDecision；上传与下载共用同一队列与弹窗（direction 区分文案与
 * 重命名探测的目标端）。
 * ctx 与 fileTransfers 共享 FileTransfersContext（bindFileTransfersContext
 * 级联注入）；调用方仍经 fileTransfers 的 re-export 取函数。
 */
import { reactive } from 'vue';
import { invokeBackend } from '@/services/backend';
import type { RemoteFileEntry } from '@/types/domain';
import type { OverwriteDecision, PendingOverwrite } from '@/lib/fileTypes';
import type { FileTransfersContext } from '@/lib/fileTransfers';
import { joinLocalPath, joinPath } from '@/stores/workbench';

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
    // B2 并发修复：后端 sftp_stat 契约与 fs_local_stat 对齐——SSH_FX_NO_SUCH_FILE
    // 在后端结构化判定为 Ok(null)，Err 一律「无法确认」。此前前端对错误串做
    // /no such file/i 正则匹配（形状猜测）：错误文案一变就会把「无法确认」
    // 误判成「不存在」，绕过覆盖确认静默覆盖远端文件。
    const result = await tc().statRemotePath(remoteTarget);
    return !!result;
  } catch {
    return null;
  }
}

/**
 * 下载前探测本地同名目标（B2 下载冲突门）。三态与 probeRemoteTarget 对齐：
 * fs_local_stat 后端契约——Ok(null)=明确不存在（ErrorKind::NotFound），
 * Err=无法确认（权限/路径黑名单/运行时不可用，保守走确认）。
 */
export async function probeLocalTarget(localTarget: string): Promise<boolean | null> {
  try {
    const st = await invokeBackend<RemoteFileEntry | null>('fs_local_stat', { path: localTarget });
    return !!st;
  } catch {
    return null;
  }
}

// ============================================================
// 批次默认策略（「应用到本批剩余冲突」勾选）
// 用户在某次冲突上勾选 applyAll 并选了覆盖/跳过后，同批次后续冲突直接按
// 该策略 resolve 不再弹窗；重命名永远逐条（自动编号易撞名，不参与）。
// 条目随批次产生、体量极小（每批一条），不做主动清理。
// ============================================================
const batchOverwriteDefaults = new Map<string, 'overwrite' | 'skip'>();

/** 冲突弹窗表单态：FileOverwriteContent 双向绑定，测试亦可直改后提交。 */
export const overwriteForm = reactive({
  rename: '',
  error: '',
  applyAll: false
});

/** 冲突建议名：`report.txt` → `report (1).txt`；无扩展名/隐藏文件整体加后缀。 */
export function suggestConflictName(name: string): string {
  const dot = name.lastIndexOf('.');
  if (dot <= 0) return name + ' (1)';
  return name.slice(0, dot) + ' (1)' + name.slice(dot);
}

function showModalFor(item: PendingOverwrite) {
  // 每次展示（含队列推进到下一项）都重置表单并预填建议名——上一项的勾选/
  // 错误不跨项残留
  overwriteForm.rename = suggestConflictName(item.entry?.name || '');
  overwriteForm.error = '';
  overwriteForm.applyAll = false;
  tc().wb().modal = {
    type: 'confirmFileOverwrite',
    payload: {
      path: item.target,
      direction: item.direction,
      name: item.entry?.name ?? '',
      dir: item.dir,
      batchId: item.batchId ?? null
    }
  };
}

/**
 * 传输前同名确认：入队并返回一个 Promise，由 confirmFileOverwrite /
 * cancelFileOverwrite / renameFileOverwrite 逐个（串行）resolve。
 * resolve({kind:'overwrite'}) 继续传输；{kind:'skip'} 跳过该文件（不中断
 * 整批）；{kind:'rename', name} 以新名拼目标路径继续。
 * 批次默认策略命中（用户此前勾选「应用到剩余」）时不入队直接 resolve。
 * 队列保证每个 Promise 都会 settle（用户选择、或批次中止时的收尾清理），
 * 不存在永久挂起的传输循环。
 */
export function askFileOverwrite(opts: {
  entry: RemoteFileEntry | null;
  target: string;
  dir: string;
  direction: 'upload' | 'download';
  batchId?: string;
}) {
  const preset = opts.batchId ? batchOverwriteDefaults.get(opts.batchId) : undefined;
  let settleOwn: (() => void) | null = null;
  const promise = new Promise<OverwriteDecision>(resolve => {
    if (preset) {
      resolve({ kind: preset });
      return;
    }
    const item: PendingOverwrite = {
      entry: opts.entry,
      target: opts.target,
      dir: opts.dir,
      direction: opts.direction,
      batchId: opts.batchId,
      resolve,
      settled: false
    };
    tc().overwriteQueue.value.push(item);
    settleOwn = () => settleOwnFileOverwrite(resolve);
    // 队列为空时本次即队首，立即展示；否则当前弹窗仍归上一项，等它 settle 后推进。
    const head = tc().overwriteQueue.value[0];
    if (head && head.resolve === resolve) {
      showModalFor(item);
    }
  });
  return {
    promise,
    /**
     * 只结算「本次这一条」确认（批次中止时用）。返回的句柄绑定的是本次的
     * resolve，因此不会碰到并发批次或其他文件留下的 pending；批次默认
     * 命中的快路径没有入队，settle 为空操作。
     */
    settle: () => settleOwn?.()
  };
}

/**
 * 只 settle「自己发起的那条」覆盖确认（批次中止时用）。
 *
 * 为什么不能直接 `handleFileOverwrite({kind:'skip'})`：那只 pop 队首，若此刻
 * 队首属于另一个并发批次，就会把**别人的**确认当成「取消」静默掉。这里按
 * resolve 引用定位自己的条目，已 settle 则跳过；命中的若不是队首也不影响
 * 弹窗（队列推进由 confirm/cancel/rename 负责），只保证自己那个 Promise
 * 不会永久悬空。
 */
function settleOwnFileOverwrite(pendingResolve: (choice: OverwriteDecision) => void) {
  const mine = tc().overwriteQueue.value.find(item => item.resolve === pendingResolve);
  if (!mine || mine.settled) return;
  mine.settled = true;
  mine.resolve({ kind: 'skip' });
  // 若自己这条正是队首（弹窗显示的也是它），必须把弹窗推进到下一项或关掉，
  // 否则会出现「队列已 settle 但弹窗还停在一条已失效的确认上」。
  if (tc().overwriteQueue.value[0] === mine) {
    tc().overwriteQueue.value.shift();
    const next = tc().overwriteQueue.value[0];
    if (next) showModalFor(next);
    else tc().wb().modal = { type: null };
  } else {
    tc().overwriteQueue.value = tc().overwriteQueue.value.filter(item => item !== mine);
  }
}

/**
 * settle 队首并出队（confirm/cancel/rename 的公共收尾）：队列还有下一项就
 * 继续显示下一个确认弹窗，空了才关闭弹窗。
 */
export function handleFileOverwrite(decision: OverwriteDecision) {
  const pending = tc().overwriteQueue.value.shift();
  if (pending) {
    pending.settled = true;
    pending.resolve(decision);
  }
  const next = tc().overwriteQueue.value[0];
  if (next) showModalFor(next);
  else tc().wb().modal = { type: null };
}

function recordBatchDefault(kind: 'overwrite' | 'skip') {
  const pending = tc().overwriteQueue.value[0];
  if (overwriteForm.applyAll && pending?.batchId) {
    batchOverwriteDefaults.set(pending.batchId, kind);
    // 并发批次（B2 并发修复）：同批其它文件的确认可能已随并发探测提前入队
    // 在等弹窗——「应用到本批剩余冲突」按字面覆盖它们，就地按默认结算出队
    //（否则勾选形同虚设：并发下「剩余」大多已在队列里，弹窗会一个个再来）。
    // 只结算非队首项：队首是本次决策本身，随后的 handleFileOverwrite 负责。
    if (tc().overwriteQueue.value.length > 1) {
      const keep: PendingOverwrite[] = [];
      for (const item of tc().overwriteQueue.value) {
        if (item !== pending && item.batchId === pending.batchId && !item.settled) {
          item.settled = true;
          item.resolve({ kind });
        } else {
          keep.push(item);
        }
      }
      tc().overwriteQueue.value.splice(0, tc().overwriteQueue.value.length, ...keep);
    }
  }
}

export function confirmFileOverwrite() {
  recordBatchDefault('overwrite');
  handleFileOverwrite({ kind: 'overwrite' });
}

export function cancelFileOverwrite() {
  // 提示必须取「被取消的那一项」的名字，不是随手一项
  const pending = tc().overwriteQueue.value[0];
  if (pending) {
    tc().announce('已跳过同名文件' + (pending.entry?.name ? '：' + pending.entry.name : ''), { level: 'warn' });
  }
  recordBatchDefault('skip');
  handleFileOverwrite({ kind: 'skip' });
}

/**
 * 冲突三选之「重命名」：校验名称格式后按方向探测新目标——已存在或无法确认
 * 都保持弹窗报错（fail-closed：宁可误拦让用户换名，不冒静默覆盖新目标的风险），
 * 探测通过才 settle。applyAll 勾选对重命名无效（按钮侧已禁用）。
 */
export async function renameFileOverwrite(rawName: string) {
  const pending = tc().overwriteQueue.value[0];
  if (!pending) return;
  const name = String(rawName || '').trim();
  if (!name) {
    overwriteForm.error = '请输入新名称';
    return;
  }
  if (name.includes('/') || name.includes('\\')) {
    overwriteForm.error = '名称不能包含路径分隔符 / 或 \\';
    return;
  }
  if (name === '.' || name === '..') {
    overwriteForm.error = '名称不能为 . 或 ..';
    return;
  }
  if (pending.entry?.name && name === pending.entry.name) {
    overwriteForm.error = '新名称与当前相同；如需覆盖请直接点「覆盖」';
    return;
  }
  overwriteForm.error = '';
  const candidate =
    pending.direction === 'upload'
      ? joinPath(pending.dir, name)
      : joinLocalPath(pending.dir, name);
  const exists = pending.direction === 'upload'
    ? await probeRemoteTarget(candidate)
    : await probeLocalTarget(candidate);
  // 队列可能已被并发批次推进/清空：结算前复核队首仍是当初那条
  if (tc().overwriteQueue.value[0] !== pending) return;
  if (exists === true) {
    overwriteForm.error = '新名称的目标也已存在，请换一个名称';
    return;
  }
  if (exists === null) {
    overwriteForm.error = '无法确认新名称目标是否可用（连接/权限问题），请重试或选择覆盖/跳过';
    return;
  }
  handleFileOverwrite({ kind: 'rename', name });
  tc().announce(
    '已重命名' + (pending.direction === 'upload' ? '上传' : '下载') + '为：' + name,
    { level: 'info' }
  );
}

/** 远程写操作前置守卫：remotePath 未加载（空串）时 joinPath('', name) 会折叠到
 * 文件系统根（`/name`），把文件传到 /. 未加载一律中止并提示，不猜测目标目录。 */
export function requireRemotePath(): boolean {
  if (tc().remotePath.value) return true;
  tc().announce('远程目录尚未加载，请先刷新后再操作', { level: 'warn' });
  return false;
}
