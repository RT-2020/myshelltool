/**
 * terminalTypes — 终端会话条目与生命周期模块的共享类型
 * （从 sessions store 抽出，拆分第一刀：store 与 lib/terminalLifecycle、
 * lib/terminalStream 三方共用）。
 */
import type { Terminal } from '@xterm/xterm';
import type { FitAddon } from '@xterm/addon-fit';
import type { SearchAddon } from '@xterm/addon-search';
import type { SerializeAddon } from '@xterm/addon-serialize';
import type { NormalizedConnectionAsset } from '@/types/domain';

/** 动态加载的 xterm 模块集（ensureTerminalModules 缓存，类型取自各包 d.ts）。 */
export interface TerminalModules {
  Terminal: typeof import('@xterm/xterm').Terminal;
  FitAddon: typeof import('@xterm/addon-fit').FitAddon;
  SearchAddon: typeof import('@xterm/addon-search').SearchAddon;
  SerializeAddon: typeof import('@xterm/addon-serialize').SerializeAddon;
}

/** 每个 session 一个的自动重连控制器（useAutoReconnect 返回值的最小结构，不耦合其完整类型）。 */
export interface AutoReconnectController {
  schedule(reconnectFn: (attempt: number) => void): void;
  cancel(): void;
  reset(): void;
  /**
   * 连接/重连成功后调用：启动「稳定窗口」，窗口内不再断开才把重连计数归零
   * （v2.6：`ssh_connect` 成功 ≠ 会话能维持，建连即断的服务器曾导致无限重连）。
   */
  markConnected(): number;
}

/**
 * 会话条目（sessions 数组元素）。term/fit/search/serialize 经 markRaw，
 * reactive 只代理普通字段。fontSize/lineHeight/themeMode 是 adopt 路径的
 * 样式快照覆盖（createTerminalForAsset overrides 展开进来的运行时字段）。
 */
export interface SessionEntry {
  sessionId: string;
  asset: NormalizedConnectionAsset;
  term: Terminal;
  fit: FitAddon;
  search: SearchAddon;
  serialize: SerializeAddon;
  termDiv: HTMLDivElement;
  unlisten: (() => Promise<void>) | null;
  resizeObserver: ResizeObserver | null;
  status: string;
  oscTitle: string;
  connectError: string | null;
  manualDisconnect: boolean;
  reconnectAttempt: number;
  reconnectTotal: number;
  searchOpts: { caseSensitive: boolean; regex: boolean; wholeWord: boolean };
  searchMatch: { index: number; total: number };
  allowedPastePatterns: Set<string>;
  decoder: TextDecoder;
  autoReconnect?: AutoReconnectController | null;
  searchResultsDisposable?: { dispose(): void } | null;
  fontSize?: number;
  lineHeight?: number;
  themeMode?: string;
}

/** createTerminalForAsset 的 overrides（adoptSession 跨窗口迁移传真实 sessionId/样式快照）。 */
export interface SessionOverrides {
  sessionId?: string;
  status?: string;
  oscTitle?: string;
  fontSize?: number;
  lineHeight?: number;
  themeMode?: string;
}

/** adoptSession 入参（sessionHandoff 迁移协议的目标侧 payload）。 */
export interface AdoptSessionPayload {
  sessionId?: string | null;
  assetId?: string | null;
  scrollback?: string | null;
  oscTitle?: string | null;
  style?: { fontSize?: number; lineHeight?: number } | null;
}

/** OSC 序列解析器（createOscParser 的返回契约：feed 拆 OSC 0/1/2 标题 + OSC 7 cwd）。 */
export interface OscParser {
  feed(text: string): void;
}
