<script setup>
/**
 * AssetPrivateKeyInput — 资产私钥输入与安全托管组件。
 *
 * 位于 GlobalModals 资产编辑器中，当 auth_method === 'PrivateKey' 时渲染。
 * 允许用户：
 * 1. 【推荐】将私钥导入到安全保管箱（SecretStore），支持随资产端到端加密同步；
 * 2. 或指定本地文件绝对路径（传统方式，换机可能因路径不存在而失效）。
 */
import { ref } from 'vue';
import { KeyRound, ShieldCheck, FolderOpen, FileText, AlertCircle, Trash2, Undo2 } from 'lucide-vue-next';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';
import { openPrivateKeyFileDialog } from '@/services/backend.js';

const props = defineProps({
  asset: { type: Object, required: true },
  credential: { type: Object, required: true },
  disabled: { type: Boolean, default: false }
});

const fileInputRef = ref(null);
const showTextarea = ref(false);
const showPathFallback = ref(Boolean(props.asset.private_key_path && !props.asset.private_key_credential_id));

function triggerFileInput() {
  if (fileInputRef.value) {
    fileInputRef.value.value = '';
    fileInputRef.value.click();
  }
}

async function onFileSelected(event) {
  const file = event.target.files?.[0];
  if (!file) return;
  try {
    const content = await file.text();
    if (content && content.trim()) {
      props.credential.privateKey = content.trim();
      props.credential.clearPrivateKey = false;
      showTextarea.value = false;
    }
  } catch (err) {
    console.error('读取私钥文件失败:', err);
  }
}

function clearLoadedKey() {
  props.credential.privateKey = '';
}

function toggleClearExistingKey() {
  props.credential.clearPrivateKey = !props.credential.clearPrivateKey;
}

async function browsePath() {
  try {
    const path = await openPrivateKeyFileDialog();
    if (path) {
      props.asset.private_key_path = path;
    }
  } catch {
    // 浏览器预览模式回退
  }
}
</script>

<template>
  <div class="private-key-manager">
    <!-- 隐藏的原生文件输入框，用于安全读取私钥文本 -->
    <input
      ref="fileInputRef"
      type="file"
      class="hidden-file-input"
      accept=".pem,.key,.pub,id_rsa,id_ed25519,id_ecdsa,*"
      @change="onFileSelected"
    />

    <!-- 方案一：托管私钥（推荐） -->
    <div class="vault-card" :class="{ 'has-vault-key': asset.private_key_credential_id && !credential.clearPrivateKey }">
      <div class="card-header">
        <span class="card-title">
          <KeyRound :size="14" class="title-icon" />
          私钥托管箱（跨机安全同步）
        </span>
        <span class="rec-badge">推荐</span>
      </div>

      <!-- 情况 A：已有托管私钥且未标记清除 -->
      <div v-if="asset.private_key_credential_id && !credential.clearPrivateKey" class="vault-status">
        <div class="status-info">
          <ShieldCheck :size="16" class="icon-ok" />
          <span>私钥已在保管箱中安全托管（DPAPI 加密保护，支持随 Gist 换机同步）</span>
        </div>
        <div class="status-actions">
          <AppButton size="sm" variant="ghost" :disabled="disabled" @click="triggerFileInput">
            <FolderOpen :size="13" />更换文件
          </AppButton>
          <AppButton size="sm" variant="ghost" :disabled="disabled" @click="showTextarea = !showTextarea">
            <FileText :size="13" />{{ showTextarea ? '收起粘贴' : '粘贴替换' }}
          </AppButton>
          <AppButton size="sm" variant="danger" :disabled="disabled" @click="toggleClearExistingKey">
            <Trash2 :size="13" />清除私钥
          </AppButton>
        </div>
      </div>

      <!-- 情况 B：已标记清除既有私钥 -->
      <div v-else-if="asset.private_key_credential_id && credential.clearPrivateKey" class="vault-status is-cleared">
        <div class="status-info">
          <AlertCircle :size="16" class="icon-warn" />
          <span>保存时将从安全保管箱中删除既有私钥</span>
        </div>
        <AppButton size="sm" variant="ghost" :disabled="disabled" @click="toggleClearExistingKey">
          <Undo2 :size="13" />撤销清除
        </AppButton>
      </div>

      <!-- 情况 C：当前已载入新的私钥内容 -->
      <div v-else-if="credential.privateKey" class="vault-status is-ready">
        <div class="status-info">
          <ShieldCheck :size="16" class="icon-ok" />
          <span>已载入私钥文本（共 {{ credential.privateKey.length }} 字符），保存时将存入保管箱</span>
        </div>
        <div class="status-actions">
          <AppButton size="sm" variant="ghost" :disabled="disabled" @click="showTextarea = !showTextarea">
            <FileText :size="13" />{{ showTextarea ? '收起编辑' : '查看/编辑' }}
          </AppButton>
          <AppButton size="sm" variant="danger" :disabled="disabled" @click="clearLoadedKey">
            <Trash2 :size="13" />清除
          </AppButton>
        </div>
      </div>

      <!-- 情况 D：尚未托管私钥，显示导入与粘贴操作 -->
      <div v-else class="vault-empty">
        <p class="empty-tip">
          将私钥导入到保管箱后，换电脑同步时无需到处拷贝私钥文件，拉取后即可一键直连。
        </p>
        <div class="empty-actions">
          <AppButton size="sm" variant="primary" :disabled="disabled" @click="triggerFileInput">
            <FolderOpen :size="13" />选择私钥文件导入
          </AppButton>
          <AppButton size="sm" variant="ghost" :disabled="disabled" @click="showTextarea = !showTextarea">
            <FileText :size="13" />{{ showTextarea ? '收起文本框' : '直接粘贴私钥文本' }}
          </AppButton>
        </div>
      </div>

      <!-- 粘贴/编辑私钥文本域 -->
      <div v-if="showTextarea" class="paste-zone">
        <textarea
          v-model="credential.privateKey"
          class="key-textarea"
          rows="5"
          placeholder="-----BEGIN OPENSSH PRIVATE KEY-----&#10;...&#10;-----END OPENSSH PRIVATE KEY-----"
          :disabled="disabled"
        ></textarea>
      </div>
    </div>

    <!-- 方案二：本地物理路径（备用） -->
    <div class="fallback-path-section">
      <button
        type="button"
        class="toggle-path-btn"
        @click="showPathFallback = !showPathFallback"
      >
        {{ showPathFallback ? '▼ 收起物理文件路径' : '▶ 或使用本地物理文件路径（不推荐换机用户）' }}
      </button>

      <div v-if="showPathFallback" class="path-inputs">
        <div class="inline-field-row">
          <AppInput
            :model-value="asset.private_key_path"
            :disabled="disabled"
            placeholder="~/.ssh/id_ed25519 或点击浏览选择"
            data-asset-field="private_key_path"
            @update:model-value="v => asset.private_key_path = v"
          />
          <AppButton size="sm" :disabled="disabled" @click="browsePath">浏览…</AppButton>
        </div>
        <p class="path-hint">
          注意：本地绝对路径仅在当前机器有效，换电脑同步后可能无法找到该文件。优先建议使用上方保管箱。
        </p>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.private-key-manager {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  width: 100%;
}

.hidden-file-input {
  display: none;
}

.vault-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  background: var(--app-panel);
}

.vault-card.has-vault-key {
  border-color: color-mix(in oklab, var(--success), transparent 60%);
  background: color-mix(in oklab, var(--success), transparent 96%);
}

.card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.card-title {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-xs);
  font-weight: 600;
  color: var(--app-strong);
}

.title-icon {
  color: var(--accent);
}

.rec-badge {
  font-size: 11px;
  padding: 1px 6px;
  border-radius: var(--radius-pill);
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 500;
}

.vault-status {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  padding: var(--space-2);
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  background: color-mix(in oklab, var(--app-bg), transparent 30%);
}

.vault-status.is-cleared {
  background: color-mix(in oklab, var(--danger), transparent 92%);
}

.vault-status.is-ready {
  background: color-mix(in oklab, var(--success), transparent 92%);
}

.status-info {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--app-strong);
}

.status-actions {
  display: flex;
  align-items: center;
  gap: 6px;
}

.icon-ok {
  color: var(--success);
  flex-shrink: 0;
}

.icon-warn {
  color: var(--warn);
  flex-shrink: 0;
}

.vault-empty {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.empty-tip {
  margin: 0;
  font-size: var(--text-xs);
  color: var(--app-muted);
  line-height: 1.5;
}

.empty-actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.paste-zone {
  margin-top: var(--space-1);
}

.key-textarea {
  width: 100%;
  font-family: var(--font-mono, monospace);
  font-size: var(--text-xs);
  padding: var(--space-2);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-control);
  color: var(--app-text);
  resize: vertical;
  line-height: 1.4;

  &:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }
}

.fallback-path-section {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.toggle-path-btn {
  background: transparent;
  border: none;
  padding: 0;
  color: var(--app-muted);
  font-size: var(--text-xs);
  cursor: pointer;
  text-align: left;

  &:hover {
    color: var(--app-strong);
  }
}

.path-inputs {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 4px;
}

.inline-field-row {
  display: flex;
  gap: var(--space-2);
  align-items: center;

  :deep(.app-input) {
    flex: 1 1 0;
    min-width: 0;
  }
}

.path-hint {
  margin: 0;
  font-size: 11px;
  color: var(--app-muted);
  line-height: 1.4;
}
</style>
