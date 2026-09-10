// 危险命令检测：粘贴/写入时拦截，由用户二次确认。
// 宁可误报（用户可在弹窗里选择「仍然粘贴」），不可漏报。

/** 命中危险模式时的返回：pattern 为正则 source，sample 为文本前缀样本。 */
export interface DangerousCommandMatch {
  pattern: string;
  sample: string;
}

const DANGEROUS_PATTERNS: RegExp[] = [
  /rm\s+(-[a-zA-Z]*r[a-zA-Z]*f|--recursive\b.*--force\b)/i,
  /\bmkfs\b/i,
  /\bdd\b[^|]*\bof=\/dev\//i,
  /:\s*\(\)\s*\{[^}]*:\|:\s*&\s*\}\s*;/,
  />\s*\/dev\/sd[a-z]/i,
  /\bshutdown\b/i,
  /\breboot\b/i,
  /\bhalt\b/i,
  /\bpoweroff\b/i,
  /\binit\s+0\b/i,
  /\bchmod\s+-R\s+[0-7]{3,4}\s+\/(?!tmp|var\/tmp|home|Users)/i,
  /\bchown\s+-R\b/i,
  /\biptables\s+-F\b/i,
  /\b:()\{\s*:\|:&\s*\};:/,
  /\bcurl\b[^|]*\|\s*(bash|sh|zsh)\b/i,
  /\bwget\b[^|]*\|\s*(bash|sh|zsh)\b/i
];

export function detectDangerousCommand(text: unknown): DangerousCommandMatch | null {
  if (!text || typeof text !== 'string') return null;
  if (text.length < 4) return null;
  for (const pattern of DANGEROUS_PATTERNS) {
    if (pattern.test(text)) {
      return { pattern: pattern.source, sample: text.slice(0, 80) };
    }
  }
  return null;
}
