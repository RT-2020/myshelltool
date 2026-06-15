<script setup>
/**
 * GlobalModals — Wave 3 Step 3.5
 *
 * Top-level modal hub. Hosts every modal that is NOT absorbed by a child
 * surface (TerminalSurface owns terminalSearch; FileSurface owns mkdir /
 * localMkdir / rename / localRename — but the FORM for those four still
 * renders here so the legacy #modalLayer / #modalBody / #modalPrimary /
 * .modal-actions .btn.danger selectors keep working for tests/ui-host-key.mjs
 * Wave 5 expansion).
 *
 * Store-bound (workbench re-export of ui/sessions/assets/tunnels state +
 * the save/delete/resolve actions). The Pinia wiring stays untouched —
 * only the rendering moves out of App.vue.
 *
 * Modal types handled:
 *   - assetEditor      : 资产编辑表单
 *   - tunnelCreate     : 新建隧道表单
 *   - hostKeyVerify    : 主机密钥验证（confirm/deny）
 *   - keyboardInteractive : 键盘交互提示
 *   - tokenConfig      : GitHub PAT 配置（默认 settingsHub 分支）
 *   - mkdir / localMkdir / rename / localRename : 文件操作（form 渲染于此，
 *     触发逻辑在 FileSurface）
 *   - terminalSearch   : fallback —— TerminalSurface 已内嵌，但 store 切到
 *     该 modal 时关闭，避免双开
 */
import { computed, reactive, ref, watch } from 'vue';
import { storeToRefs } from 'pinia';
import { useWorkbenchStore } from '@/stores/workbench.js';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';
import AppSelect from '@/components/ui/AppSelect.vue';

const store = useWorkbenchStore();
const {
  modal,
  hostKeyPrompt,
  keyboardPrompt,
  remotePath,
  localPath,
  githubPatConfigured
} = storeToRefs(store);

// ============================================================
// Local form state — mirrors the App.vue reactive forms we deleted.
// All of these are only ever visible while modal.type matches their
// respective branch, so they don't bleed across types.
// ============================================================
const editingAsset = reactive(emptyAsset());
const editingCredential = reactive(emptyCredential());
const tunnelForm = reactive(emptyTunnelForm());
const tokenInput = ref('');
const mkdirName = ref('');
const renameTarget = reactive({ path: '', current: '', next: '' });
const keyboardResponses = reactive({});

const authMethodOptions = [
  { label: 'Password', value: 'Password' },
  { label: 'PrivateKey', value: 'PrivateKey' }
];
const assetStatusOptions = [
  { label: 'Connected', value: 'Connected' },
  { label: 'Warning', value: 'Warning' },
  { label: 'Idle', value: 'Idle' }
];
const tunnelKindOptions = [
  { label: 'Local（本地端口转发）', value: 'local' },
  { label: 'Dynamic SOCKS', value: 'dynamic' },
  { label: 'Remote（暂未实现，将提示错误）', value: 'remote' }
];

// ============================================================
// Modal type → human title (mirrors App.vue modalTitle)
// ============================================================
const modalTitle = computed(() => {
  switch (modal.value.type) {
    case 'assetEditor': return editingAsset.id ? '编辑连接资产' : '新增连接资产';
    case 'tunnelCreate': return '新增隧道';
    case 'tokenConfig': return '配置 / 更新 GitHub token';
    case 'hostKeyVerify': return '主机密钥验证';
    case 'keyboardInteractive': return '键盘交互认证';
    case 'mkdir': return '新建远程目录';
    case 'rename': return '重命名远程条目';
    case 'localMkdir': return '新建本地目录';
    case 'localRename': return '重命名本地条目';
    case 'terminalSearch': return '终端搜索';
    default: return '提示';
  }
});

const assetCredentialHint = computed(() => {
  if (!editingAsset.id) return '新连接，密码/passphrase 可在下方填入';
  if (editingAsset.credential_id) return '密码已存储（重新输入会覆盖）';
  return '尚未存储密码';
});

// ============================================================
// Sync form state when modal type changes (mirrors App.vue watch).
// ============================================================
watch(() => modal.value.type, type => {
  if (type === 'assetEditor') {
    Object.assign(editingAsset, modal.value.asset ? cloneAsset(modal.value.asset) : emptyAsset());
    Object.assign(editingCredential, emptyCredential());
  }
  if (type === 'tunnelCreate') {
    Object.assign(tunnelForm, emptyTunnelForm());
  }
  if (type === 'mkdir' || type === 'localMkdir') {
    mkdirName.value = '';
  }
  if (type === 'rename' || type === 'localRename') {
    Object.assign(renameTarget, {
      path: modal.value.entry?.path || '',
      current: modal.value.entry?.name || '',
      next: modal.value.entry?.name || ''
    });
  }
  if (type === 'terminalSearch') {
    store.closeTerminalSearchInline();
  }
  if (type === 'keyboardInteractive') {
    Object.keys(keyboardResponses).forEach(key => delete keyboardResponses[key]);
  }
});

function emptyAsset() {
  return {
    id: '',
    name: '',
    host: '',
    port: 22,
    username: '',
    auth_method: 'Password',
    private_key_path: '',
    group: '未分组',
    tags: '',
    status: 'Idle',
    credential_id: null,
    passphrase_credential_id: null
  };
}

function emptyCredential() {
  return { password: '', passphrase: '' };
}

function emptyTunnelForm() {
  return {
    name: '',
    kind: 'local',
    local_addr: '127.0.0.1',
    local_port: '',
    remote_addr: '',
    remote_port: '',
    auto_start: false
  };
}

function cloneAsset(asset) {
  return {
    ...asset,
    tags: asset.tags.join(', '),
    private_key_path: asset.private_key_path || ''
  };
}

function splitTags(tags) {
  return Array.isArray(tags) ? tags : String(tags || '').split(/[·,，\s]+/).filter(Boolean);
}

// ============================================================
// Modal actions — close / submit / deny (mirror App.vue)
// ============================================================
function closeModal() {
  store.modal = { type: null, asset: null };
}

function submitModal() {
  switch (modal.value.type) {
    case 'assetEditor':
      if (editingAsset.auth_method === 'Password' && !editingAsset.credential_id && !editingCredential.password) {
        window.alert('Password 认证需要密码：首次保存请填写密码字段，否则无法连接');
        return;
      }
      store.saveAsset(
        { ...editingAsset, tags: splitTags(editingAsset.tags) },
        {
          password: editingCredential.password,
          passphrase: editingCredential.passphrase
        }
      );
      return;
    case 'tokenConfig':
      store.saveToken(tokenInput.value).then(saved => {
        if (saved) tokenInput.value = '';
      });
      return;
    case 'tunnelCreate':
      store.createTunnel({
        ...tunnelForm,
        local_port: Number(tunnelForm.local_port),
        remote_port: Number(tunnelForm.remote_port)
      });
      return;
    case 'mkdir':
      store.mkdirRemote(mkdirName.value).then(() => closeModal());
      return;
    case 'localMkdir':
      store.localMkdir(mkdirName.value).then(() => closeModal());
      return;
    case 'rename':
      store.renameRemote({ path: renameTarget.path, name: renameTarget.current }, renameTarget.next).then(() => closeModal());
      return;
    case 'localRename':
      store.localRename(renameTarget.path, renameTarget.next).then(() => closeModal());
      return;
    case 'hostKeyVerify':
      store.resolveHostKeyPrompt(hostKeyPrompt.value.request_id, true);
      closeModal();
      return;
    case 'keyboardInteractive':
      store.resolveKeyboardPrompt(keyboardPrompt.value.request_id, Object.values(keyboardResponses));
      closeModal();
      return;
    case 'terminalSearch':
      if (modal.value.payload?.sessionId) {
        store.executeTerminalSearch(modal.value.payload.sessionId, store.terminalSearch.query);
      }
      return;
    default:
      closeModal();
  }
}

function denyHostKey() {
  store.resolveHostKeyPrompt(hostKeyPrompt.value.request_id, false);
  closeModal();
}
</script>

<template>
  <!--
    Legacy .modal-layer / #modalLayer / .modal / #modalBody / .modal-actions
    selectors preserved for tests/ui-host-key.mjs Wave 5 expansion:
      - #modalLayer.open   (host-key test step 3)
      - #modalBody         (host-key test step 3)
      - #modalPrimary      (host-key test step 4)
      - .modal-actions .btn.danger  (host-key test step reject)
  -->
  <div class="modal-layer" id="modalLayer" :class="{ open: modal.type }" :aria-hidden="String(!modal.type)">
    <div class="modal" role="dialog" aria-modal="true" aria-labelledby="modalTitle">
      <div class="modal-head">
        <h2 id="modalTitle">{{ modalTitle }}</h2>
        <button class="icon-btn" id="modalClose" aria-label="关闭" @click="closeModal">×</button>
      </div>
      <div class="modal-body" id="modalBody">
        <!-- assetEditor -->
        <div v-if="modal.type === 'assetEditor'" class="stack">
          <div class="grid-2">
            <label class="stack"><span class="muted">名称</span>
              <AppInput :model-value="editingAsset.name" @update:model-value="v => editingAsset.name = v" data-asset-field="name" />
            </label>
            <label class="stack"><span class="muted">主机</span>
              <AppInput :model-value="editingAsset.host" @update:model-value="v => editingAsset.host = v" data-asset-field="host" />
            </label>
            <label class="stack"><span class="muted">端口</span>
              <AppInput :model-value="editingAsset.port" type="number" @update:model-value="v => editingAsset.port = v" data-asset-field="port" />
            </label>
            <label class="stack"><span class="muted">用户名</span>
              <AppInput :model-value="editingAsset.username" @update:model-value="v => editingAsset.username = v" data-asset-field="username" />
            </label>
            <label class="stack"><span class="muted">分组</span>
              <AppInput :model-value="editingAsset.group" @update:model-value="v => editingAsset.group = v" data-asset-field="group" />
            </label>
            <label class="stack"><span class="muted">标签</span>
              <AppInput :model-value="editingAsset.tags" placeholder="prod, app" @update:model-value="v => editingAsset.tags = v" data-asset-field="tags" />
            </label>
            <label class="stack"><span class="muted">认证方式</span>
              <AppSelect :model-value="editingAsset.auth_method" :options="authMethodOptions"
                @update:model-value="v => editingAsset.auth_method = v" />
            </label>
            <label class="stack"><span class="muted">私钥路径</span>
              <AppInput :model-value="editingAsset.private_key_path" :disabled="editingAsset.auth_method !== 'PrivateKey'"
                placeholder="~/.ssh/id_ed25519" @update:model-value="v => editingAsset.private_key_path = v" data-asset-field="private_key_path" />
            </label>
            <label class="stack"><span class="muted">状态</span>
              <AppSelect :model-value="editingAsset.status" :options="assetStatusOptions"
                @update:model-value="v => editingAsset.status = v" data-asset-field="status" />
            </label>
          </div>
          <div class="callout">
            <strong>凭据</strong>
            <p class="muted">{{ assetCredentialHint }}</p>
          </div>
          <div class="grid-2">
            <label v-if="editingAsset.auth_method === 'Password'" class="stack">
              <span class="muted">密码（明文不会回显，仅保存到本地安全存储）
                <span v-if="!editingAsset.credential_id" style="color:var(--danger,#dc2626)"> · 首次保存必填</span>
              </span>
              <AppInput :model-value="editingCredential.password" type="password" placeholder="首次保存必填；编辑时留空保留既有密码"
                @update:model-value="v => editingCredential.password = v" data-asset-field="password" />
            </label>
            <label v-if="editingAsset.auth_method === 'PrivateKey'" class="stack">
              <span class="muted">Passphrase（可选）</span>
              <AppInput :model-value="editingCredential.passphrase" type="password" placeholder="无加密私钥留空"
                @update:model-value="v => editingCredential.passphrase = v" data-asset-field="passphrase" />
            </label>
          </div>
        </div>

        <!-- tunnelCreate -->
        <div v-else-if="modal.type === 'tunnelCreate'" class="stack">
          <label class="stack"><span>名称</span>
            <AppInput :model-value="tunnelForm.name" id="tunnelName" placeholder="mysql-local"
              @update:model-value="v => tunnelForm.name = v" />
          </label>
          <label class="stack"><span>类型</span>
            <AppSelect :model-value="tunnelForm.kind" :options="tunnelKindOptions"
              @update:model-value="v => tunnelForm.kind = v" />
          </label>
          <label class="stack"><span>本地地址</span>
            <AppInput :model-value="tunnelForm.local_addr" id="tunnelLocalAddr"
              @update:model-value="v => tunnelForm.local_addr = v" />
          </label>
          <label class="stack"><span>本地端口</span>
            <AppInput :model-value="tunnelForm.local_port" id="tunnelLocalPort" type="number" placeholder="13306"
              @update:model-value="v => tunnelForm.local_port = v" />
          </label>
          <div v-if="tunnelForm.kind !== 'dynamic'" id="tunnelRemoteFields">
            <label class="stack"><span>远程地址</span>
              <AppInput :model-value="tunnelForm.remote_addr" id="tunnelRemoteAddr" placeholder="10.10.9.32"
                @update:model-value="v => tunnelForm.remote_addr = v" />
            </label>
            <label class="stack"><span>远程端口</span>
              <AppInput :model-value="tunnelForm.remote_port" id="tunnelRemotePort" type="number" placeholder="3306"
                @update:model-value="v => tunnelForm.remote_port = v" />
            </label>
          </div>
          <label><input v-model="tunnelForm.auto_start" type="checkbox" id="tunnelAutoStart" /> 自动启动</label>
        </div>

        <!-- hostKeyVerify -->
        <div v-else-if="modal.type === 'hostKeyVerify'" class="stack">
          <p class="muted">检测到主机密钥，请确认是否信任该主机。</p>
          <dl class="context-grid">
            <dt>主机</dt><dd>{{ hostKeyPrompt?.host_port }}</dd>
            <dt>密钥类型</dt><dd>{{ hostKeyPrompt?.key_type }}</dd>
            <dt>指纹</dt><dd class="num" style="word-break:break-all">{{ hostKeyPrompt?.fingerprint }}</dd>
            <dt>状态</dt><dd>{{ hostKeyPrompt?.is_changed ? '密钥已变更（警告）' : '首次连接' }}</dd>
          </dl>
          <p class="muted">确认后将会保存到本地 known_hosts，下次连接不再提示。</p>
        </div>

        <!-- keyboardInteractive -->
        <div v-else-if="modal.type === 'keyboardInteractive'" class="stack">
          <p class="muted">服务器需要键盘交互认证，请根据提示输入：</p>
          <p v-if="keyboardPrompt?.name"><strong>{{ keyboardPrompt.name }}</strong></p>
          <p v-if="keyboardPrompt?.instructions" class="muted">{{ keyboardPrompt.instructions }}</p>
          <label v-for="(prompt, idx) in (keyboardPrompt?.prompts || [])" :key="idx" class="stack">
            <span class="muted">{{ prompt }}</span>
            <AppInput :model-value="keyboardResponses[idx]" type="password" :placeholder="prompt"
              @update:model-value="v => keyboardResponses[idx] = v" />
          </label>
        </div>

        <!-- mkdir / localMkdir / rename / localRename (forms render here;
             triggers live in FileSurface). -->
        <div v-else-if="modal.type === 'mkdir'" class="stack">
          <label class="stack"><span>目录名</span>
            <AppInput :model-value="mkdirName" placeholder="new-folder"
              @update:model-value="v => mkdirName = v" />
          </label>
          <p class="muted">将在当前远程路径下创建：{{ remotePath }}</p>
        </div>
        <div v-else-if="modal.type === 'localMkdir'" class="stack">
          <label class="stack"><span>目录名</span>
            <AppInput :model-value="mkdirName" placeholder="new-folder"
              @update:model-value="v => mkdirName = v" />
          </label>
          <p class="muted">将在当前本地路径下创建：{{ localPath }}</p>
        </div>
        <div v-else-if="modal.type === 'rename'" class="stack">
          <label class="stack"><span>新名称</span>
            <AppInput :model-value="renameTarget.next"
              @update:model-value="v => renameTarget.next = v" />
          </label>
          <p class="muted">原名称：{{ renameTarget.current }}</p>
        </div>
        <div v-else-if="modal.type === 'localRename'" class="stack">
          <label class="stack"><span>新名称</span>
            <AppInput :model-value="renameTarget.next"
              @update:model-value="v => renameTarget.next = v" />
          </label>
          <p class="muted">原名称：{{ renameTarget.current }}</p>
        </div>

        <!-- terminalSearch (fallback — TerminalSurface handles inline) -->
        <div v-else-if="modal.type === 'terminalSearch'" class="stack">
          <p class="muted">终端搜索由工具栏触发，此入口仅作兼容。</p>
        </div>

        <!-- default: tokenConfig / settingsHub -->
        <div v-else class="stack">
          <p>token 仅写入本地安全存储。界面提交后只展示"已配置"或"未配置"。</p>
          <label class="stack"><span class="muted">Personal Access Token</span>
            <AppInput :model-value="tokenInput" type="password" data-sync-token placeholder="粘贴 token，保存后立即隐藏"
              @update:model-value="v => tokenInput = v" />
          </label>
          <p class="muted" data-token-storage-status>本地安全存储：{{ githubPatConfigured ? '已配置' : '未配置' }}</p>
          <AppButton variant="danger" data-delete-credential @click="store.deleteToken">清除已保存的 token</AppButton>
        </div>
      </div>
      <div class="modal-actions">
        <button v-if="modal.type === 'hostKeyVerify'" class="btn danger" @click="denyHostKey">拒绝</button>
        <button class="btn" id="modalSecondary" @click="closeModal">取消</button>
        <button class="btn primary" id="modalPrimary" @click="submitModal">确认</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* Minimal legacy styles — visual chrome lives in main.scss. These mirror
   the .modal-layer / .modal / .modal-head / .modal-actions selectors
   the original App.vue shipped, so the legacy CSS rules in main.scss
   still target them by class. */
.modal-layer {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: none;
  align-items: center;
  justify-content: center;
  z-index: var(--z-modal);
}
.modal-layer.open { display: flex; }
.modal {
  background: var(--app-bg, #1a1a1a);
  border: 1px solid var(--app-border, rgba(255, 255, 255, 0.12));
  border-radius: 8px;
  width: 100%;
  max-width: 560px;
  max-height: calc(100vh - 80px);
  display: flex;
  flex-direction: column;
}
.modal-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--app-border, rgba(255, 255, 255, 0.12));
}
.modal-head h2 {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
}
.modal-body {
  padding: 16px;
  overflow: auto;
}
.modal-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 16px;
  border-top: 1px solid var(--app-border, rgba(255, 255, 255, 0.12));
}
.icon-btn {
  background: transparent;
  border: none;
  color: var(--app-muted, #888);
  cursor: pointer;
  font-size: 18px;
  padding: 4px 8px;
  border-radius: 4px;
}
.icon-btn:hover { background: rgba(255, 255, 255, 0.06); color: var(--app-strong, #fff); }

/* Legacy .btn classes — kept so the host-key test's
   ".modal-actions .btn.danger" selector still resolves. main.scss
   defines the global .btn / .btn.primary / .btn.danger rules; these
   scoped rules exist only as a fallback if global CSS is overridden. */
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  background: var(--app-control, #2a2a2a);
  color: var(--app-text, #fff);
  border: 1px solid var(--app-border, rgba(255, 255, 255, 0.12));
  border-radius: 4px;
  font-size: 13px;
  cursor: pointer;
}
.btn:hover { background: var(--app-hover, rgba(255, 255, 255, 0.06)); }
.btn.primary {
  background: var(--accent, #2f6feb);
  color: #fff;
  border-color: var(--accent, #2f6feb);
}
.btn.danger {
  background: var(--danger, #da3633);
  color: #fff;
  border-color: var(--danger, #da3633);
}

.stack {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.grid-2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.muted { color: var(--app-muted, #888); font-size: 12px; }
.context-grid {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px 12px;
  margin: 8px 0;
}
.context-grid dt { color: var(--app-muted, #888); font-size: 12px; }
.callout {
  padding: 10px 12px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid var(--app-border, rgba(255, 255, 255, 0.12));
  border-radius: 6px;
  font-size: 12px;
}
.callout strong { display: block; margin-bottom: 4px; }
.num { font-family: ui-monospace, monospace; }
</style>
