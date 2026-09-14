/**
 * terminalLifecycle — 终端实例生命周期（xterm 创建/挂载/回放/可见性/尺寸观察），
 * 从 sessions store 按域拆出（architecture-log「sessions 终端生命周期拆分」预案）。
 *
 * 职责：xterm + addons 工厂（createTerminalForAsset，连接与跨窗口接管共用）、
 * 容器/模块的懒加载与单例缓存（本窗口全局）、终端 DIV 可见性切换、
 * resize observer 防抖、adoptSession 的 scrollback 分块回放。
 * SSH 连接与会话状态管理留在 sessions store（流接线见 terminalStream）。
 *
 * context 传参（先例 lib/modalSubmit）：store 在 setup 组装一次 ctx，
 * 成员为 refs 或回调，取值恒新。
 */
import { markRaw, reactive, type Ref } from 'vue';
import type { NotifyOptions, NormalizedConnectionAsset } from '@/types/domain';
import { buildTerminalOptions } from '@/composables/useTerminalConfig';
import { useAutoReconnect } from '@/composables/useAutoReconnect';
import { composeKeyHandlers, createGlobalSearchHotkeyRelease, createNativePasteGuard } from '@/lib/terminalGuards';
import { isAssetWindowMode } from '@/lib/assetWindows';
import { invokeBackend } from '@/services/backend';
import { errorMessage } from '@/lib/errorMessage';
import type { AdoptSessionPayload, SessionEntry, SessionOverrides, TerminalModules } from '@/lib/terminalTypes';

export interface TerminalLifecycleContext {
  sessions: Ref<SessionEntry[]>;
  activeSessionId: Ref<string | null>;
  terminalFontSize: Ref<number>;
  terminalLineHeight: Ref<number>;
  /** 当前主题名（store 的 effectiveTheme 包装 bridge） */
  effectiveTheme(): unknown;
  announce(message: string, opts?: NotifyOptions): void;
  /** 危险粘贴守卫入口（store 的 pasteGuard 单一入口，勿绕过） */
  requestDangerousPaste(sessionId: string, text: string): void;
  setActiveSession(sessionId: string): void;
  /** adopt 接管后重建输出流监听（terminalStream.registerSessionStream 的包装） */
  registerSessionStream(session: SessionEntry): Promise<void>;
  onSessionConnected(assetId?: string | null): void;
  setTab(tab: string): void;
  /** 全量资产（adopt 时按 id 重解析最新资产） */
  getAssets(): NormalizedConnectionAsset[] | null;
}

// —— 本窗口全局单例：终端容器 div、动态加载的 xterm 模块集、store 绑定的 ctx ——
let terminalContainer: HTMLDivElement | null = null;
let terminalModules: TerminalModules | null = null;
let boundCtx: TerminalLifecycleContext | null = null;

/**
 * 绑定 store 上下文（sessions store setup 时调用一次，先于任何终端操作）。
 * 绑定式而非逐函数传 ctx：导出函数保持拆分前的原签名（store 的 return 块与
 * 内部调用点零改动）；终端表面本就是窗口级单例，模块级绑定与之一致。
 */
export function bindTerminalLifecycleContext(ctx: TerminalLifecycleContext): void {
  boundCtx = ctx;
}

function lc(): TerminalLifecycleContext {
  if (!boundCtx) throw new Error('terminalLifecycle 未绑定 context（sessions store 未初始化）');
  return boundCtx;
}

/** 容器是否已挂载（connectSelected 的前置校验；状态私有，经访问器读）。 */
export function hasTerminalContainer(): boolean {
  return terminalContainer !== null;
}

export function setTerminalContainer(element: HTMLDivElement | null) {
  const ctx = lc();
  terminalContainer = element;
  showOnlyActiveTerminal();
}

export async function ensureTerminalModules(): Promise<TerminalModules> {
  if (terminalModules) return terminalModules;
  const [{ Terminal }, { FitAddon }, { SearchAddon }, { SerializeAddon }] = await Promise.all([
    import('@xterm/xterm'),
    import('@xterm/addon-fit'),
    import('@xterm/addon-search'),
    import('@xterm/addon-serialize')
  ]);
  terminalModules = { Terminal, FitAddon, SearchAddon, SerializeAddon };
  return terminalModules;
}

export function showOnlyActiveTerminal() {
  const ctx = lc();
  for (const session of ctx.sessions.value) {
    const isActive = session.sessionId === ctx.activeSessionId.value;
    session.termDiv.style.display = isActive ? '' : 'none';
    if (isActive) {
      // 双 rAF 替代原 setTimeout(20ms)：第一帧应用 display:'' 到布局，
      // 第二帧元素已有真实尺寸，fit() 才能测准（20ms 定时器测到中间态）。
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          // xterm fit()/focus() 在元素不可见或已 dispose 时抛错，属可忽略：
          // 这里只做「布局稳定后测量」，抛错时下一帧/下一次 resize 会再测。
          try { session.fit.fit(); } catch { /* 见上：测量失败无副作用，后续 resize 会重测 */ }
          try { session.term.focus(); } catch { /* 同上：不可见时 focus 失败无副作用 */ }
        });
      });
    }
  }
}

export function attachResizeObserver(session: SessionEntry) {
  const ctx = lc(); // 保留统一取法（当前实现未用 ctx，签名语义不因拆分漂移）
  void ctx;
  if (typeof ResizeObserver === 'undefined') return;
  // 关键：不要在 RO 回调里裸调 fit()（xterm 重绘 → 容器子像素抖动 → RO 再触发
  // → fit → onResize → ssh_resize → 服务器重绘 → UI 乱跳的循环）。用 rAF 合并
  // 同一帧回调 + cols/rows 去重：尺寸没变就不重新 fit/通知后端。
  let rafId = 0;
  let lastCols = -1;
  let lastRows = -1;
  const runFit = () => {
    rafId = 0;
    try {
      session.fit.fit();
    } catch { /* 元素不可见时 fit() 抛错，尺寸保持上次值即可，不阻断后续 resize 逻辑 */ }
    // 仅当 xterm 实际尺寸变化时才让 onResize 链路生效（见下方 onResize 守卫，
    // 这里只负责稳定测量，不再直接触发 ssh_resize）。
    const c = session.term.cols;
    const r = session.term.rows;
    if (c === lastCols && r === lastRows) return;
    lastCols = c;
    lastRows = r;
  };
  const observer = new ResizeObserver(() => {
    if (rafId) return;          // 本帧已排队，合并掉
    rafId = requestAnimationFrame(runFit);
  });
  observer.observe(session.termDiv);
  session.resizeObserver = observer;
}

// 创建终端 + session 的共用工厂（connectSelected 新建连接与 adoptSession 跨窗口
// 接管共用，消除两份实现漂移风险）：termDiv + Terminal + addons（fit/search/
// serialize/webLinks/webgl）+ 危险粘贴守卫 + resize observer + autoReconnect 实例 +
// 搜索结果订阅，最后挂入 sessions 并激活。overrides 覆盖 session 初始字段（adopt
// 路径传真实 sessionId / status='connected' / oscTitle）。
export async function createTerminalForAsset(asset: NormalizedConnectionAsset, overrides: SessionOverrides = {}): Promise<SessionEntry> {
  const ctx = lc();
  // 调用契约：connectSelected / adoptSession 均已 await ensureTerminalModules() 且
  // terminalContainer 非空（非空断言只为通过类型检查，行为与原 JS 一致）
  const mods = terminalModules!;
  const termDiv = document.createElement('div');
  // 关键修复：termDiv 一开始就可见、占满容器。绝不能在 display:none 上调
  // term.open()（WebGL canvas 会 0×0 初始化，fit 后也不重建，终端空白）。
  termDiv.style.cssText = 'display:block;width:100%;height:100%;';
  terminalContainer!.appendChild(termDiv);

  const term = markRaw(new mods.Terminal(buildTerminalOptions({
    // overrides 可携带跨窗口迁移的样式快照（adoptSession）；缺省读本窗口设置
    fontSize: overrides.fontSize ?? ctx.terminalFontSize.value,
    lineHeight: overrides.lineHeight ?? ctx.terminalLineHeight.value,
    // 历史：ctx.effectiveTheme().value 恒 undefined（bridge 返回已解包 string），
    // pickTerminalTheme 忽略参数。cast 仅为通过类型检查（见 getTerminalTheme）。
    themeMode: overrides.themeMode || (ctx.effectiveTheme() as unknown as { value?: string }).value
  })));
  const fit = markRaw(new mods.FitAddon());
  const search = markRaw(new mods.SearchAddon());
  // serialize：跨窗口迁移导出带 SGR 颜色/样式的 scrollback（sessionHandoff 消费）
  const serialize = markRaw(new mods.SerializeAddon());
  term.loadAddon(fit);
  term.loadAddon(search);
  term.loadAddon(serialize);
  // 可选 addon：URL 可点击。失败静默回退
  try {
    const { WebLinksAddon } = await import('@xterm/addon-web-links');
    term.loadAddon(markRaw(new WebLinksAddon()));
  } catch (_) { /* addon 不可用时降级 */ }
  // 在已可见的 termDiv 上 open（元素此刻有真实尺寸）。
  term.open(termDiv);
  // WebGL addon 在 open 之后加载：此时 termDiv 已有真实尺寸，WebGL canvas
  // 会按正确尺寸初始化，避免 0×0 损坏。降级时提示用户（info 级 toast）。
  try {
    const { WebglAddon } = await import('@xterm/addon-webgl');
    const webgl = markRaw(new WebglAddon());
    webgl.onContextLoss(() => {
      try { webgl.dispose(); } catch (_) { /* noop */ }
      ctx.announce('显卡渲染不可用，已降级为软件渲染', { level: 'info' });
    });
    term.loadAddon(webgl);
  } catch (_) {
    ctx.announce('显卡渲染不可用，已降级为软件渲染', { level: 'info' });
  }
  // 等一帧让浏览器完成布局后立即 fit，拿到准确的 cols/rows。
  await new Promise(resolve => requestAnimationFrame(resolve));
  try { fit.fit(); } catch { /* 首帧不可见时 fit 抛错，后续 resize 事件会重测 */ }

  // 用 reactive() 包裹 session 对象：创建方后续会通过本地 session 变量多次
  // 修改其属性（status / sessionId / oscTitle / unlisten 等）。若 push 的是普通
  // 对象，本地引用指向【原始对象】，对其属性的赋值不经过代理 set trap，不触发
  // 响应式更新——这曾导致终端区域 status 永远停在 'connecting'（侧栏圆点靠
  // computed 重算碰巧更新，但终端组件的细粒度依赖收不到通知）。
  // reactive() 让本地 session 引用本身成为代理，所有属性变更都可靠触发更新。
  // term/fit/search 已 markRaw，reactive 不会再深代理它们。
  const session = reactive<SessionEntry>({
    sessionId: 'pending-' + asset.id + '-' + Date.now(),
    asset,
    term,
    fit,
    search,
    serialize,
    termDiv,
    unlisten: null,
    resizeObserver: null,
    status: 'connecting',
    oscTitle: '',
    connectError: null,
    manualDisconnect: false,
    reconnectAttempt: 0,
    reconnectTotal: 0,
    searchOpts: reactive({ caseSensitive: false, regex: false, wholeWord: false }),
    searchMatch: { index: 0, total: 0 },
    allowedPastePatterns: new Set<string>(),
    // 原写法 { stream: true }：stream 并非 TextDecoder 构造选项（WebIDL 忽略
    // 未知键，流式语义实际在 decode() 的第二参数），类型化后显式标注保留原样。
    decoder: new TextDecoder('utf-8', { stream: true } as unknown as TextDecoderOptions),
    ...overrides
  });
  // 每个 session 一个自动重连实例（退避 1s/2s/5s/15s 共 4 次）。
  // v2.6：计数不再「连接一成功就清零」，改由 markConnected() 启动稳定窗口——
  // 见 useAutoReconnect 头部注释（建连即断的服务器曾导致无限重连风暴）。
  session.autoReconnect = useAutoReconnect({
    onAttempt: ({ attempt, total, delay }) => {
      session.reconnectAttempt = attempt;
      session.reconnectTotal = total;
      ctx.announce(`连接断开，${Math.round(delay / 1000)} 秒后重连 (${attempt}/${total})`, { level: 'warn' });
    },
    onExhausted: () => {
      session.status = 'error';
      session.connectError = '自动重连失败，请手动重连';
      ctx.announce('自动重连失败：' + session.asset.name + '，请手动重连', { level: 'error' });
    }
  });
  // 终端按键守卫链（attachCustomKeyEventHandler 只能挂一个，经 composeKeyHandlers
  // 组合；connectSelected / adopt 两条创建路径都走本函数，天然全覆盖）：
  // ① Ctrl+K 放行——焦点在终端时 xterm 会把它消费成 VT kill-line（\x0b）且事件
  //    不冒泡，App.vue 的全局搜索快捷键因此失效（实测复现）；主窗口放行给全局
  //    监听，资产独立窗口不注册全局搜索、保留终端 kill-line 原生行为。
  // ② 原生 Ctrl+V 粘贴守卫（安全红线）。闭包经 getSessionId 取 session 当前
  //    sessionId（重连期间 sessionId 会切换，不能写死初始值）。
  term.attachCustomKeyEventHandler(composeKeyHandlers(
    createGlobalSearchHotkeyRelease({ enabled: !isAssetWindowMode() }),
    createNativePasteGuard({
      getSessionId: () => session.sessionId,
      requestDangerousPaste: ctx.requestDangerousPaste,
      announce: ctx.announce
    })
  ));
  // 搜索计数订阅：findNext/findPrevious 结果变化时更新 session.searchMatch
  session.searchResultsDisposable = search.onDidChangeResults(({ resultIndex, resultCount }) => {
    session.searchMatch = { index: resultIndex, total: resultCount };
  });

  ctx.sessions.value.push(session);
  ctx.activeSessionId.value = session.sessionId;
  showOnlyActiveTerminal();

  // 输入/尺寸/观察器与连接状态无关，创建即挂；重连复用同一 session 无需重挂
  term.onData(data => {
    const encoder = new TextEncoder();
    // catch null：pending- 占位 id 期间（重连中）键入会失败，静默丢弃
    invokeBackend('ssh_write', { sessionId: session.sessionId, data: Array.from(encoder.encode(data)) }).catch(() => null);
  });
  term.onResize(({ cols, rows }) => {
    // 守卫：fit() 在元素 0 尺寸（隐藏/未布局）时会算出 0 或异常值，
    // 不能把 0×0 PTY 发给服务器（会导致服务器重置光标、整屏重绘 → UI 跳动）。
    // 与连接处的守卫（见 attachSessionStream 的 cols/rows 计算）保持一致。
    if (!cols || !rows || cols <= 0 || rows <= 0) return;
    invokeBackend('ssh_resize', { sessionId: session.sessionId, cols, rows }).catch(() => null);
  });
  attachResizeObserver(session);
  return session;
}

// 跨窗口会话接管（sessionHandoff 迁移协议的目标侧）：以真实 sessionId 重建终端
// 并回放 scrollback，不重连（Rust 侧 SSH 连接从未断开，直接复用）。
// 注意：不调 injectShellCwdIntegration——远端 shell 存活、PROMPT_COMMAND 仍在
// （注入只发生在最初连接时），无新 shell 可注入。
export async function adoptSession({ sessionId, assetId, scrollback, oscTitle, style }: AdoptSessionPayload) {
  const ctx = lc();
  if (!sessionId) return false;
  // 幂等守卫：已接管（或本就是本窗口会话）直接视为成功
  if (ctx.sessions.value.some(item => item.sessionId === sessionId)) return true;
  // 资产重解析（与 attachSessionStream 同源逻辑）：迁移期间资产可能被编辑
  const latestAssets = ctx.getAssets();
  const asset = Array.isArray(latestAssets)
    ? latestAssets.find(item => item && item.id === assetId)
    : null;
  if (!asset) {
    // 跨窗口迁移诊断关键日志：asset 列表未加载完 / bridge 未注入都能在此分辨
    console.warn('[sessions] adoptSession 失败：资产未找到', {
      assetId,
      bridgeAttached: ctx.getAssets() !== null,
      assetCount: latestAssets?.length ?? null
    });
    return false;
  }
  try {
    await ensureTerminalModules();
  } catch (error) {
    ctx.announce('终端模块加载失败：' + errorMessage(error));
    return false;
  }
  if (!terminalContainer) {
    console.warn('[sessions] adoptSession 失败：终端容器未就绪（TerminalPane 未挂载）');
    return false;
  }

  // 样式快照（v2.4）：每个窗口独立 webview + 独立 localStorage 缓存（跨窗口
  // 非实时），目标窗口全局设置可能陈旧。快照【只作用于被迁移的终端实例】
  // （createTerminalForAsset overrides），保证会话视觉与源窗口一致；
  // 绝不回写目标窗口的全局字号/行距/主题——那是窗口级用户偏好，迁移不应改写。
  const styleSnapshot = style && typeof style === 'object' ? style : null;

  const session = await createTerminalForAsset(asset, {
    sessionId,
    status: 'connected',
    oscTitle: oscTitle || '',
    fontSize: styleSnapshot?.fontSize,
    lineHeight: styleSnapshot?.lineHeight
    // themeMode 不传：pickTerminalTheme 恒返回 darkTheme，终端主题与 app 主题解耦
  });
  // scrollback 回放：分块 write（每 100 行间隔一帧 rAF），防大文本一次性写入卡顿
  const text = String(scrollback || '');
  if (text) {
    const lines = text.split('\r\n');
    for (let i = 0; i < lines.length; i += 100) {
      if (i > 0) await new Promise(resolve => requestAnimationFrame(resolve));
      const end = Math.min(i + 100, lines.length);
      session.term.write(lines.slice(i, end).join('\r\n') + (end < lines.length ? '\r\n' : ''));
    }
  }
  // 重建 ssh-output/closed 事件接线（闭包私有，直接调）
  await ctx.registerSessionStream(session);
  ctx.setActiveSession(sessionId);
  // bridge.onSessionConnected 必须在 setActiveSession 之后：files handler 只对
  // 面板当前 selectedAsset 生效，setActiveSession 先经 syncAssetSelection 切过去
  try {
    ctx.onSessionConnected(asset.id);
  } catch (_) { /* noop */ }
  // merge 回主窗口后切到 terminal tab，让用户立即看到回迁的终端
  try {
    ctx.setTab('terminal');
  } catch (_) { /* noop */ }
  return true;
}
