<script setup>
defineProps({
  variant: { type: String, default: 'ghost' }, // primary | ghost | subtle | danger
  size: { type: String, default: 'md' }, // sm | md
  disabled: { type: Boolean, default: false },
  loading: { type: Boolean, default: false }
});
</script>

<template>
  <button
    class="app-btn"
    :class="[`app-btn--${variant}`, `app-btn--${size}`, { 'is-disabled': disabled, 'is-loading': loading }]"
    :disabled="disabled || loading"
  >
    <span v-if="loading" class="app-btn-spinner" aria-hidden="true"></span>
    <span class="app-btn-label"><slot /></span>
  </button>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.app-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  border-radius: var(--radius-sm);
  font-family: var(--font-body);
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
  border: 1px solid transparent;
  background: transparent;
  color: var(--app-text);
  transition: background var(--motion-fast) var(--ease-standard),
    border-color var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard),
    opacity var(--motion-fast) var(--ease-standard);
}

.app-btn--sm {
  padding: 4px 10px;
  font-size: var(--text-xs);
}
.app-btn--md {
  padding: 6px 14px;
  font-size: var(--text-sm);
}

// variants
.app-btn--primary {
  background: var(--accent);
  color: var(--accent-on);
  border-color: var(--accent);
}
.app-btn--primary:hover:not(.is-disabled) {
  background: var(--accent-hover);
  border-color: var(--accent-hover);
}

.app-btn--ghost {
  border-color: var(--app-border);
  color: var(--app-text);
}
.app-btn--ghost:hover:not(.is-disabled) {
  background: var(--app-hover);
}

.app-btn--subtle {
  color: var(--app-muted);
}
.app-btn--subtle:hover:not(.is-disabled) {
  background: var(--app-hover);
  color: var(--app-strong);
}

.app-btn--danger {
  background: var(--danger);
  color: #ffffff;
  border-color: var(--danger);
}
.app-btn--danger:hover:not(.is-disabled) {
  background: color-mix(in oklab, var(--danger), black 10%);
  border-color: color-mix(in oklab, var(--danger), black 10%);
}

.app-btn.is-disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.app-btn-spinner {
  width: 12px;
  height: 12px;
  border: 2px solid currentColor;
  border-right-color: transparent;
  border-radius: var(--radius-pill);
  animation: app-btn-spin 0.6s linear infinite;
}

@keyframes app-btn-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
