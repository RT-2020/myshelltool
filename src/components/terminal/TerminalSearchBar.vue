<script setup lang="ts">
import { computed, ref, watch, nextTick } from 'vue';
import { ChevronUp, ChevronDown, X, CaseSensitive, Regex, WholeWord } from 'lucide-vue-next';

const props = withDefaults(defineProps<{
  open?: boolean;
  query?: string;
  result?: string | null;
  matchIndex?: number;
  matchTotal?: number;
  caseSensitive?: boolean;
  regex?: boolean;
  wholeWord?: boolean;
}>(), {
  open: false,
  query: '',
  result: '',
  matchIndex: 0,
  matchTotal: 0,
  caseSensitive: false,
  regex: false,
  wholeWord: false
});
const emit = defineEmits<{
  'update:query': [value: string];
  'update:caseSensitive': [value: boolean];
  'update:regex': [value: boolean];
  'update:wholeWord': [value: boolean];
  next: [];
  prev: [];
  close: [];
}>();

const inputRef = ref<HTMLInputElement | null>(null);
watch(() => props.open, async (v) => {
  if (v) {
    await nextTick();
    inputRef.value?.focus();
    inputRef.value?.select();
  }
});

const matchText = computed(() => {
  if (!props.query) return '';
  if (props.matchTotal > 0) return `${props.matchIndex || 0}/${props.matchTotal}`;
  if (props.result === 'miss') return '未匹配';
  return '';
});
</script>

<template>
  <div v-if="open" class="terminal-searchbar" role="search">
    <input
      ref="inputRef"
      :value="query"
      type="search"
      placeholder="查找终端内容"
      spellcheck="false"
      aria-label="搜索终端内容"
      @input="emit('update:query', ($event.target as HTMLInputElement).value)"
      @keydown.enter.prevent="$event.shiftKey ? emit('prev') : emit('next')"
      @keydown.escape.prevent="emit('close')"
    />
    <div class="search-actions">
      <button type="button" class="search-toggle" :class="{ active: caseSensitive }" title="区分大小写" :aria-pressed="caseSensitive" @click="emit('update:caseSensitive', !caseSensitive)"><CaseSensitive :size="14" /></button>
      <button type="button" class="search-toggle" :class="{ active: wholeWord }" title="全字匹配" :aria-pressed="wholeWord" @click="emit('update:wholeWord', !wholeWord)"><WholeWord :size="14" /></button>
      <button type="button" class="search-toggle" :class="{ active: regex }" title="正则表达式" :aria-pressed="regex" @click="emit('update:regex', !regex)"><Regex :size="14" /></button>
      <span class="search-count" :class="{ miss: result === 'miss' }" :title="matchText || undefined" aria-live="polite">{{ matchText }}</span>
      <button type="button" class="search-nav-btn" title="上一个 (Shift+Enter)" aria-label="上一个匹配" @click="emit('prev')"><ChevronUp :size="14" /></button>
      <button type="button" class="search-nav-btn" title="下一个 (Enter)" aria-label="下一个匹配" @click="emit('next')"><ChevronDown :size="14" /></button>
      <button type="button" class="search-nav-btn" title="关闭 (Esc)" aria-label="关闭搜索" @click="emit('close')"><X :size="14" /></button>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.terminal-searchbar {
  position: absolute;
  top: var(--space-2);
  right: var(--space-2);
  z-index: var(--z-popover);
  // 窄画布下随终端容器同步收窄；主桌面宽/中画布保留足够输入宽度，极端窄宽时工具组整组换行。
  inline-size: clamp(390px, 56%, 480px);
  max-inline-size: calc(100% - 2 * var(--space-2));
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-end;
  gap: var(--space-1);
  padding: var(--space-1) var(--space-2);
  background: var(--app-panel-2);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  box-shadow: var(--shadow-md);
}

.terminal-searchbar input {
  flex: 1 1 var(--space-20);
  min-inline-size: 0;
  inline-size: 0;
  min-block-size: 26px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel);
  padding: 3px var(--space-2);
  font-family: var(--font-body);
  font-size: var(--text-xs);
  line-height: 1.4;
  color: var(--app-strong);
  outline: none;
  transition: border-color var(--dur-fast) var(--ease-standard),
    box-shadow var(--dur-fast) var(--ease-standard);
}

.terminal-searchbar input::-webkit-search-cancel-button {
  display: none;
  appearance: none;
}

.terminal-searchbar input:focus {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

.search-actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-end;
  gap: var(--space-1);
  max-inline-size: 100%;
}

.search-actions > button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  inline-size: 26px;
  block-size: 26px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel);
  color: var(--app-muted);
  cursor: pointer;
  transition: background var(--dur-fast) var(--ease-standard),
    border-color var(--dur-fast) var(--ease-standard),
    color var(--dur-fast) var(--ease-standard);
}

.search-actions > button:hover {
  border-color: var(--app-border-strong);
  background: var(--app-hover);
  color: var(--app-strong);
}

.search-actions > button:active {
  background: var(--app-active);
}

.search-toggle.active {
  background: color-mix(in oklab, var(--accent) 18%, transparent);
  color: var(--accent);
  border-color: var(--accent);
}

.search-count {
  flex: none;
  padding: 0 var(--space-1);
  max-inline-size: 8ch;
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--app-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: end;
  font-variant-numeric: tabular-nums;
}

.search-count:empty { display: none; }

.search-count.miss { color: var(--danger); }
</style>
