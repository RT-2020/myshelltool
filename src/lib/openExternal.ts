/**
 * openExternal — 在系统默认浏览器中打开外链。
 *
 * 为什么需要它：此前 SettingsPanelContent / UpdateSection 各自内联同一份
 * 实现（「同一概念多份实现」红线），更新日志区块是第三个使用点，收敛于此。
 * Tauri runtime 下走 plugin-opener（系统默认浏览器）；浏览器预览模式
 * （npm run dev，无 Tauri runtime）回落 window.open，不报错。
 */

import { isTauriRuntime } from '@/services/backend';

export async function openExternal(url: string) {
  if (!isTauriRuntime()) {
    window.open(url, '_blank', 'noopener');
    return;
  }
  try {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
  } catch (e) {
    // opener 失败兜底（权限缺失等），至少不让用户卡死
    console.warn('[openExternal] opener failed, fallback to window.open:', e);
    window.open(url, '_blank', 'noopener');
  }
}
