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
import McpPanelContent from '@/components/shell/McpPanelContent.vue';
import SyncPanelContent from '@/components/shell/SyncPanelContent.vue';

const store = useWorkbenchStore();
const {
  modal,
  hostKeyPrompt,
  keyboardPrompt,
  mcpApprovalPrompt,
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

// ---- 分组管理 / 删除确认 表单状态 ----
// renameGroup: 输入新分组名（单段，禁 '/');由 modal.value.path 提供旧路径
const renameGroupInput = ref('');
// createGroup: 输入分组路径（可含 '/' 建子级）
const createGroupInput = ref('');
// moveAsset: 目标分组路径
const moveGroupInput = ref('');
// assetEditor 内联校验错误（替代 window.alert）
const assetFormError = ref('');

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
    case 'mcpApproval': return '⚠️ MCP 高危操作审批';
    case 'mcpPanel': return 'MCP 服务管理';
    case 'syncPanel': return '资产同步（Gist）';
    case 'mkdir': return '新建远程目录';
    case 'rename': return '重命名远程条目';
    case 'localMkdir': return '新建本地目录';
    case 'localRename': return '重命名本地条目';
    case 'terminalSearch': return '终端搜索';
    case 'confirmDelete': return '删除连接资产';
    case 'renameGroup': return '重命名分组';
    case 'createGroup': return '新建分组';
    case 'moveAsset': return '移动到分组';
    default: return '提示';
  }
});

const assetCredentialHint = computed(() => {
  if (!editingAsset.id) return '新连接，密码/passphrase 可在下方填入';
  if (editingAsset.credential_id) return '密码已存储（重新输入会覆盖）';
  return '尚未存储密码';
});

// 移动/新建分组时可选的分组建议列表：显式声明 ∪ 资产现有 group，去重，含「未分组」。
const moveGroupOptions = computed(() => {
  const set = new Set(['未分组']);
  for (const g of (store.declaredGroups || [])) set.add(g);
  for (const a of (store.assets || [])) if (a.group) set.add(a.group);
  return [...set];
});

// ============================================================
// Sync form state when modal type changes (mirrors App.vue watch).
// ============================================================
watch(() => modal.value.type, type => {
  if (type === 'assetEditor') {
    Object.assign(editingAsset, modal.value.asset ? cloneAsset(modal.value.asset) : emptyAsset());
    Object.assign(editingCredential, emptyCredential());
    assetFormError.value = '';
  }
  if (type === 'renameGroup') {
    // 默认填入当前分组名的最后一段（方便就地改名）
    const path = modal.value.path || '';
    renameGroupInput.value = path.split('/').pop() || '';
  }
  if (type === 'createGroup') {
    createGroupInput.value = '';
  }
  if (type === 'moveAsset') {
    // 默认填入资产当前分组
    moveGroupInput.value = modal.value.asset?.group || '未分组';
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
        assetFormError.value = 'Password 认证首次保存需填写密码字段，否则无法连接。';
        return;
      }
      assetFormError.value = '';
      store.saveAsset(
        { ...editingAsset, tags: splitTags(editingAsset.tags) },
        {
          password: editingCredential.password,
          passphrase: editingCredential.passphrase
        }
      );
      return;
    case 'confirmDelete':
      store.deleteAsset(modal.value.asset?.id);
      return;
    case 'renameGroup': {
      const newName = renameGroupInput.value.trim();
      if (!newName) return;
      if (newName.includes('/')) {
        // 单段重命名禁 '/'；改层级请用新建分组
        return;
      }
      const oldPath = modal.value.path || '';
      const parent = oldPath.includes('/') ? oldPath.slice(0, oldPath.lastIndexOf('/')) : '';
      const newPath = parent ? `${parent}/${newName}` : newName;
      store.renameGroup(oldPath, newPath);
      return;
    }
    case 'createGroup': {
      const path = createGroupInput.value.trim();
      if (!path) return;
      store.createGroup(path);
      return;
    }
    case 'moveAsset': {
      const target = moveGroupInput.value.trim() || '未分组';
      store.moveAsset(modal.value.asset?.id, target);
      return;
    }
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
    case 'mcpApproval':
      // v1.1：用户确认执行高危操作 → 回传 true（解除 pipe dispatch 阻塞）
      store.resolveMcpApproval(mcpApprovalPrompt.value.request_id, true);
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

// v1.1：MCP 审批确认框的「拒绝」按钮（照 denyHostKey 范式）。
function denyMcpApproval() {
  store.resolveMcpApproval(mcpApprovalPrompt.value.request_id, false);
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
          <p v-if="assetFormError" class="form-error">{{ assetFormError }}</p>
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

        <!-- v1.1 mcpApproval：MCP 客户端不支持 elicitation 时，经 GUI 弹三段式确认框 -->
        <div v-else-if="modal.type === 'mcpApproval'" class="stack">
          <p class="muted">检测到 MCP 工具调用的高危操作，请确认是否允许执行。</p>
          <dl class="context-grid">
            <dt>AI 声明意图</dt>
            <dd>{{ mcpApprovalPrompt?.intent || '(AI 未声明意图)' }}</dd>
            <dt>真实命令</dt>
            <dd class="num" style="word-break:break-all">{{ mcpApprovalPrompt?.command }}</dd>
            <dt>后果预测</dt>
            <dd>{{ mcpApprovalPrompt?.consequence }}</dd>
          </dl>
          <p class="muted">此请求来自 MCP 客户端（如 ZCode），因客户端不支持原生确认框，改由本应用弹窗确认。</p>
        </div>

        <!-- v1.2 mcpPanel：MCP 服务可观测与配置引导（内容抽到子组件，避免本 SFC 超 500 行） -->
        <McpPanelContent v-else-if="modal.type === 'mcpPanel'" />

        <!-- v1.3 syncPanel：Gist 资产同步管理（setup/push/pull/冲突/重置/清空） -->
        <SyncPanelContent v-else-if="modal.type === 'syncPanel'" />

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

        <!-- confirmDelete -->
        <div v-else-if="modal.type === 'confirmDelete'" class="stack">
          <p>将永久删除连接「<strong>{{ modal.asset?.name }}</strong>」
            <span class="num muted">（{{ modal.asset?.host }} · {{ modal.asset?.username }}）</span></p>
          <p class="muted">同时清除已保存的密码 / 密钥凭据。此操作不可撤销。</p>
        </div>

        <!-- renameGroup -->
        <div v-else-if="modal.type === 'renameGroup'" class="stack">
          <p class="muted">重命名分组「{{ modal.path }}」的最后一段名称。
            （改层级路径请用「新建分组」+「移动」组合）</p>
          <label class="stack"><span>新名称（不含 '/'）</span>
            <AppInput :model-value="renameGroupInput" placeholder="分组名"
              @update:model-value="v => renameGroupInput = v" />
          </label>
        </div>

        <!-- createGroup -->
        <div v-else-if="modal.type === 'createGroup'" class="stack">
          <p class="muted">输入分组路径，可用「/」创建多级嵌套分组，如「生产/数据库」。</p>
          <label class="stack"><span>分组路径</span>
            <AppInput :model-value="createGroupInput" placeholder="生产/数据库"
              @update:model-value="v => createGroupInput = v" />
          </label>
        </div>

        <!-- moveAsset -->
        <div v-else-if="modal.type === 'moveAsset'" class="stack">
          <p>移动连接「<strong>{{ modal.asset?.name }}</strong>」到分组：</p>
          <label class="stack"><span>目标分组（可输入新路径或选已有）</span>
            <!-- 用原生 input 而非 AppInput：AppInput 不透传 list 属性，
                 移动分组需要 datalist 自动补全 + 允许输入新路径，故此处用裸 input。 -->
            <input
              class="native-input"
              v-model="moveGroupInput"
              list="group-list-move"
              placeholder="未分组 或 生产/数据库"
            />
            <datalist id="group-list-move">
              <option v-for="g in moveGroupOptions" :key="g" :value="g"></option>
            </datalist>
          </label>
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
        <!-- v1.1 mcpApproval：高危操作，「拒绝」用 danger 按钮 -->
        <button v-if="modal.type === 'mcpApproval'" class="btn danger" @click="denyMcpApproval">拒绝执行</button>
        <!-- mcpPanel / syncPanel 等自包含面板隐藏「取消」（操作在面板内部完成） -->
        <button v-if="modal.type !== 'mcpPanel' && modal.type !== 'syncPanel'" class="btn" id="modalSecondary" @click="closeModal">取消</button>
        <button
          v-if="modal.type === 'confirmDelete'"
          class="btn danger"
          data-modal-primary-danger
          @click="submitModal"
        >删除</button>
        <!-- v1.1 mcpApproval：高危操作主确认也用 danger（与 confirmDelete 一致） -->
        <button
          v-else-if="modal.type === 'mcpApproval'"
          class="btn danger"
          data-modal-primary-danger
          @click="submitModal"
        >确认执行</button>
        <!-- v1.2 mcpPanel / v1.3 syncPanel：自包含面板，主按钮「关闭」 -->
        <button v-else-if="modal.type === 'mcpPanel' || modal.type === 'syncPanel'" class="btn primary" id="modalPrimary" @click="submitModal">关闭</button>
        <button v-else class="btn primary" id="modalPrimary" @click="submitModal">确认</button>
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

/* 分组移动用的原生 input（AppInput 不透传 list 属性，故裸 input 对齐 AppInput 视觉）。 */
.native-input {
  width: 100%;
  padding: 6px 10px;
  background: var(--app-control);
  color: var(--app-text);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  font-family: var(--font-body);
  outline: none;
  transition: border-color var(--motion-fast) var(--ease-standard),
    box-shadow var(--motion-fast) var(--ease-standard);
}
.native-input:focus {
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

/* assetEditor 内联校验错误（替代 window.alert）。 */
.form-error {
  margin: 0;
  padding: 8px 10px;
  border: 1px solid color-mix(in oklab, var(--danger), transparent 50%);
  border-radius: var(--radius-sm);
  background: color-mix(in oklab, var(--danger), transparent 88%);
  color: var(--danger);
  font-size: var(--text-xs);
}
</style>
