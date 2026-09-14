<script setup lang="ts">
/**
 * FileOpFormsContent — 文件操作四表单（mkdir / localMkdir / rename / localRename）
 * 的弹窗体（GlobalModals 拆分第二刀）。触发逻辑在 FileSurface，仅表单渲染于此。
 *
 * 表单值由父组件持有（submitModal 分发读值），reactive 对象 props 就地编辑。
 */
import AppInput from '@/components/ui/AppInput.vue';

export type FileOpFormKind = 'mkdir' | 'localMkdir' | 'rename' | 'localRename';

defineProps<{
  kind: FileOpFormKind;
  /** mkdir/localMkdir：展示目标父路径 */
  basePath?: string;
  /** rename/localRename：共享的改名单（path/current/next） */
  renameTarget: { path: string; current: string; next: string };
  fileForm: { mkdirName: string };
}>();
</script>

<template>
  <div v-if="kind === 'mkdir' || kind === 'localMkdir'" class="stack">
    <label class="stack"><span>目录名</span>
      <AppInput :model-value="fileForm.mkdirName" placeholder="new-folder"
        @update:model-value="v => fileForm.mkdirName = v" />
    </label>
    <p class="muted">将在当前{{ kind === 'mkdir' ? '远程' : '本地' }}路径下创建：{{ basePath }}</p>
  </div>
  <div v-else class="stack">
    <label class="stack"><span>新名称</span>
      <AppInput :model-value="renameTarget.next"
        @update:model-value="v => renameTarget.next = v" />
    </label>
    <p class="muted">原名称：{{ renameTarget.current }}</p>
  </div>
</template>
