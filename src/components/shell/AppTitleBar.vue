<script setup>
/**
 * AppTitleBar — 顶栏 chrome（app.html 全量还原）。
 *
 * 结构（app.css L252-295 严格同步）：
 *   .titlebar (grid 1fr auto 1fr)
 *     .tb-left:   .tb-brand(SVG logo + name + sep + project)
 *     .tb-center: .tb-search(⌘K，全局快速搜索 + 建议下拉)
 *     .tb-right:  3 个纯 icon-btn（主题/同步/设置）+ 功能性按钮(警告/折叠右栏/重置布局)
 *
 * 与旧版差异：
 *   - 保留品牌区，不再渲染 macOS 风窗口控制圆点
 *   - .brand-lockup → .tb-brand（SVG logo 替代文字 mst）
 *   - .quick-search → .tb-search（固定 480px 宽）
 *   - .title-actions → .tb-right（AppButton 带文字 → 纯 icon-btn 28x28）
 *
 * emit 事件全部保留（App.vue 已接线，不可断）：
 *   toggle-theme / open-sync / toggle-warnings / toggle-right / reset-layout / open-settings
 *   update:searchQuery / activate-suggestion
 */
import { computed, ref, watch, nextTick } from 'vue';
import {
  Sun, Moon, RefreshCw, AlertTriangle,
  PanelRight, PanelRightOpen, Settings, Menu
} from 'lucide-vue-next';

const props = defineProps({
  themeLabel: { type: String, default: '' },
  warningCount: { type: Number, default: 0 },
  rightCollapsed: { type: Boolean, default: false },
  searchQuery: { type: String, default: '' },
  searchState: {
    type: Object,
    default: () => ({ open: false, suggestions: () => [] })
  }
});

const emit = defineEmits([
  'update:searchQuery',
  'toggle-theme',
  'open-sync',
  'toggle-warnings',
  'toggle-right',
  'reset-layout',
  'open-settings',
  'activate-suggestion'
]);

const themeIcon = computed(() => {
  const label = props.themeLabel || '';
  if (label.includes('浅') || label.toLowerCase().includes('light')) return Sun;
  if (label.includes('深') || label.toLowerCase().includes('dark')) return Moon;
  return RefreshCw;
});

const suggestions = computed(() => props.searchState?.suggestions || []);
const isOpen = computed(() => !!props.searchState?.open && suggestions.value.length);
const layoutMenuOpen = ref(false);

// 搜索框 DOM ref —— Ctrl+K 打开搜索时聚焦
const searchFieldEl = ref(null);
watch(
  () => props.searchState?.open,
  open => {
    if (open) nextTick(() => searchFieldEl.value?.focus());
  }
);

function onSearchInput(e) {
  emit('update:searchQuery', e.target.value);
}

function onSearchKeydown(e) {
  if (e.key === 'Enter' && suggestions.value.length) {
    e.preventDefault();
    emit('activate-suggestion', suggestions.value[0]);
  }
}

function activateSuggestion(item) {
  emit('activate-suggestion', item);
}

function toggleLayoutMenu() {
  layoutMenuOpen.value = !layoutMenuOpen.value;
}

function closeLayoutMenu() {
  layoutMenuOpen.value = false;
}

function resetLayoutFromMenu() {
  closeLayoutMenu();
  emit('reset-layout');
}

function onLayoutMenuFocusout(event) {
  if (!event.currentTarget.contains(event.relatedTarget)) closeLayoutMenu();
}
</script>

<template>
  <div class="titlebar">
    <!-- ============ Left: brand ============ -->
    <div class="tb-left">
      <div class="tb-brand" data-tauri-drag-region>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true">
          <rect x="3" y="4" width="18" height="16" rx="2" />
          <path d="M7 9l3 3-3 3M13 15h4" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
        <span class="tb-name">myshelltool</span>
        <span class="tb-sep" aria-hidden="true">·</span>
        <span class="tb-project">Windows SSH 客户端</span>
      </div>
    </div>

    <!-- ============ Center: global quick-search ============ -->
    <div class="tb-center">
      <div class="tb-search" role="combobox" :aria-expanded="String(isOpen)" aria-label="全局快速搜索">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true">
          <circle cx="11" cy="11" r="7" />
          <path d="M21 21l-4.3-4.3" stroke-linecap="round" />
        </svg>
        <input
          ref="searchFieldEl"
          class="tb-search-input tb-search-text"
          type="search"
          :value="searchQuery"
          placeholder="搜索连接 / 命令 / 文件"
          aria-label="全局搜索和快速连接"
          @input="onSearchInput"
          @keydown="onSearchKeydown"
        />
        <kbd>⌘K</kbd>
        <!-- 建议下拉 -->
        <ul v-if="isOpen" class="tb-suggestions" role="listbox">
          <li
            v-for="(item, idx) in suggestions"
            :key="idx"
            role="option"
            @mousedown.prevent="activateSuggestion(item)"
          >
            <strong v-if="item.kind === 'asset'">{{ item.asset.name }}</strong>
            <strong v-else>{{ item.label }}</strong>
            <span class="tb-sugg-muted">{{
              item.kind === 'asset'
                ? `${item.asset.username}@${item.asset.host}`
                : '回车建立连接'
            }}</span>
          </li>
        </ul>
      </div>
    </div>

    <!-- ============ Right: icon buttons ============ -->
    <div class="tb-right">
      <button class="icon-btn" @click="emit('toggle-theme')" title="切换主题（system / light / dark）" aria-label="切换主题">
        <component :is="themeIcon" :size="14" />
      </button>
      <button class="icon-btn" @click="emit('open-sync')" title="同步 / 安全 · GitHub Gist 资产同步" aria-label="同步设置">
        <RefreshCw :size="14" />
      </button>
      <button
        v-if="warningCount > 0"
        class="icon-btn"
        :class="{ 'has-warning': warningCount > 0 }"
        @click="emit('toggle-warnings')"
        :title="`${warningCount} 个警告`"
        aria-label="查看警告"
      >
        <AlertTriangle :size="14" />
      </button>
      <span class="tb-sep-divider" aria-hidden="true"></span>
      <button
        class="icon-btn"
        @click="emit('toggle-right')"
        :title="rightCollapsed ? '展开右侧面板' : '收起右侧面板'"
        :aria-pressed="String(rightCollapsed)"
        aria-label="切换右侧面板"
      >
        <component :is="rightCollapsed ? PanelRightOpen : PanelRight" :size="14" />
      </button>
      <button class="icon-btn" @click="emit('open-settings')" title="应用设置" aria-label="设置">
        <Settings :size="14" />
      </button>
      <div
        class="tb-menu"
        :data-open="layoutMenuOpen ? 'true' : 'false'"
        @focusout="onLayoutMenuFocusout"
        @keydown.escape.stop="closeLayoutMenu"
      >
        <button
          class="icon-btn"
          type="button"
          aria-label="布局菜单"
          title="布局菜单"
          aria-haspopup="menu"
          :aria-expanded="String(layoutMenuOpen)"
          @click.stop="toggleLayoutMenu"
        >
          <Menu :size="14" />
        </button>
        <div class="menu-popover" role="menu" aria-label="布局菜单">
          <button
            class="menu-item"
            type="button"
            role="menuitem"
            @mousedown.prevent
            @click="resetLayoutFromMenu"
          >
            <span class="menu-item-label">恢复默认布局</span>
            <span class="menu-item-hint">⌘.</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// ============================================================
// TitleBar（app.css L252-266 严格同步）
// ============================================================
.titlebar {
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  align-items: center;
  height: var(--titlebar-h, 52px);
  padding: 0 12px;
  position: relative;
  z-index: 10;
}
.tb-left, .tb-right { display: flex; align-items: center; gap: 8px; }
.tb-right { justify-content: flex-end; }
.tb-center { display: flex; justify-content: center; }

// —— brand（app.css L277-281）——
.tb-brand {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-left: 12px;
  font-size: 13px;
}
.tb-brand svg {
  width: 16px;
  height: 16px;
  color: var(--accent);
  stroke-width: 1.7;
}
.tb-name {
  font-weight: 600;
  color: var(--app-text);
  letter-spacing: -0.01em;
}
.tb-sep { color: var(--app-subtle); }
.tb-project {
  color: var(--app-muted);
  font-weight: 400;
}

// —— search（app.css L283-295）——
.tb-search {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  width: 480px;
  height: 32px;
  padding: 0 12px;
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: 8px;
  cursor: pointer;
  transition: border-color var(--motion-fast), background var(--motion-fast);
}
.tb-search:hover { border-color: var(--app-border-strong); }
.tb-search:focus-within {
  border-color: var(--accent);
  cursor: text;
}
.tb-search > svg {
  width: 14px;
  height: 14px;
  color: var(--app-muted);
  stroke-width: 1.7;
  flex-shrink: 0;
}
.tb-search-input {
  flex: 1;
  min-width: 0;
  border: 0;
  background: transparent;
  font-size: 12.5px;
  color: var(--app-text);
  outline: none;
}
.tb-search-input::placeholder { color: var(--app-muted); }
.tb-search kbd {
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 2px 6px;
  border: 1px solid var(--app-border);
  border-radius: 5px;
  background: var(--app-panel-2);
  color: var(--app-muted);
  letter-spacing: 0.02em;
  flex-shrink: 0;
}

// 建议下拉（保留功能，样式与 app.css dropdown 风格一致）
.tb-suggestions {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  right: 0;
  margin: 0;
  padding: 4px 0;
  list-style: none;
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-pop);
  z-index: var(--z-dropdown);
  max-height: 320px;
  overflow-y: auto;
}
.tb-suggestions li {
  display: flex;
  flex-direction: column;
  padding: 6px 12px;
  cursor: pointer;
}
.tb-suggestions li:hover { background: var(--app-hover); }
.tb-sugg-muted {
  font-size: 11px;
  color: var(--app-muted);
}

// ============================================================
// icon-btn（app.css L298-317 严格同步，统一规格）
// ============================================================
.icon-btn {
  width: 28px;
  height: 28px;
  display: inline-grid;
  place-items: center;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--app-muted);
  cursor: pointer;
  transition: background var(--motion-fast), color var(--motion-fast), border-color var(--motion-fast);
}
.icon-btn svg {
  width: 14px;
  height: 14px;
  stroke-width: 1.6;
}
.icon-btn:hover {
  background: var(--app-hover);
  color: var(--app-text);
}
.icon-btn:active { background: var(--app-active); }
.icon-btn:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}
.icon-btn.has-warning { color: var(--warn); }

// 分隔细条
.tb-sep-divider {
  width: 1px;
  height: 16px;
  background: var(--app-border);
  flex-shrink: 0;
  margin: 0 2px;
}

.tb-menu {
  position: relative;
}

.tb-menu[data-open="true"] .menu-popover {
  opacity: 1;
  transform: translateY(0);
  pointer-events: auto;
}

.menu-popover {
  position: absolute;
  top: calc(100% + 10px);
  right: 0;
  min-width: 210px;
  padding: 6px;
  border: 1px solid var(--app-border);
  border-radius: 10px;
  background: var(--app-panel);
  box-shadow: var(--shadow-lg);
  opacity: 0;
  transform: translateY(-4px);
  pointer-events: none;
  transition: opacity var(--motion-fast), transform var(--motion-fast);
  z-index: var(--z-dropdown);
}

.menu-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  width: 100%;
  min-height: 34px;
  padding: 0 10px;
  border: 0;
  border-radius: 8px;
  background: transparent;
  color: var(--app-text);
  cursor: pointer;
  transition: background var(--motion-fast), color var(--motion-fast);
}

.menu-item:hover {
  background: var(--app-hover);
}

.menu-item-label {
  font-size: 12.5px;
  font-weight: 500;
}

.menu-item-hint {
  flex-shrink: 0;
  font: 11px var(--font-mono);
  color: var(--app-subtle);
}
</style>
