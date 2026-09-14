/**
 * useSidebarContextMenu — 侧栏右键菜单（资产/分组两套，从 ConnectionSidebar
 * 抽出，拆分第二刀）。
 *
 * 状态契约：共用一个 contextMenu ref（kind 区分资产/分组），open 定位到
 * event.clientX/Y，close 保留 x/y 只清 visible（菜单组件按 kind 选
 * assetMenuItems / groupMenuItems 渲染）。「未分组」是保留节点，不给分组菜单。
 *
 * store-agnostic：动作经回调接线（组件侧 emit / openAssetWindow）。
 */
import { computed, ref } from 'vue';
import { isTauriRuntime } from '@/services/backend';
import type { NormalizedConnectionAsset } from '@/types/domain';

export interface SidebarMenuState {
  visible: boolean;
  kind: '' | 'asset' | 'group';
  x: number;
  y: number;
  asset: NormalizedConnectionAsset | null;
  path: string;
}

export interface SidebarContextMenuHandlers {
  onEditAsset: (asset: NormalizedConnectionAsset) => void;
  onDuplicateAsset: (asset: NormalizedConnectionAsset) => void;
  onMoveAsset: (asset: NormalizedConnectionAsset) => void;
  onDeleteAsset: (asset: NormalizedConnectionAsset) => void;
  onRenameGroup: (path: string) => void;
  onDissolveGroup: (path: string) => void;
  /** 在独立窗口打开（组件侧 openAssetWindow + 失败 toast） */
  openDetached: (asset: NormalizedConnectionAsset) => void;
}

export function useSidebarContextMenu(handlers: SidebarContextMenuHandlers) {
  const contextMenu = ref<SidebarMenuState>({ visible: false, kind: '', x: 0, y: 0, asset: null, path: '' });

  function openContextMenu(event: MouseEvent, kind: 'asset' | 'group', payload: { asset?: NormalizedConnectionAsset; path?: string }) {
    contextMenu.value = {
      visible: true,
      kind,
      x: event.clientX,
      y: event.clientY,
      asset: payload.asset || null,
      path: payload.path || ''
    };
  }

  function onAssetContextMenu(event: MouseEvent, asset: NormalizedConnectionAsset) {
    openContextMenu(event, 'asset', { asset });
  }

  function onGroupContextMenu(event: MouseEvent, path: string) {
    // 「未分组」是保留节点，不提供分组管理菜单
    if (path === '未分组') return;
    openContextMenu(event, 'group', { path });
  }

  function closeContextMenu() {
    contextMenu.value = { ...contextMenu.value, visible: false };
  }

  const assetMenuItems = computed(() => {
    if (!contextMenu.value.visible || contextMenu.value.kind !== 'asset') return [];
    const a = contextMenu.value.asset;
    if (!a) return [];
    const make = (label: string, fn: () => void, opts: { danger?: boolean } = {}) => ({ label, action: fn, ...opts });
    return [
      make('编辑', () => handlers.onEditAsset(a)),
      make('复制', () => handlers.onDuplicateAsset(a)),
      { separator: true },
      make('移动到分组…', () => handlers.onMoveAsset(a)),
      // 独立窗口入口：仅 Tauri runtime 显示（浏览器预览无多窗口能力）
      ...(isTauriRuntime()
        ? [{ separator: true }, make('在独立窗口打开', () => handlers.openDetached(a))]
        : []),
      { separator: true },
      make('删除', () => handlers.onDeleteAsset(a), { danger: true })
    ];
  });

  const groupMenuItems = computed(() => {
    if (!contextMenu.value.visible || contextMenu.value.kind !== 'group') return [];
    const path = contextMenu.value.path;
    if (!path) return [];
    const make = (label: string, fn: () => void, opts: { danger?: boolean } = {}) => ({ label, action: fn, ...opts });
    return [
      make('重命名…', () => handlers.onRenameGroup(path)),
      { separator: true },
      make('解散分组', () => handlers.onDissolveGroup(path), { danger: true })
    ];
  });

  return {
    contextMenu,
    onAssetContextMenu,
    onGroupContextMenu,
    closeContextMenu,
    assetMenuItems,
    groupMenuItems
  };
}
