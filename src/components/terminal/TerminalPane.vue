<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { TerminalSquare, PlugZap } from 'lucide-vue-next';
import type { useSessionsStore } from '@/stores/sessions';
import type { NormalizedConnectionAsset } from '@/types/domain';

/** context-menu 事件 payload 契约（TerminalSurface 侧同构解构）。 */
interface TerminalContextMenuPayload {
  x: number;
  y: number;
  hasSelection: boolean;
}

const props = withDefaults(defineProps<{
  store: ReturnType<typeof useSessionsStore>;
  hasActiveSession?: boolean;
  hasConnectedOnce?: boolean;
  isTauriCore?: boolean;
  selectedAsset?: NormalizedConnectionAsset | null;
}>(), {
  hasActiveSession: false,
  hasConnectedOnce: false,
  isTauriCore: false,
  selectedAsset: null
});

const emit = defineEmits<{
  'connect-selected': [];
  'open-asset-editor': [];
  'context-menu': [payload: TerminalContextMenuPayload];
}>();

const mountRef = ref<HTMLDivElement | null>(null);

function setContainer() {
  if (mountRef.value && props.store?.setTerminalContainer) {
    props.store.setTerminalContainer(mountRef.value);
  }
}

onMounted(setContainer);
onBeforeUnmount(() => {
  // Keep the store container reference stable across short-lived tab remounts.
});

function onWheel(e: WheelEvent) {
  if (!e.ctrlKey) return;
  e.preventDefault();
  if (e.deltaY < 0) props.store?.runTerminalAction?.('font-inc');
  if (e.deltaY > 0) props.store?.runTerminalAction?.('font-dec');
}

function onContextMenu(event: MouseEvent) {
  if (!props.hasActiveSession) return;
  const hasSelection = Boolean(props.store?.activeSession?.term?.getSelection());
  emit('context-menu', { x: event.clientX, y: event.clientY, hasSelection });
}
</script>

<template>
  <div class="terminal-pane-host" :class="{ 'has-session': hasActiveSession }" @wheel="onWheel" @contextmenu.prevent="onContextMenu">
    <div id="terminalContainer" ref="mountRef" aria-label="终端区域"></div>

    <div v-if="!hasActiveSession" class="term-empty">
      <div v-if="!hasConnectedOnce" class="term-empty-icon" aria-hidden="true">
        <TerminalSquare :size="42" />
      </div>
      <div class="term-empty-title">{{ hasConnectedOnce ? '会话已全部关闭' : '暂无活跃会话' }}</div>
      <div class="term-empty-desc">
        {{ hasConnectedOnce ? '从左侧重新选择一台主机，' : '从左侧选择连接资产，' }}或在底部输入 <kbd>ssh user@host</kbd> 快速打开会话。
        <span v-if="!isTauriCore" class="term-empty-runtime">SSH 连接需要桌面客户端，浏览器预览下不可用。</span>
      </div>
      <div class="term-empty-actions">
        <button v-if="isTauriCore && selectedAsset" class="btn-primary" type="button" @click="emit('connect-selected')">
          <PlugZap :size="14" aria-hidden="true" />连接 {{ selectedAsset.name }}
        </button>
        <button v-if="isTauriCore" class="btn-primary secondary" type="button" @click="emit('open-asset-editor')">新增连接</button>
        <button v-else class="btn-primary" type="button" @click="emit('open-asset-editor')">打开新会话</button>
        <span class="hint-keys"><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd> 连接所选主机</span>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.terminal-pane-host {
  height: 100%;
  position: relative;
  overflow: hidden;
}

:deep(.xterm) {
  height: 100%;
  background: var(--terminal-bg);

  .xterm-viewport {
    background: var(--terminal-bg);

    &::-webkit-scrollbar {
      width: 8px;
    }

    &::-webkit-scrollbar-thumb {
      background: var(--app-subtle);
      border-radius: var(--radius-sm);
    }
  }

  .xterm-screen {
    background: var(--terminal-bg);
  }
}

#terminalContainer {
  position: relative;
  z-index: var(--z-base);
  height: 100%;
  background: var(--terminal-bg);
  overflow: hidden;
}

.term-empty {
  position: absolute;
  inset: 0;
  z-index: var(--z-base);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
  padding: var(--space-5);
  color: var(--terminal-text, var(--term-text));
  text-align: center;
  pointer-events: none;
}

.term-empty-icon {
  width: 44px;
  height: 44px;
  display: grid;
  place-items: center;
  color: var(--term-muted);
}
.term-empty-icon svg {
  width: 100%;
  height: 100%;
  stroke-width: 1.4;
}

.term-empty-title {
  font-size: 14px;
  font-weight: 500;
  color: var(--terminal-text, var(--term-text));
}

.term-empty-desc {
  max-width: 340px;
  font-size: var(--text-xs);
  color: var(--term-muted);
  line-height: 1.6;
}

.term-empty-runtime {
  display: block;
  margin-top: var(--space-1);
}

.term-empty-actions {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-wrap: wrap;
  gap: 10px;
  margin-top: 2px;
  pointer-events: auto;
}

/* 恒深色画布内局部空态 CTA（03 A4）：与全局 .btn-primary 变更驱动不同，仅保留于本组件 scoped 并对齐 token */
.btn-primary {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 14px;
  border: 0;
  border-radius: var(--radius-sm);
  background: var(--accent);
  color: var(--accent-on);
  font-size: var(--text-xs);
  font-weight: 500;
  cursor: pointer;
  transition: background var(--dur-fast) var(--ease-standard),
    border-color var(--dur-fast) var(--ease-standard),
    color var(--dur-fast) var(--ease-standard);
}
.btn-primary:hover {
  background: var(--accent-hover);
}
.btn-primary:active {
  background: var(--accent-active);
}
.btn-primary.secondary {
  background: transparent;
  color: var(--terminal-text, var(--term-text));
  border: 1px solid var(--term-border);
}
.btn-primary.secondary:hover {
  background: color-mix(in oklab, var(--term-text) 10%, transparent);
  border-color: var(--app-border-strong);
}
.btn-primary.secondary:active {
  background: color-mix(in oklab, var(--term-text) 16%, transparent);
}

.hint-keys {
  color: var(--term-muted);
  font-size: 11px;
}

.term-empty-desc kbd,
.hint-keys kbd {
  color: var(--terminal-text, var(--term-text));
  border-color: var(--term-border);
  background: color-mix(in oklab, var(--term-text) 6%, transparent);
}
</style>
