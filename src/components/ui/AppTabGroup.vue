<script setup lang="ts">
import type { Component } from 'vue';
import AppTab from './AppTab.vue';

/** tab 条目契约（AppTabGroup 消费方按此形状传入）。 */
interface TabItem {
  id: string | number;
  label: string;
  icon?: Component | null;
}

withDefaults(
  defineProps<{
    tabs?: TabItem[];
    active?: string | number;
  }>(),
  {
    tabs: () => [],
    active: ''
  }
);
const emit = defineEmits<{ 'update:active': [id: string | number] }>();

function onSelect(id: string | number) {
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
