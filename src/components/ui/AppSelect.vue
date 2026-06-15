<script setup>
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue';
import { ChevronDown, Check } from 'lucide-vue-next';

const props = defineProps({
  modelValue: { type: [String, Number], default: '' },
  options: { type: Array, default: () => [] }, // [{ label, value }]
  placeholder: { type: String, default: '' }
});
const emit = defineEmits(['update:modelValue']);

const open = ref(false);
const rootRef = ref(null);

const currentLabel = computed(() => {
  const found = props.options.find((o) => o.value === props.modelValue);
  return found ? found.label : '';
});

function toggle() {
  open.value = !open.value;
}

function select(value) {
  emit('update:modelValue', value);
  open.value = false;
}

function onDocClick(e) {
  if (rootRef.value && !rootRef.value.contains(e.target)) {
    open.value = false;
  }
}

function onKeydown(e) {
  if (!open.value) return;
  if (e.key === 'Escape') {
    e.preventDefault();
    open.value = false;
  }
}

onMounted(() => {
  document.addEventListener('click', onDocClick);
  document.addEventListener('keydown', onKeydown);
});
onBeforeUnmount(() => {
  document.removeEventListener('click', onDocClick);
  document.removeEventListener('keydown', onKeydown);
});

watch(open, () => {});
</script>

<template>
  <div class="app-select" ref="rootRef">
    <button
      type="button"
      class="app-select-trigger"
      :class="{ 'is-open': open }"
      @click="toggle"
    >
      <span class="app-select-value" :class="{ 'is-placeholder': !currentLabel }">
        {{ currentLabel || placeholder }}
      </span>
      <ChevronDown :size="14" class="app-select-caret" />
    </button>
    <Transition name="app-select-menu">
      <ul v-if="open" class="app-select-menu" role="listbox">
        <li
          v-for="opt in options"
          :key="opt.value"
          class="app-select-option"
          :class="{ 'is-selected': opt.value === modelValue }"
          role="option"
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
  outline: none;
  text-align: left;
  transition: border-color var(--motion-fast) var(--ease-standard),
    box-shadow var(--motion-fast) var(--ease-standard);
}
.app-select-trigger.is-open {
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
