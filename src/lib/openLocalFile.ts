/**
 * openLocalFile — 打开本地文件/文件夹与「在资源管理器中显示」。
 *
 * 为什么需要它：下载完成的 toast 需要「打开」（系统默认程序打开文件）与
 * 「所在文件夹」（资源管理器定位高亮）两个动作，收敛于此（与 openExternal
 * 的外链场景分工）。权限事实（tauri-plugin-opener 2.5.4 permissions/default.toml）：
 * `opener:default` 已含 allow-open-url / allow-reveal-item-in-dir，但
 * **不含 allow-open-path**——capabilities/default.json 已显式补齐。
 *
 * 浏览器预览模式（npm run dev，无 Tauri runtime）没有本地文件系统能力：
 * 返回 'unsupported' 让调用方给出降级提示，而不是抛错。
 * Tauri 下 opener 失败（路径不存在/权限等）会 throw，由调用方 catch 呈现原因。
 *
 * 安全注记：openPath = ShellExecute，用系统默认程序打开——下载的 .exe 会真的
 * 执行（与浏览器下载条行为一致）；「所在文件夹」（reveal）只定位不执行。
 */
import { isTauriRuntime } from '@/services/backend';

export type LocalOpenResult = 'ok' | 'unsupported';

/** 用系统默认程序打开文件或文件夹（文件夹则在资源管理器中打开该目录）。 */
export async function openLocalPath(path: string): Promise<LocalOpenResult> {
  if (!isTauriRuntime()) return 'unsupported';
  const { openPath } = await import('@tauri-apps/plugin-opener');
  await openPath(path);
  return 'ok';
}

/** 在资源管理器中显示并选中该文件（不动文件本体）。 */
export async function revealLocalItem(path: string): Promise<LocalOpenResult> {
  if (!isTauriRuntime()) return 'unsupported';
  const { revealItemInDir } = await import('@tauri-apps/plugin-opener');
  await revealItemInDir(path);
  return 'ok';
}
