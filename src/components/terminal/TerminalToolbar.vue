<script setup>
import { computed } from 'vue';
import { Search, Copy, ClipboardPaste, Plus, Minus, Eraser, RefreshCw, XCircle, PanelRight, Maximize2 } from 'lucide-vue-next';

const props = defineProps({
  session: { type: Object, default: null },
  fontSize: { type: Number, default: 14 },
  subtitle: { type: String, default: '' },
  asideOpen: { type: Boolean, default: false },
  reconnectAttempt: { type: Number, default: 0 },
  reconnectTotal: { type: Number, default: 0 },
  selectedAsset: { type: Object, default: null }
});

const emit = defineEmits([
  'search', 'copy', 'paste', 'font-inc', 'font-dec', 'clear',
  'reconnect', 'cancel-connect', 'toggle-aside', 'fullscreen', 'connect'
]);

const status = computed(() => {
  if (props.reconnectAttempt > 0) return 'reconnecting';
  return props.session?.status || (props.selectedAsset ? 'idle' : 'idle');
});

const hostName = computed(() => props.session?.asset?.name || props.selectedAsset?.name || '未连接');
const oscTitle = computed(() => props.session?.oscTitle || '');
const statusLabel = computed(() => {
  if (props.reconnectAttempt > 0) return `重连 ${props.reconnectAttempt}/${props.reconnectTotal}`;
  switch (status.value) {
    case 'connected': return '已连接';
    case 'connecting': return '连接中';
    case 'disconnected': return '已断开';
    case 'error': return '错误';
    default: return '空闲';
  }
});
</script>

<template>
  <div class="term-toolbar-row terminal-toolbar" role="toolbar" aria-label="终端操作">
    <button class="icon-btn" aria-label="终端内搜索" title="搜索 (Ctrl+Shift+F)" @click="emit('search')"><Search :size="16" /></button>
    <button class="icon-btn" aria-label="复制" title="复制 (Ctrl+Shift+C)" @click="emit('copy')"><Copy :size="16" /></button>
    <button class="icon-btn" aria-label="粘贴" title="粘贴 (Ctrl+Shift+V)" @click="emit('paste')"><ClipboardPaste :size="16" /></button>
    <span class="tb-sep" aria-hidden="true"></span>
    <button class="icon-btn" aria-label="字号增大" title="字号增大 (Ctrl+=)" @click="emit('font-inc')"><Plus :size="16" /></button>
    <button class="icon-btn" aria-label="字号减小" title="字号减小 (Ctrl+-)" @click="emit('font-dec')"><Minus :size="16" /></button>
    <span class="terminal-font-badge" :title="'字体 ' + fontSize + 'px'">{{ fontSize }}px</span>
    <span class="tb-sep" aria-hidden="true"></span>
    <button class="icon-btn" aria-label="清屏" title="清屏" @click="emit('clear')"><Eraser :size="16" /></button>
    <button v-if="session && session.status === 'connecting'" class="icon-btn warn" aria-label="取消连接" title="取消连接" @click="emit('cancel-connect')"><XCircle :size="16" /></button>
    <button v-if="session && (session.status === 'disconnected' || session.status === 'error')" class="icon-btn warn" aria-label="重连" title="重连" @click="emit('reconnect')"><RefreshCw :size="16" /></button>
    <button class="icon-btn" :class="{ active: asideOpen }" aria-label="会话详情" title="会话详情抽屉" @click="emit('toggle-aside')"><PanelRight :size="16" /></button>
    <button class="icon-btn" aria-label="全屏" title="全屏 (Alt+Enter)" @click="emit('fullscreen')"><Maximize2 :size="16" /></button>

    <div class="term-toolbar-spacer"></div>

    <span class="term-status-pill" role="status" aria-live="polite" :title="subtitle || hostName">
      <span :class="['dot', status]" aria-hidden="true"></span>
      <span>{{ statusLabel }}</span>
      <span class="terminal-host">{{ hostName }}</span>
      <span v-if="oscTitle" class="terminal-osc-title">· {{ oscTitle }}</span>
    </span>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.term-toolbar-row {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 0 8px;
  background: var(--app-panel);
  border-bottom: 1px solid var(--app-border);
  min-width: 0;
}

// 通用 icon-btn（含 .active/.warn）已收敛为全局类（_utilities.scss 单一权威实现）

.tb-sep {
  width: 1px;
  height: 16px;
  background: var(--app-border);
  margin: 0 4px;
}

.term-toolbar-spacer {
  flex: 1;
  min-width: var(--space-2);
}

.terminal-font-badge,
.term-status-pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 24px;
  padding: 0 8px;
  border-radius: var(--radius-pill);
  background: var(--app-panel-2);
  border: 1px solid var(--app-border);
  font: 11px var(--font-mono);
  color: var(--app-muted);
  white-space: nowrap;
}

// 状态圆点已收敛为全局 .dot（_utilities.scss，含 connecting/reconnecting pulse）

.terminal-host {
  max-width: 130px;
  overflow: hidden;
  text-overflow: ellipsis;
  color: var(--app-text);
}
.terminal-osc-title {
  max-width: 180px;
  overflow: hidden;
  text-overflow: ellipsis;
  color: var(--app-subtle);
}
</style>
