<script setup>
import { ref, watch, nextTick, onBeforeUnmount } from 'vue';
import { X } from 'lucide-vue-next';

const props = defineProps({
  open: { type: Boolean, default: false },
  title: { type: String, default: '' },
  width: { type: String, default: '480px' }
});
const emit = defineEmits(['close']);

const panelRef = ref(null);

function close() {
  emit('close');
}

function onKeydown(e) {
  if (!props.open) return;
  if (e.key === 'Escape') {
    e.preventDefault();
    close();
    return;
  }
  if (e.key === 'Tab' && panelRef.value) {
    const focusable = panelRef.value.querySelectorAll(
      'a[href], area[href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), button:not([disabled]), [tabindex]:not([tabindex="-1"])'
    );
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;
    if (e.shiftKey) {
      if (active === first || !panelRef.value.contains(active)) {
        e.preventDefault();
        last.focus();
      }
    } else {
      if (active === last || !panelRef.value.contains(active)) {
        e.preventDefault();
        first.focus();
      }
    }
  }
}

function onBackdropClick(e) {
  if (e.target === e.currentTarget) close();
}

watch(
  () => props.open,
  (v) => {
    if (v) {
      document.addEventListener('keydown', onKeydown);
      nextTick(() => {
        if (panelRef.value) {
          const focusable = panelRef.value.querySelector(
            'a[href], area[href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), button:not([disabled]), [tabindex]:not([tabindex="-1"])'
          );
          if (focusable) focusable.focus();
        }
      });
    } else {
      document.removeEventListener('keydown', onKeydown);
    }
  }
);

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKeydown);
});
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="app-modal-backdrop" @click="onBackdropClick" @mousedown="onBackdropClick">
      <div
        ref="panelRef"
        class="app-modal"
        role="dialog"
        aria-modal="true"
        :style="{ maxWidth: width }"
        @click.stop
      >
        <div class="app-modal-header">
          <div class="app-modal-title">{{ title }}</div>
          <button class="app-modal-close" title="关闭 (Esc)" @click="close">
            <X :size="16" />
          </button>
        </div>
        <div class="app-modal-body">
          <slot />
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.app-modal-backdrop {
  position: fixed;
  inset: 0;
  background: var(--app-scrim);
  z-index: var(--z-modal);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-5);
}

.app-modal {
  width: 100%;
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-lg);
  box-shadow: var(--app-shadow);
  display: flex;
  flex-direction: column;
  max-height: calc(100vh - 80px);
  overflow: hidden;
}

.app-modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--app-border);
}

.app-modal-title {
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--app-strong);
}

.app-modal-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  background: transparent;
  color: var(--app-muted);
  cursor: pointer;
  border-radius: var(--radius-sm);
}
.app-modal-close:hover {
  background: var(--app-hover);
  color: var(--app-strong);
}

.app-modal-body {
  padding: var(--space-4);
  overflow: auto;
}
</style>
