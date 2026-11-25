<script setup lang="ts">
import { computed } from 'vue';
import { Server } from 'lucide-vue-next';
import { useClipboard } from '@/composables/useClipboard';
import { useAssetsStore } from '@/stores/assets';
import { useTunnelsStore } from '@/stores/tunnels';
import { useSessionsStore } from '@/stores/sessions';
import { useWorkbenchStore } from '@/stores/workbench';

const assets = useAssetsStore();
const tunnels = useTunnelsStore();
const sessions = useSessionsStore();

/** 摘要行（模板 dt/dd 渲染，muted 可选弱化，copyValue 可点复制，title 可选悬浮说明）。 */
interface SummaryRow {
  label: string;
  value: string;
  muted?: boolean;
  copyValue?: string;
  title?: string;
}

const workbench = useWorkbenchStore();
const { copy } = useClipboard();

async function onCopySessionId(id: string) {
  if (!id) return;
  const ok = await copy(id);
  if (ok) {
    workbench.announce('已复制完整会话 ID');
  } else {
    workbench.announce('复制会话 ID 失败', { level: 'error' });
  }
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
  if (!activeSession.value) return '空闲';
  return activeSession.value.status === 'connected' ? '已连接' : '连接中';
});

const summaryRows = computed<SummaryRow[]>(() => {
  if (!selected.value) return [];

  const connected = activeSession.value?.status === 'connected';
  // .id 同为早期会话对象的兼容字段（运行时可能 undefined）
  const sessionId = activeSession.value?.sessionId || (activeSession.value as { id?: string } | null)?.id || '';

  return [
    {
      label: '会话',
      value: activeSession.value ? `${sessionId.slice(0, 8)} · ${connected ? '已连接' : '连接中'}` : '未连接',
      muted: !activeSession.value,
      copyValue: activeSession.value && sessionId ? sessionId : undefined,
      title: activeSession.value && sessionId ? `点击复制完整会话 ID：${sessionId}` : ''
    },
    { label: '主机', value: `${selected.value.username || '—'}@${selected.value.host || '—'}:${selected.value.port || 22}` },
    { label: '隧道', value: `${tunnelsActive.value} / ${tunnelsTotal.value}`, muted: tunnelsTotal.value === 0, title: `活动 ${tunnelsActive.value} / 共 ${tunnelsTotal.value} 条` }
  ];
});

const credentialBadge = computed(() => {
  if (!selected.value) return '未绑定';
  return selected.value.credential_id || selected.value.passphrase_credential_id
    ? '本地安全存储'
    : '未绑定';
});

// MCP/同步状态不在本面板重复展示（L0 信息唯一性）：与底部状态栏 badge 重复，
// 且状态栏版可点击打开对应设置面板。凭据状态并入上方摘要清单。
const credentialTone = computed(() => (credentialBadge.value === '未绑定' ? 'warn' : 'muted'));
</script>

<template>
  <section class="rs-section ops-section" data-region="ops-summary">
    <div class="rs-section-head">
      <span class="rs-section-title">会话摘要</span>
      <span class="rs-section-meta">{{ summaryState }}</span>
    </div>

    <div v-if="!selected" class="rs-empty-banner">
      <Server :size="14" aria-hidden="true" />
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
          <dd>
            <button
              v-if="row.copyValue"
              type="button"
              class="val summary-copy-btn"
              :title="row.title || ''"
              :aria-label="row.title || row.value"
              @click="onCopySessionId(row.copyValue)"
            >
              {{ row.value }}
            </button>
            <span v-else class="val" :class="{ muted: row.muted }" :title="row.title || ''">{{ row.value }}</span>
          </dd>
        </div>
        <div class="summary-row">
          <dt>凭据</dt>
          <dd>
            <span class="badge" :class="credentialTone">{{ credentialBadge }}</span>
          </dd>
        </div>
      </dl>
    </template>
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

/* P0 对比度（docs/设计/06 P0-1）：信息性小字 subtle 2.61-2.79:1 → muted 达标 */
.rs-section-title {
  color: var(--app-muted);
  font: 500 10px var(--font-mono);
  letter-spacing: 0.08em;
}

.rs-section-meta {
  color: var(--app-muted);
  font: 10px var(--font-mono);
  letter-spacing: 0.04em;
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
  color: var(--app-muted);
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
  color: var(--app-text);
  font: 11.5px var(--font-mono);
  letter-spacing: 0.01em;
  text-overflow: ellipsis;
  white-space: nowrap;
}

// muted 行（占位/未连接）比普通值更弱一档，形成视觉层级（docs/设计/06 P0-1：
// 数据=text、占位=muted，两档均达 4.5:1）
.summary-row .val.muted { color: var(--app-muted); }

.summary-row .summary-copy-btn {
  display: inline-block;
  max-width: 100%;
  border: 0;
  padding: 0 4px;
  margin: 0 -4px 0 0;
  border-radius: var(--radius-xs);
  background: transparent;
  text-align: right;
  cursor: pointer;
}

.summary-row .summary-copy-btn:hover {
  background: var(--app-hover);
  color: var(--app-strong);
}

.summary-row .summary-copy-btn:active {
  background: var(--app-active);
}

.summary-row .summary-copy-btn:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}
</style>
