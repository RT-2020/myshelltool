<script setup>
/**
 * SyncAutoSyncControl — v1.6 自动同步开关子组件。
 *
 * 在 SyncPanelContent 的「日常管理」视图中渲染（同步已配置后）。
 * 抽成独立组件因 SyncPanelContent.vue 加自动同步 UI 后超 500 行 SFC 硬上限
 * （AGENTS.md 红线），参照 SyncPatGuide / McpCapabilityList 的拆分先例。
 *
 * 单一职责：自动同步的启用/关闭交互。
 * - 启用：弹一次性主密码输入（验证后派生会话密钥即丢弃）→ store.syncEnableAutoSync
 * - 关闭：直接禁用（删会话密钥）→ store.syncDisableAutoSync
 *
 * 视觉语言照父组件的 block 范式，零新增 token（AGENTS.md 红线：用 var(--token)）。
 */
import { ref } from 'vue';
import { storeToRefs } from 'pinia';
import { Zap, ZapOff, ShieldCheck } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench.js';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';

const store = useWorkbenchStore();
const { syncAutoSyncEnabled, syncLoading } = storeToRefs(store);

// 启用自动同步的一次性主密码输入（验证后派生会话密钥即丢弃）
const autoSyncPassword = ref('');
const showAutoSyncSetup = ref(false);

async function onEnableAutoSync() {
  if (autoSyncPassword.value.length < 1) return;
  const ok = await store.syncEnableAutoSync(autoSyncPassword.value);
  if (ok) {
    autoSyncPassword.value = '';
    showAutoSyncSetup.value = false;
  }
}

async function onDisableAutoSync() {
  await store.syncDisableAutoSync();
}
</script>

<template>
  <section class="block auto-sync-block" :class="{ 'is-on': syncAutoSyncEnabled }">
    <header class="block-head">
      <component :is="syncAutoSyncEnabled ? Zap : ZapOff" :size="12" />自动同步
    </header>
    <template v-if="!syncAutoSyncEnabled">
      <p class="block-note muted">
        启用后，资产增删改时<strong>自动推送</strong>到 Gist，无需每次输主密码。
        会派生一个加密密钥用 <strong>DPAPI</strong>（绑定 Windows 用户）保护后存本地，主密码仍不落盘。
      </p>
      <div v-if="showAutoSyncSetup" class="reset-form">
        <label class="field">
          <span class="field-label">主密码（验证后派生密钥）</span>
          <AppInput v-model="autoSyncPassword" type="password" placeholder="输入主密码" />
        </label>
        <div class="actions">
          <AppButton variant="primary" size="sm" :disabled="!autoSyncPassword || syncLoading" @click="onEnableAutoSync">
            <Zap :size="12" />启用自动同步
          </AppButton>
          <AppButton variant="ghost" size="sm" @click="showAutoSyncSetup = false">取消</AppButton>
        </div>
      </div>
      <button v-else class="link-btn" @click="showAutoSyncSetup = true">
        <Zap :size="12" />启用自动同步
      </button>
    </template>
    <template v-else>
      <p class="block-note muted">
        <ShieldCheck :size="12" class="icon-ok" />已启用——资产变更会自动推送。
        远端有更新时顶部会显示徽章提示，点击「拉取」合并。
      </p>
      <button class="link-btn danger-link" :disabled="syncLoading" @click="onDisableAutoSync">
        <ZapOff :size="12" />关闭自动同步
      </button>
    </template>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// ─── block 通用（照父组件 SyncPanelContent / McpPanelContent 范式）───
.block {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  background: var(--app-panel);
}
.auto-sync-block.is-on {
  border-color: color-mix(in oklab, var(--success), transparent 55%);
  background: color-mix(in oklab, var(--success), transparent 93%);
}
.auto-sync-block.is-on .block-head { color: var(--success); }
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
.block-head :deep(svg) { flex-shrink: 0; }
.block-note {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.6;
}
.block-note strong { color: var(--app-strong); }
.icon-ok { color: var(--success); flex-shrink: 0; vertical-align: -2px; }

// ─── 通用字段 ───
.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.field-label {
  font-size: var(--text-xs);
  color: var(--app-muted);
}
.actions { display: flex; gap: var(--space-2); flex-wrap: wrap; align-items: center; }
.actions :deep(svg) { vertical-align: -2px; }

// ─── 折叠表单 / 链接按钮 ───
.reset-form {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin-top: var(--space-2);
}
.link-btn {
  background: transparent;
  border: none;
  padding: 0;
  color: var(--app-muted);
  font: inherit;
  font-size: var(--text-xs);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.link-btn:hover { color: var(--app-strong); }
.danger-link:hover { color: var(--danger); }

.muted { color: var(--app-muted); font-size: var(--text-xs); line-height: 1.5; margin: 0; }
.muted strong { color: var(--app-strong); }
</style>
