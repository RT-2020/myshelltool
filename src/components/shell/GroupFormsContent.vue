<script setup lang="ts">
/**
 * GroupFormsContent — 分组管理三表单（renameGroup / createGroup / moveAsset）的
 * 弹窗体（GlobalModals 拆分第二刀）。
 *
 * 表单值与校验错误由父组件持有（submitModal 分发校验并读值），经 reactive
 * 对象 props 就地编辑——先例同 AssetEditorContent。
 */
import AppInput from '@/components/ui/AppInput.vue';
import AppCombobox from '@/components/ui/AppCombobox.vue';

export type GroupFormKind = 'renameGroup' | 'createGroup' | 'moveAsset';

defineProps<{
  kind: GroupFormKind;
  /** renameGroup：被改名的分组完整路径（modal.path） */
  path?: string;
  /** moveAsset：被移动资产名（仅展示） */
  assetName?: string;
  groupOptions: { label: string; value: string }[];
  inputs: { rename: string; create: string; move: string };
  formError: string;
}>();
</script>

<template>
  <div class="stack">
    <template v-if="kind === 'renameGroup'">
      <p class="muted">输入分组「{{ path }}」的新完整路径，可用「/」调整层级
        （如「生产/数据库」）；其子级分组与资产随行。</p>
      <label class="stack"><span>新路径</span>
        <AppInput :model-value="inputs.rename" placeholder="生产/数据库"
          @update:model-value="v => inputs.rename = v" />
      </label>
    </template>
    <template v-else-if="kind === 'createGroup'">
      <p class="muted">输入分组路径，可用「/」创建多级嵌套分组，如「生产/数据库」。</p>
      <label class="stack"><span>分组路径</span>
        <AppInput :model-value="inputs.create" placeholder="生产/数据库"
          @update:model-value="v => inputs.create = v" />
      </label>
    </template>
    <template v-else>
      <p>移动连接「<strong>{{ assetName }}</strong>」到分组：</p>
      <label class="stack"><span>目标分组</span>
        <!-- 可选可输：直输新分组路径即建组（保存时后端收口声明） -->
        <AppCombobox
          :model-value="inputs.move || '未分组'"
          :options="groupOptions"
          placeholder="选择或输入，如 生产/数据库"
          @update:model-value="v => inputs.move = v"
        />
      </label>
    </template>
    <p v-if="formError" class="form-error">{{ formError }}</p>
  </div>
</template>
