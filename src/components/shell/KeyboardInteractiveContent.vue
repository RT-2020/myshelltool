<script setup lang="ts">
/**
 * KeyboardInteractiveContent — 键盘交互认证（keyboardInteractive）弹窗体。
 *
 * 10 A11：每个服务器 prompt 默认遮显，旁侧提供「显示 / 隐藏」切换，
 * 不靠猜测文本是否属于秘密字段，把核对权交给用户。
 */
import { computed, reactive, watch } from 'vue';
import AppInput from '@/components/ui/AppInput.vue';
import type { KeyboardInteractivePayload } from '@/types/domain';

const props = defineProps<{
  prompt: KeyboardInteractivePayload | null;
  instructions: string;
  responses: Record<string, string>;
}>();

const revealed = reactive<Record<number, boolean>>({});
const promptLabels = computed(() =>
  (props.prompt?.prompts || []).map(item => (typeof item === 'string' ? item : item?.text || ''))
);

watch(
  () => props.prompt,
  () => {
    Object.keys(revealed).forEach((k) => delete revealed[Number(k)]);
  }
);
</script>

<template>
  <div class="stack">
    <p class="muted">服务器需要键盘交互认证，请根据提示输入：</p>
    <p v-if="prompt?.name"><strong>{{ prompt.name }}</strong></p>
    <p v-if="instructions" class="muted">{{ instructions }}</p>
    <label v-for="(item, idx) in promptLabels" :key="idx" class="stack">
      <span class="muted">{{ item }}</span>
      <span class="secret-row">
        <AppInput
          :model-value="responses[idx]"
          :type="revealed[idx] ? 'text' : 'password'"
          :placeholder="item"
          @update:model-value="v => responses[idx] = v"
        />
        <button
          type="button"
          class="secret-toggle"
          :aria-pressed="Boolean(revealed[idx])"
          :aria-label="(revealed[idx] ? '隐藏' : '显示') + item"
          @click="revealed[idx] = !revealed[idx]"
        >
          {{ revealed[idx] ? '隐藏' : '显示' }}
        </button>
      </span>
    </label>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_form-feedback' as *;

.secret-row {
  @include secret-row-control;

  :deep(.app-input) {
    flex: 1 1 0;
    min-width: 0;
  }
}
</style>
