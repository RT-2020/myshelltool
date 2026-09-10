// 文件列 grid 模板权威定义（S5）：行（FileColumnList）与表头（FileColumnColumns）
// 共享同一组列宽，避免两处硬编码漂移导致表头与行错位。
// 行首列 16px 是图标列；表头无图标列故不含 16px。
// 元数据列必须定宽：行的滚动容器比表头窄一条滚动条（约 8px），定宽列两侧同宽才对得齐，
// 差额只能由响应式的名称列（minmax(0, 1fr)）吸收。
export const FILE_COLUMN_COLS = '76px 58px 46px 130px'; // permissions/size/type/mtime
export const FILE_COLUMN_GRID_ROW = `16px minmax(0, 1fr) ${FILE_COLUMN_COLS}`;
export const FILE_COLUMN_GRID_HEADER = `minmax(0, 1fr) ${FILE_COLUMN_COLS}`;
// 窄栏回退模板（见 FileColumn.vue 的 container-type + 两处 @container 规则）：
// 双栏模式每栏仅约 400px 时去掉权限列，否则名称列被元数据列挤到十几像素。
export const FILE_COLUMN_COLS_NARROW = '58px 46px 130px'; // size/type/mtime
export const FILE_COLUMN_GRID_ROW_NARROW = `16px minmax(0, 1fr) ${FILE_COLUMN_COLS_NARROW}`;
export const FILE_COLUMN_GRID_HEADER_NARROW = `minmax(0, 1fr) ${FILE_COLUMN_COLS_NARROW}`;
export const FILE_COLUMN_GRID_COMPACT = '16px minmax(0, 1fr)';

/** 文件列表条目的最小形状（真实 SFTP/本地条目的结构子集）。 */
export interface FileColumnEntry {
  name: string;
  kind?: string;
  modified?: string | null;
  /** 权限八进制串（如 "0755"），后端未提供时为 undefined/null。 */
  permissions?: string | null;
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

/** 后端未提供权限时的占位（Windows 本地文件、老 SFTP server）。 */
export const PERMISSION_UNKNOWN = '—';

/**
 * 权限八进制串 → `ls -l` 风格符号串（`drwxr-xr-x`）。
 *
 * 契约：后端（`ssh.rs` / `fs_local.rs`）恒输出 `format!("{:04o}", mode & 0o7777)`
 * 的八进制串；非该形状（空串/未提供/异常值）一律按「无权限信息」渲染成 '—'，
 * 不猜、不补位（同 `formatFileEntryTime` 的 fail-closed 处理）。
 * 文件类型字符取自 `kind`（SFTP attrs 已被后端掩掉类型位，不能从 mode 推断）：
 * setuid/setgid/sticky 按 `ls -l` 规则显示 s/S、t/T。
 */
export function formatFileEntryPermissions(entry: FileColumnEntry): string {
  const raw = (entry.permissions ?? '').trim();
  if (!/^[0-7]{1,4}$/.test(raw)) return PERMISSION_UNKNOWN;
  const mode = parseInt(raw, 8);
  const typeChar = entry.kind === 'directory' ? 'd' : entry.kind === 'symlink' ? 'l' : '-';
  // 三元组：special 位会让执行位显示成 s/t（有执行位）或 S/T（无执行位）。
  const triad = (bits: number, special: 's' | 't' | 0) => {
    const exec = bits & 1 ? 'x' : '-';
    const last = special === 0
      ? exec
      : (bits & 1 ? special : special.toUpperCase());
    return `${bits & 4 ? 'r' : '-'}${bits & 2 ? 'w' : '-'}${last}`;
  };
  return typeChar
    + triad((mode >> 6) & 7, mode & 0o4000 ? 's' : 0)
    + triad((mode >> 3) & 7, mode & 0o2000 ? 's' : 0)
    + triad(mode & 7, mode & 0o1000 ? 't' : 0);
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
