<script setup lang="ts">
/**
 * EditorTabBar — 编辑器多文件 tab 条（v0.18）。
 * 样式照 TerminalTabs 精简：dirty 圆点 + 关闭 ✕；遵守 hover 铁律
 * （只改绘制属性，常驻占位）。
 */
import { X } from 'lucide-vue-next';
import type { EditorTab } from '@/types/editor';

defineProps<{
  tabs: EditorTab[];
  activeTabId: string | null;
}>();

const emit = defineEmits<{
  (e: 'select', id: string): void;
  (e: 'close', id: string): void;
  (e: 'hide'): void;
}>();
</script>

<template>
  <div class="editor-tabbar" data-region="editor-tabbar">
    <div class="tabs" role="tablist">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        type="button"
        role="tab"
        class="tab"
        :class="{ active: tab.id === activeTabId }"
        :title="tab.target.path"
        @click="emit('select', tab.id)"
      >
        <span class="tab-kind" :class="tab.target.kind">{{ tab.target.kind === 'remote' ? '远' : '本' }}</span>
        <span class="tab-label">{{ tab.name }}</span>
        <span class="tab-dirty" :class="{ on: tab.dirty }" aria-hidden="true"></span>
        <span
          class="tab-close"
          role="button"
          aria-label="关闭"
          @click.stop="emit('close', tab.id)"
        >
          <X :size="12" />
        </span>
      </button>
    </div>
    <button type="button" class="hide-btn" title="收起编辑器（tab 保留）" @click="emit('hide')">收起</button>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.editor-tabbar {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 0 var(--space-2);
  min-height: 34px;
  border-bottom: 1px solid var(--app-border);
  background: var(--app-panel);
}

.tabs {
  display: flex;
  align-items: stretch;
  gap: 2px;
  overflow-x: auto;
  min-width: 0;
  flex: 1 1 auto;
}

.tab {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  padding: 4px var(--space-2);
  border: 1px solid transparent;
  border-radius: var(--radius-sm) var(--radius-sm) 0 0;
  background: transparent;
  color: var(--app-muted);
  font-size: var(--text-xs);
  cursor: pointer;
  white-space: nowrap;
  max-width: 220px;

  &:hover {
    background: var(--app-hover);
    color: var(--app-text);
  }

  &.active {
    background: var(--app-hover);
    border-color: var(--app-border);
    border-bottom-color: transparent;
    color: var(--app-strong);
  }
}

.tab-kind {
  flex: 0 0 auto;
  font-size: 10px;
  line-height: 1;
  padding: 2px 3px;
  border-radius: var(--radius-sm);
  background: var(--app-hover);
  color: var(--app-muted);

  &.remote {
    color: var(--accent);
    background: var(--accent-soft);
  }
}

.tab-label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

// dirty 圆点：常驻占位（hover 铁律），默认透明
.tab-dirty {
  flex: 0 0 auto;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: transparent;
  transition: background-color var(--dur-fast) var(--ease-standard);

  &.on {
    background: var(--warn);
  }
}

// 关闭按钮常驻占位，hover 只改颜色
.tab-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border-radius: var(--radius-sm);
  color: var(--app-muted);
  opacity: 0.6;
  transition: opacity var(--dur-fast) var(--ease-standard), background-color var(--dur-fast) var(--ease-standard);

  &:hover {
    opacity: 1;
    background: var(--app-hover);
    color: var(--app-strong);
  }
}

.hide-btn {
  flex: 0 0 auto;
  padding: 2px var(--space-2);
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--app-muted);
  font-size: var(--text-xs);
  cursor: pointer;

  &:hover {
    background: var(--app-hover);
    color: var(--app-text);
  }
}
</style>
