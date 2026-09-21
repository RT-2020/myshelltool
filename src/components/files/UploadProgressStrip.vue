<script setup lang="ts">
/**
 * UploadProgressStrip — 上传进度提示条（v0.19，上传区域正下方）
 *
 * 位置与形态：FileSurface 网格第三行（file-dual 之下），常驻文档流内联，
 * 不遮挡上传按钮/文件列表；未上传时不渲染（网格 auto 行塌缩为 0）。
 *
 * 数据源：复用 files store 的 transferQueue + `sftp-transfer-progress` 事件
 * （不另起进度通道）。上传批次经 buildTransferItem 的 batchId 聚合——本组件
 * 只跟随「最近发起」的批次（batchId 最大 startedAt），并发旧批次仍可在
 * TransferDrawer（状态栏「传输」胶囊）查看。
 *
 * 交互契约（验收红线）：
 *   - ✕ 关闭 = 只隐藏本批次的提示，**绝不取消上传**（取消是每行独立的
 *     「取消上传」按钮，走 sftp_upload_cancel 旗标通道，两者必须可区分）；
 *   - 点击主体在展开/收起间切换；展开显示文件名/大小/已传/速度/剩余/错误，
 *     失败行带重试（复用 retryTransfer）；
 *   - 总体进度 = 总字节数加权（Σ已传 / Σ总量），完成后强制 100%；
 *   - 全部完成后收起态 8s 自动消失（与 toast 时长同一量级）；失败/取消不
 *     自动消失，等用户处理或关闭。
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import type { Component } from 'vue';
import { storeToRefs } from 'pinia';
import { AlertCircle, CheckCircle2, ChevronDown, Loader2, RotateCcw, X, XCircle } from 'lucide-vue-next';
import { useFilesStore } from '@/stores/files';
import { formatBytes, formatEta, formatSpeed } from '@/lib/transferUtils';
import type { QueueItem } from '@/lib/fileTypes';

/** 完成后收起态自动消失的等待（与 announce 默认 8s toast 同量级）。 */
const COMPLETE_AUTOHIDE_MS = 8000;

const filesStore = useFilesStore();
const { transferQueue } = storeToRefs(filesStore);

const expanded = ref(false);
/** 已被用户关闭的批次（关闭 ≠ 取消：队列项继续跑，仅本组件不再显示该批次）。 */
const dismissedBatchIds = ref(new Set<string>());

const uploadItems = computed(() => transferQueue.value.filter(i => i.direction === 'upload'));
/** 最近发起的批次：按 startedAt 取最大（同一毫秒退化为后来者覆盖，可接受）。 */
const currentBatchId = computed<string | null>(() => {
  let latest: QueueItem | null = null;
  for (const item of uploadItems.value) {
    if (item.batchId && (!latest || item.startedAt >= latest.startedAt)) latest = item;
  }
  return latest?.batchId ?? null;
});
const batchItems = computed(() => {
  const bid = currentBatchId.value;
  return bid ? uploadItems.value.filter(i => i.batchId === bid) : [];
});

const visible = computed(() => {
  const bid = currentBatchId.value;
  return !!bid && batchItems.value.length > 0 && !dismissedBatchIds.value.has(bid);
});

function dismiss() {
  const bid = currentBatchId.value;
  if (!bid) return;
  const next = new Set(dismissedBatchIds.value);
  next.add(bid);
  dismissedBatchIds.value = next;
}

// 新批次开始：默认收起态出现（上一批次的展开状态不继承）。
watch(currentBatchId, () => { expanded.value = false; });

// ---- 批次状态归并（上传中 > 失败 > 取消 > 已完成）----
const phase = computed<'uploading' | 'failed' | 'cancelled' | 'done'>(() => {
  const items = batchItems.value;
  if (items.some(i => i.status === 'running' || i.status === 'pending')) return 'uploading';
  if (items.some(i => i.status === 'error')) return 'failed';
  if (items.some(i => i.status === 'cancelled')) return 'cancelled';
  return 'done';
});
const doneCount = computed(() => batchItems.value.filter(i => i.status === 'done').length);
const phaseMeta = computed<{ icon: Component; label: string }>(() => {
  if (phase.value === 'uploading') return { icon: Loader2, label: '上传中' };
  if (phase.value === 'failed') return { icon: AlertCircle, label: doneCount.value > 0 ? '部分失败' : '上传失败' };
  if (phase.value === 'cancelled') return { icon: XCircle, label: doneCount.value > 0 ? '部分取消' : '已取消' };
  return { icon: CheckCircle2, label: '上传完成' };
});

// ---- 总体进度：总字节数加权（验收要求明示口径）。total 全为 0 的极端批次
// （stat 失败等）无法按字节加权：上传中显示 0，落定后按 100% 处理。 ----
const totalBytes = computed(() => batchItems.value.reduce((sum, i) => sum + (Number(i.total) || 0), 0));
const doneBytes = computed(() =>
  batchItems.value.reduce((sum, i) => sum + Math.min(Number(i.transferred) || 0, Number(i.total) || 0), 0)
);
const overallPercent = computed(() => {
  if (phase.value === 'done') return 100;
  if (totalBytes.value <= 0) return 0;
  return Math.min(100, Math.round((doneBytes.value / totalBytes.value) * 100));
});
const overallSpeed = computed(() =>
  batchItems.value.reduce((sum, i) => (i.status === 'running' ? sum + (i.speed || 0) : sum), 0)
);
const overallEta = computed(() => {
  const rest = totalBytes.value - doneBytes.value;
  return overallSpeed.value > 0 && rest > 0 ? Math.round(rest / overallSpeed.value) : null;
});

const summaryText = computed(() =>
  batchItems.value.length > 1 ? `${batchItems.value.length} 个文件 · 按字节加权` : batchItems.value[0]?.name || ''
);

function rowTone(item: QueueItem): 'uploading' | 'done' | 'failed' | 'cancelled' {
  if (item.status === 'done') return 'done';
  if (item.status === 'error') return 'failed';
  if (item.status === 'cancelled') return 'cancelled';
  return 'uploading';
}
function rowMeta(item: QueueItem): { icon: Component } {
  const tone = rowTone(item);
  if (tone === 'done') return { icon: CheckCircle2 };
  if (tone === 'failed') return { icon: AlertCircle };
  if (tone === 'cancelled') return { icon: XCircle };
  return { icon: Loader2 };
}
const rowSpeedText = (item: QueueItem) => formatSpeed(item.speed);
const rowEtaText = (item: QueueItem) => formatEta(item.eta);

// ---- 完成自动消失：仅收起态计时（用户展开查看时不抢走面板）；失败/取消不自动消失 ----
let hideTimer: ReturnType<typeof setTimeout> | null = null;
const shouldAutoHide = computed(() => visible.value && !expanded.value && phase.value === 'done');
watch(shouldAutoHide, active => {
  if (hideTimer) { clearTimeout(hideTimer); hideTimer = null; }
  if (active) hideTimer = setTimeout(dismiss, COMPLETE_AUTOHIDE_MS);
});
onBeforeUnmount(() => { if (hideTimer) { clearTimeout(hideTimer); hideTimer = null; } });
</script>

<template>
  <aside v-if="visible" class="ups" data-up-progress aria-label="上传进度">
    <div class="ups-head">
      <button
        type="button"
        class="ups-main"
        data-up-progress-toggle
        :aria-expanded="expanded"
        aria-controls="ups-detail"
        :title="expanded ? '点击收起详情' : '点击展开详情'"
        @click="expanded = !expanded"
      >
        <span class="ups-line">
          <span class="ups-pill" :class="`tone-${phase}`">
            <component :is="phaseMeta.icon" :size="12" :class="{ spin: phase === 'uploading' }" />
            {{ phaseMeta.label }}
          </span>
          <strong class="ups-percent" :data-up-progress-percent="overallPercent">{{ overallPercent }}%</strong>
          <span class="ups-summary">{{ summaryText }}</span>
        </span>
        <span
          class="ups-bar"
          role="progressbar"
          :aria-valuenow="overallPercent"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-label="上传总进度（按字节加权）"
        >
          <span class="ups-bar-fill" :class="`tone-${phase}`" :style="{ width: overallPercent + '%' }"></span>
        </span>
      </button>
      <button
        type="button"
        class="ups-action"
        :aria-label="expanded ? '收起上传详情' : '展开上传详情'"
        :title="expanded ? '收起' : '展开详情'"
        @click="expanded = !expanded"
      >
        <ChevronDown :size="14" class="ups-chev" :class="{ open: expanded }" />
      </button>
      <button
        type="button"
        class="ups-action"
        data-up-progress-close
        aria-label="关闭进度提示（不会取消上传）"
        title="关闭提示（不会取消上传）"
        @click="dismiss()"
      >
        <X :size="14" />
      </button>
    </div>

    <div v-show="expanded" id="ups-detail" class="ups-detail">
      <div class="ups-detail-meta">
        共 {{ batchItems.length }} 个文件 · 已传 {{ formatBytes(doneBytes) }} / {{ formatBytes(totalBytes) }}
        <template v-if="overallSpeed > 0"> · {{ formatSpeed(overallSpeed) }}</template>
        <template v-if="overallEta != null"> · 剩余 {{ formatEta(overallEta) }}</template>
      </div>
      <ul class="ups-rows">
        <li v-for="item in batchItems" :key="item.id" class="ups-row" :data-transfer-id="item.id">
          <span class="ups-row-icon" :class="`tone-${rowTone(item)}`">
            <component :is="rowMeta(item).icon" :size="12" :class="{ spin: item.status === 'running' }" />
          </span>
          <div class="ups-row-main">
            <div class="ups-row-top">
              <span class="ups-row-name" :title="item.remotePath">{{ item.name }}</span>
              <span class="ups-row-meta">{{ formatBytes(item.transferred) }} / {{ formatBytes(item.total) }}</span>
            </div>
            <div
              class="ups-bar ups-row-bar"
              role="progressbar"
              :aria-valuenow="item.percent || 0"
              aria-valuemin="0"
              aria-valuemax="100"
              :aria-label="`上传进度：${item.name}`"
            >
              <span class="ups-bar-fill" :class="`tone-${rowTone(item)}`" :style="{ width: (item.percent || 0) + '%' }"></span>
            </div>
            <div class="ups-row-sub">
              <template v-if="item.status === 'running'">
                <span v-if="rowSpeedText(item)">{{ rowSpeedText(item) }}</span>
                <span v-if="rowEtaText(item)">· 剩余 {{ rowEtaText(item) }}</span>
              </template>
              <span v-if="item.error" class="ups-row-error">{{ item.error }}</span>
            </div>
          </div>
          <span class="ups-row-side">
            <!-- 取消上传是独立按钮（旗标通道），与头部「关闭提示」语义严格分离 -->
            <button
              v-if="item.status === 'running' || item.status === 'pending'"
              type="button"
              class="ups-action"
              :aria-label="`取消上传：${item.name}`"
              title="取消上传"
              @click="filesStore.cancelTransfer(item.id)"
            >
              <X :size="12" />
            </button>
            <button
              v-if="item.status === 'error' || item.status === 'cancelled'"
              type="button"
              class="ups-action"
              :aria-label="`重试上传：${item.name}`"
              title="重试"
              @click="filesStore.retryTransfer(item.id)"
            >
              <RotateCcw :size="12" />
            </button>
          </span>
        </li>
      </ul>
    </div>
  </aside>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.ups {
  grid-column: 1 / -1;
  min-width: 0;
  background: var(--app-panel);
  border-top: 1px solid var(--app-border);
}

.ups-head {
  display: flex;
  align-items: stretch;
  gap: 2px;
  padding: 2px 6px;
}
.ups-main {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
  padding: 4px 6px;
  background: transparent;
  border: none;
  border-radius: var(--radius-sm);
  cursor: pointer;
  text-align: left;
  font-family: var(--font-body);
  transition: background var(--dur-fast) var(--ease-standard);
}
.ups-main:hover { background: var(--app-hover); }
.ups-main:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }

.ups-line {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}
.ups-pill {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 1px 8px;
  font-size: var(--text-xs);
  border-radius: var(--radius-pill);
  background: var(--app-panel-2);
  white-space: nowrap;
  flex: 0 0 auto;
}
.ups-pill.tone-uploading { color: var(--accent); }
.ups-pill.tone-done { color: var(--success); }
.ups-pill.tone-failed { color: var(--danger); }
.ups-pill.tone-cancelled { color: var(--warn); }

.ups-percent {
  font-size: var(--text-xs);
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
  color: var(--app-strong);
  flex: 0 0 auto;
}
.ups-summary {
  font-size: var(--text-xs);
  color: var(--app-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.ups-bar {
  display: block;
  width: 100%;
  height: 4px;
  background: var(--app-border-soft);
  border-radius: var(--radius-pill);
  overflow: hidden;
}
.ups-bar-fill {
  display: block;
  height: 100%;
  border-radius: var(--radius-pill);
  background: var(--accent);
  transition: width 0.2s var(--ease-standard), background var(--dur-fast) var(--ease-standard);
}
.ups-bar-fill.tone-done { background: var(--success); }
.ups-bar-fill.tone-failed { background: var(--danger); }
.ups-bar-fill.tone-cancelled { background: var(--warn); }

.ups-action {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  align-self: center;
  width: 24px;
  height: 24px;
  flex: 0 0 auto;
  background: transparent;
  border: none;
  border-radius: var(--radius-sm);
  color: var(--app-muted);
  cursor: pointer;
  transition: background var(--dur-fast) var(--ease-standard), color var(--dur-fast) var(--ease-standard);
}
.ups-action:hover { background: var(--app-hover); color: var(--app-strong); }
.ups-action:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.ups-chev { transition: transform var(--dur-fast) var(--ease-standard); }
.ups-chev.open { transform: rotate(180deg); }

.ups-detail {
  max-height: 220px;
  overflow: auto;
  padding: 2px 12px var(--space-2);
  border-top: 1px solid var(--app-border-soft);
}
.ups-detail-meta {
  padding: var(--space-2) 0 2px;
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}
.ups-rows {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.ups-row {
  display: grid;
  grid-template-columns: 18px 1fr auto;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-1) 2px;
  border-radius: var(--radius-sm);
}
.ups-row-icon { display: inline-flex; color: var(--app-muted); }
.ups-row-icon.tone-uploading { color: var(--accent); }
.ups-row-icon.tone-done { color: var(--success); }
.ups-row-icon.tone-failed { color: var(--danger); }
.ups-row-icon.tone-cancelled { color: var(--warn); }
.ups-row-main { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
.ups-row-top { display: flex; align-items: baseline; justify-content: space-between; gap: var(--space-2); min-width: 0; }
.ups-row-name {
  font-size: var(--text-xs);
  color: var(--app-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
.ups-row-meta {
  font-size: var(--text-xs);
  color: var(--app-muted);
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
  flex: 0 0 auto;
}
.ups-row-bar { height: 3px; }
.ups-row-sub {
  display: flex;
  gap: 4px;
  font-size: var(--text-xs);
  color: var(--app-subtle);
  font-family: var(--font-mono);
}
.ups-row-error { color: var(--danger); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ups-row-side { display: inline-flex; align-items: center; gap: 2px; }

.spin { animation: ups-spin 0.9s linear infinite; }
@keyframes ups-spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
</style>
