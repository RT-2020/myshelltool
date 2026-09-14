/**
 * createOscParser — OSC 转义序列解析（从 sessions store 抽出，纯函数）。
 * OSC 0/1/2 标题序列（更新 Tab 标签）+ OSC 7（cwd 上报，文件面板跟随终端目录）。
 */
import type { OscParser } from '@/lib/terminalTypes';

export function createOscParser(onTitle: (title: string) => void, onCwd?: (cwd: string) => void): OscParser {
  let buffer = '';
  // OSC 7 payload 形如 file://host/path 或 file:///path（路径可能带 %XX 编码）
  function cwdFromPayload(payload?: string): string | null {
    let raw = String(payload || '');
    if (raw.startsWith('file://')) {
      raw = raw.slice(7);
      const slash = raw.indexOf('/');
      if (slash < 0) return null;
      raw = raw.slice(slash);
    }
    if (!raw.startsWith('/')) return null;
    try {
      return decodeURIComponent(raw);
    } catch {
      return raw;
    }
  }
  return {
    feed(text) {
      // 拆分：寻找 OSC 序列起始 (\x1b])，匹配到 ST (\x07 或 \x1b\\)
      buffer += text;
      // 仅处理最近一段缓冲，避免无限增长
      if (buffer.length > 4096) buffer = buffer.slice(-4096);
      const regex = /\x1b\](\d+);([^\x07\x1b]*)\x07|\x1b\](\d+);([^\x07\x1b]*)\x1b\\/g;
      let match: RegExpExecArray | null;
      let lastIndex = 0;
      while ((match = regex.exec(buffer)) !== null) {
        const code = Number(match[1] || match[3]);
        const title = match[2] || match[4];
        if ((code === 0 || code === 1 || code === 2) && title) {
          onTitle(title.slice(0, 200));
        } else if (code === 7 && onCwd) {
          const cwd = cwdFromPayload(title);
          if (cwd) onCwd(cwd);
        }
        lastIndex = regex.lastIndex;
      }
      if (lastIndex > 0) {
        // 保留未消费的尾部，但丢弃开头非序列部分
        const lastEscape = buffer.lastIndexOf('\x1b]', lastIndex);
        buffer = lastEscape >= 0 && lastEscape < lastIndex ? buffer.slice(lastEscape) : '';
      }
    }
  };
}

// ============================================================
// Terminal mount / lifecycle
// ============================================================
