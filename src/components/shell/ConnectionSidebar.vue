<script setup lang="ts">
/**
 * ConnectionSidebar — Wave 3 Step 3.2（+ 分组管理扩展）
 *
 * 左侧连接资产面板。资产按 group 字段（'/' 分隔多级路径，如 "生产/数据库/主"）
 * 聚合成递归树，由子组件 AssetGroupNode 递归渲染。
 *
 * 本组件是 store-agnostic 展示组件：仅消费 props + emit 事件，父级 App.vue 接 store。
 *
 * 操作入口（资产）：悬停显示「编辑/删除」快捷按钮 + 右键菜单（编辑/复制/移动/独立窗口/删除）；
 * 拖出主窗口边界释放也开独立资产窗口（Phase 2，开窗/判定逻辑在 lib/assetWindows）。
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
  Search,
  Server,
  Zap
} from 'lucide-vue-next';
import AppContextMenu from '../ui/AppContextMenu.vue';
import AssetGroupNode from './AssetGroupNode.vue';
import { useSessionsStore } from '@/stores/sessions';
import { useWorkbenchStore, normalizeStatus } from '@/stores/workbench';
import { openAssetWindow } from '@/lib/assetWindows';
import { useAssetDnd } from '@/composables/useAssetDnd';
import { useSidebarContextMenu } from '@/composables/useSidebarContextMenu';
import { parseSshTarget } from '@/lib/parseSshTarget';
import { errorMessage } from '@/lib/errorMessage';
import type { GroupTreeNode, NormalizedConnectionAsset } from '@/types/domain';

const props = withDefaults(
  defineProps<{
    assets?: NormalizedConnectionAsset[];
    /** 分组树根（assets store buildGroupTree 的产物）。 */
    groupedAssets?: GroupTreeNode;
    selectedAssetId?: string;
    assetsCollapsed?: boolean;
    searchQuery?: string;
    quickConnectInput?: string;
  }>(),
  {
    assets: () => [],
    groupedAssets: () => ({ name: '', path: '', parent: '', children: [], items: [] }),
    selectedAssetId: '',
    assetsCollapsed: false,
    searchQuery: '',
    quickConnectInput: ''
  }
);

/** 快速连接解析结果（emit 'quick-connect' 的 payload）。 */
interface QuickConnectTarget {
  username: string;
  host: string;
  port: number;
}

const emit = defineEmits<{
  'update:searchQuery': [value: string];
  'update:quickConnectInput': [value: string];
  'select-asset': [id: string];
  'connect-asset': [id: string];
  'quick-connect': [target: QuickConnectTarget];
  'toggle-collapse': [];
  'create-asset': [];
  // 分组头悬停「+」：在该分组内快捷新增（父级打开预填分组的资产编辑器）
  'create-asset-in-group': [group: string];
  'create-group': [];
  // 以下事件 payload 均为 asset 对象或 group path 字符串
  'edit-asset': [asset: NormalizedConnectionAsset];
  'delete-asset': [asset: NormalizedConnectionAsset];
  'duplicate-asset': [asset: NormalizedConnectionAsset];
  'move-asset': [asset: NormalizedConnectionAsset];
  'rename-group': [path: string];
  'dissolve-group': [path: string];
  // 拖拽：直接落盘，不走弹窗
  'move-asset-direct': [payload: { id: string; group: string }]; // 资产拖到分组 → 直接移动
  'reorder-groups': [paths: string[]]; // 分组拖拽排序后的全量新顺序
}>();

// ============================================================
// 运行时连接态派生：session.status 是唯一权威源（sessions store 维护）。
// 侧栏圆点不读 asset.status（连接流程中从不更新），而是查该 asset 是否有
// connected/connecting 会话。无会话时回退 asset.status（编辑器初始值，通常 Idle → 灰点）。
// ============================================================
const sessionsStore = useSessionsStore();
const workbench = useWorkbenchStore();
const connectedAssetIds = computed(() => {
  const set = new Set<string | undefined>();
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
const collapsedGroups = ref(new Set<string>());

function toggleGroup(path: string) {
  const next = new Set(collapsedGroups.value);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  collapsedGroups.value = next;
}

function isCollapsed(path: string) {
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

function emptyRoot(): GroupTreeNode {
  return { name: '', path: '', parent: '', children: [], items: [] };
}

function filterNode(node: GroupTreeNode, query: string): GroupTreeNode | null {
  const matchedItems = (node.items || []).filter(asset => {
    const haystack = [
      asset.name, asset.host, asset.username, asset.group,
      asset.auth_method, ...(asset.tags || [])
    ].filter(Boolean).join(' ').toLowerCase();
    return haystack.includes(query);
  });
  const filteredChildren: GroupTreeNode[] = [];
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
  const out: NormalizedConnectionAsset[] = [];
  walkTree(visibleTree.value, false, out);
  return out;
});

function walkTree(node: GroupTreeNode, parentCollapsed: boolean, out: NormalizedConnectionAsset[]) {
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
// Status normalization — 复用 workbench 导出的 normalizeStatus。
// ============================================================
function statusClass(asset: NormalizedConnectionAsset) {
  const runtimeStatus = connectedAssetIds.value.has(asset.id) ? 'connected' : (asset.status || 'Idle');
  return normalizeStatus(runtimeStatus).dotClass;
}

function isActiveAsset(asset: NormalizedConnectionAsset) {
  return Boolean(props.selectedAssetId) && props.selectedAssetId === asset.id;
}

// ============================================================
// 拖拽：资产拖到分组（移动）+ 分组间拖拽（同级排序）。
// dataTransfer 用自定义 MIME 携带 { kind, id|path, parent }，区分两类拖拽。
// 「未分组」不可拖动排序，但可作为资产拖入目标。
// ============================================================
// —— 拖拽状态机（useAssetDnd，store-agnostic 经回调接线）——
// 「未分组」不可拖动排序，但可作为资产拖入目标；行为契约见 composable 头注释。
const {
  onAssetDragStart, onAssetDragEnd, isDraggingAsset,
  onGroupDragStart, onGroupDragEnd, onGroupDragOver, onGroupDragEnter,
  onGroupDragLeave, onGroupDrop, groupHeaderClass
} = useAssetDnd({
  getAssets: () => props.assets,
  getGroupedAssets: () => props.groupedAssets,
  onMoveAssetDirect: payload => emit('move-asset-direct', payload),
  onReorderGroups: paths => emit('reorder-groups', paths),
  openDetached: openAssetDetached
});

// 开独立资产窗口（拖出判定 / 右键菜单共用入口）：失败 console.error + toast 提示，不阻断
function openAssetDetached(asset: NormalizedConnectionAsset) {
  openAssetWindow(asset).catch(error => {
    console.error('[ConnectionSidebar] open asset window failed:', error);
    workbench.announce('打开独立窗口失败：' + errorMessage(error), { level: 'error' });
  });
}

// ============================================================
// Quick connect parser — `ssh user@host[:port]`
// 解析逻辑抽到 lib/parseSshTarget（与 ui store 全局搜索共用，含 IPv6 括号形式）
// ============================================================
function parseQuickConnect(input: string): QuickConnectTarget | null {
  return parseSshTarget(input);
}

// 快速连接解析失败就地提示（输入变化即清除）
const quickConnectError = ref('');

function onQuickConnectInput(event: Event) {
  quickConnectError.value = '';
  emit('update:quickConnectInput', (event.target as HTMLInputElement).value);
}

function onQuickConnectEnter() {
  const parsed = parseQuickConnect(props.quickConnectInput);
  if (parsed) {
    quickConnectError.value = '';
    emit('quick-connect', parsed);
    emit('update:quickConnectInput', '');
    return;
  }
  // 非空输入解析失败 → 输入框下方红字提示 + warn toast；
  // 解析成功后的落盘（saveAsset）由父级 WorkbenchShell 处理，本组件不做。
  const input = (props.quickConnectInput || '').trim();
  if (input) {
    quickConnectError.value = '格式：ssh user@host[:port]';
    workbench.announce('快速连接格式错误：应为 ssh user@host[:port]', { level: 'warn' });
  }
}

// ============================================================
// Keyboard navigation — Up/Down 移动焦点，Enter 连接
// ============================================================
function onAssetKeydown(event: KeyboardEvent, asset: NormalizedConnectionAsset) {
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
const assetElMap = ref(new Map<string, HTMLElement>());
function registerAssetEl(id: string, el: unknown) {
  if (el) assetElMap.value.set(id, el as HTMLElement);
  else assetElMap.value.delete(id);
}

// —— 右键菜单（useSidebarContextMenu，动作经回调接线 emit / openAssetWindow）——
const {
  contextMenu, closeContextMenu, assetMenuItems, groupMenuItems,
  onAssetContextMenu, onGroupContextMenu
} = useSidebarContextMenu({
  onEditAsset: a => emit('edit-asset', a),
  onDuplicateAsset: a => emit('duplicate-asset', a),
  onMoveAsset: a => emit('move-asset', a),
  onDeleteAsset: a => emit('delete-asset', a),
  onRenameGroup: path => emit('rename-group', path),
  onDissolveGroup: path => emit('dissolve-group', path),
  openDetached: openAssetDetached
});

// ============================================================
// provide：AssetGroupNode 通过 inject('connectionSidebar') 调用这些。
// 用 ref 函数包裹响应式依赖，避免 provide 快照失效。
// （注入契约的完整形状见 AssetGroupNode.vue 的 SidebarContext interface）
// ============================================================
provide('connectionSidebar', {
  isCollapsed,
  toggleGroup,
  statusClass,
  isActiveAsset,
  isUngrouped: (path: string) => path === '未分组',
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
  onSelectAsset: (id: string) => emit('select-asset', id),
  onConnectAsset: (id: string) => emit('connect-asset', id),
  onAssetKeydown,
  onAssetContextMenu,
  onGroupContextMenu,
  onEditAsset: (asset: NormalizedConnectionAsset) => emit('edit-asset', asset),
  onDeleteAsset: (asset: NormalizedConnectionAsset) => emit('delete-asset', asset),
  onDuplicateAsset: (asset: NormalizedConnectionAsset) => emit('duplicate-asset', asset),
  onAddAssetToGroup: (path: string) => emit('create-asset-in-group', path),
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
          :aria-expanded="!assetsCollapsed ? 'true' : 'false'"
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
      <Search :size="13" class="sb-search-icon" aria-hidden="true" />
      <input
        type="search"
        class="sb-search-input"
        :value="searchQuery"
        placeholder="筛选分组、标签、主机、用户"
        aria-label="筛选资产"
        @input="emit('update:searchQuery', ($event.target as HTMLInputElement).value)"
      />
    </div>

    <!-- ============================================================
         Tree (scrollable middle) — 递归渲染分组树（app.html sb-tree）
         ============================================================ -->
    <div class="sb-tree" role="tree" aria-label="连接资产列表">
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
        <Zap :size="13" aria-hidden="true" />
        <input
          type="text"
          class="sb-quick-input"
          :value="quickConnectInput"
          placeholder="ssh user@host[:port]"
          spellcheck="false"
          aria-label="快速连接"
          aria-describedby="quick-connect-error"
          @input="onQuickConnectInput"
          @keydown.enter.prevent="onQuickConnectEnter"
        />
      </div>
      <p v-if="quickConnectError" id="quick-connect-error" class="sb-quick-error" role="alert">{{ quickConnectError }}</p>
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

<style scoped lang="scss" src="./ConnectionSidebar.sidebar.scss"></style>
