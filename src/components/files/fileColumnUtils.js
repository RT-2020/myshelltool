// 文件列 grid 模板权威定义（S5）：行（FileColumnList）与表头（FileColumnColumns）
// 共享同一组列宽，避免两处硬编码漂移导致表头与行错位。
// 行首列 16px 是图标列；表头无图标列故不含 16px。
export const FILE_COLUMN_COLS = '64px 56px 130px 56px 92px'; // size/type/mtime/perm/owner
export const FILE_COLUMN_GRID_ROW = `16px minmax(0, 1fr) ${FILE_COLUMN_COLS}`;
export const FILE_COLUMN_GRID_HEADER = `minmax(0, 1fr) ${FILE_COLUMN_COLS}`;
export const FILE_COLUMN_GRID_COMPACT = '16px minmax(0, 1fr)';

export function inferFileEntryType(entry) {
  if (entry.kind === 'directory') return 'DIR';
  if (entry.kind === 'symlink') return 'LNK';

  const dot = entry.name.lastIndexOf('.');
  if (dot <= 0 || dot === entry.name.length - 1) return 'FILE';

  const ext = entry.name.slice(dot + 1).toUpperCase();
  return ext.length > 5 ? ext.slice(0, 5) : ext;
}

export function formatFileEntrySize(bytes) {
  const size = Number(bytes) || 0;
  if (size >= 1024 * 1024) return Math.round(size / 1024 / 1024) + ' MB';
  if (size >= 1024) return Math.round(size / 1024) + ' KB';
  return size + ' B';
}

export function formatFileEntryTime(entry) {
  if (!entry.modified) return '—';
  if (/^\d+$/.test(entry.modified) && entry.modified.length >= 8) {
    const d = new Date(Number(entry.modified) * 1000);
    if (!Number.isNaN(d.getTime())) return d.toLocaleString();
  }
  return entry.modified;
}

export function formatFileEntryOwner(entry) {
  const user = entry.user;
  const group = entry.group;
  if (!user && !group) return '—';
  if (user && group) return `${user}:${group}`;
  return user || group || '—';
}

export function buildPathCrumbs(path) {
  const raw = path || '';
  if (!raw) return [];

  const normalized = raw.replace(/\\/g, '/');
  const segments = normalized.split('/').filter(Boolean);

  if (normalized.startsWith('/')) {
    const result = [{ label: '/', path: '/' }];
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
