<script setup lang="ts">
/**
 * SyncAutoSyncControl — 「这台电脑免密」设置行（v2.7 重设计）。
 *
 * 从前是一个自带标题/说明/按钮的卡片；现在只作为 SyncAdvancedSettings 折叠区里的
 * **一行**：状态 + 一句解释 + 一个动作。理由：免密在日常视图里不该占位置
 * （默认已由 setup/push 自动开启），它属于"配置"，不属于"每天看的读数"。
 *
 * 语义（说人话）：把「会话密钥 + DPAPI + 自动同步」这堆实现词翻译成用户能理解的两件事：
 * ① 这台电脑不用再输密码；② 改动会自动备份。安全代价在关闭按钮旁一句话说清。
 */
import { ref } from 'vue';
import { storeToRefs } from 'pinia';
import { ShieldCheck, Zap, ZapOff } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';

const store = useWorkbenchStore();
const { syncAutoSyncEnabled, syncLoading } = storeToRefs(store);

const autoSyncPassword = ref('');
const showSetup = ref(false);

async function onEnable() {
  if (!autoSyncPassword.value) return;
  const ok = await store.syncEnableAutoSync(autoSyncPassword.value);
  if (ok) {
    autoSyncPassword.value = '';
    showSetup.value = false;
  }
}
</script>

<template>
  <section class="setting-row" :class="{ 'is-on': syncAutoSyncEnabled }">
    <div class="row-main">
      <span class="row-title">
        <component :is="syncAutoSyncEnabled ? ShieldCheck : ZapOff" :size="13" />
        这台电脑免密
        <em v-if="syncAutoSyncEnabled" class="badge">已开启</em>
      </span>
      <p class="note">
        <template v-if="syncAutoSyncEnabled">
          推拉免密 + 改动自动推送。密钥由 Windows DPAPI 保护、仅本机可用；换机或重装后用主密码重新开启。
        </template>
        <template v-else>
          推拉免密 + 改动自动备份。只需输<strong>一次</strong>主密码，本机用它派生并保存密钥（主密码本身不落盘）。
        </template>
      </p>
    </div>

    <div v-if="!syncAutoSyncEnabled" class="row-action">
      <template v-if="showSetup">
        <AppInput
          v-model="autoSyncPassword"
          type="password"
          placeholder="主密码（输一次）"
          @keyup.enter="onEnable"
        />
        <div class="btn-row">
          <AppButton variant="primary" size="sm" :disabled="!autoSyncPassword || syncLoading" @click="onEnable">
            <Zap :size="12" />开启免密
          </AppButton>
          <AppButton variant="ghost" size="sm" @click="showSetup = false">取消</AppButton>
        </div>
      </template>
      <AppButton v-else variant="subtle" size="sm" @click="showSetup = true">
        <Zap :size="12" />开启
      </AppButton>
    </div>

    <button
      v-else
      type="button"
      class="link-btn"
      :disabled="syncLoading"
      @click="store.syncDisableAutoSync()"
    >
      取消免密（恢复每次输主密码）
    </button>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.setting-row {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}
.row-main {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.row-title {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--app-strong);
}
.row-title :deep(svg) { flex-shrink: 0; color: var(--app-muted); }
.setting-row.is-on .row-title :deep(svg) { color: var(--success); }
.badge {
  font-style: normal;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.04em;
  padding: 1px 6px;
  border-radius: var(--radius-pill);
  color: var(--success);
  background: var(--success-soft);
}
.note {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.6;
  color: var(--app-muted);
}
.note strong { color: var(--app-strong); }

.row-action {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}
.btn-row { display: flex; gap: var(--space-2); flex-wrap: wrap; }

.link-btn {
  align-self: flex-start;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  background: none;
  border: none;
  padding: 0;
  color: var(--app-muted);
  font: inherit;
  font-size: var(--text-xs);
  cursor: pointer;
}
.link-btn:hover { color: var(--danger); }
.link-btn:focus-visible { outline: none; box-shadow: var(--focus-ring); border-radius: var(--radius-sm); }
.link-btn :deep(svg) { flex-shrink: 0; }
</style>
