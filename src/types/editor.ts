/**
 * editor — 内置文本/配置文件编辑器域类型（v0.18）。
 *
 * 与后端契约对齐（docs/IPC契约与数据模型.md「编辑器文本读写」节）：
 * - ReadTextResult / WriteTextResult 对齐 Rust 端 camelCase 序列化；
 * - 错误经 Err(String) reject，正文带 `[editor:kind]` 前缀（editorIo.ts 解析）。
 */

export type EditorTargetKind = 'remote' | 'local';

/** 编辑目标：远端文件（挂在某资产会话上）或本地文件。 */
export interface EditorTarget {
  kind: EditorTargetKind;
  /** kind=remote 时必填：文件所属资产 id（会话解析与归属展示用）。 */
  assetId: string | null;
  /** 远端绝对路径 / 本地路径。 */
  path: string;
}

/** 编码白名单（与 core::text_codec::SUPPORTED_ENCODING_LABELS 一一对应）。 */
export type EditorEncoding =
  | 'utf-8'
  | 'gbk'
  | 'gb18030'
  | 'big5'
  | 'shift_jis'
  | 'euc-jp'
  | 'euc-kr'
  | 'windows-1252';

export type EditorEol = 'lf' | 'crlf' | 'mixed';

/** 后端 ReadTextResult（sftp_read_text / fs_local_read_text / editor_backup_read）。 */
export interface ReadTextResult {
  content: string;
  encoding: string;
  hasBom: boolean;
  eol: EditorEol;
  size: number;
  /** Unix 秒字符串（与 RemoteFileEntry.modified 同源）。 */
  modified: string;
  readOnly?: boolean;
}

/** 后端 WriteTextResult。status==='conflict' 时未写任何字节。 */
export interface WriteTextResult {
  status: 'written' | 'conflict';
  size: number;
  modified: string;
  conflictNote?: string | null;
}

/** 备份版本元数据（editor_backup_list）。 */
export interface EditorBackupMeta {
  /** 版本 id（毫秒时间戳字符串，纯数字）。 */
  id: string;
  at: string;
  size: number;
  target: string;
  path: string;
}

/** 草稿（editor_draft_get）。 */
export interface EditorDraft {
  target: string;
  path: string;
  content: string;
  encoding: string;
  eol: string;
  savedAt: string;
}

/**
 * 编辑器错误分类（后端 `[editor:kind]` 前缀协议 + 前端补充的网络类）。
 * retryable 由 kind 推导：确定性错误（too-large/binary/lossy/dir）不给重试。
 */
export type EditorErrorKind =
  | 'too-large'
  | 'binary'
  | 'not-utf8'
  | 'encoding'
  | 'lossy'
  | 'not-found'
  | 'dir'
  | 'permission'
  | 'network'
  | 'unknown';

export interface EditorClassifiedError {
  kind: EditorErrorKind;
  /** 去掉前缀后的正文。 */
  message: string;
  /** 原始错误串。 */
  raw: string;
}

export type EditorTabStatus = 'loading' | 'ready' | 'error';

/** 打开时记录的原始基线（保存冲突检测的 expected 值来源）。 */
export interface EditorBaseline {
  size: number;
  modified: string;
  encoding: string;
  eol: EditorEol;
  hasBom: boolean;
}

export interface EditorTab {
  id: string;
  target: EditorTarget;
  /** 展示名（basename）。 */
  name: string;
  status: EditorTabStatus;
  error: EditorClassifiedError | null;
  /** 用户显式选择的编码（null = 默认 UTF-8 管线；切换编码重读时更新）。 */
  encodingOverride: string | null;
  baseline: EditorBaseline | null;
  /** 只读态（权限位/只读属性推断；写失败也会如实提示）。 */
  readOnly: boolean;
  readOnlyReason: string | null;
  dirty: boolean;
  /** 后端返回 conflict 时的暂存（三选弹窗的上下文）。 */
  conflict: { note: string; size: number; modified: string } | null;
  /** 正文版本号：reload/恢复草稿后 +1，驱动挂载中的 CodeMirror 重载 doc。 */
  contentVersion: number;
}

// ============================================================
// 编辑器弹窗（GlobalModals 单一 type `editorDialog` 的载荷与桥接）
// ============================================================

export interface EditorDialogButton {
  label: string;
  danger?: boolean;
  primary?: boolean;
  /** 返回值约定：string = 内联错误（弹窗保持打开）；其余（含 Promise 非 string）= 关闭弹窗。 */
  action?: (inputValue?: string) => unknown;
}

export interface EditorDialogPayload {
  title: string;
  message: string;
  detail?: string;
  /** 提供则渲染路径输入框（另存为/上传远端/打开路径）。 */
  input?: { label: string; placeholder?: string; value?: string };
  buttons: EditorDialogButton[];
}
