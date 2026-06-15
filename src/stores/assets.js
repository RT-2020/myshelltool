import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import {
  invokeBackend,
  normalizeAsset,
  slugify
} from '../services/backend.js';

/**
 * useAssetsStore — Wave 2 Step 2.2
 *
 * 从 workbench.js 抽取连接资产相关 state / actions / computed。
 *
 * 跨 store 桥接（lazy getter 注入）：
 *   - workbench.announce(message) / statusMessage
 *   - workbench.modal（saveAsset 完成后清空）
 *   - workbench.clearFileSelection()（selectAsset 时清空 files 选中）
 *
 * localStorage keys（CRITICAL Critic 改进 3，禁重命名）：
 *   - 'myshelltool-assets' 由 useUiStore 管理；assets store 不直接读写
 */
export const useAssetsStore = defineStore('assets', () => {
  // ============================================================
  // State（原 workbench.js:34-53）
  // ============================================================
  const assetSource = ref({ source: 'loading', count: 0 });
  const assets = ref([]);
  const selectedAssetId = ref(null);
  const githubPatConfigured = ref(false);

  // ============================================================
  // 跨 store 桥接（lazy）
  // ============================================================
  let workbenchBridge = null;
  function attachWorkbench(store) {
    workbenchBridge = store;
  }
  function wb() {
    if (!workbenchBridge) {
      throw new Error('assets store: workbench bridge not attached. Call assetsStore.attachWorkbench(workbenchStore) at App.vue init.');
    }
    return workbenchBridge;
  }
  function announce(message) {
    if (workbenchBridge && typeof workbenchBridge.announce === 'function') {
      return workbenchBridge.announce(message);
    }
    // eslint-disable-next-line no-console
    console.log('[assets] announce:', message);
  }

  // ============================================================
  // Computed（原 workbench.js:85-100）
  // ============================================================
  const selectedAsset = computed(() => assets.value.find(asset => asset.id === selectedAssetId.value) || assets.value[0] || null);
  const groupedAssets = computed(() => {
    const groups = new Map();
    for (const asset of assets.value) {
      const group = asset.group || '未分组';
      if (!groups.has(group)) groups.set(group, []);
      groups.get(group).push(asset);
    }
    return [...groups.entries()].map(([name, items]) => ({ name, items }));
  });
  const assetSourceText = computed(() => `${assetSource.value.source || assetSource.value.mode || 'unknown'} · ${assetSource.value.count ?? assets.value.length} 项`);
  const syncText = computed(() => githubPatConfigured.value ? 'PAT 已配置' : 'PAT 未配置');

  // ============================================================
  // Actions
  // ============================================================
  function selectAsset(id, announceSelection = true) {
    if (!assets.value.some(asset => asset.id === id)) return;
    selectedAssetId.value = id;
    // 切换 asset 必须清空选择 — 调用 workbench bridge（workbench 转发到 files store）
    if (workbenchBridge && typeof workbenchBridge.clearFileSelection === 'function') {
      workbenchBridge.clearFileSelection();
    }
    if (announceSelection && selectedAsset.value) announce('已选择连接：' + selectedAsset.value.name);
  }

  function credentialIdFor(assetId, kind) {
    return `${assetId}:${kind}`;
  }

  function uniqueAssetId(name, host) {
    const base = slugify(name || host || 'asset');
    let candidate = base;
    let index = 2;
    while (assets.value.some(item => item.id === candidate)) {
      candidate = base + '-' + index;
      index += 1;
    }
    return candidate;
  }

  async function saveAsset(input, credentials = {}) {
    const previous = assets.value.find(asset => asset.id === input.id);
    const id = input.id || uniqueAssetId(input.name, input.host);
    const item = normalizeAsset({
      ...input,
      id,
      last_connected: previous?.last_connected || '从未'
    });

    if (credentials.password) {
      await invokeBackend('save_credential', { id: credentialIdFor(id, 'password'), secret: credentials.password });
      item.credential_id = credentialIdFor(id, 'password');
    } else if (previous?.credential_id) {
      item.credential_id = previous.credential_id;
    }
    if (credentials.passphrase) {
      await invokeBackend('save_credential', { id: credentialIdFor(id, 'passphrase'), secret: credentials.passphrase });
      item.passphrase_credential_id = credentialIdFor(id, 'passphrase');
    } else if (previous?.passphrase_credential_id) {
      item.passphrase_credential_id = previous.passphrase_credential_id;
    }

    const result = await invokeBackend('save_connection_asset', { asset: item });
    assets.value = (result.assets || []).map(normalizeAsset);
    assetSource.value = { ...result, count: result.count ?? result.assets?.length ?? assets.value.length };
    selectAsset(item.id, false);
    wb().modal = { type: null, asset: null };
    announce('连接资产已保存：' + item.name);
  }

  async function saveToken(secret) {
    if (!secret.trim()) {
      announce('token 不能为空');
      return false;
    }
    await invokeBackend('save_credential', { id: 'github-pat', secret });
    const status = await invokeBackend('get_credential_status', { id: 'github-pat' });
    githubPatConfigured.value = Boolean(status.exists);
    announce('同步配置已保存，本地安全存储：' + (githubPatConfigured.value ? '已配置' : '未配置'));
    return true;
  }

  async function deleteToken() {
    await invokeBackend('delete_credential', { id: 'github-pat' });
    githubPatConfigured.value = false;
    announce('已清除本地安全存储中的 token');
  }

  return {
    // state
    assetSource,
    assets,
    selectedAssetId,
    githubPatConfigured,
    // computed
    selectedAsset,
    groupedAssets,
    assetSourceText,
    syncText,
    // bridge
    attachWorkbench,
    // actions
    selectAsset,
    credentialIdFor,
    uniqueAssetId,
    saveAsset,
    saveToken,
    deleteToken
  };
});
