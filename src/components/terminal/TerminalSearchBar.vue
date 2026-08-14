<script setup>
import { computed, ref, watch, nextTick } from 'vue';
import { Search, ChevronUp, ChevronDown, X, CaseSensitive, Regex, WholeWord } from 'lucide-vue-next';

const props = defineProps({
  open: { type: Boolean, default: false },
  query: { type: String, default: '' },
  result: { type: String, default: '' },
  matchIndex: { type: Number, default: 0 },
  matchTotal: { type: Number, default: 0 },
  caseSensitive: { type: Boolean, default: false },
  regex: { type: Boolean, default: false },
  wholeWord: { type: Boolean, default: false }
});
const emit = defineEmits([
  'update:query', 'update:caseSensitive', 'update:regex', 'update:wholeWord',
  'next', 'prev', 'close'
]);

const inputRef = ref(null);
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
    <Search :size="14" class="search-icon" aria-hidden="true" />
    <input
      ref="inputRef"
      :value="query"
      type="search"
      placeholder="搜索终端内容 (Enter 下一个 / Shift+Enter 上一个 / Esc 关闭)"
      spellcheck="false"
      aria-label="搜索终端内容"
      @input="emit('update:query', $event.target.value)"
      @keydown.enter.prevent="emit($event.shiftKey ? 'prev' : 'next')"
      @keydown.escape.prevent="emit('close')"
    />
    <button class="search-toggle" :class="{ active: caseSensitive }" title="区分大小写" :aria-pressed="caseSensitive" @click="emit('update:caseSensitive', !caseSensitive)"><CaseSensitive :size="14" /></button>
    <button class="search-toggle" :class="{ active: wholeWord }" title="全字匹配" :aria-pressed="wholeWord" @click="emit('update:wholeWord', !wholeWord)"><WholeWord :size="14" /></button>
    <button class="search-toggle" :class="{ active: regex }" title="正则表达式" :aria-pressed="regex" @click="emit('update:regex', !regex)"><Regex :size="14" /></button>
    <span v-if="matchText" class="search-count" :class="{ miss: result === 'miss' }" aria-live="polite">{{ matchText }}</span>
    <button class="btn mini" title="上一个 (Shift+Enter)" aria-label="上一个匹配" @click="emit('prev')"><ChevronUp :size="14" /></button>
    <button class="btn mini" title="下一个 (Enter)" aria-label="下一个匹配" @click="emit('next')"><ChevronDown :size="14" /></button>
    <button class="btn mini" title="关闭 (Esc)" aria-label="关闭搜索" @click="emit('close')"><X :size="14" /></button>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.terminal-searchbar {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  background: var(--app-panel-2);
  border-block-end: 1px solid var(--app-border);
}

.search-icon {
  color: var(--app-muted);
  flex: 0 0 auto;
}

.terminal-searchbar input {
  flex: 1;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel);
  padding: 4px 8px;
  font-size: var(--text-sm);
  color: var(--app-strong);
  outline: none;
  transition: border-color var(--motion-fast) var(--ease-standard),
    box-shadow var(--motion-fast) var(--ease-standard);
}

.terminal-searchbar input:focus {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

.search-toggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel);
  color: var(--app-muted);
  cursor: pointer;
  transition: background var(--motion-fast) var(--ease-standard),
    border-color var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard);
}

.search-toggle:hover {
  background: var(--app-hover);
  color: var(--app-strong);
}

.search-toggle.active {
  background: color-mix(in oklab, var(--accent) 18%, transparent);
  color: var(--accent);
  border-color: var(--accent);
}

.search-count {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--app-muted);
  padding: 0 var(--space-2);
  min-width: 50px;
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.search-count.miss { color: var(--danger); }
</style>
