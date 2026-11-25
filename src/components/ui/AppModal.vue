<script setup lang="ts">
import { ref } from 'vue';
import { X } from 'lucide-vue-next';
import { useModalFocus } from '@/composables/useModalFocus';

const props = withDefaults(
  defineProps<{
    open?: boolean;
    title?: string;
    /** CSS max-width 值（如 '480px'）。 */
    width?: string;
  }>(),
  {
    open: false,
    title: '',
    width: '480px'
  }
);
const emit = defineEmits<{ close: [] }>();

const panelRef = ref<HTMLElement | null>(null);

function close() {
  emit('close');
}

// 遮罩空白点击不关闭弹窗（防误触丢表单，与 GlobalModals 语义一致）：
// 关闭只走右上角 × / 确认 / 取消按钮 + Esc。
useModalFocus({
  open: () => props.open,
  containerRef: panelRef,
  onEscape: close
});
</script>

<template>
  <Teleport to="body">
    <Transition name="app-modal">
      <div v-if="open" class="app-modal-backdrop">
        <div
          ref="panelRef"
          class="app-modal"
          tabindex="-1"
          role="dialog"
          aria-modal="true"
          aria-labelledby="app-modal-title"
          :style="{ maxWidth: width }"
          @click.stop
        >
          <div class="app-modal-header">
            <div class="app-modal-title" id="app-modal-title">{{ title }}</div>
            <button class="app-modal-close" type="button" data-modal-close title="关闭 (Esc)" aria-label="关闭 (Esc)" @click="close">
              <X :size="16" />
            </button>
          </div>
          <div class="app-modal-body" data-modal-body>
            <slot />
          </div>
          <!-- footer 具名插槽出口（P0 修复，docs/设计/10 A1）：曾只有默认插槽，
               SshImportDialog 写在 #footer 的「取消/导入」按钮从不渲染（导入主
               操作不可达）。无 footer 传入时零 DOM 影响，既有消费方渲染不变。 -->
          <div v-if="$slots.footer" class="app-modal-footer">
            <slot name="footer" />
          </div>
        </div>
      </div>
    </Transition>
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
  max-height: calc(100vh - var(--space-20));
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
.app-modal-close:active {
  background: var(--app-active);
}

.app-modal-body {
  padding: var(--space-4);
  overflow: auto;
}

.app-modal-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  border-top: 1px solid var(--app-border);
}

// —— 开合动效：backdrop fade + panel scale/fade（enter 200ms，leave 略快）——
.app-modal-enter-active,
.app-modal-leave-active {
  transition: opacity var(--motion-base);
}
.app-modal-leave-active {
  transition: opacity var(--motion-fast);
}
.app-modal-enter-active .app-modal,
.app-modal-leave-active .app-modal {
  transition: opacity var(--motion-base), transform var(--motion-base);
}
.app-modal-leave-active .app-modal {
  transition: opacity var(--motion-fast), transform var(--motion-fast);
}
.app-modal-enter-from,
.app-modal-leave-to {
  opacity: 0;
}
.app-modal-enter-from .app-modal,
.app-modal-leave-to .app-modal {
  opacity: 0;
  transform: scale(0.96);
}
</style>
