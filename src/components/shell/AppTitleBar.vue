<script setup>
/**
 * AppTitleBar — top chrome row of the shell.
 *
 * Holds brand lockup + global quick-search (Ctrl+K) + title actions
 * (status chip, theme toggle, sync/security, warnings). Replaces the
 * existing App.vue titlebar block (lines 864-908) — kept symmetrical
 * in slot usage so App.vue (Wave 3.5) can swap without reflowing.
 */
import { computed, ref, watch, nextTick } from 'vue';
import {
  Sun, Moon, RefreshCw, AlertTriangle,
  PanelRight, PanelRightOpen, RotateCcw, Settings
} from 'lucide-vue-next';
import AppButton from '../ui/AppButton.vue';

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

// 搜索框 DOM ref —— Ctrl+K 打开搜索时聚焦。原先 App.vue 的 globalSearchInput ref
// 从未绑定到任何元素（输入框在此组件内），导致 Ctrl+K 聚焦静默失败。改由本组件在
// searchState.open 变 true 时自行聚焦。
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
</script>

<template>
  <div class="titlebar">
    <!-- Left: brand lockup -->
    <div class="brand-lockup">
      <div class="app-mark" aria-hidden="true">mst</div>
      <div class="brand-title">
        <strong>myshelltool</strong>
        <span class="muted">Windows SSH 客户端</span>
      </div>
    </div>

    <!-- Center: global quick-search -->
    <div class="quick-search">
      <div class="quick-search-wrap">
        <input
          ref="searchFieldEl"
          class="quick-search-field"
          type="search"
          :value="searchQuery"
          placeholder="搜索主机、标签、命令；输入 ssh user@host 快速连接"
          aria-label="全局搜索和快速连接"
          @input="onSearchInput"
          @keydown="onSearchKeydown"
        />
        <kbd class="quick-search-kbd">Ctrl K</kbd>
      </div>
      <ul v-if="isOpen" class="search-suggestions" role="listbox">
        <li
          v-for="(item, idx) in suggestions"
          :key="idx"
          role="option"
          @mousedown.prevent="activateSuggestion(item)"
        >
          <strong v-if="item.kind === 'asset'">{{ item.asset.name }}</strong>
          <strong v-else>{{ item.label }}</strong>
          <span class="muted">{{
            item.kind === 'asset'
              ? `${item.asset.username}@${item.asset.host}`
              : '回车建立连接'
          }}</span>
        </li>
      </ul>
    </div>

    <!-- Right: title actions -->
    <div class="title-actions">
      <AppButton variant="ghost" size="sm" @click="emit('toggle-theme')" title="点击切换主题">
        <component :is="themeIcon" :size="14" />
        <span>{{ themeLabel }}</span>
      </AppButton>

      <AppButton variant="ghost" size="sm" @click="emit('open-sync')" title="同步 / 安全">
        <RefreshCw :size="14" />
        <span>同步 / 安全</span>
      </AppButton>

      <AppButton
        variant="ghost"
        size="sm"
        :class="{ 'has-warning': warningCount > 0 }"
        @click="emit('toggle-warnings')"
        :title="`${warningCount} warning`"
      >
        <AlertTriangle :size="14" />
        <span>{{ warningCount }} warning</span>
      </AppButton>

      <!-- 分隔：布局控制 -->
      <span class="title-action-divider" aria-hidden="true"></span>

      <AppButton
        variant="ghost"
        size="sm"
        @click="emit('toggle-right')"
        :title="rightCollapsed ? '展开右侧面板' : '收起右侧面板'"
        :aria-pressed="String(rightCollapsed)"
      >
        <component :is="rightCollapsed ? PanelRightOpen : PanelRight" :size="14" />
      </AppButton>

      <AppButton
        variant="ghost"
        size="sm"
        @click="emit('reset-layout')"
        title="恢复默认布局"
      >
        <RotateCcw :size="14" />
      </AppButton>

      <!-- 设置入口（预留，未来扩展实际设置页） -->
      <AppButton
        variant="ghost"
        size="sm"
        @click="emit('open-settings')"
        title="设置"
      >
        <Settings :size="14" />
      </AppButton>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.titlebar {
  display: grid;
  grid-template-columns: auto minmax(220px, 1fr) auto;
  align-items: center;
  gap: var(--space-4);
  padding: 0 var(--space-4);
  height: 52px;
  width: 100%;
}

// Brand lockup ------------------------------------------------------------
.brand-lockup {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  color: var(--app-strong);
}

.app-mark {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border-radius: var(--radius-sm);
  background: var(--accent);
  color: var(--accent-on);
  font-family: var(--font-display);
  font-weight: 600;
  font-size: var(--text-xs);
  letter-spacing: 0.04em;
}

.brand-title {
  display: flex;
  flex-direction: column;
  line-height: 1.2;
}

.brand-title strong {
  font-size: var(--text-sm);
  color: var(--app-strong);
}

.brand-title .muted {
  font-size: var(--text-xs);
  color: var(--app-muted);
}

// Quick search ------------------------------------------------------------
.quick-search {
  position: relative;
  display: flex;
  align-items: center;
  min-width: 0;
}

.quick-search-wrap {
  position: relative;
  display: flex;
  align-items: center;
  width: 100%;
}

.quick-search-field {
  width: 100%;
  padding: 6px 10px;
  padding-inline-end: 64px; // room for kbd badge
  background: var(--app-control);
  color: var(--app-text);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  font-family: var(--font-body);
  outline: none;
  transition: border-color var(--motion-fast) var(--ease-standard),
    box-shadow var(--motion-fast) var(--ease-standard);
}

.quick-search-field::placeholder {
  color: var(--app-subtle);
}

.quick-search-field:focus {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

.quick-search-kbd {
  position: absolute;
  inset-inline-end: 8px;
  top: 50%;
  transform: translateY(-50%);
  padding: 1px 6px;
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--app-muted);
  background: color-mix(in oklab, var(--app-hover), transparent 50%);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  pointer-events: none;
}

.search-suggestions {
  position: absolute;
  top: calc(100% + 4px);
  inset-inline-start: 0;
  inset-inline-end: 0;
  margin: 0;
  padding: var(--space-1) 0;
  list-style: none;
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  box-shadow: var(--app-shadow);
  z-index: var(--z-dropdown);
  max-height: 320px;
  overflow-y: auto;
}

.search-suggestions li {
  display: flex;
  flex-direction: column;
  padding: 6px var(--space-3);
  cursor: pointer;
}

.search-suggestions li:hover {
  background: var(--app-hover);
}

.search-suggestions li .muted {
  font-size: var(--text-xs);
  color: var(--app-muted);
}

// Title actions -----------------------------------------------------------
.title-actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.title-action-divider {
  width: 1px;
  height: 16px;
  background: var(--app-border);
  flex-shrink: 0;
}

.has-warning {
  color: var(--warn);
}
</style>
