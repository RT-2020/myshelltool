export function isTauriRuntime() {
  return typeof getTauriInvoke() === 'function';
}

export async function invokeBackend(command, args = {}) {
  const tauriInvoke = getTauriInvoke();
  if (typeof tauriInvoke === 'function') {
    return tauriInvoke(command, args);
  }
  throw new Error(`Backend command "${command}" requires the Tauri desktop runtime. Run "npm run tauri:dev" instead of "npm run dev".`);
}

export async function listenBackendEvent(eventName, handler) {
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

// 返回 Tauri 2 当前窗口对象（WebviewWindow 或 Window），无 runtime 时 null。
// 用于调 setFullscreen / maximize / minimize 等 OS 级窗口 API。
export function getTauriWindow() {
  return window.__TAURI__?.webviewWindow?.getCurrentWebviewWindow?.()
    || window.__TAURI__?.window?.getCurrentWindow?.()
    || window.__TAURI__?.core?.getCurrentWindow?.()
    || null;
}

export async function minimizeTauriWindow() {
  const currentWindow = getTauriWindow();
  if (typeof currentWindow?.minimize === 'function') {
    await currentWindow.minimize();
    return true;
  }
  return false;
}

export async function toggleTauriWindowMaximize() {
  const currentWindow = getTauriWindow();
  if (typeof currentWindow?.toggleMaximize === 'function') {
    await currentWindow.toggleMaximize();
    return true;
  }
  return false;
}

export async function closeTauriWindow() {
  const currentWindow = getTauriWindow();
  if (typeof currentWindow?.close === 'function') {
    await currentWindow.close();
    return true;
  }
  return false;
}

export async function isTauriWindowMaximized() {
  const currentWindow = getTauriWindow();
  if (typeof currentWindow?.isMaximized === 'function') {
    return currentWindow.isMaximized();
  }
  return false;
}

export async function startTauriWindowDragging() {
  const currentWindow = getTauriWindow();
  if (typeof currentWindow?.startDragging === 'function') {
    await currentWindow.startDragging();
    return true;
  }
  return false;
}

function getTauriInvoke() {
  return window.__TAURI__?.core?.invoke;
}

export function normalizeAsset(item) {
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
    passphrase_credential_id: item?.passphrase_credential_id || item?.passphraseCredentialId || null
  };
}

export function normalizeTunnelConfig(config = {}) {
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

export function normalizeTunnelStatus(tunnel = {}) {
  const config = normalizeTunnelConfig(tunnel.config || tunnel);
  return {
    id: String(tunnel.id || config.id),
    config,
    active: Boolean(tunnel.active),
    error: tunnel.error ? String(tunnel.error) : null
  };
}

export function slugify(value) {
  const slug = String(value).trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
  return slug || 'asset';
}
