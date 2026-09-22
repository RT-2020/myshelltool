/**
 * filePanelLocal — 文件面板本地侧目录操作（v0.20 自 filePanel 拆出，S2 的
 * 一刀；逻辑原样迁移：refreshLocal/navigate/mkdir/rename/视图切换）。
 * ctx 与 filePanel 共享同一 FilePanelContext（bindFilePanelContext 级联注入），
 * 调用方仍经 filePanel 的 re-export 取函数（路径不变零改动）。
 */
import type { LocalDirectoryListResult } from '@/types/domain';
import { invokeBackend, isTauriRuntime } from '@/services/backend';
import { joinLocalPath, parentLocalPath } from '@/stores/workbench';
import { errorMessage } from '@/lib/errorMessage';
import type { FilePanelContext } from '@/lib/filePanel';

let ctx: FilePanelContext | null = null;

/** filePanel.bindFilePanelContext 级联注入（同一 ctx 实例，不单独 bind）。 */
export function bindFilePanelLocal(context: FilePanelContext): void {
  ctx = context;
}

function pc(): FilePanelContext {
  if (!ctx) throw new Error('filePanelLocal: context not bound');
  return ctx;
}

export async function refreshLocalFiles(path: string | null = null) {
  if (!isTauriRuntime()) {
    pc().announce('本地浏览需要桌面客户端（npm run tauri:dev）', { level: 'warn' });
    return;
  }
  try {
    await pc().withFileOperation('local', '正在读取本地目录...', async () => {
      const target = path !== null ? path : (pc().localPath.value || await invokeBackend<string>('fs_local_home_dir'));
      const result = await invokeBackend<LocalDirectoryListResult>('fs_local_list_dir', { path: target });
      pc().localPath.value = result.path;
      pc().localEntries.value = result.entries || [];
    });
  } catch (error) {
    pc().announce('本地目录读取失败：' + errorMessage(error), { level: 'error' });
  }
}

export async function navigateLocalPath(target: string) {
  if (!target) return;
  await refreshLocalFiles(target);
}

export async function navigateLocalUp() {
  if (!pc().localPath.value) return;
  try {
    const result = await pc().withFileOperation('local', '正在读取本地目录...', () =>
      invokeBackend<LocalDirectoryListResult>('fs_local_list_dir', { path: pc().localPath.value })
    );
    if (!result.parent || result.parent === pc().localPath.value) {
      pc().announce('已是根目录');
      return;
    }
    await refreshLocalFiles(result.parent);
  } catch (error) {
    pc().announce('返回上级失败：' + errorMessage(error));
  }
}

export async function localMkdir(name: string) {
  if (!name?.trim()) {
    pc().announce('目录名不能为空', { level: 'warn' });
    return;
  }
  try {
    await pc().withFileOperation('local', '正在创建本地目录...', async () => {
      const target = joinLocalPath(pc().localPath.value, name.trim());
      await invokeBackend('fs_local_mkdir', { path: target });
      await refreshLocalFiles(pc().localPath.value);
    });
    pc().announce('已创建本地目录：' + name.trim(), { level: 'success' });
  } catch (error) {
    pc().announce('创建本地目录失败：' + errorMessage(error), { level: 'error' });
    throw error;
  }
}

export async function localRename(oldPath: string, newName: string) {
  if (!newName?.trim()) {
    pc().announce('新名称不能为空', { level: 'warn' });
    return;
  }
  try {
    await pc().withFileOperation('local', '正在重命名本地文件...', async () => {
      const newPath = joinLocalPath(parentLocalPath(oldPath), newName.trim());
      await invokeBackend('fs_local_rename', { oldPath, newPath });
      await refreshLocalFiles(pc().localPath.value);
    });
    pc().announce('已重命名：' + newName.trim(), { level: 'success' });
  } catch (error) {
    pc().announce('重命名失败：' + errorMessage(error), { level: 'error' });
    throw error;
  }
}

export function setLocalViewMode(mode: string) {
  if (mode !== 'queue' && mode !== 'browser') return;
  pc().localViewMode.value = mode;
  if (mode === 'browser' && !pc().localEntries.value.length) refreshLocalFiles().catch(() => null);
}
