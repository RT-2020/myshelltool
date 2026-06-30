<script setup>
/**
 * SyncPanelContent — v1.6 重写 Gist 同步管理面板。
 *
 * 信息架构修复（对齐 McpPanelContent 的 hero+block 范式）：
 *   1. Hero 状态条 —— PAT+同步整体状态全宽横幅，三态着色（success/warn/danger）
 *   2. 视图分支（互斥，按优先级）：
 *      a. 冲突框（syncConflict）—— 最高优先级，独占
 *      b. PAT 未配置阻断视图 —— 委托 SyncPatGuide 子组件（3 步上手 + 外链 + 加密说明）
 *      c. 首次设置（PAT OK 但同步未配）—— 主密码 + 可选 gist_id
 *      d. 日常管理（同步已配）—— 状态网格 + push/pull + 重置/清空
 *
 * PAT 获取引导（v1.6 新增）：原版未配 PAT 时只显示一行警告，无跳转无指引。
 * 现在阻断式展示，委托 SyncPatGuide（opener 直达 github.com/settings/tokens）。
 *
 * 由 GlobalModals.vue 的 modal.type === 'syncPanel' 分支渲染。抽成独立组件
 * 是为避免 GlobalModals.vue 超 500 行 SFC 硬上限（AGENTS.md 质量红线）。
 *
 * 视觉语言照搬 McpPanelContent：hero / block-head / detail-grid / field，
 * 零新增 token，零硬编码色值（AGENTS.md 红线：用 var(--token)）。
 */
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import {
  CloudUpload, CloudDownload, KeyRound, AlertTriangle,
  ShieldCheck, RefreshCw
} from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench.js';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';
import SyncPatGuide from '@/components/shell/SyncPatGuide.vue';
import SyncAutoSyncControl from '@/components/shell/SyncAutoSyncControl.vue';
import SyncConflictResolver from '@/components/shell/SyncConflictResolver.vue';

const store = useWorkbenchStore();
const {
  syncConfigured, syncLastSyncedAt, syncGistIdMasked, syncConflict,
  syncLoading, githubPatConfigured,
  // v1.6 自动同步（autoSyncEnabled 用于 push/pull 主密码可留空判定 + 状态展示）
  syncAutoSyncEnabled, syncRemoteHasUpdates
} = storeToRefs(store);

// ─── 表单输入 ───
const setupPassword = ref('');
const setupPasswordConfirm = ref('');
const setupGistId = ref(''); // 可选：换机器时填已有 gist_id
// 日常操作时临时输入主密码（push/pull/重置都需要）。
// v1.6：启用自动同步后可留空（走会话密钥路径）。
const opPassword = ref('');
const resetOldPassword = ref('');
const resetNewPassword = ref('');
const showReset = ref(false); // 重置密码子表单展开开关

const setupPasswordMismatch = computed(() =>
  setupPassword.value && setupPasswordConfirm.value && setupPassword.value !== setupPasswordConfirm.value
);

const canSetup = computed(() =>
  setupPassword.value.length >= 6 && !setupPasswordMismatch.value && githubPatConfigured.value
);

// ─── 操作 ───
async function onSetup() {
  if (!canSetup.value) return;
  const result = await store.syncSetup(setupPassword.value, setupGistId.value.trim());
  if (!result) return; // 失败：flashMessage 已在 store 通知用户
  if (result.kind === 'PulledRemote') {
    // 拉取成功后刷新资产列表（后端已写入本地 connection-assets.json）
    await store.listAssets();
  }
  setupPassword.value = '';
  setupPasswordConfirm.value = '';
  setupGistId.value = '';
}

async function onPush() {
  // v1.6：启用自动同步后主密码可留空（走会话密钥）；否则必须输入
  if (!opPassword.value && !syncAutoSyncEnabled.value) return;
  const result = await store.syncPush(opPassword.value);
  if (result) opPassword.value = '';
}

async function onPull() {
  // v1.6：启用自动同步后主密码可留空（走会话密钥）；否则必须输入
  if (!opPassword.value && !syncAutoSyncEnabled.value) return;
  const result = await store.syncPull(opPassword.value);
  if (!result) return;
  // pull 成功（含 PullRemote/Conflict 解决后）刷新资产列表
  if (result.decision === 'Pulled') {
    await store.listAssets();
  }
  opPassword.value = '';
}

async function onResetPassword() {
  if (!resetOldPassword.value || !resetNewPassword.value || resetNewPassword.value.length < 6) return;
  const ok = await store.syncResetMasterPassword(resetOldPassword.value, resetNewPassword.value);
  if (ok === undefined) return; // store 失败返回 undefined，flashMessage 已通知
  resetOldPassword.value = '';
  resetNewPassword.value = '';
  showReset.value = false;
}

// 清空二次确认（内联，不用 window.confirm——AGENTS.md 红线）
const confirmingClear = ref(false);
async function onClearSync() {
  await store.syncClear();
  confirmingClear.value = false;
}

function onRefresh() {
  store.syncRefreshStatus();
}

function fmtTime(iso) {
  if (!iso) return '—';
  try {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    const pad = n => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  } catch { return iso; }
}
</script>

<template>
  <div class="sync-panel">
    <!-- ① Hero 状态条：PAT+同步整体状态，三态着色，第一眼即知「能不能同步」 -->
    <div class="hero" :class="{
      'is-connected': syncConfigured,
      'is-warn': !syncConfigured && githubPatConfigured,
      'is-offline': !githubPatConfigured
    }">
      <div class="hero-main">
        <component :is="syncConfigured ? ShieldCheck : (!githubPatConfigured ? AlertTriangle : CloudUpload)"
          :size="20" class="hero-icon" />
        <div class="hero-text">
          <span class="hero-status">{{
            syncConfigured ? (syncAutoSyncEnabled ? '同步已配置（自动）' : '同步已配置') : (!githubPatConfigured ? '需先配置 GitHub Token' : '同步未配置')
          }}</span>
          <span class="hero-sub">{{
            syncConfigured
              ? `Gist ${syncGistIdMasked || '—'} · 上次同步 ${fmtTime(syncLastSyncedAt)}${syncAutoSyncEnabled ? ' · 自动同步中' : ''}`
              : (!githubPatConfigured
                ? '资产同步通过 GitHub Gist 加密备份，需先配置访问凭证'
                : '已配置 PAT，设置主密码即可启用加密同步')
          }}</span>
        </div>
      </div>
      <div class="hero-meta">
        <!-- v1.6：远端有更新时显示徽章（点击拉取） -->
        <span v-if="syncRemoteHasUpdates" class="update-badge" @click="onPull">
          <CloudDownload :size="12" />远端有更新
        </span>
        <AppButton variant="subtle" size="sm" @click="onRefresh"><RefreshCw :size="12" />刷新</AppButton>
      </div>
    </div>

    <!-- ⚠️ 冲突优先级最高：委托 SyncConflictResolver 子组件（syncConflict 有值时独占） -->
    <SyncConflictResolver v-if="syncConflict" />

    <!-- ② PAT 未配置阻断视图：委托 SyncPatGuide（3 步引导 + 外链 + 加密说明） -->
    <SyncPatGuide v-else-if="!githubPatConfigured" />

    <!-- ③ 首次设置（PAT OK 但同步未配） -->
    <section v-else-if="!syncConfigured" class="block">
      <header class="block-head"><CloudUpload :size="12" />设置主密码</header>
      <p class="block-note muted">
        主密码用于加解密资产。<strong>不存储</strong>，每次 push/pull 需重新输入。
        建议用密码管理器保存——忘了只能「清空同步」重来。
      </p>
      <label class="field">
        <span class="field-label">主密码 <em class="req">至少 6 位</em></span>
        <AppInput v-model="setupPassword" type="password" placeholder="主密码" />
      </label>
      <label class="field">
        <span class="field-label">确认主密码</span>
        <AppInput v-model="setupPasswordConfirm" type="password" placeholder="再次输入" />
      </label>
      <p v-if="setupPasswordMismatch" class="error-inline">两次输入不一致</p>
      <label class="field">
        <span class="field-label">已有 Gist ID（可选，换机器时填）</span>
        <AppInput v-model="setupGistId" placeholder="留空则创建新 Gist" />
      </label>
      <div class="actions">
        <AppButton variant="primary" :disabled="!canSetup || syncLoading" @click="onSetup">
          {{ setupGistId.trim() ? '拉取远端数据' : '创建同步' }}
        </AppButton>
      </div>
    </section>

    <!-- ④ 日常管理（同步已配） -->
    <template v-else>
      <section class="block">
        <header class="block-head"><CloudUpload :size="12" />同步状态</header>
        <dl class="detail-grid">
          <dt>Gist ID</dt>
          <dd class="num">{{ syncGistIdMasked || '—' }}</dd>
          <dt>上次同步</dt>
          <dd class="num">{{ fmtTime(syncLastSyncedAt) }}</dd>
          <dt>自动同步</dt>
          <dd class="num" :class="syncAutoSyncEnabled ? 'tone-ok' : 'tone-muted'">
            {{ syncAutoSyncEnabled ? '已启用' : '未启用' }}
          </dd>
        </dl>
      </section>

      <!-- v1.6 自动同步开关：委托 SyncAutoSyncControl 子组件（资产变更后自动推送） -->
      <SyncAutoSyncControl />

      <section class="block">
        <header class="block-head"><CloudDownload :size="12" />推送 / 拉取</header>
        <label class="field">
          <span class="field-label">
            主密码<span v-if="syncAutoSyncEnabled" class="muted">（自动同步已启用，可留空）</span>
          </span>
          <AppInput v-model="opPassword" type="password" placeholder="输入主密码" />
        </label>
        <div class="actions">
          <AppButton variant="primary" size="sm" :disabled="(!opPassword && !syncAutoSyncEnabled) || syncLoading" @click="onPush">
            <CloudUpload :size="14" />推送
          </AppButton>
          <AppButton variant="subtle" size="sm" :disabled="(!opPassword && !syncAutoSyncEnabled) || syncLoading" @click="onPull">
            <CloudDownload :size="14" />拉取
          </AppButton>
        </div>
      </section>

      <!-- 重置密码（折叠） -->
      <section class="block">
        <button class="link-btn" @click="showReset = !showReset">
          <KeyRound :size="12" />{{ showReset ? '收起重置密码' : '重置主密码' }}
        </button>
        <div v-if="showReset" class="reset-form">
          <label class="field">
            <span class="field-label">旧主密码</span>
            <AppInput v-model="resetOldPassword" type="password" />
          </label>
          <label class="field">
            <span class="field-label">新主密码 <em class="req">至少 6 位</em></span>
            <AppInput v-model="resetNewPassword" type="password" />
          </label>
          <AppButton variant="ghost" size="sm"
            :disabled="!resetOldPassword || resetNewPassword.length < 6 || syncLoading"
            @click="onResetPassword">重置</AppButton>
        </div>
      </section>

      <!-- 清空（逃生口）—— 内联二次确认，不用 window.confirm（AGENTS.md 红线） -->
      <section class="block">
        <button v-if="!confirmingClear" class="link-btn danger-link" @click="confirmingClear = true">
          清空同步配置（忘了主密码时用）
        </button>
        <div v-else class="confirm-clear">
          <p class="warn-inline">确定清空？本地资产不受影响，但 Gist 上的远端数据需手动去 GitHub 删除。</p>
          <div class="actions">
            <AppButton variant="danger" size="sm" :disabled="syncLoading" @click="onClearSync">确认清空</AppButton>
            <AppButton variant="ghost" size="sm" @click="confirmingClear = false">取消</AppButton>
          </div>
        </div>
      </section>
    </template>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.sync-panel {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  font-size: var(--text-sm);
  max-height: 62vh;
  overflow-y: auto;
  padding-right: 2px;
}

// ─── ① Hero 状态条（照 McpPanelContent 范式）───
.hero {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-md);
  border: 1px solid var(--app-border);
  background: var(--app-panel-2);
  transition: border-color var(--motion-base) var(--ease-standard),
    background var(--motion-base) var(--ease-standard);
}
.hero.is-connected {
  border-color: color-mix(in oklab, var(--success), transparent 55%);
  background: color-mix(in oklab, var(--success), transparent 90%);
}
.hero.is-warn {
  border-color: color-mix(in oklab, var(--accent), transparent 60%);
  background: color-mix(in oklab, var(--accent), transparent 92%);
}
.hero.is-offline {
  border-style: dashed;
  border-color: color-mix(in oklab, var(--warn), transparent 55%);
  background: color-mix(in oklab, var(--warn), transparent 92%);
}
.hero-main {
  display: inline-flex;
  align-items: center;
  gap: var(--space-3);
}
.hero-icon {
  flex-shrink: 0;
  color: var(--app-muted);
}
.hero.is-connected .hero-icon { color: var(--success); }
.hero.is-warn .hero-icon { color: var(--accent); }
.hero.is-offline .hero-icon { color: var(--warn); }
.hero-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.hero-status {
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--app-strong);
}
.hero.is-connected .hero-status { color: var(--success); }
.hero.is-warn .hero-status { color: var(--accent); }
.hero.is-offline .hero-status { color: var(--warn); }
.hero-sub {
  font-size: var(--text-xs);
  color: var(--app-muted);
}

// ─── block 通用（照 McpPanelContent 的 chrome header 范式）───
.block {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  background: var(--app-panel);
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
.block-head :deep(svg) { flex-shrink: 0; }
.block-note {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.6;
}
.block-note strong { color: var(--app-strong); }

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
.req {
  font-style: normal;
  font-family: var(--font-mono);
  font-size: 11px;
  background: var(--app-hover);
  padding: 0 4px;
  border-radius: 4px;
}
.warn-inline { color: var(--danger); font-size: var(--text-xs); margin: 0; line-height: 1.4; }
.error-inline { color: var(--danger); font-size: var(--text-xs); margin: 0; }
.actions { display: flex; gap: var(--space-2); flex-wrap: wrap; align-items: center; }
.actions :deep(svg) { vertical-align: -2px; margin-right: 4px; }

// ─── detail-grid（照 McpPanelContent 范式）───
.detail-grid {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px var(--space-3);
  margin: 0;
}
.detail-grid dt {
  font-size: var(--text-xs);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--app-muted);
  align-self: center;
}
.detail-grid dd {
  margin: 0;
  font-size: var(--text-xs);
  color: var(--app-strong);
}
.num { font-family: var(--font-mono); }

// ─── 折叠区 / 链接按钮 ───
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
.reset-form {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin-top: var(--space-2);
}

// ─── v1.6 hero-meta + 远端更新徽章 ───
.hero-meta {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  flex-shrink: 0;
}
.update-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: var(--text-xs);
  padding: 3px 8px;
  border-radius: var(--radius-pill);
  background: color-mix(in oklab, var(--accent), transparent 85%);
  color: var(--accent);
  cursor: pointer;
  border: 1px solid color-mix(in oklab, var(--accent), transparent 60%);
  transition: background var(--motion-fast) var(--ease-standard);
}
.update-badge:hover {
  background: color-mix(in oklab, var(--accent), transparent 75%);
}
.update-badge :deep(svg) { flex-shrink: 0; }

// detail-grid 的自动同步状态行语义色（自动同步 block 的样式已随 SyncAutoSyncControl 迁出）
.tone-ok { color: var(--success); }
.tone-muted { color: var(--app-muted); }

.muted { color: var(--app-muted); font-size: var(--text-xs); line-height: 1.5; margin: 0; }
.muted strong { color: var(--app-strong); }
</style>
