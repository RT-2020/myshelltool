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

const pauseBtnTitle = computed(() => {
  if (!sessions.activeSessionId) return '连接后可用';
  return rm.enabled ? '暂停采样' : '继续采样';
});

const exportBtnTitle = computed(() => (rm.snapshot ? '导出快照' : '连接后可用'));
const exportBtnAria = computed(() =>
  rm.snapshot ? '导出资源快照到剪贴板' : '无采样数据，连接后可用'
);

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
      </div>

      <div class="rs-header-actions">
        <button
          class="icon-btn"
          type="button"
          :title="pauseBtnTitle"
          :aria-label="pauseBtnTitle"
          :disabled="!sessions.activeSessionId"
          @click="onPauseToggle"
        >
          <Pause v-if="rm.enabled" :size="14" />
          <Play v-else :size="14" />
        </button>
        <button
          class="icon-btn"
          type="button"
          :title="exportBtnTitle"
          :aria-label="exportBtnAria"
          :disabled="!rm.snapshot"
          @click="onExport"
        >
          <Download :size="14" />
        </button>
        <button class="icon-btn" type="button" title="收起右侧栏" aria-label="收起右侧栏" @click="emit('collapse')">
          <PanelRightClose :size="14" />
        </button>
      </div>
    </header>

    <div class="rs-body">
      <ResourceMonitorPanel />
      <OpsSummaryPanel />
    </div>
  </aside>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.right-sidebar {
  grid-area: right;
  display: grid;
  grid-template-rows: auto 1fr;
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
  // 高度与终端标签条（.region-terminal 38px）、左栏 sb-header 对齐，三列头部横线共线
  height: 38px;
  padding: 0 var(--space-3);
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
  font-family: var(--font-display);
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.04em;
  color: var(--app-muted);
  white-space: nowrap;
}

.rs-header-actions {
  display: flex;
  gap: 2px;
  flex-shrink: 0;
}

// icon-btn 全局类（_utilities.scss）；仅保留本组件 24×24 紧凑尺寸的局部覆盖
.rs-header-actions .icon-btn {
  width: 24px;
  height: 24px;
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
</style>

