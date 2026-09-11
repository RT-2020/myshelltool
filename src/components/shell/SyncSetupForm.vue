<script setup lang="ts">
/**
 * SyncSetupForm — 「创建备份」/「从已有备份恢复」表单（v2.7 重设计）。
 *
 * 关键改动：**一次只问一件事**。
 *   - 默认（新建）：设一个主密码 → [创建备份]。主密码是你给这份备份定的钥匙，
 *     系统无法替你生成（它同时也是换机恢复的唯一凭据）。
 *   - 点「从已有备份恢复」才出现 Gist ID，并要求填**原来那台电脑上的**主密码；
 *     恢复模式不要"确认密码"——解密成功本身就是校验，多一个字段只是多一次误判机会。
 *
 * 术语纪律：界面上只出现「主密码」「Gist ID」两个专有名词，不再出现
 * 会话密钥/DPAPI/载荷 这类实现词（那些属于高级设置与文档）。
 */
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { ArrowLeft, CloudDownload, CloudUpload, WandSparkles } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';
import { useSyncRecoveryPassword } from '@/composables/useSyncRecoveryPassword';
import { useClipboard } from '@/composables/useClipboard';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';
import PatConfigCard from '@/components/shell/PatConfigCard.vue';

const store = useWorkbenchStore();
const { syncLoading, githubPatConfigured } = storeToRefs(store);
const { generate } = useSyncRecoveryPassword();
const { copy } = useClipboard();

const password = ref('');
const passwordConfirm = ref('');
const gistId = ref('');
/** 恢复模式：默认关闭，用户显式选择后才出现 Gist ID —— 默认视图只有一个字段。 */
const restoring = ref(false);
const showAccount = ref(false);
/** 本次由「生成强密码」产生的密码（用于明示一次 + 复制）；改用自己的密码时清空。 */
const generated = ref('');
const generating = ref(false);

const mismatch = computed(() => Boolean(password.value && passwordConfirm.value && password.value !== passwordConfirm.value));
const canSubmit = computed(() => {
  if (password.value.length < 6 || mismatch.value) return false;
  if (!githubPatConfigured.value) return false;
  return restoring.value ? gistId.value.trim().length > 0 : true;
});

function toggleRestoring() {
  restoring.value = !restoring.value;
  gistId.value = '';
  passwordConfirm.value = '';
  generated.value = '';
}

/** 生成高熵密码：后端生成 + 存本机 DPAPI（换机时可查看/复制），同时填入两个字段。 */
async function onGenerate() {
  generating.value = true;
  try {
    const pw = await generate();
    if (!pw) return;
    generated.value = pw;
    password.value = pw;
    passwordConfirm.value = pw; // 自动生成必然一致，不让用户再抄一遍
    await copy(pw); // 顺手进剪贴板：用户想存到别处可以直接粘贴
    store.announce('已生成强密码并保存在这台电脑（已复制到剪贴板）');
  } finally {
    generating.value = false;
  }
}

async function copyGenerated() {
  if (generated.value) await copy(generated.value);
}

async function onSubmit() {
  if (!canSubmit.value) return;
  const result = await store.syncSetup(password.value, restoring.value ? gistId.value.trim() : '');
  if (!result) return; // 失败：store 已 announce/flash
  password.value = '';
  passwordConfirm.value = '';
  gistId.value = '';
  generated.value = '';
}
</script>

<template>
  <section class="setup-card">
    <header class="head">
      <component :is="restoring ? CloudDownload : CloudUpload" :size="13" />
      <span>{{ restoring ? '从已有备份恢复' : '创建加密备份' }}</span>
    </header>

    <p class="note">
      <template v-if="restoring">
        填<strong>原来那台电脑上设置的主密码</strong>。Gist 里只有密文，主密码不上传，所以恢复只能靠它。
      </template>
      <template v-else>
        给这份备份设一个主密码（<strong>不是 GitHub 密码</strong>）。没头绪就点「生成强密码」——
        生成的那份保存在这台电脑上，换机时可查看/复制；你自己输入的密码不会被保存。
      </template>
    </p>

    <label v-if="restoring" class="field">
      <span class="field-label">Gist ID</span>
      <AppInput v-model="gistId" placeholder="在 GitHub 上那份 Gist 的 ID" />
    </label>

    <label class="field">
      <span class="field-label">
        {{ restoring ? '原主密码' : '主密码' }} <em class="req">至少 6 位</em>
      </span>
      <AppInput v-model="password" type="password" placeholder="主密码" @keyup.enter="onSubmit" />
    </label>

    <template v-if="!restoring">
      <label class="field">
        <span class="field-label">确认主密码</span>
        <AppInput v-model="passwordConfirm" type="password" placeholder="再输一次" />
      </label>
      <p v-if="mismatch" class="error-inline">两次输入不一致</p>

      <!-- 顺序：先动作、后结果（结果块紧跟在生成按钮之后，读者不需要往下找） -->
      <AppButton
        class="generate-btn"
        variant="subtle"
        size="sm"
        :disabled="generating"
        @click="onGenerate"
      >
        <WandSparkles :size="12" />{{ generated ? '换一个密码' : '生成强密码' }}
      </AppButton>

      <!-- 生成的密码在这里明示一次：它会被保存在本机，不是"看不见的黑盒" -->
      <div v-if="generated" class="generated">
        <span class="generated-label">已生成并保存在这台电脑</span>
        <code class="generated-code">{{ generated }}</code>
        <button type="button" class="link-btn" @click="copyGenerated">复制</button>
      </div>
    </template>

    <div class="actions">
      <AppButton variant="primary" size="sm" :disabled="!canSubmit || syncLoading" data-sync-setup @click="onSubmit">
        {{ restoring ? '从云端恢复' : '创建备份' }}
      </AppButton>
      <button type="button" class="link-btn" @click="toggleRestoring">
        <ArrowLeft v-if="restoring" :size="12" />{{ restoring ? '改为创建新备份' : '从已有备份恢复' }}
      </button>
    </div>

    <!-- 账号信息属于配置，折叠：默认视图只留"设密码 → 创建"这一条路径 -->
    <button type="button" class="link-btn is-quiet" @click="showAccount = !showAccount">
      {{ showAccount ? '收起账号' : '账号' }}
    </button>
    <div v-if="showAccount" class="account-slot">
      <PatConfigCard />
    </div>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.setup-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  background: var(--app-panel);
}

.head {
  display: flex;
  align-items: center;
  gap: 6px;
  padding-bottom: var(--space-1);
  border-bottom: 1px solid var(--app-border-soft);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--app-muted);
}
.head :deep(svg) { flex-shrink: 0; }

.note {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.6;
  color: var(--app-muted);
}
.note strong { color: var(--app-strong); }

.field { display: flex; flex-direction: column; gap: var(--space-1); }
.field-label { font-size: var(--text-xs); color: var(--app-muted); }
.req {
  font-style: normal;
  font-family: var(--font-mono);
  font-size: 11px;
  background: var(--app-hover);
  padding: 0 4px;
  border-radius: var(--radius-sm);
}
.error-inline { margin: 0; font-size: var(--text-xs); color: var(--danger); }

// 列方向 flex 默认 align-items: stretch，会把按钮拉满宽并居中显示 —— 显式靠左
.generate-btn { align-self: flex-start; }

// 生成结果明示一次（等宽可选中，方便用户自己再抄一份）
.generated {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-wrap: wrap;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--accent-soft);
}
.generated-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  color: var(--accent);
}
.generated-code {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--app-strong);
  user-select: all;
  word-break: break-all;
}

.actions {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  flex-wrap: wrap;
  margin-top: var(--space-1);
}
.actions :deep(svg) { flex-shrink: 0; }

.link-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  background: none;
  border: none;
  padding: 0;
  color: var(--accent);
  font: inherit;
  font-size: var(--text-xs);
  cursor: pointer;
}
.link-btn:hover { color: var(--accent-hover); text-decoration: underline; }
.link-btn:focus-visible { outline: none; box-shadow: var(--focus-ring); border-radius: var(--radius-sm); }
.link-btn.is-quiet { color: var(--app-muted); align-self: flex-start; }
.link-btn.is-quiet:hover { color: var(--app-strong); }

.account-slot {
  padding: var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
}
</style>
