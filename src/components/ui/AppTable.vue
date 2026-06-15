<script setup>
import { ref, computed } from 'vue';

const props = defineProps({
  columns: { type: Array, default: () => [] }, // [{ key, label, sortable, width }]
  data: { type: Array, default: () => [] },
  rowKey: { type: String, default: 'id' }
});
const emit = defineEmits(['row-click', 'sort-change']);

const sortKey = ref('');
const sortDir = ref('asc'); // asc | desc

function onHeaderClick(col) {
  if (!col.sortable) return;
  if (sortKey.value === col.key) {
    sortDir.value = sortDir.value === 'asc' ? 'desc' : 'asc';
  } else {
    sortKey.value = col.key;
    sortDir.value = 'asc';
  }
  emit('sort-change', { key: col.key, dir: sortDir.value });
}

function onRowClick(row) {
  emit('row-click', row);
}

const isEmpty = computed(() => !props.data || props.data.length === 0);
</script>

<template>
  <div class="app-table">
    <div class="app-table-scroll">
      <table>
        <thead>
          <tr>
            <th
              v-for="col in columns"
              :key="col.key"
              :style="col.width ? { width: col.width } : null"
              :class="{ sortable: col.sortable, active: sortKey === col.key }"
              @click="onHeaderClick(col)"
            >
              <span class="app-table-th-label">{{ col.label }}</span>
              <span v-if="col.sortable && sortKey === col.key" class="app-table-sort">
                {{ sortDir === 'asc' ? '↑' : '↓' }}
              </span>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="(row, idx) in data"
            :key="row[rowKey] ?? idx"
            @click="onRowClick(row)"
          >
            <td v-for="col in columns" :key="col.key">
              <slot :name="`cell-${col.key}`" :row="row" :value="row[col.key]">
                {{ row[col.key] }}
              </slot>
            </td>
          </tr>
        </tbody>
      </table>
      <div v-if="isEmpty" class="app-table-empty">
        <slot name="empty">暂无数据</slot>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.app-table {
  width: 100%;
}

.app-table-scroll {
  max-height: 100%;
  overflow: auto;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  background: var(--app-panel);
}

table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--text-sm);
}

thead {
  position: sticky;
  top: 0;
  background: var(--app-panel-2);
  z-index: var(--z-sticky);
}

th {
  padding: 8px 12px;
  text-align: left;
  font-weight: 500;
  color: var(--app-muted);
  border-bottom: 1px solid var(--app-border);
  white-space: nowrap;
  user-select: none;
}
th.sortable {
  cursor: pointer;
}
th.sortable:hover {
  color: var(--app-strong);
}
th.active {
  color: var(--accent);
}

.app-table-th-label {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.app-table-sort {
  font-size: var(--text-xs);
}

td {
  padding: 8px 12px;
  color: var(--app-text);
  border-bottom: 1px solid var(--app-border);
}

tbody tr {
  cursor: default;
  transition: background var(--motion-fast) var(--ease-standard);
}
tbody tr:hover {
  background: var(--app-hover);
}
tbody tr:last-child td {
  border-bottom: none;
}

.app-table-empty {
  padding: var(--space-6);
  text-align: center;
  color: var(--app-subtle);
  font-size: var(--text-sm);
}
</style>
