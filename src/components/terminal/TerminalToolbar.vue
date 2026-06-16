<script setup>
import { computed } from 'vue';
import { Search, Copy, ClipboardPaste, Plus, Minus, Eraser, RefreshCw, PanelRight, Maximize2 } from 'lucide-vue-next';
import ConnectionStatusPill from './ConnectionStatusPill.vue';

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
  'reconnect', 'toggle-aside', 'fullscreen', 'connect'
]);

const status = computed(() => {
  if (props.reconnectAttempt > 0) return 'reconnecting';
  return props.session?.status || (props.selectedAsset ? 'idle' : 'idle');
});
const hostName = computed(() => props.session?.asset?.name || props.selectedAsset?.name || '未连接');
const oscTitle = computed(() => props.session?.oscTitle || '');
</script>

<template>
  <div class="pane-toolbar terminal-toolbar">
    <div class="terminal-toolbar-left">
      <ConnectionStatusPill
        :status="status"
        :reconnect-attempt="reconnectAttempt"
        :reconnect-total="reconnectTotal"
      />
      <strong class="terminal-host">{{ hostName }}</strong>
      <span class="muted terminal-meta">{{ subtitle }}</span>
      <span v-if="oscTitle" class="terminal-osc-title">· {{ oscTitle }}</span>
    </div>
    <div class="terminal-toolbar-right">
      <span class="terminal-font-badge" :title="'字体 ' + fontSize + 'px（Ctrl+= / Ctrl+- / Ctrl+0 / Ctrl+滚轮）'">{{ fontSize }}px</span>
      <div class="icon-toolbar" role="group" aria-label="终端操作">
        <button class="icon-tool" aria-label="搜索" title="搜索 (Ctrl+Shift+F)" @click="emit('search')"><Search :size="16" /></button>
        <button class="icon-tool" aria-label="复制选中" title="复制 (Ctrl+Shift+C)" @click="emit('copy')"><Copy :size="16" /></button>
        <button class="icon-tool" aria-label="粘贴" title="粘贴 (Ctrl+Shift+V)" @click="emit('paste')"><ClipboardPaste :size="16" /></button>
        <button class="icon-tool" aria-label="字体增大" title="字体增大 (Ctrl+=)" @click="emit('font-inc')"><Plus :size="16" /></button>
        <button class="icon-tool" aria-label="字体减小" title="字体减小 (Ctrl+-)" @click="emit('font-dec')"><Minus :size="16" /></button>
        <button class="icon-tool" aria-label="清屏" title="清屏" @click="emit('clear')"><Eraser :size="16" /></button>
        <button v-if="session && session.status !== 'connected'" class="icon-tool warn" aria-label="重连" title="重连" @click="emit('reconnect')"><RefreshCw :size="16" /></button>
        <button class="icon-tool" :class="{ active: asideOpen }" aria-label="会话详情抽屉" title="会话详情抽屉" @click="emit('toggle-aside')"><PanelRight :size="16" /></button>
        <button class="icon-tool" aria-label="全屏" title="全屏 (Alt+Enter)" @click="emit('fullscreen')"><Maximize2 :size="16" /></button>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.terminal-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border-block-end: 1px solid var(--app-border);
  background: var(--app-panel-2); // 统一 center 区 toolbar 底色：与 file-surface-toolbar 一致
}

.terminal-toolbar-left {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
  flex: 1;
}

.terminal-host {
  font-size: var(--text-sm);
  font-weight: 600; // 统一 header 标题字重：与 sidebar-header / file-surface-title 一致
  color: var(--app-strong);
}

.terminal-meta {
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
}

.terminal-osc-title {
  font-size: var(--text-xs);
  color: var(--app-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 200px;
}

.terminal-toolbar-right {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex: 0 0 auto;
}

.terminal-font-badge {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--app-muted);
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  background: var(--app-panel-2);
  border: 1px solid var(--app-border);
  font-variant-numeric: tabular-nums;
}

.icon-toolbar {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.icon-tool {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--app-muted);
  cursor: pointer;
  border-radius: var(--radius-sm);
  transition: background var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard);
}

.icon-tool:hover {
  background: var(--app-hover);
  color: var(--app-strong);
}

.icon-tool:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

.icon-tool.active {
  background: color-mix(in oklab, var(--accent) 15%, transparent);
  color: var(--accent);
}

.icon-tool.warn {
  color: var(--warn);
}

.icon-tool.warn:hover {
  background: color-mix(in oklab, var(--warn) 18%, transparent);
  color: var(--warn);
}
</style>
