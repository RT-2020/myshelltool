<script setup lang="ts">
import { computed } from 'vue';
import { useAssetsStore } from '@/stores/assets';
import { useTunnelsStore } from '@/stores/tunnels';
import { useSessionsStore } from '@/stores/sessions';

const assets = useAssetsStore();
const tunnels = useTunnelsStore();
const sessions = useSessionsStore();

/** 摘要行（模板 dt/dd 渲染，muted 可选弱化）。 */
interface SummaryRow {
  label: string;
  value: string;
  muted?: boolean;
}

/** 系统功能行（badge tone 语义色类名）。 */
interface SystemRow {
  label: string;
  value: string;
  tone: string;
}

const selected = computed(() => assets.selectedAsset || null);
const tags = computed(() => {
  const value = selected.value?.tags;
  if (!value) return [];
  if (Array.isArray(value)) return value;
  return String(value).split(/[,\s]+/).filter(Boolean);
});

const activeSession = computed(() => {
  if (!selected.value) return null;
  return sessions.sessions.find(
    session =>
      // assetId 是早期会话对象的兼容字段（store 类型未声明，运行时可能 undefined）
      ((session as { assetId?: string }).assetId === selected.value.id || session.asset?.id === selected.value.id)
      && (session.status === 'connected' || session.status === 'connecting')
  ) || null;
});

const tunnelsActive = computed(() => tunnels.tunnels.filter(tunnel => tunnel.active).length);
const tunnelsTotal = computed(() => tunnels.tunnels.length);
const summaryState = computed(() => {
  if (!selected.value) return '未选择';
  return activeSession.value ? '已连接' : '空闲';
});

const summaryRows = computed<SummaryRow[]>(() => {
  if (!selected.value) return [];

  const connected = activeSession.value?.status === 'connected';
  // .id 同为早期会话对象的兼容字段（运行时可能 undefined）
  const sessionId = activeSession.value?.sessionId || (activeSession.value as { id?: string } | null)?.id || '';

  return [
    { label: '会话', value: activeSession.value ? `${sessionId.slice(0, 8)} · ${connected ? '已连接' : '连接中'}` : '— · 未连接', muted: !activeSession.value },
    { label: '主机', value: `${selected.value.username || '—'}@${selected.value.host || '—'}:${selected.value.port || 22}` },
    { label: '隧道', value: `${tunnelsActive.value} / ${tunnelsTotal.value}`, muted: tunnelsTotal.value === 0 }
  ];
});

const credentialBadge = computed(() => {
  if (!selected.value) return '未绑定';
  return selected.value.credential_id || selected.value.passphrase_credential_id
    ? '本地安全存储'
    : '未绑定';
});

// MCP/同步状态不在本面板重复展示（L0 信息唯一性）：与底部状态栏 badge 重复，
// 且状态栏版可点击打开对应设置面板。此处仅保留凭据绑定状态。
const systemRows = computed<SystemRow[]>(() => [
  { label: '凭据', value: credentialBadge.value, tone: selected.value ? 'muted' : 'warn' }
]);
</script>

<template>
  <section class="rs-section ops-section" data-region="ops-summary">
    <div class="rs-section-head">
      <span class="rs-section-title">会话摘要</span>
      <span class="rs-section-meta">{{ summaryState }}</span>
    </div>

    <div v-if="!selected" class="rs-empty-banner">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true">
        <rect x="3" y="4" width="18" height="6" rx="1.5" />
        <rect x="3" y="14" width="18" height="6" rx="1.5" />
        <path d="M7 7h.01M7 17h.01" stroke-linecap="round" />
      </svg>
      <span>未选择主机 · 点击左侧资产树选择一个连接</span>
    </div>

    <template v-else>
      <div class="ops-host">
        <strong class="ops-host-name">{{ selected.name }}</strong>
        <span v-if="selected.group && selected.group !== '未分组'" class="ops-host-group">{{ selected.group }}</span>
      </div>

      <div v-if="tags.length" class="ops-tags">
        <span v-for="tag in tags" :key="tag" class="ops-tag">{{ tag }}</span>
      </div>

      <dl class="summary-list">
        <div v-for="row in summaryRows" :key="row.label" class="summary-row">
          <dt>{{ row.label }}</dt>
          <dd class="val" :class="{ muted: row.muted }">{{ row.value }}</dd>
        </div>
      </dl>
    </template>

    <div class="system-block">
      <div class="rs-section-head compact">
        <span class="rs-section-title">系统功能</span>
        <span class="rs-section-meta">状态</span>
      </div>

      <dl class="summary-list">
        <div v-for="row in systemRows" :key="row.label" class="summary-row">
          <dt>{{ row.label }}</dt>
          <dd>
            <span class="badge" :class="row.tone">{{ row.value }}</span>
          </dd>
        </div>
      </dl>
    </div>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.ops-section {
  padding: var(--space-4) var(--space-3) var(--space-3);
}

.rs-section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  margin-bottom: var(--space-3);
}

.rs-section-head.compact {
  margin: var(--space-4) 0 var(--space-2);
}

.rs-section-title {
  color: var(--app-subtle);
  font: 500 10px var(--font-mono);
  letter-spacing: 0.08em;
}

.rs-section-meta {
  color: var(--app-subtle);
  font: 10px var(--font-mono);
  letter-spacing: 0.04em;
}

.rs-empty-banner {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 8px 10px;
  border: 1px dashed var(--app-border-strong);
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
  color: var(--app-subtle);
  font: 11px var(--font-display);
}

.rs-empty-banner svg {
  width: 14px;
  height: 14px;
  stroke-width: 1.6;
  flex-shrink: 0;
}

.ops-host {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-2);
  margin-bottom: var(--space-2);
}

.ops-host-name {
  min-width: 0;
  color: var(--app-strong);
  font: 600 13px var(--font-display);
  overflow-wrap: anywhere;
}

.ops-host-group {
  flex-shrink: 0;
  color: var(--app-muted);
  font: 10px var(--font-mono);
  letter-spacing: 0.04em;
}

.ops-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-bottom: var(--space-3);
}

.ops-tag {
  padding: 2px 7px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-pill);
  background: var(--app-panel-2);
  color: var(--app-muted);
  font: 10px var(--font-mono);
}

.summary-list {
  display: flex;
  flex-direction: column;
  margin: 0;
}

.summary-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: baseline;
  gap: var(--space-3);
  padding: 6px 0;
  border-top: 1px solid var(--app-border-soft);
}

.summary-row:first-child {
  padding-top: 2px;
  border-top: 0;
}

.summary-row dt {
  color: var(--app-subtle);
  font: 10.5px var(--font-mono);
  letter-spacing: 0.04em;
  white-space: nowrap;
}

.summary-row dd {
  min-width: 0;
  margin: 0;
  text-align: right;
}

.summary-row .val {
  display: block;
  overflow: hidden;
  color: var(--app-muted);
  font: 11.5px var(--font-mono);
  letter-spacing: 0.01em;
  text-overflow: ellipsis;
  white-space: nowrap;
}

// muted 行（占位/未连接）比普通值更弱一档，形成视觉层级
.summary-row .val.muted { color: var(--app-subtle); }

.system-block {
  margin-top: var(--space-1);
}

.badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  max-width: 100%;
  padding: 2px 7px;
  border-radius: var(--radius-pill);
  background: var(--app-panel-2);
  color: var(--app-muted);
  font: 10.5px/1.4 var(--font-display);
  white-space: nowrap;
}

.badge::before {
  content: '';
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: currentColor;
  flex-shrink: 0;
}

.badge.success {
  background: var(--success-soft);
  color: var(--success);
}

.badge.warn {
  background: var(--warn-soft);
  color: var(--warn);
}

.badge.muted {
  background: var(--app-panel-2);
  color: var(--app-muted);
}
</style>
