<script setup>
defineProps({});
</script>

<template>
  <div class="app-status-bar statusbar">
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
  display: flex;
  align-items: center;
  justify-content: space-between;
  align-items: center;
  width: 100%;
  height: var(--statusbar-h);
  padding: 0 12px;
  background: var(--app-chrome);
  border-top: 1px solid var(--app-border);
  font: 11.5px var(--font-mono);
  color: var(--app-muted);
  overflow: hidden;
}

.app-status-cell {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.app-status-left {
  justify-content: flex-start;
  flex: 1 1 0;
}
.app-status-center {
  justify-content: center;
  flex: 0 0 auto;
}
.app-status-right {
  justify-content: flex-end;
  flex: 1 1 0;
}
</style>
