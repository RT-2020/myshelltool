<script setup>
import { computed, onBeforeUnmount, onMounted } from 'vue';
import { useWorkbenchStore } from './stores/workbench.js';
import { isTauriRuntime } from './services/backend.js';
import { usePanelResize } from './composables/usePanelResize.js';
import { useAutoUpdate } from './composables/useAutoUpdate.js';
import WorkbenchShell from './components/workbench/WorkbenchShell.vue';
import GlobalModals from './components/shell/GlobalModals.vue';
import TransferDrawer from './components/files/TransferDrawer.vue';

const store = useWorkbenchStore();
const panelResize = usePanelResize();
const autoUpdate = useAutoUpdate({ announce: msg => store.announce(msg) });
const desktopRuntimeAvailable = computed(() => isTauriRuntime());

onMounted(() => {
  store.initialize();
  autoUpdate.init();
  window.addEventListener('keydown', handleGlobalKeydown);
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', handleGlobalKeydown);
  store.disposeEventListeners();
});

function handleGlobalKeydown(event) {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k' && !event.shiftKey) {
    event.preventDefault();
    store.openGlobalSearch();
  }
  if (event.key === 'Escape' && store.searchState.open) store.closeGlobalSearch();
}

function openSettings(tab = 'about') {
  store.modal = { type: 'settings', tab, autoUpdate };
}

function createAsset() {
  store.modal = { type: 'assetEditor', asset: null };
}

function createGroup() {
  store.modal = { type: 'createGroup' };
}

function connectSelected() {
  store.connectSelected();
}

function openSync() {
  store.syncRefreshStatus();
  openSettings('sync');
}

function openMcpPanel() {
  store.refreshMcpStatus();
  openSettings('mcp');
}

function resetLayout() {
  panelResize.resetLayout();
  store.announce('布局已恢复默认');
}

function toggleAssets() {
  store.toggleAssets();
  panelResize.syncCollapse();
}

function toggleRight() {
  store.toggleRight();
  panelResize.syncCollapse();
}
</script>

<template>
  <div class="window">
    <div v-if="!desktopRuntimeAvailable" class="desktop-only-banner">
      <strong>桌面客户端模式未启动。</strong>
      SSH/SFTP/隧道功能需要 Tauri 桌面运行时。请运行
      <code>npm run tauri:dev</code> 或 <code>npm run tauri:build</code>。
    </div>

    <WorkbenchShell
      :store="store"
      :desktop-runtime-available="desktopRuntimeAvailable"
      :panel-resize="panelResize"
      @create-asset="createAsset"
      @create-group="createGroup"
      @connect-selected="connectSelected"
      @open-settings="openSettings"
      @open-sync="openSync"
      @open-mcp-panel="openMcpPanel"
      @toggle-theme="store.toggleTheme"
      @toggle-assets="toggleAssets"
      @toggle-right="toggleRight"
      @reset-layout="resetLayout"
      @toggle-transfer-drawer="store.toggleTransferDrawer"
    />

    <TransferDrawer :open="store.transferDrawerOpen" @toggle="store.toggleTransferDrawer" />
    <GlobalModals />
  </div>
</template>

<style>
.desktop-only-banner {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  z-index: var(--z-toast, 9999);
  padding: 10px 24px;
  background: #fef3c7;
  color: #78350f;
  border-bottom: 1px solid #f59e0b;
  font-size: 13px;
  text-align: center;
}

.desktop-only-banner code {
  padding: 2px 6px;
  border-radius: 3px;
  background: rgba(0, 0, 0, 0.08);
  font-family: var(--font-mono);
}

.window {
  width: 100vw;
  height: 100vh;
  overflow: hidden;
}
</style>
