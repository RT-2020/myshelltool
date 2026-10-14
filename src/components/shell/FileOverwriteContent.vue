<script setup lang="ts">
/**
 * FileOverwriteContent — 冲突三选弹窗体（B2：覆盖 / 跳过 / 重命名）。
 * 按钮在 GlobalModals 的 modal-actions（取消=跳过 / 重命名 / 覆盖）；表单值
 * 由 fileOverwrite lib 的 reactive 就地编辑（父组件传入，同 FileOpForms 模式）。
 * 「应用到本批剩余冲突」勾选只在目录/多文件批次显示，且只对覆盖/跳过生效
 * （重命名始终逐条——勾选时重命名按钮在 GlobalModals 侧禁用）。
 */
import AppInput from '@/components/ui/AppInput.vue';

withDefaults(defineProps<{
  direction: 'upload' | 'download';
  path: string;
  form: { rename: string; error: string; applyAll: boolean };
  /** 所属批次存在时才显示「应用到剩余」勾选（单文件冲突无批次可应用）。 */
  hasBatch?: boolean;
}>(), { hasBatch: false });
</script>

<template>
  <div class="stack">
    <p>{{ direction === 'upload' ? '远程已存在同名文件，覆盖将替换其内容：' : '本地已存在同名文件，下载覆盖将替换该文件：' }}</p>
    <p class="num overwrite-path">{{ path }}</p>
    <label class="stack overwrite-rename">
      <span>重命名{{ direction === 'upload' ? '上传' : '下载' }}为</span>
      <AppInput
        :model-value="form.rename"
        :error="form.error"
        placeholder="新名称"
        spellcheck="false"
        @update:model-value="v => (form.rename = v)"
      />
    </label>
    <label v-if="hasBatch" class="overwrite-apply">
      <input v-model="form.applyAll" type="checkbox" />
      <span>应用到本批剩余冲突（仅覆盖/跳过；重命名仍逐个询问）</span>
    </label>
    <p class="muted">此操作不可撤销。</p>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.overwrite-rename {
  margin-top: var(--space-2);
}

.overwrite-apply {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-xs);
  color: var(--app-text-muted);

  input {
    accent-color: var(--accent);
  }
}
</style>
