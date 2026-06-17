<script setup>
/**
 * McpCapabilityList — MCP server 暴露的能力清单（工具 + resources + prompts）。
 *
 * 从 McpPanelContent.vue 抽出，因为 McpPanelContent 超 500 行 SFC 硬上限
 * （AGENTS.md 质量红线），能力清单是自包含的纯展示区块，适合独立。
 *
 * 视觉语言对齐 OpsSummaryPanel：chrome section header + tone 语义色 +
 * tag 徽章。工具按高危/只读分组（高危优先、红色左边框更显眼）。
 */
import { computed } from 'vue';
import { ShieldAlert, BookOpen } from 'lucide-vue-next';

const props = defineProps({
  tools: { type: Array, default: () => [] },
  resources: { type: Array, default: () => [] },
  prompts: { type: Array, default: () => [] }
});

const dangerousTools = computed(() => props.tools.filter(t => t.tag === 'dangerous'));
const readonlyTools = computed(() => props.tools.filter(t => t.tag !== 'dangerous'));
</script>

<template>
  <section class="block">
    <header class="block-head">暴露能力</header>

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
}

.tool-group {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}
.tool-group + .tool-group { margin-top: var(--space-1); }
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
  padding: var(--space-2) var(--space-3);
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
  margin-top: var(--space-2);
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
