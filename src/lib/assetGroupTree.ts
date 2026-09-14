/**
 * buildGroupTree — 资产 → 分组树构建（含空分组占位，「未分组」永远首位）
 * （从 assets store 抽出，v2.8 第五轮；纯函数）。
 */
import type { GroupTreeNode, NormalizedConnectionAsset } from '@/types/domain';

export function buildGroupTree(assetList: NormalizedConnectionAsset[], groupList?: string[] | null): GroupTreeNode {
  const root: GroupTreeNode = { name: '', path: '', parent: '', children: [], items: [] };
  const ungrouped: GroupTreeNode = { name: '未分组', path: '未分组', parent: '', children: [], items: [] };
  const nodeMap = new Map<string, GroupTreeNode>(); // path -> node（不含未分组，未分组单独管理）
  const insertionOrder: string[] = []; // 记录 path 首次出现顺序（含中间隐式节点）

  // 收集所有路径，按首次出现顺序保留
  const seenPaths = new Set<string>();
  const orderedPaths: string[] = [];
  for (const p of groupList || []) {
    if (p && p !== '未分组' && !seenPaths.has(p)) { seenPaths.add(p); orderedPaths.push(p); }
  }
  for (const asset of assetList) {
    const p = asset.group && asset.group !== '未分组' ? asset.group : '';
    if (p && !seenPaths.has(p)) { seenPaths.add(p); orderedPaths.push(p); }
  }

  function ensureNode(path: string): GroupTreeNode {
    if (path === '未分组') return ungrouped;
    const existing = nodeMap.get(path);
    if (existing) return existing;
    const slashIdx = path.lastIndexOf('/');
    const parentPath = slashIdx === -1 ? '' : path.slice(0, slashIdx);
    const name = slashIdx === -1 ? path : path.slice(slashIdx + 1);
    const parent = parentPath ? ensureNode(parentPath) : root;
    const node: GroupTreeNode = { name, path, parent: parentPath, children: [], items: [] };
    parent.children.push(node);
    nodeMap.set(path, node);
    insertionOrder.push(path);
    return node;
  }

  // 先建所有声明路径的节点骨架（保证空分组可见、层级顺序稳定）
  for (const p of orderedPaths) ensureNode(p);

  // 把 asset 挂到对应节点
  for (const asset of assetList) {
    const g = asset.group || '未分组';
    if (g === '未分组') {
      ungrouped.items.push(asset);
    } else {
      // 若 asset.group 是声明路径中未出现过的（理论不会，上面已收集），兜底建节点
      const node = ensureNode(g);
      node.items.push(asset);
    }
  }

  // 未分组置于根 children 末尾（且仅在有内容时保留）
  if (ungrouped.items.length || ungrouped.children.length) {
    root.children.push(ungrouped);
  }
  return root;
}
