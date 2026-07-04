import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import {
  normalizeAsset,
  slugify
} from '../services/backend.js';
import {
  applyTheme,
  readSystemPrefersDark,
  startSystemThemeListener,
  THEME_LABELS,
  THEME_ORDER
} from '../composables/useTheme.js';

// localStorage key（CRITICAL: do NOT rename — Critic 改进 3）
const THEME_STORAGE_KEY = 'myshelltool-theme';
const ASSETS_COLLAPSED_KEY = 'myshelltool-assets';
const RIGHT_COLLAPSED_KEY = 'myshelltool-right';

function readStored(key) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function normalizeStoredTheme(value) {
  return THEME_ORDER.includes(value) ? value : 'system';
}

/**
 * 把 assets 收起状态写入 dataset 并可选持久化到 localStorage
 */
function applyAssetsState(collapsed, persist) {
  if (typeof document !== 'undefined' && document.documentElement) {
    document.documentElement.dataset.assets = collapsed ? 'collapsed' : 'expanded';
  }
  if (persist) {
    try {
      localStorage.setItem(ASSETS_COLLAPSED_KEY, collapsed ? 'collapsed' : 'expanded');
    } catch {
      /* localStorage 不可用时静默忽略 */
    }
  }
}

/**
 * 把右侧面板（资源监控+运维摘要）收起状态写入 dataset 并持久化。
 * 镜像 applyAssetsState：AppShellLayout 用 :global(:root[data-right='collapsed'])
 * 把 --right-w 覆盖为 0，整列折叠。
 */
function applyRightState(collapsed, persist) {
  if (typeof document !== 'undefined' && document.documentElement) {
    document.documentElement.dataset.right = collapsed ? 'collapsed' : 'expanded';
  }
  if (persist) {
    try {
      localStorage.setItem(RIGHT_COLLAPSED_KEY, collapsed ? 'collapsed' : 'expanded');
    } catch {
      /* localStorage 不可用时静默忽略 */
    }
  }
}

function tabLabel(tab) {
  return {
    overview: '工作区总览',
    terminal: '终端',
    files: '文件',
    tunnels: '隧道管理',
    editor: '编辑器'
  }[tab] || tab;
}

/**
 * useUiStore — Wave 2 Step 2.3
 *
 * 从 workbench.js 抽取 UI / 全局 / 主题 / 搜索 / backend 状态：
 *   - theme / effectiveTheme / themeLabel / systemPrefersDark（三态主题）
 *   - activeTab（4 tab 切换，setTab 跨 store 编排）
 *   - assetsCollapsed（左栏折叠 + localStorage 持久化）
 *   - statusMessage（底部状态栏文本）
 *   - modal（资产/隧道/hostKey 编辑器中心化）
 *   - searchState（全局搜索 + ssh:// 快速连接）
 *   - backendStatus（backend_status 命令结果）
 *
 * 跨 store 桥接（lazy getter 注入）：
 *   - sessionsStore（setTab 切到 files/tunnels 时不需要 sessions，但搜索激活时需要）
 *   - filesStore（setTab 切到 files 时刷新 remote + sync manual path）
 *   - tunnelsStore（setTab 切到 tunnels 时刷新隧道列表）
 *   - assetsStore（搜索 suggestion 走 assets + selectAsset 转发）
 *
 * localStorage keys（CRITICAL Critic 改进 3，禁重命名）：
 *   - 'myshelltool-theme' 主题持久化
 *   - 'myshelltool-assets' 左栏折叠状态持久化
 */
export const useUiStore = defineStore('ui', () => {
  // ============================================================
  // State
  // ============================================================
  const backendStatus = ref({ ready: false, mode: 'loading' });
  const activeTab = ref('overview');
  const theme = ref(normalizeStoredTheme(readStored(THEME_STORAGE_KEY)));
  const systemPrefersDark = ref(readSystemPrefersDark());
  const assetsCollapsed = ref(readStored(ASSETS_COLLAPSED_KEY) === 'collapsed');
  const rightCollapsed = ref(readStored(RIGHT_COLLAPSED_KEY) === 'collapsed');
  const statusMessage = ref('就绪：连接资产可收起，双击主机打开 SSH 会话。');
  const modal = ref({ type: null, asset: null });
  const searchState = ref({ open: false, query: '', suggestions: [] });

  // module-level closure — system theme listener unlisten（必须 store factory 内初始化）
  let systemThemeUnlisten = null;

  // ============================================================
  // Computed
  // ============================================================
  const effectiveTheme = computed(() => {
    if (theme.value === 'system') return systemPrefersDark.value ? 'dark' : 'light';
    return theme.value;
  });
  const themeLabel = computed(() => THEME_LABELS[theme.value] || theme.value);
  const backendMode = computed(() => backendStatus.value?.mode || 'unknown');
  const backendStatusText = computed(() =>
    backendStatus.value.ready
      ? `已连接 · ${backendMode.value}`
      : `未就绪 · ${backendMode.value}`
  );

  // ============================================================
  // 跨 store 桥接（lazy）
  // ============================================================
  let workbenchBridge = null;
  function attachWorkbench(store) {
    workbenchBridge = store;
  }
  function wb() {
    if (!workbenchBridge) {
      throw new Error('ui store: workbench bridge not attached. Call uiStore.attachWorkbench(workbenchStore) at App.vue init.');
    }
    return workbenchBridge;
  }
  function announce(message) {
    if (workbenchBridge && typeof workbenchBridge.announce === 'function') {
      return workbenchBridge.announce(message);
    }
    statusMessage.value = message;
  }

  // ============================================================
  // Actions — 主题
  // ============================================================
  function applyThemeValue(value) {
    applyTheme(value);
  }

  function toggleTheme() {
    const current = THEME_ORDER.indexOf(theme.value);
    theme.value = THEME_ORDER[(current + 1) % THEME_ORDER.length];
    applyTheme(effectiveTheme.value);
    try {
      localStorage.setItem(THEME_STORAGE_KEY, theme.value);
    } catch {
      /* localStorage 不可用，静默 */
    }
    announce('主题已切换：' + (THEME_LABELS[theme.value] || theme.value));
  }

  // 设置面板「外观」tab 用：点哪个选哪个（区别于 toggleTheme 的循环切换）。
  // value 必须是 THEME_ORDER 之一，非法值由 normalizeStoredTheme 兜底。
  function setTheme(value) {
    const next = normalizeStoredTheme(value);
    if (next === theme.value) return; // 无变化不重复 announce
    theme.value = next;
    applyTheme(effectiveTheme.value);
    try {
      localStorage.setItem(THEME_STORAGE_KEY, theme.value);
    } catch {
      /* localStorage 不可用，静默 */
    }
    announce('主题已切换：' + (THEME_LABELS[theme.value] || theme.value));
  }

  // ============================================================
  // Actions — assets 收起
  // ============================================================
  function toggleAssets() {
    assetsCollapsed.value = !assetsCollapsed.value;
    applyAssetsState(assetsCollapsed.value, true);
    announce(assetsCollapsed.value ? '连接资产已收起，主工作区已扩展' : '连接资产已展开');
  }

  // ============================================================
  // Actions — 右侧面板收起（资源监控+运维摘要整列）
  // ============================================================
  function toggleRight() {
    rightCollapsed.value = !rightCollapsed.value;
    applyRightState(rightCollapsed.value, true);
    announce(rightCollapsed.value ? '右侧面板已收起' : '右侧面板已展开');
  }

  // ============================================================
  // Actions — setTab（跨 store 编排）
  // ============================================================
  function setTab(tab) {
    activeTab.value = tab;
    if (workbenchBridge) {
      if (tab === 'files' && typeof workbenchBridge.filesStore === 'function') {
        const filesStore = workbenchBridge.filesStore();
        if (filesStore && typeof filesStore.refreshRemoteFiles === 'function') {
          filesStore.refreshRemoteFiles().catch(error => announce('远程文件刷新失败：' + error.message));
          filesStore.manualRemotePathInput = filesStore.remotePath;
          filesStore.manualLocalPathInput = filesStore.localPath;
          if (!filesStore.localPath) filesStore.refreshLocalFiles().catch(() => null);
        }
      }
      if (tab === 'tunnels' && typeof workbenchBridge.tunnelsStore === 'function') {
        const tunnelsStore = workbenchBridge.tunnelsStore();
        if (tunnelsStore && typeof tunnelsStore.refreshTunnels === 'function') {
          tunnelsStore.refreshTunnels().catch(() => null);
        }
      }
    }
    announce('已切换到 ' + tabLabel(tab));
  }

  // ============================================================
  // Actions — 全局搜索 / 快速连接
  // ============================================================
  function openGlobalSearch() {
    searchState.value = { ...searchState.value, open: true };
  }

  function closeGlobalSearch() {
    searchState.value = { ...searchState.value, open: false };
  }

  function setGlobalSearchQuery(query) {
    searchState.value.query = query;
    const trimmed = query.trim();
    // 有输入即展开建议下拉；清空则关闭。修复：原先只更新 query/suggestions 而未置
    // open=true，导致 AppTitleBar 的 isOpen(= open && suggestions.length) 恒为 false，
    // 下拉永远不显示——这是「全局搜索未接入实际功能」的根因。
    searchState.value.open = trimmed.length > 0;
    if (!trimmed) {
      searchState.value.suggestions = [];
      return;
    }
    const sshMatch = trimmed.match(/^ssh\s+([^\s@]+)@([^\s:]+)(?::(\d+))?$/);
    if (sshMatch) {
      searchState.value.suggestions = [{
        kind: 'quick-connect',
        username: sshMatch[1],
        host: sshMatch[2],
        port: Number(sshMatch[3] || 22),
        label: `快速连接 ${sshMatch[1]}@${sshMatch[2]}`
      }];
      return;
    }
    const lower = trimmed.toLowerCase();
    const assets = (workbenchBridge && typeof workbenchBridge.assets === 'function')
      ? workbenchBridge.assets()
      : [];
    const list = Array.isArray(assets) ? assets : [];
    searchState.value.suggestions = list
      .filter(asset => [asset.name, asset.host, asset.username, asset.group, ...(asset.tags || [])].join(' ').toLowerCase().includes(lower))
      .slice(0, 6)
      .map(asset => ({ kind: 'asset', asset }));
  }

  async function activateSuggestion(suggestion) {
    if (!suggestion) return;
    if (suggestion.kind === 'asset') {
      if (workbenchBridge && typeof workbenchBridge.selectAsset === 'function') {
        workbenchBridge.selectAsset(suggestion.asset.id);
      }
      closeGlobalSearch();
      return;
    }
    if (suggestion.kind === 'quick-connect') {
      closeGlobalSearch();
      const tempId = slugify(suggestion.host);
      const item = normalizeAsset({
        id: tempId,
        name: `${suggestion.username}@${suggestion.host}`,
        host: suggestion.host,
        port: suggestion.port,
        username: suggestion.username,
        auth_method: 'Password',
        group: '快速连接',
        tags: ['quick']
      });
      if (workbenchBridge) {
        if (typeof workbenchBridge.saveAsset === 'function') {
          await workbenchBridge.saveAsset(item);
        }
        if (typeof workbenchBridge.sessionsStore === 'function') {
          const sessionsStore = workbenchBridge.sessionsStore();
          if (sessionsStore && typeof sessionsStore.connectSelected === 'function') {
            await sessionsStore.connectSelected();
          }
        }
      }
    }
  }

  // ============================================================
  // Initialize
  // ============================================================
  function initializeTheme() {
    applyTheme(effectiveTheme.value);
    if (typeof systemThemeUnlisten !== 'function') {
      systemThemeUnlisten = startSystemThemeListener(matches => { systemPrefersDark.value = matches; });
    }
    applyAssetsState(assetsCollapsed.value, false);
    applyRightState(rightCollapsed.value, false);
  }

  function disposeSystemThemeListener() {
    if (typeof systemThemeUnlisten === 'function') {
      systemThemeUnlisten();
      systemThemeUnlisten = null;
    }
  }

  return {
    // state
    backendStatus,
    activeTab,
    theme,
    systemPrefersDark,
    assetsCollapsed,
    rightCollapsed,
    statusMessage,
    modal,
    searchState,
    // computed
    effectiveTheme,
    themeLabel,
    backendMode,
    backendStatusText,
    // bridge
    attachWorkbench,
    // theme actions
    toggleTheme,
    setTheme,
    applyThemeValue,
    initializeTheme,
    disposeSystemThemeListener,
    // 面板折叠 actions
    toggleAssets,
    toggleRight,
    setTab,
    // search actions
    openGlobalSearch,
    closeGlobalSearch,
    setGlobalSearchQuery,
    activateSuggestion
  };
});
