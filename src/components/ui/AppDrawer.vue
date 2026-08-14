<script setup>
import { ref, watch, nextTick, onBeforeUnmount } from 'vue';

const props = defineProps({
  open: { type: Boolean, default: false },
  side: { type: String, default: 'right' }, // right | bottom
  width: { type: String, default: '320px' },
  height: { type: String, default: 'auto' },
  ariaLabel: { type: String, default: '侧边抽屉' }
});
const emit = defineEmits(['close']);

const panelRef = ref(null);
const lastFocus = ref(null);

function close() {
  emit('close');
}

function onKeydown(e) {
  if (props.open && e.key === 'Escape') {
    e.preventDefault();
    close();
  }
}

function onBackdropClick(e) {
  if (e.target === e.currentTarget) close();
}

watch(
  () => props.open,
  (v) => {
    if (v) {
      // 打开时记住触发元素，关闭后还原焦点
      lastFocus.value = document.activeElement;
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
      if (lastFocus.value && lastFocus.value.isConnected) {
        lastFocus.value.focus();
      }
    }
  }
);

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKeydown);
});
</script>

<template>
  <Teleport to="body">
    <Transition :name="`app-drawer-fade-${side}`">
    <div v-if="open" class="app-drawer-backdrop" @click="onBackdropClick" @mousedown="onBackdropClick">
      <div
        ref="panelRef"
        class="app-drawer"
        :class="[`app-drawer--${side}`]"
        role="dialog"
        aria-modal="true"
        :aria-label="ariaLabel"
        :style="side === 'right' ? { width } : { height }"
        @click.stop
      >
        <slot />
      </div>
    </div>
    </Transition>
  </Teleport>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.app-drawer-backdrop {
  position: fixed;
  inset: 0;
  background: var(--app-scrim);
  z-index: var(--z-drawer);
  display: flex;
}

.app-drawer {
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  box-shadow: var(--app-shadow);
  overflow: auto;
}

.app-drawer--right {
  margin-left: auto;
  height: 100vh;
  border-radius: var(--radius-lg) 0 0 var(--radius-lg);
}

.app-drawer--bottom {
  margin-top: auto;
  width: 100vw;
  border-radius: var(--radius-lg) var(--radius-lg) 0 0;
}

// Transitions
.app-drawer-fade-right-enter-active,
.app-drawer-fade-right-leave-active {
  transition: opacity var(--motion-base);
}
.app-drawer-fade-right-enter-active .app-drawer,
.app-drawer-fade-right-leave-active .app-drawer {
  transition: transform var(--motion-base);
}
.app-drawer-fade-right-enter-from,
.app-drawer-fade-right-leave-to {
  opacity: 0;
}
.app-drawer-fade-right-enter-from .app-drawer,
.app-drawer-fade-right-leave-to .app-drawer {
  transform: translateX(100%);
}

.app-drawer-fade-bottom-enter-active,
.app-drawer-fade-bottom-leave-active {
  transition: opacity var(--motion-base);
}
.app-drawer-fade-bottom-enter-active .app-drawer,
.app-drawer-fade-bottom-leave-active .app-drawer {
  transition: transform var(--motion-base);
}
.app-drawer-fade-bottom-enter-from,
.app-drawer-fade-bottom-leave-to {
  opacity: 0;
}
.app-drawer-fade-bottom-enter-from .app-drawer,
.app-drawer-fade-bottom-leave-to .app-drawer {
  transform: translateY(100%);
}
</style>
