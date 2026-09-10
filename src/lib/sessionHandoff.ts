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
import type { Terminal } from '@xterm/xterm';
import type { SerializeAddon } from '@xterm/addon-serialize';
import type { NormalizedConnectionAsset } from '@/types/domain';
import {
  emitBackendEvent,
  getExistingTauriWebviewWindow,
  getTauriWindow,
  invokeBackend,
  listenBackendEvent
} from '../services/backend';
import { assetWindowLabel, openAssetWindow } from './assetWindows';

export const HANDOFF_EVENTS = {
  TEAROFF: 'session-handoff-tearoff',
  MERGE_PUSH: 'session-handoff-merge-push',
  // v2.4 移回主窗口的回执：主窗口 adopt 成败回告发起的 asset 窗口。
  // 此前 push 是「先斩后奏」——先移除本地会话再广播，主窗口 adopt 失败时
  // 会话随 take 原子删除直接蒸发，asset 窗口还自动关窗（用户视角＝点一下
  // 按钮什么都没了）。ACK 协议下失败/超时会话原地无恙。
  MERGE_ACK: 'session-handoff-merge-ack'
};

const SCROLLBACK_MAX_LINES = 2000;
const MAX_JSON_CHARS = 1000000; // 单条 handoff 的字符预算（防超大 scrollback 撑爆 IPC）

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
// 数据层：scrollback 导出 + Rust 内存中转读写（TTL 60s 由 Rust 侧管理）
// ============================================================

/** 迁移协议涉及的 session 最小形状（真实 session 的结构子集，调用方传入协变兼容）。 */
export interface HandoffSessionEntry {
  sessionId: string;
  asset?: NormalizedConnectionAsset | null;
  oscTitle?: string | null;
  term?: Terminal | null;
  serialize?: SerializeAddon | null;
  unlisten?: (() => void | Promise<void>) | null;
}

/** 会话迁移数据（Rust 中转 take 的返回 / adoptSession 的入参形状）。 */
export interface SessionHandoffData {
  sessionId: string;
  assetId: string;
  oscTitle: string;
  scrollback: string;
  style?: { fontSize: number; lineHeight: number };
}

/** 迁移原语所需的最小 sessions store 形状（方法签名双变，真实 store 协变兼容）。 */
export interface HandoffSessionsStore {
  sessions: HandoffSessionEntry[];
  terminalFontSize: number;
  terminalLineHeight: number;
  removeSessionEntry(session: HandoffSessionEntry): void;
  adoptSession(data: SessionHandoffData): Promise<boolean>;
}

/** 迁移原语所需的最小 workbench store 形状（成员全部可选，防御式调用）。 */
export interface HandoffWorkbenchStore {
  announce?(message: string, options?: { level?: string }): unknown;
  onSessionClosed?(assetId?: string | null): unknown;
}

// 遍历 normal buffer 导出纯文本（serialize addon 不可用时的回退）。已知限制：
// 不处理 isWrapped 软换行（超宽行会被拆成多行还原）；alternate buffer 场景
// （vim/less 中拖出）只能还原进 alt 前的 normal 内容。
export function exportTerminalText(term?: Terminal | null, maxLines: number = SCROLLBACK_MAX_LINES): string {
  const buffer = term?.buffer?.normal;
  if (!buffer) return '';
  const lines: string[] = [];
  for (let i = 0; i < buffer.length; i++) {
    lines.push(buffer.getLine(i)?.translateToString(true) ?? '');
  }
  // 去尾部空行（translateToString(true) 已 trimRight，空行即 ''）
  while (lines.length && lines[lines.length - 1] === '') lines.pop();
  const start = Math.max(0, lines.length - maxLines);
  return lines.slice(start).join('\r\n');
}

// 跨窗口迁移导出（优先路径）：SerializeAddon 序列化 scrollback 为带 SGR
// 颜色/样式的文本，回放后 prompt 等 ANSI 色得以保留（曾用纯文本导出，迁移后
// 终端历史整体褪成默认前景色——用户实测「debian@debian:~ 蓝色 prompt 变白」）。
// excludeModes：不带源终端 mode 状态（光标形状等，回放端不该继承）；
// excludeAltBuffer：拖出时正在 vim/less 只还原 normal buffer（与旧语义一致）。
export function exportTerminalScrollback(session?: HandoffSessionEntry | null, maxLines: number = SCROLLBACK_MAX_LINES): string {
  if (session?.serialize) {
    try {
      const text = session.serialize.serialize({
        scrollback: maxLines,
        excludeModes: true,
        excludeAltBuffer: true
      });
      if (text) return text;
    } catch { /* serialize 失败回退纯文本（老会话对象无 addon 等） */ }
  }
  return exportTerminalText(session?.term, maxLines);
}

// 写入 Rust 中转。字符预算：scrollback 超预算时从头部按 500 行步进截断
// （Rust 内存无配额，无需 localStorage 时代的 quota 重试）。
// style（可选）：终端渲染样式快照（fontSize/lineHeight/themeMode）——每个窗口
// 独立 webview + 独立 localStorage 缓存（WebView2 跨窗口非实时），目标窗口
// 读到的可能是陈旧设置；迁移携带源窗口实际值，adopt 时所见即所得还原。
export interface WriteHandoffDataArgs {
  sessionId: string;
  assetId?: string | null;
  scrollback?: string | null;
  oscTitle?: string | null;
  style?: { fontSize?: number; lineHeight?: number } | null;
}

export async function writeHandoffData({ sessionId, assetId, scrollback, oscTitle, style }: WriteHandoffDataArgs): Promise<void> {
  const meta: {
    sessionId: string;
    assetId: string;
    oscTitle: string;
    style?: { fontSize: number; lineHeight: number };
  } = {
    sessionId: String(sessionId),
    assetId: String(assetId || ''),
    oscTitle: String(oscTitle || '')
  };
  if (style) {
    meta.style = {
      fontSize: Number(style.fontSize) || 12,
      lineHeight: Number(style.lineHeight) || 1
      // themeMode 不再携带：终端始终深色，与 app 主题解耦（见 terminalThemes.js）
    };
  }
  let lines = String(scrollback || '').split('\r\n');
  let json = JSON.stringify({ ...meta, scrollback: lines.join('\r\n') });
  while (json.length > MAX_JSON_CHARS && lines.length > 0) {
    lines = lines.length <= 500 ? [] : lines.slice(500);
    // 带 SGR 的序列化是 delta 式（颜色变化才发序列），从头部截断会令后续行
    // 丢失颜色前序——prepend reset 使其回落默认色，避免继承错色
    json = JSON.stringify({
      ...meta,
      scrollback: lines.length ? '\x1b[0m\r\n' + lines.join('\r\n') : ''
    });
  }
  await invokeBackend('session_handoff_put', { payload: JSON.parse(json) });
}

// 读取即原子取出（take 即删，防多窗口重复 adopt）；失败/缺失/过期一律 null。
export async function readHandoffData(sessionId: string): Promise<SessionHandoffData | null> {
  try {
    return await invokeBackend<SessionHandoffData>('session_handoff_take', { sessionId }) || null;
  } catch {
    return null;
  }
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

// ============================================================
// MERGE_ACK 等待（asset 窗口侧）：push 后挂起等主窗口回执，超时兜底
// ============================================================
interface MergeAckResult {
  ok: boolean;
  reason?: string;
}

interface PendingAckEntry {
  resolve: (result: MergeAckResult) => void;
  timer: ReturnType<typeof setTimeout> | null;
}

const pendingAcks = new Map<string, PendingAckEntry>(); // sessionId -> { resolve, timer }

/**
 * 主窗口接管回执的等待上限（v2.6：6s → 20s，另加 10s 宽限轮询）。
 *
 * 旧值 6000ms 把「ACK 还没来」当成「主窗口没接管」：主窗口最小化时
 * `adoptSession` 里的 requestAnimationFrame 被节流、首次 adopt 还要冷加载 xterm 与
 * 分帧回放，很容易超过 6s → 弹「移回失败」但主窗口稍后**仍然接管成功**，变成两个窗口
 * 同时监听同一会话（用户之后关 asset 窗口还会把主窗口正在用的连接断掉）。
 * 20s + 宽限轮询覆盖冷启动与节流场景；已知残留：ACK 在宽限期之后才到时本窗口不会
 * 自动移交（超时文案已改为「尚未确认」并说明可从主窗口继续使用，不再谎报失败）。
 */
const MERGE_ACK_TIMEOUT_MS = 20000;

async function waitMergeAck(sessionId: string, timeoutMs = MERGE_ACK_TIMEOUT_MS): Promise<MergeAckResult> {
  return new Promise(resolve => {
    const entry: PendingAckEntry = { resolve, timer: null };
    entry.timer = setTimeout(() => {
      pendingAcks.delete(sessionId);
      resolve({ ok: false, reason: 'timeout' });
    }, timeoutMs);
    pendingAcks.set(sessionId, entry);
  });
}

function settleMergeAck(sessionId: string, result: MergeAckResult): boolean {
  const entry = pendingAcks.get(sessionId);
  if (!entry) return false;
  pendingAcks.delete(sessionId);
  clearTimeout(entry.timer!);
  entry.resolve(result);
  return true;
}

/** 是否仍在等这条会话的 MERGE_ACK（未超时/未被 settle）。 */
export function isMergeAckPending(sessionId: string): boolean {
  return pendingAcks.has(sessionId);
}

/**
 * 等待 ACK 的 Promise **或** 一个宽限轮询（谁先有结果用谁）。
 *
 * 为什么需要轮询：ACK 到达时 `settleMergeAck` 会 resolve 原 Promise；但如果上游因
 * 别的原因提前放弃了那个 Promise（旧实现的 6s 超时即如此），late ACK 就没人接。
 * 宽限期版本让「主窗口慢但在推进」的场景能自我纠正。
 */
function waitMergeAckWithGrace(sessionId: string, primary: Promise<MergeAckResult>, graceMs = 10000): Promise<MergeAckResult> {
  return new Promise(resolve => {
    let done = false;
    const finish = (result: MergeAckResult) => {
      if (done) return;
      done = true;
      clearInterval(poll);
      resolve(result);
    };
    const poll = setInterval(() => {
      // ACK 在宽限期内到达 → 主窗口确实接管了；原 Promise 可能已被超时弃用
      if (!pendingAcks.has(sessionId)) finish({ ok: true, reason: 'late-ack' });
    }, 250);
    void primary.then(finish);
    setTimeout(() => finish({ ok: false, reason: 'timeout' }), graceMs);
  });
}

// asset 窗口标题栏「移回主窗口」（v2.4 ACK 协议）：
// 1. 确认主窗口存活 → 2. 写中转（会话保持本地活态）→ 3. 广播 MERGE_PUSH
// → 4. 等 MERGE_ACK：ok 才真正 detach + 空窗自动关；失败/超时会话原地无恙。
// 会话被主窗口接管到 ACK 到达之间两窗口短暂同时监听 ssh-output（毫秒级，
// 旧终端随即 dispose，无害）。
export async function pushSessionToMainWindow({ sessionsStore, workbenchStore, sessionId }: MigrateSessionOutArgs): Promise<boolean> {
  const session = sessionsStore.sessions.find(item => item.sessionId === sessionId);
  if (!session) return false;
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
