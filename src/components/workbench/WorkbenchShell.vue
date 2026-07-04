<script setup>
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import {
  Menu,
  Minus,
  Moon,
  PanelRight,
  RefreshCw,
  Search,
  Settings,
  Square,
  Sun,
  TerminalSquare,
  X,
} from 'lucide-vue-next';
import ConnectionSidebar from '@/components/shell/ConnectionSidebar.vue';
import RightSidebar from '@/components/shell/RightSidebar.vue';
import TerminalSurface from '@/components/terminal/TerminalSurface.vue';
import FileSurface from '@/components/files/FileSurface.vue';
import {
  closeTauriWindow,
  isTauriRuntime,
  isTauriWindowMaximized,
  minimizeTauriWindow,
  startTauriWindowDragging,
  toggleTauriWindowMaximize
} from '@/services/backend.js';

const props = defineProps({
  store: { type: Object, required: true },
  desktopRuntimeAvailable: { type: Boolean, default: false },
  panelResize: { type: Object, default: null }
});

const emit = defineEmits([
  'create-asset',
  'create-group',
  'connect-selected',
  'open-settings',
  'open-sync',
  'open-mcp-panel',
  'toggle-theme',
  'toggle-assets',
  'toggle-right',
  'reset-layout',
  'toggle-transfer-drawer'
]);

const sidebarSearch = ref('');
const quickConnect = ref('');
const menuOpen = ref(false);
const isMaximized = ref(false);
const searchInputRef = ref(null);
const activeSearchIndex = ref(0);

const backendMode = computed(() => props.store.backendStatus?.mode || 'tauri');
const activeTransferCount = computed(() => props.store.activeTransfers?.length || 0);
const completedTransferCount = computed(() => props.store.completedTransfers?.length || 0);
const syncText = computed(() => props.store.syncText || '未配置同步');
const mcpText = computed(() => props.store.mcpClientConnected ? 'MCP 可用' : 'MCP 不可用');
const appClasses = computed(() => ({
  'sidebar-collapsed': props.store.assetsCollapsed,
  'right-collapsed': props.store.rightCollapsed
}));
const desktopWindowControlsVisible = computed(() => isTauriRuntime());
const searchState = computed(() => props.store.searchState || { open: false, query: '', suggestions: [] });
const searchSuggestions = computed(() => searchState.value.suggestions || []);
const searchOpen = computed(() => Boolean(searchState.value.open && searchSuggestions.value.length));

watch(
  () => searchState.value.open,
  open => {
    if (open) nextTick(() => searchInputRef.value?.focus());
  }
);

watch(searchSuggestions, () => {
  activeSearchIndex.value = 0;
});

function openSearch() {
  props.store.openGlobalSearch();
  nextTick(() => searchInputRef.value?.focus());
}

function updateSearchQuery(event) {
  props.store.setGlobalSearchQuery(event.target.value);
}

function activateSearchSuggestion(item) {
  props.store.activateSuggestion(item);
}

function onSearchKeydown(event) {
  if (event.key === 'Escape') {
    event.preventDefault();
    props.store.closeGlobalSearch();
    searchInputRef.value?.blur();
    return;
  }
  if (!searchSuggestions.value.length) return;
  if (event.key === 'ArrowDown') {
    event.preventDefault();
    activeSearchIndex.value = Math.min(activeSearchIndex.value + 1, searchSuggestions.value.length - 1);
    return;
  }
  if (event.key === 'ArrowUp') {
    event.preventDefault();
    activeSearchIndex.value = Math.max(activeSearchIndex.value - 1, 0);
    return;
  }
  if (event.key === 'Enter') {
    event.preventDefault();
    activateSearchSuggestion(searchSuggestions.value[activeSearchIndex.value]);
  }
}

function selectAsset(id) {
  props.store.selectAsset(id);
}

function connectAsset(id) {
  props.store.selectAsset(id);
  props.store.connectSelected();
}

function quickConnectAsset(parsed) {
  props.store.activateSuggestion({ kind: 'quick-connect', ...parsed });
}

function editAsset(asset) {
  props.store.selectAsset(asset.id, false);
  props.store.modal = { type: 'assetEditor', asset };
}

function duplicateAsset(asset) {
  props.store.duplicateAsset(asset);
}

function deleteAsset(asset) {
  props.store.modal = { type: 'confirmDelete', asset };
}

function moveAsset(asset) {
  props.store.modal = { type: 'moveAsset', asset };
}

function moveAssetDirect(payload) {
  props.store.moveAsset(payload.id, payload.group);
}

function renameGroup(path) {
  props.store.modal = { type: 'renameGroup', path };
}

function dissolveGroup(path) {
  props.store.dissolveGroup(path);
}

function reorderGroups(paths) {
  props.store.reorderGroups(paths);
}

function startResize(event, which) {
  props.panelResize?.startResize?.(event, which);
}

function resizeKeydown(event, which) {
  props.panelResize?.handleResizeKeydown?.(event, which);
}

function resetPane(which) {
  props.panelResize?.resetPane?.(which);
}

async function syncWindowState() {
  isMaximized.value = await isTauriWindowMaximized();
}

async function handleTitlebarPointerDown(event) {
  if (!isTauriRuntime() || event.button !== 0) return;
  if (event.target.closest('button, input, [role="button"], [data-no-drag="true"]')) return;
  await startTauriWindowDragging();
}

async function handleTitlebarDoubleClick(event) {
  if (!isTauriRuntime()) return;
  if (event.target.closest('button, input, [role="button"], [data-no-drag="true"]')) return;
  await toggleWindowMaximize();
}

async function toggleWindowMaximize() {
  await toggleTauriWindowMaximize();
  await syncWindowState();
}

async function minimizeWindow() {
  await minimizeTauriWindow();
}

async function closeWindow() {
  await closeTauriWindow();
}

onMounted(() => {
  syncWindowState();
});
</script>

<template>
  <div class="workbench-shell app" :class="appClasses" role="application" aria-label="myshelltool">
    <header
      class="titlebar"
      data-region="titlebar"
      @pointerdown="handleTitlebarPointerDown"
      @dblclick="handleTitlebarDoubleClick"
    >
      <div class="tb-left" data-tauri-drag-region>
        <div class="tb-brand">
          <TerminalSquare :size="16" />
          <span class="tb-name">myshelltool</span>
          <span class="tb-sep">·</span>
          <span class="tb-project">未命名工作区</span>
        </div>
      </div>

      <div class="tb-center" data-tauri-drag-region>
        <div
          class="tb-search"
          data-no-drag="true"
          role="combobox"
          :aria-expanded="String(searchOpen)"
          aria-label="全局搜索"
          @click="openSearch"
        >
          <Search :size="14" />
          <input
            ref="searchInputRef"
            class="tb-search-input tb-search-text"
            type="search"
            :value="searchState.query"
            placeholder="搜索连接 / 命令 / 文件"
            spellcheck="false"
            @input="updateSearchQuery"
            @keydown="onSearchKeydown"
          />
          <kbd>⌘K</kbd>
          <ul v-if="searchOpen" class="tb-suggestions" role="listbox">
            <li
              v-for="(item, idx) in searchSuggestions"
              :key="item.kind === 'asset' ? item.asset.id : `${item.kind}-${item.host}-${idx}`"
              :class="{ active: idx === activeSearchIndex }"
              role="option"
              :aria-selected="String(idx === activeSearchIndex)"
              @mouseenter="activeSearchIndex = idx"
              @mousedown.prevent="activateSearchSuggestion(item)"
            >
              <strong>{{ item.kind === 'asset' ? item.asset.name : item.label }}</strong>
              <span>
                {{ item.kind === 'asset' ? `${item.asset.username}@${item.asset.host}` : `${item.username}@${item.host}:${item.port}` }}
              </span>
            </li>
          </ul>
        </div>
      </div>

      <div class="tb-right">
        <button class="icon-btn" type="button" title="切换主题" @click="emit('toggle-theme')">
          <Sun v-if="store.effectiveTheme === 'light'" />
          <Moon v-else />
        </button>
        <button class="icon-btn" type="button" title="同步配置" @click="emit('open-sync')">
          <RefreshCw />
        </button>
        <button
          class="icon-btn"
          type="button"
          :title="store.rightCollapsed ? '展开右侧面板' : '收起右侧面板'"
          :aria-pressed="String(store.rightCollapsed)"
          @click="emit('toggle-right')"
        >
          <PanelRight />
        </button>
        <button class="icon-btn" type="button" title="设置" @click="emit('open-settings')">
          <Settings />
        </button>
        <div class="tb-menu" :data-open="String(menuOpen)">
          <button class="icon-btn" type="button" title="布局菜单" @click="menuOpen = !menuOpen">
            <Menu />
          </button>
          <div class="menu-popover" role="menu">
            <button class="menu-item" type="button" @click="emit('reset-layout'); menuOpen = false">
              <span class="menu-item-label">恢复默认布局</span>
              <span class="menu-item-hint">⌘0</span>
            </button>
          </div>
        </div>
        <div v-if="desktopWindowControlsVisible" class="window-controls" data-no-drag="true" aria-label="窗口控制">
          <button class="window-btn" type="button" aria-label="最小化" title="最小化" @click="minimizeWindow">
            <Minus :size="14" />
          </button>
          <button
            class="window-btn"
            type="button"
            :aria-label="isMaximized ? '还原窗口' : '最大化'"
            :title="isMaximized ? '还原窗口' : '最大化'"
            @click="toggleWindowMaximize"
          >
            <Square :size="12" />
          </button>
          <button class="window-btn danger" type="button" aria-label="关闭" title="关闭" @click="closeWindow">
            <X :size="14" />
          </button>
        </div>
      </div>
    </header>

    <aside class="sidebar-region" data-region="sidebar" aria-label="连接资产">
      <ConnectionSidebar
        :assets="store.assets"
        :grouped-assets="store.groupedAssets"
        :selected-asset-id="store.selectedAssetId"
        :assets-collapsed="store.assetsCollapsed"
        :search-query="sidebarSearch"
        :quick-connect-input="quickConnect"
        @update:search-query="sidebarSearch = $event"
        @update:quick-connect-input="quickConnect = $event"
        @select-asset="selectAsset"
        @connect-asset="connectAsset"
        @quick-connect="quickConnectAsset"
        @toggle-collapse="emit('toggle-assets')"
        @create-asset="emit('create-asset')"
        @create-group="emit('create-group')"
        @edit-asset="editAsset"
        @delete-asset="deleteAsset"
        @duplicate-asset="duplicateAsset"
        @move-asset="moveAsset"
        @rename-group="renameGroup"
        @dissolve-group="dissolveGroup"
        @move-asset-direct="moveAssetDirect"
        @reorder-groups="reorderGroups"
      />
    </aside>

    <div
      class="resize resize-h"
      :class="{ active: panelResize?.resizing?.value === 'sidebar' }"
      data-target="sidebar"
      role="separator"
      aria-orientation="vertical"
      aria-label="调整左侧栏宽度"
      tabindex="0"
      @pointerdown="startResize($event, 'sidebar')"
      @keydown="resizeKeydown($event, 'sidebar')"
      @dblclick="resetPane('sidebar')"
    ></div>

    <main class="main">
      <TerminalSurface data-region="center-top" />
      <FileSurface data-region="center-bottom" />
    </main>

    <div
      class="resize resize-v"
      :class="{ active: panelResize?.resizing?.value === 'center-row' }"
      data-target="split"
      role="separator"
      aria-orientation="horizontal"
      aria-label="调整终端与文件高度"
      tabindex="0"
      @pointerdown="startResize($event, 'center-row')"
      @keydown="resizeKeydown($event, 'center-row')"
      @dblclick="resetPane('center-row')"
    ></div>

    <div
      class="resize resize-h"
      :class="{ active: panelResize?.resizing?.value === 'right' }"
      data-target="right"
      role="separator"
      aria-orientation="vertical"
      aria-label="调整右侧栏宽度"
      tabindex="0"
      @pointerdown="startResize($event, 'right')"
      @keydown="resizeKeydown($event, 'right')"
      @dblclick="resetPane('right')"
    ></div>

    <RightSidebar @collapse="emit('toggle-right')" />

    <footer class="statusbar app-status-bar" data-region="statusbar">
      <div class="sb-left">
        <span class="sb-item"><span class="conn-dot idle"></span>SSH 空闲 · 后端 {{ backendMode }}</span>
        <span class="sb-sep">·</span>
        <span class="sb-item muted">{{ store.statusMessage || '无新消息' }}</span>
      </div>
      <div class="sb-center">
        <span class="sb-item mono">UTF-8</span>
        <span class="sb-sep">·</span>
        <span class="sb-item mono">LF</span>
        <span class="sb-sep">·</span>
        <span class="sb-item mono">zsh</span>
      </div>
      <div class="sb-right">
        <button class="sb-item transfer-pill" type="button" @click="emit('toggle-transfer-drawer')">
          传输 <span class="mono">{{ activeTransferCount }} / {{ completedTransferCount }}</span>
        </button>
        <span class="sb-sep">·</span>
        <span class="badge muted">{{ syncText }}</span>
        <span class="sb-sep">·</span>
        <button
          class="badge"
          :class="store.mcpClientConnected ? 'success' : 'warn'"
          type="button"
          @click="emit('open-mcp-panel')"
        >
          {{ mcpText }}
        </button>
      </div>
    </footer>
  </div>
</template>
