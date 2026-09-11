<script setup lang="ts">
/**
 * SyncAdvancedSettings — 「设置与高级」折叠区：把低频配置收进一个入口。
 *
 * 为什么合并（信息架构改动）：此前免密、凭据同步、账号、重置主密码、换机说明、清空
 * 是 5-6 个并列卡片，把"该按哪个按钮"的主任务淹在配置里。日常只需要状态 + 两个按钮，
 * 配置是一次性的 —— 折叠后默认视图只剩状态、动作、这一个入口。
 *
 * 内容顺序按"改动风险"排：无害开关在上（免密 / 密码与密钥），账号与恢复居中，
 * 清空（逃生口）永远最后且要二次确认。
 *
 * 子组件复用：SyncAutoSyncControl（免密开关）、SyncSecurityOptionsCard（凭据开关）、
 * PatConfigCard（账号）原样嵌入，不复制逻辑；两者已改为"设置行"样式以适配折叠区。
 */
import { ref } from 'vue';
import { storeToRefs } from 'pinia';
import { ChevronDown, ChevronRight, ArrowLeftRight, KeyRound, Settings2 } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';
import { useSyncStore } from '@/stores/sync';
import { useSyncRecoveryPassword } from '@/composables/useSyncRecoveryPassword';
import { useClipboard } from '@/composables/useClipboard';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';
import SyncAutoSyncControl from '@/components/shell/SyncAutoSyncControl.vue';
import SyncSecurityOptionsCard from '@/components/shell/SyncSecurityOptionsCard.vue';
import PatConfigCard from '@/components/shell/PatConfigCard.vue';

const store = useWorkbenchStore();
const { syncLoading } = storeToRefs(store);
const syncStore = useSyncStore();
const { recoveryPasswordSaved } = storeToRefs(syncStore);
const { reveal } = useSyncRecoveryPassword();
const { copy } = useClipboard();

const open = ref(false);
const showReset = ref(false);
const resetOldPassword = ref('');
const resetNewPassword = ref('');
const confirmingClear = ref(false);
/** 已展开的恢复密码明文：只在这个组件内短暂存在，不进 store、不落 localStorage。 */
const revealed = ref('');

/** 点「查看」才读一次；失败时 composable 已 toast，这里保持空白而不是假装有值。 */
async function onReveal() {
  const pw = await reveal();
  if (pw) revealed.value = pw;
}

async function copyRevealed() {
  if (revealed.value) {
    await copy(revealed.value);
    store.announce('恢复密码已复制到剪贴板');
  }
}

async function onResetPassword() {
  if (!resetOldPassword.value || resetNewPassword.value.length < 6) return;
  const ok = await store.syncResetMasterPassword(resetOldPassword.value, resetNewPassword.value);
  if (ok === undefined) return; // store 失败返回 undefined，flashMessage 已通知
  resetOldPassword.value = '';
  resetNewPassword.value = '';
  showReset.value = false;
}

async function onClearSync() {
  await store.syncClear();
  confirmingClear.value = false;
}
</script>

<template>
  <section class="adv" :class="{ 'is-open': open }">
    <button
      type="button"
      class="adv-head"
      :aria-expanded="open"
      data-sync-advanced
      @click="open = !open"
    >
      <component :is="open ? ChevronDown : ChevronRight" :size="13" />
      <Settings2 :size="13" />
      <span>设置与高级</span>
      <span class="adv-note">免密 · 密码与密钥 · 账号 · 换机 · 清空</span>
    </button>

    <div v-if="open" class="adv-body">
      <!-- ① 免密（改风险最低的开关放最前） -->
      <SyncAutoSyncControl />

      <!-- ② 密码与密钥一并备份 -->
      <SyncSecurityOptionsCard />

      <!-- ③ 账号与主密码 -->
      <section class="row-block">
        <header class="row-head"><KeyRound :size="12" />GitHub 账号与主密码</header>
        <PatConfigCard flat />
        <button class="link-btn" type="button" @click="showReset = !showReset">
          {{ showReset ? '收起重置主密码' : '重置主密码' }}
        </button>
        <div v-if="showReset" class="reset-form">
          <label class="field">
            <span class="field-label">当前主密码</span>
            <AppInput v-model="resetOldPassword" type="password" />
          </label>
          <label class="field">
            <span class="field-label">新主密码 <em class="req">至少 6 位</em></span>
            <AppInput v-model="resetNewPassword" type="password" />
          </label>
          <AppButton
            variant="ghost"
            size="sm"
            :disabled="!resetOldPassword || resetNewPassword.length < 6 || syncLoading"
            @click="onResetPassword"
          >重置并用新密码重新推送</AppButton>
        </div>
      </section>

      <!-- ④ 换机恢复：自成一小节（它讲的是另一台机器上的操作，不该和账号设置粘在一起） -->
      <section class="row-block">
        <header class="row-head"><ArrowLeftRight :size="12" />换机恢复</header>
        <p class="note">
          新电脑上：安装 myshelltool → 登录同一 GitHub 账号 → 在同步页点「从已有备份恢复」，
          填 Gist ID 与<strong>这份备份的主密码</strong>即可取回全部资产。
        </p>

        <!-- 应用生成的恢复密码存在本机，可随时查看/复制带走；用户自设的密码不在应用手里 -->
        <template v-if="recoveryPasswordSaved">
          <button v-if="!revealed" type="button" class="link-btn" @click="onReveal">
            查看这台电脑的恢复密码
          </button>
          <div v-else class="reveal">
            <span class="reveal-label">这份备份的主密码（应用生成，保存在这台电脑）</span>
            <code class="reveal-code">{{ revealed }}</code>
            <div class="confirm-row">
              <AppButton variant="subtle" size="sm" @click="copyRevealed">复制</AppButton>
              <AppButton variant="ghost" size="sm" @click="revealed = ''">隐藏</AppButton>
            </div>
          </div>
        </template>
        <p v-else class="note is-quiet">
          你使用的是自己设置的主密码：应用没有保存它（只保存了本机免密用的派生密钥），
          请自行记牢 —— 它是换机恢复的唯一凭据。
        </p>
      </section>

      <!-- ④ 清空（逃生口，永远最后） -->
      <section class="row-block danger-zone">
        <button v-if="!confirmingClear" class="link-btn danger-link" type="button" @click="confirmingClear = true">
          清空同步配置（忘了主密码时用）
        </button>
        <template v-else>
          <p class="note is-danger">
            确定清空？本地资产不受影响，但 GitHub 上的 Gist 需你手动删除。
          </p>
          <div class="confirm-row">
            <AppButton variant="danger" size="sm" :disabled="syncLoading" @click="onClearSync">确认清空</AppButton>
            <AppButton variant="ghost" size="sm" @click="confirmingClear = false">取消</AppButton>
          </div>
        </template>
      </section>
    </div>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.adv {
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  background: var(--app-panel-2);
  overflow: hidden;
}
.adv.is-open { background: var(--app-panel); }

.adv-head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  padding: var(--space-3);
  background: none;
  border: none;
  color: var(--app-muted);
  font: inherit;
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.04em;
  text-align: left;
  cursor: pointer;
}
.adv-head:hover { color: var(--app-strong); background: var(--app-hover); }
.adv-head:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}
.adv-head :deep(svg) { flex-shrink: 0; }
.adv-note {
  margin-left: auto;
  font-weight: 400;
  color: var(--app-subtle);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.adv-body {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: 0 var(--space-3) var(--space-3);
}

// 折叠区里的分节：不用卡片（避免卡片套卡片），只用一条 hairline 分隔。
// 统一由 `> * + *` 提供分隔线，子组件（免密行/凭据行）无需各自带边框。
.adv-body > * + * {
  padding-top: var(--space-3);
  border-top: 1px solid var(--app-border-soft);
}
.row-block {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}
.row-head {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--app-muted);
}
.row-head :deep(svg) { flex-shrink: 0; }

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
.reset-form {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
}
.confirm-row { display: flex; gap: var(--space-2); flex-wrap: wrap; }

.link-btn {
  align-self: flex-start;
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
.danger-link { color: var(--danger); }
.danger-link:hover { color: var(--danger); }

.note {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.6;
  color: var(--app-muted);
}
.note strong { color: var(--app-strong); }
.note.is-quiet { color: var(--app-subtle); }
.note.is-danger { color: var(--danger); }
.danger-zone { border-top-color: var(--app-border); }

// 恢复密码明文：等宽、可整段选中，方便用户自己另存一份
.reveal {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--accent-soft);
}
.reveal-code {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--app-strong);
  user-select: all;
  word-break: break-all;
}
.reveal-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  color: var(--accent);
}
</style>
