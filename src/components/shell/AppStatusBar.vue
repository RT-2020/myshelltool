<script setup>
/**
 * AppStatusBar (shell) — specialized wrapper around the generic
 * `ui/AppStatusBar.vue` for myshelltool's status content.
 *
 * Replaces the existing App.vue statusbar block (lines 1379-1382):
 *   - Left: SSH status dot + backend mode + status message
 *   - Center: (empty placeholder — Wave 3.4 will inject transfer progress)
 *   - Right: sync status + tunnel health + warning count
 */
import { computed } from 'vue';
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
  mcpConnected: { type: Boolean, default: false }
});

const emit = defineEmits(['click-status', 'toggle-transfer-drawer', 'open-mcp-panel']);

const sshLabel = computed(() => (props.activeSessions > 0 ? 'connected' : 'idle'));
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
      <!-- Wave 3.4 will inject transfer progress summary here -->
      <button
        type="button"
        class="transfer-summary-btn"
        title="展开传输队列抽屉"
        @click="emit('toggle-transfer-drawer')"
      >
        传输
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

.transfer-summary-btn {
  background: transparent;
  border: 1px solid var(--app-border);
  color: var(--app-muted);
  padding: 2px 8px;
  font-size: var(--text-xs);
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.transfer-summary-btn:hover {
  background: var(--app-hover);
  color: var(--app-strong);
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
