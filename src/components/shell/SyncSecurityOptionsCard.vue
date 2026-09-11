<script setup lang="ts">
/**
 * SyncSecurityOptionsCard — 「密码与密钥一并备份」设置行（v2.7 重设计）。
 *
 * 与 SyncAutoSyncControl 同样的收敛：从"独立卡片 + 大段解释"变成折叠区里的**一行**。
 * 文案只回答用户真正在权衡的问题：「换机之后我的连接密码还在不在」，以及
 * 「关掉会不会更安全」——而不是罗列算法名（算法是事实，但不该是这一行的主角）。
 */
import { storeToRefs } from 'pinia';
import { KeyRound, ShieldCheck } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';

const store = useWorkbenchStore();
const { syncCredentialsEnabled, syncLoading } = storeToRefs(store);

async function onToggle(event: Event) {
  await store.setSyncCredentialsEnabled((event.target as HTMLInputElement).checked);
}
</script>

<template>
  <section class="setting-row">
    <label class="toggle">
      <input
        type="checkbox"
        :checked="syncCredentialsEnabled"
        :disabled="syncLoading"
        data-sync-credentials
        @change="onToggle"
      />
      <span class="toggle-body">
        <span class="toggle-title">
          <KeyRound :size="13" />密码与密钥一并备份
          <em v-if="syncCredentialsEnabled" class="badge"><ShieldCheck :size="10" />已开启</em>
        </span>
        <span class="note">
          <template v-if="syncCredentialsEnabled">
            主机密码与托管私钥随资产一起加密上传，新机器恢复后可直接连接。关闭后只同步主机列表。
          </template>
          <template v-else>
            只备份主机列表（IP、端口、用户名）。换机后需要重新输入每台服务器的密码。
          </template>
        </span>
      </span>
    </label>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.setting-row { display: flex; flex-direction: column; gap: var(--space-2); }

.toggle {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  cursor: pointer;
  user-select: none;

  input[type='checkbox'] {
    margin-top: 2px;
    cursor: pointer;
    accent-color: var(--accent);
    flex-shrink: 0;
  }
  input[type='checkbox']:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }
}
.toggle-body {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}
.toggle-title {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--app-strong);
}
.toggle-title :deep(svg) { flex-shrink: 0; color: var(--app-muted); }
.badge {
  display: inline-flex;
  align-items: center;
  gap: 3px;
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
  font-size: var(--text-xs);
  line-height: 1.6;
  color: var(--app-muted);
}
</style>
