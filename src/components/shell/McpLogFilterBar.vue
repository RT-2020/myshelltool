<script setup lang="ts">
/**
 * McpLogFilterBar — MCP 执行日志过滤条（v2.3）。
 *
 * 从 McpExecutionLogList 抽出（主文件贴近 500 行 SFC 红线）。纯条件收集组件：
 * 关键字 + 工具/资产/结果 三个下拉；工具与资产的选项由日志数据实时推导
 * （只出现数据中出现过的值）。过滤执行在父组件的 filtered computed 中。
 */
import { computed } from 'vue';
import { Search } from 'lucide-vue-next';
import AppInput from '@/components/ui/AppInput.vue';
import AppSelect from '@/components/ui/AppSelect.vue';
import { TOOL_LABELS, OUTCOME_LABELS } from '@/lib/mcpLogLabels';
import type { McpExecutionLogEntry } from '@/types/domain';

const props = withDefaults(
  defineProps<{
    /** 原始日志（推导下拉选项用）。 */
    logs?: McpExecutionLogEntry[];
    search?: string;
    tool?: string;
    asset?: string;
    outcome?: string;
  }>(),
  {
    logs: () => [],
    search: '',
    tool: '',
    asset: '',
    outcome: ''
  }
);
const emit = defineEmits<{
  'update:search': [value: string];
  'update:tool': [value: string];
  'update:asset': [value: string];
  'update:outcome': [value: string];
}>();

const ALL = { label: '全部工具', value: '' };

// 工具下拉：只列数据中出现过的工具，按中文名排序
const toolOptions = computed(() => {
  const present = [...new Set(props.logs.map(l => l.tool).filter(Boolean))];
  present.sort((a, b) => (TOOL_LABELS[a] || a).localeCompare(TOOL_LABELS[b] || b, 'zh'));
  return [ALL, ...present.map(value => ({ value, label: TOOL_LABELS[value] || value }))];
});

// 资产下拉：按 assetId 去重（同名资产多主机时 id 更稳），展示名优先
const assetOptions = computed(() => {
  const map = new Map<string, string>();
  for (const log of props.logs) {
    const id = log.assetId || log.host || '';
    if (!id || map.has(id)) continue;
    map.set(id, log.assetName || log.host || id);
  }
  const options = [...map.entries()]
    .sort((a, b) => a[1].localeCompare(b[1], 'zh'))
    .map(([value, label]) => ({ value, label }));
  return [{ label: '全部资产', value: '' }, ...options];
});

const outcomeOptions = [
  { label: '全部结果', value: '' },
  { label: OUTCOME_LABELS.ok, value: 'ok' },
  { label: OUTCOME_LABELS.error, value: 'error' },
  { label: OUTCOME_LABELS.skipped, value: 'skipped' }
];

const hasAnyFilter = computed(() =>
  Boolean(props.search || props.tool || props.asset || props.outcome)
);
</script>

<template>
  <div class="log-filters">
    <div class="filter-search">
      <Search :size="12" />
      <AppInput
        :model-value="search"
        placeholder="搜索命令 / 主机 / 资产 / 意图"
        @update:model-value="v => emit('update:search', v)"
      />
      <button
        v-if="hasAnyFilter"
        type="button"
        class="filter-clear"
        title="清除全部筛选"
        @click="emit('update:search', ''); emit('update:tool', ''); emit('update:asset', ''); emit('update:outcome', '')"
      >
        ×
      </button>
    </div>
    <div class="filter-row">
      <AppSelect
        class="filter-select"
        :model-value="tool"
        :options="toolOptions"
        @update:model-value="v => emit('update:tool', String(v))"
      />
      <AppSelect
        class="filter-select"
        :model-value="asset"
        :options="assetOptions"
        @update:model-value="v => emit('update:asset', String(v))"
      />
      <AppSelect
        class="filter-select"
        :model-value="outcome"
        :options="outcomeOptions"
        @update:model-value="v => emit('update:outcome', String(v))"
      />
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.log-filters {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.filter-search {
  position: relative;
  display: flex;
  align-items: center;

  :deep(svg) {
    position: absolute;
    left: 8px;
    color: var(--app-subtle);
    pointer-events: none;
  }

  :deep(.app-input),
  :deep(input) {
    width: 100%;
    padding-left: 26px;
  }
}

.filter-clear {
  position: absolute;
  right: 4px;
  width: 18px;
  height: 18px;
  display: grid;
  place-items: center;
  border: none;
  background: transparent;
  color: var(--app-muted);
  font-size: 14px;
  line-height: 1;
  cursor: pointer;
  border-radius: var(--radius-pill);

  &:hover {
    color: var(--danger);
    background: var(--app-hover);
  }
}

.filter-row {
  display: flex;
  gap: var(--space-2);
  flex-wrap: wrap;
}

.filter-select {
  flex: 1 1 100px;
  min-width: 0;
}
</style>
