<script setup lang="ts">
/**
 * GlobalModals — Wave 3 Step 3.5
 *
 * Top-level modal hub. Hosts every modal that is NOT absorbed by a child
 * surface (FileSurface owns mkdir / localMkdir / rename / localRename actions —
 * their forms render here so legacy #modalLayer / #modalBody / #modalPrimary /
 * .modal-actions .btn.danger selectors keep working for tests/ui-host-key.mjs).
 *
 * Modal types handled (see domain ModalType):
 *   - assetEditor / reauthPassword / tunnelCreate / gatewayWizard / tokenConfig
 *   - hostKeyVerify / mcpApproval / keyboardInteractive
 *   - mcpPanel / settings / mkdir / localMkdir / rename / localRename
 *   - confirmDelete / confirmFileDelete / confirmFileOverwrite / renameGroup /
 *     createGroup / moveAsset / confirmCloseAssetWindow / editorDialog
 */
import { computed, ref, watch } from 'vue';
import { storeToRefs } from 'pinia';
import { useWorkbenchStore } from '@/stores/workbench';
import McpPanelContent from '@/components/shell/McpPanelContent.vue';
import SettingsPanelContent from '@/components/shell/SettingsPanelContent.vue';
import ConfirmCloseAssetWindowContent from '@/components/shell/ConfirmCloseAssetWindowContent.vue';
import PatConfigCard from '@/components/shell/PatConfigCard.vue';
import AssetEditorContent from '@/components/shell/AssetEditorContent.vue';
import ReauthPasswordContent from '@/components/shell/ReauthPasswordContent.vue';
import TunnelCreateContent from '@/components/shell/TunnelCreateContent.vue';
import GatewayWizardContent from '@/components/shell/GatewayWizardContent.vue';
import GroupFormsContent from '@/components/shell/GroupFormsContent.vue';
import FileOpFormsContent from '@/components/shell/FileOpFormsContent.vue';
import FileOverwriteContent from '@/components/shell/FileOverwriteContent.vue';
import SecurityPromptContent from '@/components/shell/SecurityPromptContent.vue';
import KeyboardInteractiveContent from '@/components/shell/KeyboardInteractiveContent.vue';
import { overwriteForm } from '@/lib/fileOverwrite';
import { dispatchModalSubmit, type ModalSubmitContext } from '@/lib/modalSubmit';
import { modalTitleFor } from '@/lib/modalTitles';
import { useModalDismiss } from '@/composables/useModalDismiss';
import { useModalFocus } from '@/composables/useModalFocus';
import { useModalView } from '@/composables/useModalView';
import { useModalFormSync, type EditorDialogViewPayload } from '@/composables/useModalFormSync';
import type { ModalState, ModalType, RemoteFileEntry } from '@/types/domain';

const KNOWN_MODAL_TYPES: ReadonlySet<ModalType> = new Set([
  'assetEditor', 'reauthPassword', 'tunnelCreate', 'gatewayWizard', 'tokenConfig',
  'hostKeyVerify', 'keyboardInteractive', 'mcpApproval', 'mcpPanel', 'settings',
  'mkdir', 'rename', 'localMkdir', 'localRename', 'confirmDelete',
  'confirmFileDelete', 'confirmFileOverwrite', 'renameGroup', 'createGroup',
  'moveAsset', 'confirmCloseAssetWindow', 'confirmBatchCloseTabs', 'editorDialog'
]);
const DESTRUCTIVE_MODAL_TYPES: ReadonlySet<string> = new Set([
  'confirmDelete', 'confirmFileDelete', 'confirmFileOverwrite', 'confirmCloseAssetWindow', 'confirmBatchCloseTabs'
]);
const SELF_CONTAINED_MODAL_TYPES: ReadonlySet<string> = new Set([
  'mcpPanel', 'settings', 'editorDialog', 'gatewayWizard'
]);

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
const editorDialog = computed<EditorDialogViewPayload | null>(() =>
  view.value.type === 'editorDialog'
    ? ((view.value.payload as unknown as EditorDialogViewPayload | undefined) ?? {})
    : null
);
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
const modalPanelRef = ref<HTMLElement | null>(null);
const isKnownViewType = computed(() => Boolean(view.value.type && KNOWN_MODAL_TYPES.has(view.value.type)));
const isDestructiveModal = computed(() => Boolean(view.value.type && DESTRUCTIVE_MODAL_TYPES.has(view.value.type)));
const isSelfContainedModal = computed(() => Boolean(view.value.type && SELF_CONTAINED_MODAL_TYPES.has(view.value.type)));

if (import.meta.env.DEV) {
  watch(() => modal.value.type, (t) => {
    if (t && !KNOWN_MODAL_TYPES.has(t)) {
      store.announce(`未知弹窗类型：${String(t)}`, { level: 'error' });
    }
  });
}

// 退场动画视图层：关闭瞬间 store 已置空，view 滞留最后载荷让淡出播完（状态即时、渲染滞后）
const { view, closing, onExitAnimationEnd } = useModalView(modal);

// Tab 圈闭 + 初始焦点策略 + 关闭还原触发焦点（10 A2；Esc 仍由 useModalDismiss 分发）
useModalFocus({
  open: () => Boolean(modal.value.type),
  containerRef: modalPanelRef,
  activeKey: () => modal.value.type
});

// ============================================================
// 表单状态 + modal 切换同步：v0.20 拆至 composables/useModalFormSync（S2 刀，
// 行为零变化；submitting 是提交生命周期态，留在本组件）
// ============================================================
const {
  editingAsset, editingCredential, tunnelForm, fileForm, renameTarget,
  keyboardResponses, groupInputs, assetFormError, groupFormError, reauthForm,
  editorDialogInput, editorDialogError, keyboardInstructions
} = useModalFormSync({ modal, keyboardPrompt });

// 异步提交进行中：主/副按钮禁用 + spinner，防止重复提交（提交生命周期态，
// 与表单内容状态不同源——留在本组件而非 composable）
const submitting = ref(false);

// 可选分组列表（资产编辑器与移动分组共用）：显式声明 ∪ 资产现有 group，去重，含「未分组」。
// AppSelect 选项形态：value 即分组路径（含多级「生产/数据库」），label 同值。
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
    // accept/确认后的直关（绕过 closeModal 对安全审批类的 deny 路由）
    closeDirect: () => { store.modal = { type: null, asset: null }; },
    announce: (message, opts) => store.announce(message, opts),
    get assets() { return store.assets; }
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

// Esc 关闭语义抽到 composables/useModalDismiss（行为契约见该文件）。
// 遮罩空白点击不关闭弹窗（防误触丢表单）：关闭只走右上角 × / 确认 / 取消按钮 + Esc。
useModalDismiss(
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
  <div class="modal-layer" id="modalLayer" :class="{ open: view.type, closing }" :aria-hidden="!view.type ? 'true' : 'false'" @animationend="onExitAnimationEnd">
    <div ref="modalPanelRef" class="modal" tabindex="-1" :class="view.type ? `modal--${view.type}` : undefined" role="dialog" aria-modal="true" aria-labelledby="modalTitle">
      <div class="modal-head">
        <h2 id="modalTitle">{{ modalTitle }}</h2>
        <button
          v-if="view.type === 'hostKeyVerify' || view.type === 'mcpApproval'"
          type="button"
          class="head-reject-btn"
          id="modalClose"
          aria-label="拒绝并关闭 (Esc)"
          title="拒绝并关闭 (Esc)"
          @click="closeModal"
        >拒绝并关闭</button>
        <button
          v-else
          type="button"
          class="icon-btn"
          id="modalClose"
          data-modal-close
          aria-label="关闭 (Esc)"
          title="关闭 (Esc)"
          @click="closeModal"
        >×</button>
      </div>
      <div class="modal-body" id="modalBody" data-modal-body>
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

        <!-- gatewayWizard（v0.22 网关部署向导：两段式自包含，表单/提交均在组件内） -->
        <GatewayWizardContent
          v-else-if="view.type === 'gatewayWizard'"
        />

        <!-- hostKeyVerify / mcpApproval（10 §3.2 抽至 SecurityPromptContent） -->
        <SecurityPromptContent
          v-else-if="view.type === 'hostKeyVerify' || view.type === 'mcpApproval'"
          :kind="view.type"
          :host-key-prompt="hostKeyPrompt"
          :mcp-approval-prompt="mcpApprovalPrompt"
        />

        <!-- keyboardInteractive（10 A11 显示/隐藏切换抽至 KeyboardInteractiveContent） -->
        <KeyboardInteractiveContent
          v-else-if="view.type === 'keyboardInteractive'"
          :prompt="keyboardPrompt"
          :instructions="keyboardInstructions || ''"
          :responses="keyboardResponses"
        />

        <!-- v1.2 mcpPanel：MCP 服务可观测与配置引导（内容抽到子组件，避免本 SFC 超 500 行） -->
        <McpPanelContent v-else-if="view.type === 'mcpPanel'" />

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

        <!-- confirmFileOverwrite（B2 冲突三选：覆盖/跳过/重命名，body 在子组件） -->
        <FileOverwriteContent
          v-else-if="view.type === 'confirmFileOverwrite'"
          :direction="(view.payload?.direction as 'upload' | 'download') || 'upload'"
          :path="String(view.payload?.path || '')"
          :form="overwriteForm"
          :has-batch="!!view.payload?.batchId"
        />

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

        <!-- confirmBatchCloseTabs（03 A15：关闭其他 / 关闭右侧终端标签确认） -->
        <div v-else-if="view.type === 'confirmBatchCloseTabs'" class="stack">
          <p>将断开并关闭 <strong>{{ Number(view.payload?.count || 0) }}</strong> 个终端会话（{{ view.payload?.scopeLabel || '其他标签' }}）。</p>
          <p class="danger-note">断开后对应远程会话内未完成的前台进程将随之终止。</p>
        </div>

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

        <!-- tokenConfig 显式分支（PAT 表单已抽到 PatConfigCard；10 A7 未知 type 独立守卫） -->
        <PatConfigCard v-else-if="view.type === 'tokenConfig'" />
        <div v-else-if="view.type" class="stack">
          <p class="form-error">未知弹窗类型（{{ view.type }}），请关闭后重试。</p>
        </div>
      </div>
      <div class="modal-actions">
        <button v-if="view.type === 'hostKeyVerify'" id="modalDeny" data-modal-autofocus class="btn danger" :disabled="submitting" @click="denyHostKey">拒绝</button>
        <button v-else-if="view.type === 'mcpApproval'" id="modalDeny" data-modal-autofocus class="btn danger" :disabled="submitting" @click="denyMcpApproval">拒绝执行</button>
        <!-- mcpPanel / settings / editorDialog / gatewayWizard 自包含面板及未知守卫隐藏「取消」 -->
        <button
          v-if="isKnownViewType && !isSelfContainedModal"
          class="btn"
          id="modalSecondary"
          :data-modal-autofocus="isDestructiveModal ? '' : undefined"
          :disabled="submitting"
          @click="secondaryAction"
        >取消</button>
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
          v-else-if="view.type === 'confirmBatchCloseTabs'"
          class="btn danger"
          data-modal-primary-danger
          :disabled="submitting"
          @click="submitModal"
        >全部关闭</button>
        <button
          v-else-if="view.type === 'confirmFileDelete'"
          class="btn danger"
          :class="{ 'is-loading': submitting }"
          data-modal-primary-danger
          :disabled="submitting"
          @click="runFileDeleteConfirm"
        >
          <span class="btn-spinner-wrap" aria-hidden="true"><span class="btn-spinner"></span></span>
          <span class="btn-label">删除</span>
        </button>
        <!-- B2 冲突三选：重命名 + 覆盖同属一支（template 包裹保持 v-else-if 链） -->
        <template v-else-if="view.type === 'confirmFileOverwrite'">
          <button
            class="btn"
            id="modalRename"
            :disabled="submitting || overwriteForm.applyAll"
            :title="overwriteForm.applyAll ? '已勾选「应用到剩余」，本批按统一策略处理；如需逐个重命名请取消勾选' : '以新名称传输（输入框）'"
            @click="store.renameFileOverwrite(overwriteForm.rename)"
          >重命名</button>
          <button
            class="btn danger"
            :class="{ 'is-loading': submitting }"
            id="modalPrimary"
            data-modal-primary-danger
            :disabled="submitting"
            @click="store.confirmFileOverwrite"
          >
            <span class="btn-spinner-wrap" aria-hidden="true"><span class="btn-spinner"></span></span>
            <span class="btn-label">覆盖</span>
          </button>
        </template>
        <!-- editorDialog：按钮完全由 payload 定义（danger/primary/顺序），逐个分发 -->
        <template v-else-if="view.type === 'editorDialog'">
          <button
            v-for="(btn, i) in editorDialog?.buttons || []"
            :key="btn.label + '-' + i"
            class="btn"
            :class="{ danger: btn.danger, primary: btn.primary, 'is-loading': submitting && btn.primary }"
            :disabled="submitting"
            @click="runEditorDialog(i)"
          >
            <span v-if="btn.primary" class="btn-spinner-wrap" aria-hidden="true"><span class="btn-spinner"></span></span>
            <span class="btn-label">{{ btn.label }}</span>
          </button>
        </template>
        <!-- 自包含面板：主按钮「关闭」 -->
        <button v-else-if="view.type === 'mcpPanel' || view.type === 'settings' || view.type === 'gatewayWizard'" class="btn primary" id="modalPrimary" :disabled="submitting" @click="submitModal">关闭</button>
        <button v-else-if="isKnownViewType" class="btn primary" :class="{ 'is-loading': submitting }" id="modalPrimary" :disabled="submitting" @click="submitModal">
          <span class="btn-spinner-wrap" aria-hidden="true"><span class="btn-spinner"></span></span>
          <span class="btn-label">确认</span>
        </button>
        <button v-else class="btn primary" id="modalPrimary" @click="closeModal">关闭</button>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss" src="./GlobalModals.modal.scss"></style>
