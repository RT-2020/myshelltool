<script setup>
/**
 * TransferDrawer — Wave 3 Step 3.4
 * Bottom drawer showing transfer queue with per-item progress (Tabby-style).
 * Store-bound: subscribes to useFilesStore.transferQueue + transferDrawerOpen.
 *
 * Uses AppDrawer (side="bottom") for the slide-up sheet, plus AppProgress for
 * the aggregate progress bar. Each row shows direction / name / size ratio /
 * status pill with a thin linear progress fill. Compact, low visual weight.
 */
import { computed } from 'vue';
import { storeToRefs } from 'pinia';
import { Upload, Download, ChevronUp, ChevronDown, AlertCircle, CheckCircle2, Loader2 } from 'lucide-vue-next';
import { useFilesStore } from '@/stores/files.js';
import AppDrawer from '@/components/ui/AppDrawer.vue';

const props = defineProps({
  open: { type: Boolean, default: false }
});
const emit = defineEmits(['toggle']);

const filesStore = useFilesStore();
const { transferQueue, activeTransfers, completedTransfers } = storeToRefs(filesStore);

const hasItems = computed(() => transferQueue.value.length > 0);

// Aggregate percent across running items only; done items count as 100.
const overallPercent = computed(() => {
  const list = transferQueue.value;
  if (!list.length) return 0;
  let sum = 0;
  for (const item of list) {
    if (item.status === 'done') sum += 100;
    else if (item.status === 'error') sum += 100;
    else sum += item.percent || 0;
  }
  return Math.round(sum / list.length);
});

function formatBytes(bytes) {
  const size = Number(bytes) || 0;
  if (size >= 1024 * 1024) return Math.round(size / 1024 / 1024) + ' MB';
  if (size >= 1024) return Math.round(size / 1024) + ' KB';
  return size + ' B';
}

function statusMeta(item) {
  if (item.status === 'running') return { icon: Loader2, spin: true, label: '传输中', tone: 'running' };
  if (item.status === 'done') return { icon: CheckCircle2, spin: false, label: '完成', tone: 'done' };
  if (item.status === 'error') return { icon: AlertCircle, spin: false, label: '失败', tone: 'error' };
  return { icon: Loader2, spin: false, label: '排队', tone: 'pending' };
}
</script>

<template>
  <!-- Trigger bar: always visible; click toggles the slide-up sheet -->
  <header class="transfer-trigger" @click="emit('toggle')">
    <div class="transfer-trigger-title">
      <strong>传输队列</strong>
      <span class="transfer-trigger-sub">
        {{ activeTransfers.length }} 进行中 · {{ completedTransfers.length }} 完成
      </span>
    </div>
    <div class="transfer-trigger-actions">
      <div class="transfer-trigger-bar" v-if="hasItems">
        <div class="transfer-trigger-bar-fill" :style="{ width: overallPercent + '%' }"></div>
      </div>
      <button
        class="transfer-trigger-toggle"
        type="button"
        :title="open ? '收起' : '展开'"
        :aria-expanded="String(open)"
        @click.stop="emit('toggle')"
      >
        <component :is="open ? ChevronDown : ChevronUp" :size="14" />
      </button>
    </div>
  </header>

  <Teleport to="body">
    <Transition name="transfer-sheet">
      <div v-if="open" class="transfer-sheet" role="dialog" aria-label="传输队列">
        <div class="transfer-sheet-head">
          <div class="transfer-sheet-title">
            <strong>传输队列</strong>
            <span class="transfer-sheet-sub">
              {{ activeTransfers.length }} 进行中 · {{ completedTransfers.length }} 完成 ·
              {{ transferQueue.length }} 总计
            </span>
          </div>
          <button
            class="transfer-sheet-close"
            type="button"
            title="收起"
            @click="emit('toggle')"
          >
            <ChevronDown :size="14" />
          </button>
        </div>

        <div class="transfer-sheet-body">
          <p v-if="!hasItems" class="transfer-empty">暂无传输任务</p>
          <ul v-else class="transfer-list">
            <li
              v-for="item in transferQueue"
              :key="item.id"
              class="transfer-row"
              :class="`is-${item.status}`"
              :data-transfer-id="item.id"
            >
              <span class="transfer-row-icon" :class="`dir-${item.direction}`">
                <component :is="item.direction === 'upload' ? Upload : Download" :size="14" />
              </span>
              <div class="transfer-row-main">
                <div class="transfer-row-top">
                  <strong class="transfer-row-name" :title="item.name">{{ item.name }}</strong>
                  <span class="transfer-row-meta">
                    {{ formatBytes(item.transferred) }} / {{ formatBytes(item.total) }}
                  </span>
                </div>
                <div class="transfer-row-bar">
                  <div
                    class="transfer-row-bar-fill"
                    :class="`is-${item.status}`"
                    :style="{ width: (item.percent || 0) + '%' }"
                  ></div>
                </div>
                <div v-if="item.error" class="transfer-row-error">{{ item.error }}</div>
              </div>
              <span class="transfer-row-pill" :class="`tone-${statusMeta(item).tone}`">
                <component
                  :is="statusMeta(item).icon"
                  :size="12"
                  :class="{ spin: statusMeta(item).spin }"
                />
                <span>{{ item.percent || 0 }}%</span>
              </span>
            </li>
          </ul>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// Trigger bar: thin sticky footer; click anywhere toggles the sheet.
.transfer-trigger {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  background: var(--app-panel-2);
  border-block-start: 1px solid var(--app-border);
  cursor: pointer;
  user-select: none;
  font-size: var(--text-xs);
  color: var(--app-text);
  transition: background var(--motion-fast) var(--ease-standard);
}
.transfer-trigger:hover {
  background: var(--app-hover);
}

.transfer-trigger-title {
  display: inline-flex;
  align-items: baseline;
  gap: var(--space-2);
  min-width: 0;
}
.transfer-trigger-title strong {
  font-size: var(--text-xs);
  font-weight: 600;
}
.transfer-trigger-sub {
  color: var(--app-muted);
}

.transfer-trigger-actions {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
}

// Inline mini progress bar inside the trigger.
.transfer-trigger-bar {
  width: 120px;
  height: 3px;
  background: var(--app-panel);
  border-radius: var(--radius-pill);
  overflow: hidden;
}
.transfer-trigger-bar-fill {
  height: 100%;
  background: var(--accent);
  border-radius: var(--radius-pill);
  transition: width 0.2s ease;
}

.transfer-trigger-toggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  background: transparent;
  border: none;
  color: var(--app-muted);
  cursor: pointer;
  border-radius: var(--radius-sm);
  transition: background var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard);
}
.transfer-trigger-toggle:hover {
  background: var(--app-hover);
  color: var(--app-strong);
}

// Slide-up sheet (teleported). Fixed bottom dock; no nested card chrome.
.transfer-sheet {
  position: fixed;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: var(--z-drawer);
  display: flex;
  flex-direction: column;
  max-height: 50vh;
  background: var(--app-panel);
  border-block-start: 1px solid var(--app-border);
  box-shadow: var(--app-shadow);
}

.transfer-sheet-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-2) var(--space-3);
  border-block-end: 1px solid var(--app-border);
}
.transfer-sheet-title {
  display: inline-flex;
  align-items: baseline;
  gap: var(--space-2);
  min-width: 0;
}
.transfer-sheet-title strong {
  font-size: var(--text-sm);
  font-weight: 600;
}
.transfer-sheet-sub {
  font-size: var(--text-xs);
  color: var(--app-muted);
}
.transfer-sheet-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  background: transparent;
  border: none;
  color: var(--app-muted);
  cursor: pointer;
  border-radius: var(--radius-sm);
}
.transfer-sheet-close:hover {
  background: var(--app-hover);
  color: var(--app-strong);
}

.transfer-sheet-body {
  flex: 1 1 auto;
  overflow: auto;
  padding: var(--space-2) var(--space-3);
}

.transfer-empty {
  margin: 0;
  padding: var(--space-4);
  text-align: center;
  color: var(--app-muted);
  font-size: var(--text-sm);
}

.transfer-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.transfer-row {
  display: grid;
  grid-template-columns: 20px 1fr auto;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2);
  background: transparent;
  border-radius: var(--radius-sm);
  transition: background var(--motion-fast) var(--ease-standard);
}
.transfer-row:hover {
  background: var(--app-hover);
}

.transfer-row-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--app-muted);
}
.transfer-row-icon.dir-upload { color: var(--accent); }
.transfer-row-icon.dir-download { color: var(--success); }

.transfer-row-main {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.transfer-row-top {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-2);
  min-width: 0;
}
.transfer-row-name {
  font-size: var(--text-sm);
  font-weight: 500;
  color: var(--app-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
.transfer-row-meta {
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
  flex: 0 0 auto;
}

.transfer-row-bar {
  width: 100%;
  height: 3px;
  background: var(--app-panel-2);
  border-radius: var(--radius-pill);
  overflow: hidden;
}
.transfer-row-bar-fill {
  height: 100%;
  border-radius: var(--radius-pill);
  background: var(--accent);
  transition: width 0.2s ease, background var(--motion-fast) var(--ease-standard);
}
.transfer-row-bar-fill.is-done { background: var(--success); }
.transfer-row-bar-fill.is-error { background: var(--danger); }

.transfer-row-error {
  font-size: var(--text-xs);
  color: var(--danger);
}

.transfer-row-pill {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  font-size: var(--text-xs);
  border-radius: var(--radius-pill);
  background: var(--app-panel-2);
  color: var(--app-muted);
  white-space: nowrap;
}
.transfer-row-pill.tone-running { color: var(--accent); }
.transfer-row-pill.tone-done { color: var(--success); }
.transfer-row-pill.tone-error { color: var(--danger); }
.transfer-row-pill.tone-pending { color: var(--app-muted); }

.spin {
  animation: transfer-spin 0.9s linear infinite;
}
@keyframes transfer-spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

// Sheet slide-up transition.
.transfer-sheet-enter-active,
.transfer-sheet-leave-active {
  transition: transform var(--motion-base) var(--ease-standard),
    opacity var(--motion-base) var(--ease-standard);
}
.transfer-sheet-enter-from,
.transfer-sheet-leave-to {
  opacity: 0;
  transform: translateY(100%);
}
</style>
