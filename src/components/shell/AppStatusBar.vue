<script setup>
/**
 * AppStatusBar (shell) — specialized wrapper around the generic
 * `ui/AppStatusBar.vue` for myshelltool's status content.
 *
 *   - Left: SSH status dot + backend mode + status message
 *   - Center: 传输胶囊（Upload 图标 + 计数）。传输中高亮 + 图标动效，点击展开
 *     全局 TransferDrawer sheet（v2：原 FileSurface 底部 trigger bar 已删除）。
 *   - Right: sync status + tunnel health + warning count + MCP 指示灯
 */
import { computed } from 'vue';
import { Upload } from 'lucide-vue-next';
import AppStatusBar from '../ui/AppStatusBar.vue';

const props = defineProps({
  activeSessions: { type: Number, default: 0 },
  backendMode: { type: String, default: '' },
  statusMessage: { type: String, default: '' },
  runningTunnels: { type: Number, default: 0 },
  totalTunnels: { type: Number, default: 0 },
  warningCount: { type: Number, default: 0 },
  syncText: { type: String, default: '' },
  // v1.2：MCP client 是否连着 GUI pipe（外部 LLM 宿主是否启动了 MCP exe）。
  mcpConnected: { type: Boolean, default: false },
  // v2：传输胶囊数据。activeTransfers 含 running + pending；completedTransfers 含 done + error。
  activeTransfers: { type: Number, default: 0 },
  completedTransfers: { type: Number, default: 0 },
  transferDrawerOpen: { type: Boolean, default: false }
});

const emit = defineEmits(['click-status', 'toggle-transfer-drawer', 'open-mcp-panel']);

const sshLabel = computed(() => (props.activeSessions > 0 ? 'connected' : 'idle'));

// 有进行中传输 → 胶囊高亮 + 图标呼吸动效。
const hasActiveTransfers = computed(() => props.activeTransfers > 0);
// 总数显示：有活跃任务时显示活跃数，否则若历史有完成也显示「✓完成数」，都没有就空胶囊。
const transferCountLabel = computed(() => {
  if (props.activeTransfers > 0) return String(props.activeTransfers);
  if (props.completedTransfers > 0) return '✓' + props.completedTransfers;
  return '';
});
const transferTitle = computed(() => {
  const parts = [];
  parts.push(props.activeTransfers + ' 进行中');
  parts.push(props.completedTransfers + ' 完成');
  parts.push(props.transferDrawerOpen ? '点击收起' : '点击展开');
  return parts.join(' · ');
});
</script>

<template>
  <AppStatusBar>
    <template #left>
      <span class="ssh-status">
        <span class="dot" :class="{ running: activeSessions > 0 }" aria-hidden="true"></span>
        SSH {{ sshLabel }}
      </span>
      <span class="backend-mode">backend <span class="num">{{ backendMode }}</span></span>
      <button
        v-if="statusMessage"
        type="button"
        class="status-message-btn"
        :title="statusMessage"
        @click="emit('click-status')"
      >
        {{ statusMessage }}
      </button>
    </template>

    <template #center>
      <!-- 传输胶囊：Upload 图标 + 计数。传输中（activeTransfers>0）高亮 + 图标呼吸动效，
           点击切换全局 TransferDrawer sheet。原 FileSurface 底部 trigger bar 已删除。 -->
      <button
        type="button"
        class="transfer-pill"
        :class="{ active: hasActiveTransfers, open: transferDrawerOpen }"
        :title="transferTitle"
        @click="emit('toggle-transfer-drawer')"
      >
        <Upload :size="12" class="transfer-pill-icon" :class="{ breathing: hasActiveTransfers }" />
        <span class="transfer-pill-label">传输</span>
        <span v-if="transferCountLabel" class="transfer-pill-count">{{ transferCountLabel }}</span>
      </button>
    </template>

    <template #right>
      <span v-if="syncText" class="sync-text">{{ syncText }}</span>
      <button
        type="button"
        class="mcp-indicator"
        :class="{ connected: mcpConnected }"
        :title="mcpConnected ? 'MCP 可用：检测到 MCP 进程心跳，可被外部 LLM 宿主调用 — 点击查看详情与配置' : 'MCP 不可用：未检测到 MCP 进程心跳 — 点击查看如何接入 Claude Desktop / Cursor'"
        @click="emit('open-mcp-panel')"
      >
        <span class="dot" :class="{ running: mcpConnected }" aria-hidden="true"></span>
        MCP {{ mcpConnected ? '可用' : '不可用' }}
      </button>
      <span class="tunnels">tunnels {{ runningTunnels }}/{{ totalTunnels }}</span>
      <span class="warnings">
        <span class="dot warn" aria-hidden="true"></span>
        {{ warningCount }} warning
      </span>
    </template>
  </AppStatusBar>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.ssh-status {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--app-strong);
}

.dot {
  display: inline-block;
  width: 6px;
  height: 6px;
  border-radius: var(--radius-pill);
  background: var(--app-muted);
  flex-shrink: 0;
}

.dot.running {
  background: var(--success);
}

.dot.warn {
  background: var(--warn);
}

.backend-mode {
  color: var(--app-muted);
}

.num {
  font-family: var(--font-mono);
  color: var(--app-strong);
}

.status-message-btn {
  background: transparent;
  border: none;
  padding: 0;
  color: var(--app-muted);
  font: inherit;
  font-size: var(--text-xs);
  cursor: pointer;
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status-message-btn:hover {
  color: var(--app-strong);
  text-decoration: underline;
}

// 传输胶囊：Upload 图标 + 「传输」+ 计数。低视觉重量，传输中高亮 + 图标呼吸动效。
.transfer-pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  background: transparent;
  border: 1px solid var(--app-border);
  color: var(--app-muted);
  padding: 2px 8px 2px 7px;
  font-size: var(--text-xs);
  border-radius: var(--radius-pill);
  cursor: pointer;
  transition: background var(--motion-fast) var(--ease-standard),
    border-color var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard);
}

.transfer-pill:hover {
  background: var(--app-hover);
  color: var(--app-strong);
  border-color: var(--app-border-strong);
}

// 传输中：accent 高亮（边框 + 文字），图标开始呼吸。
.transfer-pill.active {
  border-color: color-mix(in oklab, var(--accent), transparent 30%);
  color: var(--accent);
  background: color-mix(in oklab, var(--accent), transparent 90%);
}
.transfer-pill.active:hover {
  background: color-mix(in oklab, var(--accent), transparent 82%);
}

// sheet 打开时：边框转实色，指示当前聚焦面板。
.transfer-pill.open {
  border-color: var(--accent);
}

.transfer-pill-label {
  line-height: 1;
}

.transfer-pill-count {
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
  padding-inline-start: 2px;
  border-inline-start: 1px solid color-mix(in oklab, currentColor, transparent 60%);
  padding-inline: 5px 0;
  margin-inline-start: 1px;
  line-height: 1;
}

// 图标呼吸动效（仅传输中触发）。
.transfer-pill-icon {
  flex-shrink: 0;
}
.transfer-pill-icon.breathing {
  animation: transfer-breath 1.4s ease-in-out infinite;
}
@keyframes transfer-breath {
  0%, 100% { transform: translateY(0); opacity: 1; }
  50% { transform: translateY(-1.5px); opacity: 0.65; }
}

.sync-text {
  color: var(--app-muted);
}

.mcp-indicator {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background: transparent;
  border: none;
  padding: 0;
  color: var(--app-muted);
  font: inherit;
  font-size: var(--text-xs);
  cursor: pointer;
  border-radius: var(--radius-sm);
}

.mcp-indicator:hover {
  color: var(--app-strong);
}

.mcp-indicator.connected {
  color: var(--app-strong);
}

.tunnels {
  color: var(--app-muted);
}

.warnings {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--app-muted);
}

.warnings .dot.warn {
  background: var(--warn);
}
</style>
