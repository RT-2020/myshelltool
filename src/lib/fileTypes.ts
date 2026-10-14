/**
 * fileTypes — files store 与其拆出的面板/传输 lib 的共享类型
 * （从 stores/files.ts 抽出，v2.8 第五轮）。
 */
import type {
  ModalState,
  NormalizedConnectionAsset,
  NotifyOptions,
  RemoteFileEntry
} from '@/types/domain';
import type { TransferQueueItem } from '@/lib/transferUtils';

/** 传输队列项：transferUtils 的 TransferQueueItem + 进度节流的私有时间戳（不入 utils 类型）。 */
export type QueueItem = TransferQueueItem & { _lastProgressAt?: number };

/** 文件操作遮罩栈条目（remote/local 各一个栈）。 */
export interface FileOperationEntry {
token: symbol;
message: string;
}

/** 删除确认链暂存（confirmFileDelete 执行真正删除）。 */
export interface PendingFileDelete {
kind: 'remote' | 'local';
paths: string[];
names: string[];
/**
 * 打开确认弹窗那一刻的面板资产 id（本地删除为 null）。
 *
 * 为什么必须在**打开时**记录：确认函数只在用户点「删除」时才解析会话。如果在
 * 弹窗打开期间切了资产，`remoteEntries` 会换成另一台机器的目录；若两台恰好存在
 * 同名绝对路径（/etc/nginx/nginx.conf 这类），删除就会落到**另一台服务器**上。
 * 有了这个快照，确认时比对不上就拒绝执行。
 */
assetId: string | null;
}

/** 上传/下载覆盖确认的用户决策（B2 三选）：重命名携带新名（只改目标路径，
 *  不走 rename 命令）。skip=跳过该文件不中断整批。 */
export type OverwriteDecision =
  | { kind: 'overwrite' }
  | { kind: 'skip' }
  | { kind: 'rename'; name: string };

/** 覆盖确认暂存（confirmFileOverwrite/cancelFileOverwrite/renameFileOverwrite
 *  调用 resolve 结算；B2 起上传与下载共用同一队列）。
 *  单一权威点在此——fileTransfers/fileOverwrite 均从这里 import。 */
export interface PendingOverwrite {
  entry: RemoteFileEntry | null;
  /** 冲突目标全路径（上传=远端，下载=本地）。 */
  target: string;
  /** 目标所在目录（重命名探测新名时拼路径用）。 */
  dir: string;
  /** 冲突方向：决定弹窗文案与重命名探测走远端还是本地。 */
  direction: 'upload' | 'download';
  /** 所属传输批次（目录整传/多文件批）；勾选「应用到剩余」时按它记批次默认。 */
  batchId?: string;
  resolve: (choice: OverwriteDecision) => void;
  /**
   * 已被 confirm/cancel/rename/中止清理 settle 过。
   *
   * 为什么需要它：一个上传批次中止时要把「自己那一条」确认收掉，但它并不知道
   * 自己那条此刻是不是队首（用户可能正在回答别批次弹出的确认）。有了这个标记，
   * 调用方就能用**自己的 resolve 引用**安全地只 settle 本方那条 —— 命中已 settle
   * 的项直接跳过，绝不会把别人的确认误判成取消。
   */
  settled: boolean;
}

/** 右键菜单状态。 */
export interface ContextMenuState {
visible: boolean;
x: number;
y: number;
side: string;
entry: RemoteFileEntry | null;
}

/** files store 消费的 sessions store 最小结构（lazy 解析，避免循环 import）。 */
export interface FilesSessionLike {
sessionId: string;
asset: { id: string };
}

export interface FilesSessionsStoreLike {
activeSession: FilesSessionLike | null;
sessions: FilesSessionLike[];
connectSelected(): Promise<unknown>;
}

/** files store 实际消费的 workbench bridge 最小结构。 */
export interface FilesWorkbenchBridge {
announce(message: string, opts?: NotifyOptions): unknown;
selectedAsset: NormalizedConnectionAsset | null;
setTab(tab: string): unknown;
modal: ModalState;
sessionsStore(): FilesSessionsStoreLike | null;
/**
 * 全部资产列表（含未连接的）。传输重试要按队列项记录的 assetId 找回**原资产**
 * ——用户失败后可能已切走，只靠 selectedAsset 找不到它，因此必须能按 id 查全表。
 */
assets(): NormalizedConnectionAsset[];
}


/**
 * 栏间拖拽的内部 MIME（v0.18 起为单一事实源，从 FileSurface/FileColumnList
 * 的两份同值常量上移）：dragstart 载荷 = JSON.stringify({ entries: RemoteFileEntry[] })。
 * EditorSurface 也以它作为「拖文件进编辑器打开」的判定。
 */
export const FILE_DRAG_MIME = 'application/x-myshelltool-file';
