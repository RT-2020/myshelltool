/**
 * handoffData — 会话迁移数据层（v0.20 自 sessionHandoff 拆出，S2 的一刀；
 * 逻辑原样迁移）：迁移协议类型 + 终端 scrollback 导出（serialize 优先/
 * 纯文本回退）+ Rust 内存中转读写（TTL 60s 由 Rust 侧管理，take 即原子删）。
 *
 * 依赖方向：sessionHandoff → 本模块（值 import），本模块不回引（无环）；
 * 外部调用方仍经 sessionHandoff 的 re-export 取符号（路径零改动）。
 */
import type { Terminal } from '@xterm/xterm';
import type { SerializeAddon } from '@xterm/addon-serialize';
import type { NormalizedConnectionAsset } from '@/types/domain';
import { invokeBackend } from '../services/backend';

export const SCROLLBACK_MAX_LINES = 2000;
/** 单条 handoff 的字符预算（防超大 scrollback 撑爆 IPC）。 */
export const MAX_JSON_CHARS = 1000000;

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
  // 本窗口的在途/排队传输（workbench.activeTransfers）。移交守据（见
  // sessionHandoff 的 pushSessionToMainWindow）：会话移走后本窗口即销毁，在途
  // 传输依赖的写入句柄还没跑完清理代码就连同 JS 上下文一起消失，句柄变成
  // 无人认领的孤儿。可选成员：asset 窗口外的调用方（如主窗口 tearoff 路径）
  // 传的 store 无需提供。
  activeTransfers?: unknown[];
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
