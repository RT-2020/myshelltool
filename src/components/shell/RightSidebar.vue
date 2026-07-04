<script setup>
import { computed } from 'vue';
import { Download, PanelRightClose, Pause, Play } from 'lucide-vue-next';
import ResourceMonitorPanel from '@/components/resource-monitor/ResourceMonitorPanel.vue';
import OpsSummaryPanel from './OpsSummaryPanel.vue';
import { useResourceMonitorStore } from '@/stores/resourceMonitor.js';
import { useSessionsStore } from '@/stores/sessions.js';
import { useWorkbenchStore } from '@/stores/workbench.js';
import { useClipboard } from '@/composables/useClipboard.js';

const emit = defineEmits(['collapse']);

const rm = useResourceMonitorStore();
const sessions = useSessionsStore();
const workbench = useWorkbenchStore();
const clipboard = useClipboard();

const connState = computed(() => {
  if (!rm.isDesktopRuntime) return { dot: 'idle', text: '浏览器预览' };
  if (!sessions.activeSessionId) return { dot: 'idle', text: '空闲' };
  if (!rm.snapshot) return { dot: 'connecting', text: '采样中' };
  return { dot: 'connected', text: '已连接' };
});

const pauseBtnTitle = computed(() => (rm.enabled ? '暂停采样' : '继续采样'));

async function onPauseToggle() {
  if (rm.enabled) {
    await rm.stop().catch(() => {});
    workbench.announce('已暂停资源采样');
    return;
  }

  if (sessions.activeSessionId) {
    await rm.start(sessions.activeSessionId).catch(() => {});
    workbench.announce('已恢复资源采样');
  }
}

async function onExport() {
  if (!rm.snapshot) return;

  const payload = {
    exportedAt: new Date().toISOString(),
    sessionId: sessions.activeSessionId,
    snapshot: rm.snapshot,
    history: rm.history
  };

  const ok = await clipboard.copy(JSON.stringify(payload, null, 2));
  workbench.announce(ok ? '资源快照已复制到剪贴板' : '资源快照复制失败，请重试');
}
</script>

<template>
  <aside class="right-sidebar" data-region="right" aria-label="监控侧边栏">
    <header class="rs-header">
      <div class="rs-header-left">
        <span class="rs-title">监控</span>
        <span class="rs-status-pill" :data-state="connState.dot">
          <span class="conn-dot" :class="connState.dot"></span>
          {{ connState.text }}
        </span>
      </div>

      <div class="rs-header-actions">
        <button class="icon-btn" type="button" :title="pauseBtnTitle" :aria-label="pauseBtnTitle" @click="onPauseToggle">
          <Pause v-if="rm.enabled" />
          <Play v-else />
        </button>
        <button
          class="icon-btn"
          type="button"
          title="导出快照"
          aria-label="导出资源快照到剪贴板"
          :disabled="!rm.snapshot"
          @click="onExport"
        >
          <Download />
        </button>
        <button class="icon-btn" type="button" title="收起右侧栏" aria-label="收起右侧栏" @click="emit('collapse')">
          <PanelRightClose />
        </button>
      </div>
    </header>

    <div class="rs-body">
      <ResourceMonitorPanel />
      <OpsSummaryPanel />
    </div>

    <footer class="rs-footer">
      <span class="rs-footer-meta">采样 2 秒 · 历史 60 点</span>
      <button class="collapse-btn" type="button" @click="emit('collapse')">
        <PanelRightClose />
        <span>收起</span>
      </button>
    </footer>
  </aside>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.right-sidebar {
  grid-area: right;
  display: grid;
  grid-template-rows: auto 1fr auto;
  min-width: 0;
  min-height: 0;
  height: 100%;
  overflow: hidden;
  background: var(--app-panel);
  border-left: 1px solid var(--app-border);
}

.rs-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  padding: var(--space-3);
  border-bottom: 1px solid var(--app-border-soft);
  background: var(--app-panel);
}

.rs-header-left {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}

.rs-title {
  font-family: var(--font-mono);
  font-size: 10.5px;
  font-weight: 500;
  letter-spacing: 0.08em;
  color: var(--app-muted);
  white-space: nowrap;
}

.rs-status-pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  min-width: 0;
  padding: 2px 8px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-pill);
  background: var(--app-panel-2);
  font: 11px var(--font-mono);
  color: var(--app-muted);
}

.conn-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--app-subtle);
  flex-shrink: 0;
}

.conn-dot.connecting {
  background: var(--warn);
  animation: rs-dot-pulse var(--motion-base) infinite alternate cubic-bezier(.4, 0, .2, 1);
}

.conn-dot.connected { background: var(--success); }

.rs-header-actions {
  display: flex;
  gap: 2px;
  flex-shrink: 0;
}

.icon-btn {
  width: 24px;
  height: 24px;
  display: inline-grid;
  place-items: center;
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--app-muted);
  transition: background var(--motion-fast), color var(--motion-fast);
}

.icon-btn svg {
  width: 14px;
  height: 14px;
  stroke-width: 1.6;
}

.icon-btn:hover:not(:disabled) {
  background: var(--app-hover);
  color: var(--app-text);
}

.icon-btn:focus-visible,
.collapse-btn:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

.icon-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
  pointer-events: none;
}

.rs-body {
  min-height: 0;
  overflow-y: auto;
  scrollbar-width: thin;
  scrollbar-color: var(--app-border-strong) transparent;
}

.rs-body::-webkit-scrollbar { width: 8px; }
.rs-body::-webkit-scrollbar-thumb {
  background: var(--app-border);
  border: 2px solid var(--app-panel);
  border-radius: 4px;
}
.rs-body::-webkit-scrollbar-thumb:hover { background: var(--app-border-strong); }

.rs-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border-top: 1px solid var(--app-border-soft);
  background: var(--app-panel);
}

.rs-footer-meta {
  overflow: hidden;
  color: var(--app-subtle);
  font: 10px var(--font-mono);
  letter-spacing: 0.04em;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.collapse-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  height: 24px;
  padding: 0 8px;
  border: 1px solid var(--app-border);
  border-radius: 6px;
  color: var(--app-muted);
  font: 11px var(--font-display);
}

.collapse-btn svg {
  width: 13px;
  height: 13px;
  stroke-width: 1.7;
}

.collapse-btn:hover {
  background: var(--app-hover);
  color: var(--app-text);
}

@keyframes rs-dot-pulse {
  from { opacity: 0.5; transform: scale(0.9); }
  to { opacity: 1; transform: scale(1.1); }
}
</style>
