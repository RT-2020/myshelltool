<script setup lang="ts">
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
import { ref, computed, onMounted, unref, watch } from 'vue';
import type { Component } from 'vue';
import { storeToRefs } from 'pinia';
import { Info, LayoutGrid, Palette, RefreshCw, Plug, Sun, Moon, Monitor, Download, ExternalLink, TerminalSquare } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';
import { THEME_ORDER, THEME_LABELS } from '@/composables/useTheme';
import type { useAutoUpdate } from '@/composables/useAutoUpdate';
import AppTabGroup from '@/components/ui/AppTabGroup.vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppProgress from '@/components/ui/AppProgress.vue';
import AppSelect from '@/components/ui/AppSelect.vue';
import AppBrandLogo from '@/components/ui/AppBrandLogo.vue';
import McpPanelContent from '@/components/shell/McpPanelContent.vue';
import SyncPanelContent from '@/components/shell/SyncPanelContent.vue';
import { isTauriRuntime } from '@/services/backend';

/**
 * settings modal 的运行时附加字段（App.vue 注入，ModalState 之外的动态扩展，
 * 故经 cast 窄化读取——动态边界）。
 */
interface SettingsModalExtras {
  autoUpdate?: ReturnType<typeof useAutoUpdate> | null;
  resetLayout?: (() => void) | null;
  tab?: string;
}

const store = useWorkbenchStore();
const { theme, modal } = storeToRefs(store);

const modalExtras = computed<SettingsModalExtras>(() => (modal.value ?? {}) as unknown as SettingsModalExtras);

// autoUpdate 实例由 App.vue 通过 modal payload 注入（store.modal = { type:'settings', autoUpdate, tab }）。
// 同一实例，与状态栏点击共享状态。未注入时（浏览器预览）更新区降级隐藏。
const autoUpdate = computed(() => modalExtras.value.autoUpdate || null);

// resetLayout 回调由 App.vue 通过 modal payload 注入（原顶栏布局菜单删除后的
// 补偿入口）。无回调时不渲染「恢复默认布局」按钮。
const resetLayout = computed(() => modalExtras.value.resetLayout || null);

// —— 终端排版（外观 tab）——
const fontSizeOptions = [10, 11, 12, 13, 14, 15, 16, 18, 20]
  .map(px => ({ label: `${px}px`, value: px }));
const lineHeightOptions = [1.0, 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.8, 2.0]
  .map(lh => ({ label: lh.toFixed(1), value: lh }));
// store 的 setTerminalFontSize 是增量式（delta）；面板选绝对值 → 换算差值复用同一管线。
function setTerminalFontSizeTo(value: string | number) {
  const delta = Number(value) - store.terminalFontSize;
  if (delta) store.setTerminalFontSize(delta);
}

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
const initialTab = modalExtras.value.tab ?? '';
const activeTab = ref(validTabs.includes(initialTab) ? initialTab : 'about');

watch(() => modalExtras.value.tab, next => {
  if (next && validTabs.includes(next)) {
    activeTab.value = next;
  }
});

async function openExternal(url: string) {
  if (!isTauriRuntime()) {
    window.open(url, '_blank', 'noopener');
    return;
  }
  try {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
  } catch (e) {
    console.warn('[settings] opener failed, fallback to window.open:', e);
    window.open(url, '_blank', 'noopener');
  }
}

// —— 版本号（关于与更新 tab）——
// 接 @tauri-apps/api/app 的 getVersion（运行时真实值）；浏览器预览无 Tauri runtime 时 fallback。
const appVersion = ref('—');
onMounted(async () => {
  try {
    if (!isTauriRuntime()) return; // 浏览器预览：保持 — 不报错
    const { getVersion } = await import('@tauri-apps/api/app'); // 动态加载：浏览器预览缺 Tauri 模块
    appVersion.value = await getVersion();
  } catch (err) {
    console.warn('[settings] 获取版本号失败：', (err as Error | undefined)?.message || err);
  }
});

// —— 更新按钮状态机（关于与更新 tab）——
// 复用注入的 autoUpdate（来自 App.vue，与状态栏点击同一实例）。未注入时隐藏整个更新区。
const hasUpdater = computed(() => !!autoUpdate.value);
const updateState = computed(() => unref(autoUpdate.value?.state) || 'idle');
const newVersion = computed(() => unref(autoUpdate.value?.newVersion) || '');
const errorMessage = computed(() => unref(autoUpdate.value?.errorMessage) || '');
const downloadProgress = computed(() => unref(autoUpdate.value?.downloadProgress) ?? 0);
// 下载进度：useAutoUpdate 把进度写进 statusMessage（文字流），这里展示进度条。
const isBusy = computed(() => updateState.value === 'checking' || updateState.value === 'downloading');

// 主按钮文案随状态机变化（单一主操作，ui-ux-pro-max §4）。
const updateBtnLabel = computed(() => {
  switch (updateState.value) {
    case 'checking': return '检查中…';
    case 'downloading': return downloadProgress.value > 0 ? `下载中 ${downloadProgress.value}%` : '下载中…';
    case 'available': return `下载并安装 v${newVersion.value}`;
    case 'error': return '重试';
    case 'up_to_date': return '重新检查';
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
const themeIcons: Record<string, Component> = { system: Monitor, light: Sun, dark: Moon };
const themeDescription = computed(() => {
  if (theme.value === 'light') return '「浅色」始终保持明亮清爽的界面风格，切换即时生效并持久化。';
  if (theme.value === 'dark') return '「深色」适合弱光环境与沉浸式终端运维操作，切换即时生效并持久化。';
  return '「跟随系统」随系统外观明暗自动无缝切换，切换即时生效并持久化。';
});
function selectTheme(value: string) {
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
          <AppBrandLogo :size="48" class="about-logo" />
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
            <span
              class="muted update-hint"
              :class="{
                'is-error': updateState === 'error',
                'is-success': updateState === 'available' || updateState === 'up_to_date'
              }"
            >
              <template v-if="updateState === 'available'">发现新版本 v{{ newVersion }}，点击下载并安装，完成后自动重启</template>
              <template v-else-if="updateState === 'downloading'">正在下载更新安装包（{{ downloadProgress }}%），请稍候…</template>
              <template v-else-if="updateState === 'error'">检查或更新失败：{{ errorMessage || '网络连接失败或超时' }}</template>
              <template v-else-if="updateState === 'up_to_date'">当前已是最新版本 (v{{ appVersion }})</template>
              <template v-else>检查 GitHub releases 是否有新版本</template>
            </span>
          </div>
          <AppProgress v-if="updateState === 'downloading'" :value="downloadProgress > 0 ? downloadProgress : null" />
        </section>

        <section class="block">
          <header class="block-head"><Info :size="12" />关于</header>
          <dl class="detail-grid">
            <dt>项目主页</dt>
            <dd>
              <button
                type="button"
                class="link-action"
                title="在系统默认浏览器中打开"
                @click="openExternal('https://github.com/RT-2020/myshelltool')"
              >
                <code class="mono-path">github.com/RT-2020/myshelltool</code>
                <ExternalLink :size="12" />
              </button>
            </dd>
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
        <p class="muted theme-hint">{{ themeDescription }}</p>

        <!-- 终端排版：字号 / 行间距（sessions store 权威值，改变即热更新所有终端） -->
        <section class="block">
          <header class="block-head"><TerminalSquare :size="12" />终端排版</header>
          <div class="terminal-typography-row">
            <label class="setting-field">
              <span class="setting-label">字号</span>
              <AppSelect
                :model-value="store.terminalFontSize"
                :options="fontSizeOptions"
                @update:model-value="v => setTerminalFontSizeTo(v)"
              />
            </label>
            <label class="setting-field">
              <span class="setting-label">行间距</span>
              <AppSelect
                :model-value="store.terminalLineHeight"
                :options="lineHeightOptions"
                @update:model-value="v => store.setTerminalLineHeight(Number(v))"
              />
            </label>
          </div>
          <p class="muted">即时生效并持久化；字号也可在终端内 Ctrl+滚轮 / Ctrl+= / Ctrl+- 调整。</p>
        </section>

        <!-- 恢复默认布局：次要操作（低频不常驻），无 resetLayout 回调时不渲染 -->
        <section v-if="resetLayout" class="block">
          <header class="block-head"><LayoutGrid :size="12" />布局</header>
          <div class="update-row">
            <AppButton variant="subtle" size="sm" @click="resetLayout()">恢复默认布局</AppButton>
            <span class="muted update-hint">重置侧栏与终端/文件的分区宽度</span>
          </div>
        </section>
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
  // 不自带滚动：AppModal body 是唯一滚动容器，避免外层大滚动条嵌内层小滚动条
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
  width: 48px;
  height: 48px;
  flex-shrink: 0;
  filter: drop-shadow(0 4px 12px rgba(44, 95, 229, 0.25));
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

// —— 终端排版区 ——
.terminal-typography-row {
  display: flex;
  align-items: flex-end;
  gap: var(--space-4);
  flex-wrap: wrap;
}
.setting-field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 120px;
}
.setting-label {
  font-size: var(--text-xs);
  color: var(--app-muted);
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
  line-height: 1.5;

  &.is-error {
    color: var(--danger);
  }

  &.is-success {
    color: var(--success);
  }
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
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
  color: var(--app-muted);
  cursor: pointer;
  font: inherit;
  font-size: 13px;
  transition: border-color var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard),
    background var(--motion-fast) var(--ease-standard),
    box-shadow var(--motion-fast) var(--ease-standard);

  &:hover {
    border-color: var(--accent);
    color: var(--app-strong);
    background: var(--app-hover);
  }
  &.active {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
    box-shadow: 0 0 0 1px var(--accent) inset;
  }
}

.link-action {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background: none;
  border: none;
  padding: 0;
  color: var(--accent);
  cursor: pointer;
  font: inherit;

  &:hover {
    color: var(--accent-hover);
    text-decoration: underline;
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

