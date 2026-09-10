// withGlobalTauri（tauri.conf.json）注入的 window.__TAURI__ 全局对象的最小类型声明。
// 覆盖 src/services/backend.ts 与 src/composables/useClipboard.js 的全部实际用法；
// 刻意保持宽松（unknown / 可选成员）：强类型由 backend.ts 包装层承担，这里只画边界。
// 字段对应 Tauri 2 的 @tauri-apps/api 子命名空间：core / event / window / webviewWindow。

/** Tauri event.listen 回调收到的事件对象（调用方普遍只读 e.payload）。 */
interface TauriEvent<Payload = unknown> {
  event: string;
  id: number;
  payload: Payload;
}

type TauriEventHandler = (event: TauriEvent) => void;

/** event.listen 的返回值：解绑函数（UnlistenFn）。 */
type TauriUnlistenFn = () => void;

interface TauriEventNamespace {
  listen?: (event: string, handler: TauriEventHandler) => Promise<TauriUnlistenFn>;
  emit?: (event: string, payload?: unknown) => Promise<void>;
}

/** getTauriWindow 返回的窗口对象并集：成员全部可选，匹配 backend.ts 的 typeof 防御式调用。 */
interface TauriWindowLike {
  listen?: (event: string, handler: TauriEventHandler) => Promise<TauriUnlistenFn>;
  minimize?: () => Promise<void>;
  toggleMaximize?: () => Promise<void>;
  close?: () => Promise<void>;
  isMaximized?: () => Promise<boolean>;
  startDragging?: () => Promise<void>;
}

/** WebviewWindow 实例（createTauriWebviewWindow / getExistingTauriWebviewWindow 的返回类型）。 */
interface TauriWebviewWindow extends TauriWindowLike {
  minimize: () => Promise<void>;
  toggleMaximize: () => Promise<void>;
  close: () => Promise<void>;
  isMaximized: () => Promise<boolean>;
  startDragging: () => Promise<void>;
}

/** WebviewWindow 构造函数 + 静态方法（注意 getByLabel 是 async 的）。 */
interface TauriWebviewWindowConstructor {
  new (label: string, options?: Record<string, unknown>): TauriWebviewWindow;
  getByLabel(label: string): Promise<TauriWebviewWindow | null>;
}

interface TauriCoreNamespace {
  invoke?: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
  getCurrentWindow?: () => TauriWindowLike;
}

interface TauriWindowNamespace {
  getCurrentWindow?: () => TauriWindowLike;
}

interface TauriWebviewWindowNamespace {
  getCurrentWebviewWindow?: () => TauriWindowLike;
  WebviewWindow?: TauriWebviewWindowConstructor;
}

interface TauriGlobal {
  core?: TauriCoreNamespace;
  event?: TauriEventNamespace;
  window?: TauriWindowNamespace;
  webviewWindow?: TauriWebviewWindowNamespace;
}

interface Window {
  /** Tauri 2 withGlobalTauri 注入（tauri.conf.json 开启时才存在）。 */
  __TAURI__?: TauriGlobal;
  /** Tauri 2 webview 内部注入（useClipboard.js 仅探测其存在性，不触内部成员）。 */
  __TAURI_INTERNALS__?: unknown;
}
