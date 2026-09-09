<script setup>
import { computed, onBeforeUnmount, onMounted } from 'vue';
import { useWorkbenchStore } from './stores/workbench.js';
import { isTauriRuntime } from './services/backend.js';
import { usePanelResize } from './composables/usePanelResize.js';
import { useAutoUpdate } from './composables/useAutoUpdate.js';
import { bootAssetWindow } from './lib/assetWindowBoot.js';
import { setupHandoffListeners } from './lib/sessionHandoff.js';
import { useSessionsStore } from './stores/sessions.js';
import WorkbenchShell from './components/workbench/WorkbenchShell.vue';
import AssetWindowShell from './components/workbench/AssetWindowShell.vue';
import GlobalModals from './components/shell/GlobalModals.vue';
import TransferDrawer from './components/files/TransferDrawer.vue';
import AppToastHost from './components/ui/AppToastHost.vue';

// 独立资产窗口（WebviewWindow url 带 ?win=asset&assetId=...）：模块加载即定
const params = new URLSearchParams(window.location.search);
const isAssetWindow = params.get('win') === 'asset' && Boolean(params.get('assetId'));

const store = useWorkbenchStore();
// 单实例（关键）：panelResize 必须全局唯一——两实例会互相覆盖 documentElement
// CSS 变量与 resize 监听。资产窗口用独立 storageKey，布局不与主窗口互相泄漏。
const panelResize = usePanelResize(isAssetWindow ? { storageKey: 'myshelltool:layout-asset:v1' } : undefined);
const autoUpdate = useAutoUpdate({ announce: (msg, opts) => store.announce(msg, opts) });
const desktopRuntimeAvailable = computed(() => isTauriRuntime());

// 跨窗口会话迁移监听（sessionHandoff 协议）：主/asset 两分支都要注册
// （角色门控在 setupHandoffListeners 内部：TEAROFF 仅 asset 窗口、MERGE_PUSH 仅主窗口）
let handoffDispose = null;

onMounted(() => {
  if (isAssetWindow) {
    // 资产窗口：asset 模式初始化（跳过 MCP/同步）→ 引导（adopt 迁移会话或按 assetId 连接）；
    // 不启动自动更新、不注册 Ctrl+K 全局搜索（主窗口职责）
    store
      .initialize({ mode: 'asset' })
      .then(() => bootAssetWindow(store, {
        assetId: params.get('assetId'),
        adoptSessionId: params.get('adopt')
      }))
      .catch(() => null);
  } else {
    store.initialize();
    autoUpdate.init();
    window.addEventListener('keydown', handleGlobalKeydown);
  }
  handoffDispose = setupHandoffListeners({
    sessionsStore: useSessionsStore(),
    workbenchStore: store,
    assetId: params.get('assetId')
  });
});

onBeforeUnmount(() => {
  if (!isAssetWindow) window.removeEventListener('keydown', handleGlobalKeydown);
  handoffDispose?.();
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
  // resetLayout：恢复默认布局回调，供设置弹窗「外观」tab 的次要按钮调用
  // （原顶栏布局菜单入口删除后的补偿入口）
  store.modal = {
    type: 'settings',
    tab,
    autoUpdate,
    resetLayout: () => {
      panelResize.resetLayout();
      store.announce('布局已恢复默认');
    }
  };
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

function openMcpPanel() {
  store.refreshMcpStatus();
  openSettings('mcp');
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

    <AssetWindowShell
      v-if="isAssetWindow"
      :store="store"
      :desktop-runtime-available="desktopRuntimeAvailable"
      :panel-resize="panelResize"
    />

    <WorkbenchShell
      v-else
      :store="store"
      :desktop-runtime-available="desktopRuntimeAvailable"
      :panel-resize="panelResize"
      :auto-update="autoUpdate"
      @create-asset="createAsset"
      @create-group="createGroup"
      @connect-selected="connectSelected"
      @open-settings="openSettings"
      @open-mcp-panel="openMcpPanel"
      @toggle-theme="store.toggleTheme"
      @toggle-assets="toggleAssets"
      @toggle-right="toggleRight"
      @toggle-transfer-drawer="store.toggleTransferDrawer"
    />

    <TransferDrawer :open="store.transferDrawerOpen" @toggle="store.toggleTransferDrawer" />
    <GlobalModals />
    <AppToastHost />
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
  background: var(--warn-soft);
  color: var(--warn);
  border-bottom: 1px solid var(--warn);
  font-size: 13px;
  text-align: center;
}

.desktop-only-banner code {
  padding: 2px 6px;
  border-radius: 3px;
  background: var(--app-hover);
  font-family: var(--font-mono);
}

.window {
  width: 100vw;
  height: 100vh;
  overflow: hidden;
}
</style>
