/**
 * dirTransfer — B2 目录整传与上传公开入口域。
 *
 * 职责：本地/远端目录树收集（逐层 list，前端递归）、mkdir 幂等（失败→stat
 * 复核为目录则复用，不整目录替换）、目录上传/下载执行器（逐文件复用
 * fileTransfers 的批次体与下载入口——冲突三选、队列、进度、取消全部继承）、
 * 目录批次取消注册表（UploadProgressStrip「取消整批」与 TransferDrawer
 * 「取消批次」共用）。
 *
 * 依赖方向：dirTransfer → fileTransfers（值 import：uploadLocalFileEntries /
 * downloadEntry / cancelTransfer）。fileTransfers **不反向 import 本模块**——
 * 上传入口（uploadLocalPaths / uploadLocalEntry）随目录路由迁到本域，files
 * store 的 re-export 路径不变。ctx 经 bindDirTransferContext 注入（files store
 * 初始化时与 bindFileTransfersContext 并联调用，同一 ctx）。
 */
import { reactive } from 'vue';
import type { RemoteFileEntry } from '@/types/domain';
import { invokeBackend } from '@/services/backend';
import { joinLocalPath, joinPath } from '@/lib/pathUtils';
import { errorMessage } from '@/lib/errorMessage';
import { uploadLocalFileEntries, downloadEntry, cancelTransfer, type UploadSpec, type FileTransfersContext } from '@/lib/fileTransfers';
import type { FilesSessionLike } from '@/lib/fileTypes';
import { getTransferConcurrency, runWithConcurrencyPool } from '@/lib/transferConcurrency';

/** 递归深度上限（对齐 MCP recursive_remove_dir 的 16）。 */
const MAX_TREE_DEPTH = 16;
/** 单批文件数上限：超限明确报错中止整批（fail-closed，不静默截断）。 */
const MAX_TREE_FILES = 2000;

let ctx: FileTransfersContext | null = null;

/** files store 初始化时注入（与 bindFileTransfersContext 同一 ctx）。 */
export function bindDirTransferContext(context: FileTransfersContext): void {
  ctx = context;
}

function tc(): FileTransfersContext {
  if (!ctx) throw new Error('dirTransfer: context not bound');
  return ctx;
}

// ============================================================
// 目录批次取消注册表
// activeDirBatches 用 reactive Map（TransferDrawer 的「取消批次」按钮按
// size 判定可见性，需要响应式）；取消旗标本身不依赖响应式。
// ============================================================
const activeDirBatches = reactive(new Map<string, number>());
const dirBatchCancelled = new Map<string, boolean>();

export function isDirBatchCancelled(batchId: string): boolean {
  return dirBatchCancelled.get(batchId) === true;
}

/**
 * 取消目录批次：置旗标（收集/mkdir/传输循环看到即停，剩余文件不再入队）+
 * 对该批 running 的队列项走现有 cancelTransfer 通道（块边界中止 + 半截清理）。
 * 只碰 running：批次内文件是逐个入队逐个跑的，pending 残留项若被置
 * cancelled 标记但永远不执行，会以 pending 态卡在抽屉里。
 */
export function cancelDirBatch(batchId: string) {
  if (!activeDirBatches.has(batchId)) return;
  dirBatchCancelled.set(batchId, true);
  for (const item of tc().transferQueue.value) {
    if (item.batchId === batchId && item.status === 'running') {
      cancelTransfer(item.id);
    }
  }
}

/** 最近发起且仍在进行的目录批次（TransferDrawer「取消批次」按钮的目标）。 */
export function latestActiveDirBatch(): string | null {
  let latest: string | null = null;
  let latestAt = 0;
  for (const [id, startedAt] of activeDirBatches) {
    if (startedAt >= latestAt) {
      latest = id;
      latestAt = startedAt;
    }
  }
  return latest;
}

function beginDirBatch(): string {
  const batchId = 'dirbatch-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8);
  activeDirBatches.set(batchId, Date.now());
  return batchId;
}

function endDirBatch(batchId: string) {
  activeDirBatches.delete(batchId);
  dirBatchCancelled.delete(batchId);
}

/** 批次被取消的 unwind 信号（收集/建目录阶段的快速退出，不按错误 announce）。 */
class DirBatchCancelled extends Error {
  constructor() {
    super('dir batch cancelled');
  }
}

// ============================================================
// 目录树收集（相对路径表示：relDir 为相对批根的目录段数组）
// ============================================================
interface CollectedFile {
  entry: RemoteFileEntry;
  relDir: string[];
}

interface CollectedTree {
  /** 文件（DFS 序：父目录的文件先于子目录内容）。 */
  files: CollectedFile[];
  /** 需创建的子目录（'sub' / 'sub/inner' 形态，DFS 前序 = 父先于子）。 */
  dirs: string[];
  skippedSymlinks: number;
}

function assertCollectLimits(depth: number, fileCount: number, path: string) {
  if (depth > MAX_TREE_DEPTH) {
    throw new Error(`目录层级超过上限 ${MAX_TREE_DEPTH} 层：${path}（整批已中止）`);
  }
  if (fileCount > MAX_TREE_FILES) {
    throw new Error(`文件数超过上限 ${MAX_TREE_FILES} 个：${path}（整批已中止）`);
  }
}

async function walkLocal(batchId: string, absDir: string, rel: string[], depth: number, tree: CollectedTree) {
  if (isDirBatchCancelled(batchId)) throw new DirBatchCancelled();
  assertCollectLimits(depth, tree.files.length, absDir);
  const list = await invokeBackend<{ entries?: RemoteFileEntry[] }>('fs_local_list_dir', { path: absDir });
  for (const e of list?.entries ?? []) {
    if (isDirBatchCancelled(batchId)) throw new DirBatchCancelled();
    if (e.kind === 'symlink') {
      // 不跟随（与面板展示一致）：跳过并计数，摘要如实呈现
      tree.skippedSymlinks += 1;
      continue;
    }
    if (e.kind === 'directory') {
      const nextRel = [...rel, e.name];
      tree.dirs.push(nextRel.join('/'));
      await walkLocal(batchId, e.path, nextRel, depth + 1, tree);
    } else {
      tree.files.push({ entry: e, relDir: rel });
    }
  }
}

async function walkRemote(batchId: string, session: FilesSessionLike, absDir: string, rel: string[], depth: number, tree: CollectedTree) {
  if (isDirBatchCancelled(batchId)) throw new DirBatchCancelled();
  assertCollectLimits(depth, tree.files.length, absDir);
  const list = await invokeBackend<{ entries?: RemoteFileEntry[] }>('sftp_list_dir', {
    sessionId: session.sessionId,
    path: absDir
  });
  for (const e of list?.entries ?? []) {
    if (isDirBatchCancelled(batchId)) throw new DirBatchCancelled();
    if (e.kind === 'symlink') {
      // 远端 symlink 不下钻（元数据不跟随 + 防环），跳过计数
      tree.skippedSymlinks += 1;
      continue;
    }
    if (e.kind === 'directory') {
      const nextRel = [...rel, e.name];
      tree.dirs.push(nextRel.join('/'));
      await walkRemote(batchId, session, e.path, nextRel, depth + 1, tree);
    } else {
      tree.files.push({ entry: e, relDir: rel });
    }
  }
}

// ============================================================
// mkdir 幂等：失败 → stat 复核为 directory 则复用（「存在即复用」，不做
// 整目录替换）；复核为文件或无法确认 → 中止整批（fail-closed）。
// 三态返回：'created'（首次成功 = 之前不存在，目录必为空——调用方可据此
// 跳过其下文件的冲突探测，省 1 次 stat 往返/文件）/ 'reused' / 'failed'。
// ============================================================
async function ensureRemoteDir(session: FilesSessionLike, remoteDir: string): Promise<'created' | 'reused' | 'failed'> {
  try {
    await invokeBackend('sftp_mkdir', { sessionId: session.sessionId, path: remoteDir });
    return 'created';
  } catch {
    // SFTP mkdir 的已存在错误与权限错误 Display 形态不可靠区分，用 stat 复核
    //（契约：Ok(null)=明确不存在，directory=已存在复用；三者都中止 fail-closed）。
    // B2 并发修复：stat 返回 null（明确不存在）说明创建确实失败，不再并入
    // 「已是文件」的误导文案。
    try {
      const st = await invokeBackend<RemoteFileEntry | null>('sftp_stat', { sessionId: session.sessionId, path: remoteDir });
      if (st?.kind === 'directory') return 'reused';
      if (st === null) {
        tc().announce('无法创建远端目录（创建失败且目标不存在）：' + remoteDir, { level: 'error' });
      } else {
        tc().announce('远端路径已是文件，无法创建目录：' + remoteDir, { level: 'error' });
      }
      return 'failed';
    } catch {
      tc().announce('无法创建远端目录（权限不足或无法确认）：' + remoteDir, { level: 'error' });
      return 'failed';
    }
  }
}

async function ensureLocalDir(localDir: string): Promise<boolean> {
  try {
    await invokeBackend('fs_local_mkdir', { path: localDir });
    return true;
  } catch {
    try {
      const st = await invokeBackend<RemoteFileEntry | null>('fs_local_stat', { path: localDir });
      if (st?.kind === 'directory') return true;
      tc().announce('本地路径已是文件，无法创建目录：' + localDir, { level: 'error' });
      return false;
    } catch {
      tc().announce('无法创建本地目录（权限不足或无法确认）：' + localDir, { level: 'error' });
      return false;
    }
  }
}

/** 本地路径最后一段（'C:\\proj\\mydir' / 'D:/x' → 'mydir' / 'x'）。 */
function localBaseName(p: string): string {
  const norm = p.replace(/[\\/]+$/, '');
  const idx = Math.max(norm.lastIndexOf('/'), norm.lastIndexOf('\\'));
  return idx >= 0 ? norm.slice(idx + 1) : norm;
}

/** 摘要后缀：跳过的符号链接数（0 个时不啰嗦）。 */
function symlinkNote(tree: CollectedTree): string {
  return tree.skippedSymlinks ? `，跳过 ${tree.skippedSymlinks} 个符号链接` : '';
}

// ============================================================
// 上传公开入口（自 fileTransfers 迁入；files store re-export 路径不变）
// ============================================================

/** 上传入口①：本机路径批量（原生文件对话框 / OS 拖入）。文件走平铺批次，
 *  目录走目录整传；stat null（明确不存在）逐条报错不拖垮整批。 */
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
  const baseDir = tc().remotePath.value;
  const fileSpecs: UploadSpec[] = [];
  const dirPaths: string[] = [];
  let missingCount = 0;
  let symlinkCount = 0;
  for (const p of list) {
    try {
      const st = await invokeBackend<RemoteFileEntry | null>('fs_local_stat', { path: p });
      if (st?.kind === 'file') fileSpecs.push({ entry: st, targetDir: baseDir });
      else if (st?.kind === 'directory') dirPaths.push(p);
      else if (st?.kind === 'symlink') symlinkCount += 1;
      else missingCount += 1;
    } catch (error) {
      // 单条 stat 失败（权限/系统目录黑名单）不拖垮整批：报错跳过
      tc().announce('无法读取本地路径：' + p + '：' + errorMessage(error), { level: 'error' });
    }
  }
  if (missingCount) tc().announce('已跳过 ' + missingCount + ' 个不存在的路径', { level: 'warn' });
  if (symlinkCount) tc().announce('已跳过 ' + symlinkCount + ' 个符号链接', { level: 'warn' });
  if (fileSpecs.length) await uploadLocalFileEntries(fileSpecs, session);
  for (const dirPath of dirPaths) {
    await uploadDirectory(dirPath);
  }
}

/** 上传入口②：本地面板条目（双击 / 右键 / 栏间拖拽）——文件平铺批，目录整传。 */
export async function uploadLocalEntry(entry: RemoteFileEntry | null) {
  const session = tc().getActiveSession();
  if (!session) {
    tc().announce(tc().noSessionMessage('上传'), { level: 'warn' });
    tc().wb().setTab('terminal');
    return;
  }
  if (!entry || !entry.path) {
    tc().announce('请选择要上传的文件或目录', { level: 'warn' });
    return;
  }
  if (entry.kind === 'symlink') {
    tc().announce('暂不支持上传符号链接：' + entry.name, { level: 'warn' });
    return;
  }
  if (entry.kind === 'directory') {
    await uploadDirectory(entry.path);
    return;
  }
  await uploadLocalFileEntries([{ entry, targetDir: tc().remotePath.value }], session);
}

/**
 * 目录上传：收集本地树 → 远端建目录（幂等）→ 逐文件复用上传批次体（冲突
 * 三选 / 队列 / 进度 / 取消全部继承）。整目录共用一个 batchId——
 * UploadProgressStrip 与 TransferDrawer 按批次聚合。
 */
export async function uploadDirectory(localRoot: string) {
  const session = tc().getActiveSession();
  if (!session) {
    tc().announce(tc().noSessionMessage('上传'), { level: 'warn' });
    tc().wb().setTab('terminal');
    return;
  }
  const rootName = localBaseName(localRoot);
  if (!rootName) {
    tc().announce('无法确定目录名称：' + localRoot, { level: 'warn' });
    return;
  }
  const batchId = beginDirBatch();
  try {
    tc().announce('正在扫描本地目录：' + rootName);
    const tree: CollectedTree = { files: [], dirs: [], skippedSymlinks: 0 };
    await walkLocal(batchId, localRoot, [], 1, tree);
    if (!tc().remotePath.value) {
      tc().announce('远程目录尚未加载，请先刷新后再操作', { level: 'warn' });
      return;
    }
    const remoteRoot = joinPath(tc().remotePath.value, rootName);
    // 目标根 + 各子目录按 DFS 前序创建（父先于子；上传批次体开工前全部就绪）。
    // mkdir 首次成功的目录记入 assumedEmptyDirs——其下文件跳过冲突探测（空目录
    // 不可能同名，省 1 次 stat 往返/文件）；复用已存在目录的不跳过。
    const assumedEmptyDirs = new Set<string>();
    const rootState = await ensureRemoteDir(session, remoteRoot);
    if (rootState === 'failed') return;
    if (rootState === 'created') assumedEmptyDirs.add(remoteRoot);
    for (const rel of tree.dirs) {
      if (isDirBatchCancelled(batchId)) break;
      const dirState = await ensureRemoteDir(session, joinPath(remoteRoot, rel));
      if (dirState === 'failed') return;
      if (dirState === 'created') assumedEmptyDirs.add(joinPath(remoteRoot, rel));
    }
    if (isDirBatchCancelled(batchId)) {
      tc().announce('已取消目录上传：' + rootName, { level: 'warn' });
      return;
    }
    const specs: UploadSpec[] = tree.files.map(f => ({
      entry: f.entry,
      targetDir: f.relDir.length ? joinPath(remoteRoot, f.relDir.join('/')) : remoteRoot
    }));
    const summary = await uploadLocalFileEntries(specs, session, {
      batchId,
      isCancelled: () => isDirBatchCancelled(batchId),
      assumedEmptyDirs
    });
    if (summary.aborted) return;
    const failNote = summary.failed ? `，${summary.failed} 个失败` : '';
    const skippedNote = summary.skipped ? `，跳过 ${summary.skipped} 个同名文件` : '';
    tc().announce(
      `目录上传完成：${summary.uploaded} 个文件 → ${remoteRoot}${failNote}${skippedNote}${symlinkNote(tree)}`,
      { level: summary.failed ? 'warn' : 'success' }
    );
  } catch (error) {
    if (error instanceof DirBatchCancelled) {
      tc().announce('已取消目录上传：' + rootName, { level: 'warn' });
      return;
    }
    tc().announce('目录上传失败：' + rootName + '：' + errorMessage(error), { level: 'error' });
  } finally {
    endDirBatch(batchId);
  }
}

/**
 * 目录下载：选本地目标目录（或沿用调用方给的）→ 收集远端树 → 本地建目录
 * （幂等）→ 逐文件复用 downloadEntry（每次传该文件实际所在的本地区域目录，
 * 冲突三选逐文件生效）。取消：收集/建目录阶段旗标即停；传输阶段逐文件检查。
 */
export async function downloadDirectory(entry: RemoteFileEntry, destDir?: string | null) {
  const session = tc().getActiveSession();
  if (!session) {
    tc().announce(tc().noSessionMessage('下载'), { level: 'warn' });
    return;
  }
  if (!entry?.path || entry.kind === 'symlink') {
    tc().announce('请选择要下载的目录', { level: 'warn' });
    return;
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
      tc().announce('已取消目录下载：' + entry.name, { level: 'warn' });
      return;
    }
  }
  const batchId = beginDirBatch();
  try {
    tc().announce('正在扫描远端目录：' + entry.name);
    const tree: CollectedTree = { files: [], dirs: [], skippedSymlinks: 0 };
    await walkRemote(batchId, session, entry.path, [], 1, tree);
    const localRoot = joinLocalPath(dir, entry.name);
    if (!(await ensureLocalDir(localRoot))) return;
    // 逐段 joinLocalPath（分隔符随基座自适应），不预拼 'a/b' 字符串
    const localDirFor = (rel: string[]): string => rel.reduce((acc, seg) => joinLocalPath(acc, seg), localRoot);
    for (const rel of tree.dirs) {
      if (isDirBatchCancelled(batchId)) break;
      if (!(await ensureLocalDir(localDirFor(rel.split('/'))))) return;
    }
    if (isDirBatchCancelled(batchId)) {
      tc().announce('已取消目录下载：' + entry.name, { level: 'warn' });
      return;
    }
    let done = 0;
    let failed = 0;
    let skipped = 0;
    const concurrency = getTransferConcurrency();
    await runWithConcurrencyPool(
      tree.files,
      concurrency,
      async f => {
        if (isDirBatchCancelled(batchId)) return;
        // downloadEntry 返回队列项（失败已在 runDownload 内 announce + 标记
        // error 态，不靠抛错传递）；null=未入队（用户跳过/取消选择）。
        // cancelled 终态由外层取消分支汇总，不计入任何一栏。
        const item = await downloadEntry(f.entry, localDirFor(f.relDir));
        if (!item) skipped += 1;
        else if (item.status === 'done') done += 1;
        else if (item.status === 'error') failed += 1;
      },
      () => isDirBatchCancelled(batchId)
    );
    if (isDirBatchCancelled(batchId)) {
      const remaining = tree.files.length - done - failed - skipped;
      tc().announce(
        `已取消目录下载：${entry.name}（已下载 ${done} 个，剩余 ${remaining} 个未下载）`,
        { level: 'warn' }
      );
      return;
    }
    const failNote = failed ? `，${failed} 个失败` : '';
    const skipNote = skipped ? `，跳过 ${skipped} 个同名文件` : '';
    tc().announce(
      `目录下载完成：${done} 个文件 → ${localRoot}${failNote}${skipNote}${symlinkNote(tree)}`,
      { level: failed ? 'warn' : 'success' }
    );
  } catch (error) {
    if (error instanceof DirBatchCancelled) {
      tc().announce('已取消目录下载：' + entry.name, { level: 'warn' });
      return;
    }
    tc().announce('目录下载失败：' + entry.name + '：' + errorMessage(error), { level: 'error' });
  } finally {
    endDirBatch(batchId);
  }
}
