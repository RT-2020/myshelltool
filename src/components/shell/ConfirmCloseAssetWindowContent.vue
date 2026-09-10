<script setup lang="ts">
// confirmCloseAssetWindow 的弹窗 body（独立资产窗口关闭确认，Phase 1-A）。
// 按钮在 GlobalModals 的 modal-actions（照 confirmDelete 模式：danger 主按钮
// 「断开并关闭」走 submitModal → modal.onConfirm，次按钮「取消」走 closeModal）。
withDefaults(defineProps<{ count?: number }>(), { count: 0 });
</script>

<template>
  <p>关闭将断开 <strong>{{ count }}</strong> 个活跃会话的连接。</p>
  <p class="close-warn">断开后本窗口内未完成的终端会话与传输将中断。</p>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

p {
  margin: 0;
  color: var(--app-text);
  font-size: var(--text-sm);
}

.close-warn {
  margin-top: var(--space-2);
  padding: 8px 10px;
  border: 1px solid color-mix(in oklab, var(--danger), transparent 50%);
  border-radius: var(--radius-sm);
  background: color-mix(in oklab, var(--danger), transparent 88%);
  color: var(--danger);
  font-size: var(--text-xs);
  font-weight: 500;
}
</style>
