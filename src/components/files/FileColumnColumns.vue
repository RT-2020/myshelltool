<script setup lang="ts">
import { ChevronDown, ChevronUp } from 'lucide-vue-next';
import { FILE_COLUMN_GRID_HEADER, FILE_COLUMN_GRID_HEADER_NARROW } from './fileColumnUtils';

withDefaults(defineProps<{
  sortKey?: string;
  sortDir?: string;
}>(), {
  sortKey: 'name',
  sortDir: 'asc'
});

const emit = defineEmits<{ sort: [key: string] }>();

// 对齐：名称左对齐（文本主列，省略号收尾）；其余元数据列一律居中（权限/大小/类型/修改时间）。
// 表头按钮与数据单元格必须用同一套对齐类，否则表头与内容会错开。
const columns = [
  { key: 'name', label: '名称', className: 'col-name col-sort' },
  { key: 'permissions', label: '权限', className: 'col-perm col-sort col-sort--center' },
  { key: 'size', label: '大小', className: 'col-size col-sort col-sort--center' },
  { key: 'type', label: '类型', className: 'col-type col-sort col-sort--center' },
  { key: 'modified', label: '修改时间', className: 'col-mtime col-sort col-sort--center' }
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
  gap: var(--space-2);
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

// 窄栏（双栏模式每栏约 400px）：去掉权限列并换 4 列模板，否则名称列被元数据列挤没。
// 阈值 560px = 含权限的固定列 382px + 名称列约 180px；容器是 FileColumn 的 .file-column。
// 选择器带回 .col-header 前缀：同组件的 .col-sort{display:inline-flex} 与它是同级
// （0,1,0），跨组件打包顺序不保证，必须靠特异性压过，否则隐藏列后表头按钮会掉进隐式列。
@container (max-width: 560px) {
  .col-header .col-perm {
    display: none;
  }

  .col-header,
  .file-column-cols {
    grid-template-columns: v-bind(FILE_COLUMN_GRID_HEADER_NARROW);
  }
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

// 居中（权限/大小/类型/修改时间四个元数据列）
.col-sort--center {
  justify-content: center;
  text-align: center;
}

.col-sort:hover {
  color: var(--app-strong);
}

.col-sort.active {
  color: var(--accent);
}
</style>
