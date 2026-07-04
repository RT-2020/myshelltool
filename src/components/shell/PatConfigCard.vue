<script setup>
/**
 * PatConfigCard — GitHub Personal Access Token 配置卡片。
 *
 * v1.8 从 GlobalModals.vue 的 tokenConfig 内联表单抽出，供：
 *   1. GlobalModals 的 tokenConfig modal（保留原 data-* hook 兼容）
 *   2. 设置面板「同步」tab 内嵌
 * 抽出动机：GlobalModals.vue 已 728 行（超 Vue SFC 500 行硬上限，
 * AGENTS.md 质量红线），PAT 表单是自包含逻辑，适合独立成组件去重。
 *
 * 自包含：内部持有 input ref + 保存/清除动作，不依赖外部 modal 主按钮。
 * 两个场景都正确（settings tab 无主按钮；tokenConfig modal 主按钮改为仅关闭）。
 */
import { ref } from 'vue';
import { storeToRefs } from 'pinia';
import { useWorkbenchStore } from '@/stores/workbench.js';
import AppInput from '@/components/ui/AppInput.vue';
import AppButton from '@/components/ui/AppButton.vue';

const store = useWorkbenchStore();
const { githubPatConfigured } = storeToRefs(store);

const tokenInput = ref('');

function saveToken() {
  store.saveToken(tokenInput.value).then(saved => {
    if (saved) tokenInput.value = '';
  });
}
</script>

<template>
  <div class="pat-card stack">
    <p class="muted">token 仅写入本地安全存储。界面提交后只展示“已配置”或“未配置”。</p>
    <label class="stack">
      <span class="muted">Personal Access Token</span>
      <AppInput
        :model-value="tokenInput"
        type="password"
        data-sync-token
        placeholder="粘贴 token，保存后立即隐藏"
        @update:model-value="v => (tokenInput = v)"
        @keyup.enter="saveToken"
      />
    </label>
    <p class="muted" data-token-storage-status>本地安全存储：{{ githubPatConfigured ? '已配置' : '未配置' }}</p>
    <div class="pat-actions">
      <AppButton variant="primary" size="sm" :disabled="!tokenInput" @click="saveToken">保存 token</AppButton>
      <AppButton variant="danger" size="sm" data-delete-credential @click="store.deleteToken">清除已保存的 token</AppButton>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.pat-card {
  // .stack 由全局 _utilities 提供（stack/gap），这里只补卡片语义。
  // 视觉语言对齐 McpPanelContent/SyncPanelContent：muted 副文本 + danger 按钮。
  // 不硬编码颜色，全部走 token（已由 _base / _utilities 覆盖 .muted / .stack）。
}

.pat-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
</style>
