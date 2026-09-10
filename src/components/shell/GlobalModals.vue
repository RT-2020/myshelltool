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
import { computed, onBeforeUnmount, reactive, ref, watch } from 'vue';
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
import AssetPrivateKeyInput from '@/components/shell/AssetPrivateKeyInput.vue';
import { openPrivateKeyFileDialog } from '@/services/backend';
import type {
  ConnectionAssetInput,
  ModalState,
  RemoteFileEntry
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

// ============================================================
// Local form state — mirrors the App.vue reactive forms we deleted.
// All of these are only ever visible while modal.type matches their
// respective branch, so they don't bleed across types.
// ============================================================
/** 资产编辑表单（tags 编辑态为逗号串，port/auth_method 收 AppInput/AppSelect 的 string 回传）。 */
interface AssetEditorForm {
  id: string;
  name: string;
  host: string;
  port: number | string;
  username: string;
  auth_method: string | number;
  private_key_path: string;
  group: string;
  tags: string;
  status: string;
  credential_id: string | null;
  passphrase_credential_id: string | null;
  private_key_credential_id: string | null;
}

/** 资产编辑器的凭据表单（clear* 为「清除已存凭据」标记，与重新输入互斥）。 */
interface CredentialEditorForm {
  password: string;
  passphrase: string;
  privateKey: string;
  clearPassword: boolean;
  clearPassphrase: boolean;
  clearPrivateKey: boolean;
}

/** 隧道新建表单（端口为 string，提交时 Number() 转换）。 */
interface TunnelEditorForm {
  name: string;
  kind: string | number;
  local_addr: string;
  local_port: string;
  remote_addr: string;
  remote_port: string;
  auto_start: boolean;
}

const editingAsset = reactive(emptyAsset());
const editingCredential = reactive(emptyCredential());
const tunnelForm = reactive(emptyTunnelForm());
const mkdirName = ref('');
const renameTarget = reactive({ path: '', current: '', next: '' });
const keyboardResponses = reactive<Record<string, string>>({});

// ---- 分组管理 / 删除确认 表单状态 ----
// renameGroup: 输入新分组名（单段，禁 '/');由 modal.value.path 提供旧路径
const renameGroupInput = ref('');
// createGroup: 输入分组路径（可含 '/' 建子级）
const createGroupInput = ref('');
// moveAsset: 目标分组路径
const moveGroupInput = ref('');
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

// 认证方式选项。Token 认证后端支持存疑，本轮不加（follow-up：确认 save_credential
// / ssh_connect 的 token 语义后再补选项）。
const authMethodOptions = [
  { label: 'Password', value: 'Password' },
  { label: 'PrivateKey', value: 'PrivateKey' }
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
    case 'reauthPassword': return '重新输入密码';
    case 'tunnelCreate': return '新增隧道';
    case 'tokenConfig': return '配置 / 更新 GitHub token';
    case 'hostKeyVerify': return '主机密钥验证';
    case 'keyboardInteractive': return '键盘交互认证';
    case 'mcpApproval': return 'MCP 高危操作审批';
    case 'mcpPanel': return 'MCP 服务管理';
    case 'syncPanel': return '资产同步（Gist）';
    case 'settings': return '设置';
    case 'mkdir': return '新建远程目录';
    case 'rename': return '重命名远程条目';
    case 'localMkdir': return '新建本地目录';
    case 'localRename': return '重命名本地条目';
    case 'terminalSearch': return '终端搜索';
    case 'confirmDelete': return '删除连接资产';
    case 'confirmFileDelete': return '删除文件确认';
    case 'confirmFileOverwrite': return '覆盖同名文件？';
    case 'renameGroup': return '重命名分组';
    case 'createGroup': return '新建分组';
    case 'moveAsset': return '移动到分组';
    case 'confirmCloseAssetWindow': return '关闭独立窗口';
    default: return '提示';
  }
});

const assetCredentialHint = computed(() => {
  if (!editingAsset.id) return '新连接，凭据与密钥将在保存时加密存储到安全保管箱';
  if (editingAsset.auth_method === 'PrivateKey') {
    if (editingAsset.private_key_credential_id) return '私钥已在安全保管箱托管（支持跨设备端到端加密同步）';
    if (editingAsset.private_key_path) return '当前使用物理文件路径，建议导入到保管箱以便换机同步';
    return '尚未配置私钥';
  }
  if (editingAsset.credential_id) return '密码已存储（重新输入会覆盖）';
  return '尚未存储密码';
});

// 可选分组建议列表（资产编辑器与移动分组共用）：显式声明 ∪ 资产现有 group，去重，含「未分组」。
const groupOptions = computed(() => {
  const set = new Set(['未分组']);
  for (const g of (store.declaredGroups || [])) set.add(g);
  for (const a of (store.assets || [])) if (a.group) set.add(a.group);
  return [...set];
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
  if (type === 'assetEditor') {
    Object.assign(editingAsset, modal.value.asset ? cloneAsset(modal.value.asset) : emptyAsset());
    Object.assign(editingCredential, emptyCredential());
    assetFormError.value = '';
  }
  if (type === 'reauthPassword') {
    reauthForm.password = '';
    reauthForm.error = '';
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
  if (type === 'confirmCloseAssetWindow') {
    // 纯确认弹窗（无表单状态）：count/onConfirm 直接读 modal payload
  }
});

function emptyAsset(): AssetEditorForm {
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
    passphrase_credential_id: null,
    private_key_credential_id: null
  };
}

function emptyCredential(): CredentialEditorForm {
  // clearPassword / clearPassphrase / clearPrivateKey：编辑器「清除」按钮的标记（保存时删除凭据）。
  // 与重新输入互斥：输入框有值会重置对应标记。
  return {
    password: '',
    passphrase: '',
    privateKey: '',
    clearPassword: false,
    clearPassphrase: false,
    clearPrivateKey: false
  };
}

function emptyTunnelForm(): TunnelEditorForm {
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

function cloneAsset(asset: NonNullable<ModalState['asset']>): AssetEditorForm {
  return {
    ...asset,
    tags: asset.tags.join(', '),
    private_key_path: asset.private_key_path || ''
  };
}

function splitTags(tags: string | string[]): string[] {
  return Array.isArray(tags) ? tags : String(tags || '').split(/[·,，\s]+/).filter(Boolean);
}

// 浏览选择私钥文件（系统对话框）；用户取消或浏览器预览模式不影响手输路径
async function browsePrivateKey() {
  try {
    const path = await openPrivateKeyFileDialog();
    if (path) editingAsset.private_key_path = path;
  } catch {
    // 浏览器预览模式（无 Tauri runtime）无系统对话框——静默回退到手输路径
  }
}

// 标记清除已存凭据（保存时生效；再次点击撤销；重新输入也会撤销）
function toggleClearPassword() {
  editingCredential.clearPassword = !editingCredential.clearPassword;
}

function toggleClearPassphrase() {
  editingCredential.clearPassphrase = !editingCredential.clearPassphrase;
}

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

async function submitModal() {
  if (submitting.value) return;
  switch (modal.value.type) {
    case 'assetEditor':
      if (editingAsset.auth_method === 'Password' && !editingAsset.credential_id && !editingCredential.password && !editingCredential.clearPassword) {
        assetFormError.value = '首次保存 Password 认证需填写密码；若要用私钥登录，请将认证方式切换为 PrivateKey。';
        return;
      }
      assetFormError.value = '';
      await runSubmit(() =>
        store.saveAsset(
          // 表单态（port/auth_method 可能被输入控件回传 string）按 saveAsset 入参契约断言透传，与迁移前运行时行为一致
          {
            ...editingAsset,
            private_key_path: String(editingAsset.private_key_path || '').trim() || null,
            tags: splitTags(editingAsset.tags)
          } as ConnectionAssetInput,
          {
            password: editingCredential.password,
            passphrase: editingCredential.passphrase,
            privateKey: editingCredential.privateKey,
            clearPassword: editingCredential.clearPassword,
            clearPassphrase: editingCredential.clearPassphrase,
            clearPrivateKey: editingCredential.clearPrivateKey
          }
        )
      );
      return;
    case 'confirmDelete':
      // confirmDelete 弹窗打开时必有 asset（payload 契约），断言仅类型层收窄
      await runSubmit(() => store.deleteAsset(modal.value.asset?.id as string));
      return;
    case 'renameGroup': {
      const newName = renameGroupInput.value.trim();
      if (!newName) {
        groupFormError.value = '请输入新名称';
        return;
      }
      if (newName.includes('/')) {
        groupFormError.value = '名称不能包含 /';
        return;
      }
      const oldPath = modal.value.path || '';
      const parent = oldPath.includes('/') ? oldPath.slice(0, oldPath.lastIndexOf('/')) : '';
      const newPath = parent ? `${parent}/${newName}` : newName;
      await runSubmit(() => store.renameGroup(oldPath, newPath));
      return;
    }
    case 'createGroup': {
      const path = createGroupInput.value.trim();
      if (!path) {
        groupFormError.value = '请输入分组路径';
        return;
      }
      await runSubmit(() => store.createGroup(path));
      return;
    }
    case 'moveAsset': {
      const target = moveGroupInput.value.trim() || '未分组';
      if (!target.trim()) {
        groupFormError.value = '请输入目标分组';
        return;
      }
      // moveAsset 弹窗打开时必有 asset（payload 契约），断言仅类型层收窄
      await runSubmit(() => store.moveAsset(modal.value.asset?.id as string, target));
      return;
    }
    case 'tokenConfig':
      // v1.8：PAT 表单已抽到 PatConfigCard（自带保存按钮，自包含）。
      // 主按钮「确认」仅关闭弹窗，保存动作在卡片内完成。
      closeModal();
      return;
    case 'tunnelCreate':
      await runSubmit(() =>
        store.createTunnel({
          ...tunnelForm,
          local_port: Number(tunnelForm.local_port),
          remote_port: Number(tunnelForm.remote_port)
        })
      );
      return;
    case 'mkdir':
      if (await runSubmit(() => store.mkdirRemote(mkdirName.value))) closeModal();
      return;
    case 'localMkdir':
      if (await runSubmit(() => store.localMkdir(mkdirName.value))) closeModal();
      return;
    case 'rename':
      if (await runSubmit(() => store.renameRemote({ path: renameTarget.path, name: renameTarget.current } as RemoteFileEntry, renameTarget.next))) closeModal();
      return;
    case 'localRename':
      if (await runSubmit(() => store.localRename(renameTarget.path, renameTarget.next))) closeModal();
      return;
    case 'hostKeyVerify':
      store.resolveHostKeyPrompt(hostKeyPrompt.value!.request_id, true);
      // 不走 closeModal()：它对 hostKeyVerify 路由到 denyHostKey()，会在 accept
      // 之后又补发一次 accepted:false（denyHostKey → closeModal 还会递归）
      store.modal = { type: null, asset: null };
      return;
    case 'confirmCloseAssetWindow':
      // onConfirm 自管理断开与窗口 destroy，不 await——立即关 modal 让流程接管
      modal.value.onConfirm?.();
      store.modal = { type: null, asset: null };
      return;
    case 'mcpApproval':
      store.resolveMcpApproval(mcpApprovalPrompt.value!.request_id, true);
      // 同 hostKeyVerify：closeModal 会路由 denyMcpApproval 补发 accepted:false
      store.modal = { type: null, asset: null };
      return;
    case 'keyboardInteractive':
      store.resolveKeyboardPrompt(keyboardPrompt.value!.request_id, Object.values(keyboardResponses));
      closeModal();
      return;
    case 'terminalSearch': {
      // 事实备注：workbench store 未导出 executeTerminalSearch（迁移前即如此，运行时
      // 该调用抛 TypeError 未捕获；该分支为兼容入口，正常运行流程不会进入）。按
      // 「行为零变化」保留原调用语义（无条件调用，非可选链）。
      const sessionId = modal.value.payload?.sessionId as string | undefined;
      if (sessionId) {
        (store as unknown as { executeTerminalSearch: (sessionId: string, query: string) => void })
          .executeTerminalSearch(sessionId, store.terminalSearch.query);
      }
      return;
    }
    case 'reauthPassword': {
      if (!reauthForm.password) {
        reauthForm.error = '请输入密码';
        return;
      }
      reauthForm.error = '';
      await runSubmit(async () => {
        // 从 store 取最新资产（modal 里的 asset 是打开弹窗时的快照，可能已被编辑过）
        const current = (store.assets || []).find(a => a && a.id === modal.value.asset?.id) || modal.value.asset;
        // saveAsset 成功后会关闭 modal，先取 sessionId 再调用；
        // 切到密码认证时顺带清掉残留的 passphrase 凭据（PrivateKey 专属，避免换回时旧值干扰）
        const sessionId = modal.value.payload?.sessionId as string | undefined;
        await store.saveAsset(
          { ...current, auth_method: 'Password' },
          { password: reauthForm.password, clearPassphrase: true }
        );
        if (sessionId) await store.reconnectSession(sessionId);
      });
      return;
    }
    default:
      closeModal();
  }
}

/**
 * 执行异步提交：期间 submitting=true（按钮禁用 + spinner，防重复提交）。
 * 成功返回 true；失败返回 false（错误由对应 store action announce，弹窗保持打开可重试）。
 */
async function runSubmit(task: () => Promise<unknown>): Promise<boolean> {
  if (submitting.value) return false;
  submitting.value = true;
  try {
    await task();
    return true;
  } catch (error) {
    // 提交失败必须可见（部分 store action 不自带失败 announce），弹窗保持打开可重试
    store.announce('操作失败：' + ((error as Error | undefined)?.message || error), { level: 'error' });
    return false;
  } finally {
    submitting.value = false;
  }
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

// ============================================================
// Esc / 遮罩关闭映射（S2，v2.3 拆分 Esc 与遮罩点击）：
//   - Esc：hostKeyVerify / mcpApproval → 对应的 deny（安全语义：Esc=拒绝，
//     键盘操作明确无误触风险，保留 fail-secure）；其余 → closeModal
//   - 遮罩空白点击：hostKeyVerify / mcpApproval → 完全不响应（防误触点空白
//     即拒绝审批/断开连接，只能通过弹窗内按钮操作）；其余 → closeModal
// 注册时机 watch(modal.type)：弹窗打开注册、关闭移除，避免全局常驻监听。
// ============================================================
function onBackdropClick() {
  const type = modal.value.type;
  if (!type) return;
  // 安全审批类弹窗：遮罩点击不产生任何效果（防误操作）
  if (type === 'hostKeyVerify' || type === 'mcpApproval') return;
  closeModal();
}

function dismissByEsc() {
  const type = modal.value.type;
  if (!type) return;
  if (type === 'hostKeyVerify') { denyHostKey(); return; }
  if (type === 'mcpApproval') { denyMcpApproval(); return; }
  closeModal();
}

function onKeydownEsc(event: KeyboardEvent) {
  if (event.key !== 'Escape') return;
  event.preventDefault();
  dismissByEsc();
}

watch(() => modal.value.type, type => {
  if (type) window.addEventListener('keydown', onKeydownEsc);
  else window.removeEventListener('keydown', onKeydownEsc);
});
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydownEsc));
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
  <div class="modal-layer" id="modalLayer" :class="{ open: modal.type }" :aria-hidden="!modal.type ? 'true' : 'false'" @click.self="onBackdropClick">
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
              <!-- 用原生 input 而非 AppInput：AppInput 不透传 list 属性，
                   分组需 datalist 自动补全（选已有）+ 允许输入新路径（新建），故用裸 input。 -->
              <input
                class="native-input"
                :value="editingAsset.group"
                @input="e => editingAsset.group = (e.target as HTMLInputElement).value"
                list="group-list-editor"
                placeholder="未分组 或 生产/数据库"
                data-asset-field="group"
              />
              <datalist id="group-list-editor">
                <option v-for="g in groupOptions" :key="g" :value="g"></option>
              </datalist>
            </label>
            <label class="stack"><span class="muted">标签</span>
              <AppInput :model-value="editingAsset.tags" placeholder="prod, app" @update:model-value="v => editingAsset.tags = v" data-asset-field="tags" />
            </label>
            <label class="stack"><span class="muted">认证方式</span>
              <AppSelect :model-value="editingAsset.auth_method" :options="authMethodOptions"
                @update:model-value="v => editingAsset.auth_method = v" />
            </label>
          </div>
          <div class="callout">
            <strong>凭据与密钥安全</strong>
            <p class="muted">{{ assetCredentialHint }}</p>
          </div>
          <p v-if="assetFormError" class="form-error">{{ assetFormError }}</p>

          <!-- Password 认证模式 -->
          <div v-if="editingAsset.auth_method === 'Password'" class="grid-2">
            <label class="stack">
              <span class="muted">密码（明文不会回显，仅保存到本地安全存储）
                <span v-if="!editingAsset.credential_id" style="color:var(--danger)"> · 首次保存必填</span>
              </span>
              <div class="inline-field-row">
                <AppInput :model-value="editingCredential.password" type="password" placeholder="首次保存必填；编辑时留空保留既有密码"
                  @update:model-value="v => { editingCredential.password = v; editingCredential.clearPassword = false; }" data-asset-field="password" />
                <AppButton v-if="editingAsset.credential_id" size="sm" variant="danger" @click="toggleClearPassword">
                  {{ editingCredential.clearPassword ? '撤销清除' : '清除密码' }}
                </AppButton>
              </div>
            </label>
          </div>

          <!-- PrivateKey 认证模式（私钥托管箱 + 本地路径 + 口令） -->
          <div v-else-if="editingAsset.auth_method === 'PrivateKey'" class="stack">
            <AssetPrivateKeyInput
              :asset="editingAsset"
              :credential="editingCredential"
              :disabled="submitting"
            />
            <label class="stack" style="max-width: 480px;">
              <span class="muted">Passphrase（可选，无口令保护的私钥留空）</span>
              <div class="inline-field-row">
                <AppInput :model-value="editingCredential.passphrase" type="password" placeholder="无加密私钥留空"
                  @update:model-value="v => { editingCredential.passphrase = v; editingCredential.clearPassphrase = false; }" data-asset-field="passphrase" />
                <AppButton v-if="editingAsset.passphrase_credential_id" size="sm" variant="danger" @click="toggleClearPassphrase">
                  {{ editingCredential.clearPassphrase ? '撤销清除' : '清除 passphrase' }}
                </AppButton>
              </div>
            </label>
          </div>
        </div>

        <!-- reauthPassword：认证失败快捷重连（TerminalSurface 错误卡片入口） -->
        <div v-else-if="modal.type === 'reauthPassword'" class="stack">
          <p class="muted">
            连接 <strong>{{ modal.asset?.name }}</strong>（{{ modal.asset?.username }}@{{ modal.asset?.host }}）认证失败。
            输入登录密码后将切换为密码认证并自动重连。
          </p>
          <label class="stack"><span class="muted">密码</span>
            <AppInput :model-value="reauthForm.password" type="password" placeholder="服务器登录密码"
              @update:model-value="v => reauthForm.password = v" data-asset-field="reauth_password" />
          </label>
          <p v-if="reauthForm.error" class="form-error">{{ reauthForm.error }}</p>
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

        <!-- mcpApproval（v1.5）：MCP 高危工具审批，客户端不支持 elicitation 时弹此窗 -->
        <div v-else-if="modal.type === 'mcpApproval'" class="stack">
          <p class="muted">外部 AI 客户端（如 ZCode）请求执行高危命令，请确认三段信息是否一致：</p>
          <dl class="context-grid">
            <dt>AI 声明意图</dt><dd>{{ mcpApprovalPrompt?.intent || '(未声明)' }}</dd>
            <dt>真实命令</dt><dd class="num" style="word-break:break-all">{{ mcpApprovalPrompt?.command }}</dd>
            <dt>后果预测</dt><dd>{{ mcpApprovalPrompt?.consequence }}</dd>
          </dl>
          <p class="muted">核对意图与命令是否相符后再确认。拒绝或关闭都会阻止执行。</p>
        </div>

        <!-- keyboardInteractive -->
        <div v-else-if="modal.type === 'keyboardInteractive'" class="stack">
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
        <McpPanelContent v-else-if="modal.type === 'mcpPanel'" />

        <!-- v1.3 syncPanel：Gist 资产同步管理（setup/push/pull/冲突/重置/清空） -->
        <SyncPanelContent v-else-if="modal.type === 'syncPanel'" />

        <!-- v1.8 settings：统一设置中心（关于与更新/外观/同步/MCP，内容抽到子组件避免本 SFC 超 500 行） -->
        <SettingsPanelContent v-else-if="modal.type === 'settings'" />

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

        <!-- confirmFileDelete（S2：文件删除确认链，替代 window.confirm） -->
        <div v-else-if="modal.type === 'confirmFileDelete'" class="stack">
          <p>将删除以下 <strong>{{ pendingFileDelete?.paths?.length || 0 }}</strong> 项：</p>
          <ul class="delete-file-list">
            <li v-for="name in pendingFileNames" :key="name" class="num">{{ name }}</li>
          </ul>
          <p v-if="pendingFileNamesMore > 0" class="muted">等 {{ pendingFileNamesMore }} 项</p>
          <p class="danger-note">此操作不可撤销，且不经回收站。</p>
        </div>

        <!-- confirmFileOverwrite（S2：上传覆盖同名确认） -->
        <div v-else-if="modal.type === 'confirmFileOverwrite'" class="stack">
          <p>远程已存在同名文件，覆盖将替换其内容：</p>
          <p class="num overwrite-path">{{ modal.payload?.path }}</p>
          <p class="muted">此操作不可撤销。</p>
        </div>

        <!-- renameGroup -->
        <div v-else-if="modal.type === 'renameGroup'" class="stack">
          <p class="muted">重命名分组「{{ modal.path }}」的最后一段名称。
            （改层级路径请用「新建分组」+「移动」组合）</p>
          <label class="stack"><span>新名称（不含 '/'）</span>
            <AppInput :model-value="renameGroupInput" placeholder="分组名"
              @update:model-value="v => renameGroupInput = v" />
          </label>
          <p v-if="groupFormError" class="form-error">{{ groupFormError }}</p>
        </div>

        <!-- createGroup -->
        <div v-else-if="modal.type === 'createGroup'" class="stack">
          <p class="muted">输入分组路径，可用「/」创建多级嵌套分组，如「生产/数据库」。</p>
          <label class="stack"><span>分组路径</span>
            <AppInput :model-value="createGroupInput" placeholder="生产/数据库"
              @update:model-value="v => createGroupInput = v" />
          </label>
          <p v-if="groupFormError" class="form-error">{{ groupFormError }}</p>
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
              <option v-for="g in groupOptions" :key="g" :value="g"></option>
            </datalist>
          </label>
          <p v-if="groupFormError" class="form-error">{{ groupFormError }}</p>
        </div>

        <!-- confirmCloseAssetWindow（Phase 1-A：独立资产窗口关闭确认，body 在子组件） -->
        <ConfirmCloseAssetWindowContent
          v-else-if="modal.type === 'confirmCloseAssetWindow'"
          :count="modal.count || 0"
        />

        <!-- default: tokenConfig（PAT 表单已抽到 PatConfigCard，供此处与 settings 同步 tab 复用） -->
        <PatConfigCard v-else />
      </div>
      <div class="modal-actions">
        <button v-if="modal.type === 'hostKeyVerify'" class="btn danger" :disabled="submitting" @click="denyHostKey">拒绝</button>
        <button v-else-if="modal.type === 'mcpApproval'" class="btn danger" :disabled="submitting" @click="denyMcpApproval">拒绝执行</button>
        <!-- mcpPanel / syncPanel / settings 等自包含面板隐藏「取消」（操作在面板内部完成） -->
        <button v-if="modal.type !== 'mcpPanel' && modal.type !== 'syncPanel' && modal.type !== 'settings'" class="btn" id="modalSecondary" :disabled="submitting" @click="secondaryAction">取消</button>
        <button
          v-if="modal.type === 'confirmDelete'"
          class="btn danger"
          data-modal-primary-danger
          :disabled="submitting"
          @click="submitModal"
        >删除</button>
        <button
          v-else-if="modal.type === 'confirmCloseAssetWindow'"
          class="btn danger"
          data-modal-primary-danger
          :disabled="submitting"
          @click="submitModal"
        >断开并关闭</button>
        <button
          v-else-if="modal.type === 'confirmFileDelete'"
          class="btn danger"
          data-modal-primary-danger
          :disabled="submitting"
          @click="runFileDeleteConfirm"
        >
          <span v-if="submitting" class="btn-spinner" aria-hidden="true"></span>删除
        </button>
        <button
          v-else-if="modal.type === 'confirmFileOverwrite'"
          class="btn primary"
          id="modalPrimary"
          :disabled="submitting"
          @click="store.confirmFileOverwrite"
        >覆盖</button>
        <!-- v1.2 mcpPanel / v1.3 syncPanel / v1.8 settings：自包含面板，主按钮「关闭」 -->
        <button v-else-if="modal.type === 'mcpPanel' || modal.type === 'syncPanel' || modal.type === 'settings'" class="btn primary" id="modalPrimary" :disabled="submitting" @click="submitModal">关闭</button>
        <button v-else class="btn primary" id="modalPrimary" :disabled="submitting" @click="submitModal">
          <span v-if="submitting" class="btn-spinner" aria-hidden="true"></span>确认
        </button>
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
  background: var(--app-scrim);
  display: none;
  align-items: center;
  justify-content: center;
  z-index: var(--z-modal);
}
.modal-layer.open {
  display: flex;
  /* 开合动效：display 切换时触发（勿改 v-if 结构，保持 legacy 选择器） */
  animation: modal-layer-fade var(--motion-fast) var(--ease-standard);
}
@keyframes modal-layer-fade {
  from { opacity: 0; }
  to { opacity: 1; }
}
.modal {
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-pop);
  width: 100%;
  max-width: 580px;
  max-height: calc(100vh - 80px);
  display: flex;
  flex-direction: column;
  animation: modal-pop var(--motion-base) var(--ease-emphasized);
}
@keyframes modal-pop {
  from {
    opacity: 0;
    transform: scale(0.96) translateY(4px);
  }
  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}
.modal-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--app-border);
}
.modal-head h2 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  color: var(--app-text);
}
.modal-body {
  padding: 16px;
  overflow-y: auto;
  flex: 1;
}
.modal-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 16px;
  border-top: 1px solid var(--app-border);
}
/* 弹窗关闭按钮是文本「×」形态（18px 字形），与全局 28px svg 图标按钮规格不同，
   故保留局部 .icon-btn 定义；hover 用 token 化背景（原 rgba(255,255,255,.06) 硬编码） */
.icon-btn {
  background: transparent;
  border: none;
  color: var(--app-muted);
  cursor: pointer;
  font-size: 18px;
  padding: 4px 8px;
  border-radius: var(--radius-sm);
  transition: background var(--motion-fast), color var(--motion-fast);
}
.icon-btn:hover { background: var(--app-hover); color: var(--app-strong); }

/* Legacy .btn classes — kept so the host-key test's
   ".modal-actions .btn.danger" selector still resolves. main.scss
   defines the global .btn / .btn.primary / .btn.danger rules; these
   scoped rules exist only as a fallback if global CSS is overridden. */
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 32px;
  padding: 0 16px;
  background: var(--app-control);
  color: var(--app-text);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  font-weight: 500;
  cursor: pointer;
  transition: background var(--motion-fast), border-color var(--motion-fast), color var(--motion-fast), box-shadow var(--motion-fast);
}
.btn:hover { background: var(--app-hover); }
.btn.primary {
  background: var(--accent);
  color: var(--accent-on);
  border-color: var(--accent);
}
.btn.primary:hover {
  background: var(--accent-hover);
  border-color: var(--accent-hover);
}
.btn.danger {
  background: var(--danger);
  color: var(--accent-on);
  border-color: var(--danger);
}
.btn.danger:hover {
  background: color-mix(in oklab, var(--danger), black 10%);
  border-color: color-mix(in oklab, var(--danger), black 10%);
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
.muted { color: var(--app-muted); font-size: 12px; }
.context-grid {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px 12px;
  margin: 8px 0;
}
.context-grid dt { color: var(--app-muted); font-size: 12px; }
.callout {
  padding: 10px 12px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid var(--app-border);
  border-radius: 6px;
  font-size: 12px;
}
.callout strong { display: block; margin-bottom: 4px; }

/* 表单内联组：输入框 + 附加按钮（私钥路径浏览 / 凭据清除） */
.inline-field-row {
  display: flex;
  align-items: stretch;
  gap: var(--space-2);
}

.inline-field-row > :first-child {
  flex: 1 1 auto;
  min-width: 0;
}
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

/* 文件删除确认：名称清单 + 不可撤销警示（S2） */
.delete-file-list {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 140px;
  overflow: auto;
}
.delete-file-list li {
  padding: 4px 8px;
  background: var(--app-panel-2);
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  color: var(--app-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.danger-note {
  margin: 0;
  padding: 8px 10px;
  border: 1px solid color-mix(in oklab, var(--danger), transparent 50%);
  border-radius: var(--radius-sm);
  background: color-mix(in oklab, var(--danger), transparent 88%);
  color: var(--danger);
  font-size: var(--text-xs);
  font-weight: 500;
}
.overwrite-path {
  margin: 0;
  padding: 8px 10px;
  background: var(--app-panel-2);
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  word-break: break-all;
  color: var(--app-text);
}

/* 主/副按钮异步提交 spinner（S2：复用 .app-btn-spinner 思路，token 驱动） */
.btn-spinner {
  width: 12px;
  height: 12px;
  border: 2px solid currentColor;
  border-top-color: transparent;
  border-radius: 50%;
  animation: modal-btn-spin 0.7s linear infinite;
  opacity: 0.85;
  flex: 0 0 auto;
}
@keyframes modal-btn-spin {
  to { transform: rotate(360deg); }
}
</style>
