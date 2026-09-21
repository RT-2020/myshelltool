/**
 * editorValidation — 保存前/工具栏的按类型校验与 JSON 工具。
 *
 * 口径：JSON 有精确行列定位（解析引擎错误串里的 position 换算）；
 * YAML 用 js-yaml（mark 带 0 基行列，+1 展示）；TOML 用 smol-toml
 * （best-effort，错误串原样透出）；XML 用浏览器 DOMParser（零依赖，
 * parsererror 判定）；ini/properties/conf 纯高亮不校验（返回 ok）。
 *
 * 这里解析的是「浏览器/解析库自己的错误文本」，属于自家依赖的固定输出，
 * 不是对外部环境的猜测（对照 AGENTS.md 指南 §7）。
 */
import type { EditorLanguageId } from '@/lib/editor/editorLanguages';

export interface ValidationIssue {
  ok: boolean;
  /** ok=false 时的可读描述（含行列）。 */
  message: string;
  /** 1 基行列（尽力提取；拿不到时为 null）。 */
  line: number | null;
  column: number | null;
}

const OK: ValidationIssue = { ok: true, message: '', line: null, column: null };

/** 把字符偏移换算成 1 基行列。 */
function offsetToLineCol(text: string, offset: number): { line: number; column: number } {
  const clamped = Math.max(0, Math.min(offset, text.length));
  let line = 1;
  let lastLineStart = 0;
  for (let i = 0; i < clamped; i++) {
    if (text.charCodeAt(i) === 10) {
      line++;
      lastLineStart = i + 1;
    }
  }
  return { line, column: clamped - lastLineStart + 1 };
}

export function validateJsonText(text: string): ValidationIssue {
  try {
    JSON.parse(text);
    return OK;
  } catch (e) {
    const raw = e instanceof Error ? e.message : String(e);
    // V8 错误文本形态：… at position N …（老形态）/ …(line L column C)…（新形态）
    const posMatch = raw.match(/at position (\d+)/);
    if (posMatch) {
      const { line, column } = offsetToLineCol(text, Number(posMatch[1]));
      return { ok: false, message: `JSON 语法错误（第 ${line} 行第 ${column} 列）：${raw}`, line, column };
    }
    const lcMatch = raw.match(/\(line (\d+) column (\d+)\)/);
    if (lcMatch) {
      return { ok: false, message: `JSON 语法错误（第 ${lcMatch[1]} 行第 ${lcMatch[2]} 列）：${raw}`, line: Number(lcMatch[1]), column: Number(lcMatch[2]) };
    }
    return { ok: false, message: `JSON 语法错误：${raw}`, line: null, column: null };
  }
}

export function formatJsonText(text: string): string {
  return JSON.stringify(JSON.parse(text), null, 2);
}

export function minifyJsonText(text: string): string {
  return JSON.stringify(JSON.parse(text));
}

async function validateYaml(text: string): Promise<ValidationIssue> {
  const yaml = await import('js-yaml');
  try {
    yaml.load(text);
    return OK;
  } catch (e) {
    const err = e as { message?: string; mark?: { line?: number; column?: number } };
    const line = typeof err.mark?.line === 'number' ? err.mark.line + 1 : null;
    const column = typeof err.mark?.column === 'number' ? err.mark.column + 1 : null;
    const where = line !== null && column !== null ? `（第 ${line} 行第 ${column} 列）` : '';
    return { ok: false, message: `YAML 解析错误${where}：${err.message ?? String(e)}`, line, column };
  }
}

async function validateToml(text: string): Promise<ValidationIssue> {
  const { parse } = await import('smol-toml');
  try {
    parse(text);
    return OK;
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    return { ok: false, message: `TOML 解析错误：${msg}`, line: null, column: null };
  }
}

function validateXml(text: string): ValidationIssue {
  const doc = new DOMParser().parseFromString(text, 'application/xml');
  const parseError = doc.getElementsByTagName('parsererror')[0];
  if (parseError) {
    return { ok: false, message: `XML 解析错误：${parseError.textContent?.trim() || '格式不合法'}`, line: null, column: null };
  }
  return OK;
}

/** 按语言校验（保存前与工具栏「校验」共用）。不支持的类型恒 ok。
 *  ini/properties/conf 无可靠语法校验器（TOML 校验会误报——两者对反斜杠、
 *  重复键、裸值规则并不兼容），按「可能时校验」原则跳过。 */
export async function validateForLanguage(
  language: EditorLanguageId,
  text: string
): Promise<ValidationIssue> {
  switch (language) {
    case 'json': return validateJsonText(text);
    case 'yaml': return validateYaml(text);
    case 'toml': return validateToml(text);
    case 'xml': return validateXml(text);
    default: return OK;
  }
}
