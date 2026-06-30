<script setup>
/**
 * SyncConflictResolver — v1.6 冲突解决子组件。
 *
 * 在 SyncPanelContent 检测到 syncConflict（pull 返回 Conflict）时渲染（独占面板）。
 * 抽成独立组件因 SyncPanelContent.vue 超 500 行 SFC 硬上限（AGENTS.md 红线），
 * 参照 SyncPatGuide / SyncAutoSyncControl 的拆分先例。
 *
 * 单一职责：展示本地/远端摘要 + 让用户选择保留哪一方。
 * 子组件自管主密码输入（与父组件的 opPassword 解耦），直接调 store.syncResolveConflict。
 *
 * 视觉语言照父组件的 block 范式，零新增 token（AGENTS.md 红线：用 var(--token)）。
 */
import { ref } from 'vue';
import { storeToRefs } from 'pinia';
import { AlertTriangle } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench.js';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';

const store = useWorkbenchStore();
const { syncConflict, syncLoading, syncAutoSyncEnabled } = storeToRefs(store);

// 子组件自管主密码（v1.6：启用自动同步后可留空）
const opPassword = ref('');

async function onResolve(choice) {
  await store.syncResolveConflict(opPassword.value, choice);
  await store.listAssets();
  opPassword.value = '';
}

function onDismiss() {
  store.syncDismissConflict();
}

// 资产 JSON 摘要（冲突框展示用）
function assetSummary(jsonStr) {
  try {
    const data = JSON.parse(jsonStr);
    const count = data.assets?.length ?? 0;
    const groups = data.groups?.length ?? 0;
    return `${count} 个连接 · ${groups} 个分组`;
  } catch {
    return '（无法解析）';
  }
}
</script>

<template>
  <section v-if="syncConflict" class="block conflict-block">
    <header class="block-head conflict-head">
      <AlertTriangle :size="12" />检测到同步冲突
    </header>
    <p class="block-note muted">
      本地和远端都有变更，请选择保留哪一份。<strong>未选中的一方将被覆盖。</strong>
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
      <AppButton variant="primary" size="sm"
        :disabled="(!opPassword && !syncAutoSyncEnabled) || syncLoading"
        @click="onResolve('remote')">用远端覆盖本地</AppButton>
      <AppButton variant="ghost" size="sm" @click="onDismiss">取消</AppButton>
    </div>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// ─── block 通用（照父组件 / McpPanelContent 范式）───
.block {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  background: var(--app-panel);
}
.block-head {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-xs);
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--app-muted);
  padding-bottom: 4px;
  border-bottom: 1px solid var(--app-border);
}
.block-head :deep(svg) { flex-shrink: 0; }
.block-note {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.6;
}
.block-note strong { color: var(--app-strong); }

// ─── 冲突框语义色 ───
.conflict-block { border-color: color-mix(in oklab, var(--danger), transparent 55%); }
.conflict-head { color: var(--danger); border-color: color-mix(in oklab, var(--danger), transparent 65%); }
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

// ─── 通用字段 ───
.field { display: flex; flex-direction: column; gap: 4px; }
.field-label { font-size: var(--text-xs); color: var(--app-muted); }
.actions { display: flex; gap: var(--space-2); flex-wrap: wrap; align-items: center; }

.muted { color: var(--app-muted); font-size: var(--text-xs); line-height: 1.5; margin: 0; }
</style>
