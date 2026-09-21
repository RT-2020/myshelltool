<script setup lang="ts">
/**
 * TunnelCreateContent — 新建隧道弹窗体（GlobalModals 第一轮拆分）。
 *
 * 表单状态由父组件持有（submitModal 的 tunnelCreate 分支做端口数字化后
 * 调 store.createTunnel），经 props 传 reactive 对象就地编辑。
 * 元素 id（tunnelName 等）保留：UI 测试按 id 定位。
 */
import AppInput from '@/components/ui/AppInput.vue';
import AppSelect from '@/components/ui/AppSelect.vue';
import type { TunnelEditorForm } from '@/types/domain';

defineProps<{
  form: TunnelEditorForm;
}>();

const tunnelKindOptions = [
  { label: 'Local（本地端口转发）', value: 'local' },
  { label: 'Dynamic SOCKS', value: 'dynamic' },
  { label: 'Remote（远程端口转发）', value: 'remote' }
];
</script>

<template>
  <div class="stack">
    <label class="stack"><span>名称</span>
      <AppInput :model-value="form.name" id="tunnelName" placeholder="mysql-local"
        @update:model-value="v => form.name = v" />
    </label>
    <label class="stack"><span>类型</span>
      <AppSelect :model-value="form.kind" :options="tunnelKindOptions"
        @update:model-value="v => form.kind = v" />
    </label>
    <label class="stack"><span>本地地址</span>
      <AppInput :model-value="form.local_addr" id="tunnelLocalAddr"
        @update:model-value="v => form.local_addr = v" />
    </label>
    <label class="stack"><span>本地端口</span>
      <AppInput :model-value="form.local_port" id="tunnelLocalPort" type="number" placeholder="13306"
        @update:model-value="v => form.local_port = v" />
    </label>
    <div v-if="form.kind !== 'dynamic'" id="tunnelRemoteFields">
      <label class="stack"><span>远程地址</span>
        <AppInput :model-value="form.remote_addr" id="tunnelRemoteAddr" placeholder="10.10.9.32"
          @update:model-value="v => form.remote_addr = v" />
      </label>
      <label class="stack"><span>远程端口</span>
        <AppInput :model-value="form.remote_port" id="tunnelRemotePort" type="number" placeholder="3306"
          @update:model-value="v => form.remote_port = v" />
      </label>
    </div>
    <label><input v-model="form.auto_start" type="checkbox" id="tunnelAutoStart" /> 自动启动</label>
  </div>
</template>
