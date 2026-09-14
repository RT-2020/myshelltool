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
      if (!asset) return;
      const targetPath = path || remotePathForAsset(asset);
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
          privateKeyCredentialId: asset.private_key_credential_id || null,
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

// ------------------------------------------------------------
// 删除确认链（统一走 GlobalModals，禁止 window.confirm）
// removeRemote / localDelete / batchRemoteDelete 只组装 pendingFileDelete
// 并弹 confirmFileDelete modal；confirmFileDelete 执行真正删除。
// ------------------------------------------------------------
function openFileDeleteConfirm(kind: 'remote' | 'local', paths: string[], names: string[]) {
  pc().pendingFileDelete.value = {
    kind,
    paths,
    names,
    // 仅远程删除需要归属：本地删除与资产无关
    assetId: kind === 'remote' ? (pc().wb().selectedAsset?.id ?? null) : null
  };
  pc().wb().modal = { type: 'confirmFileDelete' };
}

export function removeRemote(entry: RemoteFileEntry) {
  const session = getActiveSession();
  if (!session) {
    pc().announce(noSessionMessage('删除'), { level: 'warn' });
    return;
  }
  openFileDeleteConfirm('remote', [entry.path], [entry.name]);
}

export function localDelete(paths?: string[] | null) {
  if (!paths?.length) return;
  const names = paths.map(p => pc().localEntries.value.find(e => e.path === p)?.name || p);
  openFileDeleteConfirm('local', [...paths], names);
}

export function batchRemoteDelete() {
  const paths = Array.from(ls().selectedRemote.value);
  if (!paths.length) return;
  const session = getActiveSession();
  if (!session) {
    pc().announce(noSessionMessage('删除'), { level: 'warn' });
    return;
  }
  const names = paths.map(p => pc().remoteEntries.value.find(e => e.path === p)?.name || p);
  openFileDeleteConfirm('remote', paths, names);
}

/**
 * 删除结果播报：数字一律用【实际成功删除数】。弹窗期间面板刷新/切换资产后
 * remoteEntries/localEntries 里可能已找不到这些 path（实际删 0 个），照用户
 * 选中数播报会让用户误以为已删掉——有跳过项必须降级 warn 并说明原因。
 */
function announceDeleteResult(label: string, removed: number, skipped: number) {
  if (skipped > 0) {
    pc().announce(
      removed > 0
        ? '已删除' + label + ' ' + removed + ' 项，跳过 ' + skipped + ' 项（已不在当前列表中）'
        : '未删除任何项：' + skipped + ' 项均已不在当前列表中，请刷新后重试',
      { level: 'warn' }
    );
    return;
  }
  pc().announce('已删除' + label + ' ' + removed + ' 项', { level: 'success' });
}

export async function confirmFileDelete() {
  const pending = pc().pendingFileDelete.value;
  if (!pending) return;
  const { kind, paths, assetId } = pending;
  // 实际成功删除数由循环内累加（跳过项不算），用于播报真实结果
  let removed = 0;
  try {
    if (kind === 'remote') {
      // 归属校验：弹窗打开期间若切换了资产，这些 path 已属于另一台机器。
      // 两台存在同名绝对路径时会删错服务器，因此这里直接拒绝而不是"尽力而为"。
      if ((pc().wb().selectedAsset?.id ?? null) !== assetId) {
        pc().announce('已切换资产，取消删除以免误删其它服务器上的同名文件（请重新选择后删除）', { level: 'warn' });
        pc().pendingFileDelete.value = null;
        pc().wb().modal = { type: null };
        return;
      }
      const session = getActiveSession();
      if (!session) {
        pc().announce(noSessionMessage('删除'), { level: 'warn' });
        return;
      }
      await pc().withFileOperation('remote', '正在删除远程文件...', async () => {
        for (const path of paths) {
          const entry = pc().remoteEntries.value.find(e => e.path === path);
          if (!entry) continue;
          await invokeBackend('sftp_remove', { sessionId: session.sessionId, path, kind: entry.kind });
          removed += 1;
        }
        if (paths.length > 1) ls().selectedRemote.value = new Set();
        await refreshRemoteFiles(pc().remotePath.value);
      });
      announceDeleteResult('远程', removed, paths.length - removed);
    } else {
      await pc().withFileOperation('local', '正在删除本地文件...', async () => {
        for (const path of paths) {
          const entry = pc().localEntries.value.find(e => e.path === path);
          if (!entry) continue;
          await invokeBackend('fs_local_delete', { path, kind: entry.kind });
          removed += 1;
        }
        await refreshLocalFiles(pc().localPath.value);
      });
      announceDeleteResult('本地', removed, paths.length - removed);
    }
    pc().pendingFileDelete.value = null;
    pc().wb().modal = { type: null };
  } catch (error) {
    // 删除失败：保留 pending 与弹窗，用户可直接重试（不做静默失败）
    pc().announce('删除失败：' + errorMessage(error), { level: 'error' });
  }
}

export function cancelFileDelete() {
  pc().pendingFileDelete.value = null;
  pc().wb().modal = { type: null };
}

// ============================================================
// Local fs_local_* 操作
// ============================================================
export async function refreshLocalFiles(path: string | null = null) {
  if (!isTauriRuntime()) {
    pc().announce('本地浏览需要桌面客户端（npm run tauri:dev）', { level: 'warn' });
    return;
  }
  try {
    await pc().withFileOperation('local', '正在读取本地目录...', async () => {
      const target = path !== null ? path : (pc().localPath.value || await invokeBackend<string>('fs_local_home_dir'));
      const result = await invokeBackend<LocalDirectoryListResult>('fs_local_list_dir', { path: target });
      pc().localPath.value = result.path;
      pc().localEntries.value = result.entries || [];
    });
  } catch (error) {
    pc().announce('本地目录读取失败：' + errorMessage(error), { level: 'error' });
  }
}

export async function navigateLocalPath(target: string) {
  if (!target) return;
  await refreshLocalFiles(target);
}

export async function navigateLocalUp() {
  if (!pc().localPath.value) return;
  try {
    const result = await pc().withFileOperation('local', '正在读取本地目录...', () =>
      invokeBackend<LocalDirectoryListResult>('fs_local_list_dir', { path: pc().localPath.value })
    );
    if (!result.parent || result.parent === pc().localPath.value) {
      pc().announce('已是根目录');
      return;
    }
    await refreshLocalFiles(result.parent);
  } catch (error) {
    pc().announce('返回上级失败：' + errorMessage(error));
  }
}

export async function localMkdir(name: string) {
  if (!name?.trim()) {
    pc().announce('目录名不能为空', { level: 'warn' });
    return;
  }
  try {
    await pc().withFileOperation('local', '正在创建本地目录...', async () => {
      const target = joinLocalPath(pc().localPath.value, name.trim());
      await invokeBackend('fs_local_mkdir', { path: target });
      await refreshLocalFiles(pc().localPath.value);
    });
    pc().announce('已创建本地目录：' + name.trim(), { level: 'success' });
  } catch (error) {
    pc().announce('创建本地目录失败：' + errorMessage(error), { level: 'error' });
    throw error;
  }
}

export async function localRename(oldPath: string, newName: string) {
  if (!newName?.trim()) {
    pc().announce('新名称不能为空', { level: 'warn' });
    return;
  }
  try {
    await pc().withFileOperation('local', '正在重命名本地文件...', async () => {
      const newPath = joinLocalPath(parentLocalPath(oldPath), newName.trim());
      await invokeBackend('fs_local_rename', { oldPath, newPath });
      await refreshLocalFiles(pc().localPath.value);
    });
    pc().announce('已重命名：' + newName.trim(), { level: 'success' });
  } catch (error) {
    pc().announce('重命名失败：' + errorMessage(error), { level: 'error' });
    throw error;
  }
}

export function setLocalViewMode(mode: string) {
  if (mode !== 'queue' && mode !== 'browser') return;
  pc().localViewMode.value = mode;
  if (mode === 'browser' && !pc().localEntries.value.length) refreshLocalFiles().catch(() => null);
}

// ============================================================
// 列表视图域（排序/过滤视图 + 多选/视图模式，v2.8 第五轮自 shell 迁入）
// ============================================================

/** 视图状态由 shell 持有并经 bindRemoteListState 注入（箭头/refs 取值恒新）。 */
export interface RemoteListViews {
  filtered: RemoteFileEntry[];
  sorted: RemoteFileEntry[];
}
interface RemoteListState {
  filter: { value: string };
  sortKey: { value: string };
  sortDir: { value: string };
  lastRemote: { value: number };
  lastLocal: { value: number };
  selectedRemote: { value: Set<string> };
  selectedLocal: { value: Set<string> };
  remoteEntries: { value: RemoteFileEntry[] };
  listMode: { value: string };
  localListMode: { value: string };
}

let st: RemoteListState | null = null;

export function bindRemoteListState(state: RemoteListState): void {
  st = state;
}

function ls(): RemoteListState {
  if (!st) throw new Error('filePanel 列表视图状态未绑定（files store 未初始化）');
  return st;
}

/** 排序/过滤视图（原 shell 的 filteredRemoteEntries/sortedRemoteEntries computed，逐句迁移）。 */
export function computeFiltered(): RemoteFileEntry[] {
  const q = ls().filter.value.trim().toLowerCase();
  if (!q) return ls().remoteEntries.value;
  return ls().remoteEntries.value.filter(e => e.name.toLowerCase().includes(q));
}

export function sortEntries(filtered: RemoteFileEntry[]): RemoteFileEntry[] {
  const key = ls().sortKey.value;
  const dir = ls().sortDir.value === 'asc' ? 1 : -1;
  // 类型推断：目录→DIR / 符号链接→LNK / 普通文件→扩展名大写（无扩展名→FILE）。
  const typeOf = (e: RemoteFileEntry) => {
    if (e.kind === 'directory') return 'DIR';
    if (e.kind === 'symlink') return 'LNK';
    const dot = e.name.lastIndexOf('.');
    if (dot <= 0 || dot === e.name.length - 1) return 'FILE';
    return e.name.slice(dot + 1).toUpperCase();
  };
  // 用户:组 组合串用于排序。
  const ownerOf = (e: RemoteFileEntry) => [e.user || '', e.group || ''].join(':');
  const cmp = (a: RemoteFileEntry, b: RemoteFileEntry) => {
    const aDir = a.kind === 'directory' ? 0 : 1;
    const bDir = b.kind === 'directory' ? 0 : 1;
    if (aDir !== bDir) return aDir - bDir;
    let av: number | string;
    let bv: number | string;
    if (key === 'size') { av = a.size || 0; bv = b.size || 0; }
    else if (key === 'modified') { av = Number(a.modified) || 0; bv = Number(b.modified) || 0; }
    else if (key === 'type') { av = typeOf(a); bv = typeOf(b); }
    else if (key === 'permissions') {
      // 权限按八进制数值排（缺权限当作 0）。
      av = a.permissions ? parseInt(a.permissions, 8) || 0 : 0;
      bv = b.permissions ? parseInt(b.permissions, 8) || 0 : 0;
    }
    else if (key === 'owner') { av = ownerOf(a); bv = ownerOf(b); }
    else { av = a.name.toLowerCase(); bv = b.name.toLowerCase(); }
    if (av < bv) return -1 * dir;
    if (av > bv) return 1 * dir;
    return 0;
  };
  return [...filtered].sort(cmp);
}

  // ============================================================
  // Remote 多选 / 排序 / 过滤
  // ============================================================
  export function toggleRemoteSelection(path: string, { additive = false, range = false } = {}) {
    const list = sortEntries(computeFiltered());
    const idx = list.findIndex(e => e.path === path);
    if (range && ls().lastRemote.value >= 0 && idx >= 0) {
      const [start, end] = [ls().lastRemote.value, idx].sort((a, b) => a - b);
      const next = new Set(ls().selectedRemote.value);
      for (let i = start; i <= end; i++) next.add(list[i].path);
      ls().selectedRemote.value = next;
      return;
    }
    const next = new Set(additive ? ls().selectedRemote.value : []);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    ls().selectedRemote.value = next;
    ls().lastRemote.value = idx;
  }

  export function selectAllRemote() {
    ls().selectedRemote.value = new Set(sortEntries(computeFiltered()).map(e => e.path));
  }

  export function clearRemoteSelection() {
    if (ls().selectedRemote.value.size) ls().selectedRemote.value = new Set();
    ls().lastRemote.value = -1;
  }

  export function toggleLocalSelection(path: string, { additive = false, range = false } = {}) {
    const list = pc().localEntries.value;
    const idx = list.findIndex(e => e.path === path);
    if (range && ls().lastLocal.value >= 0 && idx >= 0) {
      // shift 范围多选：与远程 range 逻辑同构（参照 toggleRemoteSelection）
      const [start, end] = [ls().lastLocal.value, idx].sort((a, b) => a - b);
      const next = new Set(ls().selectedLocal.value);
      for (let i = start; i <= end; i++) next.add(list[i].path);
      ls().selectedLocal.value = next;
      return;
    }
    const next = new Set(additive ? ls().selectedLocal.value : []);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    ls().selectedLocal.value = next;
    ls().lastLocal.value = idx;
  }

  export function selectAllLocal() {
    ls().selectedLocal.value = new Set(pc().localEntries.value.map(e => e.path));
  }

  export function clearLocalSelection() {
    if (ls().selectedLocal.value.size) ls().selectedLocal.value = new Set();
  }

  export function setRemoteSort(key: string) {
    if (ls().sortKey.value === key) {
      ls().sortDir.value = ls().sortDir.value === 'asc' ? 'desc' : 'asc';
    } else {
      ls().sortKey.value = key;
      ls().sortDir.value = 'asc';
    }
  }

  export function setRemoteFilter(query: string) {
    ls().filter.value = query;
  }

  export function setRemoteListMode(mode: string) {
    if (mode === 'compact' || mode === 'detailed') ls().listMode.value = mode;
  }

  export function setLocalListMode(mode: string) {
    if (mode === 'compact' || mode === 'detailed') ls().localListMode.value = mode;
  }

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
