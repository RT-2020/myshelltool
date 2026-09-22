/**
 * editorRemoteTargets — 编辑器「另存为 / 上传到远端」流程（v0.20 自
 * editorFlows 拆出，S2 的一刀；逻辑原样迁移）。两者共用「远端目标已存在 →
 * 覆盖确认 → performWrite」骨架；本地另存为走系统保存对话框 + probeLocalExists
 * 覆盖确认。依赖（fc/findTab/probe/performWrite）经 bindEditorTargets 级联
 * 注入，避免与 editorFlows 成值 import 环。
 */
import type { EditorTab } from '@/types/editor';
import type { EditorDialogPayload } from '@/types/editor';

export interface EditorTargetDeps {
  fc(): {
    remotePathPrefix(): string;
    selectedAssetId(): string | null;
    announce(message: string, opts?: { level?: string }): unknown;
    openDialog(payload: EditorDialogPayload): void;
  };
  findTab(tabId: string): EditorTab | undefined;
  probeRemoteExists(path: string, assetId: string | null): Promise<'yes' | 'no' | 'error'>;
  performWrite(tab: EditorTab, opts: Record<string, unknown>): Promise<boolean>;
}

let deps: EditorTargetDeps | null = null;

/** editorFlows.bindEditorFlows 级联注入。 */
export function bindEditorTargets(d: EditorTargetDeps): void {
  deps = d;
}

function t(): EditorTargetDeps {
  if (!deps) throw new Error('editorRemoteTargets: deps not bound');
  return deps;
}

export function openSaveAsDialog(tab: EditorTab): void {
  if (tab.target.kind === 'local') {
    void saveLocalAsViaDialog(tab);
    return;
  }
  const suggested = `${t().fc().remotePathPrefix()}/${tab.name}`.replace(/\/+/g, '/');
  t().fc().openDialog({
    title: '另存为（远端）',
    message: '输入远端绝对路径（保存到新路径；已存在时将请求确认）。',
    input: { label: '远端路径', placeholder: '/etc/myshelltool/example.conf', value: suggested },
    buttons: [
      { label: '取消' },
      {
        label: '保存',
        primary: true,
        action: async (inputValue?: string) => {
          const path = (inputValue || '').trim();
          if (!path.startsWith('/')) return '请输入以 / 开头的远端绝对路径';
          const exists = await t().probeRemoteExists(path, tab.target.assetId);
          if (exists === 'error') return '无法确认目标状态（会话或网络异常），请重试';
          if (exists === 'yes') {
            // 单弹窗槽位：直接替换为覆盖确认（返回 null 视作无内联错误）
            t().fc().openDialog({
              title: '目标已存在',
              message: `远端已存在 ${path}，覆盖将替换其全部内容。`,
              buttons: [
                { label: '取消' },
                {
                  label: '覆盖保存',
                  danger: true,
                  primary: true,
                  action: () => {
                    void t().performWrite(tab, { newPath: path, targetKind: 'remote' });
                  }
                }
              ]
            });
            return null;
          }
          await t().performWrite(tab, { newPath: path, targetKind: 'remote' });
        }
      }
    ]
  });
}

async function saveLocalAsViaDialog(tab: EditorTab): Promise<void> {
  try {
    const { openLocalSaveDialog } = await import('@/lib/editor/editorDialogs');
    const target = await openLocalSaveDialog(tab.name);
    if (!target) return;
    const { probeLocalExists } = await import('@/lib/editor/editorIo');
    const exists = await probeLocalExists(target);
    if (exists === 'yes') {
      t().fc().openDialog({
        title: '目标已存在',
        message: `本地已存在 ${target}，覆盖将替换其全部内容。`,
        buttons: [
          { label: '取消' },
          {
            label: '覆盖保存',
            danger: true,
            primary: true,
            action: () => {
              void t().performWrite(tab, { newPath: target, targetKind: 'local' });
            }
          }
        ]
      });
      return;
    }
    if (exists === 'error') {
      t().fc().announce('无法确认本地目标状态，已取消另存为', { level: 'warn' });
      return;
    }
    await t().performWrite(tab, { newPath: target, targetKind: 'local' });
  } catch (error) {
    t().fc().announce(`另存为失败：${(error as Error).message}`, { level: 'error' });
  }
}

/** 编辑本地文件 → 上传到远端（写到指定路径；目标资产=当前选中）。 */
export function uploadToRemote(tabId: string): void {
  const tab = t().findTab(tabId);
  if (!tab || tab.target.kind !== 'local') return;
  if (!t().fc().selectedAssetId()) {
    t().fc().announce('上传到远端需要先在侧栏选中并连接一个资产', { level: 'warn' });
    return;
  }
  const suggested = `${t().fc().remotePathPrefix()}/${tab.name}`.replace(/\/+/g, '/');
  t().fc().openDialog({
    title: '上传到远端',
    message: '把当前内容写入远端路径（会话为当前选中资产；已存在时将请求确认）。',
    input: { label: '远端路径', placeholder: '/tmp/example.conf', value: suggested },
    buttons: [
      { label: '取消' },
      {
        label: '上传',
        primary: true,
        action: async (inputValue?: string) => {
          const path = (inputValue || '').trim();
          if (!path.startsWith('/')) return '请输入以 / 开头的远端绝对路径';
          const assetId = t().fc().selectedAssetId();
          const exists = await t().probeRemoteExists(path, assetId);
          if (exists === 'error') return '无法确认目标状态（会话或网络异常），请重试';
          if (exists === 'yes') {
            t().fc().openDialog({
              title: '目标已存在',
              message: `远端已存在 ${path}，覆盖将替换其全部内容。`,
              buttons: [
                { label: '取消' },
                {
                  label: '覆盖上传',
                  danger: true,
                  primary: true,
                  action: () => {
                    void t().performWrite(tab, {
                      newPath: path,
                      targetKind: 'remote',
                      assetId
                    });
                  }
                }
              ]
            });
            return null;
          }
          await t().performWrite(tab, {
            newPath: path,
            targetKind: 'remote',
            assetId
          });
        }
      }
    ]
  });
}
