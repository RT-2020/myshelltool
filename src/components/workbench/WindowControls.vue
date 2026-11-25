<script setup lang="ts">
/**
 * WindowControls — 标题栏无边框窗口三键（最小化 / 最大化还原 / 关闭），01 A10。
 * WorkbenchShell 与 AssetWindowShell 共用；关闭动作由父壳各自决定（直关 vs 确认守卫）。
 */
import { Minus, Square, X } from 'lucide-vue-next';

withDefaults(
  defineProps<{
    visible?: boolean;
    isMaximized?: boolean;
  }>(),
  {
    visible: false,
    isMaximized: false
  }
);

const emit = defineEmits<{
  minimize: [];
  'toggle-maximize': [];
  close: [];
}>();
</script>

<template>
  <div v-if="visible" class="window-controls" data-no-drag="true" aria-label="窗口控制">
    <button class="window-btn" type="button" aria-label="最小化" title="最小化" @click="emit('minimize')">
      <Minus :size="14" />
    </button>
    <button
      class="window-btn"
      type="button"
      :aria-label="isMaximized ? '还原窗口' : '最大化'"
      :title="isMaximized ? '还原窗口' : '最大化'"
      @click="emit('toggle-maximize')"
    >
      <Square :size="12" />
    </button>
    <button class="window-btn danger" type="button" aria-label="关闭" title="关闭" @click="emit('close')">
      <X :size="14" />
    </button>
  </div>
</template>
