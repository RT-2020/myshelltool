<script setup>
/**
 * ConnectionSidebar — Wave 3 Step 3.2
 *
 * Left-region asset tree component for the 5-region shell layout.
 *
 * Visual style: Tabby/Termius — low visual weight, single-pixel borders,
 * muted group headers, status dots, status-pill tags. No nested cards.
 *
 * This component is "store-agnostic": it only consumes props and emits
 * events. Parent (App.vue in Wave 3 Step 3.5) wires Pinia stores to
 * props/emits.
 *
 * Features (AC6):
 *   - Asset tree grouped by `group`, collapsible group headers
 *   - Filter input (AppInput type=search) — real-time, by name/host/
 *     username/group/auth_method/tags
 *   - Quick connect input (mono font) — parses `ssh user@host[:port]`
 *   - Click asset → select-asset emit
 *   - Double-click asset → connect-asset emit
 *   - Active state highlight (border-inline-start accent + app-hover bg)
 *   - Status dot via `.dot` utility + status class
 *   - Collapse/expand sidebar button
 *   - Keyboard navigation (tabindex=0, Enter to connect, Arrow Up/Down)
 *   - Empty state + no-filter-result state
 */
import { computed, ref } from 'vue';
import {
  ChevronDown,
  ChevronRight,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Server,
  Terminal
} from 'lucide-vue-next';
import AppInput from '../ui/AppInput.vue';

const props = defineProps({
  assets: { type: Array, default: () => [] },
  groupedAssets: { type: Array, default: () => [] },
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
  'create-asset'
]);

// ============================================================
// Local UI state — group collapse (component-local, not persisted)
// ============================================================
const collapsedGroups = ref(new Set());

function toggleGroup(name) {
  if (collapsedGroups.value.has(name)) {
    collapsedGroups.value.delete(name);
    collapsedGroups.value = new Set(collapsedGroups.value);
  } else {
    collapsedGroups.value.add(name);
    collapsedGroups.value = new Set(collapsedGroups.value);
  }
}

function isGroupCollapsed(name) {
  return collapsedGroups.value.has(name);
}

// ============================================================
// Computed: filtered groups (performs real-time filter on the
// grouped view, so large lists (100+) recompute once per change)
// ============================================================
const visibleGroups = computed(() => {
  const query = (props.searchQuery || '').trim().toLowerCase();
  if (!query) return props.groupedAssets;
  const out = [];
  for (const group of props.groupedAssets) {
    const matchInGroupName = group.name.toLowerCase().includes(query);
    const items = matchInGroupName
      ? group.items
      : group.items.filter(asset => {
          const haystack = [
            asset.name,
            asset.host,
            asset.username,
            asset.group,
            asset.auth_method,
            ...(asset.tags || [])
          ]
            .filter(Boolean)
            .join(' ')
            .toLowerCase();
          return haystack.includes(query);
        });
    if (items.length) out.push({ name: group.name, items });
  }
  return out;
});

const hasAssets = computed(() => props.assets.length > 0);
const hasResults = computed(() => visibleGroups.value.length > 0);
const hasQuery = computed(() => (props.searchQuery || '').trim().length > 0);

// Flatten visible items for keyboard arrow-key navigation
const flatAssets = computed(() => {
  const out = [];
  for (const group of visibleGroups.value) {
    if (isGroupCollapsed(group.name)) continue;
    for (const item of group.items) out.push(item);
  }
  return out;
});

// ============================================================
// Status normalization — mirrors workbench.js normalizeStatus so the
// `.dot` utility class receives the expected modifier.
// ============================================================
function normalizeStatus(status) {
  if (status === 'Connected' || status === 'connected') return { label: 'connected', dotClass: 'running' };
  if (status === 'Warning' || status === 'warning') return { label: 'warning', dotClass: 'warn' };
  return { label: 'idle', dotClass: '' };
}

function statusClass(status) {
  return normalizeStatus(status).dotClass;
}

function isActiveAsset(asset) {
  return Boolean(props.selectedAssetId) && props.selectedAssetId === asset.id;
}

function assetBadge(asset) {
  return (asset.tags && asset.tags[0]) || asset.auth_method || '';
}

// ============================================================
// Quick connect parser — spec Wave 3 Step 3.2 contract
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
// Keyboard navigation — Up/Down to move, Enter to connect
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
      const el = templateRefAsset(next.id);
      if (el && typeof el.focus === 'function') el.focus();
    }
  }
}

// Lookup DOM node by asset id (scoped ref map)
const assetElMap = ref(new Map());
function registerAssetEl(id, el) {
  if (el) assetElMap.value.set(id, el);
  else assetElMap.value.delete(id);
}
function templateRefAsset(id) {
  return assetElMap.value.get(id) || null;
}
</script>

<template>
  <div class="connection-sidebar" :class="{ 'is-collapsed': assetsCollapsed }">
    <!-- ============================================================
         Header (sticky top): title row + filter input
         ============================================================ -->
    <header class="sidebar-header">
      <div class="title-row">
        <h2 class="title">连接资产</h2>
        <div class="title-actions">
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
            aria-label="新增连接"
            title="新增连接"
            @click="emit('create-asset')"
          >
            <Plus :size="16" />
          </button>
        </div>
      </div>
      <AppInput
        :model-value="searchQuery"
        type="search"
        placeholder="筛选分组、标签、主机、用户"
        @update:model-value="emit('update:searchQuery', $event)"
      />
    </header>

    <!-- ============================================================
         Tree (scrollable middle)
         ============================================================ -->
    <div class="sidebar-tree" role="tree" aria-label="连接资产列表">
      <!-- Empty state: no assets at all -->
      <div v-if="!hasAssets" class="empty-state">
        <Server :size="28" class="empty-icon" />
        <p class="empty-text">尚未添加连接资产</p>
        <button
          type="button"
          class="empty-cta"
          @click="emit('create-asset')"
        >
          <Plus :size="14" /> 新增连接
        </button>
      </div>

      <!-- No filter result state -->
      <div v-else-if="!hasResults && hasQuery" class="empty-state">
        <p class="empty-text">无匹配「{{ searchQuery }}」的连接</p>
      </div>

      <!-- Asset tree -->
      <template v-else>
        <div
          v-for="group in visibleGroups"
          :key="group.name"
          class="group"
        >
          <button
            type="button"
            class="group-header"
            :aria-expanded="String(!isGroupCollapsed(group.name))"
            @click="toggleGroup(group.name)"
          >
            <component
              :is="isGroupCollapsed(group.name) ? ChevronRight : ChevronDown"
              :size="12"
              class="group-chevron"
            />
            <span class="group-name">{{ group.name }}</span>
            <span class="group-count">{{ group.items.length }}</span>
          </button>
          <ul v-show="!isGroupCollapsed(group.name)" class="group-items" role="group">
            <li
              v-for="asset in group.items"
              :key="asset.id"
              :ref="el => registerAssetEl(asset.id, el)"
              class="asset-node"
              :class="{ 'is-active': isActiveAsset(asset) }"
              role="treeitem"
              tabindex="0"
              :aria-selected="String(isActiveAsset(asset))"
              :title="`${asset.name} · ${asset.host} · ${asset.username}`"
              @click="emit('select-asset', asset.id)"
              @dblclick="emit('connect-asset', asset.id)"
              @keydown="onAssetKeydown($event, asset)"
            >
              <span class="dot" :class="statusClass(asset.status)" aria-hidden="true"></span>
              <div class="asset-body">
                <div class="asset-name">{{ asset.name }}</div>
                <div class="asset-meta">{{ asset.host }} · {{ asset.username }}</div>
              </div>
              <span v-if="assetBadge(asset)" class="asset-badge">{{ assetBadge(asset) }}</span>
            </li>
          </ul>
        </div>
      </template>
    </div>

    <!-- ============================================================
         Footer (sticky bottom): quick connect + hint
         ============================================================ -->
    <footer class="sidebar-footer">
      <AppInput
        :model-value="quickConnectInput"
        type="text"
        placeholder="ssh user@host[:port]"
        mono
        @update:model-value="emit('update:quickConnectInput', $event)"
        @keydown.enter.prevent="onQuickConnectEnter"
      />
      <p class="footer-hint">
        <Terminal :size="12" />
        <span>回车快速连接</span>
      </p>
    </footer>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.connection-sidebar {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-height: 0;
  background: var(--app-panel);
  color: var(--app-text);
  font-family: var(--font-body);
}

// ============================================================
// Header
// ============================================================
.sidebar-header {
  flex: 0 0 auto;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3);
  border-block-end: 1px solid var(--app-border);
  background: var(--app-panel);
  position: sticky;
  inset-block-start: 0;
  z-index: var(--z-sticky);
}

.title-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
}

.title {
  margin: 0;
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--app-strong);
  letter-spacing: var(--tracking-display);
}

.title-actions {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
}

.icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
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

.icon-btn:hover {
  background: var(--app-hover);
  color: var(--app-strong);
  border-color: var(--app-border);
}

.icon-btn:focus-visible {
  outline: none;
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

// ============================================================
// Tree
// ============================================================
.sidebar-tree {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  padding: var(--space-2) var(--space-1);
}

.group {
  display: flex;
  flex-direction: column;
  margin-block-end: var(--space-1);
}

.group-header {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  width: 100%;
  padding: var(--space-1) var(--space-2);
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
// Asset node
// ============================================================
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

// .dot is provided by global _utilities.scss; modifier classes here
// pair with the .running / .warn modifiers in that stylesheet.
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
// Empty / no-result state
// ============================================================
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  padding: var(--space-8) var(--space-4);
  text-align: center;
}

.empty-icon {
  color: var(--app-subtle);
}

.empty-text {
  margin: 0;
  font-size: var(--text-xs);
  color: var(--app-muted);
}

.empty-cta {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  padding: 4px 10px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-control);
  color: var(--app-text);
  font-size: var(--text-xs);
  font-weight: 500;
  cursor: pointer;
  transition: background var(--motion-fast) var(--ease-standard),
    border-color var(--motion-fast) var(--ease-standard);
}

.empty-cta:hover {
  background: var(--app-hover);
  border-color: var(--app-border-strong);
}

// ============================================================
// Footer
// ============================================================
.sidebar-footer {
  flex: 0 0 auto;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  padding: var(--space-3);
  border-block-start: 1px solid var(--app-border);
  background: var(--app-panel);
  position: sticky;
  inset-block-end: 0;
  z-index: var(--z-sticky);
}

.footer-hint {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  margin: 0;
  font-size: 10px;
  color: var(--app-subtle);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.footer-hint svg {
  color: var(--app-subtle);
}
</style>
