/**
 * editorCodec — 编码/换行的前端清单（与 core::text_codec 白名单一一对应）。
 *
 * 后端是权威（白名单外 label 一律拒绝）；这里只维护展示名与下拉清单，
 * 漂移会在「按 GBK 重读」等链路上被后端 fail-closed 拦下并如实报错。
 */
import type { EditorEncoding } from '@/types/editor';

export interface EncodingOption {
  label: EditorEncoding;
  text: string;
}

export const ENCODING_OPTIONS: readonly EncodingOption[] = [
  { label: 'utf-8', text: 'UTF-8（默认）' },
  { label: 'gbk', text: 'GBK（简体中文）' },
  { label: 'gb18030', text: 'GB18030（简体中文超集）' },
  { label: 'big5', text: 'Big5（繁体中文）' },
  { label: 'shift_jis', text: 'Shift_JIS（日文）' },
  { label: 'euc-jp', text: 'EUC-JP（日文）' },
  { label: 'euc-kr', text: 'EUC-KR（韩文）' },
  { label: 'windows-1252', text: 'Windows-1252（西文）' }
];

/** 前端侧白名单校验（提交给后端前的快速失败；后端仍是权威）。 */
export function isSupportedEncodingLabel(label: string): label is EditorEncoding {
  return ENCODING_OPTIONS.some(opt => opt.label === label.toLowerCase());
}

export interface EolOption {
  id: 'lf' | 'crlf';
  text: string;
}

export const EOL_OPTIONS: readonly EolOption[] = [
  { id: 'lf', text: 'LF（Unix）' },
  { id: 'crlf', text: 'CRLF（Windows）' }
];

export function eolDisplayText(eol: string): string {
  switch (eol) {
    case 'crlf': return 'CRLF';
    case 'mixed': return '混合';
    default: return 'LF';
  }
}
