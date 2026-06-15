<script setup>
import AppTab from './AppTab.vue';

const props = defineProps({
  tabs: { type: Array, default: () => [] }, // [{ id, label, icon }]
  active: { type: [String, Number], default: '' }
});
const emit = defineEmits(['update:active']);

function onSelect(id) {
  emit('update:active', id);
}
</script>

<template>
  <div class="app-tab-group" role="tablist">
    <AppTab
      v-for="tab in tabs"
      :key="tab.id"
      :id="tab.id"
      :label="tab.label"
      :icon="tab.icon"
      :active="tab.id === active"
      @click="onSelect(tab.id)"
    />
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.app-tab-group {
  display: flex;
  align-items: center;
  gap: 2px;
  border-bottom: 1px solid var(--app-border);
}
</style>
