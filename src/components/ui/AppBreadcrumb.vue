<script setup>
import { computed } from 'vue';
import { ChevronRight } from 'lucide-vue-next';

const props = defineProps({
  items: { type: Array, default: () => [] } // [{ name, path }]
});
const emit = defineEmits(['navigate']);

const lastIndex = computed(() => props.items.length - 1);

function onClick(item, idx) {
  if (idx === lastIndex.value) return;
  emit('navigate', item.path);
}
</script>

<template>
  <nav class="app-breadcrumb" aria-label="Breadcrumb">
    <template v-for="(item, idx) in items" :key="idx">
      <span
        class="app-breadcrumb-item"
        :class="{ current: idx === lastIndex }"
        @click="onClick(item, idx)"
      >{{ item.name }}</span>
      <ChevronRight
        v-if="idx < lastIndex"
        :size="12"
        class="app-breadcrumb-sep"
      />
    </template>
  </nav>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.app-breadcrumb {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: var(--text-xs);
  color: var(--app-muted);
  flex-wrap: nowrap;
  overflow: hidden;
}

.app-breadcrumb-item {
  cursor: pointer;
  white-space: nowrap;
  padding: 2px 4px;
  border-radius: var(--radius-sm);
  transition: color var(--motion-fast) var(--ease-standard),
    background var(--motion-fast) var(--ease-standard);
}
.app-breadcrumb-item:hover:not(.current) {
  color: var(--app-strong);
  text-decoration: underline;
}
.app-breadcrumb-item.current {
  color: var(--app-strong);
  cursor: default;
  font-weight: 500;
}

.app-breadcrumb-sep {
  color: var(--app-subtle);
  flex: 0 0 auto;
}
</style>
