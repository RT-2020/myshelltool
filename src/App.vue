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
import { usePanelResize } from './composables/usePanelResize.js';
import { useAutoUpdate } from './composables/useAutoUpdate.js';
import AppShellLayout from './components/shell/AppShellLayout.vue';
import AppTitleBar from './components/shell/AppTitleBar.vue';
import AppStatusBar from './components/shell/AppStatusBar.vue';
import ConnectionSidebar from './components/shell/ConnectionSidebar.vue';
import OpsSummaryPanel from './components/shell/OpsSummaryPanel.vue';
import GlobalModals from './components/shell/GlobalModals.vue';
import TerminalSurface from './components/terminal/TerminalSurface.vue';
import FileSurface from './components/files/FileSurface.vue';
import TransferDrawer from './components/files/TransferDrawer.vue';
import ResourceMonitorPanel from './components/resource-monitor/ResourceMonitorPanel.vue';

const desktopRuntimeAvailable = computed(() => isTauriRuntime());
const store = useWorkbenchStore();
const {
  backendStatus, activeSessions, themeLabel, assetsCollapsed, rightCollapsed,
  statusMessage, assets, groupedAssets, selectedAssetId, searchState,
  runningTunnels, tunnels, warningCount, syncText, mcpClientConnected,
  // v2：传输胶囊数据 + sheet 开关（TransferDrawer 全局挂载，原 FileSurface 底部 trigger 已删）。
  activeTransfers, completedTransfers, transferDrawerOpen
} = storeToRefs(store);

// 面板拖拽布局（三列宽 + 中间两行高），reset/syncCollapse 供标题栏按钮与折叠态调用。
const panelResize = usePanelResize();

// 应用内自动更新（v1.7）：发现新版本时把提示写入 statusMessage，点击状态栏触发下载安装。
// 浏览器预览模式（无 Tauri runtime）下 init 静默 no-op。
const autoUpdate = useAutoUpdate({ announce: msg => store.announce(msg) });

const sidebarSearch = ref('');
const sidebarQuickConnect = ref('');

onMounted(() => {
  store.initialize();
  autoUpdate.init(); // 异步检查更新，失败静默
  window.addEventListener('keydown', handleGlobalKeydown);
});
onBeforeUnmount(() => {
  window.removeEventListener('keydown', handleGlobalKeydown);
  store.disposeEventListeners();
});

// App-level shortcuts only — surface shortcuts (terminal/files) live in
// TerminalSurface / FileSurface. Ctrl+K 打开全局搜索（聚焦由 AppTitleBar 自行处理）。
function handleGlobalKeydown(event) {
  if ((event.ctrlKey || event.metaKey) && (event.key === 'k' || event.key === 'K') && !event.shiftKey) {
    event.preventDefault();
    store.openGlobalSearch();
    return;
  }
  if (event.key === 'Escape' && searchState.value.open) store.closeGlobalSearch();
}

// Sidebar emits
function onSelectAsset(id) { store.selectAsset(id); }
function onConnectAsset(id) { store.selectAsset(id); store.connectSelected(); }
function onQuickConnect(parsed) { store.activateSuggestion({ kind: 'quick-connect', ...parsed }); }
function onCreateAsset() { store.modal = { type: 'assetEditor', asset: null }; }
// v1.2：状态栏 MCP 指示灯点击 → 打开 MCP 管理面板（配置引导 + 能力清单）。
function onOpenMcpPanel() {
  store.refreshMcpStatus();
  store.modal = { type: 'mcpPanel' };
}
// 侧栏折叠：store 切换后同步拖拽内联变量（折叠列清除内联让 44px 生效）。
function onToggleAssets() {
  store.toggleAssets();
  panelResize.syncCollapse();
}
// 资产管理：编辑/复制/删除（资产对象）
function onEditAsset(asset) {
  store.selectAsset(asset.id, false);
  store.modal = { type: 'assetEditor', asset };
}
function onDuplicateAsset(asset) { store.duplicateAsset(asset); }
function onDeleteAsset(asset) { store.modal = { type: 'confirmDelete', asset }; }
// 分组管理：新建/重命名/解散/移动
function onCreateGroup() { store.modal = { type: 'createGroup' }; }
function onRenameGroup(path) { store.modal = { type: 'renameGroup', path }; }
function onDissolveGroup(path) { store.dissolveGroup(path); }
function onMoveAsset(asset) { store.modal = { type: 'moveAsset', asset }; }
// 拖拽：直接落盘，不走弹窗
function onMoveAssetDirect({ id, group }) { store.moveAsset(id, group); }
function onReorderGroups(orderedPaths) { store.reorderGroups(orderedPaths); }

// Titlebar emits
function onToggleTheme() { store.toggleTheme(); }
function onOpenSync() {
  // v1.3：标题栏「同步」→ 打开 Gist 同步管理面板（setup/push/pull/冲突/重置/清空）。
  // PAT 配置作为 syncPanel 内的前置提示（未配 PAT 时面板会引导）。
  store.syncRefreshStatus();
  store.modal = { type: 'syncPanel' };
}
function onToggleWarnings() { store.announce(`当前 warning：${warningCount.value}`); }
function onToggleRight() {
  store.toggleRight();
  // 折叠态变化后同步内联变量：折叠列清除内联（让 dataset 选择器的 0 生效），
  // 展开列恢复上次拖拽宽度。
  panelResize.syncCollapse();
}
function onResetLayout() {
  // usePanelResize 在 setup 顶部实例化，resetLayout 清 localStorage + 重置内联变量
  panelResize.resetLayout();
  store.announce('布局已恢复默认');
}
function onOpenSettings() { store.announce('设置功能开发中'); }
function onActivateSuggestion(item) { store.activateSuggestion(item); }

// Statusbar emits
// 点击状态栏消息：若当前是更新提示（由 useAutoUpdate 写入），触发更新流程；
// 否则回显消息（旧行为）。
function onClickStatus() {
  const msg = statusMessage.value || '就绪';
  if (msg.includes('更新')) {
    autoUpdate.onClick();
    return;
  }
  store.announce(msg);
}
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

    <AppShellLayout
      :start-resize="panelResize.startResize"
      :sidebar-collapsed="assetsCollapsed"
      :right-collapsed="rightCollapsed"
    >
      <template #titlebar>
        <AppTitleBar
          :theme-label="themeLabel"
          :warning-count="warningCount"
          :right-collapsed="rightCollapsed"
          :search-query="searchState.query"
          :search-state="searchState"
          @update:search-query="store.setGlobalSearchQuery($event)"
          @toggle-theme="onToggleTheme"
          @open-sync="onOpenSync"
          @toggle-warnings="onToggleWarnings"
          @toggle-right="onToggleRight"
          @reset-layout="onResetLayout"
          @open-settings="onOpenSettings"
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
          @toggle-collapse="onToggleAssets"
          @create-asset="onCreateAsset"
          @create-group="onCreateGroup"
          @edit-asset="onEditAsset"
          @duplicate-asset="onDuplicateAsset"
          @delete-asset="onDeleteAsset"
          @move-asset="onMoveAsset"
          @move-asset-direct="onMoveAssetDirect"
          @reorder-groups="onReorderGroups"
          @rename-group="onRenameGroup"
          @dissolve-group="onDissolveGroup"
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
          :mcp-connected="mcpClientConnected"
          :active-transfers="activeTransfers.length"
          :completed-transfers="completedTransfers.length"
          :transfer-drawer-open="transferDrawerOpen"
          @click-status="onClickStatus"
          @toggle-transfer-drawer="onToggleTransferDrawer"
          @open-mcp-panel="onOpenMcpPanel"
        />
      </template>
    </AppShellLayout>

    <!-- 全局传输 sheet：Teleport 到 body，由状态栏「传输」胶囊按钮控制开关。
         v2 从 FileSurface 底部迁出（原 trigger bar 已删），与终端/文件区 tab 解耦。 -->
    <TransferDrawer :open="transferDrawerOpen" @toggle="onToggleTransferDrawer" />

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
