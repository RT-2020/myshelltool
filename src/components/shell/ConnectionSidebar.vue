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
  ListTree,
  Network,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Search,
  Zap
} from 'lucide-vue-next';
import AppContextMenu from '../ui/AppContextMenu.vue';
import AssetGroupNode from './AssetGroupNode.vue';
import AssetSidebarEmpty from './AssetSidebarEmpty.vue';
import { useSessionsStore } from '@/stores/sessions';
import { useWorkbenchStore } from '@/stores/workbench';
import { normalizeSessionStatus, pickDominantSessionStatus } from '@/lib/sessionStatus';
import { openAssetWindow } from '@/lib/assetWindows';
import { useAssetDnd } from '@/composables/useAssetDnd';
import { useSidebarContextMenu } from '@/composables/useSidebarContextMenu';
import { parseSshTarget } from '@/lib/parseSshTarget';
import { errorMessage } from '@/lib/errorMessage';
import type { AssetListResult, GroupTreeNode, NormalizedConnectionAsset } from '@/types/domain';

const props = withDefaults(
  defineProps<{
    assets?: NormalizedConnectionAsset[];
    /** 分组树根（assets store buildGroupTree 的产物）。 */
    groupedAssets?: GroupTreeNode;
    assetSource?: AssetListResult;
    selectedAssetId?: string;
    assetsCollapsed?: boolean;
    searchQuery?: string;
    quickConnectInput?: string;
  }>(),
  {
    assets: () => [],
    groupedAssets: () => ({ name: '', path: '', parent: '', children: [], items: [] }),
    assetSource: () => ({ source: 'loading', count: 0 }),
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
  'create-gateway': []; // v0.22 网关部署向导（经公网 VPS relay 访问客户内网）
  // 分组头悬停「+」：在该分组内快捷新增（父级打开预填分组的资产编辑器）
  'create-asset-in-group': [group: string];
  // 「新建分组」按钮无参；右键「新建子分组」带父路径（上游预填弹窗）
  'create-group': [parentPath?: string];
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
// 按资产 id 取最具代表性的真实会话状态（connecting/reconnecting/error/disconnected 均如实反映）；
// 无会话时回退 asset.status（通常 Idle）。
// ============================================================
const sessionsStore = useSessionsStore();
const workbench = useWorkbenchStore();
const runtimeStatusByAssetId = computed(() => {
  const buckets = new Map<string, string[]>();
  for (const session of sessionsStore.sessions) {
    const id = session.asset?.id;
    if (!id) continue;
    const list = buckets.get(id) || [];
    list.push(session.status);
    buckets.set(id, list);
  }
  const map = new Map<string, string>();
  for (const [id, list] of buckets) {
    const dom = pickDominantSessionStatus(list);
    if (dom) map.set(id, dom);
  }
  return map;
});

// ============================================================
// 折叠态：按完整 path 存储并持久化至 localStorage（02 OQ6）。
// ============================================================
const COLLAPSED_STORAGE_KEY = 'myshelltool:sidebar-collapsed-groups:v1';
function loadCollapsedGroups(): Set<string> {
  try {
    if (typeof localStorage === 'undefined') return new Set();
    const raw = localStorage.getItem(COLLAPSED_STORAGE_KEY);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? new Set(parsed.filter(v => typeof v === 'string')) : new Set();
  } catch {
    return new Set();
  }
}
const collapsedGroups = ref<Set<string>>(loadCollapsedGroups());

function toggleGroup(path: string) {
  const next = new Set(collapsedGroups.value);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  collapsedGroups.value = next;
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(COLLAPSED_STORAGE_KEY, JSON.stringify([...next]));
    }
  } catch {
    // localStorage 写入失败不影响当次内存折叠切换
  }
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
const emptyMode = computed<'unavailable' | 'empty' | 'no-match' | null>(() => {
  if (!hasAssets.value) {
    return props.assetSource?.source === 'unavailable' ? 'unavailable' : 'empty';
  }
  if (!hasResults.value && hasQuery.value) return 'no-match';
  return null;
});

// 扁平化可见树节点键（含分组头与展开的资产项），供 ArrowUp/ArrowDown 遍历（02 #11c）
const flatTreeKeys = computed(() => {
  const out: string[] = [];
  for (const child of (visibleTree.value.children || [])) {
    collectTreeKeys(child, out);
  }
  return out;
});

function collectTreeKeys(node: GroupTreeNode, out: string[]) {
  out.push(`group:${node.path}`);
  const expanded = hasQuery.value || !collapsedGroups.value.has(node.path);
  if (!expanded) return;
  for (const child of (node.children || [])) {
    collectTreeKeys(child, out);
  }
  for (const item of (node.items || [])) {
    out.push(`asset:${item.id}`);
  }
}

// ============================================================
// Status normalization — 统一走 lib/sessionStatus 权威映射（02 #1）。
// ============================================================
function resolveAssetStatus(asset: NormalizedConnectionAsset) {
  const runtime = runtimeStatusByAssetId.value.get(asset.id) ?? (asset.status || 'Idle');
  return normalizeSessionStatus(runtime);
}
function statusClass(asset: NormalizedConnectionAsset) {
  return resolveAssetStatus(asset).dotClass;
}
function statusLabel(asset: NormalizedConnectionAsset) {
  return resolveAssetStatus(asset).label;
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
// Keyboard navigation — ArrowUp/Down 跨分组与资产移动，Enter 连接，ArrowLeft/Right 折叠（02 #11c）
// ============================================================
const treeElMap = ref(new Map<string, HTMLElement>());
function registerTreeEl(key: string, el: unknown) {
  if (el) treeElMap.value.set(key, el as HTMLElement);
  else treeElMap.value.delete(key);
}
function focusTreeKey(key: string) {
  const el = treeElMap.value.get(key);
  if (el && typeof el.focus === 'function') el.focus();
}
function moveTreeFocus(currentKey: string, delta: 1 | -1) {
  const idx = flatTreeKeys.value.indexOf(currentKey);
  if (idx === -1) return;
  const next = flatTreeKeys.value[idx + delta];
  if (next) focusTreeKey(next);
}

function onAssetKeydown(event: KeyboardEvent, asset: NormalizedConnectionAsset) {
  if (event.key === 'Enter') {
    event.preventDefault();
    emit('connect-asset', asset.id);
    return;
  }
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    moveTreeFocus(`asset:${asset.id}`, event.key === 'ArrowDown' ? 1 : -1);
    return;
  }
  if (event.key === 'ArrowLeft' && asset.group) {
    event.preventDefault();
    focusTreeKey(`group:${asset.group}`);
  }
}

function onGroupKeydown(event: KeyboardEvent, node: GroupTreeNode) {
  const key = `group:${node.path}`;
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    moveTreeFocus(key, event.key === 'ArrowDown' ? 1 : -1);
    return;
  }
  if (event.key === 'ArrowRight') {
    event.preventDefault();
    if (isCollapsed(node.path)) toggleGroup(node.path);
    else moveTreeFocus(key, 1);
    return;
  }
  if (event.key === 'ArrowLeft') {
    event.preventDefault();
    if (!isCollapsed(node.path)) toggleGroup(node.path);
    else if (node.parent) focusTreeKey(`group:${node.parent}`);
  }
}

// 折叠态 flyout 触发通告态（02 #11a：同步 aria-expanded 与 Esc 收起）
const flyoutOpen = ref(false);
function closeFlyoutOnEsc() {
  flyoutOpen.value = false;
  const active = document.activeElement as HTMLElement | null;
  if (active && active.closest('.sb-flyout, .sb-rail-trigger')) {
    active.blur();
  }
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
  onAddAssetToGroup: path => emit('create-asset-in-group', path),
  onCreateSubgroup: path => emit('create-group', path),
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
  statusLabel,
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
  onGroupKeydown,
  onAssetContextMenu,
  onGroupContextMenu,
  onEditAsset: (asset: NormalizedConnectionAsset) => emit('edit-asset', asset),
  onDeleteAsset: (asset: NormalizedConnectionAsset) => emit('delete-asset', asset),
  onDuplicateAsset: (asset: NormalizedConnectionAsset) => emit('duplicate-asset', asset),
  onAddAssetToGroup: (path: string) => emit('create-asset-in-group', path),
  registerTreeEl
});
</script>

<template>
    <div class="sidebar" :class="{ 'is-collapsed': assetsCollapsed }">
    <!-- Header（app.html sb-header）-->
    <header class="sb-header">
      <div class="sb-title">
        <span class="chrome-label">连接资产</span>
        <span class="sb-count">{{ assetsCount }}<span class="sr-only"> 个已配置连接</span></span>
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
        <!-- header 工具按钮组（紧凑格式：本组贴 size 硬上限，格式紧凑统一） -->
        <button type="button" class="icon-btn" aria-label="部署跳板网关" title="部署跳板网关（经公网 VPS 访问客户内网）" @click="emit('create-gateway')">
          <Network :size="16" />
        </button>
        <button type="button" class="icon-btn" aria-label="新建分组" title="新建分组" @click="emit('create-group')">
          <FolderPlus :size="16" />
        </button>
        <button type="button" class="icon-btn primary" aria-label="新增连接" title="新增连接" @click="emit('create-asset')">
          <Plus :size="16" />
        </button>
      </div>
    </header>

    <div class="sb-rail" role="toolbar" aria-orientation="vertical" aria-label="折叠侧栏快捷操作">
      <!-- 展开开关置顶：顶部按钮天然被读作「还原侧栏」入口，与展开态 header 首位
           收起开关同位（心智一致）；ListTree 置顶会被误当展开按钮点出 flyout -->
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
      <!-- 资产菜单触发：hover/focus 弹出 flyout 面板（antd 折叠菜单式），点击聚焦可钉住 -->
      <button
        type="button"
        class="icon-btn sb-rail-trigger"
        aria-haspopup="true"
        :aria-expanded="flyoutOpen ? 'true' : 'false'"
        aria-label="资产菜单"
        title="资产菜单（悬停展开）"
        @mouseenter="flyoutOpen = true"
        @mouseleave="flyoutOpen = false"
        @focus="flyoutOpen = true"
        @blur="flyoutOpen = false"
        @keydown.escape="closeFlyoutOnEsc"
      >
        <ListTree :size="16" />
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

    <!-- 折叠态为 fixed 弹出面板（trigger hover/focus 弹出），展开态是普通布局行 -->
    <div
      class="sb-flyout"
      @mouseenter="flyoutOpen = true"
      @mouseleave="flyoutOpen = false"
      @keydown.escape="closeFlyoutOnEsc"
    >
    <!-- Search（app.html sb-search-wrap + sb-search-input） -->
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

    <!-- Tree（app.html sb-tree，滚动中段，递归分组树） -->
    <div class="sb-tree" role="tree" aria-label="连接资产列表">
      <AssetSidebarEmpty
        v-if="emptyMode"
        :mode="emptyMode"
        :search-query="searchQuery"
        @create-asset="emit('create-asset')"
        @clear-search="emit('update:searchQuery', '')"
      />

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

    <!-- Footer（app.html sb-footer + sb-quick，快速连接） -->
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
    </div><!-- /sb-flyout -->

    <!-- 右键菜单（资产 / 分组共用 AppContextMenu，按 kind 切 items） -->
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
