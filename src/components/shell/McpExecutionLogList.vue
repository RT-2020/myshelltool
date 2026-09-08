<script setup>
/**
 * McpExecutionLogList — v2 MCP 工具执行日志区块。
 *
 * 在 McpPanelContent 的「能力清单」之前渲染。数据直接 useMcpStore()
 *（execLogs / loadExecLogs / clearExecLogs），不经 workbench re-export。
 *
 * 列：时间 / 资产（host）/ 工具 / 命令（mono + ellipsis + title 全文）/
 * 决策 / 结果。决策与结果用中文映射展示。行点击在表格下方展开该条目的
 * outputSummary（简单 v-if，不依赖 AppTable 高级特性）。
 * 清空走内联二次确认（照 SyncPanelContent.vue 的模式，不用 window.confirm）。
 */
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { ScrollText, RefreshCw, Trash2 } from 'lucide-vue-next';
import { useMcpStore } from '@/stores/mcp.js';
import AppButton from '@/components/ui/AppButton.vue';
import AppTable from '@/components/ui/AppTable.vue';

const mcpStore = useMcpStore();
// setup store 解构必须走 storeToRefs（直接解构丢响应性，AGENTS.md Pinia 约定）
const { execLogs, execLogsLoading } = storeToRefs(mcpStore);

// 决策中文映射（Rust execution_log.rs::decision 常量的镜像）
const DECISION_LABELS = {
  not_required: '无需审批',
  auto_approved: '自动放行',
  minimal_allowed: '低拦截放行',
  elicitation_accepted: '客户端确认执行',
  elicitation_declined: '客户端拒绝',
  gui_accepted: 'GUI 确认执行',
  gui_declined: 'GUI 拒绝',
  rejected: '已拒绝',
  timeout: '审批超时'
};
const OUTCOME_LABELS = {
  ok: '成功',
  error: '失败',
  skipped: '未执行'
};
// 结果 tone：成功/失败/未执行
function outcomeClass(outcome) {
  if (outcome === 'ok') return 'tone-ok';
  if (outcome === 'error') return 'tone-err';
  return 'tone-muted';
}

const COLUMNS = [
  { key: 'timestampMs', label: '时间', width: '150px' },
  { key: 'host', label: '资产', width: '140px' },
  { key: 'tool', label: '工具', width: '110px' },
  { key: 'command', label: '命令' },
  { key: 'decision', label: '决策', width: '120px' },
  { key: 'outcome', label: '结果', width: '70px' }
];

const rows = computed(() => execLogs.value ?? []);

// 行点击展开 outputSummary（同一条目再点收起）
const expandedId = ref('');
const expandedEntry = computed(() =>
  rows.value.find(r => r.id === expandedId.value) || null
);
function onRowClick(row) {
  expandedId.value = expandedId.value === row.id ? '' : row.id;
}

// 清空内联二次确认（照 SyncPanelContent.vue 模式）
const confirmingClear = ref(false);
async function onClear() {
  await mcpStore.clearExecLogs();
  confirmingClear.value = false;
  expandedId.value = '';
}

function fmtTime(ms) {
  if (!ms) return '—';
  try {
    const d = new Date(ms);
    if (Number.isNaN(d.getTime())) return String(ms);
    const pad = n => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} `
      + `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  } catch {
    return String(ms);
  }
}

function decisionLabel(value) {
  return DECISION_LABELS[value] || value;
}
function outcomeLabel(value) {
  return OUTCOME_LABELS[value] || value;
}
</script>

<template>
  <section class="block">
    <header class="block-head">
      <ScrollText :size="12" />执行日志
      <span class="head-note muted">记录每次真实触发远程执行的 MCP 工具调用（保留 30 天）</span>
      <span class="head-actions">
        <AppButton variant="ghost" size="sm" :loading="execLogsLoading" @click="mcpStore.loadExecLogs()">
          <RefreshCw v-if="!execLogsLoading" :size="12" />刷新
        </AppButton>
        <template v-if="!confirmingClear">
          <AppButton v-if="rows.length" variant="ghost" size="sm" @click="confirmingClear = true">
            <Trash2 :size="12" />清空
          </AppButton>
        </template>
        <template v-else>
          <AppButton variant="danger" size="sm" @click="onClear">确认清空？</AppButton>
          <AppButton variant="ghost" size="sm" @click="confirmingClear = false">取消</AppButton>
        </template>
      </span>
    </header>

    <div v-if="execLogsLoading && !rows.length" class="loading muted">正在加载执行记录…</div>
    <template v-else-if="rows.length">
      <AppTable
        :columns="COLUMNS"
        :data="rows"
        row-key="id"
        @row-click="onRowClick"
      >
        <template #cell-timestampMs="{ value }">
          <span class="mono">{{ fmtTime(value) }}</span>
        </template>
        <template #cell-host="{ row }">
          <span :title="`${row.assetName || row.assetId} (${row.username || '—'}@${row.host || '—'}:${row.port || '—'})`">
            {{ row.host || row.assetName || row.assetId || '—' }}
          </span>
        </template>
        <template #cell-command="{ value }">
          <code class="cmd" :title="value">{{ value || '—' }}</code>
        </template>
        <template #cell-decision="{ value }">
          <span :class="['decision', `decision-${value}`]">{{ decisionLabel(value) }}</span>
        </template>
        <template #cell-outcome="{ value }">
          <span :class="outcomeClass(value)">{{ outcomeLabel(value) }}</span>
        </template>
        <template #empty>暂无执行记录</template>
      </AppTable>

      <!-- 行点击展开的输出摘要（简单 v-if，点击同一条目收起） -->
      <div v-if="expandedEntry" class="detail">
        <div class="detail-head">
          <span class="mono">{{ fmtTime(expandedEntry.timestampMs) }}</span>
          <span class="muted">输出摘要（头尾各 250 字符，中间截断）</span>
        </div>
        <div class="detail-meta muted">
          意图：{{ expandedEntry.intent || '（未声明）' }}
          <template v-if="expandedEntry.assetName">
            · 资产：{{ expandedEntry.assetName }}（{{ expandedEntry.username }}@{{ expandedEntry.host }}:{{ expandedEntry.port }}）
          </template>
        </div>
        <pre class="detail-output"><code>{{ expandedEntry.outputSummary || '（无输出）' }}</code></pre>
      </div>
    </template>
    <p v-else class="muted empty">暂无执行记录</p>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.block {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
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
  flex-wrap: wrap;
}
.block-head :deep(svg) { flex-shrink: 0; }
.head-note {
  text-transform: none;
  letter-spacing: 0;
  font-size: 11px;
}
.head-actions {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  text-transform: none;
  letter-spacing: 0;
}

.loading {
  padding: var(--space-3);
  text-align: center;
  font-size: var(--text-xs);
}
.empty {
  margin: 0;
  padding: var(--space-2);
  font-size: var(--text-xs);
}

.mono {
  font-family: var(--font-mono);
  font-size: 11px;
  white-space: nowrap;
}
.cmd {
  display: block;
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono);
  font-size: 11px;
  background: var(--app-hover);
  padding: 1px 6px;
  border-radius: var(--radius-sm);
}

// 决策徽章：放行类偏中性，拒绝/超时类偏 warn/danger 语义
.decision {
  display: inline-block;
  font-size: var(--text-xs);
  white-space: nowrap;
}
.decision-auto-approved,
.decision-not-required {
  color: var(--app-muted);
}
.decision-minimal_allowed {
  color: var(--warn);
}
.decision-elicitation_accepted,
.decision-gui_accepted {
  color: var(--success);
}
.decision-elicitation_declined,
.decision-gui_declined,
.decision-rejected,
.decision-timeout {
  color: var(--danger);
}

.tone-ok { color: var(--success); }
.tone-err { color: var(--danger); }
.tone-muted { color: var(--app-muted); }

// 展开的详情块（输出摘要）
.detail {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
}
.detail-head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-wrap: wrap;
}
.detail-meta {
  font-size: var(--text-xs);
  line-height: 1.5;
}
.detail-output {
  margin: 0;
  background: var(--terminal-bg);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  padding: var(--space-2) var(--space-3);
  overflow-x: auto;
  max-height: 200px;
}
.detail-output code {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--terminal-text);
  white-space: pre-wrap;
  word-break: break-all;
  line-height: 1.5;
}
.muted { color: var(--app-muted); }
</style>
