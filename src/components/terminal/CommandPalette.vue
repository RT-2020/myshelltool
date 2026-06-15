<script setup>
import { ref, computed, watch, nextTick } from 'vue';
import { Search, CornerDownLeft, ArrowUp, ArrowDown } from 'lucide-vue-next';

const props = defineProps({
  open: { type: Boolean, default: false },
  sessions: { type: Array, default: () => [] },
  activeSessionId: { type: String, default: '' }
});
const emit = defineEmits(['close', 'selectSession', 'runAction']);

const query = ref('');
const inputRef = ref(null);
const activeIdx = ref(0);

const ACTIONS = [
  { id: 'action:search', label: '搜索终端内容', hint: 'Ctrl+Shift+F', keywords: 'search find' },
  { id: 'action:copy', label: '复制选中', hint: 'Ctrl+Shift+C', keywords: 'copy' },
  { id: 'action:paste', label: '粘贴', hint: 'Ctrl+Shift+V', keywords: 'paste' },
  { id: 'action:font-inc', label: '字体增大', hint: 'Ctrl+=', keywords: 'font larger' },
  { id: 'action:font-dec', label: '字体减小', hint: 'Ctrl+-', keywords: 'font smaller' },
  { id: 'action:font-reset', label: '字体重置', hint: 'Ctrl+0', keywords: 'font reset' },
  { id: 'action:clear', label: '清屏', hint: '', keywords: 'clear' },
  { id: 'action:fullscreen', label: '切换全屏', hint: 'Alt+Enter', keywords: 'fullscreen' },
  { id: 'action:connect', label: '连接所选主机', hint: 'Ctrl+Shift+T', keywords: 'connect ssh' },
  { id: 'action:cheatsheet', label: '查看快捷键速查', hint: '?', keywords: 'help shortcut' }
];

function fuzzyMatch(text, q) {
  if (!q) return true;
  const t = text.toLowerCase();
  const ql = q.toLowerCase();
  if (t.includes(ql)) return 2;
  // 简单 fuzzy：所有字符按序出现
  let idx = 0;
  for (const ch of t) {
    if (ch === ql[idx]) idx++;
    if (idx >= ql.length) return 1;
  }
  return idx >= ql.length ? 1 : 0;
}

const results = computed(() => {
  const items = [];
  const q = query.value.trim();
  // sessions
  for (const s of props.sessions) {
    const label = `会话: ${s.asset?.name || ''} (${s.asset?.host || ''})`;
    const score = fuzzyMatch((label + ' ' + (s.asset?.host || '')).trim(), q);
    if (score > 0) items.push({ id: 'session:' + s.sessionId, label, hint: s.status, kind: 'session', _score: score });
  }
  // actions
  for (const a of ACTIONS) {
    const score = fuzzyMatch((a.label + ' ' + a.keywords).trim(), q);
    if (score > 0) items.push({ id: a.id, label: a.label, hint: a.hint, kind: 'action', _score: score });
  }
  items.sort((a, b) => b._score - a._score);
  return items;
});

watch(() => props.open, (v) => {
  if (v) {
    query.value = '';
    activeIdx.value = 0;
    nextTick(() => inputRef.value?.focus());
  }
});

watch(results, () => { activeIdx.value = 0; });

function onKeydown(e) {
  if (e.key === 'ArrowDown') { e.preventDefault(); activeIdx.value = Math.min(activeIdx.value + 1, results.value.length - 1); }
  else if (e.key === 'ArrowUp') { e.preventDefault(); activeIdx.value = Math.max(activeIdx.value - 1, 0); }
  else if (e.key === 'Enter') {
    e.preventDefault();
    pick(results.value[activeIdx.value]);
  } else if (e.key === 'Escape') {
    e.preventDefault();
    emit('close');
  }
}

function pick(item) {
  if (!item) return;
  if (item.kind === 'session') {
    emit('selectSession', item.id.replace('session:', ''));
  } else if (item.kind === 'action') {
    emit('runAction', item.id.replace('action:', ''));
  }
  emit('close');
}
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="palette-overlay" @click.self="emit('close')">
      <div class="palette-modal" role="dialog" aria-modal="true" aria-label="命令面板">
        <div class="palette-input-wrap">
          <Search :size="18" class="palette-search-icon" />
          <input
            ref="inputRef"
            v-model="query"
            class="palette-input"
            type="text"
            placeholder="搜索会话或动作…"
            spellcheck="false"
            @keydown="onKeydown"
          />
        </div>
        <ul class="palette-list" v-if="results.length">
          <li
            v-for="(item, idx) in results"
            :key="item.id"
            :class="['palette-item', { active: idx === activeIdx }]"
            @mouseenter="activeIdx = idx"
            @click="pick(item)"
          >
            <span class="palette-item-kind" :data-kind="item.kind">{{ item.kind === 'session' ? '会话' : '动作' }}</span>
            <span class="palette-item-label">{{ item.label }}</span>
            <span v-if="item.hint" class="palette-item-hint">{{ item.hint }}</span>
            <CornerDownLeft v-if="idx === activeIdx" :size="14" class="palette-item-enter" />
          </li>
        </ul>
        <div v-else class="palette-empty">无匹配项</div>
        <footer class="palette-footer">
          <span class="palette-hint"><ArrowUp :size="12" /><ArrowDown :size="12" /> 选择</span>
          <span class="palette-hint"><CornerDownLeft :size="12" /> 执行</span>
          <span class="palette-hint">Esc 关闭</span>
        </footer>
      </div>
    </div>
  </Teleport>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.palette-overlay {
  position: fixed;
  inset: 0;
  background: color-mix(in oklab, var(--app-scrim) 40%, transparent);
  display: flex;
  justify-content: center;
  align-items: flex-start;
  padding-block-start: 12vh;
  z-index: var(--z-modal);
  backdrop-filter: blur(2px);
}

.palette-modal {
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  box-shadow: var(--elev-raised);
  width: min(560px, 92vw);
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.palette-input-wrap {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-3);
  border-block-end: 1px solid var(--app-border);
}

.palette-search-icon {
  color: var(--app-muted);
  flex: 0 0 auto;
}

.palette-input {
  flex: 1;
  border: none;
  outline: none;
  background: transparent;
  color: var(--app-strong);
  font-size: var(--text-base);
  font-family: var(--font-body);
}

.palette-input::placeholder {
  color: var(--app-muted);
}

.palette-list {
  list-style: none;
  margin: 0;
  padding: var(--space-1);
  max-height: 360px;
  overflow-y: auto;
}

.palette-item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 8px var(--space-2);
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: var(--text-sm);
}

.palette-item.active {
  background: color-mix(in oklab, var(--accent) 18%, transparent);
}

.palette-item-kind {
  font-size: 10px;
  padding: 2px 6px;
  border-radius: var(--radius-pill);
  background: var(--app-panel-2);
  color: var(--app-muted);
  flex: 0 0 auto;
}

.palette-item-kind[data-kind='session'] {
  background: color-mix(in oklab, var(--accent) 15%, transparent);
  color: var(--accent);
}

.palette-item-label {
  flex: 1;
  color: var(--app-strong);
}

.palette-item-hint {
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
}

.palette-item-enter {
  color: var(--app-muted);
  flex: 0 0 auto;
}

.palette-empty {
  padding: var(--space-4);
  text-align: center;
  color: var(--app-muted);
  font-size: var(--text-sm);
}

.palette-footer {
  display: flex;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border-block-start: 1px solid var(--app-border);
  background: var(--app-panel-2);
}

.palette-hint {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: var(--text-xs);
  color: var(--app-muted);
}
</style>
