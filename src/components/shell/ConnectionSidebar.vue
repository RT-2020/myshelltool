<script setup>
/**
 * ConnectionSidebar — Wave 3 Step 3.2（+ 分组管理扩展）
 *
 * 左侧连接资产面板。资产按 group 字段（'/' 分隔多级路径，如 "生产/数据库/主"）
 * 聚合成递归树，由子组件 AssetGroupNode 递归渲染。
 *
 * 本组件是 store-agnostic 展示组件：仅消费 props + emit 事件，父级 App.vue 接 store。
 *
 * 操作入口（资产）：悬停显示「编辑/删除」快捷按钮 + 右键菜单（编辑/复制/移动/删除）。
 * 操作入口（分组）：分组头右键菜单（重命名/解散）。「未分组」是保留节点，无分组菜单。
 *
 * 通过 provide('connectionSidebar', ...) 把 handler/state 注入给 AssetGroupNode，
 * 避免递归组件层层 emit 透传。
 */
import { computed, provide, ref } from 'vue';
import {
  FolderPlus,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Server,
  Terminal
} from 'lucide-vue-next';
import AppInput from '../ui/AppInput.vue';
import AppContextMenu from '../ui/AppContextMenu.vue';
import AssetGroupNode from './AssetGroupNode.vue';
import { useSessionsStore } from '@/stores/sessions.js';
import { normalizeStatus } from '@/stores/workbench.js';

const props = defineProps({
  assets: { type: Array, default: () => [] },
  // 分组树根：{ name:'', path:'', parent:'', children:[TreeNode], items:[asset] }
  groupedAssets: { type: Object, default: () => ({ name: '', path: '', parent: '', children: [], items: [] }) },
  selectedAssetId: { type: String, default: '' },
  assetsCollapsed: { type: Boolean, default: false },
  searchQuery: { type: String, default: '' },
  quickConnectInput: { type: String, default: '' }
});

const emit = defineEmits([
  'update:searchQuery',
  'update:quickConnectInput',
  'select-asset',
  'connect-asset',
  'quick-connect',
  'toggle-collapse',
  'create-asset',
  'create-group',
  // 以下事件 payload 均为 asset 对象或 group path 字符串
  'edit-asset',
  'delete-asset',
  'duplicate-asset',
  'move-asset',           // 右键菜单「移动到分组…」→ 打开弹窗
  'rename-group',
  'dissolve-group',
  // 拖拽：直接落盘，不走弹窗
  'move-asset-direct',    // payload { id, group }：资产拖到分组 → 直接移动
  'reorder-groups'        // payload string[]：分组拖拽排序后的全量新顺序
]);

// ============================================================
// 运行时连接态派生：session.status 是唯一权威源（sessions.js 维护）。
// 侧栏圆点不读 asset.status（连接流程中从不更新），而是查该 asset 是否有
// connected/connecting 会话。无会话时回退 asset.status（编辑器初始值，通常 Idle → 灰点）。
// ============================================================
const sessionsStore = useSessionsStore();
const connectedAssetIds = computed(() => {
  const set = new Set();
  for (const session of sessionsStore.sessions) {
    if (session.status === 'connected' || session.status === 'connecting') {
      set.add(session.asset?.id);
    }
  }
  return set;
});

// ============================================================
// 折叠态：按完整 path 存储（嵌套下同名子分组需区分）。
// ============================================================
const collapsedGroups = ref(new Set());

function toggleGroup(path) {
  const next = new Set(collapsedGroups.value);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  collapsedGroups.value = next;
}

function isCollapsed(path) {
  return collapsedGroups.value.has(path);
}

// ============================================================
// 搜索过滤：对树递归过滤，保留「自身资产命中 或 任一子孙命中」的子树。
// 命中时临时展开（忽略 collapsedGroups，由过滤后的节点决定可见性）。
// ============================================================
const visibleTree = computed(() => {
  const query = (props.searchQuery || '').trim().toLowerCase();
  if (!query) return props.groupedAssets;
  return filterNode(props.groupedAssets, query) || emptyRoot();
});

function emptyRoot() {
  return { name: '', path: '', parent: '', children: [], items: [] };
}

function filterNode(node, query) {
  const matchedItems = (node.items || []).filter(asset => {
    const haystack = [
      asset.name, asset.host, asset.username, asset.group,
      asset.auth_method, ...(asset.tags || [])
    ].filter(Boolean).join(' ').toLowerCase();
    return haystack.includes(query);
  });
  const filteredChildren = [];
  for (const child of (node.children || [])) {
    const fc = filterNode(child, query);
    if (fc) filteredChildren.push(fc);
  }
  // 节点名匹配则保留整棵子树（含所有 items/children）
  const nameMatch = node.name && node.name.toLowerCase().includes(query);
  if (nameMatch) {
    return node;
  }
  if (matchedItems.length || filteredChildren.length) {
    return { ...node, items: matchedItems, children: filteredChildren };
  }
  return null;
}

const hasAssets = computed(() => props.assets.length > 0);
const assetsCount = computed(() => props.assets.length);
const hasResults = computed(() => {
  const root = visibleTree.value;
  return (root.items?.length || 0) + (root.children?.length || 0) > 0;
});
const hasQuery = computed(() => (props.searchQuery || '').trim().length > 0);

// 扁平化可见资产（跳过折叠分组），用于箭头键导航
const flatAssets = computed(() => {
  const out = [];
  walkTree(visibleTree.value, false, out);
  return out;
});

function walkTree(node, parentCollapsed, out) {
  // 有搜索词时忽略折叠态（过滤树已剔除无关项，全部展开便于浏览）
  const collapsed = hasQuery.value ? false : (parentCollapsed || collapsedGroups.value.has(node.path));
  if (!collapsed) {
    for (const item of (node.items || [])) out.push(item);
  }
  for (const child of (node.children || [])) {
    walkTree(child, collapsed, out);
  }
}

// ============================================================
// Status normalization — 复用 workbench.js 导出的 normalizeStatus。
// ============================================================
function statusClass(asset) {
  const runtimeStatus = connectedAssetIds.value.has(asset.id) ? 'connected' : (asset.status || 'Idle');
  return normalizeStatus(runtimeStatus).dotClass;
}

function isActiveAsset(asset) {
  return Boolean(props.selectedAssetId) && props.selectedAssetId === asset.id;
}

// ============================================================
// 拖拽：资产拖到分组（移动）+ 分组间拖拽（同级排序）。
// dataTransfer 用自定义 MIME 携带 { kind, id|path, parent }，区分两类拖拽。
// 「未分组」不可拖动排序，但可作为资产拖入目标。
// ============================================================
const DRAG_MIME = 'application/x-myshelltool-drag';

// 拖拽态：当前被拖对象 / 当前悬停目标分组 / 上半还是下半区
const dragSource = ref(null);   // { kind:'asset', id } | { kind:'group', path, parent } | null
const dropTarget = ref(null);   // { path, position:'in'|'before'|'after' } | null

function readDragData(event) {
  const raw = event.dataTransfer?.getData(DRAG_MIME);
  if (!raw) return null;
  try { return JSON.parse(raw); } catch { return null; }
}
function writeDragData(event, payload) {
  // 兜底 text/plain：某些环境（部分 webview）对自定义 MIME 支持不稳
  event.dataTransfer?.setData(DRAG_MIME, JSON.stringify(payload));
  event.dataTransfer?.setData('text/plain', payload.kind === 'asset' ? payload.id : payload.path);
  event.dataTransfer.effectAllowed = 'move';
}

// —— 资产拖拽 source ——
function onAssetDragStart(event, asset) {
  dragSource.value = { kind: 'asset', id: asset.id };
  writeDragData(event, { kind: 'asset', id: asset.id });
}
function onAssetDragEnd() {
  dragSource.value = null;
  dropTarget.value = null;
}
function isDraggingAsset(id) {
  return dragSource.value?.kind === 'asset' && dragSource.value.id === id;
}

// —— 分组拖拽 source（排序）——
function onGroupDragStart(event, path, parent) {
  if (path === '未分组') { event.preventDefault(); return; } // 保留节点不可拖
  dragSource.value = { kind: 'group', path, parent };
  writeDragData(event, { kind: 'group', path, parent });
}
function onGroupDragEnd() {
  dragSource.value = null;
  dropTarget.value = null;
}

// —— 分组头作为 drop target（同时收资产拖入与分组排序）——
function onGroupDragOver(event, path, parent) {
  const src = readDragData(event) || dragSource.value;
  if (!src) return;
  // 资产拖入：任意分组都可接收（含「未分组」），effectAllowed=move 需配 dropEffect
  if (src.kind === 'asset') {
    event.dataTransfer.dropEffect = 'move';
    event.preventDefault();
    return;
  }
  // 分组排序：仅同级可排（同 parent），且不能排到自己上、不能排到「未分组」
  if (src.kind === 'group') {
    if (path === '未分组' || src.path === path || src.parent !== parent) return;
    event.dataTransfer.dropEffect = 'move';
    event.preventDefault();
  }
}
function onGroupDragEnter(event, path) {
  const src = readDragData(event) || dragSource.value;
  if (!src) return;
  if (src.kind === 'asset') {
    dropTarget.value = { path, position: 'in' };
  } else if (src.kind === 'group' && path !== '未分组' && src.path !== path) {
    // 上半/下半区决定 before/after
    const rect = event.currentTarget.getBoundingClientRect();
    const position = event.clientY < rect.top + rect.height / 2 ? 'before' : 'after';
    dropTarget.value = { path, position };
  }
}
function onGroupDragLeave(event, path) {
  // dragleave 会因子元素冒泡频繁触发；仅在真正离开该分组头时清目标
  if (dropTarget.value?.path === path) {
    const related = event.relatedTarget;
    if (!event.currentTarget.contains(related)) {
      dropTarget.value = null;
    }
  }
}
function onGroupDrop(event, targetPath, targetParent) {
  const src = readDragData(event) || dragSource.value;
  dropTarget.value = null;
  dragSource.value = null;
  if (!src) return;
  if (src.kind === 'asset') {
    // 资产拖入分组 → 直接移动
    emit('move-asset-direct', { id: src.id, group: targetPath });
    return;
  }
  if (src.kind === 'group') {
    // 同级排序：把 src.path 插到 targetPath 的 before/after
    if (targetPath === '未分组' || src.path === targetPath || src.parent !== targetParent) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const placeAfter = event.clientY >= rect.top + rect.height / 2;
    emit('reorder-groups', buildReorderedPaths(src.path, targetPath, placeAfter));
  }
}

// 把分组拖拽结果转成全量新顺序（扁平路径，父在子前 DFS 先序）。
// 思路：从当前分组树按 DFS 先序收集所有非「未分组」路径，移除 src，
// 插到 target 的 before/after 位置。
function buildReorderedPaths(srcPath, targetPath, placeAfter) {
  const ordered = collectGroupPathsDfs(props.groupedAssets);
  const filtered = ordered.filter(p => p !== srcPath);
  const idx = filtered.indexOf(targetPath);
  if (idx === -1) return ordered; // 兜底：target 不在列表，原样返回
  filtered.splice(placeAfter ? idx + 1 : idx, 0, srcPath);
  return filtered;
}
// DFS 先序收集分组路径（root.children 起步），跳过「未分组」。
function collectGroupPathsDfs(root) {
  const out = [];
  const walk = (node) => {
    for (const child of (node.children || [])) {
      if (child.path !== '未分组') out.push(child.path);
      walk(child);
    }
  };
  walk(root);
  return out;
}

// 分组头拖放态 class（AssetGroupNode 调用）
function groupHeaderClass(path) {
  if (!dropTarget.value || dropTarget.value.path !== path) {
    return dragSource.value?.kind === 'group' && dragSource.value.path === path ? 'is-dragging' : '';
  }
  const pos = dropTarget.value.position;
  if (pos === 'in') return 'is-drop-in';
  if (pos === 'before') return 'is-drop-before';
  if (pos === 'after') return 'is-drop-after';
  return '';
}

// ============================================================
// Quick connect parser — `ssh user@host[:port]`
// ============================================================
function parseQuickConnect(input) {
  const trimmed = input.trim();
  const match = trimmed.match(/^ssh\s+([^\s@]+)@([^\s:]+)(?::(\d+))?$/);
  if (!match) return null;
  return {
    username: match[1],
    host: match[2],
    port: Number(match[3] || 22)
  };
}

function onQuickConnectEnter() {
  const parsed = parseQuickConnect(props.quickConnectInput);
  if (parsed) {
    emit('quick-connect', parsed);
    emit('update:quickConnectInput', '');
  }
}

// ============================================================
// Keyboard navigation — Up/Down 移动焦点，Enter 连接
// ============================================================
function onAssetKeydown(event, asset) {
  if (event.key === 'Enter') {
    event.preventDefault();
    emit('connect-asset', asset.id);
    return;
  }
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    const idx = flatAssets.value.findIndex(item => item.id === asset.id);
    if (idx === -1) return;
    const delta = event.key === 'ArrowDown' ? 1 : -1;
    const next = flatAssets.value[idx + delta];
    if (next) {
      const el = assetElMap.value.get(next.id);
      if (el && typeof el.focus === 'function') el.focus();
    }
  }
}

// DOM ref 映射（AssetGroupNode 通过 inject 调用 registerAssetEl）
const assetElMap = ref(new Map());
function registerAssetEl(id, el) {
  if (el) assetElMap.value.set(id, el);
  else assetElMap.value.delete(id);
}

// ============================================================
// 右键菜单：资产 / 分组两套，共用一个 contextMenu ref（带 kind 区分）
// ============================================================
const contextMenu = ref({ visible: false, kind: '', x: 0, y: 0, asset: null, path: '' });

function openContextMenu(event, kind, payload) {
  contextMenu.value = {
    visible: true,
    kind,
    x: event.clientX,
    y: event.clientY,
    asset: payload.asset || null,
    path: payload.path || ''
  };
}

function onAssetContextMenu(event, asset) {
  openContextMenu(event, 'asset', { asset });
}

function onGroupContextMenu(event, path) {
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
  const make = (label, fn, opts = {}) => ({ label, action: fn, ...opts });
  return [
    make('编辑', () => emit('edit-asset', a)),
    make('复制', () => emit('duplicate-asset', a)),
    { separator: true },
    make('移动到分组…', () => emit('move-asset', a)),
    { separator: true },
    make('删除', () => emit('delete-asset', a), { danger: true })
  ];
});

const groupMenuItems = computed(() => {
  if (!contextMenu.value.visible || contextMenu.value.kind !== 'group') return [];
  const path = contextMenu.value.path;
  if (!path) return [];
  const make = (label, fn, opts = {}) => ({ label, action: fn, ...opts });
  return [
    make('重命名…', () => emit('rename-group', path)),
    { separator: true },
    make('解散分组', () => emit('dissolve-group', path), { danger: true })
  ];
});

// ============================================================
// provide：AssetGroupNode 通过 inject('connectionSidebar') 调用这些。
// 用 ref 函数包裹响应式依赖，避免 provide 快照失效。
// ============================================================
provide('connectionSidebar', {
  isCollapsed,
  toggleGroup,
  statusClass,
  isActiveAsset,
  isUngrouped: path => path === '未分组',
  // 拖拽态
  isDraggingAsset,
  groupHeaderClass,
  onAssetDragStart,
  onAssetDragEnd,
  onGroupDragStart,
  onGroupDragEnd,
  onGroupDragOver,
  onGroupDragEnter,
  onGroupDragLeave,
  onGroupDrop,
  onSelectAsset: id => emit('select-asset', id),
  onConnectAsset: id => emit('connect-asset', id),
  onAssetKeydown,
  onAssetContextMenu,
  onGroupContextMenu,
  onEditAsset: asset => emit('edit-asset', asset),
  onDeleteAsset: asset => emit('delete-asset', asset),
  onDuplicateAsset: asset => emit('duplicate-asset', asset),
  registerAssetEl
});
</script>

<template>
  <div class="sidebar" :class="{ 'is-collapsed': assetsCollapsed }">
    <!-- ============================================================
         Header (sticky top): chrome-label + count + actions（app.html sb-header）
         ============================================================ -->
    <header class="sb-header">
      <div class="sb-title">
        <span class="chrome-label">连接资产</span>
        <span class="sb-count" aria-label="已配置连接数">{{ assetsCount }}</span>
      </div>
      <div class="sb-actions">
        <button
          type="button"
          class="icon-btn"
          :aria-label="assetsCollapsed ? '展开连接资产' : '收起连接资产'"
          :aria-expanded="String(!assetsCollapsed)"
          title="收起 / 展开"
          @click="emit('toggle-collapse')"
        >
          <PanelLeftClose v-if="!assetsCollapsed" :size="16" />
          <PanelLeftOpen v-else :size="16" />
        </button>
        <button
          type="button"
          class="icon-btn"
          aria-label="新建分组"
          title="新建分组"
          @click="emit('create-group')"
        >
          <FolderPlus :size="16" />
        </button>
        <button
          type="button"
          class="icon-btn primary"
          aria-label="新增连接"
          title="新增连接"
          @click="emit('create-asset')"
        >
          <Plus :size="16" />
        </button>
      </div>
    </header>

    <div class="sb-rail" role="toolbar" aria-orientation="vertical" aria-label="折叠侧栏快捷操作">
      <button
        type="button"
        class="icon-btn"
        aria-label="展开连接资产"
        title="展开侧栏"
        @click="emit('toggle-collapse')"
      >
        <PanelLeftOpen :size="16" />
      </button>
      <div class="sb-rail-divider" aria-hidden="true"></div>
      <button
        type="button"
        class="icon-btn"
        aria-label="新建分组"
        title="新建分组"
        @click="emit('create-group')"
      >
        <FolderPlus :size="16" />
      </button>
      <button
        type="button"
        class="icon-btn primary"
        aria-label="新增连接"
        title="新增连接"
        @click="emit('create-asset')"
      >
        <Plus :size="16" />
      </button>
    </div>

    <!-- ============================================================
         Search（app.html sb-search-wrap + sb-search-input）
         ============================================================ -->
    <div class="sb-search-wrap">
      <svg class="sb-search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true">
        <circle cx="11" cy="11" r="7" />
        <path d="M21 21l-4.3-4.3" stroke-linecap="round" />
      </svg>
      <input
        type="search"
        class="sb-search-input"
        :value="searchQuery"
        placeholder="筛选分组、标签、主机、用户"
        aria-label="筛选资产"
        @input="emit('update:searchQuery', $event.target.value)"
      />
    </div>

    <!-- ============================================================
         Tree (scrollable middle) — 递归渲染分组树（app.html sb-tree）
         ============================================================ -->
    <div class="sb-tree" role="tree" aria-label="连接资产列表">
      <div v-if="hasAssets" class="sb-tree-head">
        <span class="chrome-label">已有分组</span>
        <span class="sb-tree-hint">拖拽资产即可重新分组</span>
      </div>

      <!-- Empty state: no assets at all（app.html sb-empty 虚线边框容器）-->
      <div v-if="!hasAssets" class="sb-empty">
        <div class="sb-empty-icon" aria-hidden="true">
          <Server :size="22" />
        </div>
        <div class="sb-empty-title">尚未添加连接资产</div>
        <div class="sb-empty-desc">新增连接后会按分组聚合到此处。按 <kbd>+</kbd> 快速添加。</div>
        <button type="button" class="btn-primary" @click="emit('create-asset')">添加第一个连接</button>
      </div>

      <!-- No filter result state -->
      <div v-else-if="!hasResults && hasQuery" class="sb-empty">
        <div class="sb-empty-title">无匹配「{{ searchQuery }}」的连接</div>
      </div>

      <!-- Asset tree：顶层每个子节点递归渲染 -->
      <template v-else>
        <AssetGroupNode
          v-for="child in visibleTree.children"
          :key="child.path || child.name"
          :node="child"
          :depth="0"
        />
      </template>
    </div>

    <!-- ============================================================
         Footer (sticky bottom): quick connect（app.html sb-footer + sb-quick）
         ============================================================ -->
    <footer class="sb-footer">
      <div class="sb-quick">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true">
          <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
        <input
          type="text"
          class="sb-quick-input"
          :value="quickConnectInput"
          placeholder="ssh user@host[:port]"
          spellcheck="false"
          aria-label="快速连接"
          @input="emit('update:quickConnectInput', $event.target.value)"
          @keydown.enter.prevent="onQuickConnectEnter"
        />
      </div>
      <p class="sb-quick-hint">
        <Terminal :size="12" />
        <span>输入 <kbd>ssh user@host</kbd> 回车连接</span>
      </p>
    </footer>

    <!-- ============================================================
         右键菜单（资产 / 分组共用 AppContextMenu，按 kind 切 items）
         ============================================================ -->
    <AppContextMenu
      :open="contextMenu.visible && contextMenu.kind === 'asset'"
      :items="assetMenuItems"
      :x="contextMenu.x"
      :y="contextMenu.y"
      @close="closeContextMenu"
    />
    <AppContextMenu
      :open="contextMenu.visible && contextMenu.kind === 'group'"
      :items="groupMenuItems"
      :x="contextMenu.x"
      :y="contextMenu.y"
      @close="closeContextMenu"
    />
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// ============================================================
// Sidebar 容器（app.css L397-404 严格同步）
// ============================================================
.sidebar {
  display: grid;
  grid-template-rows: auto auto 1fr auto;
  width: 100%;
  height: 100%;
  min-height: 0;
  background: var(--app-window);
  color: var(--app-text);
  font-family: var(--font-body);
  overflow: hidden;
}

// ============================================================
// Header（app.css L405-418 sb-header + chrome-label + sb-count）
// ============================================================
.sb-header {
  padding: 12px 12px 8px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  border-bottom: 1px solid var(--app-border-soft);
  flex: 0 0 auto;
}
.sb-title {
  display: flex;
  align-items: center;
  gap: 8px;
}
// chrome-label（app.css L182-188：mono 小写大字距，但中文友好调整）
.chrome-label {
  font-family: var(--font-display);
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.04em;
  color: var(--app-muted);
}
.sb-count {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--app-muted);
  background: var(--app-panel-2);
  border-radius: 4px;
  padding: 1px 6px;
}
.sb-actions {
  display: flex;
  gap: 2px;
}

.sb-rail {
  display: none;
}

// 通用 icon-btn（与 TitleBar/RightSidebar 同规格，app.css L298-317）
.icon-btn {
  width: 28px;
  height: 28px;
  display: inline-grid;
  place-items: center;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--app-muted);
  cursor: pointer;
  padding: 0;
  transition: background var(--motion-fast), color var(--motion-fast), border-color var(--motion-fast);
}
.icon-btn svg {
  width: 14px;
  height: 14px;
  stroke-width: 1.6;
}
.icon-btn:hover {
  background: var(--app-hover);
  color: var(--app-text);
}
.icon-btn:active { background: var(--app-active); }
.icon-btn:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}
.icon-btn.primary { color: var(--accent); }
.icon-btn.primary:hover {
  background: var(--accent-soft);
  color: var(--accent-hover);
}

// ============================================================
// Search（app.css L420-446 sb-search-wrap + sb-search-input）
// ============================================================
.sb-search-wrap {
  position: relative;
  padding: 8px 12px;
  flex: 0 0 auto;
}
.sb-search-icon {
  position: absolute;
  left: 22px;
  top: 50%;
  transform: translateY(-50%);
  width: 13px;
  height: 13px;
  color: var(--app-muted);
  stroke-width: 1.7;
  pointer-events: none;
}
.sb-search-input {
  width: 100%;
  height: 30px;
  padding: 0 12px 0 32px;
  background: var(--app-panel-2);
  border: 1px solid transparent;
  border-radius: 7px;
  font-size: 12.5px;
  font-family: var(--font-body);
  color: var(--app-text);
  outline: none;
  transition: border-color var(--motion-fast), background var(--motion-fast);
}
.sb-search-input::placeholder { color: var(--app-subtle); }
.sb-search-input:focus {
  background: var(--app-control);
  border-color: var(--accent);
}

// ============================================================
// Tree（app.css L448-451 sb-tree）
// ============================================================
.sb-tree {
  overflow-y: auto;
  padding: 4px 8px 12px;
  min-height: 0;
}
.sb-tree-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 4px 6px 8px;
}
.sb-tree-hint {
  font-size: 11px;
  color: var(--app-subtle);
  white-space: nowrap;
}
// 滚动条（app.css L958-962）
.sb-tree::-webkit-scrollbar { width: 8px; }
.sb-tree::-webkit-scrollbar-thumb {
  background: var(--app-border);
  border-radius: 4px;
  border: 2px solid transparent;
  background-clip: padding-box;
}
.sb-tree::-webkit-scrollbar-thumb:hover { background: var(--app-border-strong); }

// 顶层直属资产（树根 items）—— 与 AssetGroupNode 内 .asset-node 同款
.asset-node {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-2);
  margin-block-end: 1px;
  border-radius: var(--radius-sm);
  border-inline-start: 2px solid transparent;
  cursor: pointer;
  position: relative;
  transition: background var(--motion-fast), border-color var(--motion-fast);
}
.asset-node:hover { background: var(--app-hover); }
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
  font-size: 13px;
  color: var(--app-text);
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
.dot {
  flex: 0 0 auto;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--app-subtle);
}
.dot.running {
  background: var(--success);
  box-shadow: 0 0 0 2px var(--success-soft);
}
.dot.warn {
  background: var(--warn);
  box-shadow: 0 0 0 2px var(--warn-soft);
}

// ============================================================
// Empty state（app.css L453-475 sb-empty 虚线边框容器）
// ============================================================
.sb-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 32px 16px;
  margin: 8px 4px;
  text-align: center;
  border: 1px dashed var(--app-border-strong);
  border-radius: 8px;
}
.sb-empty-icon {
  width: 40px;
  height: 40px;
  display: grid;
  place-items: center;
  border-radius: 10px;
  background: var(--app-panel-2);
  border: 1px solid var(--app-border);
  color: var(--app-subtle);
}
.sb-empty-title {
  font-size: 13px;
  font-weight: 500;
  color: var(--app-text);
}
.sb-empty-desc {
  font-size: 11.5px;
  color: var(--app-muted);
  line-height: 1.5;
}
.sb-empty-desc kbd,
.sb-quick-hint kbd {
  font-family: var(--font-mono);
  font-size: 10px;
  padding: 1px 5px;
  border: 1px solid var(--app-border);
  border-radius: 4px;
  background: var(--app-panel-2);
  color: var(--app-muted);
}
// 主按钮（app.css L319-332 btn-primary）
.btn-primary {
  height: 30px;
  padding: 0 14px;
  background: var(--accent);
  color: var(--accent-on);
  border: 0;
  border-radius: 7px;
  font-size: 12.5px;
  font-weight: 500;
  cursor: pointer;
  margin-top: 4px;
  transition: background var(--motion-fast);
}
.btn-primary:hover { background: var(--accent-hover); }
.btn-primary:active { background: var(--accent-active); }

// ============================================================
// Footer（app.css L416-418 + L419-433 sb-footer + sb-quick）
// ============================================================
.sb-footer {
  flex: 0 0 auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 12px 12px;
  border-top: 1px solid var(--app-border-soft);
  background: var(--app-window);
}
.sb-quick {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 30px;
  padding: 0 10px;
  background: var(--app-panel-2);
  border: 1px solid transparent;
  border-radius: 7px;
  transition: border-color var(--motion-fast), background var(--motion-fast);
}
.sb-quick:focus-within {
  background: var(--app-control);
  border-color: var(--accent);
}
.sb-quick svg {
  width: 13px;
  height: 13px;
  color: var(--app-muted);
  stroke-width: 1.7;
  flex-shrink: 0;
}
.sb-quick-input {
  flex: 1;
  min-width: 0;
  border: 0;
  background: transparent;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--app-text);
  outline: none;
}
.sb-quick-input::placeholder { color: var(--app-subtle); }
.sb-quick-hint {
  display: flex;
  align-items: center;
  gap: 5px;
  margin: 0;
  padding-left: 2px;
  font-size: 11px;
  color: var(--app-subtle);
}
.sb-quick-hint svg { color: var(--app-subtle); }

// ============================================================
// Collapsed rail（实用优先偏离点：保留竖排图标列）
// app.html 设计稿折叠态几乎空白，但保留竖排图标列保证折叠后仍可操作（展开/新建分组/新增）
// ============================================================
.sidebar.is-collapsed {
  grid-template-rows: 1fr;
  padding: 8px 0;
  place-items: start center;
}
.sidebar.is-collapsed .sb-header,
.sidebar.is-collapsed .sb-search-wrap,
.sidebar.is-collapsed .sb-tree,
.sidebar.is-collapsed .sb-footer {
  display: none;
}
.sidebar.is-collapsed .sb-rail {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding-top: 4px;
}
.sb-rail-divider {
  width: 18px;
  height: 1px;
  background: var(--app-border);
  margin: 2px 0;
}
.sb-rail .icon-btn.primary {
  background: var(--accent-soft);
  color: var(--accent);
}
.sb-rail .icon-btn.primary:hover {
  background: var(--accent-soft-strong);
  color: var(--accent-hover);
}
</style>
