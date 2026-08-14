<script setup>
import { AlertTriangle, CheckCircle2, Info, X, XCircle } from 'lucide-vue-next';
import { useUiStore } from '@/stores/ui.js';

// 直连 ui store 读 toast 队列（与 ResourceMonitorPanel 直连模式一致）
const ui = useUiStore();

const LEVEL_ICONS = {
  success: CheckCircle2,
  warn: AlertTriangle,
  error: XCircle,
  info: Info
};

function iconFor(level) {
  return LEVEL_ICONS[level] || Info;
}

// action 按钮：先执行 run()，再关闭该 toast
function runAction(toast) {
  if (toast.action && typeof toast.action.run === 'function') {
    toast.action.run();
  }
  ui.dismissToast(toast.id);
}

function dismiss(id) {
  ui.dismissToast(id);
}
</script>

<template>
  <Teleport to="body">
    <div class="toast-host" role="status" aria-live="polite">
      <TransitionGroup name="toast">
        <div
          v-for="toast in ui.toasts"
          :key="toast.id"
          class="toast-item"
          :class="toast.level"
          :role="toast.level === 'error' ? 'alert' : undefined"
        >
          <component :is="iconFor(toast.level)" :size="16" class="toast-icon" aria-hidden="true" />
          <span class="toast-message">{{ toast.message }}</span>
          <button
            v-if="toast.action"
            class="toast-action"
            type="button"
            @click="runAction(toast)"
          >
            {{ toast.action.label }}
          </button>
          <button class="toast-close" type="button" aria-label="关闭提示" @click="dismiss(toast.id)">
            <X :size="14" />
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.toast-host {
  position: fixed;
  right: 16px;
  bottom: calc(var(--statusbar-h, 28px) + 16px);
  z-index: var(--z-toast);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  // 容器不拦截点击，条目自身恢复可点
  pointer-events: none;
}

.toast-item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  max-width: 360px;
  padding: var(--space-2) var(--space-3);
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  box-shadow: var(--app-shadow);
  font-size: var(--text-sm);
  color: var(--app-text);
  pointer-events: auto;
}

// 按 level 上色：边框 + soft 背景 + 图标色
.toast-item.success { border-color: var(--success); background: var(--success-soft); }
.toast-item.warn { border-color: var(--warn); background: var(--warn-soft); }
.toast-item.error { border-color: var(--danger); background: var(--danger-soft); }
.toast-item.info { border-color: var(--accent); background: var(--accent-soft); }
.toast-item.success .toast-icon { color: var(--success); }
.toast-item.warn .toast-icon { color: var(--warn); }
.toast-item.error .toast-icon { color: var(--danger); }
.toast-item.info .toast-icon { color: var(--accent); }

.toast-icon {
  flex-shrink: 0;
}

.toast-message {
  flex: 1 1 auto;
  min-width: 0;
  overflow-wrap: break-word;
}

.toast-action {
  flex-shrink: 0;
  padding: 2px var(--space-2);
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  font-weight: 600;
  color: var(--accent);
  background: var(--accent-soft);
  cursor: pointer;
}
.toast-action:hover {
  background: var(--accent-soft-strong);
}

.toast-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border-radius: var(--radius-sm);
  color: var(--app-muted);
  cursor: pointer;
  flex-shrink: 0;
}
.toast-close:hover {
  background: var(--app-hover);
  color: var(--app-strong);
}

// 进入/退出过渡：200ms translateY + fade（--motion-base）
.toast-enter-active,
.toast-leave-active {
  transition: opacity var(--motion-base), transform var(--motion-base);
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(8px);
}
.toast-move {
  transition: transform var(--motion-base);
}
</style>
