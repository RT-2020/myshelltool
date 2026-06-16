<script setup>
/**
 * AssetGroupNode — 连接资产嵌套分组树的递归节点（Wave: 分组管理）
 *
 * 渲染单个分组节点：分组头（折叠/展开）+ 子分组（递归自身）+ 该组直属资产项。
 * 缩进按 depth 递增。分组头支持右键菜单（重命名/解散），资产项支持悬停快捷按钮
 * （编辑/删除）+ 右键菜单（编辑/复制/移动/删除）。
 *
 * 通过 provide/inject 从父级 ConnectionSidebar 获取：
 *   - statusClassFor(asset): 状态圆点 class
 *   - isActiveAsset(asset): 是否选中
 *   - assetBadge(asset): 徽标文本
 *   - isCollapsed(path): 分组是否折叠
 *   - onSelectAsset(id) / onConnectAsset(id)
 *   - onAssetKeydown(event, asset)
 *   - registerAssetEl(id, el): DOM ref 注册（键盘导航）
 *   - onAssetContextMenu(event, asset) / onGroupContextMenu(event, path)
 *   - onEditAsset(asset) / onDeleteAsset(asset) / onDuplicateAsset(asset): 悬停快捷按钮
 *   - isUngrouped(path): 是否为保留的「未分组」节点（无右键菜单）
 *
 * 这样避免了递归 emit 层层透传，组件树更简洁。
 */
import { inject } from 'vue';
import { ChevronDown, ChevronRight, Pencil, Trash2 } from 'lucide-vue-next';

defineOptions({ name: 'AssetGroupNode' });

const props = defineProps({
  node: { type: Object, required: true }, // { name, path, parent, children:[], items:[] }
  depth: { type: Number, default: 0 }
});

// 父级注入的 handler / state（见上方注释清单）
const sidebar = inject('connectionSidebar');
</script>

<template>
  <div class="group-node">
    <button
      type="button"
      class="group-header"
      :style="{ 'padding-inline-start': 8 + depth * 12 + 'px' }"
      :aria-expanded="String(!sidebar.isCollapsed(node.path))"
      @click="sidebar.toggleGroup(node.path)"
      @contextmenu.prevent="sidebar.onGroupContextMenu($event, node.path)"
    >
      <component
        :is="sidebar.isCollapsed(node.path) ? ChevronRight : ChevronDown"
        :size="12"
        class="group-chevron"
      />
      <span class="group-name">{{ node.name }}</span>
      <span class="group-count">{{ node.items.length + node.children.length }}</span>
    </button>

    <div v-show="!sidebar.isCollapsed(node.path)" class="group-body">
      <!-- 递归子分组 -->
      <AssetGroupNode
        v-for="child in node.children"
        :key="child.path"
        :node="child"
        :depth="depth + 1"
      />
      <!-- 该组直属资产项 -->
      <ul v-if="node.items.length" class="group-items" role="group">
        <li
          v-for="asset in node.items"
          :key="asset.id"
          :ref="el => sidebar.registerAssetEl(asset.id, el)"
          class="asset-node"
          :class="{ 'is-active': sidebar.isActiveAsset(asset) }"
          role="treeitem"
          tabindex="0"
          :aria-selected="String(sidebar.isActiveAsset(asset))"
          :title="`${asset.name} · ${asset.host} · ${asset.username}`"
          :style="{ 'padding-inline-start': 8 + depth * 12 + 'px' }"
          @click="sidebar.onSelectAsset(asset.id)"
          @dblclick="sidebar.onConnectAsset(asset.id)"
          @keydown="sidebar.onAssetKeydown($event, asset)"
          @contextmenu.prevent="sidebar.onAssetContextMenu($event, asset)"
        >
          <span class="dot" :class="sidebar.statusClass(asset)" aria-hidden="true"></span>
          <div class="asset-body">
            <div class="asset-name">{{ asset.name }}</div>
            <div class="asset-meta">{{ asset.host }} · {{ asset.username }}</div>
          </div>
          <span v-if="sidebar.assetBadge(asset)" class="asset-badge">{{ sidebar.assetBadge(asset) }}</span>
          <!-- 悬停快捷按钮：编辑 / 删除 -->
          <div class="asset-quick-actions">
            <button
              type="button"
              class="quick-btn"
              title="编辑"
              tabindex="-1"
              @click.stop="sidebar.onEditAsset(asset)"
            >
              <Pencil :size="13" />
            </button>
            <button
              type="button"
              class="quick-btn quick-btn--danger"
              title="删除"
              tabindex="-1"
              @click.stop="sidebar.onDeleteAsset(asset)"
            >
              <Trash2 :size="13" />
            </button>
          </div>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.group-node {
  display: flex;
  flex-direction: column;
  margin-block-end: var(--space-1);
}

.group-header {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  width: 100%;
  padding-block: var(--space-1);
  padding-inline-end: var(--space-2);
  border: none;
  background: transparent;
  color: var(--app-muted);
  cursor: pointer;
  text-align: start;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  font-size: var(--text-xs);
  font-weight: 600;
  border-radius: var(--radius-sm);
  transition: color var(--motion-fast) var(--ease-standard),
    background var(--motion-fast) var(--ease-standard);
}

.group-header:hover {
  color: var(--app-strong);
  background: var(--app-hover);
}

.group-chevron {
  flex: 0 0 auto;
  color: var(--app-subtle);
}

.group-name {
  flex: 1 1 auto;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.group-count {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 16px;
  padding: 0 var(--space-1);
  background: var(--app-control);
  color: var(--app-muted);
  border-radius: var(--radius-pill);
  font-size: 10px;
  font-weight: 500;
  font-variant-numeric: tabular-nums;
}

.group-items {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
}

// ============================================================
// Asset node — 与 ConnectionSidebar 原有样式保持一致
// ============================================================
.asset-node {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding-block: var(--space-1);
  padding-inline-end: var(--space-2);
  margin-block-end: 1px;
  border-radius: var(--radius-sm);
  border-inline-start: 2px solid transparent;
  cursor: pointer;
  position: relative;
  transition: background var(--motion-fast) var(--ease-standard),
    border-color var(--motion-fast) var(--ease-standard);
}

.asset-node:hover {
  background: var(--app-hover);
}

.asset-node:focus-visible {
  outline: none;
  background: var(--app-hover);
  border-inline-start-color: var(--accent);
}

.asset-node.is-active {
  background: var(--app-hover);
  border-inline-start-color: var(--accent);
}

.asset-body {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.asset-name {
  font-size: var(--text-sm);
  color: var(--app-strong);
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.asset-meta {
  font-size: var(--text-xs);
  color: var(--app-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono);
}

.asset-badge {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  height: 16px;
  padding: 0 var(--space-1);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-pill);
  background: var(--app-control);
  color: var(--app-muted);
  font-size: 10px;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  max-width: 64px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

// .dot 全局工具类；modifier 配对 _utilities.scss
.dot {
  flex: 0 0 auto;
  width: 8px;
  height: 8px;
  border-radius: var(--radius-pill);
  background: var(--app-subtle);
}
.dot.running {
  background: var(--success);
  box-shadow: 0 0 0 2px color-mix(in oklab, var(--success), transparent 72%);
}
.dot.warn {
  background: var(--warn);
  box-shadow: 0 0 0 2px color-mix(in oklab, var(--warn), transparent 72%);
}

// ============================================================
// 悬停快捷按钮 — 默认隐藏，hover/focus-within 显示
// ============================================================
.asset-quick-actions {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  gap: 2px;
  opacity: 0;
  transition: opacity var(--motion-fast) var(--ease-standard);
}

.asset-node:hover .asset-quick-actions,
.asset-node:focus-within .asset-quick-actions {
  opacity: 1;
}

.quick-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--app-muted);
  cursor: pointer;
  padding: 0;
  transition: background var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard),
    border-color var(--motion-fast) var(--ease-standard);
}

.quick-btn:hover {
  background: var(--app-control);
  color: var(--app-strong);
  border-color: var(--app-border);
}

.quick-btn--danger:hover {
  color: var(--danger);
  border-color: color-mix(in oklab, var(--danger), transparent 60%);
}

.quick-btn:focus-visible {
  outline: none;
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}
</style>
