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
 *   - isDraggingAsset(id): 该资产是否正被拖拽（视觉半透明）
 *   - isCollapsed(path) / toggleGroup(path): 折叠态
 *   - isUngrouped(path): 是否为保留的「未分组」节点（不可拖动排序、无右键菜单）
 *   - groupHeaderClass(path): 分组头拖放态 class（is-dragging/is-drop-in/is-drop-before/after）
 *   - onSelectAsset(id) / onConnectAsset(id) / onAssetKeydown(event, asset)
 *   - registerAssetEl(id, el): DOM ref 注册（键盘导航）
 *   - onAssetContextMenu(event, asset) / onGroupContextMenu(event, path)
 *   - onEditAsset(asset) / onDeleteAsset(asset) / onDuplicateAsset(asset): 悬停快捷按钮
 *   - 拖拽：onAssetDragStart/End、onGroupDragStart/End、onGroupDragOver/Enter/Leave/Drop
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

function assetIndicatorClasses(asset) {
  // 返回全局 .dot 修饰符集（_utilities.scss）：connected / warn / idle
  const status = sidebar.statusClass(asset);
  if (status === 'running') return ['connected'];
  if (status === 'warn') return ['warn'];
  return ['idle'];
}

function assetStatusLabel(asset) {
  const cls = assetIndicatorClasses(asset);
  if (cls.includes('connected')) return '在线';
  if (cls.includes('warn')) return '警告';
  return '待命';
}
</script>

<template>
  <div class="sb-group group-node">
    <button
      type="button"
      class="sb-group-head group-header"
      :class="sidebar.groupHeaderClass(node.path)"
      :style="{ 'padding-inline-start': 8 + depth * 12 + 'px' }"
      :aria-expanded="String(!sidebar.isCollapsed(node.path))"
      :draggable="String(!sidebar.isUngrouped(node.path))"
      @click="sidebar.toggleGroup(node.path)"
      @contextmenu.prevent="sidebar.onGroupContextMenu($event, node.path)"
      @dragstart="sidebar.onGroupDragStart($event, node.path, node.parent)"
      @dragend="sidebar.onGroupDragEnd($event)"
      @dragover="sidebar.onGroupDragOver($event, node.path, node.parent)"
      @dragenter.prevent="sidebar.onGroupDragEnter($event, node.path)"
      @dragleave="sidebar.onGroupDragLeave($event, node.path)"
      @drop.prevent="sidebar.onGroupDrop($event, node.path, node.parent, node.name)"
    >
      <span class="sb-group-left">
        <component
          :is="sidebar.isCollapsed(node.path) ? ChevronRight : ChevronDown"
          :size="12"
          class="sb-group-chevron group-chevron"
        />
        <span class="sb-group-name group-name">{{ node.name }}</span>
      </span>
      <span class="sb-group-count group-count">{{ node.items.length + node.children.length }}</span>
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
      <ul v-if="node.items.length" class="sb-asset-list group-items" role="group">
        <li
          v-for="asset in node.items"
          :key="asset.id"
          :ref="el => sidebar.registerAssetEl(asset.id, el)"
          class="sb-asset asset-node"
          :class="{ active: sidebar.isActiveAsset(asset), 'is-active': sidebar.isActiveAsset(asset), 'is-dragging': sidebar.isDraggingAsset(asset.id) }"
          role="treeitem"
          tabindex="0"
          draggable="true"
          :aria-selected="String(sidebar.isActiveAsset(asset))"
          :title="`${asset.name} · ${asset.host} · ${asset.username}`"
          :style="{ 'padding-inline-start': 8 + depth * 12 + 'px' }"
          @click="sidebar.onSelectAsset(asset.id)"
          @dblclick="sidebar.onConnectAsset(asset.id)"
          @keydown="sidebar.onAssetKeydown($event, asset)"
          @contextmenu.prevent="sidebar.onAssetContextMenu($event, asset)"
          @dragstart="sidebar.onAssetDragStart($event, asset)"
          @dragend="sidebar.onAssetDragEnd($event)"
        >
          <span class="sb-asset-indicator dot" :class="assetIndicatorClasses(asset)" aria-hidden="true"></span>
          <div class="sb-asset-body asset-body">
            <div class="sb-asset-title asset-name">{{ asset.name }}</div>
            <div class="sb-asset-meta asset-meta">{{ asset.host }} · {{ asset.username }}</div>
          </div>
          <!-- 悬停快捷按钮：编辑 / 删除 -->
          <div class="asset-trail">
            <span class="sb-asset-tag">{{ assetStatusLabel(asset) }}</span>
            <div class="asset-quick-actions">
              <button
                type="button"
                class="quick-btn"
                title="编辑"
                aria-label="编辑连接"
                @click.stop="sidebar.onEditAsset(asset)"
              >
                <Pencil :size="13" />
              </button>
              <button
                type="button"
                class="quick-btn quick-btn--danger"
                title="删除"
                aria-label="删除连接"
                @click.stop="sidebar.onDeleteAsset(asset)"
              >
                <Trash2 :size="13" />
              </button>
            </div>
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
  justify-content: space-between;
  gap: 12px;
  width: 100%;
  min-height: 34px;
  padding-block: 0;
  padding-inline-end: 8px;
  border: none;
  background: transparent;
  color: var(--app-text);
  cursor: pointer;
  text-align: start;
  text-transform: none;
  letter-spacing: 0;
  font-size: 12.5px;
  font-weight: 600;
  border-radius: 8px;
  transition: color var(--motion-fast) var(--ease-standard),
    background var(--motion-fast) var(--ease-standard);
}

.group-header:hover {
  color: var(--app-strong);
  background: var(--app-hover);
}

.sb-group-left {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.group-chevron {
  flex: 0 0 auto;
  color: var(--app-subtle);
  transition: transform var(--motion-fast) var(--ease-standard);
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
  background: var(--app-panel-2);
  color: var(--app-muted);
  border-radius: var(--radius-pill);
  font: 11px var(--font-mono);
  font-weight: 500;
  font-variant-numeric: tabular-nums;
}

.group-items {
  list-style: none;
  margin: 6px 0 0 18px;
  padding: 0 0 0 2px;
  display: grid;
  gap: 4px;
  border-left: 1px solid var(--app-border-soft);
}

// ============================================================
// Asset node — 与 ConnectionSidebar 原有样式保持一致
// ============================================================
.asset-node {
  display: grid;
  grid-template-columns: 10px minmax(0, 1fr) auto;
  align-items: center;
  gap: 10px;
  min-height: 44px;
  padding: 8px 8px 8px 10px;
  margin-block-end: 1px;
  border-radius: 10px;
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
  background: var(--app-selected);
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
  font-size: 11px;
  color: var(--app-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono);
}

.asset-trail {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.sb-asset-tag {
  flex-shrink: 0;
  padding: 2px 7px;
  border-radius: var(--radius-pill);
  background: var(--app-panel-2);
  color: var(--app-muted);
  font: 10.5px var(--font-display);
  letter-spacing: 0.04em;
}

// ============================================================
// 拖拽视觉反馈
// ============================================================
// 拖拽中的资产/分组半透明
.asset-node.is-dragging,
.group-header.is-dragging {
  opacity: 0.4;
}

// 分组头作为拖入目标时高亮（资产拖入 → 移动到该组）
.group-header.is-drop-in {
  background: color-mix(in oklab, var(--accent), transparent 84%);
  border-radius: var(--radius-sm);
}

// 分组排序插入指示线：before = 上半区（插到前面），after = 下半区（插到后面）
.group-header.is-drop-before {
  box-shadow: inset 0 2px 0 0 var(--accent);
}
.group-header.is-drop-after {
  box-shadow: inset 0 -2px 0 0 var(--accent);
}

// .dot 为全局工具类（_utilities.scss），assetIndicatorClasses 返回其修饰符
// （connected/warn/idle），此处无 scoped 副本

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
