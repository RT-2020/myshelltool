<script setup lang="ts">
/**
 * PatConfigCard — GitHub token 配置卡片（Device Flow 登录 + 手动 PAT 兜底）。
 *
 * v1.8 从 GlobalModals.vue 的 tokenConfig 内联表单抽出，供：
 *   1. GlobalModals 的 tokenConfig modal（保留原 data-* hook 兼容）
 *   2. 设置面板「同步」tab 内嵌
 *
 * v2.x 顶部新增「GitHub 账号登录」区块（OAuth Device Flow，useGithubDeviceLogin
 * 状态机）：idle/success 单按钮入口；code/pending 设备码面板（验证码 + 复制 +
 * 打开授权页 + 倒计时）；denied/expired/error 提示重试。
 *
 * 下方手动 PAT 表单（AppInput + 保存/清除）原样保留为兜底，data-* hook 一字不改。
 */
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { Check, Copy, ExternalLink, Github } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';
import { useGithubDeviceLogin } from '@/composables/useGithubDeviceLogin';
import { useClipboard } from '@/composables/useClipboard';
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

// ─── GitHub Device Flow 登录（后端 sync_oauth.rs）───
const { phase, userCode, verificationUri, countdownSec, errorMessage, start, openVerificationPage } =
  useGithubDeviceLogin();
const { copy } = useClipboard();

// code/pending 共用设备码面板（code = 请求设备码中的短暂过渡态）
const deviceFlowActive = computed(() => phase.value === 'code' || phase.value === 'pending');

const countdownText = computed(() => {
  const total = Math.max(0, Number(countdownSec.value) || 0);
  const mm = String(Math.floor(total / 60)).padStart(2, '0');
  const ss = String(total % 60).padStart(2, '0');
  return `${mm}:${ss}`;
});

const failureText = computed(() => {
  if (phase.value === 'denied') return '你在 GitHub 授权页拒绝了本次授权，可重新发起登录。';
  if (phase.value === 'expired') return '验证码已过期，请重新发起登录。';
  if (errorMessage.value) return `登录失败：${errorMessage.value}`;
  return '登录失败，请重试。';
});

async function copyUserCode() {
  // useClipboard 已三层 fallback（navigator → Tauri 插件 → execCommand），失败无需打断
  await copy(userCode.value);
}
</script>

<template>
  <div class="pat-card stack">
    <!-- GitHub Device Flow 登录 -->
    <section class="oauth-block stack">
      <header class="oauth-head"><Github :size="12" />GitHub 账号登录</header>

      <!-- idle / success：单按钮入口 -->
      <div v-if="phase === 'idle' || phase === 'success'" class="oauth-entry">
        <AppButton variant="primary" size="sm" @click="start">
          <Github :size="12" />{{ phase === 'success' ? '重新登录' : '登录 GitHub' }}
        </AppButton>
        <span v-if="phase === 'success'" class="oauth-ok"><Check :size="12" />已登录，token 已写入本地安全存储</span>
        <span v-else class="muted">浏览器内完成授权，无需手动粘贴 token</span>
      </div>

      <!-- code / pending：设备码面板 -->
      <div v-else-if="deviceFlowActive" class="stack">
        <div class="device-code-row">
          <code class="device-code">{{ userCode || '…' }}</code>
          <AppButton variant="subtle" size="sm" :disabled="!userCode" @click="copyUserCode">
            <Copy :size="12" />复制
          </AppButton>
        </div>
        <p class="muted">在打开的 GitHub 授权页输入上方验证码并确认。等待授权中…（{{ countdownText }} 后过期）</p>
        <AppButton variant="subtle" size="sm" @click="openVerificationPage">
          <ExternalLink :size="12" />打开浏览器授权
        </AppButton>
      </div>

      <!-- denied / expired / error -->
      <div v-else class="stack">
        <p class="muted">{{ failureText }}</p>
        <AppButton variant="primary" size="sm" @click="start">重新登录</AppButton>
      </div>
    </section>

    <!-- 手动 PAT 兜底（原样保留） -->
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

// ─── GitHub Device Flow 登录区块（block 范式照 SyncPatGuide.vue）───
.oauth-block {
  gap: var(--space-2);
  padding: var(--space-3);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  background: var(--app-panel);
}
.oauth-head {
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
.oauth-head :deep(svg) { flex-shrink: 0; }

.oauth-entry {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-wrap: wrap;
}
.oauth-ok {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: var(--text-xs);
  color: var(--success);
}
.oauth-ok :deep(svg) { flex-shrink: 0; }

.device-code-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-wrap: wrap;
}
.device-code {
  font-family: var(--font-mono);
  font-size: var(--text-lg);
  font-weight: 600;
  letter-spacing: 0.12em;
  color: var(--app-strong);
  background: var(--app-hover);
  padding: 2px 10px;
  border-radius: var(--radius-sm);
}
</style>
