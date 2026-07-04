<script setup>
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { TerminalSquare, PlugZap } from 'lucide-vue-next';

const props = defineProps({
  store: { type: Object, required: true },
  hasActiveSession: { type: Boolean, default: false },
  isTauriCore: { type: Boolean, default: false },
  selectedAsset: { type: Object, default: null }
});

const emit = defineEmits(['connect-selected', 'open-asset-editor', 'context-menu']);

const mountRef = ref(null);

function setContainer() {
  if (mountRef.value && props.store?.setTerminalContainer) {
    props.store.setTerminalContainer(mountRef.value);
  }
}

onMounted(setContainer);
onBeforeUnmount(() => {
  // Keep the store container reference stable across short-lived tab remounts.
});

function onWheel(e) {
  if (!e.ctrlKey) return;
  e.preventDefault();
  if (e.deltaY < 0) props.store?.runTerminalAction?.('font-inc');
  if (e.deltaY > 0) props.store?.runTerminalAction?.('font-dec');
}

function onContextMenu(event) {
  if (!props.hasActiveSession) return;
  const hasSelection = Boolean(props.store?.activeSession?.term?.getSelection());
  emit('context-menu', { x: event.clientX, y: event.clientY, hasSelection });
}
</script>

<template>
  <div class="terminal-pane-host" :class="{ 'has-session': hasActiveSession }" @wheel="onWheel" @contextmenu.prevent="onContextMenu">
    <div id="terminalContainer" ref="mountRef" aria-label="终端区域"></div>

    <div v-if="!hasActiveSession" class="term-empty">
      <div class="term-empty-icon" aria-hidden="true">
        <TerminalSquare :size="42" />
      </div>
      <div class="term-empty-title">暂无活跃会话</div>
      <div class="term-empty-desc">
        从左侧选择连接资产，或在底部输入 <kbd>ssh user@host</kbd> 快速打开会话。
      </div>
      <div class="term-empty-actions">
        <button v-if="isTauriCore && selectedAsset" class="btn-primary" type="button" @click="emit('connect-selected')">
          <PlugZap :size="14" />连接 {{ selectedAsset.name }}
        </button>
        <button v-if="isTauriCore" class="btn-primary secondary" type="button" @click="emit('open-asset-editor')">新增连接</button>
        <button v-else class="btn-primary" type="button" @click="emit('open-asset-editor')">打开新会话</button>
        <span class="hint-keys"><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd> 新建标签</span>
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
  z-index: 0;
  height: 100%;
  background: var(--terminal-bg);
  overflow: hidden;
}

.term-empty {
  position: absolute;
  inset: 0;
  z-index: 2;
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
  color: var(--term-muted, rgba(255, 255, 255, 0.45));
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
  font-size: 12px;
  color: var(--term-muted, rgba(255, 255, 255, 0.45));
  line-height: 1.6;
}

.term-empty-desc kbd {
  color: var(--terminal-text, var(--term-text));
  border-color: var(--term-border);
  background: rgba(255, 255, 255, 0.06);
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

.btn-primary {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 14px;
  border: 0;
  border-radius: 7px;
  background: var(--accent);
  color: var(--accent-on);
  font-size: 12.5px;
  font-weight: 500;
}
.btn-primary.secondary {
  background: transparent;
  color: var(--terminal-text, var(--term-text));
  border: 1px solid var(--term-border);
}

.hint-keys {
  color: var(--term-muted, rgba(255, 255, 255, 0.45));
  font-size: 11px;
}
.hint-keys kbd {
  color: var(--terminal-text, var(--term-text));
  border-color: var(--term-border);
  background: rgba(255, 255, 255, 0.06);
}
</style>
