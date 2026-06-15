<script setup>
import { computed } from 'vue';
import { Search, X } from 'lucide-vue-next';

const props = defineProps({
  modelValue: { type: [String, Number], default: '' },
  placeholder: { type: String, default: '' },
  type: { type: String, default: 'text' }, // text | password | search | number
  error: { type: String, default: '' },
  disabled: { type: Boolean, default: false },
  mono: { type: Boolean, default: false }
});
const emit = defineEmits(['update:modelValue']);

const inputEl = ($event) => $event.target.value;

const isSearch = computed(() => props.type === 'search');
const hasValue = computed(() => props.modelValue !== '' && props.modelValue !== null && props.modelValue !== undefined);

function onInput(e) {
  emit('update:modelValue', inputEl(e));
}
function clear() {
  emit('update:modelValue', '');
}
</script>

<template>
  <div class="app-input" :class="{ 'has-error': !!error, 'is-disabled': disabled, 'is-mono': mono }">
    <div class="app-input-wrap">
      <input
        class="app-input-field"
        :type="type"
        :value="modelValue"
        :placeholder="placeholder"
        :disabled="disabled"
        @input="onInput"
      />
      <button
        v-if="isSearch && hasValue"
        type="button"
        class="app-input-clear"
        title="清空"
        tabindex="-1"
        @click="clear"
      >
        <X :size="14" />
      </button>
      <span v-else-if="isSearch" class="app-input-icon" aria-hidden="true">
        <Search :size="14" />
      </span>
    </div>
    <div v-if="error" class="app-input-error">{{ error }}</div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.app-input {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.app-input-wrap {
  position: relative;
  display: flex;
  align-items: center;
}

.app-input-field {
  width: 100%;
  padding: 6px 10px;
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

.app-input.is-mono .app-input-field {
  font-family: var(--font-mono);
}

.app-input-field::placeholder {
  color: var(--app-subtle);
}

.app-input-field:focus {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

.app-input.has-error .app-input-field {
  border-color: var(--danger);
}

.app-input.is-disabled .app-input-field {
  opacity: 0.5;
  cursor: not-allowed;
}

.app-input-icon,
.app-input-clear {
  position: absolute;
  right: 8px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--app-muted);
  background: transparent;
  border: none;
  padding: 0;
  cursor: pointer;
}
.app-input-clear:hover {
  color: var(--app-strong);
}

.app-input-error {
  font-size: var(--text-xs);
  color: var(--danger);
}
</style>
