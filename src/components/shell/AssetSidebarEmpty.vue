<script setup lang="ts">
/**
 * AssetSidebarEmpty — 资产侧栏空态与不可用态三视图（02 #4/#5/#9/#10）。
 *
 * 三互斥视图：
 *   1. unavailable：后端资产读取失败（不以「尚未添加」掩盖故障，亦不提供无效重试钮）；
 *   2. empty：首次使用、确实零资产（提示新增连接后会按分组聚合，无虚假 <kbd>+</kbd>）；
 *   3. no-match：搜索筛选零结果（回显关键词 + 放宽建议 + 「清除筛选」恢复按钮）。
 */
import { AlertTriangle, SearchX, Server } from 'lucide-vue-next';

defineProps<{
  mode: 'unavailable' | 'empty' | 'no-match';
  searchQuery?: string;
}>();

const emit = defineEmits<{
  'create-asset': [];
  'clear-search': [];
}>();
</script>

<template>
  <div v-if="mode === 'unavailable'" class="sb-empty sb-empty--warn" role="status">
    <div class="sb-empty-icon sb-empty-icon--warn" aria-hidden="true">
      <AlertTriangle :size="22" />
    </div>
    <div class="sb-empty-title">连接资产暂时不可用</div>
    <div class="sb-empty-desc">
      读取本地资产数据失败，已保存的连接数据不受影响。详见底部状态栏错误信息，重启应用后重试。
    </div>
  </div>

  <div v-else-if="mode === 'empty'" class="sb-empty">
    <div class="sb-empty-icon" aria-hidden="true">
      <Server :size="22" />
    </div>
    <div class="sb-empty-title">尚未添加连接资产</div>
    <div class="sb-empty-desc">新增连接后会按分组聚合到此处。</div>
    <button type="button" class="btn-primary" @click="emit('create-asset')">添加第一个连接</button>
  </div>

  <div v-else class="sb-empty" role="status">
    <div class="sb-empty-icon" aria-hidden="true">
      <SearchX :size="20" />
    </div>
    <div class="sb-empty-title">无匹配「{{ searchQuery }}」的连接</div>
    <div class="sb-empty-desc">试试更短的关键词，或按分组、标签、主机、用户筛选。</div>
    <button type="button" class="btn-ghost" @click="emit('clear-search')">清除筛选</button>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.sb-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  padding: var(--space-8) var(--space-4);
  margin: var(--space-2) var(--space-1);
  text-align: center;
  border: 1px dashed var(--app-border-strong);
  border-radius: var(--radius-sm);
}
.sb-empty-icon {
  width: 40px;
  height: 40px;
  display: grid;
  place-items: center;
  border-radius: var(--radius-md);
  background: var(--app-panel-2);
  border: 1px solid var(--app-border);
  color: var(--app-muted);
}
.sb-empty-icon--warn {
  color: var(--warn);
  border-color: color-mix(in oklab, var(--warn), transparent 60%);
  background: var(--warn-soft);
}
.sb-empty-title {
  font-size: var(--text-sm);
  font-weight: 500;
  color: var(--app-strong);
}
.sb-empty-desc {
  font-size: var(--text-xs);
  color: var(--app-muted);
  line-height: 1.5;
}
.btn-primary {
  height: 30px;
  padding: 0 14px;
  background: var(--accent);
  color: var(--accent-on);
  border: 0;
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  font-weight: 500;
  cursor: pointer;
  margin-top: var(--space-1);
  transition: background var(--motion-fast);
}
.btn-primary:hover { background: var(--accent-hover); }
.btn-primary:active { background: var(--accent-active); }

.btn-ghost {
  height: 28px;
  padding: 0 var(--space-3);
  background: transparent;
  color: var(--app-text);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  font-weight: 500;
  cursor: pointer;
  margin-top: var(--space-1);
  transition: background var(--motion-fast), border-color var(--motion-fast);
}
.btn-ghost:hover {
  background: var(--app-hover);
  border-color: var(--app-border-strong);
}
.btn-ghost:active {
  background: var(--app-active);
}
</style>
