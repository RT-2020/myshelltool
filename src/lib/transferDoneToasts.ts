/**
 * transferDoneToasts — 传输完成通知聚合（v0.21/B2 后续：自 fileTransfers 拆出
 * 并推广到上传侧）。
 *
 * 机制（下载侧 v0.18 先例）：同一目录 1.5s 窗口内的完成合并为一条 toast——
 * 逐文件弹会把 toast 上限 5 条顶掉、连发刷屏遮挡视野。setTimeout 是纯 UI
 * 去抖（只 flush，不做「赌 N ms 后状态就绪」的探测等待）。时长 10s：比带
 * 按钮默认 8s 再宽一点，给足移动鼠标点击的窗口。
 *
 * 上传侧（B2 目录整传暴露）：同一远端目标目录归组——单文件保持「已上传：
 * name」原文案；多文件合并为「已上传 N 个文件 → /目录」+「传输队列」入口。
 *
 * ctx 与 fileTransfers 共享 FileTransfersContext（bindFileTransfersContext
 * 级联注入）；调用方经 fileTransfers 的 re-export 取函数。
 */
import { formatBytes } from '@/lib/transferUtils';
import { openLocalPath, revealLocalItem } from '@/lib/openLocalFile';
import { errorMessage } from '@/lib/errorMessage';
import type { FileTransfersContext } from '@/lib/fileTransfers';

const DONE_WINDOW_MS = 1500;
const TOAST_DURATION_MS = 10_000;

let ctx: FileTransfersContext | null = null;

/** fileTransfers.bindFileTransfersContext 级联注入。 */
export function bindTransferDoneToasts(context: FileTransfersContext): void {
  ctx = context;
}

function tc(): FileTransfersContext {
  if (!ctx) throw new Error('transferDoneToasts: context not bound');
  return ctx;
}

/**
 * 批内完成聚合器（上传/下载共用的归组 + 去抖机制）：按 key 归组，窗口期内
 * 追加即重置计时，静默 1.5s 后一次性 flush（flush 逻辑由各方向提供——
 * 下载带本地打开按钮、上传带传输队列入口，文案与动作不同机制相同）。
 */
function createDoneAggregator<T>(flush: (key: string, items: T[]) => void) {
  const buffer = new Map<string, T[]>();
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  return (key: string, item: T) => {
    const list = buffer.get(key) ?? [];
    list.push(item);
    buffer.set(key, list);
    const existing = timers.get(key);
    if (existing) clearTimeout(existing);
    timers.set(
      key,
      setTimeout(() => {
        timers.delete(key);
        const items = buffer.get(key) ?? [];
        buffer.delete(key);
        if (items.length) flush(key, items);
      }, DONE_WINDOW_MS)
    );
  };
}

// ============================================================
// 下载完成通知（自 fileTransfers 原样迁移，行为零变化）
// ============================================================
interface DownloadDoneRecord {
  name: string;
  localPath: string;
  size: number;
}

const queueDownloadDone = createDoneAggregator<DownloadDoneRecord>((destDir, records) => {
  if (records.length === 1) {
    const rec = records[0]!;
    const sizeLabel = rec.size > 0 ? `（${formatBytes(rec.size)}）` : '';
    tc().announce(`已下载：${rec.name}${sizeLabel} → ${destDir}`, {
      level: 'success',
      duration: TOAST_DURATION_MS,
      actions: [
        { label: '打开', run: () => void runLocalOpenAction('file', rec.localPath) },
        { label: '所在文件夹', run: () => void runLocalOpenAction('reveal', rec.localPath) }
      ]
    });
    return;
  }
  tc().announce(`已下载 ${records.length} 个文件 → ${destDir}`, {
    level: 'success',
    duration: TOAST_DURATION_MS,
    actions: [{ label: '打开文件夹', run: () => void runLocalOpenAction('folder', destDir) }]
  });
});

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

export function queueDownloadDoneToast(destDir: string, record: DownloadDoneRecord) {
  queueDownloadDone(destDir, record);
}

// ============================================================
// 上传完成通知（B2 后续新增：目录整传 N 文件 = N 条 toast 刷屏的治理）
// 单文件保持原「已上传：name」文案与默认时长；多文件合并并给「传输队列」入口。
// ============================================================
interface UploadDoneRecord {
  name: string;
  size: number;
}

const queueUploadDone = createDoneAggregator<UploadDoneRecord>((remoteDir, records) => {
  if (records.length === 1) {
    const rec = records[0]!;
    tc().announce('已上传：' + rec.name, { level: 'success' });
    return;
  }
  tc().announce(`已上传 ${records.length} 个文件 → ${remoteDir}`, {
    level: 'success',
    duration: TOAST_DURATION_MS,
    actions: [{ label: '传输队列', run: () => { tc().transferDrawerOpen.value = true; } }]
  });
});

export function queueUploadDoneToast(remoteDir: string, record: UploadDoneRecord) {
  queueUploadDone(remoteDir, record);
}
