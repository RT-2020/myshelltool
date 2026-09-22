<script setup lang="ts">
/**
 * AppearanceTab — 设置·外观 tab 内容（v0.20 从 SettingsPanelContent 拆出，
 * S2 第一刀：主题三选 / 终端排版 / 鼠标中键滚动三块整体迁移，逻辑零变更）。
 * store-bound（workbench re-export 的 ui/sessions 状态与动作），零 props。
 */
import { computed, type Component } from 'vue';
import { storeToRefs } from 'pinia';
import { Monitor, Moon, Palette, Sun, TerminalSquare, Mouse } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';
import AppSelect from '@/components/ui/AppSelect.vue';

const store = useWorkbenchStore();
const { theme, middleClickAutoscroll } = storeToRefs(store);

// —— 主题选择：读 uiStore.theme（原始三态 system/light/dark）+ 调 setTheme ——
const THEME_ORDER = ['system', 'light', 'dark'] as const;
const THEME_LABELS: Record<string, string> = { system: '跟随系统', light: '浅色', dark: '深色' };
const themeIcons: Record<string, Component> = { system: Monitor, light: Sun, dark: Moon };
const themeDescription = computed(() => {
  if (theme.value === 'light') return '「浅色」始终保持明亮清爽的界面风格，切换即时生效并持久化。';
  if (theme.value === 'dark') return '「深色」适合弱光环境与沉浸式终端运维操作，切换即时生效并持久化。';
  return '「跟随系统」随系统外观明暗自动无缝切换，切换即时生效并持久化。';
});
function selectTheme(value: string) {
  store.setTheme(value);
}

// —— 终端排版：字号/行距（sessions store 权威值，改变即热更新所有终端）——
const fontSizeOptions = [10, 11, 12, 13, 14, 15, 16, 18, 20].map(px => ({ label: `${px}px`, value: px }));
const lineHeightOptions = [1.0, 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.8, 2.0].map(lh => ({ label: lh.toFixed(1), value: lh }));
// store 的 setTerminalFontSize 是增量式（delta）；面板选绝对值 → 换算差值复用同一管线。
function setTerminalFontSizeTo(value: string | number) {
  const delta = Number(value) - store.terminalFontSize;
  if (delta) store.setTerminalFontSize(delta);
}
</script>

<template>
  <div class="stack">
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
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

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
  transition: border-color var(--dur-fast) var(--ease-standard),
    color var(--dur-fast) var(--ease-standard),
    background var(--dur-fast) var(--ease-standard),
    box-shadow var(--dur-fast) var(--ease-standard);

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
.theme-hint { margin: 0; }

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

.autoscroll-toggle {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--app-text);
  cursor: pointer;
  user-select: none;
}

// —— 通用 section（对齐设置面板视觉语言）——
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
</style>
