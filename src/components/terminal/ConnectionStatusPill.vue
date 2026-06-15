<script setup>
import { computed } from 'vue';

const props = defineProps({
  status: { type: String, default: 'idle' },
  tooltip: { type: String, default: '' },
  reconnectAttempt: { type: Number, default: 0 },
  reconnectTotal: { type: Number, default: 0 }
});

const dotClass = computed(() => `dot ${props.status || 'idle'}`);
const pillClass = computed(() => `status-pill ${props.status || 'idle'}`);
const label = computed(() => {
  switch (props.status) {
    case 'connected': return '已连接';
    case 'connecting': return '连接中';
    case 'disconnected': return '已断开';
    case 'reconnecting': return `重连 ${props.reconnectAttempt}/${props.reconnectTotal}`;
    case 'error': return '错误';
    default: return '空闲';
  }
});
const tooltipText = computed(() => props.tooltip || label.value);
</script>

<template>
  <span :class="pillClass" :title="tooltipText" role="status" :aria-label="tooltipText">
    <span :class="dotClass" aria-hidden="true"></span>
    <span class="pill-label">{{ label }}</span>
  </span>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.status-pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 10px;
  border-radius: var(--radius-pill);
  font-size: var(--text-xs);
  font-weight: 500;
  border: 1px solid var(--app-border);
  background: var(--app-panel-2);
  color: var(--app-text);
  line-height: 1.4;
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: var(--radius-pill);
  background: var(--app-muted);
  flex: 0 0 auto;
}

.dot.connected {
  background: var(--success);
  box-shadow: 0 0 0 3px color-mix(in oklab, var(--success) 25%, transparent);
}
.dot.connecting { background: var(--warn); animation: pulse 1.2s ease-in-out infinite; }
.dot.reconnecting { background: var(--warn); animation: pulse 0.8s ease-in-out infinite; }
.dot.disconnected { background: var(--danger); }
.dot.error { background: var(--danger); }
.dot.idle { background: var(--app-subtle); }

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}
</style>
