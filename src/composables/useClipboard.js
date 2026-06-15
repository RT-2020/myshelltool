// 剪贴板 fallback 链：navigator.clipboard → Tauri clipboard → execCommand textarea
// navigator.clipboard 在非 https / Tauri webview 下经常失败，必须有 fallback

export function useClipboard() {
  async function copy(text) {
    if (!text) return false;
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch (_) { /* fallthrough */ }
    if (window.__TAURI__?.clipboard?.writeText) {
      try {
        await window.__TAURI__.clipboard.writeText(text);
        return true;
      } catch (_) { /* fallthrough */ }
    }
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
    if (window.__TAURI__?.clipboard?.readText) {
      try {
        const text = await window.__TAURI__.clipboard.readText();
        if (text) return text;
      } catch (_) { /* fallthrough */ }
    }
    return '';
  }

  return { copy, paste };
}
