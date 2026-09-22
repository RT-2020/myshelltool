<script setup lang="ts">
/**
 * TitleBarSearch — 标题栏全局搜索（v0.20 自 WorkbenchShell 拆出，S2 的一刀；
 * 逻辑原样迁移：combobox 语义/allowClear/键盘导航/建议列表）。
 * store-bound（workbench re-export 的 ui 搜索 action），零 props/emit。
 * 样式类名（tb-search* / tb-suggest*）保持不变——全局 workbench-shell.scss
 * 与 narrow.scss 断点继续生效，无需迁移。
 */
import { computed, nextTick, ref, watch } from 'vue';
import { Search, X } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';
import type { SearchSuggestion } from '@/types/domain';

const store = useWorkbenchStore();

const searchInputRef = ref<HTMLInputElement | null>(null);
const activeSearchIndex = ref(0);

const searchState = computed(() => store.searchState || { open: false, query: '', suggestions: [] });
const searchSuggestions = computed(() => searchState.value.suggestions || []);
const searchOpen = computed(() => Boolean(searchState.value.open && searchSuggestions.value.length));

watch(
  () => searchState.value.open,
  open => {
    if (open) nextTick(() => searchInputRef.value?.focus());
  }
);

watch(searchSuggestions, () => {
  activeSearchIndex.value = 0;
});

function openSearch() {
  store.openGlobalSearch();
  nextTick(() => searchInputRef.value?.focus());
}

function updateSearchQuery(event: Event) {
  store.setGlobalSearchQuery((event.target as HTMLInputElement).value);
}

function clearSearch() {
  store.setGlobalSearchQuery('');
  searchInputRef.value?.focus();
}

function activateSearchSuggestion(item: SearchSuggestion) {
  store.activateSuggestion(item);
}

function onSearchKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault();
    store.closeGlobalSearch();
    searchInputRef.value?.blur();
    return;
  }
  if (!searchSuggestions.value.length) return;
  if (event.key === 'ArrowDown') {
    event.preventDefault();
    activeSearchIndex.value = Math.min(activeSearchIndex.value + 1, searchSuggestions.value.length - 1);
    return;
  }
  if (event.key === 'ArrowUp') {
    event.preventDefault();
    activeSearchIndex.value = Math.max(activeSearchIndex.value - 1, 0);
    return;
  }
  if (event.key === 'Enter') {
    event.preventDefault();
    activateSearchSuggestion(searchSuggestions.value[activeSearchIndex.value]);
  }
}
</script>

<template>
  <div
    class="tb-search"
    :class="{ 'has-value': Boolean(searchState.query) }"
    data-no-drag="true"
    role="combobox"
    :aria-expanded="searchOpen ? 'true' : 'false'"
    aria-label="全局搜索"
    @click="openSearch"
  >
    <Search :size="14" class="tb-search-icon" />
    <input
      ref="searchInputRef"
      class="tb-search-input tb-search-text"
      type="search"
      :value="searchState.query"
      placeholder="搜索连接 / 命令 / 文件"
      spellcheck="false"
      autocomplete="off"
      @input="updateSearchQuery"
      @keydown="onSearchKeydown"
    />
    <!-- antd allowClear 语义：有值才出现；点击清空并保持聚焦（不吞焦点） -->
    <button
      v-if="searchState.query"
      class="tb-search-clear"
      type="button"
      aria-label="清空搜索"
      title="清空"
      @click.stop="clearSearch"
    >
      <X :size="12" />
    </button>
    <kbd>Ctrl K</kbd>
    <Transition name="tb-suggest">
      <ul v-if="searchOpen" class="tb-suggestions" role="listbox">
        <li
          v-for="(item, idx) in searchSuggestions"
          :key="item.kind === 'asset' ? item.asset.id : `${item.kind}-${item.host}-${idx}`"
          :class="{ active: idx === activeSearchIndex }"
          role="option"
          :aria-selected="idx === activeSearchIndex ? 'true' : 'false'"
          @mouseenter="activeSearchIndex = idx"
          @mousedown.prevent="activateSearchSuggestion(item)"
        >
          <strong>{{ item.kind === 'asset' ? item.asset.name : item.label }}</strong>
          <span>
            {{ item.kind === 'asset' ? `${item.asset.username}@${item.asset.host}` : `${item.username}@${item.host}:${item.port}` }}
          </span>
        </li>
      </ul>
    </Transition>
  </div>
</template>
