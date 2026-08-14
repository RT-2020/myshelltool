<script setup>
import { computed } from 'vue';
import { useAssetsStore } from '@/stores/assets.js';
import { useTunnelsStore } from '@/stores/tunnels.js';
import { useSessionsStore } from '@/stores/sessions.js';
import { useMcpStore } from '@/stores/mcp.js';
import { useSyncStore } from '@/stores/sync.js';

const assets = useAssetsStore();
const tunnels = useTunnelsStore();
const sessions = useSessionsStore();
const mcp = useMcpStore();
const sync = useSyncStore();

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
      (session.assetId === selected.value.id || session.asset?.id === selected.value.id)
      && (session.status === 'connected' || session.status === 'connecting')
  ) || null;
});

const tunnelsActive = computed(() => tunnels.tunnels.filter(tunnel => tunnel.active).length);
const tunnelsTotal = computed(() => tunnels.tunnels.length);
const summaryState = computed(() => {
  if (!selected.value) return '未选择';
  return activeSession.value ? '已连接' : '空闲';
});

const summaryRows = computed(() => {
  if (!selected.value) return [];

  const connected = activeSession.value?.status === 'connected';
  const connecting = activeSession.value?.status === 'connecting';
  const sessionId = activeSession.value?.sessionId || activeSession.value?.id || '';

  return [
    { label: '会话', value: activeSession.value ? `${sessionId.slice(0, 8)} · ${connected ? '已连接' : '连接中'}` : '— · 未连接', muted: !activeSession.value },
    { label: '主机', value: `${selected.value.username || '—'}@${selected.value.host || '—'}:${selected.value.port || 22}` },
    // 指纹暂无真实数据源：connected 时也显示 '—'，不伪造「SHA256 · 已隐藏」
    { label: '指纹', value: '—', muted: true },
    { label: '时长', value: connected ? (activeSession.value?.uptime || '—') : '—', muted: !connected },
    { label: '隧道', value: `${tunnelsActive.value} / ${tunnelsTotal.value}`, muted: tunnelsTotal.value === 0 },
    { label: '最近命令', value: connecting ? '等待终端就绪' : '—', muted: true }
  ];
});

const credentialBadge = computed(() => {
  if (!selected.value) return '未绑定';
  return selected.value.credential_id || selected.value.passphrase_credential_id
    ? '本地安全存储'
    : '未绑定';
});

const systemRows = computed(() => [
  { label: 'MCP', value: mcp.clientConnected ? '可用' : '不可用', tone: mcp.clientConnected ? 'success' : 'warn' },
  { label: '同步', value: sync.syncText || '未配置', tone: sync.configured ? 'success' : 'muted' },
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
