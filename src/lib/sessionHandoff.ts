/**
 * sessionHandoff — 跨窗口会话所有权迁移协议（Windows Terminal 式 tab 拆出/移回）。
 *
 * 每个 Tauri 窗口独立 webview + Pinia 实例，但共享 Rust 后端。迁移数据
 * （scrollback + oscTitle）经 Rust 侧内存中转（session_handoff_put / take，
 * TTL 60s，take 即原子删除）——同进程强一致。此前用 localStorage 中转，
 * 但 WebView2 跨窗口 localStorage 非实时共享，新建窗口 boot 时读不到源
 * 窗口刚写入的数据，adopt 误判失败走兜底重连，故弃用。
 *
 * 迁移 = 源窗口导出 scrollback 到 Rust 中转 → 解绑本窗口事件监听（必须先于
 * remove，防双写与假「远程连接已关闭」announce）并移除 session（不动 Rust 侧
 * SSH 连接）→ 通知目标窗口从 Rust 中转读取并 adoptSession 重建终端（回放
 * scrollback + 重新接线 ssh-output-<sessionId> 监听）。migrateSessionOut
 * 模块级 inFlight 互斥防双迁移。
 *
 * 协议仅两个事件（均带 sourceWindowId，接收方校验防自吞）：
 * - TEAROFF：主窗口 tab 拖出 → 同资产 asset 窗口 / 新开窗口 adopt；
 * - MERGE_PUSH：asset 窗口「移回主窗口」按钮 → 主窗口 adopt。
 */
import type { NormalizedConnectionAsset } from '@/types/domain';
import {
  emitBackendEvent,
  getExistingTauriWebviewWindow,
  getTauriWindow,
  invokeBackend,
  listenBackendEvent
} from '../services/backend';
import { assetWindowLabel, openAssetWindow } from './assetWindows';
// v0.20（S2 刀）：数据层已拆至 lib/handoffData.ts（类型/scrollback 导出/Rust 中转读写）
import {
  SCROLLBACK_MAX_LINES, exportTerminalScrollback, writeHandoffData, readHandoffData,
  type HandoffSessionEntry, type HandoffSessionsStore, type HandoffWorkbenchStore
} from './handoffData';
import { waitMergeAck, waitMergeAckWithGrace, settleMergeAck, isMergeAckPending } from './handoffMergeAck';
export {
  exportTerminalText, exportTerminalScrollback, writeHandoffData, readHandoffData,
  SCROLLBACK_MAX_LINES, MAX_JSON_CHARS
} from './handoffData';
export type {
  HandoffSessionEntry, SessionHandoffData, HandoffSessionsStore, HandoffWorkbenchStore, WriteHandoffDataArgs
} from './handoffData';
// v0.20（S2 刀）：MERGE_ACK 等待机制拆至 lib/handoffMergeAck.ts
export { isMergeAckPending } from './handoffMergeAck';

export const HANDOFF_EVENTS = {
  TEAROFF: 'session-handoff-tearoff',
  MERGE_PUSH: 'session-handoff-merge-push',
  // v2.4 移回主窗口的回执：主窗口 adopt 成败回告发起的 asset 窗口。
  // 此前 push 是「先斩后奏」——先移除本地会话再广播，主窗口 adopt 失败时
  // 会话随 take 原子删除直接蒸发，asset 窗口还自动关窗（用户视角＝点一下
  // 按钮什么都没了）。ACK 协议下失败/超时会话原地无恙。
  MERGE_ACK: 'session-handoff-merge-ack'
};


// ============================================================
// 窗口身份（模块加载即定，不随运行变化）
// ============================================================
export type WindowRole = 'main' | 'asset';

const WINDOW_ROLE: WindowRole = typeof window !== 'undefined'
  && new URLSearchParams(window.location.search).get('win') === 'asset'
  ? 'asset'
  : 'main';

export function getWindowRole(): WindowRole {
  return WINDOW_ROLE;
}

let myWindowId: string | null = null;
export function getMyWindowId(): string {
  if (!myWindowId) {
    myWindowId = typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
      ? crypto.randomUUID()
      : 'win-' + Date.now().toString(36) + '-' + Math.random().toString(36).slice(2, 10);
  }
  return myWindowId;
}

// ============================================================
// 迁移原语（模块级私有，单一互斥）
// ============================================================
const inFlight = new Set<string>();

interface MigrateSessionOutArgs {
  sessionsStore: HandoffSessionsStore;
  workbenchStore?: HandoffWorkbenchStore | null;
  sessionId: string;
}

// 源窗口侧统一出口（tearoff / merge-push 共用）：导出 → 先 unlisten 再 remove
// （防迁移后旧窗口仍监听 ssh-output 造成双写，以及 closed 监听误报）→ 该资产
// 在本窗口已无其他会话时通知文件面板清空。返回 false = 会话不存在或互斥跳过。
async function migrateSessionOut({ sessionsStore, workbenchStore, sessionId }: MigrateSessionOutArgs): Promise<HandoffSessionEntry | false> {
  if (inFlight.has(sessionId)) return false;
  inFlight.add(sessionId);
  try {
    const session = sessionsStore.sessions.find(item => item.sessionId === sessionId);
    if (!session) return false;
    await writeHandoffData({
      sessionId,
      assetId: session.asset?.id,
      scrollback: exportTerminalScrollback(session, SCROLLBACK_MAX_LINES),
      oscTitle: session.oscTitle || '',
      style: {
        fontSize: sessionsStore.terminalFontSize,
        lineHeight: sessionsStore.terminalLineHeight
        // themeMode 不携带：终端始终深色，见 terminalThemes.js
      }
    });
    return detachSessionLocal({ sessionsStore, workbenchStore, session });
  } finally {
    inFlight.delete(sessionId);
  }
}

interface DetachSessionLocalArgs {
  sessionsStore: HandoffSessionsStore;
  workbenchStore?: HandoffWorkbenchStore | null;
  session: HandoffSessionEntry;
}

// 本地移交（不动 Rust 侧 SSH 连接）：unlisten → remove → 面板联动。ACK 协议
// 中在「主窗口已确认接管」之后才调用。返回移交的 session（未找到为 null）。
async function detachSessionLocal({ sessionsStore, workbenchStore, session }: DetachSessionLocalArgs): Promise<HandoffSessionEntry> {
  await session.unlisten?.();
  sessionsStore.removeSessionEntry(session);
  if (!sessionsStore.sessions.some(item => item.asset?.id === session.asset?.id)) {
    try { workbenchStore?.onSessionClosed?.(session.asset?.id); } catch { /* noop */ }
  }
  return session;
}

interface AdoptFromStorageArgs {
  sessionsStore: HandoffSessionsStore;
  workbenchStore?: HandoffWorkbenchStore | null;
  sessionId: string;
}

// 目标窗口侧统一入口：take（原子删除）→ adoptSession。注意 take 后数据不可
// 重试——adopt 失败由调用方兜底（boot 路径断开重连 / 事件路径记录后丢弃）。
// 失败原因只进 console（adoptSession 内部已带具体分支日志），不打扰用户。
async function adoptFromStorage({ sessionsStore, workbenchStore, sessionId }: AdoptFromStorageArgs): Promise<boolean> {
  const data = await readHandoffData(sessionId);
  if (!data) {
    console.warn('[sessionHandoff] adopt 失败：中转数据缺失或已过期', { sessionId });
    return false;
  }
  try {
    return await sessionsStore.adoptSession(data);
  } catch (error) {
    console.error('[sessionHandoff] adoptSession 抛错:', error);
    return false;
  }
}

// ============================================================
// 对外 API：tab 拆出独立窗口 / asset 窗口移回主窗口
// ============================================================

// 主窗口 tab 拖出窗外：迁移 + 广播 TEAROFF + 开独立窗口（dragend 即执行，
// 无 drop 仲裁窗口期——tab 栏 drop 合并入口已随独立窗口去 tab 条移除）。
export async function tearOffSession({ sessionsStore, workbenchStore, sessionId }: MigrateSessionOutArgs): Promise<void> {
  const session = sessionsStore.sessions.find(item => item.sessionId === sessionId);
  if (!session) return;
  const asset = session.asset;
  try {
    // v2.6 就绪门（D-6 修复）：目标窗口**已存在**时，它可能仍在加载/boot（TEAROFF
    // 监听尚未注册）。旧实现照样迁移+广播 → 事件无人接、会话从源窗口消失、后端留
    // 孤儿连接，且没有任何提示。现在：已存在但未上报就绪 → 先等它 ready（≤3s）；
    // 等不到就放弃本次拖出并说明，不迁移（会话原地不动，用户可重试）。
    // 目标窗口不存在（首次开窗）时无此问题：URL 自带 adopt= 参数，boot 直接接管。
    if (asset) {
      const targetLabel = assetWindowLabel(asset);
      const existing = await getExistingTauriWebviewWindow(targetLabel);
      if (existing) {
        const ready = await waitHandoffReady(targetLabel, 3000);
        if (!ready) {
          workbenchStore?.announce?.('目标窗口正在启动，请稍后重试拖出', { level: 'warn' });
          return;
        }
      }
    }
    const ok = await migrateSessionOut({ sessionsStore, workbenchStore, sessionId });
    if (!ok) return;
    await emitBackendEvent(HANDOFF_EVENTS.TEAROFF, {
      sessionId,
      assetId: asset?.id,
      sourceWindowId: getMyWindowId()
    });
    await openAssetWindow(asset, { adoptSessionId: sessionId });
  } catch (error) {
    // 失败兜底：会话可能已被 remove（migrate 成功但 emit/开窗失败）→ 断开防孤儿连接
    if (!sessionsStore.sessions.some(item => item.sessionId === sessionId)) {
      invokeBackend('ssh_disconnect', { sessionId }).catch(() => null);
    }
    workbenchStore?.announce?.('拆出独立窗口失败：' + (error instanceof Error ? error.message : String(error)), { level: 'error' });
  }
}

// ============================================================
// 目标窗口就绪握手（v2.6，D-6）
//
// `getByLabel` 查得到窗口 ≠ 该窗口的 webview 已跑到注册监听那一步——页面加载 +
// onMounted 之间有数百毫秒到数秒的空窗期（慢机器更久）。源窗口在此刻迁移会话，
// TEAROFF 就没人接。故让每个窗口 boot 后广播 `handoff-ready`（带自己的 label），
// 源窗口在迁移前据此确认目标已就绪。
// ============================================================

const HANDOFF_READY_EVENT = 'handoff-ready';
/** 已上报就绪的窗口 label（本进程内）。 */
const readyWindows = new Set<string>();

function waitHandoffReady(label: string, timeoutMs: number): Promise<boolean> {
  if (readyWindows.has(label)) return Promise.resolve(true);
  return new Promise(resolve => {
    let done = false;
    const finish = (ok: boolean) => {
      if (done) return;
      done = true;
      clearInterval(poll);
      clearTimeout(giveUp);
      resolve(ok);
    };
    // 事件驱动的登记（onHandoffReadyEvent）+ 轮询兜底：ready 事件可能在 setup 时就
    // 已到达而本函数尚未调用（注册与查询时序无关，读 Set 即可）。
    const poll = setInterval(() => {
      if (readyWindows.has(label)) finish(true);
    }, 100);
    const giveUp = setTimeout(() => finish(false), timeoutMs);
  });
}

/** 收到任何窗口的 `handoff-ready` 广播：登记该 label 已可接管。 */
function onHandoffReadyEvent(event: TauriEvent) {
  const payload = event?.payload as { windowLabel?: string; role?: string; assetId?: string } | undefined;
  const label = payload?.windowLabel;
  if (typeof label === 'string' && label) readyWindows.add(label);
}

/** 本窗口 boot 完成、监听已就位后广播就绪（主窗口与 asset 窗口都要发）。 */
export async function announceHandoffReady(assetId?: string): Promise<void> {
  // 自己的 label 也登记：同一进程内查询无需等事件回来
  try {
    const current = getTauriWindow() as TauriWindowLike | null;
    const label = current?.label;
    if (typeof label === 'string' && label) readyWindows.add(label);
  } catch {
    // 非 Tauri runtime（浏览器预览）：无窗口标签可登记，跳过即可
    return;
  }
  await emitBackendEvent(HANDOFF_READY_EVENT, {
    windowLabel: (getTauriWindow() as TauriWindowLike | null)?.label,
    role: getWindowRole(),
    assetId
  }).catch(() => null);
}

/** 测试/诊断用：当前已知就绪的窗口 label。 */
export function listReadyWindows(): string[] {
  return [...readyWindows];
}

// asset 窗口标题栏「移回主窗口」（v2.4 ACK 协议）：
// 1. 确认主窗口存活 → 2. 写中转（会话保持本地活态）→ 3. 广播 MERGE_PUSH
// → 4. 等 MERGE_ACK：ok 才真正 detach + 空窗自动关；失败/超时会话原地无恙。
// 会话被主窗口接管到 ACK 到达之间两窗口短暂同时监听 ssh-output（毫秒级，
// 旧终端随即 dispose，无害）。
export async function pushSessionToMainWindow({ sessionsStore, workbenchStore, sessionId }: MigrateSessionOutArgs): Promise<boolean> {
  const session = sessionsStore.sessions.find(item => item.sessionId === sessionId);
  if (!session) return false;
  // 在途传输守卫（必须早于 detachSessionLocal，也早于任何中转写入）：移交成功后
  // 本窗口会随空窗自动销毁，而在途上传/下载的写入句柄由本窗口 JS 侧的传输代码
  // 收尾——上下文一消失就再没人清理，句柄成为孤儿（远端留半截文件，且每个泄漏
  // 句柄持续占用 OpenSSH MaxSessions 配额）。关窗按钮已有同义守卫（见
  // AssetWindowShell.requestCloseAssetWindow），但这里直接 destroy() 绕过
  // close-requested，故守卫需在协议层再拦一道。
  // 先拒后动：不写中转、不移交、不销毁、不触发 MERGE_PUSH，本窗口状态完整保留。
  const activeTransfers = workbenchStore?.activeTransfers?.length || 0;
  if (activeTransfers > 0) {
    workbenchStore?.announce?.(
      `本窗口仍有 ${activeTransfers} 个传输任务，无法移回主窗口；请等待完成后再移回`,
      { level: 'warn' }
    );
    return false;
  }
  // getByLabel 是 async，须 await：主窗口已关闭时 MERGE_PUSH 无人应答
  const mainWindow = await getExistingTauriWebviewWindow('main');
  if (!mainWindow) {
    workbenchStore?.announce?.('主窗口未打开，无法移回', { level: 'error' });
    return false;
  }
  if (inFlight.has(sessionId)) return false;
  inFlight.add(sessionId);
  try {
    await writeHandoffData({
      sessionId,
      assetId: session.asset?.id,
      scrollback: exportTerminalScrollback(session, SCROLLBACK_MAX_LINES),
      oscTitle: session.oscTitle || '',
      style: {
        fontSize: sessionsStore.terminalFontSize,
        lineHeight: sessionsStore.terminalLineHeight
        // themeMode 不携带：终端始终深色，见 terminalThemes.js
      }
    });
    const ackPromise = waitMergeAck(sessionId);
    await emitBackendEvent(HANDOFF_EVENTS.MERGE_PUSH, {
      sessionId,
      assetId: session.asset?.id,
      sourceWindowId: getMyWindowId()
    });
    // 宽限对账：主窗口慢（最小化时 rAF 被节流、首次 adopt 冷加载）也不会被误判失败。
    // 旧实现 6s 超时后弹「未确认」而主窗口稍后照样接管 → 双窗口共持同一会话。
    const ack = await waitMergeAckWithGrace(sessionId, ackPromise);
    if (!ack.ok) {
      // 超时不再说成「失败」——事实是「还没收到回执」，会话仍在本窗口可用。
      // 不做本地 detach（主窗口可能已经在 adopt，移交所有权会造成会话无主）。
      console.warn('[sessionHandoff] 移回主窗口未在宽限期内确认（' + (ack.reason || 'unknown') + '），会话保留在本窗口', { sessionId });
      workbenchStore?.announce?.('移回主窗口尚未确认（主窗口可能仍在接管），会话暂时保留在本窗口', { level: 'warn' });
      return false;
    }
    if (ack.reason === 'late-ack') {
      // 迟到的 ACK：主窗口已接管，但原 Promise 已被超时弃用——这里补做移交
      console.warn('[sessionHandoff] 收到迟到的 MERGE_ACK，补做本窗口移交', { sessionId });
    }
    await detachSessionLocal({ sessionsStore, workbenchStore, session });
    if (!sessionsStore.sessions.length) {
      // tauri.d.ts 的最小窗口类型未声明 destroy，局部补齐（可选调用与原防御式一致）
      try { (getTauriWindow() as (TauriWindowLike & { destroy?: () => Promise<void> }) | null)?.destroy?.(); } catch { /* noop */ }
    }
    return true;
  } catch (error) {
    settleMergeAck(sessionId, { ok: false, reason: 'error' });
    console.error('[sessionHandoff] pushSessionToMainWindow 失败:', error);
    workbenchStore?.announce?.('移回主窗口失败：' + (error instanceof Error ? error.message : String(error)), { level: 'error' });
    return false;
  } finally {
    inFlight.delete(sessionId);
  }
}

// ============================================================
// 常驻监听矩阵（角色门控是硬要求）。返回 dispose 函数。
// ============================================================

/** TEAROFF / MERGE_PUSH / MERGE_ACK 事件的 payload 形状（字段按事件类型各有取舍）。 */
interface HandoffEventPayload {
  sessionId: string;
  assetId?: string;
  sourceWindowId?: string;
  targetWindowId?: string;
  ok?: boolean;
  reason?: string;
}

export interface SetupHandoffListenersArgs {
  sessionsStore: HandoffSessionsStore;
  workbenchStore?: HandoffWorkbenchStore | null;
  assetId?: string;
}

export function setupHandoffListeners({ sessionsStore, workbenchStore, assetId }: SetupHandoffListenersArgs): () => Promise<void> {
  const disposers: Array<() => void | Promise<void>> = [];
  let disposed = false;
  const track = (fn: () => void | Promise<void>) => {
    if (disposed) {
      try { fn(); } catch { /* noop */ }
    } else {
      disposers.push(fn);
    }
  };
  const on = (eventName: string, handler: TauriEventHandler) => {
    listenBackendEvent(eventName, handler).then(track).catch(() => null);
  };

  // handoff-ready：所有窗口都监听（登记别的窗口就绪，供拖出前的就绪门使用）。
  // 必须在其它监听之后注册也无妨——tearOffSession 是轮询 + Set 判定，
  // 事件比查询早到也不会漏（见 waitHandoffReady）。
  on(HANDOFF_READY_EVENT, onHandoffReadyEvent);

  // TEAROFF：仅 asset 窗口监听；assetId 匹配本窗口资产（另一资产的窗口忽略），
  // sourceWindowId 防自吞。
  if (getWindowRole() === 'asset') {
    on(HANDOFF_EVENTS.TEAROFF, async event => {
      // Tauri 事件 payload 是 unknown（tauri.d.ts 边界），按协议形状收窄
      const payload = event?.payload as HandoffEventPayload | undefined;
      if (!payload || payload.assetId !== assetId) return;
      if (payload.sourceWindowId === getMyWindowId()) return;
      const adopted = await adoptFromStorage({ sessionsStore, workbenchStore, sessionId: payload.sessionId });
      // 已接管（含并发路径成功）则无事可做
      if (adopted || sessionsStore.sessions.some(item => item.sessionId === payload.sessionId)) return;
      // adopt 失败防孤儿连接（对齐 assetWindowBoot boot 路径行为）：中转数据
      // 已被 take 原子删除、源窗口已移除会话，无人再能接管 → 主动断开 Rust 侧
      // 连接。断开失败无更高优先级的补救动作，吞错 + console 留痕即可。
      console.warn('[sessionHandoff] TEAROFF adopt 失败，断开孤儿会话:', payload.sessionId);
      invokeBackend('ssh_disconnect', { sessionId: payload.sessionId }).catch(() => null);
    });

    // MERGE_ACK：主窗口对 push 的回执（targetWindowId 定向到发起窗口）
    on(HANDOFF_EVENTS.MERGE_ACK, event => {
      const payload = event?.payload as HandoffEventPayload | undefined;
      if (!payload || payload.targetWindowId !== getMyWindowId()) return;
      settleMergeAck(payload.sessionId, { ok: Boolean(payload.ok), reason: payload.reason });
    });
  }

  // MERGE_PUSH：仅主窗口监听；sourceWindowId 防自吞。adopt 成败都回 ACK——
  // 发起方依赖回执决定是否移交/关窗。
  if (getWindowRole() === 'main') {
    on(HANDOFF_EVENTS.MERGE_PUSH, async event => {
      const payload = event?.payload as HandoffEventPayload | undefined;
      if (!payload || payload.sourceWindowId === getMyWindowId()) return;
      const ok = await adoptFromStorage({ sessionsStore, workbenchStore, sessionId: payload.sessionId });
      await emitBackendEvent(HANDOFF_EVENTS.MERGE_ACK, {
        sessionId: payload.sessionId,
        ok,
        reason: ok ? '' : 'adopt-failed',
        targetWindowId: payload.sourceWindowId
      }).catch(() => null);
    });
  }

  return async function dispose(): Promise<void> {
    disposed = true;
    for (const fn of disposers) {
      try { await fn(); } catch { /* noop */ }
    }
    disposers.length = 0;
  };
}
