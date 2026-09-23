/**
 * filePanel — 文件面板浏览域：远程/本地目录加载与导航、会话解析、
 * mkdir/rename/删除确认链、本地目录操作（从 stores/files.ts 按域拆出，
 * v2.8 第五轮）。绑定式 context（先例 lib/terminalLifecycle）。
 * 上传/下载/覆盖确认/传输队列在 lib/fileTransfers。
 */
import { ref, type Ref } from 'vue';
import type {
  LocalDirectoryListResult,
  NotifyOptions,
  NormalizedConnectionAsset,
  RemoteDirectoryListResult,
  RemoteFileEntry
} from '@/types/domain';
import { invokeBackend, isTauriRuntime } from '@/services/backend';
import { joinLocalPath, joinPath, parentLocalPath, parentPath, remotePathForAsset } from '@/stores/workbench';
import { errorMessage } from '@/lib/errorMessage';
import { bindFilePanelLocal, refreshLocalFiles, navigateLocalPath } from '@/lib/filePanelLocal';
import { bindFileListView, ls, bindRemoteListState } from '@/lib/fileListView';
import { bindFileDeleteDeps } from '@/lib/fileDelete';

// v0.20（S2 刀）：本地域与删除链已按域拆出，调用方路径不变（经下方 re-export）。
export {
  navigateLocalUp, localMkdir, localRename, setLocalViewMode
} from './filePanelLocal';
// refreshLocalFiles / navigateLocalPath 已值 import（bind 与 goToManualLocalPath 消费），re-export 由下方统一转发
export { refreshLocalFiles, navigateLocalPath };
export { removeRemote, localDelete, batchRemoteDelete, confirmFileDelete, cancelFileDelete } from './fileDelete';
export {
  computeFiltered, sortEntries,
  toggleRemoteSelection, selectAllRemote, clearRemoteSelection,
  toggleLocalSelection, selectAllLocal, clearLocalSelection,
  setRemoteSort, setRemoteFilter, setRemoteListMode, setLocalListMode
} from './fileListView';
export { bindRemoteListState };
export type { RemoteListViews, RemoteListState } from './fileListView';
import type {
  FileOperationEntry,
  FilesSessionLike,
  FilesWorkbenchBridge,
  PendingFileDelete,
  QueueItem
} from '@/lib/fileTypes';

export interface FilePanelContext {
  wb(): FilesWorkbenchBridge;
  announce(message: string, opts?: NotifyOptions): void;
  remotePath: Ref<string>;
  remoteEntries: Ref<RemoteFileEntry[]>;
  remoteLoaded: Ref<boolean>;
  remoteLoadedAssetId: Ref<string | null>;
  remoteError: Ref<string>;
  localPath: Ref<string>;
  localEntries: Ref<RemoteFileEntry[]>;
  localViewMode: Ref<string>;
  manualRemotePathInput: Ref<string>;
  manualLocalPathInput: Ref<string>;
  selectedRemotePaths: Ref<Set<string>>;
  pendingFileDelete: Ref<PendingFileDelete | null>;
  fileOperationStack: Ref<{ remote: FileOperationEntry[]; local: FileOperationEntry[] }>;
  requireRemotePath(): boolean;
  withFileOperation<T>(side: 'remote' | 'local', message: string, task: () => Promise<T>): Promise<T>;
  /** 下载/删除后的列表刷新回调（面板自身实现，供 shell 再注入） */
  refreshLocalFilesCb(): void;
}

let bound: FilePanelContext | null = null;

export function bindFilePanelContext(ctx: FilePanelContext): void {
  bound = ctx;
  // v0.20（S2 刀）：本地域与删除链已拆出（filePanelLocal / fileDelete），级联注入
  // 同一 ctx 与跨域依赖（函数引用惰性调用，bind 时序无关）。
  bindFilePanelLocal(ctx);
  bindFileListView(pc);
  bindFileDeleteDeps({
    pc,
    getActiveSession,
    noSessionMessage,
    refreshRemoteFiles,
    refreshLocalFiles,
    ls
  });
}

function pc(): FilePanelContext {
  if (!bound) throw new Error('filePanel 未绑定 context（files store 未初始化）');
  return bound;
}

// ============================================================
// Sessions lazy 解析（避免循环 import）
// ============================================================
// 解析指定资产当前应使用的会话：优先活跃会话（若同资产），否则回退首个匹配会话。
// 同资产多会话场景下，文件浏览跟随当前聚焦的 tab。
export function resolveSessionForAsset(asset: NormalizedConnectionAsset | null): FilesSessionLike | null {
  // null（retry 找不回原资产）→ 无可用会话：不猜当前面板/活跃会话（错资产比报错危险）
  if (!asset) return null;
  const sessionsStore = pc().wb().sessionsStore();
  const active = sessionsStore?.activeSession;
  if (active && active.asset?.id === asset.id) return active;
  return sessionsStore?.sessions.find(item => item.asset.id === asset.id) || null;
}
/**
 * 当前文件面板应使用的会话 = **selectedAsset 所属**的会话。
 *
 * 为什么不能无条件返回 activeSession：activeSession 是「终端聚焦的 tab」，
 * 与文件面板选中的资产可以不是同一台机器。曾经的实现直接 `if (active) return
 * active`，于是「已连接 A、在侧栏选中未连接的 B、切到文件 tab」时，面板若走
 * 一次性连接回落分支显示的是 B 的家目录，而所有写操作却拿到 **A 的会话**：
 * 上传按 B 的路径静默落到 A、下载从 A 读到错误内容却按 B 的文件名落盘。
 * 解析器与 refreshRemoteFiles 保持同一口径，才能保证「看到哪台就操作哪台」。
 *
 * 返回 null 表示「当前资产没有可用会话」，调用方必须据此拒绝操作并提示，
 * **不得**回落到别的资产。
 */
export function getActiveSession(): FilesSessionLike | null {
  const asset = pc().wb().selectedAsset;
  if (!asset) return null;
  return resolveSessionForAsset(asset);
}

/**
 * 远程操作缺少会话时的统一提示。
 *
 * 必须点名资产：getActiveSession 现在只认 selectedAsset 的会话，所以「没有
 * 会话」指的是**当前选中的这台**没连，而不是「这个应用没连任何东西」。旧文案
 * 「需要先建立 SSH 会话」在「A 已连接、选中 B」的场景下会让用户以为已经连上了，
 * 从而误判故障原因。
 */
export function noSessionMessage(action: string): string {
  const name = pc().wb().selectedAsset?.name;
  return name ? `${action}失败：当前资产「${name}」尚未建立 SSH 会话` : `${action}需要先建立 SSH 会话`;
}

// ============================================================
// Remote SFTP 浏览 / 操作
// ============================================================
async function doRefreshRemoteFiles(path: string | null = null, { silent = false } = {}) {
  try {
    await pc().withFileOperation('remote', '正在读取远程目录...', async () => {
      const asset = pc().wb().selectedAsset;
      if (!asset) {
        // v0.20 修复（2026-09-23）：无选中资产时静默 return 会表现为「点刷新毫无
        // 反应」（连 loading 都立即结束）——明确提示用户先选资产，不再静默吞掉。
        if (!silent) {
          pc().announce('请先在左侧选择要浏览的资产，再刷新远程目录', { level: 'warn' });
        }
        return;
      }
      // path=null 的语义 = 「刷新当前面板目录」（与 refreshLocalFiles 的本地侧一致）。
      // 只允许面板未加载（remotePath 为空）时回落到服务器默认目录——空 path 会让
      // 后端 canonicalize(".") 解析出登录家目录；若面板已加载仍传空，上传批次收尾/
      // 刷新按钮/编辑器保存后刷新都会把用户从深层目录拽回「第一次进服务器的路径」
      // （2026-09-21 用户视频实测：/var 上传失败、下载目录上传成功，面板两次跳回家目录）。
      const targetPath = path || pc().remotePath.value || remotePathForAsset(asset);
      const activeSession = resolveSessionForAsset(asset);
      if (activeSession) {
        // 会话在途（首连/重连中，sessionId 仍是 pending- 占位）：后端 ssh_handles
        // 尚无此 id，sftp_list_dir 必报 "SSH handle not found"（误导性噪音）。
        // 不发必败请求、也不回落独立连接（同资产第二条连接纯属浪费），等
        // handleSessionConnected 的自动加载链在连接完成后刷新。
        if (String(activeSession.sessionId).startsWith('pending-')) {
          if (!silent) {
            pc().announce('会话正在连接：' + asset.name + '，连接完成后自动加载目录', { level: 'info' });
          }
          return;
        }
        const result = await invokeBackend<RemoteDirectoryListResult>('sftp_list_dir', { sessionId: activeSession.sessionId, path: targetPath });
        // result.path 是服务器解析后的绝对路径（空入参时 = canonicalize 的真实家目录）
        // 迟到守卫：请求在途期间 selectedAsset 可能已切换（连续连接/切换资产的
        // 高频操作），过期结果写面板会把新资产错标成旧目录。
        if (pc().wb().selectedAsset?.id !== asset.id) return;
        applyRemoteListing(result.path || targetPath, result.entries || []);
        return;
      }
      // 无活跃会话 → 回落一次性 SSH 连接（ssh_list_directory）。
      // 该连接不在 sessions 里（用户不该看到一个没有终端的会话条目），但它走
      // ssh.rs 同一个交互式 handler，未知/变更主机密钥同样会 emit
      // ssh-host-key-verify 并等 60s。因此在途期间必须在 sessions store 登记，
      // 否则跨窗口路由守卫认不出事件归属，确认框永不出现。
      // 参数名必须是 camelCase：Tauri 2 命令宏把 Rust snake_case 参数名转
      // lowerCamelCase 后才到前端 payload 取值（多词 snake_case 键会静默失配
      // 成 None——曾致 credential_id/auth_method 全部丢失，回落连接恒报
      // 「No password provided and no stored credential」）。与 ssh_connect
      // 调用（sessions.ts attachSessionStream）保持同形。
      const unregister = pc().wb().sessionsStore()?.registerEphemeralConnection?.(asset);
      let result: RemoteDirectoryListResult;
      try {
        result = await invokeBackend<RemoteDirectoryListResult>('ssh_list_directory', {
          host: asset.host,
          port: asset.port,
          username: asset.username,
          password: '',
          credentialId: asset.credential_id || null,
          authMethod: asset.auth_method,
          privateKeyPath: asset.private_key_path,
          passphrase: null,
          passphraseCredentialId: asset.passphrase_credential_id || null,
          privateKeyCredentialId: asset.private_key_credential_id || null, jumpHost: asset.jump_host || null, connectTimeoutSecs: asset.connect_timeout_secs ?? null, keepaliveIntervalSecs: asset.keepalive_interval_secs ?? null,
          path: targetPath
        });
      } finally {
        // 一次性连接结束（成功/失败/超时）即注销：不能留下幽灵会话，否则之后
        // 别的路径产生的主机密钥确认会被本窗口误认领。
        unregister?.();
      }
      // 迟到守卫：同上，过期结果不写面板（见 sftp_list_dir 分支注释）。
      if (pc().wb().selectedAsset?.id !== asset.id) return;
      applyRemoteListing(result.path || targetPath, Array.isArray(result.entries) ? result.entries : []);
      pc().announce('远程文件已刷新：' + asset.name);
    });
    pc().remoteError.value = '';
  } catch (error) {
    // 失败：写入 remoteError（列表空态显示「加载失败 + 重试」），announce 升级 error；
    // silent 模式（自动加载/OSC 7 跟随）仅写空态，不弹错误打扰。
    // 取值统一走 errorMessage（后端 Err(String) 以字符串 reject 的原因见该文件注释）
    const message = errorMessage(error);
    pc().remoteError.value = message;
    if (!silent) {
      pc().announce('远程目录加载失败：' + message, { level: 'error' });
    }
  }
}

/**
 * 同一 (资产, 目标路径) 的刷新请求合一表。
 *
 * 为什么需要：连接成功后会有三路几乎同时的刷新——`workbench.ts` 的会话状态
 * watcher、`handleSessionConnected` 的自动加载、以及切到文件 tab 时
 * `ui.ts:setTab` 的刷新。三者目标一致却各自发起 `sftp_list_dir`，等于对同一
 * 目录并发打三次请求（多窗口下再乘窗口数）。合一后并发只发一次，后来者复用
 * 同一个 Promise。
 *
 * 键含资产 id，因此「切到另一资产」不会被旧请求吞掉；条目在 settled 后一律
 * 清理（含失败路径），避免失败被永久缓存。
 */
const remoteRefreshInFlight = new Map<string, Promise<void>>();

/**
 * 刷新远程目录（对外入口）= 在途合一 + 真实刷新。
 *
 * 复用在途请求时，本次调用的 silent 诉求由发起方自己保证（发起方本就不弹成功
 * 提示）；错误提示只由真正发起那次请求的 silent 决定——这符合预期：若有人要求
 * 非静默刷新，说明用户确实在等这个目录，报错应该可见。
 */
export function refreshRemoteFiles(path: string | null = null, { silent = false } = {}): Promise<void> {
  const key = (pc().wb().selectedAsset?.id ?? '') + '\u0000' + (path || '');
  const existing = remoteRefreshInFlight.get(key);
  if (existing) return existing;
  const promise = doRefreshRemoteFiles(path, { silent }).finally(() => {
    // 仅在仍是自己时删除：若期间有新一轮刷新覆盖了该键，不能误删新条目
    if (remoteRefreshInFlight.get(key) === promise) remoteRefreshInFlight.delete(key);
  });
  remoteRefreshInFlight.set(key, promise);
  return promise;
}

export function applyRemoteListing(path: string, entries: RemoteFileEntry[]) {
  pc().remotePath.value = path;
  pc().remoteEntries.value = entries;
  pc().remoteLoaded.value = true;
  // 列表归属当前面板资产（sftp_list_dir / ssh_list_directory 两条路都按
  // selectedAsset 发起，此处即其归属）
  pc().remoteLoadedAssetId.value = pc().wb().selectedAsset?.id ?? null;
}

/** 重置远程面板到未加载空态（切换资产 / 会话断开时调用）。 */
export function resetRemotePanel() {
  if (autoLoadRetryToken) autoLoadRetryToken.cancelled = true;
  pc().remoteEntries.value = [];
  pc().remoteLoaded.value = false;
  pc().remoteLoadedAssetId.value = null;
  pc().remoteError.value = '';
  pc().remotePath.value = '';
  ls().selectedRemote.value = new Set();
}

// ------------------------------------------------------------
// 与终端会话生命周期联动（sessions store 经 workbench bridge 调用）：
//   断开/关闭 → 清空面板（不再停留旧资产的目录）；
//   连接成功 → 自动加载远程目录（无需手动刷新）；
//   终端 cd（OSC 7）→ 跟随切换远程目录。
// 均只对「文件面板当前绑定的资产」（selectedAsset）生效。
// ------------------------------------------------------------
// 自动加载退避重试的取消令牌（会话断开/新一轮自动加载时置 cancelled）。
let autoLoadRetryToken: { cancelled: boolean } | null = null;

export function handleSessionClosed(assetId?: string | null) {
  // 取消在途自动加载退避链：不取消的话，断开 N 秒后的重试回调会把已清空的面板
  // 重新填上目录（且回落独立连接分支）。
  const current = pc().wb().selectedAsset;
  if (!current || current.id !== assetId) return;
  resetRemotePanel();
}

export async function handleSessionConnected(assetId?: string | null) {
  const current = pc().wb().selectedAsset;
  if (!current || current.id !== assetId) return;
  // 已加载【同一资产】的列表（如重连成功）不打扰；面板还挂着别的资产（或空态）
  // 时为新资产加载——旧实现只看全局 remoteLoaded，连接第二个资产时因残留
  // true 直接跳过，面板继续显示上一资产的目录。
  if (pc().remoteLoaded.value && pc().remoteLoadedAssetId.value === assetId) return;
  // ssh_connect 返回时 SFTP 通道已可用（workbench status watcher 已无延迟刷新）：
  // 立即加载，仅当失败（瞬时 SFTP 未就绪）才按 1s/2s/5s 退避重试，上限 3 次。
  // 重试前必须确认该资产仍有活跃会话——禁止回落到 refreshRemoteFiles 内的
  // ssh_list_directory 独立连接分支静默新开一条 SSH 连接。失败静默（空态已有
  // 「加载失败 + 重试」，不弹错误打扰）。
  if (autoLoadRetryToken) autoLoadRetryToken.cancelled = true;
  const token = { cancelled: false };
  autoLoadRetryToken = token;
  await refreshRemoteFiles(null, { silent: true });
  for (const delay of [1000, 2000, 5000]) {
    if (token.cancelled || pc().remoteLoaded.value) return;
    const asset = pc().wb().selectedAsset;
    if (!asset || asset.id !== assetId || !resolveSessionForAsset(asset)) return;
    await new Promise(resolve => setTimeout(resolve, delay)); // fact-guard:allow no-fixed-wait-frontend 退避重试的等待间隔：前后各行都先校验会话存活与加载状态，等待只是节流不是就绪信号
    if (token.cancelled || pc().remoteLoaded.value) return;
    const after = pc().wb().selectedAsset;
    if (!after || after.id !== assetId || !resolveSessionForAsset(after)) return;
    await refreshRemoteFiles(null, { silent: true });
  }
}

/**
 * selectedAsset 切换时由 assets.selectAsset 经 workbench bridge 通知：
 * 面板归属不同资产 → 重置残留（上一资产的目录/路径/选中态），若新资产已有
 * 会话则静默自动加载（与「连接成功自动加载」体验一致）。
 */
export function handleAssetSelected(assetId?: string | null) {
  if (!assetId || assetId === pc().remoteLoadedAssetId.value) return;
  resetRemotePanel();
  const asset = pc().wb().selectedAsset;
  if (!asset || asset.id !== assetId) return;
  if (!resolveSessionForAsset(asset)) return;
  refreshRemoteFiles(null, { silent: true }).catch(() => null);
}

let lastCwdSyncAt = 0;
export async function syncTerminalCwd(assetId?: string | null, path?: string) {
  const current = pc().wb().selectedAsset;
  if (!current || current.id !== assetId) return;
  if (!path || path === pc().remotePath.value) return;
  // 提示符每刷一次就发一次 OSC 7，简单节流防连续 cd 时并发刷新
  const now = Date.now();
  if (now - lastCwdSyncAt < 400) return;
  lastCwdSyncAt = now;
  // cd 跟随是尽力而为：失败走 silent（仅空态展示 + 重试），不弹错误 toast——
  // 公网服务器认证惩罚期/瞬时 SFTP 失败会立刻刷屏式打扰
  await refreshRemoteFiles(path, { silent: true }).catch(() => null);
}

export async function navigateRemotePath(target: string) {
  if (!target) return;
  await refreshRemoteFiles(target).catch(error => pc().announce('进入目录失败：' + errorMessage(error)));
}

export async function navigateRemoteUp() {
  const parent = parentPath(pc().remotePath.value);
  await refreshRemoteFiles(parent).catch(error => pc().announce('返回上级失败：' + errorMessage(error)));
}

// ------------------------------------------------------------
// 上传/下载传输（S2：覆盖保护 + 取消/重试 + 速度 ETA + busy 解耦）
// ------------------------------------------------------------

/** sftp_stat 路径版：上传覆盖检查复用（statRemote 的「无消费」状态由此消除）。 */
export async function statRemotePath(path: string) {
  const session = getActiveSession();
  if (!session) return null;
  return invokeBackend<Record<string, unknown> | null>('sftp_stat', { sessionId: session.sessionId, path });
}

export async function statRemote(entry?: RemoteFileEntry | null) {
  if (!entry?.path) return null;
  return statRemotePath(entry.path);
}

export async function mkdirRemote(name: string) {
  const session = getActiveSession();
  if (!session) {
    pc().announce(noSessionMessage('新建目录'), { level: 'warn' });
    return;
  }
  if (!name?.trim()) {
    pc().announce('目录名不能为空', { level: 'warn' });
    return;
  }
  if (!pc().requireRemotePath()) return;
  try {
    await pc().withFileOperation('remote', '正在创建远程目录...', async () => {
      const target = joinPath(pc().remotePath.value, name.trim());
      await invokeBackend('sftp_mkdir', { sessionId: session.sessionId, path: target });
      await refreshRemoteFiles(pc().remotePath.value);
    });
    pc().announce('已创建目录：' + name.trim(), { level: 'success' });
  } catch (error) {
    pc().announce('创建目录失败：' + errorMessage(error), { level: 'error' });
    throw error; // 抛给 GlobalModals submitModal：失败保持弹窗打开，可修正重试
  }
}

export async function renameRemote(entry: RemoteFileEntry, newName: string) {
  const session = getActiveSession();
  if (!session) {
    pc().announce(noSessionMessage('重命名'), { level: 'warn' });
    return;
  }
  if (!newName?.trim()) {
    pc().announce('新名称不能为空', { level: 'warn' });
    return;
  }
  try {
    await pc().withFileOperation('remote', '正在重命名远程文件...', async () => {
      const target = joinPath(parentPath(entry.path), newName.trim());
      await invokeBackend('sftp_rename', { sessionId: session.sessionId, oldPath: entry.path, newPath: target });
      await refreshRemoteFiles(pc().remotePath.value);
    });
    pc().announce('已重命名：' + entry.name + ' → ' + newName.trim(), { level: 'success' });
  } catch (error) {
    pc().announce('重命名失败：' + errorMessage(error), { level: 'error' });
    throw error;
  }
}

// ============================================================
// 列表视图域（v0.20 拆至 lib/fileListView.ts，S2 刀：排序/过滤视图 + 多选 + 视图模式；
// ls 经值 import 共享，pc 经 bind 注入避免 import 环）
// ============================================================
  export function setManualRemotePath(value: string) {
    pc().manualRemotePathInput.value = value;
  }

  export async function goToManualRemotePath() {
    const target = pc().manualRemotePathInput.value.trim();
    if (!target) return;
    await navigateRemotePath(target);
  }

  export function setManualLocalPath(value: string) {
    pc().manualLocalPathInput.value = value;
  }

  export async function goToManualLocalPath() {
    const target = pc().manualLocalPathInput.value.trim();
    if (!target) return;
    await navigateLocalPath(target);
  }
