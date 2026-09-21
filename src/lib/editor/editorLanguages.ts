/**
 * editorLanguages — 编辑器的文本类型判定与 CodeMirror 语言装载。
 *
 * `isTextExtension` 是「双击分流 / 右键编辑项禁用态 / 拖拽打开判定」的
 * 单一事实源：命中名单 → 双击直接开编辑器；已知二进制扩展 → 菜单禁用并
 * 说明；未知扩展 → 允许按纯文本尝试（后端二进制嗅探兜底拒绝，如实报错）。
 *
 * 语言包一律 dynamic import（按需加载，不进主 bundle）。
 */
import type { Extension } from '@codemirror/state';

/** 已知文本扩展名（小写，含点）。需求清单 + 日志/配置常见项。 */
const TEXT_EXTENSIONS: ReadonlySet<string> = new Set([
  '.txt', '.json', '.md', '.markdown',
  '.yaml', '.yml', '.toml',
  '.ini', '.conf', '.cfg', '.properties', '.xml', '.env',
  '.log'
]);

/** 已知二进制扩展名（小写，含点）——右键「编辑」禁用并说明的原因。 */
const BINARY_EXTENSIONS: ReadonlySet<string> = new Set([
  '.zip', '.rar', '.7z', '.gz', '.bz2', '.xz', '.tar',
  '.exe', '.dll', '.msi', '.so', '.dylib',
  '.png', '.jpg', '.jpeg', '.gif', '.bmp', '.webp', '.ico', '.svgz',
  '.pdf', '.doc', '.docx', '.xls', '.xlsx', '.ppt', '.pptx',
  '.mp3', '.mp4', '.avi', '.mkv', '.wav', '.flac',
  '.iso', '.img', '.bin', '.dat', '.db', '.sqlite', '.class', '.jar', '.pyc'
]);

/** CodeMirror 语言 id（plaintext 兜底，未知扩展也按纯文本打开）。 */
export type EditorLanguageId =
  | 'json' | 'markdown' | 'yaml' | 'toml' | 'ini' | 'properties' | 'xml' | 'plaintext';

function fileExt(name: string): string {
  const idx = name.lastIndexOf('.');
  if (idx <= 0) return '';
  return name.slice(idx).toLowerCase();
}

/** 点文件形态的 .env 系列（.env / .env.local / .env.production）。 */
function isEnvDotfile(name: string): boolean {
  const lower = name.toLowerCase();
  return lower === '.env' || lower.startsWith('.env.');
}

/** 是否已知文本类型（决定双击直接进编辑器）。 */
export function isTextExtension(name: string): boolean {
  if (isEnvDotfile(name)) return true;
  return TEXT_EXTENSIONS.has(fileExt(name));
}

/** 是否已知二进制类型（右键「编辑」禁用的判定；未知扩展不算二进制，允许尝试）。 */
export function isKnownBinaryExtension(name: string): boolean {
  return BINARY_EXTENSIONS.has(fileExt(name));
}

export function languageIdForName(name: string): EditorLanguageId {
  if (isEnvDotfile(name)) return 'properties';
  switch (fileExt(name)) {
    case '.json': return 'json';
    case '.md': case '.markdown': return 'markdown';
    case '.yaml': case '.yml': return 'yaml';
    case '.toml': return 'toml';
    case '.ini': case '.conf': case '.cfg': return 'ini';
    case '.properties': return 'properties';
    case '.xml': return 'xml';
    default: return 'plaintext';
  }
}

/**
 * 按语言 id 懒加载 CodeMirror 语言扩展。
 * ini/conf/cfg 用 legacy toml 模式（[section] + key=value 注释高亮最接近）；
 * properties/env 用 legacy properties 模式；纯文本返回 null。
 */
export async function loadLanguageExtension(id: EditorLanguageId): Promise<Extension | null> {
  switch (id) {
    case 'json': {
      const { json } = await import('@codemirror/lang-json');
      return json();
    }
    case 'markdown': {
      const { markdown } = await import('@codemirror/lang-markdown');
      return markdown();
    }
    case 'yaml': {
      const { yaml } = await import('@codemirror/lang-yaml');
      return yaml();
    }
    case 'xml': {
      const { xml } = await import('@codemirror/lang-xml');
      return xml();
    }
    case 'toml': {
      const { StreamLanguage } = await import('@codemirror/language');
      const { toml } = await import('@codemirror/legacy-modes/mode/toml');
      return StreamLanguage.define(toml);
    }
    case 'ini': {
      // legacy 没有独立 ini 模式，toml 的 [section]/#/注释 高亮是最接近的替身
      const { StreamLanguage } = await import('@codemirror/language');
      const { toml } = await import('@codemirror/legacy-modes/mode/toml');
      return StreamLanguage.define(toml);
    }
    case 'properties': {
      const { StreamLanguage } = await import('@codemirror/language');
      const { properties } = await import('@codemirror/legacy-modes/mode/properties');
      return StreamLanguage.define(properties);
    }
    default:
      return null;
  }
}
