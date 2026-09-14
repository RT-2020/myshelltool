/**
 * modalSubmit — GlobalModals 主按钮（确认）的按弹窗类型分发（从 GlobalModals.vue
 * 抽出，拆分第二刀）。
 *
 * 结构：组件持有全部表单状态与提交流程编排（footer 共享按钮 → submitModal），
 * 本文件只做**纯分发**——按 context 传入的状态与回调执行各类型分支，
 * 行为与迁移前逐句一致（含三条「不走 closeModal 的直关」路径与一条
 * 历史 TypeError 兼容分支，见各 case 注释）。
 *
 * store 以结构化类型约束（先例：files store 的 FilesWorkbenchBridge），
 * 只声明分支实际用到的成员，避免 lib 反向 import Pinia store。
 */
import type { Ref } from 'vue';
import type {
  AssetEditorForm,
  ConnectionAssetInput,
  CredentialEditorForm,
  ModalState,
  RemoteFileEntry,
  TunnelEditorForm
} from '@/types/domain';

/** 弹窗按需挂载载荷（与 GlobalModals 的 ModalExtras 同形；弱约束透传字段）。 */
export interface ModalExtrasLike {
  type: ModalState['type'];
  asset?: ModalState['asset'];
  path?: string;
  payload?: ModalState['payload'];
  onConfirm?: (() => void) | null;
}

/** saveAsset 的凭据入参（与 store 契约一致：输入或清除标记，二选一互斥）。 */
export interface CredentialSubmitInput {
  password: string;
  passphrase: string;
  privateKey: string;
  clearPassword: boolean;
  clearPassphrase: boolean;
  clearPrivateKey: boolean;
}

export interface ModalSubmitStore {
  saveAsset(input: ConnectionAssetInput, credential: CredentialSubmitInput | { password: string; clearPassphrase: boolean }): Promise<unknown>;
  deleteAsset(id: string): Promise<unknown>;
  renameGroup(oldPath: string, newPath: string): Promise<unknown>;
  createGroup(path: string): Promise<unknown>;
  moveAsset(id: string, target: string): Promise<unknown>;
  createTunnel(form: {
    name: string | number;
    kind: string | number;
    local_addr: string;
    local_port: number;
    remote_addr: string;
    remote_port: number;
    auto_start: boolean;
  }): Promise<unknown>;
  mkdirRemote(name: string): Promise<unknown>;
  localMkdir(name: string): Promise<unknown>;
  renameRemote(entry: RemoteFileEntry, next: string): Promise<unknown>;
  localRename(path: string, next: string): Promise<unknown>;
  resolveHostKeyPrompt(requestId: string, accepted: boolean): void;
  resolveMcpApproval(requestId: string, accepted: boolean): void;
  resolveKeyboardPrompt(requestId: string, answers: string[]): void;
  reconnectSession(sessionId: string): Promise<unknown>;
  closeTerminalSearchInline(): void;
  /** 绕过 closeModal 的 deny 路由直关弹窗（accept/确认后的三处直关共用）。 */
  closeDirect(): void;
  announce(message: string, opts: { level: string }): void;
  assets: readonly { id: string }[] | undefined;
  terminalSearch: { query: string };
}

export interface ModalSubmitContext {
  store: ModalSubmitStore;
  modal: ModalExtrasLike;
  editingAsset: AssetEditorForm;
  editingCredential: CredentialEditorForm;
  tunnelForm: TunnelEditorForm;
  /** 分组三表单（rename/create/move）的输入值 */
  groupInputs: { rename: string; create: string; move: string };
  /** 文件四表单共享的目录名输入 */
  fileForm: { mkdirName: string };
  renameTarget: { path: string; current: string; next: string };
  reauthForm: { password: string; error: string };
  keyboardResponses: Record<string, string>;
  assetFormError: Ref<string>;
  groupFormError: Ref<string>;
  hostKeyPrompt: Ref<{ request_id: string } | null>;
  mcpApprovalPrompt: Ref<{ request_id: string } | null>;
  keyboardPrompt: Ref<{ request_id: string } | null>;
  closeModal: () => void;
  submitting: Ref<boolean>;
}

/**
 * 执行异步提交：期间 submitting=true（按钮禁用 + spinner，防重复提交）。
 * 成功返回 true；失败返回 false（错误 announce，弹窗保持打开可重试）。
 */
export async function runModalSubmit(
  ctx: ModalSubmitContext,
  task: () => Promise<unknown>
): Promise<boolean> {
  if (ctx.submitting.value) return false;
  ctx.submitting.value = true;
  try {
    await task();
    return true;
  } catch (error) {
    // 提交失败必须可见（部分 store action 不自带失败 announce），弹窗保持打开可重试
    const message = (error instanceof Error ? error.message : String(error)) || '未知错误';
    ctx.store.announce('操作失败：' + message, { level: 'error' });
    return false;
  } finally {
    ctx.submitting.value = false;
  }
}

function splitTags(tags: string | string[]): string[] {
  return Array.isArray(tags) ? tags : String(tags || '').split(/[·,，\s]+/).filter(Boolean);
}

/** 主按钮分发（GlobalModals.submitModal 的 switch 逐句迁移）。 */
export async function dispatchModalSubmit(ctx: ModalSubmitContext, type: ModalState['type']): Promise<void> {
  const { store, modal } = ctx;
  const run = (task: () => Promise<unknown>) => runModalSubmit(ctx, task);
  switch (type) {
    case 'assetEditor':
      if (ctx.editingAsset.auth_method === 'Password' && !ctx.editingAsset.credential_id && !ctx.editingCredential.password && !ctx.editingCredential.clearPassword) {
        ctx.assetFormError.value = '首次保存 Password 认证需填写密码；若要用私钥登录，请将认证方式切换为 PrivateKey。';
        return;
      }
      ctx.assetFormError.value = '';
      await run(() =>
        store.saveAsset(
          // 表单态（port/auth_method 可能被输入控件回传 string）按 saveAsset 入参契约断言透传，与迁移前运行时行为一致
          {
            ...ctx.editingAsset,
            // 未选分组统一归「未分组」（normalizeAsset 亦兜底，此处显式表达产品语义）
            group: String(ctx.editingAsset.group || '').trim() || '未分组',
            private_key_path: String(ctx.editingAsset.private_key_path || '').trim() || null,
            tags: splitTags(ctx.editingAsset.tags)
          } as ConnectionAssetInput,
          {
            password: ctx.editingCredential.password,
            passphrase: ctx.editingCredential.passphrase,
            privateKey: ctx.editingCredential.privateKey,
            clearPassword: ctx.editingCredential.clearPassword,
            clearPassphrase: ctx.editingCredential.clearPassphrase,
            clearPrivateKey: ctx.editingCredential.clearPrivateKey
          }
        )
      );
      return;
    case 'confirmDelete':
      // confirmDelete 弹窗打开时必有 asset（payload 契约），断言仅类型层收窄
      await run(() => store.deleteAsset(modal.asset?.id as string));
      return;
    case 'renameGroup': {
      const newName = ctx.groupInputs.rename.trim();
      if (!newName) {
        ctx.groupFormError.value = '请输入新名称';
        return;
      }
      if (newName.includes('/')) {
        ctx.groupFormError.value = '名称不能包含 /';
        return;
      }
      const oldPath = modal.path || '';
      const parent = oldPath.includes('/') ? oldPath.slice(0, oldPath.lastIndexOf('/')) : '';
      const newPath = parent ? `${parent}/${newName}` : newName;
      await run(() => store.renameGroup(oldPath, newPath));
      return;
    }
    case 'createGroup': {
      const path = ctx.groupInputs.create.trim();
      if (!path) {
        ctx.groupFormError.value = '请输入分组路径';
        return;
      }
      await run(() => store.createGroup(path));
      return;
    }
    case 'moveAsset': {
      const target = ctx.groupInputs.move.trim() || '未分组';
      if (!target.trim()) {
        ctx.groupFormError.value = '请输入目标分组';
        return;
      }
      // moveAsset 弹窗打开时必有 asset（payload 契约），断言仅类型层收窄
      await run(() => store.moveAsset(modal.asset?.id as string, target));
      return;
    }
    case 'tokenConfig':
      // v1.8：PAT 表单已抽到 PatConfigCard（自带保存按钮，自包含）。
      // 主按钮「确认」仅关闭弹窗，保存动作在卡片内完成。
      ctx.closeModal();
      return;
    case 'tunnelCreate':
      await run(() =>
        store.createTunnel({
          ...ctx.tunnelForm,
          local_port: Number(ctx.tunnelForm.local_port),
          remote_port: Number(ctx.tunnelForm.remote_port)
        })
      );
      return;
    case 'mkdir':
      if (await run(() => store.mkdirRemote(ctx.fileForm.mkdirName))) ctx.closeModal();
      return;
    case 'localMkdir':
      if (await run(() => store.localMkdir(ctx.fileForm.mkdirName))) ctx.closeModal();
      return;
    case 'rename':
      if (await run(() => store.renameRemote({ path: ctx.renameTarget.path, name: ctx.renameTarget.current } as RemoteFileEntry, ctx.renameTarget.next))) ctx.closeModal();
      return;
    case 'localRename':
      if (await run(() => store.localRename(ctx.renameTarget.path, ctx.renameTarget.next))) ctx.closeModal();
      return;
    case 'hostKeyVerify':
      store.resolveHostKeyPrompt(ctx.hostKeyPrompt.value!.request_id, true);
      // 不走 closeModal()：它对 hostKeyVerify 路由到 denyHostKey()，会在 accept
      // 之后又补发一次 accepted:false（denyHostKey → closeModal 还会递归）
      store.closeDirect();
      return;
    case 'confirmCloseAssetWindow':
      // onConfirm 自管理断开与窗口 destroy，不 await——立即关 modal 让流程接管
      modal.onConfirm?.();
      store.closeDirect();
      return;
    case 'mcpApproval':
      store.resolveMcpApproval(ctx.mcpApprovalPrompt.value!.request_id, true);
      // 同 hostKeyVerify：closeModal 会路由 denyMcpApproval 补发 accepted:false
      store.closeDirect();
      return;
    case 'keyboardInteractive':
      store.resolveKeyboardPrompt(ctx.keyboardPrompt.value!.request_id, Object.values(ctx.keyboardResponses));
      ctx.closeModal();
      return;
    case 'terminalSearch': {
      // 事实备注：workbench store 未导出 executeTerminalSearch（迁移前即如此，运行时
      // 该调用抛 TypeError 未捕获；该分支为兼容入口，正常运行流程不会进入）。按
      // 「行为零变化」保留原调用语义（无条件调用，非可选链）。
      const sessionId = modal.payload?.sessionId as string | undefined;
      if (sessionId) {
        (store as unknown as { executeTerminalSearch: (sessionId: string, query: string) => void })
          .executeTerminalSearch(sessionId, store.terminalSearch.query);
      }
      return;
    }
    case 'reauthPassword': {
      if (!ctx.reauthForm.password) {
        ctx.reauthForm.error = '请输入密码';
        return;
      }
      ctx.reauthForm.error = '';
      await run(async () => {
        // 从 store 取最新资产（modal 里的 asset 是打开弹窗时的快照，可能已被编辑过）
        const current = (store.assets || []).find(a => a && a.id === modal.asset?.id) || modal.asset;
        // saveAsset 成功后会关闭 modal，先取 sessionId 再调用；
        // 切到密码认证时顺带清掉残留的 passphrase 凭据（PrivateKey 专属，避免换回时旧值干扰）
        const sessionId = modal.payload?.sessionId as string | undefined;
        await store.saveAsset(
          { ...current, auth_method: 'Password' },
          { password: ctx.reauthForm.password, clearPassphrase: true }
        );
        if (sessionId) await store.reconnectSession(sessionId);
      });
      return;
    }
    default:
      ctx.closeModal();
  }
}
