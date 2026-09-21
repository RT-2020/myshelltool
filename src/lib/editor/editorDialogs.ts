/**
 * editorDialogs — 编辑器专属的系统对话框封装。
 *
 * openLocalSaveDialog：本地「另存为」（plugin:dialog|save，capabilities 已
 * 补 dialog:allow-save）。返回所选目标完整路径；用户取消返回 null。
 */
import { invokeBackend } from '@/services/backend';

export async function openLocalSaveDialog(defaultFileName: string): Promise<string | null> {
  const target = await invokeBackend<string | null>('plugin:dialog|save', {
    options: {
      title: '另存为',
      defaultPath: defaultFileName
    }
  });
  return target || null;
}
