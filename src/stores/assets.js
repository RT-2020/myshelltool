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
  // 显式声明的分组路径（含空分组）。后端 ConnectionAssetStore.groups 同步而来。
  // asset.group 仅存路径字符串，无 asset 的分组刷新后丢失，故用独立列表持久化。
  const declaredGroups = ref([]);

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

  // ------------------------------------------------------------
  // 分组嵌套树：asset.group 用 '/' 分隔层级（如 "生产/数据库/主"）。
  // buildGroupTree 把扁平 asset 列表 + 显式分组声明聚合成递归树。
  //
  // 树根 root = { name:'', path:'', children:[...], items:[] }
  // 节点     = { name, path, parent, children:[], items:[] }
  // 「未分组」是保留顶级节点，path='未分组'，始终置于 children 末尾。
  //
  // 算法：
  //   1. 收集所有路径 = declaredGroups ∪ 所有 asset.group（排除「未分组」）。
  //   2. 按 '/' 拆分递归 ensureNode(path)，维护 path→node 映射，挂到 parent.children。
  //   3. 每个 asset 推入 nodeMap.get(asset.group).items；group==='未分组' 推入未分组节点。
  //   4. 节点顺序：路径按首次出现顺序（声明优先，asset.group 补充）。
  // ------------------------------------------------------------
  function buildGroupTree(assetList, groupList) {
    const root = { name: '', path: '', parent: '', children: [], items: [] };
    const ungrouped = { name: '未分组', path: '未分组', parent: '', children: [], items: [] };
    const nodeMap = new Map(); // path -> node（不含未分组，未分组单独管理）
    const insertionOrder = []; // 记录 path 首次出现顺序（含中间隐式节点）

    // 收集所有路径，按首次出现顺序保留
    const seenPaths = new Set();
    const orderedPaths = [];
    for (const p of groupList || []) {
      if (p && p !== '未分组' && !seenPaths.has(p)) { seenPaths.add(p); orderedPaths.push(p); }
    }
    for (const asset of assetList) {
      const p = asset.group && asset.group !== '未分组' ? asset.group : '';
      if (p && !seenPaths.has(p)) { seenPaths.add(p); orderedPaths.push(p); }
    }

    function ensureNode(path) {
      if (path === '未分组') return ungrouped;
      if (nodeMap.has(path)) return nodeMap.get(path);
      const slashIdx = path.lastIndexOf('/');
      const parentPath = slashIdx === -1 ? '' : path.slice(0, slashIdx);
      const name = slashIdx === -1 ? path : path.slice(slashIdx + 1);
      const parent = parentPath ? ensureNode(parentPath) : root;
      const node = { name, path, parent: parentPath, children: [], items: [] };
      parent.children.push(node);
      nodeMap.set(path, node);
      insertionOrder.push(path);
      return node;
    }

    // 先建所有声明路径的节点骨架（保证空分组可见、层级顺序稳定）
    for (const p of orderedPaths) ensureNode(p);

    // 把 asset 挂到对应节点
    for (const asset of assetList) {
      const g = asset.group || '未分组';
      if (g === '未分组') {
        ungrouped.items.push(asset);
      } else {
        // 若 asset.group 是声明路径中未出现过的（理论不会，上面已收集），兜底建节点
        const node = ensureNode(g);
        node.items.push(asset);
      }
    }

    // 未分组置于根 children 末尾（且仅在有内容时保留）
    if (ungrouped.items.length || ungrouped.children.length) {
      root.children.push(ungrouped);
    }
    return root;
  }

  const groupedAssets = computed(() => buildGroupTree(assets.value, declaredGroups.value));

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
    declaredGroups.value = result.groups || [];
    assetSource.value = { ...result, count: result.count ?? result.assets?.length ?? assets.value.length };
    selectAsset(item.id, false);
    wb().modal = { type: null, asset: null };
    announce('连接资产已保存：' + item.name);
  }

  // ------------------------------------------------------------
  // 删除连接资产 + 容错清理关联凭据（password / passphrase）。
  // ------------------------------------------------------------
  async function deleteAsset(id) {
    const target = assets.value.find(asset => asset.id === id);
    // 清理关联凭据：密码 / passphrase 可能不存在，try/catch 容错
    for (const kind of ['password', 'passphrase']) {
      const credId = credentialIdFor(id, kind);
      try {
        await invokeBackend('delete_credential', { id: credId });
      } catch {
        // 凭据可能从未存储，忽略
      }
    }
    const result = await invokeBackend('delete_connection_asset', { id });
    assets.value = (result.assets || []).map(normalizeAsset);
    declaredGroups.value = result.groups || [];
    assetSource.value = { ...result, count: result.count ?? result.assets?.length ?? assets.value.length };
    // 删除当前选中项后，回退到首项（或空）
    if (selectedAssetId.value === id) {
      selectedAssetId.value = assets.value[0]?.id || null;
    }
    wb().modal = { type: null, asset: null };
    announce('已删除连接资产：' + (target?.name || id));
  }

  // ------------------------------------------------------------
  // 复制连接资产：生成新 id（原名加 -copy 后缀），不带凭据（安全），
  // 重置 last_connected。直接落库，不开编辑器。
  // ------------------------------------------------------------
  async function duplicateAsset(asset) {
    const newId = uniqueAssetId(asset.name + '-copy', asset.host);
    await saveAsset(
      {
        ...asset,
        id: newId,
        name: asset.name + ' (副本)',
        credential_id: null,
        passphrase_credential_id: null,
        last_connected: '从未'
      },
      {} // 无凭据
    );
    announce('已复制连接资产：' + asset.name + ' (副本)');
  }

  // ------------------------------------------------------------
  // 移动单个资产到目标分组（单条，复用 saveAsset，无需新命令）。
  // ------------------------------------------------------------
  async function moveAsset(id, newGroup) {
    const asset = assets.value.find(a => a.id === id);
    if (!asset) return;
    await saveAsset({ ...asset, group: newGroup }, {});
    announce('已移动连接到分组：' + newGroup);
  }

  // ------------------------------------------------------------
  // 分组批量操作（重命名 / 解散 / 新建）—— 走后端批量命令，
  // 一次写盘更新所有匹配 asset，原子且高效。
  // ------------------------------------------------------------
  async function renameGroup(oldPath, newPath) {
    const result = await invokeBackend('rename_asset_group', { oldPath, newPath });
    assets.value = (result.assets || []).map(normalizeAsset);
    declaredGroups.value = result.groups || [];
    assetSource.value = { ...result, count: result.count ?? result.assets?.length ?? assets.value.length };
    wb().modal = { type: null };
    announce('分组已重命名：' + oldPath + ' → ' + newPath);
  }

  async function dissolveGroup(path) {
    const result = await invokeBackend('dissolve_asset_group', { path });
    assets.value = (result.assets || []).map(normalizeAsset);
    declaredGroups.value = result.groups || [];
    assetSource.value = { ...result, count: result.count ?? result.assets?.length ?? assets.value.length };
    announce('分组已解散：' + path);
  }

  async function createGroup(path) {
    const result = await invokeBackend('create_asset_group', { path });
    assets.value = (result.assets || []).map(normalizeAsset);
    declaredGroups.value = result.groups || [];
    assetSource.value = { ...result, count: result.count ?? result.assets?.length ?? assets.value.length };
    wb().modal = { type: null };
    announce('已新建分组：' + path);
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
    declaredGroups,
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
    deleteAsset,
    duplicateAsset,
    moveAsset,
    renameGroup,
    dissolveGroup,
    createGroup,
    saveToken,
    deleteToken
  };
});
