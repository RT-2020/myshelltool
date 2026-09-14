/**
 * terminalOps — 终端日常操作（内联搜索 / 剪贴板 / 危险粘贴守卫 / 写入 /
 * 工具栏动作分发），从 sessions store 按域拆出（第二刀）。粘贴守卫是
 * 安全红线单一入口（命令面板 / 右键菜单 / 原生 Ctrl+V 三路共用，状态机在
 * lib/terminalGuards）。绑定式 context（先例 terminalLifecycle）。
 */
import { reactive, type Ref } from 'vue';
import type { NotifyOptions, TerminalSearchState } from '@/types/domain';
import { invokeBackend } from '@/services/backend';
import { useClipboard } from '@/composables/useClipboard';
import { detectDangerousCommand } from '@/lib/dangerousCommands';
import { createDangerousPasteGuard } from '@/lib/terminalGuards';
import type { DangerousPastePrompt } from '@/lib/terminalGuards';
import type { SessionEntry } from '@/lib/terminalTypes';

export interface TerminalOpsContext {
  sessions: Ref<SessionEntry[]>;
  activeSessionId: Ref<string | null>;
  activeSession: Ref<SessionEntry | null>;
  terminalSearch: Ref<TerminalSearchState>;
  announce(message: string, opts?: NotifyOptions): void;
  connect(): void;
  reconnect(sessionId: string): void;
  setFontSize(delta: number): void;
  resetFontSize(): void;
}

let boundCtx: TerminalOpsContext | null = null;

export function bindTerminalOpsContext(ctx: TerminalOpsContext): void {
  boundCtx = ctx;
}

function oc(): TerminalOpsContext {
  if (!boundCtx) throw new Error('terminalOps 未绑定 context（sessions store 未初始化）');
  return boundCtx;
}

// ============================================================
// Inline search（原 workbench.js:519-552）
// ============================================================
export function openTerminalSearchInline() {
  const session = oc().activeSession.value;
  if (!session) {
    oc().announce('请先连接主机');
    return;
  }
  oc().terminalSearch.value = { open: true, query: oc().terminalSearch.value.query || '', direction: 'next', result: null };
}
export function closeTerminalSearchInline() {
  oc().terminalSearch.value = { ...oc().terminalSearch.value, open: false };
  const session = oc().activeSession.value;
  try { session?.search?.clearDecorations(); } catch (_) { /* noop */ }
}
export function setTerminalSearchQuery(query: string) {
  oc().terminalSearch.value = { ...oc().terminalSearch.value, query };
  if (!query) {
    const session = oc().activeSession.value;
    try { session?.search?.clearDecorations(); } catch (_) { /* noop */ }
  }
}
// per-session 搜索选项（searchOpts 在 session 创建时初始化）；合并而非整体替换
export function setTerminalSearchOpts(sessionId: string, opts?: Record<string, boolean>) {
  const session = oc().sessions.value.find(item => item.sessionId === sessionId);
  if (!session || !session.searchOpts) return;
  Object.assign(session.searchOpts, opts || {});
}
export async function findTerminalNext(direction = 'next') {
  const session = oc().activeSession.value;
  const query = oc().terminalSearch.value.query;
  if (!session || !query) return;
  const opts = {
    caseSensitive: session.searchOpts?.caseSensitive ?? false,
    wholeWord: session.searchOpts?.wholeWord ?? false,
    regex: session.searchOpts?.regex ?? false,
    // 装饰色必须是十六进制色值（@xterm/addon-search decorations 契约）
    decorations: { matchOverviewRuler: '#f5c518', activeMatchColorOverviewRuler: '#ef4444' }
  };
  try {
    const found = direction === 'prev'
      ? await session.search.findPrevious(query, opts)
      : await session.search.findNext(query, opts);
    oc().terminalSearch.value = { ...oc().terminalSearch.value, result: found ? 'hit' : 'miss' };
    if (!found) oc().announce('未找到匹配：' + query);
  } catch {
    oc().terminalSearch.value = { ...oc().terminalSearch.value, result: 'miss' };
  }
}

// ============================================================
// Clipboard / write
// ============================================================
const { copy: clipboardCopy, paste: clipboardPaste } = useClipboard();

export async function copyTerminalSelection(session: SessionEntry) {
  const selection = session.term.getSelection();
  if (!selection) {
    oc().announce('终端无选中内容');
    return;
  }
  const ok = await clipboardCopy(selection);
  if (ok) oc().announce('已复制终端选中文本');
  else oc().announce('剪贴板不可用');
}

export async function pasteToTerminal(session: SessionEntry) {
  const text = await clipboardPaste();
  if (!text) { oc().announce('剪贴板为空或不可用'); return; }
  // 统一走危险粘贴守卫：命中弹确认、未命中直接写入
  requestDangerousPaste(session.sessionId, text);
}

// ============================================================
// 危险粘贴守卫（单一入口：命令面板 / 右键菜单 / 原生 Ctrl+V；状态机在 terminalGuards）
// ============================================================
export const dangerousPastePrompt = reactive<DangerousPastePrompt>({ open: false, sessionId: null, command: '', matchedPattern: '' });
const pasteGuard = createDangerousPasteGuard({
  getSession: sessionId => oc().sessions.value.find(item => item.sessionId === sessionId) || null,
  prompt: dangerousPastePrompt,
  detect: detectDangerousCommand
});

export function requestDangerousPaste(sessionId: string, text: string) { return pasteGuard.request(sessionId, text); }
export function approveDangerousPaste(allowedPattern: string) { pasteGuard.approve(allowedPattern); }
export function cancelDangerousPaste() { pasteGuard.cancel(); }

export async function writeToActiveTerminal(text: string) {
  if (!text) return;
  const sessionId = oc().activeSessionId.value;
  if (!sessionId) { oc().announce('当前无活跃会话'); return; }
  const encoder = new TextEncoder();
  await invokeBackend('ssh_write', { sessionId, data: Array.from(encoder.encode(text)) });
}

// ============================================================
// Run terminal action dispatcher
// ============================================================
export function runTerminalAction(action: string) {
  if (action === 'connect') {
    oc().connect();
    return;
  }
  // 字号是全局设置，空态（无活跃会话）也放行
  if (action === 'font-inc') {
    oc().setFontSize(1);
    return;
  }
  if (action === 'font-dec') {
    oc().setFontSize(-1);
    return;
  }
  if (action === 'font-reset') {
    oc().resetFontSize();
    return;
  }
  const session = oc().activeSession.value;
  if (!session) {
    oc().announce('请先连接主机');
    return;
  }
  if (action === 'clear') {
    session.term.clear();
    oc().announce('终端已清屏');
    return;
  }
  if (action === 'search') {
    openTerminalSearchInline();
    return;
  }
  if (action === 'copy') {
    copyTerminalSelection(session);
    return;
  }
  if (action === 'paste') {
    pasteToTerminal(session);
    return;
  }
  if (action === 'reconnect') {
    oc().reconnect(session.sessionId);
    return;
  }
}
