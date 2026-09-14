/**
 * terminalAppearance — 终端外观（主题/字号/行距）应用与持久化，从 sessions
 * store 按域拆出（第二刀）。字号/行距是窗口级用户偏好（localStorage），
 * 变更即热更新全部已建终端实例。绑定式 context（先例 terminalLifecycle）。
 */
import type { Ref } from 'vue';
import type { NotifyOptions } from '@/types/domain';
import { pickTerminalTheme } from '@/lib/terminalThemes';
import type { SessionEntry } from '@/lib/terminalTypes';

export interface TerminalAppearanceContext {
  sessions: Ref<SessionEntry[]>;
  terminalFontSize: Ref<number>;
  terminalLineHeight: Ref<number>;
  effectiveTheme(): unknown;
  announce(message: string, opts?: NotifyOptions): void;
}

// localStorage key（CRITICAL: do NOT rename — Critic 改进 3）
const TERMINAL_FONT_KEY = 'myshelltool-terminal-font';
const TERMINAL_LINEHEIGHT_KEY = 'myshelltool-terminal-lineheight';

export function clampFontSize(v: number): number {
  return Math.min(28, Math.max(9, v));
}

export function clampLineHeight(v: number): number {
  return Math.min(2, Math.max(1, Math.round(v * 10) / 10));
}

let boundCtx: TerminalAppearanceContext | null = null;

export function bindTerminalAppearanceContext(ctx: TerminalAppearanceContext): void {
  boundCtx = ctx;
}

function ac(): TerminalAppearanceContext {
  if (!boundCtx) throw new Error('terminalAppearance 未绑定 context（sessions store 未初始化）');
  return boundCtx;
}

// ============================================================
// Theme
// ============================================================
export function getTerminalTheme() {
  // 历史：bridge.effectiveTheme 运行时是已解包 string，沿用的 .value 读取恒为
  // undefined（pickTerminalTheme 忽略参数恒返回 darkTheme，终端主题与 app 主题
  // 已解耦）。双重 cast 仅为通过类型检查，运行时行为与原 JS 一致。
  return pickTerminalTheme((ac().effectiveTheme() as unknown as { value?: string }).value);
}

// 主题变更时同步 xterm theme（原 workbench.js:484-489）
export function updateAllTerminalThemes() {
  const next = getTerminalTheme();
  for (const session of ac().sessions.value) {
    try { session.term.options.theme = next; } catch { /* 已销毁实例改 theme 抛错，主题由下次重建生效 */ }
  }
}

// ============================================================
// Font size（原 workbench.js:492-516）
// ============================================================
export function applyTerminalFontSizeAll() {
  for (const session of ac().sessions.value) {
    try {
      session.term.options.fontSize = ac().terminalFontSize.value;
      if (session.term.rows > 0) session.term.refresh(0, session.term.rows - 1);
      session.fit.fit();
    } catch { /* 字号已写入 options，fit 失败只影响当前帧测量 */ }
  }
}

export function setTerminalFontSize(delta: number) {
  const next = Math.min(28, Math.max(9, ac().terminalFontSize.value + delta));
  if (next === ac().terminalFontSize.value) return;
  ac().terminalFontSize.value = next;
  localStorage.setItem(TERMINAL_FONT_KEY, String(next));
  applyTerminalFontSizeAll();
  ac().announce('终端字号：' + next + 'px');
}

export function resetTerminalFontSize() {
  if (ac().terminalFontSize.value === 12) return;
  ac().terminalFontSize.value = 12;
  localStorage.setItem(TERMINAL_FONT_KEY, '12');
  applyTerminalFontSizeAll();
  ac().announce('终端字号已重置：12px');
}

// ============================================================
// Line height（设置面板可调，改变即热更新所有终端并持久化）
// ============================================================
export function applyTerminalLineHeightAll() {
  for (const session of ac().sessions.value) {
    try {
      session.term.options.lineHeight = ac().terminalLineHeight.value;
      if (session.term.rows > 0) session.term.refresh(0, session.term.rows - 1);
      session.fit.fit();
    } catch { /* 字号已写入 options，fit 失败只影响当前帧测量 */ }
  }
}

export function setTerminalLineHeight(value: number) {
  const next = clampLineHeight(Number(value));
  if (!Number.isFinite(next) || next === ac().terminalLineHeight.value) return;
  ac().terminalLineHeight.value = next;
  localStorage.setItem(TERMINAL_LINEHEIGHT_KEY, String(next));
  applyTerminalLineHeightAll();
  ac().announce('终端行间距：' + next.toFixed(1));
}

export function initTerminalFontSize(): number {
  const stored = Number(readStored(TERMINAL_FONT_KEY));
  return Number.isFinite(stored) && stored > 0 ? clampFontSize(stored) : 12;
}

export function initTerminalLineHeight(): number {
  const stored = Number(readStored(TERMINAL_LINEHEIGHT_KEY));
  return Number.isFinite(stored) && stored >= 1 ? clampLineHeight(stored) : 1;
}

function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
