<script setup lang="ts">
/**
 * McpCapabilityList — MCP server 暴露的能力清单（工具 + resources + prompts）。
 *
 * v2.9 重设计：改为默认折叠的 disclosure（照 SyncAdvancedSettings 的
 * adv-head 模式：chevron + 标题 + 右侧摘要「N 工具 · M 资源 · K 诊断」）。
 * 它是纯参考信息（MCP host 会自动发现这些能力，用户极少需要核对），
 * 不该与状态/日志争抢首屏。展开后工具列表限高内部滚动（父级最大高度 +
 * 滚动条），不再把整个设置弹窗撑长。
 *
 * 视觉语言：折叠头对齐 adv-head；工具按高危/只读分组，语义色只由
 * 左边线承担（v2.7 设计约束：同一个状态不上三遍色）。
 */
import { computed, ref } from 'vue';
import { ChevronDown, ChevronRight, Blocks, ShieldAlert, BookOpen } from 'lucide-vue-next';
import type { McpToolInfo, McpResourceInfo, McpPromptInfo } from '@/types/domain';

const props = withDefaults(
  defineProps<{
    tools?: McpToolInfo[];
    resources?: McpResourceInfo[];
    prompts?: McpPromptInfo[];
  }>(),
  {
    tools: () => [],
    resources: () => [],
    prompts: () => []
  }
);

const open = ref(false);

const dangerousTools = computed(() => props.tools.filter(t => t.tag === 'dangerous'));
const readonlyTools = computed(() => props.tools.filter(t => t.tag !== 'dangerous'));
</script>

<template>
  <section class="cap" :class="{ 'is-open': open }">
    <button
      type="button"
      class="cap-head"
      :aria-expanded="open"
      @click="open = !open"
    >
      <component :is="open ? ChevronDown : ChevronRight" :size="13" />
      <Blocks :size="13" />
      <span>暴露能力</span>
      <span class="cap-note">{{ tools.length }} 工具 · {{ resources.length }} 资源 · {{ prompts.length }} 诊断</span>
    </button>

    <div v-if="open" class="cap-body">
      <!-- 工具清单限高内部滚动：11+ 个工具带描述全展开会占掉整个弹窗高度 -->
      <div class="cap-scroll">
        <div v-if="dangerousTools.length" class="tool-group is-danger">
          <span class="group-label tone-danger"><ShieldAlert :size="11" />高危（需审批）</span>
          <ul class="tool-list">
            <li v-for="tool in dangerousTools" :key="tool.name" class="tool-item">
              <code class="tool-name">{{ tool.name }}</code>
              <p class="tool-desc muted">{{ tool.description }}</p>
            </li>
          </ul>
        </div>

        <div v-if="readonlyTools.length" class="tool-group">
          <span class="group-label tone-muted"><BookOpen :size="11" />只读（自动放行）</span>
          <ul class="tool-list">
            <li v-for="tool in readonlyTools" :key="tool.name" class="tool-item">
              <code class="tool-name">{{ tool.name }}</code>
              <p class="tool-desc muted">{{ tool.description }}</p>
            </li>
          </ul>
        </div>
      </div>

      <div class="two-col">
        <div class="sub-block">
          <span class="sub-label muted">Resources · {{ resources.length }}</span>
          <ul class="simple-list">
            <li v-for="r in resources" :key="r.uri">
              <code>{{ r.uri }}</code>
              <span v-if="r.is_template" class="tag">模板</span>
            </li>
          </ul>
        </div>
        <div class="sub-block">
          <span class="sub-label muted">Prompts · {{ prompts.length }}</span>
          <ul class="simple-list">
            <li v-for="p in prompts" :key="p.name">
              <code>{{ p.name }}</code>
              <span v-if="p.arguments?.length" class="muted args">({{ p.arguments.join(', ') }})</span>
            </li>
          </ul>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// 折叠容器：对齐 SyncAdvancedSettings 的 adv 模式
.cap {
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  background: var(--app-panel-2);
  overflow: hidden;
}
.cap.is-open { background: var(--app-panel); }

.cap-head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  padding: var(--space-3);
  background: none;
  border: none;
  color: var(--app-muted);
  font: inherit;
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.04em;
  text-align: left;
  cursor: pointer;
}
.cap-head:hover { color: var(--app-strong); background: var(--app-hover); }
.cap-head:focus-visible { outline: none; box-shadow: var(--focus-ring); }
.cap-head :deep(svg) { flex-shrink: 0; }
.cap-note {
  margin-left: auto;
  font-weight: 400;
  color: var(--app-subtle);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.cap-body {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: 0 var(--space-3) var(--space-3);
}

// 工具清单：父级限高 + 内部滚动
.cap-scroll {
  max-height: 260px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding-right: var(--space-1); // 给滚动条留位，避免压着条目右边
}

.tool-group {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}
.group-label {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: var(--text-xs);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}
.group-label.tone-danger { color: var(--danger); }
.group-label.tone-muted { color: var(--app-muted); }
.tool-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}
.tool-item {
  padding: 6px var(--space-2);
  background: var(--app-panel-2);
  border-radius: var(--radius-sm);
  border-left: 2px solid var(--app-border);
}
.tool-group.is-danger .tool-item { border-left-color: var(--danger); }
.tool-name {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--app-strong);
  font-weight: 600;
}
.tool-desc {
  margin: 2px 0 0;
  font-size: 11px;
  line-height: 1.45;
}

.two-col {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-4);
  padding-top: var(--space-2);
  border-top: 1px solid var(--app-border-soft);
}
.sub-block {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}
.sub-label {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
}
.simple-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.simple-list li {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}
.simple-list code {
  font-family: var(--font-mono);
  font-size: 11px;
  word-break: break-all;
  color: var(--app-text);
}
.args { font-size: 11px; }
.tag {
  font-size: 10px;
  padding: 1px 6px;
  background: var(--app-subtle);
  color: var(--app-muted);
  border-radius: var(--radius-pill);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
</style>
