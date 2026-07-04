<script setup>
/**
 * AppStatusBar (shell) — 底栏（app.html 全量还原）。
 *
 * 结构（app.css L919-937 严格同步）：
 *   .statusbar (flex space-between, mono 11.5px)
 *     .sb-left:   SSH 状态点 + backend mode + 状态消息（可点击）
 *     .sb-center: 编码/换行/shell 信息（UTF-8 · LF · zsh）—— app.html 新增
 *     .sb-right:  传输胶囊 + 同步 badge + MCP badge + warnings
 *
 * 与旧版差异：
 *   - 新增 .sb-center 中间区（编码/换行/shell；从活跃会话派生，无会话显示占位）
 *   - 同步/MCP 改用 .badge.muted/.warn 样式（保留点击交互）
 *   - class 命名：.sb-left/.sb-center/.sb-right/.sb-item/.sb-sep
 *
 * 仍复用基础组件 ui/AppStatusBar.vue 的三 slot 布局（避免改基础组件）。
 * emit/props 全部保留（App.vue 已接线）。
 */
import { computed } from 'vue';
import { Upload } from 'lucide-vue-next';
import AppStatusBar from '../ui/AppStatusBar.vue';

const props = defineProps({
  activeSessions: { type: Number, default: 0 },
  backendMode: { type: String, default: '' },
  statusMessage: { type: String, default: '' },
  warningCount: { type: Number, default: 0 },
  syncText: { type: String, default: '' },
  mcpConnected: { type: Boolean, default: false },
  activeTransfers: { type: Number, default: 0 },
  completedTransfers: { type: Number, default: 0 },
  transferDrawerOpen: { type: Boolean, default: false }
});

const emit = defineEmits(['click-status', 'toggle-transfer-drawer', 'open-mcp-panel']);

const sshLabel = computed(() => (props.activeSessions > 0 ? '已连接' : '空闲'));
const sshDotClass = computed(() => (props.activeSessions > 0 ? 'connected' : 'idle'));

const hasActiveTransfers = computed(() => props.activeTransfers > 0);
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

// 同步状态 badge 语义：有文案时按文案内容判定 muted/warn/success
const syncBadgeClass = computed(() => {
  const t = props.syncText || '';
  if (t.includes('未配置') || t.includes('失败') || t.includes('错误')) return 'warn';
  if (t.includes('已配置') || t.includes('成功') || t.includes('完成')) return 'success';
  return 'muted';
});
</script>

<template>
  <AppStatusBar>
    <!-- ============ Left ============ -->
    <template #left>
      <span class="sb-item">
        <span class="conn-dot" :class="sshDotClass" aria-hidden="true"></span>
        SSH {{ sshLabel }}
      </span>
      <span class="sb-sep">·</span>
      <span class="sb-item">backend {{ backendMode }}</span>
      <span v-if="statusMessage" class="sb-sep">·</span>
      <button
        v-if="statusMessage"
        type="button"
        class="sb-item sb-status-msg"
        :title="statusMessage"
        @click="emit('click-status')"
      >
        {{ statusMessage }}
      </button>
    </template>

    <!-- ============ Center（app.html 新增：编码/换行/shell）============ -->
    <template #center>
      <span class="sb-item">UTF-8</span>
      <span class="sb-sep">·</span>
      <span class="sb-item">LF</span>
      <span class="sb-sep">·</span>
      <span class="sb-item">{{ activeSessions > 0 ? 'zsh' : '—' }}</span>
    </template>

    <!-- ============ Right ============ -->
    <template #right>
      <!-- 传输胶囊（保留点击交互）-->
      <button
        type="button"
        class="sb-item transfer-pill"
        :class="{ active: hasActiveTransfers, open: transferDrawerOpen }"
        :title="transferTitle"
        @click="emit('toggle-transfer-drawer')"
      >
        <Upload :size="11" class="transfer-icon" :class="{ breathing: hasActiveTransfers }" />
        <span>传输</span>
        <span v-if="transferCountLabel" class="transfer-count">{{ transferCountLabel }}</span>
      </button>
      <span class="sb-sep">·</span>

      <!-- 同步 badge -->
      <span v-if="syncText" class="badge" :class="syncBadgeClass">{{ syncText }}</span>
      <span v-if="syncText" class="sb-sep">·</span>

      <!-- MCP badge（可点击打开 MCP 面板）-->
      <button
        type="button"
        class="badge"
        :class="mcpConnected ? 'success' : 'warn'"
        :title="mcpConnected ? 'MCP 可用 — 点击查看详情与配置' : 'MCP 不可用 — 点击查看如何接入 Claude Code / Cursor'"
        @click="emit('open-mcp-panel')"
      >
        MCP {{ mcpConnected ? '可用' : '不可用' }}
      </button>
      <span v-if="warningCount > 0" class="sb-sep">·</span>
      <span v-if="warningCount > 0" class="sb-item">{{ warningCount }} 警告</span>
    </template>
  </AppStatusBar>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// ============================================================
// 状态栏样式（app.css L919-937 + 本组件特化）
// 基础布局（flex/space-between/三 slot）由 ui/AppStatusBar.vue 提供，
// 这里只样式化内部 .sb-item/.sb-sep/.badge/.conn-dot 等。
// ============================================================

.sb-item {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 0 2px;
  color: var(--app-muted);
}
.sb-item.muted { color: var(--app-subtle); }

.sb-sep {
  color: var(--app-subtle);
  flex-shrink: 0;
}

// 状态点（app.css .conn-dot 风格）
.conn-dot {
  display: inline-block;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--app-subtle);
  flex-shrink: 0;
}
.conn-dot.connected {
  background: var(--success);
  box-shadow: 0 0 0 3px var(--success-soft);
}
.conn-dot.idle {
  background: var(--app-subtle);
}
.conn-dot.warn {
  background: var(--warn);
  animation: conn-pulse var(--motion-base) infinite alternate cubic-bezier(.4, 0, .2, 1);
}
@keyframes conn-pulse {
  from { opacity: 0.5; }
  to { opacity: 1; }
}

// 状态消息按钮（可点击）
.sb-status-msg {
  background: transparent;
  border: none;
  font: inherit;
  cursor: pointer;
  max-width: 280px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  border-radius: 4px;
  padding: 0 4px;
}
.sb-status-msg:hover {
  color: var(--app-text);
  background: var(--app-hover);
}

// 传输胶囊（保留交互 + 呼吸动效）
.transfer-pill {
  background: transparent;
  border: 1px solid transparent;
  cursor: pointer;
  border-radius: var(--radius-pill);
  padding: 1px 8px;
  transition: background var(--motion-fast), border-color var(--motion-fast), color var(--motion-fast);
}
.transfer-pill:hover {
  background: var(--app-hover);
  color: var(--app-text);
}
.transfer-pill.active {
  border-color: var(--accent-soft-strong);
  color: var(--accent);
  background: var(--accent-soft);
}
.transfer-pill.open {
  border-color: var(--accent);
}
.transfer-icon {
  flex-shrink: 0;
}
.transfer-icon.breathing {
  animation: transfer-breath 1.4s ease-in-out infinite;
}
@keyframes transfer-breath {
  0%, 100% { transform: translateY(0); opacity: 1; }
  50% { transform: translateY(-1.5px); opacity: 0.65; }
}
.transfer-count {
  font-variant-numeric: tabular-nums;
  padding-inline-start: 4px;
  margin-inline-start: 2px;
  border-inline-start: 1px solid var(--app-border);
}

// badge（同步/MCP 状态标签，app.css .badge 风格）
.badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 1px 7px;
  border-radius: var(--radius-pill);
  font-size: 10.5px;
  border: 1px solid transparent;
  background: transparent;
  font-family: var(--font-mono);
  cursor: pointer;
  transition: background var(--motion-fast), border-color var(--motion-fast);
}
.badge::before {
  content: '';
  width: 5px;
  height: 5px;
  border-radius: 50%;
  flex-shrink: 0;
}
.badge.muted {
  color: var(--app-muted);
  border-color: var(--app-border);
  background: var(--app-panel-2);
}
.badge.muted::before { background: var(--app-subtle); }
.badge.success {
  color: var(--success);
  border-color: var(--success-soft);
  background: var(--success-soft);
}
.badge.success::before { background: var(--success); }
.badge.warn {
  color: var(--warn);
  border-color: var(--warn-soft);
  background: var(--warn-soft);
}
.badge.warn::before { background: var(--warn); }
.badge:hover {
  background: var(--app-hover);
}
</style>
