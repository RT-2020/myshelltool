/**
 * editorIo — 编辑器 IPC 封装 + 错误分类。
 *
 * 错误前缀协议：后端 Err(String) 正文以 `[editor:kind]` 开头（kind 见
 * types/editor.ts 的 EditorErrorKind），无前缀的错误按「网络/会话类」归
 * network（可重试）。协议契约固化在 docs/IPC契约与数据模型.md。
 */
import { invokeBackend } from '@/services/backend';
import { errorMessage } from '@/lib/errorMessage';
import type {
  EditorBackupMeta,
  EditorClassifiedError,
  EditorDraft,
  EditorErrorKind,
  EditorTarget,
  ReadTextResult,
  WriteTextResult
} from '@/types/editor';

const KNOWN_KINDS: ReadonlySet<string> = new Set([
  'too-large', 'binary', 'not-utf8', 'encoding', 'lossy', 'not-found', 'dir', 'permission'
]);

export function classifyEditorError(raw: string): EditorClassifiedError {
  const m = raw.match(/^\[editor:([a-z-]+)\]\s*(.*)$/s);
  if (m && KNOWN_KINDS.has(m[1]!)) {
    return { kind: m[1] as EditorErrorKind, message: m[2] || raw, raw };
  }
  // 无前缀：会话断开/网络抖动/锁等——按可重试处理
  return { kind: 'network', message: raw, raw };
}

/** 统一异常 → 分类错误（Err(String) reject 出来的是字符串，Error 实例取 message）。 */
export function toClassifiedError(error: unknown): EditorClassifiedError {
  return classifyEditorError(errorMessage(error));
}

/** 确定性错误不给「重试打开」按钮（重试也必然同样失败）。 */
export function isErrorKindRetryable(kind: EditorErrorKind): boolean {
  return !(kind === 'too-large' || kind === 'binary' || kind === 'lossy' || kind === 'dir');
}

export interface ReadTextParams {
  /** kind=remote 必填。 */
  sessionId?: string;
  /** null = 默认 UTF-8 管线（非 UTF-8 会报 [editor:not-utf8] 引导切换）。 */
  encoding?: string | null;
}

export async function readText(target: EditorTarget, params: ReadTextParams = {}): Promise<ReadTextResult> {
  if (target.kind === 'remote') {
    if (!params.sessionId) throw new Error('缺少 SSH 会话（请先连接资产）');
    return invokeBackend<ReadTextResult>('sftp_read_text', {
      sessionId: params.sessionId,
      path: target.path,
      encoding: params.encoding ?? null
    });
  }
  return invokeBackend<ReadTextResult>('fs_local_read_text', {
    path: target.path,
    encoding: params.encoding ?? null
  });
}

export interface WriteTextParams {
  sessionId?: string;
  content: string;
  encoding?: string | null;
  /** 'lf' | 'crlf'；null/其它 = 保持内容原样（混合换行不归一）。 */
  eol?: string | null;
  keepBom?: boolean;
  /** 打开时的基线；提供即守护写（不一致返回 conflict），都空 = 强制写（新建/另存为）。 */
  expectedSize?: number | null;
  expectedModified?: string | null;
  backup?: boolean;
}

export async function writeText(target: EditorTarget, params: WriteTextParams): Promise<WriteTextResult> {
  const args = {
    path: target.path,
    content: params.content,
    encoding: params.encoding ?? null,
    eol: params.eol ?? null,
    keepBom: params.keepBom ?? false,
    expectedSize: params.expectedSize ?? null,
    expectedModified: params.expectedModified ?? null,
    backup: params.backup ?? false
  };
  if (target.kind === 'remote') {
    if (!params.sessionId) throw new Error('缺少 SSH 会话（请先连接资产）');
    return invokeBackend<WriteTextResult>('sftp_write_text', {
      sessionId: params.sessionId,
      ...args
    });
  }
  return invokeBackend<WriteTextResult>('fs_local_write_text', args);
}

function targetKindOf(target: EditorTarget): string {
  return target.kind;
}

/** 本地路径存在性探测：'yes' | 'no' | 'error'（fail-closed）。
 *  走 read_text 的错误前缀协议（too-large/binary/not-utf8 也证明存在）。 */
export async function probeLocalExists(path: string): Promise<'yes' | 'no' | 'error'> {
  try {
    await invokeBackend<ReadTextResult>('fs_local_read_text', { path, encoding: null });
    return 'yes';
  } catch (error) {
    const classified = classifyEditorError(errorMessage(error));
    if (classified.kind === 'not-found') return 'no';
    if (classified.kind === 'too-large' || classified.kind === 'binary' || classified.kind === 'not-utf8') return 'yes';
    return 'error';
  }
}

export async function listBackups(target: EditorTarget): Promise<EditorBackupMeta[]> {
  return invokeBackend<EditorBackupMeta[]>('editor_backup_list', {
    target: targetKindOf(target),
    path: target.path
  });
}

export async function readBackup(
  target: EditorTarget,
  id: string,
  encoding: string | null
): Promise<ReadTextResult> {
  return invokeBackend<ReadTextResult>('editor_backup_read', {
    target: targetKindOf(target),
    path: target.path,
    id,
    encoding
  });
}

export async function saveDraft(
  target: EditorTarget,
  content: string,
  encoding: string,
  eol: string
): Promise<string> {
  return invokeBackend<string>('editor_draft_save', {
    target: targetKindOf(target),
    path: target.path,
    content,
    encoding,
    eol
  });
}

export async function getDraft(target: EditorTarget): Promise<EditorDraft | null> {
  return invokeBackend<EditorDraft | null>('editor_draft_get', {
    target: targetKindOf(target),
    path: target.path
  });
}

export async function deleteDraft(target: EditorTarget): Promise<void> {
  return invokeBackend<void>('editor_draft_delete', {
    target: targetKindOf(target),
    path: target.path
  });
}
