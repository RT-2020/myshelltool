<script setup>
/**
 * McpPanelContent — v1.2 MCP 服务可观测与配置引导面板内容。
 *
 * 视觉语言对齐 OpsSummaryPanel（运维仪表盘范式）：
 *   - chrome section header（大写标题 + letter-spacing）
 *   - dl/dt/dd 网格（auto 1fr，dt 大写，dd 右对齐）
 *   - tone 语义色（success/warn/muted）
 *   - tag 小徽章（--app-subtle 背景）
 *
 * 信息架构分三层（修复原版"四个等权 section 堆叠"的混乱）：
 *   1. Hero 状态条 —— 连接状态全宽横幅，第一眼即知「连没连」+ 关键指标
 *   2. 配置引导 —— 三家共用 JSON，输入框 + 复制，warning 单行
 *   3. 能力清单 —— 工具按只读/高危分组，resources/prompts 双栏
 *
 * 由 GlobalModals.vue 的 modal.type === 'mcpPanel' 分支渲染。抽成独立组件
 * 是为避免 GlobalModals.vue 超 500 行 SFC 硬上限（AGENTS.md 质量红线）。
 */
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { Plug, Copy, RefreshCw, FileCode2, Database } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench.js';
import { useClipboard } from '@/composables/useClipboard.js';
import AppButton from '@/components/ui/AppButton.vue';
import McpCapabilityList from '@/components/shell/McpCapabilityList.vue';

const store = useWorkbenchStore();
const {
  mcpStatus, mcpProbe, mcpClientConnected, mcpDataDir, mcpServerVersion,
  mcpTools, mcpResources, mcpPrompts
} = storeToRefs(store);

const { copy } = useClipboard();

const copiedHint = ref('');

const status = computed(() => mcpStatus.value || {});
const hasStatus = computed(() => Boolean(mcpStatus.value));

function fmtTime(iso) {
  if (!iso) return '—';
  try {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    const pad = n => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  } catch {
    return iso;
  }
}

const configJson = computed(() => store.buildMcpConfig());

async function onCopyConfig() {
  const ok = await copy(configJson.value);
  flashHint(ok ? '✓ 已复制' : '复制失败');
}
async function onCopyDataDir() {
  if (!mcpDataDir.value) return;
  const ok = await copy(mcpDataDir.value);
  flashHint(ok ? '✓ 路径已复制' : '复制失败');
}
function flashHint(msg) {
  copiedHint.value = msg;
  setTimeout(() => { copiedHint.value = ''; }, 2000);
}
function onRefresh() { store.refreshMcpStatus(); }
</script>

<template>
  <div class="mcp-panel">
    <!-- ① Hero 状态条：全宽，探测结果决定整体色调，第一眼即知「MCP 能否工作」 -->
    <div class="hero" :class="mcpClientConnected ? 'is-connected' : 'is-offline'">
      <div class="hero-main">
        <Plug :size="20" class="hero-icon" />
        <div class="hero-text">
          <span class="hero-status">{{ mcpClientConnected ? 'MCP 可用' : 'MCP 不可用' }}</span>
          <span class="hero-sub">
            {{ mcpClientConnected
              ? 'HTTP 健康检查通过，MCP server 正常响应协议'
              : (mcpProbe?.detail || '探测失败，MCP 无法正常工作') }}
          </span>
        </div>
      </div>
      <div class="hero-meta">
        <span class="meta-item"><span class="meta-label">版本</span><span class="meta-val num">myshelltool {{ mcpServerVersion }}</span></span>
        <span v-if="mcpProbe?.probedAt" class="meta-item"><span class="meta-label">探测时间</span><span class="meta-val num">{{ fmtTime(mcpProbe.probedAt) }}</span></span>
        <AppButton variant="subtle" size="sm" @click="onRefresh"><RefreshCw :size="12" />刷新</AppButton>
      </div>
    </div>

    <!-- 不可用时的引导文案（可用时隐藏，减少噪音） -->
    <p v-if="!mcpClientConnected" class="hero-hint muted">
      已对 MCP HTTP endpoint 做健康检查（initialize 握手）但未通过。常见原因：HTTP server 尚未启动、端口被占用、协议异常。MCP server 随 GUI 启停，请确认 GUI 正在运行后点刷新。
    </p>

    <div v-if="!hasStatus" class="loading muted">正在探测 MCP…</div>

    <template v-else>
      <!-- ② 连接详情：紧凑 dl，照 OpsSummaryPanel 范式 -->
      <section class="block">
        <header class="block-head"><Database :size="12" />连接详情</header>
        <dl class="detail-grid">
          <dt>探测 Endpoint</dt>
          <dd><code class="mono-path">{{ mcpProbe?.exePath || '—' }}</code></dd>
          <dt>数据目录</dt>
          <dd>
            <code class="mono-path">{{ mcpDataDir || '—' }}</code>
            <button v-if="mcpDataDir" class="link-btn" @click="onCopyDataDir"><Copy :size="11" /></button>
          </dd>
          <dt>MCP Endpoint</dt>
          <dd><code class="mono-path">{{ status.endpoint || '—' }}</code></dd>
        </dl>
      </section>

      <!-- ③ 配置引导：三家共用一份 JSON -->
      <section class="block">
        <header class="block-head">
          <FileCode2 :size="12" />接入配置
          <span v-if="copiedHint" class="copy-hint">{{ copiedHint }}</span>
        </header>
        <p class="block-note muted">
          v1.4：MCP 内嵌 GUI 进程，Streamable HTTP transport。三家宿主共用
          <code>mcpServers.myshelltool</code>，仅贴入文件不同：
          <strong>Claude Code</strong> → 配置文件 ·
          <strong>Cursor</strong> → <code>.cursor/mcp.json</code> ·
          其他合规 MCP host 同理
        </p>
        <pre class="code-block"><code>{{ configJson }}</code></pre>
        <div class="config-foot">
          <AppButton variant="primary" size="sm" @click="onCopyConfig"><Copy :size="12" />复制配置 JSON</AppButton>
          <span class="warn-inline">⚠ 确保 GUI 在运行（MCP server 随 GUI 启停）</span>
        </div>
      </section>

      <!-- ④ 能力清单：抽到 McpCapabilityList 子组件（避免本 SFC 超 500 行） -->
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
  max-height: 62vh;
  overflow-y: auto;
  padding-right: 2px;
}

// ─── ① Hero 状态条 ───
// 全宽横幅，连接态用 success 边框/底色，断开用 muted。第一眼信息优先级最高。
.hero {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-md);
  border: 1px solid var(--app-border);
  background: var(--app-panel-2);
  transition: border-color var(--motion-base) var(--ease-standard),
    background var(--motion-base) var(--ease-standard);
}
.hero.is-connected {
  border-color: color-mix(in oklab, var(--success), transparent 55%);
  background: color-mix(in oklab, var(--success), transparent 90%);
}
.hero.is-offline {
  border-style: dashed;
}
.hero-main {
  display: inline-flex;
  align-items: center;
  gap: var(--space-3);
}
.hero-icon {
  flex-shrink: 0;
  color: var(--app-muted);
}
.hero.is-connected .hero-icon { color: var(--success); }
.hero-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.hero-status {
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--app-strong);
}
.hero.is-connected .hero-status { color: var(--success); }
.hero-sub {
  font-size: var(--text-xs);
  color: var(--app-muted);
}
.hero-meta {
  display: inline-flex;
  align-items: center;
  gap: var(--space-4);
  flex-shrink: 0;
}
.meta-item {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 2px;
}
.meta-label {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--app-subtle);
}
.meta-val {
  font-size: var(--text-xs);
  color: var(--app-strong);
}
.hero-hint {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.5;
  padding: 0 var(--space-1);
}

.loading {
  padding: var(--space-4);
  text-align: center;
  font-size: var(--text-xs);
}

// ─── block 通用（照 OpsSummaryPanel 的 chrome header 范式）───
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
}
.block-head :deep(svg) { flex-shrink: 0; }
.block-note {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.6;
}
.block-note code,
.req {
  font-family: var(--font-mono);
  font-size: 11px;
  background: var(--app-hover);
  padding: 0 4px;
  border-radius: 4px;
}

// ─── ② 连接详情 dl（auto 1fr 网格，dt 大写，dd 右对齐）───
.detail-grid {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px var(--space-3);
  margin: 0;
}
.detail-grid dt {
  font-size: var(--text-xs);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--app-muted);
  align-self: center;
}
.detail-grid dd {
  margin: 0;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  justify-content: flex-end;
  font-size: var(--text-xs);
  color: var(--app-strong);
}
.mono-path {
  font-family: var(--font-mono);
  font-size: 11px;
  background: var(--app-hover);
  padding: 1px 6px;
  border-radius: var(--radius-sm);
  word-break: break-all;
  max-width: 100%;
}
.link-btn {
  background: transparent;
  border: none;
  padding: 2px;
  color: var(--app-muted);
  cursor: pointer;
  border-radius: 4px;
  display: inline-flex;
}
.link-btn:hover { color: var(--app-strong); background: var(--app-hover); }

// ─── ③ 配置引导 ───
.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.field-label {
  font-size: var(--text-xs);
  color: var(--app-muted);
}
.check-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-xs);
  color: var(--app-text);
  cursor: pointer;
  margin: 2px 0;
}
.check-row code {
  font-family: var(--font-mono);
  font-size: 11px;
  background: var(--app-hover);
  padding: 0 4px;
  border-radius: 4px;
}
.code-block {
  background: var(--terminal-bg);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  padding: var(--space-2) var(--space-3);
  margin: 0;
  overflow-x: auto;
}
.code-block code {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--terminal-text);
  white-space: pre;
  line-height: 1.5;
}
.config-foot {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  flex-wrap: wrap;
}
.copy-hint {
  margin-left: auto;
  color: var(--success);
  font-size: var(--text-xs);
  text-transform: none;
  letter-spacing: 0;
}
.warn-inline {
  font-size: var(--text-xs);
  color: var(--app-muted);
  line-height: 1.4;
}
.warn-inline code {
  font-family: var(--font-mono);
  font-size: 11px;
}
</style>
