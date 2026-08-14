<script setup>
import { X } from 'lucide-vue-next';

defineProps({
  open: { type: Boolean, default: false }
});
const emit = defineEmits(['close']);

const groups = [
  {
    title: '会话',
    items: [
      { keys: ['Ctrl', 'Shift', 'T'], label: '连接所选主机' },
      { keys: ['Ctrl', 'Tab'], label: '切换到下一个会话' },
      { keys: ['Ctrl', 'Shift', 'Tab'], label: '切换到上一个会话' },
      { keys: ['Ctrl', 'W'], label: '关闭当前会话' }
    ]
  },
  {
    title: '编辑',
    items: [
      { keys: ['Ctrl', 'Shift', 'C'], label: '复制选中文本' },
      { keys: ['Ctrl', 'Shift', 'V'], label: '粘贴（危险命令会弹确认）' },
      { keys: ['Ctrl', 'Shift', 'F'], label: '搜索终端内容' }
    ]
  },
  {
    title: '字体',
    items: [
      { keys: ['Ctrl', '='], label: '字体增大' },
      { keys: ['Ctrl', '-'], label: '字体减小' },
      { keys: ['Ctrl', '0'], label: '字体重置' },
      { keys: ['Ctrl', '滚轮'], label: '随滚轮缩放' }
    ]
  },
  {
    title: '视图',
    items: [
      { keys: ['Alt', 'Enter'], label: '切换全屏' },
      { keys: ['Ctrl', 'Shift', 'P'], label: '命令面板' },
      { keys: ['?'], label: '本速查表' },
      { keys: ['Esc'], label: '关闭浮层' }
    ]
  }
];
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="shortcut-overlay" @click.self="emit('close')" role="dialog" aria-modal="true" aria-label="快捷键速查">
      <div class="shortcut-modal">
        <header class="shortcut-header">
          <h2>快捷键速查</h2>
          <button class="icon-btn" aria-label="关闭" @click="emit('close')"><X :size="18" /></button>
        </header>
        <div class="shortcut-grid">
          <section v-for="group in groups" :key="group.title" class="shortcut-group">
            <h3>{{ group.title }}</h3>
            <dl>
              <template v-for="item in group.items" :key="item.label">
                <dt>
                  <kbd v-for="(k, idx) in item.keys" :key="idx">{{ k }}</kbd>
                </dt>
                <dd>{{ item.label }}</dd>
              </template>
            </dl>
          </section>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.shortcut-overlay {
  position: fixed;
  inset: 0;
  background: color-mix(in oklab, var(--app-scrim) 60%, transparent);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: var(--z-modal);
  backdrop-filter: blur(4px);
}

.shortcut-modal {
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-lg);
  box-shadow: var(--elev-raised);
  width: min(720px, 92vw);
  max-height: 80vh;
  overflow: auto;
  padding: var(--space-5);
}

.shortcut-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-block-end: var(--space-4);
}

.shortcut-header h2 {
  margin: 0;
  font-size: var(--text-lg);
  color: var(--app-strong);
}

.shortcut-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: var(--space-4);
}

.shortcut-group h3 {
  font-size: var(--text-xs);
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--app-muted);
  margin: 0 0 var(--space-2);
}

.shortcut-group dl {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px var(--space-3);
  margin: 0;
}

.shortcut-group dt {
  display: flex;
  gap: 4px;
  align-items: center;
}

.shortcut-group dd {
  margin: 0;
  color: var(--app-text);
  font-size: var(--text-sm);
}

kbd {
  display: inline-block;
  padding: 2px 6px;
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  background: var(--app-panel-2);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  box-shadow: 0 1px 0 var(--app-border-strong);
  color: var(--app-strong);
}
</style>
