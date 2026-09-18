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
 *   - 更新链路复用 useAutoUpdate（已在 App.vue 实例化），这里直接注入后透传给
 *     UpdateSection.vue（应用更新区块）→ ReleaseNotesBlock.vue（更新日志行级渲染），
 *     两级子组件为守 500 行 SFC 硬上限自本文件拆出
 *
 * 由 GlobalModals.vue 的 modal.type === 'settings' 分支渲染。抽成独立组件是为
 * 避免 GlobalModals.vue 超 500 行 SFC 硬上限（AGENTS.md 质量红线）。
 */
import { ref, computed, onMounted, watch } from 'vue';
import type { Component } from 'vue';
import { storeToRefs } from 'pinia';
import { Info, LayoutGrid, Palette, RefreshCw, Plug, Sun, Moon, Monitor, ExternalLink, TerminalSquare, Mouse } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';
import { THEME_ORDER, THEME_LABELS } from '@/composables/useTheme';
import type { useAutoUpdate } from '@/composables/useAutoUpdate';
import { errorMessage } from '@/lib/errorMessage';
import AppButton from '@/components/ui/AppButton.vue';
import AppSelect from '@/components/ui/AppSelect.vue';
import AppBrandLogo from '@/components/ui/AppBrandLogo.vue';
import McpPanelContent from '@/components/shell/McpPanelContent.vue';
import SyncPanelContent from '@/components/shell/SyncPanelContent.vue';
import UpdateSection from '@/components/shell/UpdateSection.vue';
import ChangelogSection from '@/components/shell/ChangelogSection.vue';
import { openExternal } from '@/lib/openExternal';
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
const { theme, modal, middleClickAutoscroll } = storeToRefs(store);

const modalExtras = computed<SettingsModalExtras>(() => (modal.value ?? {}) as unknown as SettingsModalExtras);

// autoUpdate 实例由 App.vue 通过 modal payload 注入（store.modal = { type:'settings', autoUpdate, tab }）。
// 同一实例，与状态栏点击共享状态；透传给 UpdateSection。未注入时（浏览器预览）更新区整体不渲染。
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

// pane 常驻：首次访问才挂载（v-if），之后仅切换显示（v-show）——
// 避免 Sync/Mcp 面板每次切 tab 重挂载重拉数据导致闪烁，并保留滚动/折叠等局部状态。
const visitedTabs = ref<string[]>([activeTab.value]);
const panelRef = ref<HTMLElement | null>(null);

watch(() => modalExtras.value.tab, next => {
  if (next && validTabs.includes(next)) {
    activeTab.value = next;
  }
});

watch(activeTab, id => {
  if (!visitedTabs.value.includes(id)) visitedTabs.value.push(id);
  // 长 pane（如同步）滚到底部后切到短 pane，浏览器钳位后视口位置不可预期，统一回顶。
  // 滚动容器是本组件的 .settings-content（左导航 + 右内容布局后不再是 modal-body）。
  panelRef.value?.querySelector('.settings-content')?.scrollTo({ top: 0 });
});

// —— 版本号（关于与更新 tab）——
// 接 @tauri-apps/api/app 的 getVersion（运行时真实值）；浏览器预览无 Tauri runtime 时 fallback。
const appVersion = ref('—');
onMounted(async () => {
  try {
    if (!isTauriRuntime()) return; // 浏览器预览：保持 — 不报错
    const { getVersion } = await import('@tauri-apps/api/app'); // 动态加载：浏览器预览缺 Tauri 模块
    appVersion.value = await getVersion();
  } catch (err) {
    console.warn('[settings] 获取版本号失败：', errorMessage(err));
  }
});

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
  <div ref="panelRef" class="settings-panel">
    <div class="settings-body">
      <!-- 左侧竖导航：设置的顶级分类（关于/外观/同步/MCP 是不同主题，竖列表语义 = 分类树） -->
      <nav class="settings-nav" aria-label="设置分类">
        <button
          v-for="t in TABS"
          :key="t.id"
          type="button"
          class="nav-item"
          :class="{ active: activeTab === t.id }"
          :aria-current="activeTab === t.id ? 'page' : undefined"
          @click="activeTab = t.id"
        >
          <component :is="t.icon" :size="14" />
          {{ t.label }}
        </button>
      </nav>

      <!-- 右侧内容区（独立滚动；页面标题与左导航选中项呼应，告诉用户「现在在哪」） -->
      <main class="settings-content">
      <div class="tab-body">
      <!-- ① 关于与更新（默认 tab，用户最常找的更新入口） -->
      <section v-if="visitedTabs.includes('about')" v-show="activeTab === 'about'" class="stack tab-pane">
        <header class="page-title"><Info :size="15" />关于与更新</header>
        <div class="about-hero">
          <AppBrandLogo :size="36" class="about-logo" />
          <div class="about-text">
            <span class="about-name">myshelltool</span>
            <span class="about-ver num">v{{ appVersion }}</span>
          </div>
        </div>
        <p class="muted">Windows 桌面 SSH 运维客户端。多主机连接、终端、文件、隧道、资源监控。</p>

        <!-- 更新区：单一主操作（检查更新），未注入 autoUpdate 时整体隐藏（浏览器预览）。
             状态机文案/进度条/更新日志已拆至 UpdateSection.vue（500 行 SFC 硬上限约束）。 -->
        <UpdateSection v-if="autoUpdate" :auto-update="autoUpdate" :app-version="appVersion" />

        <!-- 更新日志（常驻）：只渲染当前版本一条的更新内容（离线），
             数据为构建时打包的全历史 changelog.json（组件内自取，仅依赖 appVersion）。 -->
        <ChangelogSection :app-version="appVersion" />

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
      <section v-if="visitedTabs.includes('appearance')" v-show="activeTab === 'appearance'" class="stack tab-pane">
        <header class="page-title"><Palette :size="15" />外观</header>
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

        <!-- 中键自动滚动：webview 级输入行为（ui store 权威值，main.ts 抑制器事件时读取） -->
        <section class="block">
          <header class="block-head"><Mouse :size="12" />鼠标</header>
          <label class="autoscroll-toggle">
            <input
              type="checkbox"
              :checked="middleClickAutoscroll"
              @change="store.setMiddleClickAutoscroll(($event.target as HTMLInputElement).checked)"
            />
            中键自动滚动
          </label>
          <p class="muted">按住鼠标中键拖动即可滚动页面（浏览器式自动滚动，作用于所有可滚动区域）。默认关闭；更改即时生效并持久化。</p>
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
      <section v-if="visitedTabs.includes('sync')" v-show="activeTab === 'sync'" class="stack tab-pane">
        <header class="page-title"><RefreshCw :size="15" />同步</header>
        <SyncPanelContent />
      </section>

      <!-- ④ MCP（复用 McpPanelContent，零 props 自包含） -->
      <section v-if="visitedTabs.includes('mcp')" v-show="activeTab === 'mcp'" class="stack tab-pane">
        <header class="page-title"><Plug :size="15" />MCP</header>
        <McpPanelContent />
      </section>
      </div>
      </main>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.settings-panel {
  display: flex;
  flex-direction: column;
  // 高度撑满 modal-body（modal-body 是滚动容器，这里改为内部右侧独立滚动）
  height: 100%;
  min-height: 0;
}

.settings-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

// —— 左侧竖导航：设置的顶级分类 ——
.settings-nav {
  width: 172px;
  flex-shrink: 0;
  padding: var(--space-3) var(--space-2);
  border-right: 1px solid var(--app-border-soft);
  background: var(--app-chrome);
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.nav-item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 7px var(--space-2);
  border: none;
  border-radius: var(--radius-sm);
  background: none;
  font: inherit;
  font-size: var(--text-sm);
  color: var(--app-muted);
  cursor: pointer;
  text-align: left;
  position: relative;
  transition: background var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard);
}
.nav-item :deep(svg) { flex-shrink: 0; opacity: 0.8; }
.nav-item:hover { background: var(--app-hover); color: var(--app-strong); }
.nav-item.active {
  background: var(--app-selected);
  color: var(--accent);
  font-weight: 500;
}
.nav-item.active::before {
  content: '';
  position: absolute;
  left: calc(-1 * var(--space-2));
  top: 6px;
  bottom: 6px;
  width: 3px;
  border-radius: 2px;
  background: var(--accent);
}
.nav-item.active :deep(svg) { opacity: 1; }
.nav-item:focus-visible { outline: none; box-shadow: var(--focus-ring); }

// —— 右侧内容区（独立滚动） ——
.settings-content {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding: var(--space-5) var(--space-6);
}
// 页面标题：当前分类名，与左导航选中项呼应（用户点「同步」→ 右侧立刻确认「你在同步」）
.page-title {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-base);
  font-weight: 600;
  color: var(--app-strong);
}
.page-title :deep(svg) { color: var(--app-muted); flex-shrink: 0; }

.tab-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
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

// —— 关于 hero（降级为一行，不再当页面大标题） ——
.about-hero {
  display: flex;
  align-items: center;
  gap: 12px;
}
.about-logo {
  width: 36px;
  height: 36px;
  flex-shrink: 0;
  filter: drop-shadow(0 2px 8px rgba(44, 95, 229, 0.25));
}
.about-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.about-name {
  font-size: var(--text-sm);
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

.autoscroll-toggle {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--app-text);
  cursor: pointer;
  user-select: none;
}
.setting-label {
  font-size: var(--text-xs);
  color: var(--app-muted);
}

// —— 更新/布局共用的操作行（更新区本体已拆至 UpdateSection.vue）——
.update-row {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.update-hint {
  font-size: 12px;
  line-height: 1.5;
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

// .spin 已随更新按钮迁至 UpdateSection.vue。

// .stack / .muted / .num / .mono-path 由全局 _utilities / _base 提供，此处不重复定义。

// pane 进入动画：v-show 从 display:none 翻回可见时 animation 自动重播，无需 Vue Transition
.tab-pane {
  animation: settings-pane-in var(--motion-base) var(--ease-standard);
}
@keyframes settings-pane-in {
  from { opacity: 0; transform: translateY(4px); }
  to { opacity: 1; transform: translateY(0); }
}
</style>

