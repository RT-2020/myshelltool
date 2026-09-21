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

/** 上传覆盖确认暂存（resolve 由 confirmFileOverwrite/cancelFileOverwrite 调用）。 */
export interface PendingFileOverwrite {
entry: RemoteFileEntry | null;
remoteTarget: string;
resolve: (choice: boolean) => void;
/**
 * 已被 confirm/cancel/中止清理 settle 过。
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
/**
 * 登记一次性的「非会话连接」（ssh_list_directory 回落分支），返回注销函数。
 * 该连接走 ssh.rs 的同一交互式 handler，未知主机密钥会 emit
 * ssh-host-key-verify；不登记的话跨窗口路由守卫认不出它属于本窗口，确认
 * 请求被丢弃、后端空等 60s。可选：旧注入方缺该方法时降级为「不登记」。
 */
registerEphemeralConnection?(asset: NormalizedConnectionAsset): () => void;
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

