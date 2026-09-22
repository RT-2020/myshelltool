/**
 * fileDelete — 文件面板删除确认链（v0.20 自 filePanel 拆出，S2 的一刀；
 * 逻辑原样迁移：确认弹窗登记 → confirmFileDelete 逐项删除（归属校验防
 * 切资产误删同名路径）→ 真实计数播报；cancel/× 关闭走 cancelFileDelete）。
 * 依赖（会话解析/列表刷新/列表状态）经 bindFileDeleteDeps 由 filePanel
 * 级联注入；调用方仍经 filePanel 的 re-export 取函数。
 */
import type { RemoteFileEntry } from '@/types/domain';
import { invokeBackend } from '@/services/backend';
import { errorMessage } from '@/lib/errorMessage';
import type { FilePanelContext } from '@/lib/filePanel';
import type { RemoteListState } from '@/lib/filePanel';

export interface FileDeleteDeps {
  pc(): FilePanelContext;
  getActiveSession(): { sessionId: string } | null;
  noSessionMessage(action: string): string;
  refreshRemoteFiles(path?: string | null, opts?: { silent?: boolean }): Promise<void>;
  refreshLocalFiles(path?: string | null): Promise<void>;
  ls(): RemoteListState;
}

let deps: FileDeleteDeps | null = null;

/** filePanel.bindFilePanelContext 级联注入。 */
export function bindFileDeleteDeps(d: FileDeleteDeps): void {
  deps = d;
}

function d(): FileDeleteDeps {
  if (!deps) throw new Error('fileDelete: deps not bound');
  return deps;
}

function openFileDeleteConfirm(kind: 'remote' | 'local', paths: string[], names: string[]) {
  d().pc().pendingFileDelete.value = {
    kind,
    paths,
    names,
    // 仅远程删除需要归属：本地删除与资产无关
    assetId: kind === 'remote' ? (d().pc().wb().selectedAsset?.id ?? null) : null
  };
  d().pc().wb().modal = { type: 'confirmFileDelete' };
}

export function removeRemote(entry: RemoteFileEntry) {
  const session = d().getActiveSession();
  if (!session) {
    d().pc().announce(d().noSessionMessage('删除'), { level: 'warn' });
    return;
  }
  openFileDeleteConfirm('remote', [entry.path], [entry.name]);
}

export function localDelete(paths?: string[] | null) {
  if (!paths?.length) return;
  const names = paths.map(p => d().pc().localEntries.value.find(e => e.path === p)?.name || p);
  openFileDeleteConfirm('local', [...paths], names);
}

export function batchRemoteDelete() {
  const paths = Array.from(d().ls().selectedRemote.value);
  if (!paths.length) return;
  const session = d().getActiveSession();
  if (!session) {
    d().pc().announce(d().noSessionMessage('删除'), { level: 'warn' });
    return;
  }
  const names = paths.map(p => d().pc().remoteEntries.value.find(e => e.path === p)?.name || p);
  openFileDeleteConfirm('remote', paths, names);
}

/**
 * 删除结果播报：数字一律用【实际成功删除数】。弹窗期间面板刷新/切换资产后
 * remoteEntries/localEntries 里可能已找不到这些 path（实际删 0 个），照用户
 * 选中数播报会让用户误以为已删掉——有跳过项必须降级 warn 并说明原因。
 */
function announceDeleteResult(label: string, removed: number, skipped: number) {
  if (skipped > 0) {
    d().pc().announce(
      removed > 0
        ? '已删除' + label + ' ' + removed + ' 项，跳过 ' + skipped + ' 项（已不在当前列表中）'
        : '未删除任何项：' + skipped + ' 项均已不在当前列表中，请刷新后重试',
      { level: 'warn' }
    );
    return;
  }
  d().pc().announce('已删除' + label + ' ' + removed + ' 项', { level: 'success' });
}

export async function confirmFileDelete() {
  const pending = d().pc().pendingFileDelete.value;
  if (!pending) return;
  const { kind, paths, assetId } = pending;
  // 实际成功删除数由循环内累加（跳过项不算），用于播报真实结果
  let removed = 0;
  try {
    if (kind === 'remote') {
      // 归属校验：弹窗打开期间若切换了资产，这些 path 已属于另一台机器。
      // 两台存在同名绝对路径时会删错服务器，因此这里直接拒绝而不是"尽力而为"。
      if ((d().pc().wb().selectedAsset?.id ?? null) !== assetId) {
        d().pc().announce('已切换资产，取消删除以免误删其它服务器上的同名文件（请重新选择后删除）', { level: 'warn' });
        d().pc().pendingFileDelete.value = null;
        d().pc().wb().modal = { type: null };
        return;
      }
      const session = d().getActiveSession();
      if (!session) {
        d().pc().announce(d().noSessionMessage('删除'), { level: 'warn' });
        return;
      }
      await d().pc().withFileOperation('remote', '正在删除远程文件...', async () => {
        for (const path of paths) {
          const entry = d().pc().remoteEntries.value.find(e => e.path === path);
          if (!entry) continue;
          await invokeBackend('sftp_remove', { sessionId: session.sessionId, path, kind: entry.kind });
          removed += 1;
        }
        if (paths.length > 1) d().ls().selectedRemote.value = new Set();
        await d().refreshRemoteFiles(d().pc().remotePath.value);
      });
      announceDeleteResult('远程', removed, paths.length - removed);
    } else {
      await d().pc().withFileOperation('local', '正在删除本地文件...', async () => {
        for (const path of paths) {
          const entry = d().pc().localEntries.value.find(e => e.path === path);
          if (!entry) continue;
          await invokeBackend('fs_local_delete', { path, kind: entry.kind });
          removed += 1;
        }
        await d().refreshLocalFiles(d().pc().localPath.value);
      });
      announceDeleteResult('本地', removed, paths.length - removed);
    }
    d().pc().pendingFileDelete.value = null;
    d().pc().wb().modal = { type: null };
  } catch (error) {
    // 删除失败：保留 pending 与弹窗，用户可直接重试（不做静默失败）
    d().pc().announce('删除失败：' + errorMessage(error), { level: 'error' });
  }
}

export function cancelFileDelete() {
  d().pc().pendingFileDelete.value = null;
  d().pc().wb().modal = { type: null };
}
