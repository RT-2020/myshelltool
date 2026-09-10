<script setup lang="ts">
import { computed, nextTick, onMounted, ref, unref, watch } from 'vue';
import {
  ArrowUpDown,
  Minus,
  Moon,
  PanelRight,
  Search,
  Settings,
  Sparkles,
  Square,
  Sun,
  TerminalSquare,
  X,
} from 'lucide-vue-next';
import ConnectionSidebar from '@/components/shell/ConnectionSidebar.vue';
import RightSidebar from '@/components/shell/RightSidebar.vue';
import TerminalSurface from '@/components/terminal/TerminalSurface.vue';
import FileSurface from '@/components/files/FileSurface.vue';
import { AppBrandLogo } from '@/components/ui';
import {
  closeTauriWindow,
  isTauriRuntime,
  isTauriWindowMaximized,
  minimizeTauriWindow,
  startTauriWindowDragging,
  toggleTauriWindowMaximize
} from '@/services/backend';
import type { useWorkbenchStore } from '@/stores/workbench';
import type { usePanelResize, ResizeRegion } from '@/composables/usePanelResize';
import type { useAutoUpdate } from '@/composables/useAutoUpdate';
import type { NormalizedConnectionAsset, ModalState, SearchSuggestion } from '@/types/domain';

const props = withDefaults(defineProps<{
  store: ReturnType<typeof useWorkbenchStore>;
  desktopRuntimeAvailable?: boolean;
  panelResize?: ReturnType<typeof usePanelResize> | null;
  // App.vue 的 useAutoUpdate 实例（statusbar 更新提示可点击 + 设置图标徽标）
  autoUpdate?: ReturnType<typeof useAutoUpdate> | null;
}>(), {
  desktopRuntimeAvailable: false,
  panelResize: null,
  autoUpdate: null
});

const emit = defineEmits<{
  'create-asset': [];
  'create-group': [];
  'connect-selected': [];
  'open-settings': [];
  'open-mcp-panel': [];
  'toggle-theme': [];
  'toggle-assets': [];
  'toggle-right': [];
  'toggle-transfer-drawer': [];
}>();

const sidebarSearch = ref('');
const quickConnect = ref('');
const isMaximized = ref(false);
const searchInputRef = ref<HTMLInputElement | null>(null);
const activeSearchIndex = ref(0);

const activeTransferCount = computed(() => props.store.activeTransfers?.length || 0);
const syncText = computed(() => props.store.syncText || '未配置同步');
const mcpText = computed(() => props.store.mcpClientConnected ? 'MCP 可用' : 'MCP 不可用');
// 应用内更新状态：available → 状态栏消息可点击下载安装 + 设置图标红点；
// error → 状态栏消息可点击重试；其余状态保持纯文本（aria-live）。
const updateState = computed(() => unref(props.autoUpdate?.state) || 'idle');
const updateClickable = computed(() => updateState.value === 'available' || updateState.value === 'error');
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

function updateSearchQuery(event: Event) {
  props.store.setGlobalSearchQuery((event.target as HTMLInputElement).value);
}

function activateSearchSuggestion(item: SearchSuggestion) {
  props.store.activateSuggestion(item);
}

function onSearchKeydown(event: KeyboardEvent) {
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

function selectAsset(id: string) {
  props.store.selectAsset(id);
}

function connectAsset(id: string) {
  props.store.selectAsset(id);
  props.store.connectSelected();
}

// ConnectionSidebar 的 quick-connect 解析结果（username/host/port）
function quickConnectAsset(parsed: { username: string; host: string; port: number }) {
  // SearchSuggestion 的 quick-connect 变体含 label（仅搜索列表展示用），激活路径不消费；
  // 透传保持原 payload 不加字段，故此处断言。
  props.store.activateSuggestion({ kind: 'quick-connect', ...parsed } as SearchSuggestion);
}

function editAsset(asset: NormalizedConnectionAsset) {
  props.store.selectAsset(asset.id, false);
  props.store.modal = { type: 'assetEditor', asset };
}

function duplicateAsset(asset: NormalizedConnectionAsset) {
  props.store.duplicateAsset(asset);
}

function deleteAsset(asset: NormalizedConnectionAsset) {
  props.store.modal = { type: 'confirmDelete', asset };
}

function moveAsset(asset: NormalizedConnectionAsset) {
  props.store.modal = { type: 'moveAsset', asset };
}

function moveAssetDirect(payload: { id: string; group: string }) {
  props.store.moveAsset(payload.id, payload.group);
}

function renameGroup(path: string) {
  props.store.modal = { type: 'renameGroup', path } as ModalState;
}

function dissolveGroup(path: string) {
  props.store.dissolveGroup(path);
}

function reorderGroups(paths: string[]) {
  props.store.reorderGroups(paths);
}

function startResize(event: PointerEvent, which: ResizeRegion) {
  props.panelResize?.startResize?.(event, which);
}

function resizeKeydown(event: KeyboardEvent, which: ResizeRegion) {
  props.panelResize?.handleResizeKeydown?.(event, which);
}

function resetPane(which: ResizeRegion) {
  props.panelResize?.resetPane?.(which);
}

async function syncWindowState() {
  isMaximized.value = await isTauriWindowMaximized();
}

async function handleTitlebarPointerDown(event: PointerEvent) {
  if (!isTauriRuntime() || event.button !== 0) return;
  if ((event.target as HTMLElement).closest('button, input, [role="button"], [data-no-drag="true"]')) return;
  await startTauriWindowDragging();
}

async function handleTitlebarDoubleClick(event: MouseEvent) {
  if (!isTauriRuntime()) return;
  if ((event.target as HTMLElement).closest('button, input, [role="button"], [data-no-drag="true"]')) return;
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
          <AppBrandLogo :size="22" />
          <span class="tb-name">myshelltool</span>
        </div>
      </div>

      <div class="tb-center" data-tauri-drag-region>
        <div
          class="tb-search"
          data-no-drag="true"
          role="combobox"
          :aria-expanded="searchOpen ? 'true' : 'false'"
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
          <kbd>Ctrl K</kbd>
          <ul v-if="searchOpen" class="tb-suggestions" role="listbox">
            <li
              v-for="(item, idx) in searchSuggestions"
              :key="item.kind === 'asset' ? item.asset.id : `${item.kind}-${item.host}-${idx}`"
              :class="{ active: idx === activeSearchIndex }"
              role="option"
              :aria-selected="idx === activeSearchIndex ? 'true' : 'false'"
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
        <button class="icon-btn" type="button" aria-label="切换主题" title="切换主题" @click="emit('toggle-theme')">
          <Sun v-if="store.effectiveTheme === 'light'" />
          <Moon v-else />
        </button>
        <button
          class="icon-btn"
          type="button"
          aria-label="收起或展开右侧面板"
          :title="store.rightCollapsed ? '展开右侧面板' : '收起右侧面板'"
          :aria-pressed="store.rightCollapsed ? 'true' : 'false'"
          @click="emit('toggle-right')"
        >
          <PanelRight />
        </button>
        <button
          class="icon-btn"
          :class="{ 'has-update': updateState === 'available' }"
          type="button"
          aria-label="打开设置"
          :title="updateState === 'available' ? '设置（有新版本）' : '设置'"
          @click="emit('open-settings')"
        >
          <Settings />
          <span v-if="updateState === 'available'" class="update-badge" aria-hidden="true"></span>
        </button>
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
        :selected-asset-id="store.selectedAssetId as string | undefined"
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
        <span class="sb-item muted status-text" aria-live="polite">{{ store.statusMessage || '就绪' }}</span>
        <button
          v-if="updateState === 'available'"
          class="sb-item sb-update-pill is-available"
          type="button"
          title="点击下载并安装新版本"
          @click="props.autoUpdate?.onClick()"
        >
          <Sparkles :size="12" aria-hidden="true" />
          <span>新版本就绪</span>
        </button>
      </div>
      <div class="sb-right">
        <button class="sb-item transfer-pill" type="button" aria-label="打开传输队列" @click="emit('toggle-transfer-drawer')">
          <ArrowUpDown :size="12" aria-hidden="true" />
          <span class="mono">{{ activeTransferCount }}</span>
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
