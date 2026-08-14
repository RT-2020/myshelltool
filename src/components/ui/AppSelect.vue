<script setup>
import { ref, computed, onMounted, onBeforeUnmount } from 'vue';
import { ChevronDown, Check } from 'lucide-vue-next';

const props = defineProps({
  modelValue: { type: [String, Number], default: '' },
  options: { type: Array, default: () => [] }, // [{ label, value }]
  placeholder: { type: String, default: '' }
});
const emit = defineEmits(['update:modelValue']);

const open = ref(false);
const rootRef = ref(null);
const menuRef = ref(null);
const activeIndex = ref(-1);

let typeaheadTimer = null;
let typeaheadBuffer = '';

const currentLabel = computed(() => {
  const found = props.options.find((o) => o.value === props.modelValue);
  return found ? found.label : '';
});

const activeDescendant = computed(() => {
  if (!open.value || activeIndex.value < 0) return undefined;
  const opt = props.options[activeIndex.value];
  return opt ? `app-select-option-${opt.value}` : undefined;
});

function closeMenu() {
  open.value = false;
  activeIndex.value = -1;
  typeaheadBuffer = '';
}

function toggle() {
  if (open.value) closeMenu();
  else {
    open.value = true;
    activeIndex.value = -1;
  }
}

function select(value) {
  emit('update:modelValue', value);
  closeMenu();
}

function scrollActiveIntoView() {
  if (activeIndex.value < 0 || !menuRef.value) return;
  const el = menuRef.value.querySelectorAll('.app-select-option')[activeIndex.value];
  if (el) el.scrollIntoView({ block: 'nearest' });
}

function moveHighlight(dir) {
  const len = props.options.length;
  if (!len) return;
  let next = activeIndex.value + dir;
  if (next < 0) next = len - 1;
  if (next >= len) next = 0;
  activeIndex.value = next;
  scrollActiveIntoView();
}

function jumpHighlight(index) {
  const len = props.options.length;
  if (!len) return;
  activeIndex.value = Math.max(0, Math.min(index, len - 1));
  scrollActiveIntoView();
}

function onTypeahead(char) {
  const len = props.options.length;
  if (!len) return;
  typeaheadBuffer = (typeaheadBuffer + char).slice(-20);
  if (typeaheadTimer) clearTimeout(typeaheadTimer);
  typeaheadTimer = setTimeout(() => {
    typeaheadBuffer = '';
  }, 800);
  const q = typeaheadBuffer.toLowerCase();
  for (let i = 0; i < len; i++) {
    const label = String(props.options[i].label || '');
    if (label.toLowerCase().startsWith(q)) {
      activeIndex.value = i;
      scrollActiveIntoView();
      return;
    }
  }
}

function onDocClick(e) {
  if (rootRef.value && !rootRef.value.contains(e.target)) {
    closeMenu();
  }
}

function onKeydown(e) {
  if (!open.value) return;
  if (e.key === 'Escape') {
    e.preventDefault();
    closeMenu();
    return;
  }
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault();
    moveHighlight(e.key === 'ArrowDown' ? 1 : -1);
    return;
  }
  if (e.key === 'Home') {
    e.preventDefault();
    jumpHighlight(0);
    return;
  }
  if (e.key === 'End') {
    e.preventDefault();
    jumpHighlight(props.options.length - 1);
    return;
  }
  if (e.key === 'Enter' || e.key === ' ') {
    e.preventDefault();
    const opt = props.options[activeIndex.value];
    if (opt) select(opt.value);
    return;
  }
  // Typeahead：可打印字符（排除组合键）
  if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
    onTypeahead(e.key);
  }
}

onMounted(() => {
  document.addEventListener('click', onDocClick);
  document.addEventListener('keydown', onKeydown);
});
onBeforeUnmount(() => {
  document.removeEventListener('click', onDocClick);
  document.removeEventListener('keydown', onKeydown);
  if (typeaheadTimer) clearTimeout(typeaheadTimer);
});
</script>

<template>
  <div class="app-select" ref="rootRef">
    <button
      type="button"
      class="app-select-trigger"
      :class="{ 'is-open': open }"
      aria-haspopup="listbox"
      :aria-expanded="String(open)"
      :aria-activedescendant="activeDescendant"
      @click="toggle"
    >
      <span class="app-select-value" :class="{ 'is-placeholder': !currentLabel }">
        {{ currentLabel || placeholder }}
      </span>
      <ChevronDown :size="14" class="app-select-caret" />
    </button>
    <Transition name="app-select-menu">
      <ul v-if="open" ref="menuRef" class="app-select-menu" role="listbox">
        <li
          v-for="opt in options"
          :key="opt.value"
          :id="`app-select-option-${opt.value}`"
          class="app-select-option"
          :class="{ 'is-selected': opt.value === modelValue, 'is-active': activeIndex >= 0 && options[activeIndex] === opt }"
          role="option"
          tabindex="-1"
          :aria-selected="String(opt.value === modelValue)"
          @click="select(opt.value)"
        >
          <span class="app-select-option-label">{{ opt.label }}</span>
          <Check v-if="opt.value === modelValue" :size="14" class="app-select-check" />
        </li>
      </ul>
    </Transition>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.app-select {
  position: relative;
  display: inline-block;
}

.app-select-trigger {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  width: 100%;
  padding: 6px 10px;
  background: var(--app-control);
  color: var(--app-text);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  font-family: var(--font-body);
  cursor: pointer;
  text-align: left;
  transition: border-color var(--motion-fast) var(--ease-standard),
    box-shadow var(--motion-fast) var(--ease-standard);
}
.app-select-trigger.is-open {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}
.app-select-trigger:focus-visible {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

.app-select-value {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.app-select-value.is-placeholder {
  color: var(--app-subtle);
}

.app-select-caret {
  color: var(--app-muted);
  flex: 0 0 auto;
  transition: transform var(--motion-fast) var(--ease-standard);
}
.app-select-trigger.is-open .app-select-caret {
  transform: rotate(180deg);
}

.app-select-menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: var(--z-dropdown);
  list-style: none;
  margin: 0;
  padding: 4px;
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  box-shadow: var(--app-shadow);
  max-height: 240px;
  overflow: auto;
}

.app-select-option {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  padding: 6px 10px;
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  color: var(--app-text);
  cursor: pointer;
}
.app-select-option:hover {
  background: var(--app-hover);
}
.app-select-option.is-active {
  background: var(--app-hover);
}
.app-select-option.is-selected {
  color: var(--accent);
}
.app-select-option-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.app-select-check {
  flex: 0 0 auto;
}

.app-select-menu-enter-active,
.app-select-menu-leave-active {
  transition: opacity var(--motion-fast) var(--ease-standard),
    transform var(--motion-fast) var(--ease-standard);
}
.app-select-menu-enter-from,
.app-select-menu-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
