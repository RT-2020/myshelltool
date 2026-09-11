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
 * v2.x 信息架构重设计：OAuth 登录是主路径，手动 PAT 表单默认折叠进
 * 「手动粘贴 PAT（高级）」，展开才渲染（表单元素与 data-* hook 一字不改）。
 */
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { Check, Copy, ExternalLink, Github, RefreshCw } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';
import { useGithubDeviceLogin } from '@/composables/useGithubDeviceLogin';
import { useClipboard } from '@/composables/useClipboard';
import AppInput from '@/components/ui/AppInput.vue';
import AppButton from '@/components/ui/AppButton.vue';

const store = useWorkbenchStore();
const { githubPatConfigured } = storeToRefs(store);

/**
 * `flat`：嵌在已有容器里（如「设置与高级」折叠区）时去掉自带卡片边框/背景，
 * 避免"卡片套卡片"；默认形态（独立展示）保持原样。
 */
withDefaults(defineProps<{ flat?: boolean }>(), { flat: false });

const tokenInput = ref('');

// 手动 PAT 表单折叠开关：OAuth 登录是主路径，手动粘贴降级为高级选项
const showManualPat = ref(false);

function saveToken() {
  store.saveToken(tokenInput.value).then(saved => {
    if (saved) tokenInput.value = '';
  });
}

// ─── GitHub Device Flow 登录（后端 sync_oauth.rs）───
const { phase, userCode, verificationUri, countdownSec, errorMessage, retrying, retryCount, retryReason, start, openVerificationPage } =
  useGithubDeviceLogin();
const { copy } = useClipboard();

// code/pending 共用设备码面板（code = 请求设备码中的短暂过渡态）
const deviceFlowActive = computed(() => phase.value === 'code' || phase.value === 'pending');

/**
 * 是否已登录 —— 以**本地安全存储里的 token**为权威（v2.7）。
 *
 * 此前只看本次会话的 phase：重启后 phase 回到 'idle'，卡片就显示主按钮「登录 GitHub」，
 * 用户以为"每次打开都要重新登录"（token 其实一直躺在 SecretStore 里，今天还能看到
 * `github-pat.cred`）。现在 phase==='idle' 且已配置 token 时按已登录展示，只留一个
 * 次级的「重新登录」入口。
 */
const loggedIn = computed(() => phase.value === 'success' || (phase.value === 'idle' && githubPatConfigured.value));

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
  <div class="pat-card stack" :class="{ 'is-flat': flat }">
    <!-- GitHub Device Flow 登录 -->
    <section class="oauth-block stack">
      <header class="oauth-head"><Github :size="12" />GitHub 账号登录</header>

      <!-- idle / success：单按钮入口（已登录时以「本地安全存储」为权威，不再催登录） -->
      <div v-if="phase === 'idle' || phase === 'success'" class="oauth-entry">
        <AppButton :variant="loggedIn ? 'subtle' : 'primary'" size="sm" @click="start">
          <Github :size="12" />{{ loggedIn ? '重新登录' : '登录 GitHub' }}
        </AppButton>
        <span v-if="loggedIn" class="oauth-ok">
          <Check :size="12" />已登录，token 存在本机安全存储（重启无需再登录）
        </span>
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
        <!-- 传输故障降级态：自动退避重试中，验证码仍有效，不必重新发起 -->
        <p v-if="retrying" class="muted retry-note" data-oauth-retrying>
          <RefreshCw :size="12" />网络不稳定，正在自动重试（第 {{ retryCount }} 次）：{{ retryReason }}
        </p>
        <div class="device-code-actions">
          <AppButton variant="subtle" size="sm" @click="openVerificationPage">
            <ExternalLink :size="12" />打开浏览器授权
          </AppButton>
          <AppButton variant="subtle" size="sm" @click="start">
            <RefreshCw :size="12" />重新发起登录
          </AppButton>
        </div>
      </div>

      <!-- denied / expired / error -->
      <div v-else class="stack">
        <p class="muted failure-note">{{ failureText }}</p>
        <AppButton variant="primary" size="sm" @click="start">重新登录</AppButton>
      </div>
    </section>

    <!-- 手动 PAT 兜底（默认折叠：OAuth 是主路径；原 SyncPatGuide 的指引职责收敛为下方 hint） -->
    <button class="link-btn" @click="showManualPat = !showManualPat">
      {{ showManualPat ? '收起手动 PAT' : '手动粘贴 PAT（高级）' }}
    </button>
    <div v-if="showManualPat" class="manual-pat">
      <p class="muted">到 github.com/settings/tokens 手动生成，仅勾 gist 权限。token 仅写入本地安全存储，界面提交后只展示“已配置”或“未配置”。</p>
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

// ─── 手动 PAT 折叠区（v2.x：折叠开关照 SyncPanelContent 的 link-btn 范式）───
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
.manual-pat {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

// ─── GitHub Device Flow 登录区块（block 范式照 SyncPanelContent）───
.oauth-block {
  gap: var(--space-2);
  padding: var(--space-3);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  background: var(--app-panel);
}
// flat：嵌在折叠区/设置行内部时不再自成一张卡（避免卡片套卡片），只保留内容与分隔线
.pat-card.is-flat .oauth-block {
  padding: 0;
  border: none;
  border-radius: 0;
  background: none;
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
.device-code-actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-wrap: wrap;
}
// 传输故障降级提示（自动重试中）：低调但可读，不打断等待
.retry-note {
  display: flex;
  align-items: flex-start;
  gap: 4px;
  color: var(--warn);
  word-break: break-word;
}
.retry-note :deep(svg) { flex-shrink: 0; margin-top: 2px; }

// 失败文案含传输层 cause 链（可能较长且无空格），必须允许折行
.failure-note { word-break: break-word; }
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
