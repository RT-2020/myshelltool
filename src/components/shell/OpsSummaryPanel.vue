<script setup>
import { computed } from 'vue';
import { ServerCog, Tags, Clock, KeyRound, GitBranch, Activity } from 'lucide-vue-next';
import { useAssetsStore } from '@/stores/assets.js';
import { useTunnelsStore } from '@/stores/tunnels.js';
import { useSessionsStore } from '@/stores/sessions.js';

const assets = useAssetsStore();
const tunnels = useTunnelsStore();
const sessions = useSessionsStore();

const selected = computed(() => assets.selectedAsset || null);
const tags = computed(() => {
  const t = selected.value?.tags;
  if (!t) return [];
  if (Array.isArray(t)) return t;
  return String(t).split(/[·,，\s]+/).filter(Boolean);
});
const tunnelsActive = computed(() => tunnels.tunnels.filter(t => t.active).length);
const tunnelsTotal = computed(() => tunnels.tunnels.length);
const sessionsConnected = computed(() => sessions.sessions.filter(s => s.status === 'connected').length);
const sessionsTotal = computed(() => sessions.sessions.length);

const summaryRows = computed(() => {
  if (!selected.value) return [];
  return [
    {
      icon: ServerCog,
      label: '主机',
      value: `${selected.value.host || '—'}:${selected.value.port || 22}`,
      mono: true
    },
    {
      icon: KeyRound,
      label: '凭据',
      value: selected.value.credential_id ? '已绑定' : '未绑定',
      tone: selected.value.credential_id ? 'success' : 'muted'
    },
    {
      icon: Clock,
      label: '最近连接',
      value: selected.value.last_connected || '从未',
      mono: true
    },
    {
      icon: Activity,
      label: '活跃会话',
      value: `${sessionsConnected.value} / ${sessionsTotal.value}`,
      mono: true
    },
    {
      icon: GitBranch,
      label: 'Git 同步',
      value: assets.assetSource?.source || '—',
      tone: assets.githubPatConfigured ? 'success' : 'warn'
    },
    {
      icon: ServerCog,
      label: '隧道',
      value: `${tunnelsActive.value} 活跃 / ${tunnelsTotal.value} 总`,
      mono: true
    }
  ];
});
</script>

<template>
  <section class="ops-panel" data-region="ops-summary">
    <header class="ops-head">
      <span class="ops-title"><ServerCog :size="14" /> 运维摘要</span>
    </header>

    <div v-if="!selected" class="ops-empty">
      <ServerCog :size="24" />
      <p class="muted">选择左侧主机查看详情</p>
    </div>

    <div v-else class="ops-body">
      <div class="ops-host">
        <strong class="ops-host-name">{{ selected.name }}</strong>
        <span v-if="selected.group && selected.group !== '未分组'" class="ops-host-group">{{ selected.group }}</span>
      </div>

      <div v-if="tags.length" class="ops-tags">
        <span v-for="tag in tags" :key="tag" class="ops-tag">{{ tag }}</span>
      </div>

      <dl class="ops-rows">
        <template v-for="row in summaryRows" :key="row.label">
          <dt>
            <component :is="row.icon" :size="12" v-if="row.icon" />
            <span class="ops-row-label">{{ row.label }}</span>
          </dt>
          <dd :class="['ops-row-value', row.mono ? 'mono num' : '', `tone-${row.tone || 'default'}`]">
            {{ row.value }}
          </dd>
        </template>
      </dl>
    </div>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.ops-panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1 1 40%;
  overflow: hidden;
}
.ops-head {
  display: flex;
  align-items: center;
  flex: 0 0 auto; // 固定 header，body 滚动时不被压缩
  padding: var(--space-2) var(--space-3);
  border-block-end: 1px solid var(--app-border);
  background: var(--app-chrome);
}
.ops-title {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-xs);
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--app-muted);
}
.ops-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  color: var(--app-muted);
  padding: var(--space-4);
}
.ops-empty p { margin: 0; font-size: var(--text-xs); }
.ops-body {
  flex: 1;
  overflow-y: auto;
  padding: var(--space-2) var(--space-3);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}
.ops-host {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-2);
}
.ops-host-name {
  font-size: var(--text-sm);
  color: var(--app-strong);
  font-weight: 600;
  word-break: break-all;
}
.ops-host-group {
  font-size: var(--text-xs);
  color: var(--app-muted);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  flex-shrink: 0;
}
.ops-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.ops-tag {
  font-size: 10px;
  padding: 2px 6px;
  background: var(--app-subtle);
  color: var(--app-muted);
  border-radius: var(--radius-sm);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
.ops-rows {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px var(--space-2);
  margin: 0;
}
.ops-rows dt {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: var(--text-xs);
  color: var(--app-muted);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}
.ops-row-label { white-space: nowrap; }
.ops-rows dd {
  margin: 0;
  font-size: var(--text-xs);
  color: var(--app-strong);
  text-align: end;
  word-break: break-all;
}
.ops-rows dd.tone-success { color: var(--success); }
.ops-rows dd.tone-warn { color: var(--warn); }
.ops-rows dd.tone-muted { color: var(--app-muted); }
</style>
