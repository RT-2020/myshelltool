<script setup>
/**
 * McpInterceptionSettings — v2 MCP 危险命令拦截等级设置区块。
 *
 * 在 McpPanelContent 的「能力清单」之前渲染（section.block 结构与父面板一致）。
 * 两档：
 *   - minimal（默认）：仅拦截黑名单/超高危命令，其余放行（低摩擦，用户明确选择）
 *   - strict：非白名单命令一律需确认（原 fail-secure 语义）
 * 切换即 setMcpInterceptLevel（后端更新共享配置 + 落盘，已建 MCP 会话下次
 * 调用即生效）。
 *
 * 数据直接 useMcpStore()，不经 workbench re-export（workbench.js 已贴 500 行
 * 硬上限；照 ResourceMonitorPanel 直接 use 的先例）。
 */
import { computed } from 'vue';
import { ShieldAlert, ShieldCheck } from 'lucide-vue-next';
import { useMcpStore } from '@/stores/mcp.js';
import AppSelect from '@/components/ui/AppSelect.vue';

const mcpStore = useMcpStore();

const LEVEL_OPTIONS = [
  { label: '仅拦截超高危（默认）', value: 'minimal' },
  { label: '全部需确认', value: 'strict' }
];

const level = computed(() => mcpStore.interceptLevel);
const isMinimal = computed(() => level.value !== 'strict');

function onLevelChange(value) {
  if (value === mcpStore.interceptLevel) return;
  mcpStore.setMcpInterceptLevel(value);
}
</script>

<template>
  <section class="block">
    <header class="block-head">
      <component :is="isMinimal ? ShieldAlert : ShieldCheck" :size="12" />危险命令拦截
    </header>
    <div class="setting-row">
      <div class="setting-meta">
        <span class="setting-label">拦截等级</span>
        <span class="setting-desc muted">对 MCP 工具（ssh_exec 等）执行的命令做审批判定</span>
      </div>
      <div class="setting-control">
        <AppSelect
          :model-value="level"
          :options="LEVEL_OPTIONS"
          @update:model-value="onLevelChange"
        />
      </div>
    </div>
    <!-- 固定风险提示：Minimal 档下展示（评审 P2 要求的原文），warn 色语义 -->
    <p v-if="isMinimal" class="level-warn">
      <ShieldAlert :size="12" class="warn-icon" />
      低拦截等级下，不在黑名单中的命令（如下载后执行等组合操作）将不经确认直接运行
    </p>
    <p v-else class="level-note muted">
      所有不在只读白名单中的命令均需人工确认（超高危命令两档下始终拦截）。
    </p>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// block 结构照父面板 McpPanelContent 的范式（section.block + block-head）
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

.setting-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  flex-wrap: wrap;
}
.setting-meta {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.setting-label {
  font-size: var(--text-sm);
  color: var(--app-strong);
}
.setting-desc {
  font-size: var(--text-xs);
}
.setting-control {
  min-width: 200px;
}

// warn 色风险提示（--warn token 语义）
.level-warn {
  margin: 0;
  display: flex;
  align-items: flex-start;
  gap: 6px;
  font-size: var(--text-xs);
  line-height: 1.5;
  color: var(--warn);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  background: color-mix(in oklab, var(--warn), transparent 92%);
  border: 1px solid color-mix(in oklab, var(--warn), transparent 65%);
}
.warn-icon {
  flex-shrink: 0;
  margin-top: 1px;
}
.level-note {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.5;
}
.muted { color: var(--app-muted); }
</style>
