import type {
  NormalizedConnectionAsset,
  NormalizedTunnelConfig,
  NormalizedTunnelStatus
} from '@/types/domain';

export function isTauriRuntime(): boolean {
  return typeof getTauriInvoke() === 'function';
}

export async function invokeBackend<T = unknown>(
  command: string,
  args: Record<string, unknown> = {}
): Promise<T> {
  const tauriInvoke = getTauriInvoke();
  if (typeof tauriInvoke === 'function') {
    return tauriInvoke(command, args) as Promise<T>;
  }
  throw new Error(`Backend command "${command}" requires the Tauri desktop runtime. Run "npm run tauri:dev" instead of "npm run dev".`);
}

export async function listenBackendEvent(
  eventName: string,
  handler: TauriEventHandler
): Promise<TauriUnlistenFn> {
  const tauriEventListen = window.__TAURI__?.event?.listen;
  if (typeof tauriEventListen === 'function') {
    return tauriEventListen(eventName, handler);
  }

  const currentWindow = getTauriWindow();
  if (typeof currentWindow?.listen === 'function') {
    return currentWindow.listen(eventName, handler);
  }

  throw new Error(`Event "${eventName}" requires the Tauri desktop runtime.`);
}

// 广播事件到所有 webview 窗口与 Rust（Tauri 2 event.emit 全局广播）。
// 跨窗口会话迁移协议（sessionHandoff）用；非 Tauri runtime 抛错，与 invokeBackend 一致。
export async function emitBackendEvent(eventName: string, payload?: unknown): Promise<void> {
  const tauriEventEmit = window.__TAURI__?.event?.emit;
  if (typeof tauriEventEmit !== 'function') {
    throw new Error(`Emitting event "${eventName}" requires the Tauri desktop runtime.`);
  }
  return tauriEventEmit(eventName, payload);
}

// 打开系统文件选择对话框选择私钥文件（tauri-plugin-dialog 的 open 命令）。
// 返回所选文件的绝对路径字符串；用户取消时返回 null。
// 非 Tauri runtime（浏览器预览）下抛错，与 invokeBackend 行为一致。
export async function openPrivateKeyFileDialog(): Promise<string | null> {
  return invokeBackend<string | null>('plugin:dialog|open', {
    options: {
      title: '选择私钥文件',
      multiple: false,
      directory: false,
      filters: [
        { name: 'SSH 私钥', extensions: ['pem', 'key', 'openssh', 'id_rsa', 'id_ed25519', 'ppk'] },
        { name: '所有文件', extensions: ['*'] }
      ]
    }
  });
}

// 返回 Tauri 2 当前窗口对象（WebviewWindow 或 Window），无 runtime 时 null。
// 用于调 setFullscreen / maximize / minimize 等 OS 级窗口 API。
export function getTauriWindow(): TauriWindowLike | null {
  return window.__TAURI__?.webviewWindow?.getCurrentWebviewWindow?.()
    || window.__TAURI__?.window?.getCurrentWindow?.()
    || window.__TAURI__?.core?.getCurrentWindow?.()
    || null;
}

export async function minimizeTauriWindow(): Promise<boolean> {
  const currentWindow = getTauriWindow();
  if (typeof currentWindow?.minimize === 'function') {
    await currentWindow.minimize();
    return true;
  }
  return false;
}

export async function toggleTauriWindowMaximize(): Promise<boolean> {
  const currentWindow = getTauriWindow();
  if (typeof currentWindow?.toggleMaximize === 'function') {
    await currentWindow.toggleMaximize();
    return true;
  }
  return false;
}

export async function closeTauriWindow(): Promise<boolean> {
  const currentWindow = getTauriWindow();
  if (typeof currentWindow?.close === 'function') {
    await currentWindow.close();
    return true;
  }
  return false;
}

export async function isTauriWindowMaximized(): Promise<boolean> {
  const currentWindow = getTauriWindow();
  if (typeof currentWindow?.isMaximized === 'function') {
    return currentWindow.isMaximized();
  }
  return false;
}

export async function startTauriWindowDragging(): Promise<boolean> {
  const currentWindow = getTauriWindow();
  if (typeof currentWindow?.startDragging === 'function') {
    await currentWindow.startDragging();
    return true;
  }
  return false;
}

// 创建新的 Tauri WebviewWindow（独立资产窗口等）。非 Tauri runtime 抛错，与 invokeBackend 行为一致。
// 命名空间依据：withGlobalTauri 全局注入中 WebviewWindow 类在 __TAURI__.webviewWindow 子命名空间
// （__TAURI__.webview 下只有 Webview 类），与上方 getTauriWindow 的取法一致。
export async function createTauriWebviewWindow(
  label: string,
  options?: Record<string, unknown>
): Promise<TauriWebviewWindow> {
  const WebviewWindowClass = window.__TAURI__?.webviewWindow?.WebviewWindow;
  if (typeof WebviewWindowClass !== 'function') {
    throw new Error('Creating a webview window requires the Tauri desktop runtime.');
  }
  return new WebviewWindowClass(label, options);
}

// 按 label 查找已存在的 WebviewWindow。注意 Tauri 2 的 WebviewWindow.getByLabel 是
// **async** 的（内部 await getAllWebviewWindows 再按 label 匹配，返回 Promise<WebviewWindow|null>），
// 必须 await——直接当同步用拿到的是恒 truthy 的 Promise，调用其方法即 "not a function"。
// 非 Tauri runtime、窗口不存在或查询失败时返回 null。
export async function getExistingTauriWebviewWindow(label: string): Promise<TauriWebviewWindow | null> {
  // 同 createTauriWebviewWindow：WebviewWindow 类在 __TAURI__.webviewWindow 子命名空间
  const WebviewWindowClass = window.__TAURI__?.webviewWindow?.WebviewWindow;
  if (typeof WebviewWindowClass?.getByLabel !== 'function') return null;
  try {
    return (await WebviewWindowClass.getByLabel(label)) ?? null;
  } catch {
    return null;
  }
}

function getTauriInvoke() {
  return window.__TAURI__?.core?.invoke;
}

// normalize 系列的入参是 IPC 返回的原始 JSON（形状不可信），故用宽松 record
// 边界 + 逐字段防御（返回契约见 @/types/domain.ts）。TauriEventHandler / TauriWindowLike /
// TauriWebviewWindow 等全局接口来自 @/types/tauri.d.ts。
export function normalizeAsset(item?: Record<string, any>): NormalizedConnectionAsset {
  const tags = Array.isArray(item?.tags) ? item.tags : String(item?.tags || '').split(/[·,，\s]+/).filter(Boolean);
  return {
    id: String(item?.id || slugify(item?.name || item?.host || 'asset')),
    name: String(item?.name || '未命名连接'),
    host: String(item?.host || item?.address || ''),
    port: Number(item?.port) || 22,
    username: String(item?.username || item?.user || ''),
    auth_method: item?.auth_method || item?.authMethod || 'Password',
    private_key_path: item?.private_key_path || item?.privateKeyPath || null,
    group: String(item?.group || '未分组'),
    tags,
    status: item?.status || 'Idle',
    last_connected: String(item?.last_connected || item?.lastConnected || '从未'),
    credential_id: item?.credential_id || item?.credentialId || null,
    passphrase_credential_id: item?.passphrase_credential_id || item?.passphraseCredentialId || null,
    private_key_credential_id: item?.private_key_credential_id || item?.privateKeyCredentialId || null
  };
}

export function normalizeTunnelConfig(config: Record<string, any> = {}): NormalizedTunnelConfig {
  const id = String(config.id || 'tunnel-' + Date.now());
  const kind = ['local', 'remote', 'dynamic'].includes(config.kind) ? config.kind : 'local';
  return {
    id,
    name: String(config.name || id),
    kind,
    local_addr: String(config.local_addr || '127.0.0.1'),
    local_port: Number(config.local_port) || 0,
    remote_addr: kind === 'dynamic' ? '' : String(config.remote_addr || '127.0.0.1'),
    remote_port: kind === 'dynamic' ? 0 : Number(config.remote_port) || 0,
    session_id: String(config.session_id || ''),
    auto_start: Boolean(config.auto_start)
  };
}

export function normalizeTunnelStatus(tunnel: Record<string, any> = {}): NormalizedTunnelStatus {
  const config = normalizeTunnelConfig(tunnel.config || tunnel);
  return {
    id: String(tunnel.id || config.id),
    config,
    active: Boolean(tunnel.active),
    error: tunnel.error ? String(tunnel.error) : null
  };
}

export function slugify(value: unknown): string {
  const slug = String(value).trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
  return slug || 'asset';
}
