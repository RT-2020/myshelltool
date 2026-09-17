<script setup lang="ts">
/**
 * SyncAdvancedSettings — 「设置与高级」折叠区：一张按风险分层的「配置清单」。
 *
 * 信息架构（v2.8 重设计）：不再是 5-6 个无差别的并列区块，而是**三组分层**，
 * 每组的视觉权重对应其风险等级：
 *   ① 常规        —— 低风险开关（免密 / 密码与密钥），白卡；
 *   ② 账号与备份  —— 核心配置。GitHub 账号是一行状态（PatConfigCard 去自带标题）；
 *                    「更改主密码」「换机恢复」是两个**统一模式的可展开行**（图标+名称+chevron），
 *                    主密码从「GitHub 账号」里拆出来，和换机恢复同组（换机要填的正是它俩）；
 *   ③ 危险操作    —— 清空同步配置，红边红底下沉，二次确认。
 *
 * 统一展开模式：此前「手动PAT / 重置主密码 / 清空」是三个长得一样但内容迥异的 link，
 * 用户点之前无法预期展开什么；现统一为「可展开行」，危险操作用颜色进一步区分。
 *
 * 子组件复用：SyncAutoSyncControl / SyncSecurityOptionsCard（设置行）、
 * PatConfigCard（账号，flat + hide-header）原样嵌入组卡片，不复制逻辑。
 */
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { ChevronDown, ChevronRight, Settings2, Copy, Check, KeyRound, MonitorSmartphone, Trash2 } from 'lucide-vue-next';
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
const { recoveryPasswordSaved, status } = storeToRefs(syncStore);
const { reveal } = useSyncRecoveryPassword();
const { copy } = useClipboard();

/** 完整 Gist ID（换机恢复要填的那个）；未配置时为空。 */
const gistId = computed(() => status.value?.gist_id ?? '');

const open = ref(false);
const showReset = ref(false);
const showRestore = ref(false);
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
    store.announce('主密码已复制到剪贴板');
  }
}

/** 复制 Gist ID 后的短暂「已复制」反馈（2s 后还原）。 */
const gistCopied = ref(false);
async function copyGistId() {
  if (!gistId.value) return;
  await copy(gistId.value);
  gistCopied.value = true;
  setTimeout(() => { gistCopied.value = false; }, 2000);
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
      <!-- ══ 组① 常规：低风险开关 ══ -->
      <div class="group">
        <div class="group-title">常规</div>
        <div class="group-card">
          <SyncAutoSyncControl />
          <SyncSecurityOptionsCard />
        </div>
      </div>

      <!-- ══ 组② 账号与备份 ══ -->
      <div class="group">
        <div class="group-title">账号与备份</div>
        <div class="group-card">
          <!-- GitHub 账号：状态行（PatConfigCard 去自带标题，只留登录状态与操作主体） -->
          <PatConfigCard flat hide-header />

          <!-- 更改主密码：可展开行（统一模式） -->
          <button
            type="button"
            class="expand-row"
            :aria-expanded="showReset"
            @click="showReset = !showReset"
          >
            <KeyRound :size="13" class="expand-icon" />
            <span class="expand-name">更改主密码</span>
            <ChevronRight :size="13" class="chev" />
          </button>
          <div v-if="showReset" class="expand-body">
            <label class="field">
              <span class="field-label">当前主密码</span>
              <AppInput v-model="resetOldPassword" type="password" />
            </label>
            <label class="field">
              <span class="field-label">新主密码 <em class="req">至少 6 位</em></span>
              <AppInput v-model="resetNewPassword" type="password" />
            </label>
            <div class="btn-row">
              <AppButton
                variant="primary"
                size="sm"
                :disabled="!resetOldPassword || resetNewPassword.length < 6 || syncLoading"
                @click="onResetPassword"
              >更改并重新加密备份</AppButton>
            </div>
            <p class="note">用新主密码重新加密同一份备份，凭据不丢失。旧主密码随即失效。</p>
          </div>

          <!-- 换机恢复：可展开行（统一模式） -->
          <button
            type="button"
            class="expand-row"
            :aria-expanded="showRestore"
            @click="showRestore = !showRestore"
          >
            <MonitorSmartphone :size="13" class="expand-icon" />
            <span class="expand-name">换一台电脑怎么恢复</span>
            <ChevronRight :size="13" class="chev" />
          </button>
          <div v-if="showRestore" class="expand-body">
            <p class="note">在新电脑上：登录同一个 GitHub，点「从已有备份恢复」，填下面这两样即可取回全部资产。</p>
            <div class="restore-item">
              <span class="restore-label">Gist ID</span>
              <code class="restore-code">{{ gistId || '（未配置）' }}</code>
              <button v-if="gistId" type="button" class="link-btn" @click="copyGistId">
                <component :is="gistCopied ? Check : Copy" :size="11" />{{ gistCopied ? '已复制' : '复制' }}
              </button>
            </div>
            <div class="restore-item">
              <span class="restore-label">主密码</span>
              <template v-if="recoveryPasswordSaved">
                <template v-if="!revealed">
                  <span class="restore-value muted">应用生成的那份，已保存在这台电脑</span>
                  <button type="button" class="link-btn" @click="onReveal">查看</button>
                </template>
                <template v-else>
                  <code class="restore-code">{{ revealed }}</code>
                  <span class="confirm-row">
                    <button type="button" class="link-btn" @click="copyRevealed"><Copy :size="11" />复制</button>
                    <button type="button" class="link-btn" @click="revealed = ''">隐藏</button>
                  </span>
                </template>
              </template>
              <span v-else class="restore-value muted">你自己设的，应用没有保存——换机前请确认还记得。</span>
            </div>
          </div>
        </div>
      </div>

      <!-- ══ 组③ 危险操作（视觉下沉） ══ -->
      <div class="group danger">
        <div class="group-title">危险操作</div>
        <div class="group-card">
          <button
            type="button"
            class="expand-row danger-row"
            :aria-expanded="confirmingClear"
            @click="confirmingClear = !confirmingClear"
          >
            <Trash2 :size="13" class="expand-icon" />
            <span class="expand-name">清空同步配置</span>
            <ChevronRight :size="13" class="chev" />
          </button>
          <div v-if="confirmingClear" class="expand-body">
            <p class="note is-danger">确定清空？忘了主密码时用。本地资产不受影响，但 GitHub 上的 Gist 需你手动删除。</p>
            <div class="btn-row">
              <AppButton variant="danger" size="sm" :disabled="syncLoading" @click="onClearSync">确认清空</AppButton>
              <AppButton variant="ghost" size="sm" @click="confirmingClear = false">取消</AppButton>
            </div>
          </div>
        </div>
      </div>
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
  gap: var(--space-4);
  padding: 0 var(--space-3) var(--space-3);
}

// ══ 分组：组标题 + 组卡片 ══
.group-title {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--app-muted);
  margin: 0 0 var(--space-2) 2px;
}
.group.danger .group-title { color: var(--danger); }

.group-card {
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  background: var(--app-panel);
  overflow: hidden;
  // 组内各行之间用 hairline 分隔
  > * + * { border-top: 1px solid var(--app-border-soft); }
}
.group.danger .group-card {
  border-color: var(--danger);
  background: var(--danger-soft);
}

// 子组件（免密行/凭据行/账号卡）自带 padding，放进组卡片后统一行的内边距，
// 并去掉它们自己可能的额外间距（它们是 flex column 的 setting-row/pat-card）。
.group-card > :deep(.setting-row),
.group-card > :deep(.pat-card) {
  padding: var(--space-3) var(--space-4);
}

// ══ 可展开行（次级表单的统一模式：图标 + 名称 + chevron） ══
.expand-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  padding: var(--space-3) var(--space-4);
  background: none;
  border: none;
  font: inherit;
  text-align: left;
  color: var(--app-text);
  cursor: pointer;
  transition: background var(--motion-fast) var(--ease-standard);
}
.expand-row:hover { background: var(--app-hover); }
.expand-row:focus-visible { outline: none; box-shadow: var(--focus-ring); }
.expand-icon { color: var(--app-muted); flex-shrink: 0; }
.expand-name { font-size: var(--text-sm); font-weight: 600; color: var(--app-strong); }
.chev {
  margin-left: auto;
  color: var(--app-subtle);
  flex-shrink: 0;
  transition: transform var(--motion-fast) var(--ease-standard);
}
.expand-row[aria-expanded='true'] .chev { transform: rotate(90deg); }

// 危险行的图标与名称用 danger 色
.danger-row .expand-icon,
.danger-row .expand-name { color: var(--danger); }

// 展开的表单体：与行 hairline 分隔，内容缩进对齐名称
.expand-body {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4) var(--space-3) calc(var(--space-4) + 21px);
  border-top: 1px dashed var(--app-border-soft);
}

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
.btn-row { display: flex; gap: var(--space-2); flex-wrap: wrap; }
.confirm-row { display: inline-flex; gap: var(--space-2); flex-wrap: wrap; }

.link-btn {
  align-self: flex-start;
  display: inline-flex;
  align-items: center;
  gap: 4px;
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

.note {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.6;
  color: var(--app-muted);
}
.note.is-danger { color: var(--danger); }

// 换机恢复里的「要填的东西」一行：标签 + 值（等宽可复制/可整段选中）
.restore-item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-wrap: wrap;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
}
.group.danger .restore-item { background: var(--app-panel); }
.restore-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  color: var(--app-muted);
  flex-shrink: 0;
}
.restore-code {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--app-strong);
  user-select: all;
  word-break: break-all;
}
.restore-value { font-size: var(--text-xs); }
</style>
