<script setup lang="ts">
/**
 * SyncConflictResolver — v1.6 冲突解决子组件。
 *
 * 在 SyncPanelContent 检测到 syncConflict（pull 返回 Conflict）时渲染（独占面板）。
 * 抽成独立组件因 SyncPanelContent.vue 超 500 行 SFC 硬上限（AGENTS.md 红线），
 * 参照 SyncAutoSyncControl / McpCapabilityList 的拆分先例。
 *
 * 单一职责：展示本地/远端摘要 + 让用户选择保留哪一方。
 * 子组件自管主密码输入（与父组件的 opPassword 解耦），直接调 store.syncResolveConflict。
 *
 * 视觉语言照父组件的 block 范式，零新增 token（AGENTS.md 红线：用 var(--token)）。
 */
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { AlertTriangle } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';

const store = useWorkbenchStore();
const { syncConflict, syncLoading, syncAutoSyncEnabled } = storeToRefs(store);

// 子组件自管主密码（v1.6：启用自动同步后可留空）
const opPassword = ref('');

async function onResolve(choice: string) {
  // 冲突解决后的资产列表重载在 store.resolveConflict 内部完成（后端已写盘，
  // 选「远端覆盖本地」时列表必须跟着换）。
  await store.syncResolveConflict(opPassword.value, choice);
  opPassword.value = '';
}

function onDismiss() {
  store.syncDismissConflict();
}

interface ParsedSnapshot {
  assets?: Array<{ id?: string } & Record<string, unknown>>;
  groups?: unknown[];
}

function parseSnapshot(jsonStr?: string): ParsedSnapshot | null {
  try {
    return JSON.parse(jsonStr ?? '') as ParsedSnapshot;
  } catch {
    return null;
  }
}

// 资产 JSON 摘要（冲突框展示用）
function assetSummary(jsonStr?: string) {
  const data = parseSnapshot(jsonStr);
  if (!data) return '（无法解析）';
  const count = data.assets?.length ?? 0;
  const groups = data.groups?.length ?? 0;
  return `${count} 个连接 · ${groups} 个分组`;
}

/** 按资产完整 id 对比两侧快照，避免仅靠总数相同掩盖内容差异（08 #7）。 */
const diffSummaryText = computed(() => {
  const conflict = syncConflict.value;
  if (!conflict) return '';
  const localData = parseSnapshot(conflict.localJson);
  const remoteData = parseSnapshot(conflict.remoteJson);
  if (!localData || !remoteData) return '无法解析资产差异详情，请谨慎选择保留方。';
  const localById = new Map<string, string>();
  const remoteById = new Map<string, string>();
  for (const item of localData.assets || []) {
    if (item && typeof item.id === 'string' && item.id) {
      localById.set(item.id, JSON.stringify(item));
    }
  }
  for (const item of remoteData.assets || []) {
    if (item && typeof item.id === 'string' && item.id) {
      remoteById.set(item.id, JSON.stringify(item));
    }
  }
  let localOnly = 0;
  let remoteOnly = 0;
  let changed = 0;
  for (const [id, raw] of localById) {
    const rRaw = remoteById.get(id);
    if (!rRaw) localOnly += 1;
    else if (rRaw !== raw) changed += 1;
  }
  for (const id of remoteById.keys()) {
    if (!localById.has(id)) remoteOnly += 1;
  }
  if (localOnly === 0 && remoteOnly === 0 && changed === 0) {
    return '连接条目 id 与内容一致，仅分组顺序或时间戳存在差异。';
  }
  return `差异对比：仅本地 ${localOnly} 个 · 仅远端 ${remoteOnly} 个 · 同 id 内容不同 ${changed} 个`;
});
</script>

<template>
  <section v-if="syncConflict" class="block conflict-block">
    <header class="block-head conflict-head">
      <AlertTriangle :size="12" />检测到同步冲突
    </header>
    <p class="block-note muted">
      本地和远端都有变更，请选择保留哪一份。<strong>未选中的一方将被永久覆盖。</strong>
    </p>
    <div class="conflict-options">
      <div class="option-card">
        <span class="option-label">本地</span>
        <span class="option-summary">{{ assetSummary(syncConflict.localJson) }}</span>
      </div>
      <div class="option-card">
        <span class="option-label">远端（Gist）</span>
        <span class="option-summary">{{ assetSummary(syncConflict.remoteJson) }}</span>
      </div>
    </div>
    <p class="diff-summary num">{{ diffSummaryText }}</p>
    <label class="field">
      <span class="field-label">
        主密码（用于加密推送）<span v-if="syncAutoSyncEnabled" class="muted">（自动同步已启用，可留空）</span>
      </span>
      <AppInput v-model="opPassword" type="password" placeholder="输入主密码" />
    </label>
    <div class="actions">
      <AppButton variant="danger" size="sm"
        :disabled="(!opPassword && !syncAutoSyncEnabled) || syncLoading"
        @click="onResolve('local')">用本地覆盖远端</AppButton>
      <AppButton variant="danger" size="sm"
        :disabled="(!opPassword && !syncAutoSyncEnabled) || syncLoading"
        @click="onResolve('remote')">用远端覆盖本地</AppButton>
      <AppButton variant="ghost" size="sm" @click="onDismiss">取消</AppButton>
    </div>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// .block / .block-head 基础样式由全局 _utilities.scss 提供（07 A2），本处仅保留冲突语义微调
.conflict-block { border-color: color-mix(in oklab, var(--danger), transparent 55%); }
.conflict-head {
  color: var(--danger);
  padding-bottom: 4px;
  border-bottom: 1px solid color-mix(in oklab, var(--danger), transparent 65%);
}
.block-head :deep(svg) { flex-shrink: 0; }
.block-note {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.6;
}
.block-note strong { color: var(--app-strong); }

.conflict-options {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-2);
}
.option-card {
  padding: var(--space-2) var(--space-3);
  background: var(--app-panel-2);
  border-radius: var(--radius-sm);
  border: 1px solid var(--app-border);
}
.option-label { display: block; font-size: 10px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--app-muted); }
.option-summary { display: block; font-size: var(--text-xs); margin-top: 2px; color: var(--app-strong); }
.diff-summary {
  margin: 0;
  font-size: var(--text-xs);
  color: var(--app-muted);
}

// ─── 通用字段 ───
.field { display: flex; flex-direction: column; gap: 4px; }
.field-label { font-size: var(--text-xs); color: var(--app-muted); }
.actions { display: flex; gap: var(--space-2); flex-wrap: wrap; align-items: center; }

.muted { color: var(--app-muted); font-size: var(--text-xs); line-height: 1.5; margin: 0; }
</style>
