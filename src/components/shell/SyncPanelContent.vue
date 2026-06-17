<script setup>
/**
 * SyncPanelContent — v1.3 Gist 同步管理面板。
 *
 * 由 GlobalModals.vue 的 modal.type === 'syncPanel' 分支渲染。
 * 抽成独立组件因 GlobalModals 已接近 SFC 行数上限。
 *
 * 三个视图状态（据 syncConfigured + syncConflict 切换）：
 * 1. 首次设置（未配置）：主密码 + 确认 + 可选 gist_id
 * 2. 日常管理（已配置）：状态展示 + push/pull/重置/清空
 * 3. 冲突解决（syncConflict 有值）：本地/远端摘要 + 选择按钮
 *
 * 数据源：workbench store re-export 的 syncStore。
 */
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { CloudUpload, CloudDownload, KeyRound, AlertTriangle, RefreshCw } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench.js';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';

const store = useWorkbenchStore();
const {
  syncConfigured, syncLastSyncedAt, syncGistIdMasked, syncConflict,
  syncLoading, githubPatConfigured
} = storeToRefs(store);

// ─── 表单输入 ───
const setupPassword = ref('');
const setupPasswordConfirm = ref('');
const setupGistId = ref(''); // 可选：换机器时填已有 gist_id
// 日常操作时临时输入主密码（push/pull/重置都需要，不缓存）
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
  try {
    const result = await store.syncSetup(setupPassword.value, setupGistId.value.trim());
    if (result?.kind === 'PulledRemote') {
      // 拉取成功后，前端需要把 assets_json 导入资产列表
      //（通过 list_connection_assets 重新拉取，因为后端已处理）
      await store.listAssets();
    }
    setupPassword.value = '';
    setupPasswordConfirm.value = '';
    setupGistId.value = '';
  } catch { /* flashMessage 已在 store 处理 */ }
}

async function onPush() {
  if (!opPassword.value) return;
  try {
    await store.syncPush(opPassword.value);
    opPassword.value = '';
  } catch { /* */ }
}

async function onPull() {
  if (!opPassword.value) return;
  try {
    await store.syncPull(opPassword.value);
    // pull 可能更新了本地 assets，刷新资产列表
    await store.listAssets();
    opPassword.value = '';
  } catch { /**/ }
}

async function onResolveConflict(choice) {
  try {
    await store.syncResolveConflict(opPassword.value, choice);
    await store.listAssets();
    opPassword.value = '';
  } catch { /**/ }
}

function onDismissConflict() {
  store.syncDismissConflict();
}

async function onResetPassword() {
  if (!resetOldPassword.value || !resetNewPassword.value || resetNewPassword.value.length < 6) return;
  try {
    await store.syncResetMasterPassword(resetOldPassword.value, resetNewPassword.value);
    resetOldPassword.value = '';
    resetNewPassword.value = '';
    showReset.value = false;
  } catch { /**/ }
}

async function onClearSync() {
  if (!confirm('确定清空同步配置？本地资产不受影响，但 Gist 上的远端数据需手动去 GitHub 删除。')) return;
  await store.syncClear();
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

// 资产 JSON 摘要（冲突框展示用）
function assetSummary(jsonStr) {
  try {
    const data = JSON.parse(jsonStr);
    const count = data.assets?.length ?? 0;
    const groups = data.groups?.length ?? 0;
    return `${count} 个连接 · ${groups} 个分组`;
  } catch {
    return '（无法解析）';
  }
}
</script>

<template>
  <div class="sync-panel">
    <!-- ⚠️ 冲突优先级最高：syncConflict 有值时独占面板 -->
    <div v-if="syncConflict" class="conflict-box">
      <div class="conflict-head">
        <AlertTriangle :size="20" class="icon-warn" />
        <div>
          <strong>检测到同步冲突</strong>
          <p class="muted">本地和远端都有变更，请选择保留哪一份。未选中的一方将被覆盖。</p>
        </div>
      </div>
      <div class="conflict-options">
        <div class="option-card">
          <span class="option-label">本地</span>
          <span class="option-summary">{{ assetSummary(syncConflict.localJson) }}</span>
        </div>
        <div class="option-card">
          <span class="option-label">远端（Gist）</span>
          <span class="option-summary">{{ assetSummary(syncConflict.remoteJson) }}</span>
        </div>
      </div>
      <label class="field">
        <span class="field-label">主密码（用于加密推送）</span>
        <AppInput v-model="opPassword" type="password" placeholder="输入主密码" />
      </label>
      <div class="conflict-actions">
        <AppButton variant="danger" size="sm" :disabled="!opPassword || syncLoading" @click="onResolveConflict('local')">用本地覆盖远端</AppButton>
        <AppButton variant="primary" size="sm" :disabled="!opPassword || syncLoading" @click="onResolveConflict('remote')">用远端覆盖本地</AppButton>
        <AppButton variant="ghost" size="sm" @click="onDismissConflict">取消</AppButton>
      </div>
    </div>

    <!-- 首次设置（未配置同步） -->
    <div v-else-if="!syncConfigured" class="setup-box">
      <p v-if="!githubPatConfigured" class="warn-inline">
        ⚠️ 需先配置 GitHub PAT（标题栏 → 同步 → 配置 token）。PAT 是访问 Gist 的凭证。
      </p>
      <p class="muted">
        通过 GitHub Gist 加密同步连接资产。资产用主密码 + AES-256-GCM 加密后上传，
        即使 Gist 泄露也是密文。<strong>主密码不存储</strong>，请务必记住——忘了只能清空重来。
      </p>
      <label class="field">
        <span class="field-label">设置主密码 <em class="req">至少 6 位</em></span>
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
    </div>

    <!-- 日常管理（已配置） -->
    <div v-else class="manage-box">
      <dl class="status-grid">
        <dt>Gist ID</dt>
        <dd class="num">{{ syncGistIdMasked || '—' }}</dd>
        <dt>上次同步</dt>
        <dd class="num">{{ fmtTime(syncLastSyncedAt) }}</dd>
      </dl>

      <label class="field">
        <span class="field-label">主密码（push/pull 需要）</span>
        <AppInput v-model="opPassword" type="password" placeholder="输入主密码" />
      </label>

      <div class="actions">
        <AppButton variant="primary" size="sm" :disabled="!opPassword || syncLoading" @click="onPush">
          <CloudUpload :size="14" />推送
        </AppButton>
        <AppButton variant="subtle" size="sm" :disabled="!opPassword || syncLoading" @click="onPull">
          <CloudDownload :size="14" />拉取
        </AppButton>
      </div>

      <!-- 重置密码（折叠） -->
      <div class="sub-section">
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
      </div>

      <!-- 清空（逃生口） -->
      <div class="sub-section">
        <button class="link-btn danger-link" @click="onClearSync">清空同步配置（忘了主密码时用）</button>
      </div>
    </div>
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

// ─── 冲突框 ───
.conflict-box {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-3);
  border: 1px solid color-mix(in oklab, var(--danger), transparent 55%);
  border-radius: var(--radius-md);
  background: color-mix(in oklab, var(--danger), transparent 92%);
}
.conflict-head {
  display: flex;
  gap: var(--space-2);
  align-items: flex-start;
}
.icon-warn { color: var(--danger); flex-shrink: 0; margin-top: 2px; }
.conflict-head strong { font-size: var(--text-sm); }
.conflict-head p { margin: 2px 0 0; font-size: var(--text-xs); line-height: 1.5; }
.conflict-options {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-2);
}
.option-card {
  padding: var(--space-2) var(--space-3);
  background: var(--app-panel-2);
  border-radius: var(--radius-sm);
  border: 1px solid var(--app-border);
}
.option-label { display: block; font-size: 10px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--app-muted); }
.option-summary { display: block; font-size: var(--text-xs); margin-top: 2px; }
.conflict-actions { display: flex; gap: var(--space-2); flex-wrap: wrap; }

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
.warn-inline { color: var(--danger); font-size: var(--text-xs); margin: 0; }
.error-inline { color: var(--danger); font-size: var(--text-xs); margin: 0; }
.actions { display: flex; gap: var(--space-2); flex-wrap: wrap; align-items: center; }
.actions :deep(svg) { vertical-align: -2px; margin-right: 4px; }

// ─── 日常管理 ───
.status-grid {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px var(--space-3);
  margin: 0 0 var(--space-1);
}
.status-grid dt {
  font-size: var(--text-xs);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--app-muted);
}
.status-grid dd { margin: 0; font-size: var(--text-xs); color: var(--app-strong); }

.sub-section {
  padding-top: var(--space-2);
  border-top: 1px solid var(--app-border);
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
.reset-form {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin-top: var(--space-2);
}

.muted { color: var(--app-muted); font-size: var(--text-xs); line-height: 1.5; margin: 0; }
.muted strong { color: var(--app-strong); }
</style>
