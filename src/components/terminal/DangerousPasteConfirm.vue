<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { AlertTriangle, ChevronDown, ChevronUp, X } from 'lucide-vue-next';
import { useModalFocus } from '@/composables/useModalFocus';

const props = withDefaults(defineProps<{
  open?: boolean;
  command?: string;
  matchedPattern?: string;
}>(), {
  open: false,
  command: '',
  matchedPattern: ''
});
const emit = defineEmits<{
  confirm: [allowedPattern: string | null];
  cancel: [];
}>();

const rememberRule = ref(false);
const expanded = ref(false);
const modalRef = ref<HTMLElement | null>(null);

// 03 A7/A8：打开默认焦点落安全侧「取消」；Tab 圈闭并支持关闭还焦；Enter 由原生按钮触发当前焦点动作
useModalFocus({
  open: () => props.open,
  containerRef: modalRef,
  autofocusSelector: '[data-modal-autofocus]',
  onEscape: () => emit('cancel')
});

watch(() => props.open, (v) => {
  if (v) {
    rememberRule.value = false;
    expanded.value = false;
  }
});

// confirm 携带 allowedPattern（勾选「本会话不再拦截此规则」时传 matchedPattern）
function confirm() {
  emit('confirm', rememberRule.value ? props.matchedPattern : null);
}

const allLines = computed(() => (props.command ? props.command.replace(/\r/g, '').split('\n') : []));
const hasMoreLines = computed(() => allLines.value.length > 4);
const preview = computed(() => {
  if (!allLines.value.length) return '';
  if (expanded.value || !hasMoreLines.value) return allLines.value.join('\n');
  return allLines.value.slice(0, 4).join('\n') + `\n… (+${allLines.value.length - 4} 行)`;
});
</script>

<template>
  <Teleport to="body">
    <Transition name="danger-pop">
      <div v-if="open" class="danger-overlay" @click.self="emit('cancel')">
      <div ref="modalRef" class="danger-modal" role="alertdialog" aria-modal="true" aria-label="危险命令确认">
        <header class="danger-header">
          <AlertTriangle :size="22" class="danger-icon" aria-hidden="true" />
          <h2>检测到危险命令</h2>
          <button type="button" class="icon-btn" data-modal-close aria-label="取消" @click="emit('cancel')"><X :size="18" /></button>
        </header>
        <div class="danger-body">
          <p class="muted">即将向远程主机写入以下命令，可能造成不可逆的数据丢失或服务中断：</p>
          <pre class="danger-preview"><code>{{ preview }}</code></pre>
          <button
            v-if="hasMoreLines"
            type="button"
            class="danger-expand-btn"
            :aria-expanded="expanded"
            @click="expanded = !expanded"
          >
            <ChevronUp v-if="expanded" :size="13" aria-hidden="true" />
            <ChevronDown v-else :size="13" aria-hidden="true" />
            <span>{{ expanded ? '收起预览' : `展开全部 ${allLines.length} 行` }}</span>
          </button>
          <p v-if="matchedPattern" class="danger-pattern">命中规则：<code>{{ matchedPattern }}</code></p>
          <p class="muted small">如果这是你刻意执行的（例如在沙箱里测试），可以仍然粘贴。否则请取消。</p>
        </div>
        <footer class="danger-footer">
          <label class="remember-rule">
            <input v-model="rememberRule" type="checkbox" /> 本会话不再拦截此规则
          </label>
          <button type="button" class="danger-btn" data-modal-autofocus @click="emit('cancel')">取消</button>
          <button type="button" class="danger-btn is-danger" @click="confirm">仍然粘贴</button>
        </footer>
      </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.danger-overlay {
  position: fixed;
  inset: 0;
  background: color-mix(in oklab, var(--app-scrim) 60%, transparent);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: var(--z-modal);
  backdrop-filter: blur(4px);
}

// 面板进出场：告警弹窗的进场幅度刻意小于普通弹窗（scale 0.97）——
// 危险确认要的是「稳」而不是「蹦」；出场快档只淡出
.danger-pop-enter-active {
  transition: opacity var(--dur-base) var(--ease-standard);
}
.danger-pop-enter-active .danger-modal {
  transition: transform var(--dur-base) var(--ease-emphasized),
    opacity var(--dur-base) var(--ease-emphasized);
}
.danger-pop-enter-from {
  opacity: 0;
}
.danger-pop-enter-from .danger-modal {
  transform: scale(0.97);
  opacity: 0;
}
.danger-pop-leave-active {
  transition: opacity var(--dur-fast) var(--ease-standard);
  pointer-events: none;
}
.danger-pop-leave-to {
  opacity: 0;
}

.danger-modal {
  background: var(--app-panel);
  border: 1px solid color-mix(in oklab, var(--danger) 50%, var(--app-border));
  border-radius: var(--radius-lg);
  box-shadow: var(--elev-raised);
  width: min(560px, 92vw);
  overflow: hidden;
}

.danger-header {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  background: color-mix(in oklab, var(--danger) 12%, var(--app-panel));
  border-block-end: 1px solid var(--app-border);
}

.danger-header h2 {
  margin: 0;
  flex: 1;
  font-size: var(--text-base);
  color: var(--danger);
}

.danger-icon { color: var(--danger); }

.danger-body {
  padding: var(--space-4);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.danger-preview {
  margin: 0;
  padding: var(--space-3);
  background: var(--app-panel-2);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  color: var(--app-strong);
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 240px;
  overflow: auto;
}

.danger-preview code { font-family: inherit; }

.danger-expand-btn {
  align-self: flex-start;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
  color: var(--app-muted);
  font-size: var(--text-xs);
  cursor: pointer;
  transition: background var(--dur-fast) var(--ease-standard),
    border-color var(--dur-fast) var(--ease-standard),
    color var(--dur-fast) var(--ease-standard);
}
.danger-expand-btn:hover {
  background: var(--app-hover);
  border-color: var(--app-border-strong);
  color: var(--app-strong);
}
.danger-expand-btn:active {
  background: var(--app-active);
}

.danger-pattern {
  font-size: var(--text-xs);
  color: var(--app-muted);
}

.danger-pattern code {
  font-family: var(--font-mono);
  color: var(--warn);
}

.small { font-size: var(--text-xs); }

.remember-rule {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-xs);
  color: var(--app-muted);
  cursor: pointer;
  margin-right: auto;
}
.remember-rule input {
  accent-color: var(--accent);
}

.danger-footer {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  border-block-start: 1px solid var(--app-border);
}

/* 本组件位于 Teleport body 独立作用域且需与 danger 弹窗语气一致，按 GlobalModals .btn 同 token 自绘（03 A1） */
.danger-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 32px;
  padding: 0 var(--space-4);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-control);
  color: var(--app-text);
  font-size: var(--text-sm);
  font-weight: 500;
  cursor: pointer;
  transition: background var(--dur-fast) var(--ease-standard),
    border-color var(--dur-fast) var(--ease-standard),
    color var(--dur-fast) var(--ease-standard);
}
.danger-btn:hover {
  background: var(--app-hover);
  border-color: var(--app-border-strong);
  color: var(--app-strong);
}
.danger-btn:active {
  background: var(--app-active);
}
.danger-btn.is-danger {
  background: var(--danger);
  border-color: var(--danger);
  color: var(--accent-on);
}
.danger-btn.is-danger:hover {
  background: color-mix(in oklab, var(--danger), black 10%);
  border-color: color-mix(in oklab, var(--danger), black 10%);
  color: var(--accent-on);
}
.danger-btn.is-danger:active {
  background: color-mix(in oklab, var(--danger), black 18%);
  border-color: color-mix(in oklab, var(--danger), black 18%);
  color: var(--accent-on);
}
</style>
