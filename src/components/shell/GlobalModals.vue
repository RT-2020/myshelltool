<script setup lang="ts">
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
 *   - reauthPassword   : 认证失败快捷重输密码（TerminalSurface 错误卡片入口）
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
import { useWorkbenchStore } from '@/stores/workbench';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';
import AppSelect from '@/components/ui/AppSelect.vue';
import McpPanelContent from '@/components/shell/McpPanelContent.vue';
import SyncPanelContent from '@/components/shell/SyncPanelContent.vue';
import SettingsPanelContent from '@/components/shell/SettingsPanelContent.vue';
import ConfirmCloseAssetWindowContent from '@/components/shell/ConfirmCloseAssetWindowContent.vue';
import PatConfigCard from '@/components/shell/PatConfigCard.vue';
import AssetEditorContent from '@/components/shell/AssetEditorContent.vue';
import ReauthPasswordContent from '@/components/shell/ReauthPasswordContent.vue';
import TunnelCreateContent from '@/components/shell/TunnelCreateContent.vue';
import GroupFormsContent from '@/components/shell/GroupFormsContent.vue';
import FileOpFormsContent from '@/components/shell/FileOpFormsContent.vue';
import { dispatchModalSubmit, type ModalSubmitContext } from '@/lib/modalSubmit';
import { cloneAsset, emptyAsset, emptyCredential, emptyTunnelForm } from '@/lib/modalForms';
import { modalTitleFor } from '@/lib/modalTitles';
import { useModalDismiss } from '@/composables/useModalDismiss';
import { useModalView } from '@/composables/useModalView';
import type {
  AssetEditorForm,
  ConnectionAssetInput,
  CredentialEditorForm,
  ModalState,
  RemoteFileEntry,
  TunnelEditorForm
} from '@/types/domain';

/**
 * modal 的按需挂载载荷：ModalState（ui store 的权威形状）只声明 type/asset/payload，
 * path / entry / count / onConfirm 等字段由各业务按弹窗类型附加（弱约束透传）。
 * 组件内以本接口消费（类型断言不改变运行时）。
 */
interface ModalExtras extends ModalState {
  path?: string;
  entry?: RemoteFileEntry | null;
  count?: number;
  onConfirm?: (() => void) | null;
}

const store = useWorkbenchStore();
// ---- editorDialog（编辑器通用弹窗，v0.18）：标题/正文/可选路径输入 + 动态按钮 ----
// 单一 type 承载编辑器全部弹窗（未保存三选/保存冲突/另存为/上传/草稿恢复/备份列表），
// 按钮回调在 stores/editor.ts 组装（openDialog），此处只做渲染与回调分发。
interface EditorDialogViewPayload {
  title?: string;
  message?: string;
  detail?: string;
  input?: { label: string; placeholder?: string; value?: string };
  buttons?: { label: string; danger?: boolean; primary?: boolean }[];
}
const editorDialog = computed<EditorDialogViewPayload | null>(() =>
  view.value.type === 'editorDialog'
    ? ((view.value.payload as unknown as EditorDialogViewPayload | undefined) ?? {})
    : null
);
const editorDialogInput = ref('');
const editorDialogError = ref('');
async function runEditorDialog(index: number) {
  if (submitting.value) return;
  if (!editorDialog.value?.buttons?.[index]) return;
  submitting.value = true;
  try {
    // 返回错误串 = 内联展示、弹窗保持；否则关闭（关闭逻辑归 editor store 的 action）
    const err = await store.editorResolveDialog(index, editorDialogInput.value);
    if (err) {
      editorDialogError.value = err;
    } else {
      store.modal = { type: null, asset: null };
    }
  } finally {
    submitting.value = false;
  }
}
const modalTitle = computed(() => {
  // editorDialog 标题来自 payload（modalTitles 的 switch 不承载动态标题）
  if (view.value.type === 'editorDialog') return editorDialog.value?.title || '编辑器';
  return modalTitleFor(view.value.type, { editingExistingAsset: Boolean(editingAsset.id) });
});
const {
  modal: modalRef,
  hostKeyPrompt,
  keyboardPrompt,
  mcpApprovalPrompt,
  remotePath,
  localPath,
  pendingFileDelete
} = storeToRefs(store);
const modal = computed(() => modalRef.value as ModalExtras);
// 退场动画视图层：关闭瞬间 store 已置空，view 滞留最后载荷让淡出播完（状态即时、渲染滞后）
const { view, closing, onExitAnimationEnd } = useModalView(modal);

// ============================================================
// Local form state — mirrors the App.vue reactive forms we deleted.
// All of these are only ever visible while view.type matches their
// respective branch, so they don't bleed across types.
// ============================================================
const editingAsset = reactive(emptyAsset());
const editingCredential = reactive(emptyCredential());
const tunnelForm = reactive(emptyTunnelForm());
const fileForm = reactive({ mkdirName: '' });
const renameTarget = reactive({ path: '', current: '', next: '' });
const keyboardResponses = reactive<Record<string, string>>({});

// ---- 分组管理 / 删除确认 表单状态 ----
// 分组三表单共享输入对象（rename=改名/create=新建/move=移动，submitModal 分发读值）
const groupInputs = reactive({ rename: '', create: '', move: '' });
// assetEditor 内联校验错误（替代 window.alert）
const assetFormError = ref('');
// 分组表单内联校验错误（renameGroup / createGroup / moveAsset 共用，替代静默 return）
const groupFormError = ref('');
// 异步提交进行中：主/副按钮禁用 + spinner，防止重复提交
const submitting = ref(false);
// reauthPassword：连接失败快捷重认证表单（TerminalSurface 错误卡片入口）
const reauthForm = reactive({ password: '', error: '' });

// 事实备注：KeyboardInteractivePayload 权威类型（domain.ts）字段为 instruction（单数），
// 此处历史代码访问 instructions（复数）——与后端契约不一致、运行时恒 undefined 不显示。
// 按「行为零变化」保留原属性访问路径，仅类型层断言放宽。
const keyboardInstructions = computed(() =>
  (keyboardPrompt.value as { instructions?: string | null } | null)?.instructions
);

// 可选分组列表（资产编辑器与移动分组共用）：显式声明 ∪ 资产现有 group，去重，含「未分组」。
// AppSelect 选项形态：value 即分组路径（含多级「生产/数据库」），label 同值。
// 曾用原生 input+datalist：预填当前值时建议被前缀过滤只剩一项，且 WebView2 里
// 选择后弹层不自动收起——统一换 AppSelect（选择即收起、全量列出已有分组）。
const groupOptions = computed(() => {
  const set = new Set(['未分组']);
  for (const g of (store.declaredGroups || [])) set.add(g);
  for (const a of (store.assets || [])) if (a.group) set.add(a.group);
  return [...set].map(g => ({ label: g, value: g }));
});

// 删除确认弹窗：names 最多展示前 5 个，超出显示「等 N 项」。
const pendingFileNames = computed(() => {
  const names = pendingFileDelete.value?.names || [];
  return names.slice(0, 5);
});
const pendingFileNamesMore = computed(() => {
  const names = pendingFileDelete.value?.names || [];
  return names.length > 5 ? names.length - 5 : 0;
});

// ============================================================
// Sync form state when modal type changes (mirrors App.vue watch).
// ============================================================
watch(() => modal.value.type, type => {
  // 每次切换弹窗清空分组表单校验错误（避免残留到下一弹窗）
  groupFormError.value = '';
  if (type === 'editorDialog') {
    // 路径输入预填 payload 初始值；错误态每次重开清零
    const payload = modal.value.payload as unknown as EditorDialogViewPayload | undefined;
    editorDialogInput.value = payload?.input?.value ?? '';
    editorDialogError.value = '';
  }
  if (type === 'assetEditor') {
    Object.assign(editingAsset, modal.value.asset ? cloneAsset(modal.value.asset) : emptyAsset());
    Object.assign(editingCredential, emptyCredential());
    assetFormError.value = '';
    // 分组头「+」快捷新增：预填目标分组（仅新建态生效，编辑态以资产自身分组为准）
    if (!modal.value.asset) {
      const presetGroup = modal.value.payload?.presetGroup;
      if (typeof presetGroup === 'string' && presetGroup) editingAsset.group = presetGroup;
    }
  }
  if (type === 'reauthPassword') {
    reauthForm.password = '';
    reauthForm.error = '';
  }
  if (type === 'renameGroup') {
    // 默认填入当前分组名的最后一段（方便就地改名）
    const path = modal.value.path || '';
    groupInputs.rename = path.split('/').pop() || '';
  }
  if (type === 'createGroup') {
    groupInputs.create = '';
  }
  if (type === 'moveAsset') {
    // 默认填入资产当前分组
    groupInputs.move = modal.value.asset?.group || '未分组';
  }
  if (type === 'tunnelCreate') {
    Object.assign(tunnelForm, emptyTunnelForm());
  }
  if (type === 'mkdir' || type === 'localMkdir') {
    fileForm.mkdirName = '';
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
  if (type === 'confirmCloseAssetWindow') {
    // 纯确认弹窗（无表单状态）：count/onConfirm 直接读 modal payload
  }
});

// ============================================================
// Modal actions — close / submit / deny (mirror App.vue)
// ============================================================
function closeModal() {
  // 文件删除/覆盖确认弹窗：× 关闭等价于取消（resolve(false)/清 pending），
  // 否则上传循环会永远挂起等待 Promise resolve（S2 覆盖保护的关键兜底）
  const type = modal.value.type;
  if (type === 'confirmFileOverwrite') { store.cancelFileOverwrite(); return; }
  if (type === 'confirmFileDelete') { store.cancelFileDelete(); return; }
  // 安全审批类弹窗：× 关闭等价于拒绝——立即 resolve(false) 回传后端，
  // 不能裸关弹窗（后端 oneshot 无人响应会挂到 60s 超时才拒绝）
  if (type === 'hostKeyVerify') { denyHostKey(); return; }
  if (type === 'mcpApproval') { denyMcpApproval(); return; }
  store.modal = { type: null, asset: null };
}

// 主按钮分发抽到 lib/modalSubmit（switch 逐句迁移；表单状态经 ctx 传入）。
// ctx 在 setup 组装一次：成员是 refs/reactive 对象，取值始终最新。
const submitCtx: ModalSubmitContext = {
  store: {
    saveAsset: (input, credential) => store.saveAsset(input as never, credential as never),
    deleteAsset: id => store.deleteAsset(id),
    renameGroup: (a, b) => store.renameGroup(a, b),
    createGroup: path => store.createGroup(path),
    moveAsset: (id, target) => store.moveAsset(id, target),
    createTunnel: form => store.createTunnel(form),
    mkdirRemote: name => store.mkdirRemote(name),
    localMkdir: name => store.localMkdir(name),
    renameRemote: (entry, next) => store.renameRemote(entry, next),
    localRename: (path, next) => store.localRename(path, next),
    resolveHostKeyPrompt: (id, ok) => store.resolveHostKeyPrompt(id, ok),
    resolveMcpApproval: (id, ok) => store.resolveMcpApproval(id, ok),
    resolveKeyboardPrompt: (id, answers) => store.resolveKeyboardPrompt(id, answers),
    reconnectSession: id => store.reconnectSession(id),
    closeTerminalSearchInline: () => store.closeTerminalSearchInline(),
    // accept/确认后的直关（绕过 closeModal 对安全审批类的 deny 路由）
    closeDirect: () => { store.modal = { type: null, asset: null }; },
    announce: (message, opts) => store.announce(message, opts),
    get assets() { return store.assets; },
    get terminalSearch() { return store.terminalSearch; }
  },
  modal: modal.value,
  editingAsset,
  editingCredential,
  tunnelForm,
  groupInputs,
  fileForm,
  renameTarget,
  reauthForm,
  keyboardResponses,
  assetFormError,
  groupFormError,
  hostKeyPrompt,
  mcpApprovalPrompt,
  keyboardPrompt,
  closeModal,
  submitting
};

async function submitModal() {
  if (submitting.value) return;
  // modal 是 computed 快照，分发前刷新（提交分支里 closeDirect 改 store.modal 不经它）
  submitCtx.modal = modal.value;
  await dispatchModalSubmit(submitCtx, modal.value.type);
}

/** 副按钮「取消」：文件删除/覆盖走各自的 cancel action，其余 closeModal。 */
function secondaryAction() {
  const type = modal.value.type;
  if (type === 'confirmFileDelete') { store.cancelFileDelete(); return; }
  if (type === 'confirmFileOverwrite') { store.cancelFileOverwrite(); return; }
  closeModal();
}

/** confirmFileDelete 主按钮：删除期间 submitting 防重复点击；关闭时机由 store 处理（成功关、失败留）。 */
async function runFileDeleteConfirm() {
  if (submitting.value) return;
  submitting.value = true;
  try {
    await store.confirmFileDelete();
  } finally {
    submitting.value = false;
  }
}

function denyHostKey() {
  store.resolveHostKeyPrompt(hostKeyPrompt.value!.request_id, false);
  // 直接清 modal 而非 closeModal()：本函数就是 closeModal 的 hostKeyVerify 路由
  // 目标，再走 closeModal 会无限递归（× / Esc / danger 按钮三条路径共用此处）
  store.modal = { type: null, asset: null };
}

function denyMcpApproval() {
  store.resolveMcpApproval(mcpApprovalPrompt.value!.request_id, false);
  // 同 denyHostKey：防 closeModal → denyMcpApproval 递归
  store.modal = { type: null, asset: null };
}

// Esc / 遮罩关闭语义抽到 composables/useModalDismiss（行为契约见该文件）
const { onBackdropClick } = useModalDismiss(
  computed(() => modal.value.type),
  { closeModal, denyHostKey, denyMcpApproval }
);
</script>

<template>
  <!--
    Legacy .modal-layer / #modalLayer / .modal / #modalBody / .modal-actions
    selectors preserved for tests/ui-host-key.mjs Wave 5 expansion:
      - #modalLayer.open   (host-key test step 3)
      - #modalBody         (host-key test step 3)
      - #modalPrimary      (host-key test step 4)
      - .modal-actions .btn.danger  (host-key test step reject)
    closing 态：store 已关闭、退场动画播放中（view 滞留最后载荷，见 useModalView）
  -->
  <div class="modal-layer" id="modalLayer" :class="{ open: view.type, closing }" :aria-hidden="!view.type ? 'true' : 'false'" @click.self="onBackdropClick" @animationend="onExitAnimationEnd">
    <div class="modal" :class="`modal--${view.type}`" role="dialog" aria-modal="true" aria-labelledby="modalTitle">
      <div class="modal-head">
        <h2 id="modalTitle">{{ modalTitle }}</h2>
        <button class="icon-btn" id="modalClose" aria-label="关闭" @click="closeModal">×</button>
      </div>
      <div class="modal-body" id="modalBody">
        <!-- assetEditor（表单体抽到 AssetEditorContent；校验/保存在 submitModal） -->
        <AssetEditorContent
          v-if="view.type === 'assetEditor'"
          :asset="editingAsset"
          :credential="editingCredential"
          :group-options="groupOptions"
          :submitting="submitting"
          :form-error="assetFormError"
        />

        <!-- reauthPassword（表单体抽到 ReauthPasswordContent；提交在 submitModal） -->
        <ReauthPasswordContent
          v-else-if="view.type === 'reauthPassword'"
          :asset="view.asset!"
          :form="reauthForm"
        />

        <!-- tunnelCreate（表单体抽到 TunnelCreateContent；提交在 submitModal） -->
        <TunnelCreateContent
          v-else-if="view.type === 'tunnelCreate'"
          :form="tunnelForm"
        />

        <!-- hostKeyVerify -->
        <div v-else-if="view.type === 'hostKeyVerify'" class="stack">
          <p class="muted">检测到主机密钥，请确认是否信任该主机。</p>
          <dl class="context-grid">
            <dt>主机</dt><dd>{{ hostKeyPrompt?.host_port }}</dd>
            <dt>密钥类型</dt><dd>{{ hostKeyPrompt?.key_type }}</dd>
            <dt>指纹</dt><dd class="num" style="word-break:break-all">{{ hostKeyPrompt?.fingerprint }}</dd>
            <dt>状态</dt><dd>{{ hostKeyPrompt?.is_changed ? '密钥已变更（警告）' : '首次连接' }}</dd>
          </dl>
          <p class="muted">确认后将会保存到本地 known_hosts，下次连接不再提示。</p>
        </div>

        <!-- mcpApproval（v1.5）：MCP 高危工具审批，客户端不支持 elicitation 时弹此窗 -->
        <div v-else-if="view.type === 'mcpApproval'" class="stack">
          <p class="muted">外部 AI 客户端（如 ZCode）请求执行高危命令，请确认三段信息是否一致：</p>
          <dl class="context-grid">
            <dt>AI 声明意图</dt><dd>{{ mcpApprovalPrompt?.intent || '(未声明)' }}</dd>
            <dt>真实命令</dt><dd class="num" style="word-break:break-all">{{ mcpApprovalPrompt?.command }}</dd>
            <dt>后果预测</dt><dd>{{ mcpApprovalPrompt?.consequence }}</dd>
          </dl>
          <p class="muted">核对意图与命令是否相符后再确认。拒绝或关闭都会阻止执行。</p>
        </div>

        <!-- keyboardInteractive -->
        <div v-else-if="view.type === 'keyboardInteractive'" class="stack">
          <p class="muted">服务器需要键盘交互认证，请根据提示输入：</p>
          <p v-if="keyboardPrompt?.name"><strong>{{ keyboardPrompt.name }}</strong></p>
          <p v-if="keyboardInstructions" class="muted">{{ keyboardInstructions }}</p>
          <label v-for="(prompt, idx) in ((keyboardPrompt?.prompts as string[] | undefined) || [])" :key="idx" class="stack">
            <span class="muted">{{ prompt }}</span>
            <AppInput :model-value="keyboardResponses[idx]" type="password" :placeholder="prompt"
              @update:model-value="v => keyboardResponses[idx] = v" />
          </label>
        </div>

        <!-- v1.2 mcpPanel：MCP 服务可观测与配置引导（内容抽到子组件，避免本 SFC 超 500 行） -->
        <McpPanelContent v-else-if="view.type === 'mcpPanel'" />

        <!-- v1.3 syncPanel：Gist 资产同步管理（setup/push/pull/冲突/重置/清空） -->
        <SyncPanelContent v-else-if="view.type === 'syncPanel'" />

        <!-- v1.8 settings：统一设置中心（关于与更新/外观/同步/MCP，内容抽到子组件避免本 SFC 超 500 行） -->
        <SettingsPanelContent v-else-if="view.type === 'settings'" />

        <!-- mkdir / localMkdir / rename / localRename（表单体抽到 FileOpFormsContent；
             触发逻辑在 FileSurface，提交在 submitModal） -->
        <FileOpFormsContent
          v-else-if="view.type === 'mkdir' || view.type === 'localMkdir'"
          :kind="view.type"
          :base-path="view.type === 'mkdir' ? remotePath : localPath"
          :rename-target="renameTarget"
          :file-form="fileForm"
        />
        <FileOpFormsContent
          v-else-if="view.type === 'rename' || view.type === 'localRename'"
          :kind="view.type"
          :base-path="view.type === 'rename' ? remotePath : localPath"
          :rename-target="renameTarget"
          :file-form="fileForm"
        />

        <!-- terminalSearch (fallback — TerminalSurface handles inline) -->
        <div v-else-if="view.type === 'terminalSearch'" class="stack">
          <p class="muted">终端搜索由工具栏触发，此入口仅作兼容。</p>
        </div>

        <!-- confirmDelete -->
        <div v-else-if="view.type === 'confirmDelete'" class="stack">
          <p>将永久删除连接「<strong>{{ view.asset?.name }}</strong>」
            <span class="num muted">（{{ view.asset?.host }} · {{ view.asset?.username }}）</span></p>
          <p class="muted">同时清除已保存的密码 / 密钥凭据。此操作不可撤销。</p>
        </div>

        <!-- confirmFileDelete（S2：文件删除确认链，替代 window.confirm） -->
        <div v-else-if="view.type === 'confirmFileDelete'" class="stack">
          <p>将删除以下 <strong>{{ pendingFileDelete?.paths?.length || 0 }}</strong> 项：</p>
          <ul class="delete-file-list">
            <li v-for="name in pendingFileNames" :key="name" class="num">{{ name }}</li>
          </ul>
          <p v-if="pendingFileNamesMore > 0" class="muted">等 {{ pendingFileNamesMore }} 项</p>
          <p class="danger-note">此操作不可撤销，且不经回收站。</p>
        </div>

        <!-- confirmFileOverwrite（S2：上传覆盖同名确认） -->
        <div v-else-if="view.type === 'confirmFileOverwrite'" class="stack">
          <p>远程已存在同名文件，覆盖将替换其内容：</p>
          <p class="num overwrite-path">{{ view.payload?.path }}</p>
          <p class="muted">此操作不可撤销。</p>
        </div>

        <!-- renameGroup / createGroup / moveAsset（表单体抽到 GroupFormsContent） -->
        <GroupFormsContent
          v-else-if="view.type === 'renameGroup' || view.type === 'createGroup' || view.type === 'moveAsset'"
          :kind="view.type"
          :path="view.path"
          :asset-name="view.asset?.name"
          :group-options="groupOptions"
          :inputs="groupInputs"
          :form-error="groupFormError"
        />

        <!-- confirmCloseAssetWindow（Phase 1-A：独立资产窗口关闭确认，body 在子组件） -->
        <ConfirmCloseAssetWindowContent
          v-else-if="view.type === 'confirmCloseAssetWindow'"
          :count="view.count || 0"
        />

        <!-- editorDialog（v0.18 编辑器通用弹窗：消息 + 可选路径输入，按钮动态渲染） -->
        <div v-else-if="view.type === 'editorDialog'" class="editor-dialog-body">
          <p class="editor-dialog-message">{{ editorDialog?.message }}</p>
          <p v-if="editorDialog?.detail" class="editor-dialog-detail">{{ editorDialog.detail }}</p>
          <label v-if="editorDialog?.input" class="editor-dialog-field">
            <span>{{ editorDialog.input.label }}</span>
            <input
              v-model="editorDialogInput"
              class="native-input"
              type="text"
              spellcheck="false"
              :placeholder="editorDialog.input.placeholder"
            />
          </label>
          <p v-if="editorDialogError" class="form-error">{{ editorDialogError }}</p>
        </div>

        <!-- default: tokenConfig（PAT 表单已抽到 PatConfigCard，供此处与 settings 同步 tab 复用） -->
        <PatConfigCard v-else />
      </div>
      <div class="modal-actions">
        <button v-if="view.type === 'hostKeyVerify'" class="btn danger" :disabled="submitting" @click="denyHostKey">拒绝</button>
        <button v-else-if="view.type === 'mcpApproval'" class="btn danger" :disabled="submitting" @click="denyMcpApproval">拒绝执行</button>
        <!-- mcpPanel / syncPanel / settings / editorDialog 等自包含面板隐藏「取消」（操作在面板内部完成） -->
        <button v-if="view.type !== 'mcpPanel' && view.type !== 'syncPanel' && view.type !== 'settings' && view.type !== 'editorDialog'" class="btn" id="modalSecondary" :disabled="submitting" @click="secondaryAction">取消</button>
        <button
          v-if="view.type === 'confirmDelete'"
          class="btn danger"
          data-modal-primary-danger
          :disabled="submitting"
          @click="submitModal"
        >删除</button>
        <button
          v-else-if="view.type === 'confirmCloseAssetWindow'"
          class="btn danger"
          data-modal-primary-danger
          :disabled="submitting"
          @click="submitModal"
        >断开并关闭</button>
        <button
          v-else-if="view.type === 'confirmFileDelete'"
          class="btn danger"
          data-modal-primary-danger
          :disabled="submitting"
          @click="runFileDeleteConfirm"
        >
          <span v-if="submitting" class="btn-spinner" aria-hidden="true"></span>删除
        </button>
        <button
          v-else-if="view.type === 'confirmFileOverwrite'"
          class="btn primary"
          id="modalPrimary"
          :disabled="submitting"
          @click="store.confirmFileOverwrite"
        >覆盖</button>
        <!-- editorDialog：按钮完全由 payload 定义（danger/primary/顺序），逐个分发 -->
        <template v-else-if="view.type === 'editorDialog'">
          <button
            v-for="(btn, i) in editorDialog?.buttons || []"
            :key="btn.label + '-' + i"
            class="btn"
            :class="{ danger: btn.danger, primary: btn.primary }"
            :disabled="submitting"
            @click="runEditorDialog(i)"
          >
            <span v-if="submitting && btn.primary" class="btn-spinner" aria-hidden="true"></span>{{ btn.label }}
          </button>
        </template>
        <!-- v1.2 mcpPanel / v1.3 syncPanel / v1.8 settings：自包含面板，主按钮「关闭」 -->
        <button v-else-if="view.type === 'mcpPanel' || view.type === 'syncPanel' || view.type === 'settings'" class="btn primary" id="modalPrimary" :disabled="submitting" @click="submitModal">关闭</button>
        <button v-else class="btn primary" id="modalPrimary" :disabled="submitting" @click="submitModal">
          <span v-if="submitting" class="btn-spinner" aria-hidden="true"></span>确认
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped src="./GlobalModals.modal.css"></style>
