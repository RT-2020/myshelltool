// 文件列 grid 模板权威定义（S5）：行（FileColumnList）与表头（FileColumnColumns）
// 共享同一组列宽，避免两处硬编码漂移导致表头与行错位。
// 行首列 16px 是图标列；表头无图标列故不含 16px。
export const FILE_COLUMN_COLS = '64px 56px 130px'; // size/type/mtime
export const FILE_COLUMN_GRID_ROW = `16px minmax(0, 1fr) ${FILE_COLUMN_COLS}`;
export const FILE_COLUMN_GRID_HEADER = `minmax(0, 1fr) ${FILE_COLUMN_COLS}`;
export const FILE_COLUMN_GRID_COMPACT = '16px minmax(0, 1fr)';

/** 文件列表条目的最小形状（真实 SFTP/本地条目的结构子集）。 */
export interface FileColumnEntry {
  name: string;
  kind?: string;
  modified?: string | null;
}

/** 面包屑节点。 */
export interface PathCrumb {
  label: string;
  path: string;
}

export function inferFileEntryType(entry: FileColumnEntry): string {
  if (entry.kind === 'directory') return 'DIR';
  if (entry.kind === 'symlink') return 'LNK';

  const dot = entry.name.lastIndexOf('.');
  if (dot <= 0 || dot === entry.name.length - 1) return 'FILE';

  const ext = entry.name.slice(dot + 1).toUpperCase();
  return ext.length > 5 ? ext.slice(0, 5) : ext;
}

export function formatFileEntrySize(bytes: number | null | undefined): string {
  const size = Number(bytes) || 0;
  if (size >= 1024 * 1024) return Math.round(size / 1024 / 1024) + ' MB';
  if (size >= 1024) return Math.round(size / 1024) + ' KB';
  return size + ' B';
}

export function formatFileEntryTime(entry: FileColumnEntry): string {
  if (!entry.modified) return '—';
  // entry.modified 全部由自家后端产出（恒为 epoch 秒串或空串，自身契约）：
  // 纯数字即按秒解析（0 也正确显示 epoch 起点），非数字回退原串，不用位数猜。
  if (/^\d+$/.test(entry.modified)) {
    const d = new Date(Number(entry.modified) * 1000);
    if (!Number.isNaN(d.getTime())) return d.toLocaleString();
  }
  return entry.modified;
}

export function buildPathCrumbs(path: string | null | undefined): PathCrumb[] {
  const raw = path || '';
  if (!raw) return [];

  const normalized = raw.replace(/\\/g, '/');
  const segments = normalized.split('/').filter(Boolean);

  // UNC 路径（\\server\share\docs，已 normalize 为 //server/share/docs）：
  // 前两段（server/share）是主机+共享，必须合并为一个 crumb——拆开的话点击
  // /server 必然导航失败。label 用 Windows 风格展示。不足两段（不完整 UNC）
  // 也整体合并为一个 crumb，不切出无法导航的 /server。
  if (normalized.startsWith('//')) {
    const rootSegs = segments.slice(0, 2);
    if (rootSegs.length === 0) return [];
    const result: PathCrumb[] = [{
      label: '\\\\' + rootSegs.join('\\'),
      path: '//' + rootSegs.join('/')
    }];
    let current = '//' + rootSegs.join('/');
    for (let i = 2; i < segments.length; i++) {
      current += `/${segments[i]}`;
      result.push({ label: segments[i], path: current });
    }
    return result;
  }

  if (normalized.startsWith('/')) {
    const result: PathCrumb[] = [{ label: '/', path: '/' }];
    let current = '';
    for (const segment of segments) {
      current += `/${segment}`;
      result.push({ label: segment, path: current });
    }
    return result;
  }

  let current = '';
  return segments.map((segment, index) => {
    current = index === 0 ? segment : `${current}/${segment}`;
    const displayPath = /^[a-zA-Z]:$/.test(segment) ? `${segment}\\` : current;
    return { label: segment, path: displayPath };
  });
}
