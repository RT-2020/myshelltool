<script setup lang="ts">
/**
 * McpInterceptionSettings — v2 MCP 危险命令拦截等级设置区块。
 *
 * 在 McpPanelContent 的「能力清单」之前渲染（section.block 结构与父面板一致）。
 * 两档：
 *   - minimal（默认）：仅硬拦毁灭性操作（rm -rf /、mkfs 等机器报废级与
 *     根级删除，两档恒拒不弹审批），其余命令与文件操作（上传/写入/下载/
 *     删除）不经确认直接执行（记执行日志）
 *   - strict：非白名单命令与文件操作（上传/写入/下载/删除）一律需确认
 *     （原 fail-secure 语义）；毁灭性操作两档恒拦
 * 切换即 setMcpInterceptLevel（后端更新共享配置 + 落盘，已建 MCP 会话下次
 * 调用即生效）。
 *
 * 数据直接 useMcpStore()，不经 workbench re-export（workbench.js 已贴 500 行
 * 硬上限；照 ResourceMonitorPanel 直接 use 的先例）。
 */
import { computed } from 'vue';
import { ShieldAlert, ShieldCheck } from 'lucide-vue-next';
import { useMcpStore } from '@/stores/mcp';
import AppSelect from '@/components/ui/AppSelect.vue';

const mcpStore = useMcpStore();

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
</script>

<template>
  <section class="block">
    <header class="block-head">
      <component :is="isMinimal ? ShieldAlert : ShieldCheck" :size="12" />危险命令拦截
    </header>
    <div class="setting-row">
      <div class="setting-meta">
        <span class="setting-label">拦截等级</span>
        <span class="setting-desc muted">仅硬拦毁灭性操作（rm -rf /、mkfs、根级删除等），其余命令与文件操作（上传/写入/下载/删除）直接执行</span>
      </div>
      <div class="setting-control">
        <AppSelect
          :model-value="level"
          :options="LEVEL_OPTIONS"
          @update:model-value="onLevelChange"
        />
      </div>
    </div>
    <!-- 固定风险提示：Minimal 档下展示（低拦截风险陈述），warn 色语义 -->
    <p v-if="isMinimal" class="level-warn">
      <ShieldAlert :size="12" class="warn-icon" />
      除毁灭性操作恒拒外，其余命令与文件操作（含 reboot、rm -rf 目录、curl|bash、文件上传/写入/删除）均不经确认直接执行，执行记录可在下方日志查看；敏感凭据文件读取仍需确认
    </p>
    <p v-else class="level-note muted">
      所有不在只读白名单中的命令与文件操作（上传/写入/下载/删除）均需人工确认；毁灭性操作与根级/本机系统目录删除两档下均直接拦截。
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
