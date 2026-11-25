<script setup lang="ts">
/**
 * ThemeToggleButton — 三态主题切换按钮（system / light / dark），01 A5 & A10。
 * WorkbenchShell 与 AssetWindowShell 共用，消除独立窗曾缺 system 态图标的语义漂移。
 */
import { Monitor, Moon, Sun } from 'lucide-vue-next';

defineProps<{
  theme: string;
  themeLabel: string;
}>();

const emit = defineEmits<{
  toggle: [];
}>();
</script>

<template>
  <button
    class="icon-btn"
    type="button"
    :aria-label="`切换主题（当前：${themeLabel}）`"
    :title="`切换主题（当前：${themeLabel}）`"
    @click="emit('toggle')"
  >
    <Monitor v-if="theme === 'system'" />
    <Sun v-else-if="theme === 'light'" />
    <Moon v-else />
  </button>
</template>
