<script setup>
defineProps({});
</script>

<template>
  <div class="app-status-bar">
    <div class="app-status-cell app-status-left">
      <slot name="left" />
    </div>
    <div class="app-status-cell app-status-center">
      <slot name="center" />
    </div>
    <div class="app-status-cell app-status-right">
      <slot name="right" />
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.app-status-bar {
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  align-items: center;
  height: 28px;
  padding: 0 var(--space-3);
  background: var(--app-chrome);
  border-top: 1px solid var(--app-border);
  font-size: var(--text-xs);
  color: var(--app-muted);
}

.app-status-cell {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}

.app-status-left {
  justify-content: flex-start;
}
.app-status-center {
  justify-content: center;
}
.app-status-right {
  justify-content: flex-end;
}
</style>
