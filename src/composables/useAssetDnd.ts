/**
 * useAssetDnd — 侧栏资产/分组的拖拽状态机（从 ConnectionSidebar 抽出，
 * architecture-log Baseline 预案落地）。
 *
 * 覆盖三条拖拽链路（行为与迁移前逐句一致）：
 * 1. **资产拖入分组**：任意分组（含「未分组」）可接收 → 回调 `onMoveAssetDirect`
 *    （内部消费置 internalDropHandled，dragend 不再判「拖出开窗」）；
 * 2. **分组同级排序**：同 parent 才可排、不可排到自己/「未分组」，上半/下半区
 *    决定 before/after → 回调 `onReorderGroups`（全量新顺序，父在子前 DFS 先序）；
 * 3. **资产拖出主窗口边界释放** → 开独立资产窗口（回调 `openDetached`）。
 *    判定 = 坐标在视口外 ∥ 拖拽期间离开过视口（dragend 坐标在窗外释放时不可靠，
 *    dragleave 标记是可靠信号，详见 useDragOutsideViewport）。已知边界：Esc 取消
 *    的「窗外拖拽」也可能误开窗（v1 接受，待实测）。
 *
 * dataTransfer 契约：自定义 MIME `application/x-myshelltool-drag`（JSON）+
 * text/plain 兜底（部分 webview 对自定义 MIME 支持不稳）。
 * store-agnostic：组件侧以回调接线（emit / openAssetWindow），本文件不触 store。
 */
import { ref } from 'vue';
import { useDragOutsideViewport } from '@/composables/useDragOutsideViewport';
import { isTauriRuntime } from '@/services/backend';
import type { GroupTreeNode, NormalizedConnectionAsset } from '@/types/domain';

const DRAG_MIME = 'application/x-myshelltool-drag';

/** dataTransfer 自定义 MIME 的载荷（readDragData/writeDragData 的契约）。 */
export type DragPayload =
  | { kind: 'asset'; id: string }
  | { kind: 'group'; path: string; parent: string };

export interface UseAssetDndOptions {
  /** 全量资产（拖出开窗时按 id 找回完整资产对象；取值时读，保持响应新鲜） */
  getAssets: () => NormalizedConnectionAsset[];
  /** 分组树根（排序结果按它的 DFS 先序重建） */
  getGroupedAssets: () => GroupTreeNode;
  /** 资产拖入分组（组件侧 emit move-asset-direct） */
  onMoveAssetDirect: (payload: { id: string; group: string }) => void;
  /** 分组同级排序（组件侧 emit reorder-groups） */
  onReorderGroups: (paths: string[]) => void;
  /** 拖出主窗口释放（组件侧 openAssetWindow + 失败 toast） */
  openDetached: (asset: NormalizedConnectionAsset) => void;
}

export function useAssetDnd(options: UseAssetDndOptions) {
  // 拖拽态：当前被拖对象 / 当前悬停目标分组 / 上半还是下半区
  const dragSource = ref<DragPayload | null>(null);
  const dropTarget = ref<{ path: string; position: 'in' | 'before' | 'after' } | null>(null);
  // 内部 drop 消费标记：分组移动 / 分组排序 drop 命中（onGroupDrop 的回调处）置 true，
  // dragend 据此跳过「拖出主窗口开独立窗口」判定；每次 dragend 结束时复位，不残留
  let internalDropHandled = false;
  // 拖拽期间指针是否离开过 webview 视口（窗外释放判定，见文件头注释）
  const dragOutside = useDragOutsideViewport();

  function readDragData(event: DragEvent): DragPayload | null {
    const raw = event.dataTransfer?.getData(DRAG_MIME);
    if (!raw) return null;
    try { return JSON.parse(raw) as DragPayload; } catch { return null; }
  }
  function writeDragData(event: DragEvent, payload: DragPayload) {
    // 兜底 text/plain：某些环境（部分 webview）对自定义 MIME 支持不稳
    event.dataTransfer?.setData(DRAG_MIME, JSON.stringify(payload));
    event.dataTransfer?.setData('text/plain', payload.kind === 'asset' ? payload.id : payload.path);
    // dragstart/dragover 事件的 dataTransfer 恒非空（DOM 契约），非空断言仅类型层收窄
    event.dataTransfer!.effectAllowed = 'move';
  }

  // —— 资产拖拽 source ——
  function onAssetDragStart(event: DragEvent, asset: NormalizedConnectionAsset) {
    dragSource.value = { kind: 'asset', id: asset.id };
    writeDragData(event, { kind: 'asset', id: asset.id });
    // 挂 document 级视口跟踪（仅资产拖拽需要，分组拖拽不开独立窗口）
    dragOutside.attach();
  }
  function onAssetDragEnd(event: DragEvent) {
    const src = dragSource.value; // 内部 drop 路径（onGroupDrop）会先清 dragSource，故判定前快照语义等价
    const openDetached =
      !internalDropHandled &&
      src?.kind === 'asset' &&
      isTauriRuntime() &&
      dragOutside.isOutside(event);
    dragSource.value = null;
    dropTarget.value = null;
    internalDropHandled = false;
    dragOutside.detach();
    if (!openDetached) return;
    // 类型断言：openDetached 为 true 蕴含 src 非 null 且 kind === 'asset'（上方判定式）
    const asset = options.getAssets().find(a => a.id === (src as { id: string }).id);
    if (asset) options.openDetached(asset);
  }
  function isDraggingAsset(id: string) {
    return dragSource.value?.kind === 'asset' && dragSource.value.id === id;
  }

  // —— 分组拖拽 source（排序）——
  function onGroupDragStart(event: DragEvent, path: string, parent: string) {
    if (path === '未分组') { event.preventDefault(); return; } // 保留节点不可拖
    dragSource.value = { kind: 'group', path, parent };
    writeDragData(event, { kind: 'group', path, parent });
  }
  function onGroupDragEnd(event: DragEvent) {
    dragSource.value = null;
    dropTarget.value = null;
    internalDropHandled = false; // 分组拖拽同样复位，避免残留毒化下一次资产拖出判定
  }

  // —— 分组头作为 drop target（同时收资产拖入与分组排序）——
  function onGroupDragOver(event: DragEvent, path: string, parent: string) {
    const src = readDragData(event) || dragSource.value;
    if (!src) return;
    // 资产拖入：任意分组都可接收（含「未分组」），effectAllowed=move 需配 dropEffect
    if (src.kind === 'asset') {
      event.dataTransfer!.dropEffect = 'move';
      event.preventDefault();
      return;
    }
    // 分组排序：仅同级可排（同 parent），且不能排到自己上、不能排到「未分组」
    if (src.kind === 'group') {
      if (path === '未分组' || src.path === path || src.parent !== parent) return;
      event.dataTransfer!.dropEffect = 'move';
      event.preventDefault();
    }
  }
  function onGroupDragEnter(event: DragEvent, path: string) {
    const src = readDragData(event) || dragSource.value;
    if (!src) return;
    if (src.kind === 'asset') {
      dropTarget.value = { path, position: 'in' };
    } else if (src.kind === 'group' && path !== '未分组' && src.path !== path) {
      // 上半/下半区决定 before/after
      const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
      const position = event.clientY < rect.top + rect.height / 2 ? 'before' : 'after';
      dropTarget.value = { path, position };
    }
  }
  function onGroupDragLeave(event: DragEvent, path: string) {
    // dragleave 会因子元素冒泡频繁触发；仅在真正离开该分组头时清目标
    if (dropTarget.value?.path === path) {
      const related = event.relatedTarget;
      if (!(event.currentTarget as HTMLElement).contains(related as Node | null)) {
        dropTarget.value = null;
      }
    }
  }
  function onGroupDrop(event: DragEvent, targetPath: string, targetParent: string) {
    const src = readDragData(event) || dragSource.value;
    dropTarget.value = null;
    dragSource.value = null;
    if (!src) return;
    if (src.kind === 'asset') {
      // 资产拖入分组 → 直接移动（内部消费：dragend 不再判「拖出开窗」）
      internalDropHandled = true;
      options.onMoveAssetDirect({ id: src.id, group: targetPath });
      return;
    }
    if (src.kind === 'group') {
      // 同级排序：把 src.path 插到 targetPath 的 before/after
      if (targetPath === '未分组' || src.path === targetPath || src.parent !== targetParent) return;
      internalDropHandled = true;
      const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
      const placeAfter = event.clientY >= rect.top + rect.height / 2;
      options.onReorderGroups(buildReorderedPaths(src.path, targetPath, placeAfter));
    }
  }

  // 把分组拖拽结果转成全量新顺序（扁平路径，父在子前 DFS 先序）。
  // 思路：从当前分组树按 DFS 先序收集所有非「未分组」路径，移除 src，
  // 插到 target 的 before/after 位置。
  function buildReorderedPaths(srcPath: string, targetPath: string, placeAfter: boolean): string[] {
    const ordered = collectGroupPathsDfs(options.getGroupedAssets());
    const filtered = ordered.filter(p => p !== srcPath);
    const idx = filtered.indexOf(targetPath);
    if (idx === -1) return ordered; // 兜底：target 不在列表，原样返回
    filtered.splice(placeAfter ? idx + 1 : idx, 0, srcPath);
    return filtered;
  }
  // DFS 先序收集分组路径（root.children 起步），跳过「未分组」。
  function collectGroupPathsDfs(root: GroupTreeNode): string[] {
    const out: string[] = [];
    const walk = (node: GroupTreeNode) => {
      for (const child of (node.children || [])) {
        if (child.path !== '未分组') out.push(child.path);
        walk(child);
      }
    };
    walk(root);
    return out;
  }

  // 分组头拖放态 class（AssetGroupNode 调用）
  function groupHeaderClass(path: string) {
    if (!dropTarget.value || dropTarget.value.path !== path) {
      return dragSource.value?.kind === 'group' && dragSource.value.path === path ? 'is-dragging' : '';
    }
    const pos = dropTarget.value.position;
    if (pos === 'in') return 'is-drop-in';
    if (pos === 'before') return 'is-drop-before';
    if (pos === 'after') return 'is-drop-after';
    return '';
  }

  return {
    dragSource,
    dropTarget,
    onAssetDragStart,
    onAssetDragEnd,
    isDraggingAsset,
    onGroupDragStart,
    onGroupDragEnd,
    onGroupDragOver,
    onGroupDragEnter,
    onGroupDragLeave,
    onGroupDrop,
    groupHeaderClass,
  };
}
