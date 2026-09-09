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
import {
  emitBackendEvent,
  getExistingTauriWebviewWindow,
  getTauriWindow,
  invokeBackend,
  listenBackendEvent
} from '../services/backend.js';
import { openAssetWindow } from './assetWindows.js';

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
const WINDOW_ROLE = typeof window !== 'undefined'
  && new URLSearchParams(window.location.search).get('win') === 'asset'
  ? 'asset'
  : 'main';

export function getWindowRole() {
  return WINDOW_ROLE;
}

let myWindowId = null;
export function getMyWindowId() {
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

// 遍历 normal buffer 导出纯文本（serialize addon 不可用时的回退）。已知限制：
// 不处理 isWrapped 软换行（超宽行会被拆成多行还原）；alternate buffer 场景
// （vim/less 中拖出）只能还原进 alt 前的 normal 内容。
export function exportTerminalText(term, maxLines = SCROLLBACK_MAX_LINES) {
  const buffer = term?.buffer?.normal;
  if (!buffer) return '';
  const lines = [];
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
export function exportTerminalScrollback(session, maxLines = SCROLLBACK_MAX_LINES) {
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
export async function writeHandoffData({ sessionId, assetId, scrollback, oscTitle, style }) {
  const meta = {
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
export async function readHandoffData(sessionId) {
  try {
    return await invokeBackend('session_handoff_take', { sessionId }) || null;
  } catch {
    return null;
  }
}

// ============================================================
// 迁移原语（模块级私有，单一互斥）
// ============================================================
const inFlight = new Set();

// 源窗口侧统一出口（tearoff / merge-push 共用）：导出 → 先 unlisten 再 remove
// （防迁移后旧窗口仍监听 ssh-output 造成双写，以及 closed 监听误报）→ 该资产
// 在本窗口已无其他会话时通知文件面板清空。返回 false = 会话不存在或互斥跳过。
async function migrateSessionOut({ sessionsStore, workbenchStore, sessionId }) {
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

// 本地移交（不动 Rust 侧 SSH 连接）：unlisten → remove → 面板联动。ACK 协议
// 中在「主窗口已确认接管」之后才调用。返回移交的 session（未找到为 null）。
async function detachSessionLocal({ sessionsStore, workbenchStore, session }) {
  await session.unlisten?.();
  sessionsStore.removeSessionEntry(session);
  if (!sessionsStore.sessions.some(item => item.asset?.id === session.asset?.id)) {
    try { workbenchStore?.onSessionClosed?.(session.asset?.id); } catch { /* noop */ }
  }
  return session;
}

// 目标窗口侧统一入口：take（原子删除）→ adoptSession。注意 take 后数据不可
// 重试——adopt 失败由调用方兜底（boot 路径断开重连 / 事件路径记录后丢弃）。
// 失败原因只进 console（adoptSession 内部已带具体分支日志），不打扰用户。
async function adoptFromStorage({ sessionsStore, workbenchStore, sessionId }) {
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
export async function tearOffSession({ sessionsStore, workbenchStore, sessionId }) {
  const session = sessionsStore.sessions.find(item => item.sessionId === sessionId);
  if (!session) return;
  const asset = session.asset;
  try {
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
    workbenchStore?.announce?.('拆出独立窗口失败：' + (error?.message || error), { level: 'error' });
  }
}

// ============================================================
// MERGE_ACK 等待（asset 窗口侧）：push 后挂起等主窗口回执，超时兜底
// ============================================================
const pendingAcks = new Map(); // sessionId -> { resolve, timer }

function waitMergeAck(sessionId, timeoutMs = 6000) {
  return new Promise(resolve => {
    const entry = { resolve, timer: null };
    entry.timer = setTimeout(() => {
      pendingAcks.delete(sessionId);
      resolve({ ok: false, reason: 'timeout' });
    }, timeoutMs);
    pendingAcks.set(sessionId, entry);
  });
}

function settleMergeAck(sessionId, result) {
  const entry = pendingAcks.get(sessionId);
  if (!entry) return false;
  pendingAcks.delete(sessionId);
  clearTimeout(entry.timer);
  entry.resolve(result);
  return true;
}

// asset 窗口标题栏「移回主窗口」（v2.4 ACK 协议）：
// 1. 确认主窗口存活 → 2. 写中转（会话保持本地活态）→ 3. 广播 MERGE_PUSH
// → 4. 等 MERGE_ACK：ok 才真正 detach + 空窗自动关；失败/超时会话原地无恙。
// 会话被主窗口接管到 ACK 到达之间两窗口短暂同时监听 ssh-output（毫秒级，
// 旧终端随即 dispose，无害）。
export async function pushSessionToMainWindow({ sessionsStore, workbenchStore, sessionId }) {
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
    const ack = await ackPromise;
    if (!ack.ok) {
      console.warn('[sessionHandoff] 移回主窗口未确认（' + (ack.reason || 'adopt 失败') + '），会话保留在本窗口', { sessionId });
      workbenchStore?.announce?.('移回主窗口失败：主窗口未确认接管，会话已保留', { level: 'error' });
      return false;
    }
    await detachSessionLocal({ sessionsStore, workbenchStore, session });
    if (!sessionsStore.sessions.length) {
      try { getTauriWindow()?.destroy?.(); } catch { /* noop */ }
    }
    return true;
  } catch (error) {
    settleMergeAck(sessionId, { ok: false, reason: 'error' });
    console.error('[sessionHandoff] pushSessionToMainWindow 失败:', error);
    workbenchStore?.announce?.('移回主窗口失败：' + (error?.message || error), { level: 'error' });
    return false;
  } finally {
    inFlight.delete(sessionId);
  }
}

// ============================================================
// 常驻监听矩阵（角色门控是硬要求）。返回 dispose 函数。
// ============================================================
export function setupHandoffListeners({ sessionsStore, workbenchStore, assetId }) {
  const disposers = [];
  let disposed = false;
  const track = fn => {
    if (disposed) {
      try { fn(); } catch { /* noop */ }
    } else {
      disposers.push(fn);
    }
  };
  const on = (eventName, handler) => {
    listenBackendEvent(eventName, handler).then(track).catch(() => null);
  };

  // TEAROFF：仅 asset 窗口监听；assetId 匹配本窗口资产（另一资产的窗口忽略），
  // sourceWindowId 防自吞。
  if (getWindowRole() === 'asset') {
    on(HANDOFF_EVENTS.TEAROFF, async event => {
      const payload = event?.payload;
      if (!payload || payload.assetId !== assetId) return;
      if (payload.sourceWindowId === getMyWindowId()) return;
      await adoptFromStorage({ sessionsStore, workbenchStore, sessionId: payload.sessionId });
    });

    // MERGE_ACK：主窗口对 push 的回执（targetWindowId 定向到发起窗口）
    on(HANDOFF_EVENTS.MERGE_ACK, event => {
      const payload = event?.payload;
      if (!payload || payload.targetWindowId !== getMyWindowId()) return;
      settleMergeAck(payload.sessionId, { ok: Boolean(payload.ok), reason: payload.reason });
    });
  }

  // MERGE_PUSH：仅主窗口监听；sourceWindowId 防自吞。adopt 成败都回 ACK——
  // 发起方依赖回执决定是否移交/关窗。
  if (getWindowRole() === 'main') {
    on(HANDOFF_EVENTS.MERGE_PUSH, async event => {
      const payload = event?.payload;
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

  return async function dispose() {
    disposed = true;
    for (const fn of disposers) {
      try { await fn(); } catch { /* noop */ }
    }
    disposers.length = 0;
  };
}



