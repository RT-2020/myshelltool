/**
 * fileListView — 文件面板列表视图域：排序/过滤视图 + 远程/本地多选 +
 * 视图模式（v0.20 自 filePanel 拆出，S2 的一刀；逻辑原样迁移）。
 *
 * 依赖方向：filePanel → 本模块（ls 等）；本模块的 pc() 经 bindFileListView
 * 由 filePanel 级联注入（避免与 filePanel 形成值 import 环）。
 * 调用方仍经 filePanel 的 re-export 取函数（路径不变零改动）。
 */
import type { RemoteFileEntry } from '@/types/domain';
import type { FilePanelContext } from '@/lib/filePanel';

/** 视图状态由 shell 持有并经 bindRemoteListState 注入（箭头/refs 取值恒新）。 */
export interface RemoteListViews {
  filtered: RemoteFileEntry[];
  sorted: RemoteFileEntry[];
}
export interface RemoteListState {
  filter: { value: string };
  sortKey: { value: string };
  sortDir: { value: string };
  lastRemote: { value: number };
  lastLocal: { value: number };
  selectedRemote: { value: Set<string> };
  selectedLocal: { value: Set<string> };
  remoteEntries: { value: RemoteFileEntry[] };
  listMode: { value: string };
  localListMode: { value: string };
}

let st: RemoteListState | null = null;
let pcFn: (() => FilePanelContext) | null = null;

export function bindRemoteListState(state: RemoteListState): void {
  st = state;
}

/** filePanel.bindFilePanelContext 级联注入 pc 访问器。 */
export function bindFileListView(pc: () => FilePanelContext): void {
  pcFn = pc;
}

export function ls(): RemoteListState {
  if (!st) throw new Error('filePanel 列表视图状态未绑定（files store 未初始化）');
  return st;
}

function pc(): FilePanelContext {
  if (!pcFn) throw new Error('fileListView: pc not bound');
  return pcFn();
}

/** 排序/过滤视图（原 shell 的 filteredRemoteEntries/sortedRemoteEntries computed，逐句迁移）。 */
export function computeFiltered(): RemoteFileEntry[] {
  const q = ls().filter.value.trim().toLowerCase();
  if (!q) return ls().remoteEntries.value;
  return ls().remoteEntries.value.filter(e => e.name.toLowerCase().includes(q));
}

export function sortEntries(filtered: RemoteFileEntry[]): RemoteFileEntry[] {
  const key = ls().sortKey.value;
  const dir = ls().sortDir.value === 'asc' ? 1 : -1;
  // 类型推断：目录→DIR / 符号链接→LNK / 普通文件→扩展名大写（无扩展名→FILE）。
  const typeOf = (e: RemoteFileEntry) => {
    if (e.kind === 'directory') return 'DIR';
    if (e.kind === 'symlink') return 'LNK';
    const dot = e.name.lastIndexOf('.');
    if (dot <= 0 || dot === e.name.length - 1) return 'FILE';
    return e.name.slice(dot + 1).toUpperCase();
  };
  // 用户:组 组合串用于排序。
  const ownerOf = (e: RemoteFileEntry) => [e.user || '', e.group || ''].join(':');
  const cmp = (a: RemoteFileEntry, b: RemoteFileEntry) => {
    const aDir = a.kind === 'directory' ? 0 : 1;
    const bDir = b.kind === 'directory' ? 0 : 1;
    if (aDir !== bDir) return aDir - bDir;
    let av: number | string;
    let bv: number | string;
    if (key === 'size') { av = a.size || 0; bv = b.size || 0; }
    else if (key === 'modified') { av = Number(a.modified) || 0; bv = Number(b.modified) || 0; }
    else if (key === 'type') { av = typeOf(a); bv = typeOf(b); }
    else if (key === 'permissions') {
      // 权限按八进制数值排（缺权限当作 0）。
      av = a.permissions ? parseInt(a.permissions, 8) || 0 : 0;
      bv = b.permissions ? parseInt(b.permissions, 8) || 0 : 0;
    }
    else if (key === 'owner') { av = ownerOf(a); bv = ownerOf(b); }
    else { av = a.name.toLowerCase(); bv = b.name.toLowerCase(); }
    if (av < bv) return -1 * dir;
    if (av > bv) return 1 * dir;
    return 0;
  };
  return [...filtered].sort(cmp);
}

// ============================================================
// Remote 多选 / 排序 / 过滤
// ============================================================
export function toggleRemoteSelection(path: string, { additive = false, range = false } = {}) {
  const list = sortEntries(computeFiltered());
  const idx = list.findIndex(e => e.path === path);
  if (range && ls().lastRemote.value >= 0 && idx >= 0) {
    const [start, end] = [ls().lastRemote.value, idx].sort((a, b) => a - b);
    const next = new Set(ls().selectedRemote.value);
    for (let i = start; i <= end; i++) next.add(list[i].path);
    ls().selectedRemote.value = next;
    return;
  }
  const next = new Set(additive ? ls().selectedRemote.value : []);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  ls().selectedRemote.value = next;
  ls().lastRemote.value = idx;
}

export function selectAllRemote() {
  ls().selectedRemote.value = new Set(sortEntries(computeFiltered()).map(e => e.path));
}

export function clearRemoteSelection() {
  if (ls().selectedRemote.value.size) ls().selectedRemote.value = new Set();
  ls().lastRemote.value = -1;
}

export function toggleLocalSelection(path: string, { additive = false, range = false } = {}) {
  const list = pc().localEntries.value;
  const idx = list.findIndex(e => e.path === path);
  if (range && ls().lastLocal.value >= 0 && idx >= 0) {
    // shift 范围多选：与远程 range 逻辑同构（参照 toggleRemoteSelection）
    const [start, end] = [ls().lastLocal.value, idx].sort((a, b) => a - b);
    const next = new Set(ls().selectedLocal.value);
    for (let i = start; i <= end; i++) next.add(list[i].path);
    ls().selectedLocal.value = next;
    return;
  }
  const next = new Set(additive ? ls().selectedLocal.value : []);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  ls().selectedLocal.value = next;
  ls().lastLocal.value = idx;
}

export function selectAllLocal() {
  ls().selectedLocal.value = new Set(pc().localEntries.value.map(e => e.path));
}

export function clearLocalSelection() {
  if (ls().selectedLocal.value.size) ls().selectedLocal.value = new Set();
}

export function setRemoteSort(key: string) {
  if (ls().sortKey.value === key) {
    ls().sortDir.value = ls().sortDir.value === 'asc' ? 'desc' : 'asc';
  } else {
    ls().sortKey.value = key;
    ls().sortDir.value = 'asc';
  }
}

export function setRemoteFilter(query: string) {
  ls().filter.value = query;
}

export function setRemoteListMode(mode: string) {
  if (mode === 'compact' || mode === 'detailed') ls().listMode.value = mode;
}

export function setLocalListMode(mode: string) {
  if (mode === 'compact' || mode === 'detailed') ls().localListMode.value = mode;
}
