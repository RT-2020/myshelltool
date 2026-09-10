<script setup lang="ts">
/**
 * SyncSecurityOptionsCard — 凭据与托管私钥安全同步配置子组件。
 *
 * 位于 SyncPanelContent 中（日常管理板块）。
 * 允许用户选择在加密同步资产时，是否一并同步密码凭据和托管私钥。
 * 所有凭据经主密码派生的 AES-256-GCM 端到端加密后才推送到 Gist。
 */
import { storeToRefs } from 'pinia';
import { KeyRound, ShieldCheck, Lock } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';

const store = useWorkbenchStore();
const { syncCredentialsEnabled, syncLoading } = storeToRefs(store);

async function onToggleCredentials(event: Event) {
  const checked = (event.target as HTMLInputElement).checked;
  await store.setSyncCredentialsEnabled(checked);
}
</script>

<template>
  <section class="block sync-security-block" :class="{ 'is-active': syncCredentialsEnabled }">
    <header class="block-head">
      <Lock :size="12" />凭据与私钥同步
    </header>
    <label class="checkbox-row">
      <input
        type="checkbox"
        :checked="syncCredentialsEnabled"
        :disabled="syncLoading"
        @change="onToggleCredentials"
      />
      <span class="checkbox-label">
        <KeyRound :size="13" class="key-icon" />
        同步登录密码与托管私钥（端到端加密）
      </span>
    </label>
    <p class="block-note muted">
      <template v-if="syncCredentialsEnabled">
        <ShieldCheck :size="12" class="icon-ok" />
        已启用：密码凭据与托管私钥将与主机资产一并经主密码（<strong>Argon2id + AES-256-GCM</strong>）高强度加密后同步，新机器拉取后即可一键直连。
      </template>
      <template v-else>
        未启用：仅同步主机资产拓扑配置（IP、端口、用户名等），不上传任何密码或私钥。换机后需手动重新配置连接凭据。
      </template>
    </p>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.block {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  background: var(--app-panel);
}

.sync-security-block.is-active {
  border-color: color-mix(in oklab, var(--app-accent, #3b82f6), transparent 60%);
  background: color-mix(in oklab, var(--app-accent, #3b82f6), transparent 95%);
}

.block-head {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-xs);
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--app-muted);
  padding-bottom: 4px;
  border-bottom: 1px solid var(--app-border);
}

.checkbox-row {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  cursor: pointer;
  user-select: none;
  font-size: var(--text-sm);
  color: var(--app-strong);

  input[type="checkbox"] {
    cursor: pointer;
    accent-color: var(--app-accent, #3b82f6);
  }
}

.checkbox-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-weight: 500;
}

.key-icon {
  color: var(--app-muted);
}

.block-note {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.6;

  strong {
    color: var(--app-strong);
  }
}

.icon-ok {
  color: var(--success);
  flex-shrink: 0;
  vertical-align: -2px;
}

.muted {
  color: var(--app-muted);
}
</style>
