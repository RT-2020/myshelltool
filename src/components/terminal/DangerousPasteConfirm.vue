<script setup>
import { computed, ref, watch, nextTick } from 'vue';
import { AlertTriangle, X } from 'lucide-vue-next';

const props = defineProps({
  open: { type: Boolean, default: false },
  command: { type: String, default: '' },
  matchedPattern: { type: String, default: '' }
});
const emit = defineEmits(['confirm', 'cancel']);

const rememberRule = ref(false);
const confirmRef = ref(null);

// 打开时聚焦「仍然粘贴」（Enter 即确认）；每次打开重置不再拦截勾选
watch(() => props.open, async (v) => {
  if (v) {
    rememberRule.value = false;
    await nextTick();
    confirmRef.value?.focus();
  }
});

function onKeydown(e) {
  if (e.key === 'Enter') { e.preventDefault(); confirm(); }
  else if (e.key === 'Escape') { e.preventDefault(); emit('cancel'); }
}

// confirm 携带 allowedPattern（勾选「本会话不再拦截此规则」时传 matchedPattern）
function confirm() {
  emit('confirm', rememberRule.value ? props.matchedPattern : null);
}

const preview = computed(() => {
  if (!props.command) return '';
  const lines = props.command.replace(/\r/g, '').split('\n');
  if (lines.length <= 4) return lines.join('\n');
  return lines.slice(0, 4).join('\n') + `\n… (+${lines.length - 4} 行)`;
});
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="danger-overlay" @click.self="emit('cancel')" @keydown="onKeydown" role="alertdialog" aria-modal="true" aria-label="危险命令确认">
      <div class="danger-modal">
        <header class="danger-header">
          <AlertTriangle :size="22" class="danger-icon" />
          <h2>检测到危险命令</h2>
          <button class="icon-btn" aria-label="取消" @click="emit('cancel')"><X :size="18" /></button>
        </header>
        <div class="danger-body">
          <p class="muted">即将向远程主机写入以下命令，可能造成不可逆的数据丢失或服务中断：</p>
          <pre class="danger-preview"><code>{{ preview }}</code></pre>
          <p v-if="matchedPattern" class="danger-pattern">命中规则：<code>{{ matchedPattern }}</code></p>
          <p class="muted small">如果这是你刻意执行的（例如在沙箱里测试），可以仍然粘贴。否则请取消。</p>
        </div>
        <footer class="danger-footer">
          <label class="remember-rule">
            <input v-model="rememberRule" type="checkbox" /> 本会话不再拦截此规则
          </label>
          <button class="btn ghost" @click="emit('cancel')">取消</button>
          <button ref="confirmRef" class="btn danger" @click="confirm">仍然粘贴</button>
        </footer>
      </div>
    </div>
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
</style>
