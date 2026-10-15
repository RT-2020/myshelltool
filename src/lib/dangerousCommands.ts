// 危险命令检测：粘贴/写入时拦截，由用户二次确认。
// 宁可误报（用户可在弹窗里选择「仍然粘贴」），不可漏报。
//
// 权威判定在后端（checkDangerousPaste → dangerous_paste_check 命令）：
// core 的「黑名单 ∪ catastrophic 层」并集。本文件的正则与 core 黑名单
// pattern 1 同源同漏——`rm -fr /`、`rm -r -f /`、`--force --recursive`
// 等分离/逆序形态全 MISS（core 的 rm_root 正则才补上），故降级为 IPC
// 不可用时的兜底启发式，不再承担权威语义。
import { invokeBackend } from '@/services/backend';

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
  // 仅 /tmp、/var/tmp 视为「草稿区」；家目录（/home、/Users）与系统目录递归 chmod
  // 一律告警（与 Rust 侧 dangerous_commands.rs 的 catastrophic 豁免表保持同一口径，
  // 两端共享同一判定语义——「家目录安全」是环境臆断，已废弃）。
  /\bchmod\s+-R\s+[0-7]{3,4}\s+\/(?!tmp|var\/tmp)/i,
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

/**
 * 权威判定不可用时的保守折叠哨兵（AGENTS 安全级红线：拦截类读失败只许
 * 误拦）。守卫把它折叠为空 matchedPattern——确认弹窗不显示「命中规则」，
 * 且空串不会被 approve 记进本会话放行表（防止一次放行开后门）。
 */
export const UNVERIFIED_PASTE_PATTERN = '__guard:unverified__';

/**
 * 粘贴文本的权威危险判定（异步，经后端 core 分类器，两条粘贴入口共用）。
 * 后端不可用时失败保守折叠：先回落本地正则（保住「命中规则」展示质量），
 * 正则也未命中则返回哨兵 match 强制弹确认——任何失败路径都朝误拦方向。
 */
export async function checkDangerousPaste(text: string): Promise<DangerousCommandMatch | null> {
  if (!text || text.length < 4) return null;
  try {
    return await invokeBackend<DangerousCommandMatch | null>('dangerous_paste_check', { text });
  } catch (error) {
    console.warn('[dangerousCommands] 后端权威判定不可用，保守折叠到确认弹窗：', error);
    return detectDangerousCommand(text) ?? { pattern: UNVERIFIED_PASTE_PATTERN, sample: text.slice(0, 80) };
  }
}
