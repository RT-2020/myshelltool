/**
 * useModalFormSync — GlobalModals 表单状态 + modal 切换同步（v0.20 自其
 * script 拆出，S2 的一刀）。逻辑原样迁移、行为零变化：
 * - 各表单状态仅在对应 view.type 分支可见（不跨类型残留，语义见原注释）；
 * - watch(modal.type) 做预填/清零（assetEditor 预填、renameGroup 取末段、
 *   分组快捷新增 presetGroup、editorDialog 输入预填等）。
 *
 * ctx 注入（组件组装一次）：modal（Ref）、keyboardPrompt（Ref）、
 * closeTerminalSearchInline（store action）。EditorDialogViewPayload 类型
 * 随迁导出（GlobalModals 与 modalSubmit 均消费）。
 */
import { computed, reactive, ref, watch, type Ref } from 'vue';
import type { ModalState } from '@/types/domain';
import { cloneAsset, emptyAsset, emptyCredential, emptyTunnelForm } from '@/lib/modalForms';

/** 编辑器通用弹窗载荷（v0.18）：标题/正文/可选路径输入 + 动态按钮。 */
export interface EditorDialogViewPayload {
  title?: string;
  message?: string;
  detail?: string;
  input?: { label: string; placeholder?: string; value?: string };
  buttons?: { label: string; danger?: boolean; primary?: boolean }[];
}

/** modal 的运行时附加字段（组件侧弱约束透传口径，与 GlobalModals 的 ModalExtras 同形；
 *  asset 覆写为宽类型（原样迁移时的防御性收窄，不改变运行时）。 */
export interface ModalExtras extends Omit<ModalState, 'asset'> {
  path?: string;
  entry?: { path?: string; name?: string } | null;
  payload?: Record<string, unknown> | null;
  asset?: unknown;
}

export interface ModalFormSyncCtx {
  modal: Ref<ModalState | { type: string | null; asset: unknown; path?: string; payload?: unknown }>;
  keyboardPrompt: Ref<unknown>;
  closeTerminalSearchInline(): void;
}

export function useModalFormSync(ctx: ModalFormSyncCtx) {
  const modal = ctx.modal as Ref<ModalExtras>;

  // ============================================================
  // Local form state（原 GlobalModals:127-151 原样迁移）
  // ============================================================
  const editingAsset = reactive(emptyAsset());
  const editingCredential = reactive(emptyCredential());
  const tunnelForm = reactive(emptyTunnelForm());
  const fileForm = reactive({ mkdirName: '' });
  const renameTarget = reactive({ path: '', current: '', next: '' });
  const keyboardResponses = reactive<Record<string, string>>({});
  // 分组三表单共享输入对象（rename=改名/create=新建/move=移动，submitModal 分发读值）
  const groupInputs = reactive({ rename: '', create: '', move: '' });
  // assetEditor 内联校验错误（替代 window.alert）
  const assetFormError = ref('');
  // 分组表单内联校验错误（renameGroup / createGroup / moveAsset 共用，替代静默 return）
  const groupFormError = ref('');
  // reauthPassword：连接失败快捷重认证表单（TerminalSurface 错误卡片入口）
  const reauthForm = reactive({ password: '', error: '' });
  // editorDialog：路径输入预填 + 内联错误
  const editorDialogInput = ref('');
  const editorDialogError = ref('');

  // 事实备注：KeyboardInteractivePayload 权威类型（domain.ts）字段为 instruction（单数），
  // 此处历史代码访问 instructions（复数）——与后端契约不一致、运行时恒 undefined 不显示。
  // 按「行为零变化」保留原属性访问路径，仅类型层断言放宽。
  const keyboardInstructions = computed(() =>
    (ctx.keyboardPrompt.value as { instructions?: string | null } | null)?.instructions
  );

  // ============================================================
  // Sync form state when modal type changes（原 GlobalModals:177-234 原样迁移）
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
      Object.assign(editingAsset, modal.value.asset ? cloneAsset(modal.value.asset as never) : emptyAsset());
      Object.assign(editingCredential, emptyCredential());
      assetFormError.value = '';
      // 分组头「+」快捷新增：预填目标分组（仅新建态生效，编辑态以资产自身分组为准）
      if (!modal.value.asset) {
        const presetGroup = (modal.value.payload as { presetGroup?: unknown } | undefined)?.presetGroup;
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
      groupInputs.move = (modal.value.asset as { group?: string } | null)?.group || '未分组';
    }
    if (type === 'tunnelCreate') {
      Object.assign(tunnelForm, emptyTunnelForm());
    }
    if (type === 'mkdir' || type === 'localMkdir') {
      fileForm.mkdirName = '';
    }
    if (type === 'rename' || type === 'localRename') {
      const entry = (modal.value as { entry?: { path?: string; name?: string } | null }).entry;
      Object.assign(renameTarget, {
        path: entry?.path || '',
        current: entry?.name || '',
        next: entry?.name || ''
      });
    }
    if (type === 'terminalSearch') {
      ctx.closeTerminalSearchInline();
    }
    if (type === 'keyboardInteractive') {
      Object.keys(keyboardResponses).forEach(key => delete keyboardResponses[key]);
    }
    if (type === 'confirmCloseAssetWindow') {
      // 纯确认弹窗（无表单状态）：count/onConfirm 直接读 modal payload
    }
  });

  return {
    editingAsset,
    editingCredential,
    tunnelForm,
    fileForm,
    renameTarget,
    keyboardResponses,
    groupInputs,
    assetFormError,
    groupFormError,
    reauthForm,
    editorDialogInput,
    editorDialogError,
    keyboardInstructions
  };
}
