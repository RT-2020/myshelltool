<script setup>
/**
 * McpExecutionLogList — v2 MCP 工具执行日志区块。
 *
 * 在 McpPanelContent 的「能力清单」之前渲染。数据直接 useMcpStore()
 *（execLogs / loadExecLogs / clearExecLogs），不经 workbench re-export。
 *
 * 布局：紧凑双行列表（非表格）。设置面板 / mcpPanel 弹窗容器只有 ~370-500px 宽，
 * 6 列横排表格会被挤到折行 + 横向滚动（实测「磁盘情况」折两行、命令列只剩 2 字符），
 * 故改为：第一行 结果 + 工具 + 主机 + 决策徽章 + 时间，第二行整宽命令（ellipsis）。
 * 行点击在该条目内原位展开 outputSummary 详情；决策显示短标签，title 悬浮全称。
 * 清空走内联二次确认（照 SyncPanelContent.vue 的模式，不用 window.confirm）。
 */
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { ScrollText, RefreshCw, Trash2 } from 'lucide-vue-next';
import { useMcpStore } from '@/stores/mcp.js';
import AppButton from '@/components/ui/AppButton.vue';

const mcpStore = useMcpStore();
// setup store 解构必须走 storeToRefs（直接解构丢响应性，AGENTS.md Pinia 约定）
const { execLogs, execLogsLoading } = storeToRefs(mcpStore);

// 决策中文映射（Rust execution_log.rs::decision 常量的镜像）。全称用于 title 悬浮
const DECISION_LABELS = {
  not_required: '无需审批',
  auto_approved: '自动放行',
  minimal_allowed: '低拦截放行',
  elicitation_accepted: '客户端确认执行',
  elicitation_declined: '客户端拒绝',
  gui_accepted: 'GUI 确认执行',
  gui_declined: 'GUI 拒绝',
  rejected: '已拒绝',
  hard_blocked: '毁灭性拦截',
  timeout: '审批超时'
};
// 行内短标签（窄容器扫描用），语义分组：放行 / 已确认 / 已拒绝 / 硬拦截 / 超时
const DECISION_SHORT = {
  not_required: '放行',
  auto_approved: '放行',
  minimal_allowed: '放行',
  elicitation_accepted: '已确认',
  gui_accepted: '已确认',
  elicitation_declined: '已拒绝',
  gui_declined: '已拒绝',
  rejected: '已拒绝',
  hard_blocked: '硬拦截',
  timeout: '超时'
};
const OUTCOME_LABELS = {
  ok: '成功',
  error: '失败',
  skipped: '未执行'
};

const TOOL_LABELS = {
  ssh_exec: '远程命令',
  disk_usage: '磁盘情况',
  system_status: '系统状态',
  service_status: '服务状态',
  sftp_list: '列出目录',
  sftp_read_file: '读取文件',
  sftp_write_file: '写入文件',
  sftp_upload: '上传文件',
  sftp_download: '下载文件',
  sftp_remove: '删除文件',
  resource_monitor_snapshot: '资源快照'
};

// 结果 tone：成功/失败/未执行
function outcomeClass(outcome) {
  if (outcome === 'ok') return 'tone-ok';
  if (outcome === 'error') return 'tone-err';
  return 'tone-muted';
}

const rows = computed(() => execLogs.value ?? []);

// 点击条目在其内部展开详情（再点收起）
const expandedId = ref('');
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

function pad2(n) {
  return String(n).padStart(2, '0');
}
function fmtTime(ms) {
  if (!ms) return '—';
  try {
    const d = new Date(ms);
    if (Number.isNaN(d.getTime())) return String(ms);
    return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} `
      + `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`;
  } catch {
    return String(ms);
  }
}
// 行内短时间：当天只显示时分秒，非当天补月-日（完整时间在详情与 title 中）
function fmtShortTime(ms) {
  if (!ms) return '—';
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return String(ms);
  const now = new Date();
  const sameDay = d.getFullYear() === now.getFullYear()
    && d.getMonth() === now.getMonth() && d.getDate() === now.getDate();
  const hms = `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`;
  return sameDay ? hms : `${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${hms}`;
}

function decisionLabel(value) {
  return DECISION_LABELS[value] || value;
}
function decisionShort(value) {
  return DECISION_SHORT[value] || decisionLabel(value);
}
function outcomeLabel(value) {
  return OUTCOME_LABELS[value] || value;
}
function hostTitle(row) {
  return `${row.assetName || row.assetId || '—'}（${row.username || '—'}@${row.host || '—'}:${row.port || '—'}）`;
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
    <ul v-else-if="rows.length" class="log-list">
      <li
        v-for="row in rows"
        :key="row.id"
        :class="['log-item', { open: expandedId === row.id }]"
      >
        <button
          type="button"
          class="log-row"
          :title="row.command ? `查看输出：${row.command}` : '查看输出'"
          @click="onRowClick(row)"
        >
          <span class="line1">
            <span :class="['outcome', outcomeClass(row.outcome)]" :title="outcomeLabel(row.outcome)">
              <span class="outcome-dot" />{{ outcomeLabel(row.outcome) }}
            </span>
            <span class="tool-tag" :title="row.tool">{{ TOOL_LABELS[row.tool] || row.tool }}</span>
            <span class="host" :title="hostTitle(row)">{{ row.host || row.assetName || row.assetId || '—' }}</span>
            <span class="spacer" />
            <span :class="['decision', `decision-${row.decision}`]" :title="decisionLabel(row.decision)">
              {{ decisionShort(row.decision) }}
            </span>
            <span class="time mono" :title="fmtTime(row.timestampMs)">{{ fmtShortTime(row.timestampMs) }}</span>
          </span>
          <code class="cmd">{{ row.command || '（无命令）' }}</code>
        </button>

        <!-- 条目内原位展开：完整时间 / 意图 / 资产 / 输出摘要 -->
        <div v-if="expandedId === row.id" class="detail">
          <div class="detail-meta muted">
            <span class="mono">{{ fmtTime(row.timestampMs) }}</span>
            · 意图：{{ row.intent || '（未声明）' }}
            <template v-if="row.assetName || row.assetId">
              · {{ row.username || '—' }}@{{ row.host || '—' }}:{{ row.port || '—' }}
            </template>
          </div>
          <pre class="detail-output"><code>{{ row.outputSummary || '（无输出）' }}</code></pre>
        </div>
      </li>
    </ul>
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

// —— 紧凑列表：双行条目（第一行元信息，第二行命令）——
.log-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}
.log-item {
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel);
  overflow: hidden;

  &.open {
    border-color: var(--app-border-strong);
  }
}
.log-row {
  display: flex;
  flex-direction: column;
  gap: 3px;
  width: 100%;
  padding: 6px var(--space-2);
  border: none;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
  transition: background var(--motion-fast) var(--ease-standard);

  &:hover {
    background: var(--app-hover);
  }
}
.line1 {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
  font-size: 11px;
}
.spacer { flex: 1 1 0; min-width: var(--space-1); }

// 结果：色点 + 文字（成功/失败/未执行）
.outcome {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  white-space: nowrap;
  flex: 0 0 auto;
}
.outcome-dot {
  width: 6px;
  height: 6px;
  border-radius: var(--radius-pill);
  background: currentColor;
}
.tone-ok { color: var(--success); }
.tone-err { color: var(--danger); }
.tone-muted { color: var(--app-muted); }

// 工具：中性小 pill
.tool-tag {
  flex: 0 0 auto;
  padding: 0 6px;
  border-radius: var(--radius-pill);
  background: var(--app-hover);
  color: var(--app-muted);
  white-space: nowrap;
  line-height: 16px;
}

// 主机：可收缩省略
.host {
  flex: 0 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--app-text);
  font-size: var(--text-xs);
}

// 决策：currentColor 半透明底 pill（色值由 decision-* 类决定）
.decision {
  flex: 0 0 auto;
  padding: 0 6px;
  border-radius: var(--radius-pill);
  background: color-mix(in oklab, currentColor 10%, transparent);
  white-space: nowrap;
  line-height: 16px;
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
.decision-hard_blocked,
.decision-timeout {
  color: var(--danger);
}

.time {
  flex: 0 0 auto;
  color: var(--app-subtle);
  font-size: 11px;
  white-space: nowrap;
}

// 命令：第二行主体，整宽省略
.cmd {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--app-strong);
}

.mono {
  font-family: var(--font-mono);
  font-size: 11px;
}

// —— 展开的详情块（输出摘要）——
.detail {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0 var(--space-2) var(--space-2);
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
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

