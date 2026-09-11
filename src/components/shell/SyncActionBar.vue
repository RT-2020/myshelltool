<script setup lang="ts">
/**
 * SyncActionBar — 面板的唯二动作（推送 / 拉取）+ 仅在必要时出现的主密码行。
 *
 * 设计取舍：
 * - **主密码行默认不出现**：免密开启时它是纯粹的干扰（要用户先判断"要不要输"）。
 *   只有在未免密时才占一行，并直接说清"输一次就够"。
 * - **按钮主次由状态决定**，不是固定样式：本机有改动 → 推送实心；云端有新版本 →
 *   拉取实心；两边都有改动 → 拉取实心（先拉后推是安全顺序，与 rail 的提示一致）；
 *   已同步 → 两个都安静（不制造"必须做点什么"的错觉）。
 * - **不因"没输密码"置灰**：灰按钮不给解释，只会得到"点了没反应"的反馈；
 *   这里让它可点，点下去由文案指路。
 */
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { ArrowDownToLine, ArrowUpFromLine } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';
import { useSyncStore } from '@/stores/sync';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';

const store = useWorkbenchStore();
const { syncLoading, syncAutoSyncEnabled, syncConfigured } = storeToRefs(store);
const syncStore = useSyncStore();
const { localHasChanges, remoteHasUpdates, activeOp } = storeToRefs(syncStore);

const opPassword = ref('');
const needsPassword = computed(() => !syncAutoSyncEnabled.value);

/** 该由谁当主操作：见文件头注释。 */
const primaryAction = computed<'push' | 'pull' | null>(() => {
  if (localHasChanges.value && !remoteHasUpdates.value) return 'push';
  if (remoteHasUpdates.value || localHasChanges.value) return 'pull';
  return null;
});

const pushing = computed(() => syncLoading.value && activeOp.value === 'push');
const pulling = computed(() => syncLoading.value && activeOp.value === 'pull');

async function onPush() {
  if (needsPassword.value && !opPassword.value) {
    store.announce('先输入主密码：只需这一次，推送成功后这台电脑就免密了', { level: 'warn' });
    return;
  }
  const result = await store.syncPush(opPassword.value);
  if (result) opPassword.value = '';
}

async function onPull() {
  if (needsPassword.value && !opPassword.value) {
    store.announce('先输入主密码：只需这一次，拉取成功后这台电脑就免密了', { level: 'warn' });
    return;
  }
  const result = await store.syncPull(opPassword.value);
  if (!result) return;
  opPassword.value = '';
}
</script>

<template>
  <section v-if="syncConfigured" class="actions-card">
    <label v-if="needsPassword" class="pw-row">
      <span class="pw-label">主密码</span>
      <AppInput
        v-model="opPassword"
        type="password"
        placeholder="输入主密码"
        data-sync-op-password
        @keyup.enter="primaryAction === 'push' ? onPush() : onPull()"
      />
    </label>
    <p v-if="needsPassword" class="pw-hint">
      输一次即可：之后这台电脑免密推拉（换机时用主密码恢复）。
    </p>

    <div class="row">
      <AppButton
        :variant="primaryAction === 'push' ? 'primary' : 'subtle'"
        size="sm"
        :disabled="syncLoading"
        data-sync-push
        @click="onPush"
      >
        <ArrowUpFromLine :size="13" />{{ pushing ? '推送中…' : '推送到云端' }}
      </AppButton>
      <AppButton
        :variant="primaryAction === 'pull' ? 'primary' : 'subtle'"
        size="sm"
        :disabled="syncLoading"
        data-sync-pull
        @click="onPull"
      >
        <ArrowDownToLine :size="13" />{{ pulling ? '拉取中…' : '从云端拉取' }}
      </AppButton>
    </div>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// 动作块：同样不自带外框 —— 它和上面的读数同属一块「同步面板」（.sync-surface），
// 由父级用一条 hairline 分隔"读数"与"操作"。
.actions-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  background: var(--app-panel);
}

.pw-row {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}
.pw-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  color: var(--app-muted);
}

.pw-hint {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.5;
  color: var(--app-muted);
}
.pw-hint strong { color: var(--app-strong); }
// 已免密时这句是纯确认，压低存在感
.pw-hint.is-quiet { color: var(--app-subtle); }

.row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-wrap: wrap;
}
.row :deep(svg) { flex-shrink: 0; }
</style>
