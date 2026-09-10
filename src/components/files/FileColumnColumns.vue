<script setup lang="ts">
import { ChevronDown, ChevronUp } from 'lucide-vue-next';
import { FILE_COLUMN_GRID_HEADER } from './fileColumnUtils';

withDefaults(defineProps<{
  sortKey?: string;
  sortDir?: string;
}>(), {
  sortKey: 'name',
  sortDir: 'asc'
});

const emit = defineEmits<{ sort: [key: string] }>();

const columns = [
  { key: 'name', label: '名称', className: 'col-name col-sort' },
  { key: 'size', label: '大小', className: 'col-size col-sort col-sort--num' },
  { key: 'type', label: '类型', className: 'col-type col-sort' },
  { key: 'modified', label: '修改时间', className: 'col-mtime col-sort' }
];
</script>

<template>
  <div class="col-header file-column-cols">
    <button
      v-for="column in columns"
      :key="column.key"
      :class="[column.className, { active: sortKey === column.key }]"
      type="button"
      @click="emit('sort', column.key)"
    >
      <span>{{ column.label }}</span>
      <component
        :is="sortDir === 'asc' ? ChevronUp : ChevronDown"
        v-if="sortKey === column.key"
        :size="12"
      />
    </button>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.col-header,
.file-column-cols {
  display: grid;
  grid-template-columns: v-bind(FILE_COLUMN_GRID_HEADER);
  gap: 8px;
  padding: 0 12px;
  min-height: 30px;
  align-items: center;
  background: var(--app-panel);
  border-block-end: 1px solid var(--app-border);
  font: 500 10.5px var(--font-display);
  color: var(--app-muted);
  position: sticky;
  top: 34px;
  z-index: calc(var(--z-sticky) - 1);
}

.col-sort {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  background: transparent;
  border: none;
  padding: 0;
  color: var(--app-muted);
  cursor: pointer;
  font-size: var(--text-xs);
  text-align: start;
  user-select: none;
  transition: color var(--motion-fast) var(--ease-standard);
  min-width: 0;
}

.col-sort--num {
  justify-content: flex-end;
  text-align: end;
}

.col-sort:hover {
  color: var(--app-strong);
}

.col-sort.active {
  color: var(--accent);
}
</style>
