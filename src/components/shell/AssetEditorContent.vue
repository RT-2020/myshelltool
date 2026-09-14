<script setup lang="ts">
/**
 * AssetEditorContent — 资产编辑弹窗的表单体（GlobalModals 第一轮拆分）。
 *
 * 职责边界：只渲染 + 就地编辑表单对象；校验、保存、提交流程仍在
 * GlobalModals 的 submitModal（弹窗 footer 的共享「确认」按钮驱动）。
 *
 * props 传 reactive 表单对象并就地改属性（先例：AssetPrivateKeyInput 的
 * `:asset`/`:credential`——传引用改属性不触发 props 重新赋值警告，
 * 父组件的 submit 读到的就是最新值）。
 */
import { computed } from 'vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';
import AppSelect from '@/components/ui/AppSelect.vue';
import AssetPrivateKeyInput from '@/components/shell/AssetPrivateKeyInput.vue';
import type { AssetEditorForm, CredentialEditorForm } from '@/types/domain';

const props = defineProps<{
  asset: AssetEditorForm;
  credential: CredentialEditorForm;
  /** 分组下拉选项（父组件已有 groupOptions，复用不重复实现） */
  groupOptions: { label: string; value: string }[];
  submitting: boolean;
  /** 保存前校验错误（父组件 submitModal 校验后置位，此处只展示） */
  formError: string;
}>();

// 认证方式选项。Token 认证后端支持存疑，不加（follow-up：确认 save_credential
// / ssh_connect 的 token 语义后再补选项）。
const authMethodOptions = [
  { label: 'Password', value: 'Password' },
  { label: 'PrivateKey', value: 'PrivateKey' }
];

const credentialHint = computed(() => {
  if (!props.asset.id) return '新连接，凭据与密钥将在保存时加密存储到安全保管箱';
  if (props.asset.auth_method === 'PrivateKey') {
    if (props.asset.private_key_credential_id) return '私钥已在安全保管箱托管（支持跨设备端到端加密同步）';
    if (props.asset.private_key_path) return '当前使用物理文件路径，建议导入到保管箱以便换机同步';
    return '尚未配置私钥';
  }
  if (props.asset.credential_id) return '密码已存储（重新输入会覆盖）';
  return '尚未存储密码';
});

// 标记清除已存凭据（保存时生效；再次点击撤销；重新输入也会撤销）
function toggleClearPassword() {
  props.credential.clearPassword = !props.credential.clearPassword;
}

function toggleClearPassphrase() {
  props.credential.clearPassphrase = !props.credential.clearPassphrase;
}
</script>

<template>
  <div class="stack">
    <div class="grid-2">
      <label class="stack"><span class="muted">名称</span>
        <AppInput :model-value="asset.name" @update:model-value="v => asset.name = v" data-asset-field="name" />
      </label>
      <label class="stack"><span class="muted">主机</span>
        <AppInput :model-value="asset.host" @update:model-value="v => asset.host = v" data-asset-field="host" />
      </label>
      <label class="stack"><span class="muted">端口</span>
        <AppInput :model-value="asset.port" type="number" @update:model-value="v => asset.port = v" data-asset-field="port" />
      </label>
      <label class="stack"><span class="muted">用户名</span>
        <AppInput :model-value="asset.username" @update:model-value="v => asset.username = v" data-asset-field="username" />
      </label>
      <label class="stack"><span class="muted">分组</span>
        <AppSelect
          :model-value="asset.group || '未分组'"
          :options="groupOptions"
          @update:model-value="v => asset.group = String(v)"
          data-asset-field="group"
        />
      </label>
      <label class="stack"><span class="muted">标签</span>
        <AppInput :model-value="asset.tags" placeholder="prod, app" @update:model-value="v => asset.tags = v" data-asset-field="tags" />
      </label>
      <label class="stack"><span class="muted">认证方式</span>
        <AppSelect :model-value="asset.auth_method" :options="authMethodOptions"
          @update:model-value="v => asset.auth_method = v" />
      </label>
    </div>
    <div class="callout">
      <strong>凭据与密钥安全</strong>
      <p class="muted">{{ credentialHint }}</p>
    </div>
    <p v-if="formError" class="form-error">{{ formError }}</p>

    <!-- Password 认证模式 -->
    <div v-if="asset.auth_method === 'Password'" class="grid-2">
      <label class="stack">
        <span class="muted">密码（明文不会回显，仅保存到本地安全存储）
          <span v-if="!asset.credential_id" style="color:var(--danger)"> · 首次保存必填</span>
        </span>
        <div class="inline-field-row">
          <AppInput :model-value="credential.password" type="password" placeholder="首次保存必填；编辑时留空保留既有密码"
            @update:model-value="v => { credential.password = v; credential.clearPassword = false; }" data-asset-field="password" />
          <AppButton v-if="asset.credential_id" size="sm" variant="danger" @click="toggleClearPassword">
            {{ credential.clearPassword ? '撤销清除' : '清除密码' }}
          </AppButton>
        </div>
      </label>
    </div>

    <!-- PrivateKey 认证模式（私钥托管箱 + 本地路径 + 口令） -->
    <div v-else-if="asset.auth_method === 'PrivateKey'" class="stack">
      <AssetPrivateKeyInput
        :asset="asset"
        :credential="credential"
        :disabled="submitting"
      />
      <label class="stack" style="max-width: 480px;">
        <span class="muted">Passphrase（可选，无口令保护的私钥留空）</span>
        <div class="inline-field-row">
          <AppInput :model-value="credential.passphrase" type="password" placeholder="无加密私钥留空"
            @update:model-value="v => { credential.passphrase = v; credential.clearPassphrase = false; }" data-asset-field="passphrase" />
          <AppButton v-if="asset.passphrase_credential_id" size="sm" variant="danger" @click="toggleClearPassphrase">
            {{ credential.clearPassphrase ? '撤销清除' : '清除 passphrase' }}
          </AppButton>
        </div>
      </label>
    </div>
  </div>
</template>
