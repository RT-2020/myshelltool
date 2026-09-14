<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import type { Component } from 'vue';
import AppTab from './AppTab.vue';

/** tab 条目契约（AppTabGroup 消费方按此形状传入）。 */
interface TabItem {
  id: string | number;
  label: string;
  icon?: Component | null;
}

const props = withDefaults(
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

// —— 滑动 active 指示条 ——
// DOM 顺序与 props.tabs 一致，按 index 取按钮，不用 id 拼选择器（避免 CSS.escape 问题）
const containerRef = ref<HTMLElement | null>(null);
const indicatorX = ref(0);
const indicatorWidth = ref(0);
// 是否已成功定位过：false 时指示条隐藏（无 active / 未挂载）
const measured = ref(false);
// 是否开启动画：首次定位那一帧禁用过渡，防弹窗打开时指示条从左侧滑入
const ready = ref(false);
let resizeObserver: ResizeObserver | null = null;

const indicatorStyle = computed(() => ({
  transform: `translateX(${indicatorX.value}px)`,
  width: `${indicatorWidth.value}px`,
  opacity: measured.value ? 1 : 0,
  // 首次定位那一帧显式 transition:none；ready 后交给 CSS 类的 transition
  transition: ready.value ? undefined : 'none'
}));

function updateIndicator() {
  const container = containerRef.value;
  if (!container) return;
  const index = props.tabs.findIndex(t => t.id === props.active);
  const buttons = container.querySelectorAll<HTMLElement>('[role="tab"]');
  const button = index >= 0 && index < buttons.length ? buttons[index] : null;
  if (!button) {
    measured.value = false;
    return;
  }
  indicatorX.value = button.offsetLeft;
  indicatorWidth.value = button.offsetWidth;
  if (!measured.value) {
    measured.value = true;
    nextTick(() => { ready.value = true; }); // 下一帧才开启动画
  }
}

// 按钮 label 变化只改按钮自身宽度（不改容器宽度），故容器与每个按钮都要观察
function bindResizeObserver() {
  resizeObserver?.disconnect();
  const container = containerRef.value;
  if (!container || typeof ResizeObserver === 'undefined') return;
  resizeObserver = new ResizeObserver(() => updateIndicator());
  resizeObserver.observe(container);
  container.querySelectorAll<HTMLElement>('[role="tab"]').forEach(el => resizeObserver?.observe(el));
}

onMounted(() => nextTick(updateIndicator));
watch(() => props.active, () => nextTick(updateIndicator));
// 消费方 TransferDrawer 的 tabs 是 computed，label 含计数会变 → 重绑观察 + 重定位
watch(() => props.tabs, () => nextTick(() => { bindResizeObserver(); updateIndicator(); }));
onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  resizeObserver = null;
});
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
    <span class="app-tab-indicator" aria-hidden="true" :style="indicatorStyle" />
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.app-tab-group {
  // relative 作指示条与按钮 offsetLeft 的定位基准（offsetParent）
  position: relative;
  display: flex;
  align-items: center;
  gap: 2px;
  border-bottom: 1px solid var(--app-border);
}
.app-tab-indicator {
  position: absolute;
  bottom: 0;
  left: 0;
  height: 2px;
  background: var(--accent);
  border-radius: 2px 2px 0 0;
  pointer-events: none;
  transition: transform var(--motion-base) var(--ease-standard),
    width var(--motion-base) var(--ease-standard);
}
</style>
