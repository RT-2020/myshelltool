<script setup lang="ts">
/**
 * ReauthPasswordContent — 认证失败快捷重输密码弹窗体（GlobalModals 第一轮拆分）。
 *
 * 表单状态（password/error）由父组件持有（submitModal 的 reauthPassword 分支
 * 校验并读值），经 props 传 reactive 对象就地编辑——先例同 AssetEditorContent。
 */
import AppInput from '@/components/ui/AppInput.vue';
import type { ModalState } from '@/types/domain';

defineProps<{
  asset: NonNullable<ModalState['asset']>;
  form: { password: string; error: string };
}>();
</script>

<template>
  <div class="stack">
    <p class="muted">
      连接 <strong>{{ asset.name }}</strong>（{{ asset.username }}@{{ asset.host }}）认证失败。
      输入登录密码后将切换为密码认证并自动重连。
    </p>
    <label class="stack"><span class="muted">密码</span>
      <AppInput :model-value="form.password" type="password" placeholder="服务器登录密码"
        @update:model-value="v => form.password = v" data-asset-field="reauth_password" />
    </label>
    <p v-if="form.error" class="form-error">{{ form.error }}</p>
  </div>
</template>
