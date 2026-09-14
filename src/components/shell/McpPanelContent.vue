<script setup lang="ts">
/**
 * McpPanelContent — MCP 服务面板（设置 → MCP tab / mcpPanel 弹窗共用）。
 *
 * 信息架构（v2.9 重设计，对齐「设置 → 同步」页的范式：状态优先、动作其次、参考折叠）：
 *   ① 控制面 = 一块面板（.mcp-surface），hairline 分隔三行：
 *      状态行（圆点 + 状态词 + 版本 + 刷新）/ 接入行（endpoint + 复制配置，
 *      JSON 与数据目录折叠进行内详情）/ 拦截行（等级 select + 一行动态说明）
 *   ② 执行日志（McpExecutionLogList）：审计区，列表限高内部滚动
 *   ③ 暴露能力（McpCapabilityList）：参考区，默认折叠
 *
 * 上一版的问题：七个等权区块竖直堆叠（Hero 横幅/连接详情/配置引导/拦截/日志/
 * 能力清单），两个长列表无高度上限全部内联，弹窗被拉到几千像素；「探测 Endpoint」
 * 与「MCP Endpoint」是同一值的冗余两行。这一版合并冗余、长列表各自限高滚动。
 *
 * 设计约束（同同步页）：只用既有 token；状态色只由圆点承担（surface 外框保持
 * 中性，不再整框上色）；复制反馈走 store.announce（toast），不在行内塞 flash 文案。
 *
 * 拦截设置（原 McpInterceptionSettings.vue）已收编进本组件的 surface 第三行——
 * 它是这个内嵌服务的控制面之一，与状态/接入同属一块面板；独立组件已无复用方。
 */
import { computed, ref, onMounted } from 'vue';
import { storeToRefs } from 'pinia';
import {
  Copy, RefreshCw, FileJson2, FolderOpen,
  ChevronDown, ChevronRight, ShieldAlert
} from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';
import { useMcpStore } from '@/stores/mcp';
import { useClipboard } from '@/composables/useClipboard';
import type { McpStatusResult } from '@/types/domain';
import AppButton from '@/components/ui/AppButton.vue';
import AppSelect from '@/components/ui/AppSelect.vue';
import McpCapabilityList from '@/components/shell/McpCapabilityList.vue';
import McpExecutionLogList from '@/components/shell/McpExecutionLogList.vue';

const store = useWorkbenchStore();
const {
  mcpStatus, mcpProbe, mcpClientConnected, mcpLoading, mcpDataDir, mcpServerVersion,
  mcpTools, mcpResources, mcpPrompts
} = storeToRefs(store);

// v2：拦截等级/执行日志直接用 mcp store（不经 workbench re-export，
// 照 ResourceMonitorPanel 直接 use 的先例）。
const mcpStore = useMcpStore();

const { copy } = useClipboard();

const status = computed<Partial<McpStatusResult>>(() => mcpStatus.value || {});
const hasStatus = computed(() => Boolean(mcpStatus.value));

// 状态行副标题的探测时间：当天只显示时分秒，跨天补月-日（完整时间在 title）
function fmtProbeTime(iso?: string | null) {
  if (!iso) return '';
  try {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return '';
    const pad = (n: number) => String(n).padStart(2, '0');
    const now = new Date();
    const sameDay = d.getFullYear() === now.getFullYear()
      && d.getMonth() === now.getMonth() && d.getDate() === now.getDate();
    const hms = `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
    return sameDay ? hms : `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${hms}`;
  } catch {
    return '';
  }
}

const statusSub = computed(() => {
  if (!mcpClientConnected.value) {
    return mcpProbe.value?.detail || '健康检查未通过';
  }
  const t = fmtProbeTime(mcpProbe.value?.probedAt);
  return t ? `HTTP 健康检查通过 · 探测于 ${t}` : 'HTTP 健康检查通过';
});

// —— 接入配置（JSON 详情默认收起，复制按钮不需要先看 JSON）——
const configOpen = ref(false);
const configJson = computed(() => store.buildMcpConfig());

async function onCopyConfig() {
  const ok = await copy(configJson.value);
  store.announce(ok ? '接入配置 JSON 已复制' : '复制失败');
}
async function onCopyEndpoint() {
  const url = status.value.endpoint;
  if (!url) return;
  const ok = await copy(url);
  store.announce(ok ? 'Endpoint 已复制' : '复制失败');
}
async function onCopyDataDir() {
  if (!mcpDataDir.value) return;
  const ok = await copy(mcpDataDir.value);
  store.announce(ok ? '数据目录路径已复制' : '复制失败');
}
function onRefresh() { store.refreshMcpStatus(); }

// —— 危险命令拦截（收编自 McpInterceptionSettings）——
const LEVEL_OPTIONS = [
  { label: '仅拦截毁灭性命令（默认）', value: 'minimal' },
  { label: '全部需确认', value: 'strict' }
];
const level = computed(() => mcpStore.interceptLevel);
const isMinimal = computed(() => level.value !== 'strict');

function onLevelChange(value: string | number) {
  // LEVEL_OPTIONS 的 value 恒为 string，String() 仅类型收敛、不改值
  if (value === mcpStore.interceptLevel) return;
  mcpStore.setMcpInterceptLevel(String(value));
}

onMounted(() => {
  if (!hasStatus.value) {
    store.refreshMcpStatus();
  }
  // 拦截等级 + 执行日志（面板打开时拉初始值）；
  // 日志上限 1000 条，这里拉 500 条做展示窗口，列表侧有分页渲染兜底
  mcpStore.loadMcpConfig();
  mcpStore.loadExecLogs(500);
});
</script>

<template>
  <div class="mcp-panel">
    <div v-if="!hasStatus" class="loading muted">正在探测 MCP…</div>

    <template v-else>
      <!-- ① 控制面：状态 / 接入 / 拦截，一块面板 hairline 分隔 -->
      <section class="mcp-surface">
        <!-- 状态行：圆点承担状态色，右侧版本 + 刷新 -->
        <div class="s-row s-status">
          <span class="status-dot" :class="mcpClientConnected ? 'is-up' : 'is-down'" />
          <div class="s-main">
            <span class="s-title">{{ mcpClientConnected ? 'MCP 服务正常' : 'MCP 服务不可用' }}</span>
            <span class="s-sub muted">{{ statusSub }}</span>
          </div>
          <span v-if="mcpServerVersion" class="s-ver num muted">v{{ mcpServerVersion }}</span>
          <AppButton variant="ghost" size="sm" :loading="mcpLoading" @click="onRefresh">
            <RefreshCw v-if="!mcpLoading" :size="12" />刷新
          </AppButton>
        </div>
        <!-- 不可用时的排查提示：收在状态行同一格内，不再独占一个段落 -->
        <p v-if="!mcpClientConnected" class="down-hint">
          MCP server 内嵌于本应用并随其启停，只监听本机回环。若刚启动请稍后点「刷新」；
          持续不可用多为端口被占用或协议异常。
        </p>

        <!-- 接入行：endpoint + 复制配置；JSON / 数据目录折叠进行内详情 -->
        <div class="s-cell">
          <div class="s-row s-config">
            <button
              type="button"
              class="config-toggle"
              :aria-expanded="configOpen"
              title="查看配置 JSON 与数据目录"
              @click="configOpen = !configOpen"
            >
              <component :is="configOpen ? ChevronDown : ChevronRight" :size="13" class="chev" />
              <FileJson2 :size="13" class="chev" />
              <span class="s-label">接入配置</span>
              <code class="endpoint">{{ status.endpoint || '—' }}</code>
            </button>
            <button
              v-if="status.endpoint"
              type="button"
              class="icon-act"
              title="复制 Endpoint"
              @click="onCopyEndpoint"
            ><Copy :size="12" /></button>
            <AppButton variant="primary" size="sm" @click="onCopyConfig">
              <Copy :size="12" />复制配置
            </AppButton>
          </div>
          <div v-if="configOpen" class="config-detail">
            <pre class="code-block"><code>{{ configJson }}</code></pre>
            <p class="muted detail-note">
              贴入 MCP host 配置文件：<strong>Claude Code</strong> 配置文件 ·
              <strong>Cursor</strong> <code>.cursor/mcp.json</code> · 其他合规 host 同理。
              MCP server 随本应用启停。
            </p>
            <div class="datadir-row">
              <FolderOpen :size="12" class="chev" />
              <span class="s-label">数据目录</span>
              <code class="endpoint">{{ mcpDataDir || '—' }}</code>
              <button
                v-if="mcpDataDir"
                type="button"
                class="icon-act"
                title="复制数据目录路径"
                @click="onCopyDataDir"
              ><Copy :size="12" /></button>
            </div>
          </div>
        </div>

        <!-- 拦截行：等级 select + 一行动态说明（原 warn 色块压缩为一行文字） -->
        <div class="s-cell">
          <div class="s-row">
            <ShieldAlert :size="13" class="chev" />
            <span class="s-label">危险命令拦截</span>
            <span class="s-spacer" />
            <AppSelect
              class="level-select"
              :model-value="level"
              :options="LEVEL_OPTIONS"
              @update:model-value="onLevelChange"
            />
          </div>
          <p class="level-note" :class="isMinimal ? 'is-warn' : 'muted'">
            {{ isMinimal
              ? '其余命令与文件操作（含 reboot、rm -rf 目录、curl|bash）直接执行并记入日志；敏感凭据读取仍需确认。'
              : '非白名单的命令与文件操作（上传/写入/下载/删除）一律弹窗确认；毁灭性命令两档恒拦。' }}
          </p>
        </div>
      </section>

      <!-- ② 执行日志：审计区（列表限高滚动在子组件内） -->
      <McpExecutionLogList />

      <!-- ③ 暴露能力：参考区，默认折叠 -->
      <McpCapabilityList :tools="mcpTools" :resources="mcpResources" :prompts="mcpPrompts" />
    </template>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.mcp-panel {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  font-size: var(--text-sm);
  // 不自带滚动：AppModal body 是唯一滚动容器，避免滚动条嵌套
}

.loading {
  padding: var(--space-4);
  text-align: center;
  font-size: var(--text-xs);
}

// ─── ① 控制面 surface：一个外框，内部 hairline 分隔（照 .sync-surface 手法）───
.mcp-surface {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  background: var(--app-panel);
  // 不能设 overflow: hidden——拦截行的 AppSelect 下拉菜单绝对定位在面板内，
  // 会被这里裁掉（下拉被截断的事故来源）；内部行没有满宽背景，无需圆角裁切
}
.mcp-surface > * + * {
  border-top: 1px solid var(--app-border-soft);
}

.s-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
}
.s-status {
  padding: var(--space-3);
}
.s-main {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
  flex: 1 1 auto;
}
.s-title {
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--app-strong);
}
.s-sub {
  font-size: var(--text-xs);
  line-height: 1.4;
  word-break: break-word;
}
.s-ver {
  flex-shrink: 0;
  font-size: var(--text-xs);
}
.s-label {
  flex-shrink: 0;
  font-size: var(--text-xs);
  font-weight: 600;
  color: var(--app-strong);
}
.s-spacer { flex: 1 1 0; }
.chev {
  flex-shrink: 0;
  color: var(--app-muted);
}

// 状态圆点：状态色唯一承担者（surface 外框保持中性）
.status-dot {
  flex-shrink: 0;
  width: 8px;
  height: 8px;
  border-radius: var(--radius-pill);
}
.status-dot.is-up { background: var(--success); }
.status-dot.is-down { background: var(--danger); }

.down-hint {
  margin: 0;
  padding: 0 var(--space-3) var(--space-3);
  font-size: var(--text-xs);
  line-height: 1.5;
  color: var(--app-muted);
}

// 接入行：整行左区是展开热区（chevron + label + endpoint）
.s-cell {
  display: flex;
  flex-direction: column;
}
.config-toggle {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex: 1 1 auto;
  min-width: 0;
  padding: 4px 6px;
  border: none;
  border-radius: var(--radius-sm);
  background: none;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.config-toggle:hover { background: var(--app-hover); }
.config-toggle:focus-visible { outline: none; box-shadow: var(--focus-ring); }
.endpoint {
  flex: 0 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--app-muted);
}
.icon-act {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--app-muted);
  cursor: pointer;
}
.icon-act:hover { color: var(--app-strong); background: var(--app-hover); }

// 行内详情：灰底 inset，与外框 hairline 区分层级
.config-detail {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0 var(--space-3) var(--space-3);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
}
.code-block {
  margin: 0;
  padding: var(--space-2) var(--space-3);
  background: var(--terminal-bg);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  // 默认配置 JSON 恰为 7 行：高度放到能完整显示，更长内容再内部滚动
  max-height: 180px;
  overflow: auto;
}
.code-block code {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--terminal-text);
  white-space: pre;
  line-height: 1.5;
}
.detail-note {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.6;
}
.detail-note code {
  font-family: var(--font-mono);
  font-size: 11px;
  background: var(--app-hover);
  padding: 0 4px;
  border-radius: 4px;
}
.datadir-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}

.level-select {
  min-width: 200px;
  flex-shrink: 0;
}
.level-note {
  margin: 0;
  padding: 0 var(--space-3) var(--space-3);
  font-size: var(--text-xs);
  line-height: 1.5;
}
.level-note.is-warn { color: var(--warn); }
.muted { color: var(--app-muted); }
</style>
