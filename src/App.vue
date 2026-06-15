<script setup>
/**
 * App.vue — Wave 3 Step 3.5
 * Pure skeleton. Mounts the 5-region AppShellLayout + GlobalModals.
 * All surface UI lives in dedicated components (TerminalSurface,
 * FileSurface, ConnectionSidebar, AppTitleBar, AppStatusBar).
 * Right region is a Wave 4 placeholder.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { isTauriRuntime } from './services/backend.js';
// useWorkbenchStore is the ONLY workbench import in App.vue. It is needed
// for store.initialize() — the function that orchestrates the 5-store
// cross-attach (sessions ↔ files ↔ tunnels ↔ assets ↔ ui bridges). No
// domain store alone exposes that bootstrap. Wave 5 deslop may move the
// orchestration into a composable; until then, this is the canonical
// entry point. See .omc/plans/ui-full-refactor-consensus.md Step 3.5.
import { useWorkbenchStore } from './stores/workbench.js';
import AppShellLayout from './components/shell/AppShellLayout.vue';
import AppTitleBar from './components/shell/AppTitleBar.vue';
import AppStatusBar from './components/shell/AppStatusBar.vue';
import ConnectionSidebar from './components/shell/ConnectionSidebar.vue';
import OpsSummaryPanel from './components/shell/OpsSummaryPanel.vue';
import GlobalModals from './components/shell/GlobalModals.vue';
import TerminalSurface from './components/terminal/TerminalSurface.vue';
import FileSurface from './components/files/FileSurface.vue';
import ResourceMonitorPanel from './components/resource-monitor/ResourceMonitorPanel.vue';

const desktopRuntimeAvailable = computed(() => isTauriRuntime());
const store = useWorkbenchStore();
const {
  backendStatus, activeSessions, themeLabel, assetsCollapsed,
  statusMessage, assets, groupedAssets, selectedAssetId, searchState,
  runningTunnels, tunnels, warningCount, syncText
} = storeToRefs(store);

const sidebarSearch = ref('');
const sidebarQuickConnect = ref('');
const globalSearchInput = ref(null);

onMounted(() => {
  store.initialize();
  window.addEventListener('keydown', handleGlobalKeydown);
});
onBeforeUnmount(() => {
  window.removeEventListener('keydown', handleGlobalKeydown);
  store.disposeEventListeners();
});

// App-level shortcuts only — surface shortcuts (terminal/files) live in
// TerminalSurface / FileSurface.
function handleGlobalKeydown(event) {
  if ((event.ctrlKey || event.metaKey) && (event.key === 'k' || event.key === 'K') && !event.shiftKey) {
    event.preventDefault();
    store.openGlobalSearch();
    setTimeout(() => globalSearchInput.value?.focus(), 30);
    return;
  }
  if (event.key === 'Escape' && searchState.value.open) store.closeGlobalSearch();
}

// Sidebar emits
function onSelectAsset(id) { store.selectAsset(id); }
function onConnectAsset(id) { store.selectAsset(id); store.connectSelected(); }
function onQuickConnect(parsed) { store.activateSuggestion({ kind: 'quick-connect', ...parsed }); }
function onCreateAsset() { store.modal = { type: 'assetEditor', asset: null }; }

// Titlebar emits
function onToggleTheme() { store.toggleTheme(); }
function onOpenSync() { store.modal = { type: 'tokenConfig', asset: null }; }
function onToggleWarnings() { store.announce(`当前 warning：${warningCount.value}`); }
function onActivateSuggestion(item) { store.activateSuggestion(item); }

// Statusbar emits
function onClickStatus() { store.announce(statusMessage.value || '就绪'); }
function onToggleTransferDrawer() { store.toggleTransferDrawer(); }
</script>

<template>
  <div class="window">
    <!-- Banner preserved verbatim for tests/ui-smoke.mjs (.desktop-only-banner +
         "桌面客户端模式未启动" text assertion). -->
    <div v-if="!desktopRuntimeAvailable" class="desktop-only-banner">
      <strong>桌面客户端模式未启动。</strong>
      SSH/SFTP/隧道功能需要 Tauri 桌面运行时。请运行
      <code>npm run tauri:dev</code>（开发）或 <code>npm run tauri:build</code>（打包）。
    </div>

    <AppShellLayout>
      <template #titlebar>
        <AppTitleBar
          :backend-ready="backendStatus.ready"
          :active-sessions="activeSessions"
          :theme-label="themeLabel"
          :warning-count="warningCount"
          :search-query="searchState.query"
          :search-state="searchState"
          @update:search-query="store.setGlobalSearchQuery($event)"
          @toggle-theme="onToggleTheme"
          @open-sync="onOpenSync"
          @toggle-warnings="onToggleWarnings"
          @activate-suggestion="onActivateSuggestion"
        />
      </template>

      <template #sidebar>
        <ConnectionSidebar
          :assets="assets"
          :grouped-assets="groupedAssets"
          :selected-asset-id="selectedAssetId"
          :assets-collapsed="assetsCollapsed"
          :search-query="sidebarSearch"
          :quick-connect-input="sidebarQuickConnect"
          @update:search-query="sidebarSearch = $event"
          @update:quick-connect-input="sidebarQuickConnect = $event"
          @select-asset="onSelectAsset"
          @connect-asset="onConnectAsset"
          @quick-connect="onQuickConnect"
          @toggle-collapse="store.toggleAssets"
          @create-asset="onCreateAsset"
        />
      </template>

      <template #center-top><TerminalSurface /></template>
      <template #center-bottom><FileSurface /></template>

      <template #right>
        <ResourceMonitorPanel />
        <OpsSummaryPanel />
      </template>

      <template #statusbar>
        <AppStatusBar
          :active-sessions="activeSessions"
          :backend-mode="backendStatus.mode"
          :status-message="statusMessage"
          :running-tunnels="runningTunnels"
          :total-tunnels="tunnels.length"
          :warning-count="warningCount"
          :sync-text="syncText"
          @click-status="onClickStatus"
          @toggle-transfer-drawer="onToggleTransferDrawer"
        />
      </template>
    </AppShellLayout>

    <GlobalModals />
  </div>
</template>

<style>
/* Global minimal styles — visual chrome lives in src/styles/main.scss.
   These selectors are preserved for tests/ui-smoke.mjs: .desktop-only-banner
   and .window. */
.desktop-only-banner {
  position: fixed; top: 0; left: 0; right: 0; z-index: var(--z-toast, 9999);
  padding: 14px 24px; background: #fef3c7; color: #78350f;
  border-bottom: 1px solid #f59e0b; font-size: 14px; text-align: center;
}
.desktop-only-banner code {
  background: rgba(0, 0, 0, 0.08); padding: 2px 6px; border-radius: 3px;
  font-family: ui-monospace, SFMono-Regular, monospace;
}
.window { width: 100vw; height: 100vh; overflow: hidden; }
.muted { color: var(--app-muted, #888); font-size: 11px; }
</style>
