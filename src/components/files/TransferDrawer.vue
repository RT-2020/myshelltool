<script setup lang="ts">
/**
 * TransferDrawer — Wave 3 Step 3.4（v2 精简版）
 * 传输队列 sheet：仅保留 Teleport 到 body 的上滑面板。
 *
 * v2 变化：原常驻在 FileSurface 底部的 trigger bar 已删除（占用文件区高度），
 * 触发入口下沉到全局状态栏的「传输」胶囊按钮（AppStatusBar），由它控制
 * transferDrawerOpen。本组件只在全局（App.vue）挂载一次，sheet 仍 Teleport
 * 到 body，与终端/文件区 tab 无关。
 *
 * Store-bound: 订阅 useFilesStore.transferQueue。每行显示方向 / 名称 / 大小比 /
 * 状态药丸 + 细线性进度。
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import type { Component } from 'vue';
import { storeToRefs } from 'pinia';
import { Upload, Download, ChevronDown, AlertCircle, CheckCircle2, Loader2, XCircle, X, RotateCcw, ArrowUpDown } from 'lucide-vue-next';
import { useFilesStore } from '@/stores/files';
import { formatSpeed, formatEta } from '@/lib/transferUtils';
import type { TransferQueueItem } from '@/lib/transferUtils';
import AppTabGroup from '@/components/ui/AppTabGroup.vue';

const props = withDefaults(defineProps<{
  open?: boolean;
}>(), {
  open: false
});
const emit = defineEmits<{ toggle: [] }>();

const filesStore = useFilesStore();
const { transferQueue, completedTransfers, failedTransfers } = storeToRefs(filesStore);

// 分类 tab（FinalShell 式：传输中/队列中/已完成/已失败 带计数）。
// 归类口径与 store getter 对齐：已完成 = done；已失败 = error + cancelled
// （取消是需用户注意的终结态，归入失败 tab 以便找到；药丸文字仍显示「已取消」可区分）。
type TransferTabId = 'running' | 'pending' | 'done' | 'failed';
const activeTab = ref<TransferTabId>('running');
const runningCount = computed(() => transferQueue.value.filter(i => i.status === 'running').length);
const pendingCount = computed(() => transferQueue.value.filter(i => i.status === 'pending').length);
const transferTabs = computed(() => [
  { id: 'running' as const, label: `传输中 ${runningCount.value}` },
  { id: 'pending' as const, label: `队列中 ${pendingCount.value}` },
  { id: 'done' as const, label: `已完成 ${completedTransfers.value.length}` },
  { id: 'failed' as const, label: `已失败 ${failedTransfers.value.length}` }
]);
const filteredQueue = computed(() => {
  const q = transferQueue.value;
  switch (activeTab.value) {
    case 'running': return q.filter(i => i.status === 'running');
    case 'pending': return q.filter(i => i.status === 'pending');
    case 'done': return q.filter(i => i.status === 'done');
    case 'failed': return q.filter(i => i.status === 'error' || i.status === 'cancelled');
  }
});

function formatBytes(bytes: number | null | undefined) {
  const size = Number(bytes) || 0;
  if (size >= 1024 * 1024) return Math.round(size / 1024 / 1024) + ' MB';
  if (size >= 1024) return Math.round(size / 1024) + ' KB';
  return size + ' B';
}

function statusMeta(item: TransferQueueItem): { icon: Component; spin: boolean; label: string; tone: string } {
  if (item.status === 'running') return { icon: Loader2, spin: true, label: '传输中', tone: 'running' };
  if (item.status === 'done') return { icon: CheckCircle2, spin: false, label: '完成', tone: 'done' };
  if (item.status === 'error') return { icon: AlertCircle, spin: false, label: '失败', tone: 'error' };
  if (item.status === 'cancelled') return { icon: XCircle, spin: false, label: '已取消', tone: 'cancelled' };
  return { icon: Loader2, spin: false, label: '排队', tone: 'pending' };
}

// 状态药丸文案：错误/取消显示文字而非百分比（失败项百分比无意义）
function pillLabel(item: TransferQueueItem) {
  if (item.status === 'error') return '失败';
  if (item.status === 'cancelled') return '已取消';
  return (item.percent || 0) + '%';
}

// 抽屉打开时注册 Escape 关闭，关闭/卸载时移除；打开瞬间做一次智能默认 tab
// （有传输看传输，无传输看队列，否则看完成——只定初始值，不抢用户后续切换）
function onDrawerKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') emit('toggle');
}
watch(() => props.open, open => {
  if (open) {
    window.addEventListener('keydown', onDrawerKeydown);
    activeTab.value = runningCount.value > 0 ? 'running' : pendingCount.value > 0 ? 'pending' : 'done';
  } else {
    window.removeEventListener('keydown', onDrawerKeydown);
  }
});
onBeforeUnmount(() => window.removeEventListener('keydown', onDrawerKeydown));
</script>

<template>
  <Teleport to="body">
    <Transition name="transfer-backdrop">
      <div
        v-if="open"
        class="transfer-backdrop"
        aria-hidden="true"
        @click="emit('toggle')"
      ></div>
    </Transition>
    <Transition name="transfer-sheet">
      <div v-if="open" class="transfer-sheet" role="dialog" aria-label="传输队列">
        <div class="transfer-sheet-head">
          <div class="transfer-sheet-title">
            <strong>传输队列</strong>
          </div>
          <button
            class="transfer-sheet-close"
            type="button"
            title="收起"
            @click="emit('toggle')"
          >
            <ChevronDown :size="16" />
          </button>
        </div>

        <!-- 分类 tab（FinalShell 式表头）：计数随队列实时刷新，列表按 tab 过滤 -->
        <AppTabGroup
          class="transfer-tabs"
          :tabs="transferTabs"
          :active="activeTab"
          @update:active="id => (activeTab = id as TransferTabId)"
        />

        <div class="transfer-sheet-body" aria-live="polite">
          <div v-if="!filteredQueue.length" class="transfer-empty">
            <ArrowUpDown :size="20" class="transfer-empty-icon" />
            <span>{{ transferQueue.length ? '该分类暂无任务' : '暂无传输任务' }}</span>
          </div>
          <ul v-else class="transfer-list">
            <li
              v-for="item in filteredQueue"
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
                <!-- 速度 + 剩余时间（S2：瞬时速度与 ETA 来自队列项） -->
                <div v-if="item.status === 'running'" class="transfer-row-speed">
                  <span v-if="item.speed > 0">{{ formatSpeed(item.speed) }}</span>
                  <span v-if="item.eta != null && item.eta > 0">· 剩余 {{ formatEta(item.eta) }}</span>
                </div>
                <div v-if="item.error" class="transfer-row-error">{{ item.error }}</div>
              </div>
              <span class="transfer-row-side">
                <span class="transfer-row-pill" :class="`tone-${statusMeta(item).tone}`">
                  <component
                    :is="statusMeta(item).icon"
                    :size="12"
                    :class="{ spin: statusMeta(item).spin }"
                  />
                  <span>{{ pillLabel(item) }}</span>
                </span>
                <!-- 取消：仅上传可分块取消（下载为整块 invoke，后端无中断通道，不渲染按钮） -->
                <button
                  v-if="item.direction === 'upload' && (item.status === 'running' || item.status === 'pending')"
                  class="transfer-row-action"
                  type="button"
                  title="取消上传"
                  aria-label="取消上传"
                  @click="filesStore.cancelTransfer(item.id)"
                >
                  <X :size="12" />
                </button>
                <!-- 重试：失败行复用原始参数重新入队执行 -->
                <button
                  v-if="item.status === 'error'"
                  class="transfer-row-action"
                  type="button"
                  title="重试"
                  aria-label="重试"
                  @click="filesStore.retryTransfer(item.id)"
                >
                  <RotateCcw :size="12" />
                </button>
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

// Backdrop overlay
.transfer-backdrop {
  position: fixed;
  inset: 0;
  z-index: calc(var(--z-drawer) - 1);
  background: var(--app-scrim);
  backdrop-filter: blur(2px);
}
.transfer-backdrop-enter-active,
.transfer-backdrop-leave-active {
  transition: opacity var(--motion-base) var(--ease-standard);
}
.transfer-backdrop-enter-from,
.transfer-backdrop-leave-to {
  opacity: 0;
}

// Slide-up sheet (teleported to body). Fixed bottom dock; no nested card chrome.
// trigger bar 已删除（触发入口在状态栏「传输」胶囊按钮）。
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
  border-radius: var(--radius-md) var(--radius-md) 0 0;
  box-shadow: var(--shadow-sheet);
}

.transfer-sheet-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-2) var(--space-3);
}
// 分类 tab 条：左右留白与 head 对齐（AppTabGroup 自带底部分隔线，head 不再画线防双线）
.transfer-tabs {
  margin: 0 var(--space-3);
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
.transfer-sheet-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  background: transparent;
  border: none;
  color: var(--app-muted);
  cursor: pointer;
  border-radius: var(--radius-sm);
  transition: background var(--motion-fast) var(--ease-standard), color var(--motion-fast) var(--ease-standard);
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
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  margin: 0;
  padding: var(--space-8) var(--space-4);
  text-align: center;
  color: var(--app-subtle);
  font-size: var(--text-xs);
}
.transfer-empty-icon {
  opacity: 0.6;
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
  gap: 6px;
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
  font-variant-numeric: tabular-nums;
  flex: 0 0 auto;
}

.transfer-row-bar {
  width: 100%;
  height: 4px;
  background: var(--app-border-soft);
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

// S2：速度 / ETA 行（弱化，与 meta 区分）
.transfer-row-speed {
  display: inline-flex;
  align-items: baseline;
  gap: 4px;
  font-size: var(--text-xs);
  color: var(--app-subtle);
  font-family: var(--font-mono);
}

// S2：右侧操作区（状态药丸 + 取消/重试按钮）
.transfer-row-side {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  flex: 0 0 auto;
}
.transfer-row-action {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  background: transparent;
  border: none;
  border-radius: var(--radius-sm);
  color: var(--app-muted);
  cursor: pointer;
  transition: background var(--motion-fast) var(--ease-standard), color var(--motion-fast) var(--ease-standard);
}
.transfer-row-action:hover {
  background: var(--app-hover);
  color: var(--app-strong);
}
.transfer-row.is-error .transfer-row-action:hover {
  color: var(--accent);
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
.transfer-row-pill.tone-cancelled { color: var(--warn); }
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
