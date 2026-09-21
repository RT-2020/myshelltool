/**
 * editorPaths — 编辑器侧的路径小工具（远端 / 与本地 \ 皆可的 basename）。
 * 与 lib/pathUtils 分工：那边服务文件面板的 join/parent（远端 POSIX 语义），
 * 这里只要展示名，对两种分隔符都宽容。
 */
export function basenameOf(path: string): string {
  const normalized = path.replace(/\\/g, '/');
  const idx = normalized.lastIndexOf('/');
  return idx >= 0 ? normalized.slice(idx + 1) : normalized;
}
