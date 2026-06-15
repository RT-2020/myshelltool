<script setup>
/**
 * AppTitleBar — top chrome row of the shell.
 *
 * Holds brand lockup + global quick-search (Ctrl+K) + title actions
 * (status chip, theme toggle, sync/security, warnings). Replaces the
 * existing App.vue titlebar block (lines 864-908) — kept symmetrical
 * in slot usage so App.vue (Wave 3.5) can swap without reflowing.
 */
import { computed } from 'vue';
import { Sun, Moon, RefreshCw, AlertTriangle } from 'lucide-vue-next';
import AppButton from '../ui/AppButton.vue';

const props = defineProps({
  backendReady: { type: Boolean, default: false },
  activeSessions: { type: Number, default: 0 },
  themeLabel: { type: String, default: '' },
  warningCount: { type: Number, default: 0 },
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
  'activate-suggestion'
]);

const themeIcon = computed(() => {
  const label = props.themeLabel || '';
  if (label.includes('浅') || label.toLowerCase().includes('light')) return Sun;
  if (label.includes('深') || label.toLowerCase().includes('dark')) return Moon;
  return RefreshCw;
});

const statusText = computed(() =>
  props.backendReady
    ? `online · ${props.activeSessions} sessions`
    : `preview · ${props.activeSessions} sessions`
);

const suggestions = computed(() => props.searchState?.suggestions || []);
const isOpen = computed(() => !!props.searchState?.open && suggestions.value.length);

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
      <span class="status-chip" :class="{ 'is-online': backendReady }">
        <span class="status-dot" :class="{ running: backendReady }" aria-hidden="true"></span>
        {{ statusText }}
      </span>

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

.status-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  font-size: var(--text-xs);
  color: var(--app-muted);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-pill);
}

.status-dot {
  width: 6px;
  height: 6px;
  border-radius: var(--radius-pill);
  background: var(--danger);
  flex-shrink: 0;
}

.status-dot.running {
  background: var(--success);
}

.has-warning {
  color: var(--warn);
}
</style>
