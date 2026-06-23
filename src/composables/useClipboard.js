// 剪贴板 fallback 链：navigator.clipboard → Tauri clipboard 插件 → execCommand textarea
// navigator.clipboard 在非 https / Tauri webview 非聚焦时经常失败，必须有 fallback。
// 第二层用 Tauri v2 官方插件 @tauri-apps/plugin-clipboard-manager（桌面端最可靠），
// 替代旧 v1 的 window.__TAURI__.clipboard 注入（v2 不再注入该对象，是死分支）。
// 动态 import + runtime 检测：浏览器预览模式（npm run dev）下不触发，避免 import 即崩。

// Tauri v2 runtime 探测：__TAURI_INTERNALS__ 由 withGlobalTauri 的 webview 注入。
function isTauriRuntime() {
  return Boolean(typeof window !== 'undefined' && window.__TAURI_INTERNALS__);
}

// 动态加载 Tauri clipboard 插件并写入文本。非 Tauri runtime 直接返回 false（fallthrough）。
async function tauriWriteText(text) {
  if (!isTauriRuntime()) return false;
  const { writeText } = await import('@tauri-apps/plugin-clipboard-manager');
  await writeText(text);
  return true;
}

async function tauriReadText() {
  if (!isTauriRuntime()) return null;
  const { readText } = await import('@tauri-apps/plugin-clipboard-manager');
  return await readText();
}

export function useClipboard() {
  async function copy(text) {
    if (!text) return false;
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch (_) { /* fallthrough */ }
    try {
      if (await tauriWriteText(text)) return true;
    } catch (_) { /* fallthrough */ }
    // execCommand fallback（同步 API）
    try {
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.style.position = 'fixed';
      ta.style.top = '0';
      ta.style.left = '0';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.focus();
      ta.select();
      const ok = document.execCommand('copy');
      ta.remove();
      return ok;
    } catch (_) {
      return false;
    }
  }

  async function paste() {
    try {
      const text = await navigator.clipboard.readText();
      if (text) return text;
    } catch (_) { /* fallthrough */ }
    try {
      const text = await tauriReadText();
      if (text) return text;
    } catch (_) { /* fallthrough */ }
    return '';
  }

  return { copy, paste };
}
