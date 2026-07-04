<script setup>
/**
 * SettingsPanelContent — v1.8 统一设置中心。
 *
 * 收敛此前散落的入口（同步 syncPanel / MCP mcpPanel / 主题 titlebar 按钮 / 更新）
 * 进一个 AppTabGroup 分页容器。齿轮按钮（AppTitleBar.vue）与状态栏 MCP 指示灯、
 * 标题栏同步按钮点击后，通过 uiStore.modal.tab 跳转到对应 tab。
 *
 * 信息架构（ui-ux-pro-max §9 nav-state-active / §4 primary-action）：
 *   - 横向 tab + lucide icon + label，当前 tab 视觉高亮
 *   - 「关于与更新」为默认 tab（用户最常找的更新入口），单一主操作 = 检查更新
 *
 * 复用策略（vibe-guard reuse check 通过）：
 *   - McpPanelContent / SyncPanelContent 是零 props/emit 自包含子组件，原样嵌入 tab，不复制不重写
 *   - 主题数据走 uiStore（theme/setTheme）+ useTheme 常量（THEME_ORDER/THEME_LABELS），不重复定义
 *   - 更新链路复用 useAutoUpdate（已在 App.vue 实例化），这里直接注入
 *
 * 由 GlobalModals.vue 的 modal.type === 'settings' 分支渲染。抽成独立组件是为
 * 避免 GlobalModals.vue 超 500 行 SFC 硬上限（AGENTS.md 质量红线）。
 */
import { ref, computed, onMounted } from 'vue';
import { storeToRefs } from 'pinia';
import { Info, Palette, RefreshCw, Plug, Sun, Moon, Monitor, Download } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench.js';
import { THEME_ORDER, THEME_LABELS } from '@/composables/useTheme.js';
import AppTabGroup from '@/components/ui/AppTabGroup.vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppProgress from '@/components/ui/AppProgress.vue';
import McpPanelContent from '@/components/shell/McpPanelContent.vue';
import SyncPanelContent from '@/components/shell/SyncPanelContent.vue';
import { isTauriRuntime } from '@/services/backend.js';

const store = useWorkbenchStore();
const { theme, modal } = storeToRefs(store);

// autoUpdate 实例由 App.vue 通过 modal payload 注入（store.modal = { type:'settings', autoUpdate, tab }）。
// 同一实例，与状态栏点击共享状态。未注入时（浏览器预览）更新区降级隐藏。
const autoUpdate = computed(() => modal.value?.autoUpdate || null);

// —— Tab 导航 ——
// tab 列表固定 4 项；icon 用 lucide 组件，AppTabGroup 透传给 AppTab。
const TABS = [
  { id: 'about', label: '关于与更新', icon: Info },
  { id: 'appearance', label: '外观', icon: Palette },
  { id: 'sync', label: '同步', icon: RefreshCw },
  { id: 'mcp', label: 'MCP', icon: Plug }
];
// 默认 about；外部入口通过 modal.tab 指定（合法 tab id 才采纳，否则回退 about）。
const validTabs = TABS.map(t => t.id);
const activeTab = ref(validTabs.includes(modal.value.tab) ? modal.value.tab : 'about');

// —— 版本号（关于与更新 tab）——
// 接 @tauri-apps/api/app 的 getVersion（运行时真实值）；浏览器预览无 Tauri runtime 时 fallback。
const appVersion = ref('—');
onMounted(async () => {
  try {
    if (!isTauriRuntime()) return; // 浏览器预览：保持 — 不报错
    const { getVersion } = await import('@tauri-apps/api/app'); // 动态加载：浏览器预览缺 Tauri 模块
    appVersion.value = await getVersion();
  } catch (err) {
    console.warn('[settings] 获取版本号失败：', err?.message || err);
  }
});

// —— 更新按钮状态机（关于与更新 tab）——
// 复用注入的 autoUpdate（来自 App.vue，与状态栏点击同一实例）。未注入时隐藏整个更新区。
const hasUpdater = computed(() => !!autoUpdate.value);
const updateState = computed(() => autoUpdate.value?.state?.value || 'idle');
const newVersion = computed(() => autoUpdate.value?.newVersion?.value || '');
// 下载进度：useAutoUpdate 把进度写进 statusMessage（文字流），这里展示一个 indeterminate 进度条占位。
const isBusy = computed(() => updateState.value === 'checking' || updateState.value === 'downloading');

// 主按钮文案随状态机变化（单一主操作，ui-ux-pro-max §4）。
const updateBtnLabel = computed(() => {
  switch (updateState.value) {
    case 'checking': return '检查中…';
    case 'downloading': return '下载中…';
    case 'available': return `下载并安装 v${newVersion.value}`;
    case 'error': return '重试';
    default: return '检查更新';
  }
});

function onUpdateClick() {
  if (!autoUpdate.value) return;
  if (updateState.value === 'available') {
    autoUpdate.value.onClick(); // 已发现新版 → 下载安装
  } else {
    autoUpdate.value.check(); // idle / error / checking → (重新)检查
  }
}

// —— 主题选择（外观 tab）——
// 读 uiStore.theme（原始三态 system/light/dark）+ 调 setTheme（点哪个选哪个）。
// 主题图标：system→Monitor / light→Sun / dark→Moon。
const themeIcons = { system: Monitor, light: Sun, dark: Moon };
function selectTheme(value) {
  store.setTheme(value);
}
</script>

<template>
  <div class="settings-panel">
    <!-- Tab 导航 -->
    <AppTabGroup :tabs="TABS" v-model:active="activeTab" />

    <div class="tab-body">
      <!-- ① 关于与更新（默认 tab，用户最常找的更新入口） -->
      <section v-if="activeTab === 'about'" class="stack">
        <div class="about-hero">
          <div class="about-logo">my</div>
          <div class="about-text">
            <span class="about-name">myshelltool</span>
            <span class="about-ver num">v{{ appVersion }}</span>
          </div>
        </div>
        <p class="muted">Windows 桌面 SSH 运维客户端。多主机连接、终端、文件、隧道、资源监控。</p>

        <!-- 更新区：单一主操作（检查更新），未注入 autoUpdate 时整体隐藏（浏览器预览） -->
        <section v-if="hasUpdater" class="block">
          <header class="block-head"><Download :size="12" />应用更新</header>
          <div class="update-row">
            <AppButton
              variant="primary"
              size="sm"
              :disabled="isBusy"
              @click="onUpdateClick"
            >
              <RefreshCw v-if="isBusy" :size="12" class="spin" />
              {{ updateBtnLabel }}
            </AppButton>
            <span class="muted update-hint">
              <template v-if="updateState === 'available'">发现新版本，点击下载并安装，完成后自动重启</template>
              <template v-else-if="updateState === 'downloading'">正在下载，进度显示在底部状态栏</template>
              <template v-else-if="updateState === 'error'">检查或下载失败，点击重试</template>
              <template v-else>检查 GitHub releases 是否有新版本</template>
            </span>
          </div>
          <AppProgress v-if="updateState === 'downloading'" :value="null" />
        </section>

        <section class="block">
          <header class="block-head"><Info :size="12" />关于</header>
          <dl class="detail-grid">
            <dt>项目主页</dt>
            <dd><code class="mono-path">github.com/RT-2020/myshelltool</code></dd>
            <dt>许可</dt>
            <dd>MIT</dd>
          </dl>
        </section>
      </section>

      <!-- ② 外观（主题三选） -->
      <section v-else-if="activeTab === 'appearance'" class="stack">
        <header class="block-head"><Palette :size="12" />主题</header>
        <div class="theme-grid">
          <button
            v-for="t in THEME_ORDER"
            :key="t"
            type="button"
            class="theme-card"
            :class="{ active: theme === t }"
            @click="selectTheme(t)"
          >
            <component :is="themeIcons[t]" :size="22" />
            <span>{{ THEME_LABELS[t] }}</span>
          </button>
        </div>
        <p class="muted">「跟随系统」随系统明暗自动切换；切换即时生效并持久化。</p>
      </section>

      <!-- ③ 同步（复用 SyncPanelContent + PatConfigCard，零 props 自包含） -->
      <SyncPanelContent v-else-if="activeTab === 'sync'" />

      <!-- ④ MCP（复用 McpPanelContent，零 props 自包含） -->
      <McpPanelContent v-else-if="activeTab === 'mcp'" />
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.settings-panel {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-height: 62vh; // 与 mcpPanel/syncPanel 一致，超出 modal 内部滚动
  overflow-y: auto;
}

.tab-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding-top: 4px;
}

// —— 通用 section header（对齐 McpPanelContent/OpsSummaryPanel 视觉语言）——
.block {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md, 8px);
  background: var(--bg-elevated, var(--app-surface));
}
.block-head {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--text-secondary, var(--app-muted));
}
.detail-grid {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px 16px;
  margin: 0;
  dt {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-secondary, var(--app-muted));
  }
  dd {
    margin: 0;
    font-size: 13px;
  }
}

// —— 关于 hero ——
.about-hero {
  display: flex;
  align-items: center;
  gap: 12px;
}
.about-logo {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 48px;
  height: 48px;
  border-radius: var(--radius-md, 8px);
  background: var(--app-accent, var(--app-primary));
  color: #fff;
  font-weight: 700;
  font-size: 18px;
  letter-spacing: -0.02em;
}
.about-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.about-name {
  font-size: 18px;
  font-weight: 600;
}
.about-ver {
  font-size: 12px;
  color: var(--text-secondary, var(--app-muted));
}

// —— 更新区 ——
.update-row {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.update-hint {
  font-size: 12px;
}

// —— 主题选择卡片 ——
.theme-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 10px;
}
.theme-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 16px 8px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md, 8px);
  background: var(--bg-elevated, var(--app-surface));
  color: var(--text-secondary, var(--app-muted));
  cursor: pointer;
  font: inherit;
  font-size: 13px;
  transition: border-color 0.15s ease, color 0.15s ease;

  &:hover {
    border-color: var(--app-accent, var(--app-primary));
  }
  &.active {
    border-color: var(--app-accent, var(--app-primary));
    color: var(--text-primary, var(--app-fg));
    box-shadow: 0 0 0 1px var(--app-accent, var(--app-primary)) inset;
  }
}

// —— 工具类 ——
.spin {
  animation: spin 1s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}

// .stack / .muted / .num / .mono-path 由全局 _utilities / _base 提供，此处不重复定义。
</style>
