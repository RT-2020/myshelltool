/**
 * assetWindowClose — 独立资产窗口的关窗流程（v0.20 自 AssetWindowShell 拆出，
 * S2 的一刀；逻辑原样迁移、行为零变化）。
 *
 * 流程（顺序即安全层级）：
 * 1. 在途/排队传输守卫（最前）：销毁窗口 = 销毁 JS 上下文，Rust 侧流式上传的
 *    取消旗标将无人能触发，远端可能留「半截文件」——直接阻断关窗；
 * 2. v0.18 编辑器 dirty 守卫：三选（取消/放弃更改/全部保存并继续），本轮流程
 *    内 editorDirtyHandled 放行不重复弹；
 * 3. 活跃会话确认：confirmCloseAssetWindow 弹窗 → 确认后先等 connecting 会话
 *    settle（pending 占位 sessionId 断不开会留 Rust 侧孤儿连接；250ms 轮询 +
 *    65s deadline 与 hostkey 等待契约对齐），再断开全部 → destroy 销毁窗口
 *    （destroy 绕过 close-requested 防确认递归）。
 */
import { getTauriWindow } from '@/services/backend';
import type { ModalState } from '@/types/domain';

/** 本流程消费的窗口可选能力（tauri.d.ts 未声明，局部收窄——与组件原定义同形）。 */
interface CloseableTauriWindow {
  onCloseRequested?: (handler: (event: { preventDefault(): void }) => void | Promise<void>) => Promise<() => void>;
  destroy?: () => Promise<void>;
}

/** 流程所需的最小 store 面（组件注入，保持模块 store-agnostic）。 */
export interface AssetWindowCloseStore {
  announce?(message: string, opts?: { level?: string }): unknown;
  sessions?: { sessionId: string; status: string }[];
  disconnectSession(sessionId: string): unknown | Promise<unknown>;
  editorDirtyCount: number;
  modal: ModalState | { type: string | null; asset: unknown };
}

export interface AssetWindowCloseEditorStore {
  openDialog(payload: {
    title?: string;
    message?: string;
    detail?: string;
    buttons?: { label: string; danger?: boolean; primary?: boolean; action?: () => void }[];
  }): unknown;
  saveAllEditorTabs(): Promise<boolean>;
}

export interface AssetWindowCloseCtx {
  store: AssetWindowCloseStore;
  editorStore: AssetWindowCloseEditorStore;
  activeTransferCount(): number;
}

export function createAssetWindowClose(ctx: AssetWindowCloseCtx) {
  const { store, editorStore } = ctx;
  /** 编辑器 dirty 守卫已放行（本轮关窗流程内不重复弹）。 */
  let editorDirtyHandled = false;

  async function requestCloseAssetWindow() {
    // 传输中守卫（置于最前，早于会话统计与确认弹窗）：销毁窗口会连 JS 上下文
    // 一起销毁，但 Rust 侧的流式上传（sftp_upload_from_file）仍在跑——取消旗标
    // 存在本窗口的 Pinia 上下文里，窗口没了就没有人再能取消它，远端可能留下
    // 「半截文件」（不是没传、也不是传完，而是损坏的不完整文件）。
    // 因此有在途/排队传输时直接阻断关窗，不进入会话确认流程、不销毁窗口。
    // （不做主动取消/断开：编排不在关窗流程范围内，交由用户在传输列表处理。）
    if (ctx.activeTransferCount() > 0) {
      store.announce?.(
        `有 ${ctx.activeTransferCount()} 个传输任务正在进行，请等待完成或先在传输列表取消后再关闭窗口`,
        { level: 'warn' }
      );
      return;
    }
    // v0.18 编辑器 dirty 守卫（在途传输之后、会话确认之前）：三选后经
    // editorDirtyHandled 放行，不再重复弹。
    if (!editorDirtyHandled && store.editorDirtyCount > 0) {
      openEditorCloseDialog();
      return;
    }
    const active = (store.sessions || []).filter(
      s => s.status === 'connected' || s.status === 'connecting'
    );
    if (!active.length) {
      await destroyWindow();
      return;
    }
    store.modal = {
      type: 'confirmCloseAssetWindow',
      count: active.length,
      onConfirm: () => closeAfterConfirm()
    } as ModalState;
  }

  function openEditorCloseDialog() {
    editorStore.openDialog({
      title: '未保存的修改',
      message: `编辑器中有 ${store.editorDirtyCount} 个文件未保存。`,
      detail: '全部保存将逐个写回原位置（任一失败会中止关闭）；放弃更改不保存直接进入后续关闭流程。',
      buttons: [
        { label: '取消' },
        {
          label: '放弃更改并继续',
          danger: true,
          action: () => {
            editorDirtyHandled = true;
            void requestCloseAssetWindow();
          }
        },
        {
          label: '全部保存并继续',
          primary: true,
          action: () => {
            void editorStore.saveAllEditorTabs().then(ok => {
              if (!ok) {
                store.announce?.('有文件未能保存（冲突或校验未通过），关闭已中止', { level: 'warn' });
                return;
              }
              editorDirtyHandled = true;
              void requestCloseAssetWindow();
            });
          }
        }
      ]
    });
  }

  // 确认关闭：connecting 会话的 sessionId 还是 pending 占位（断不开，会留
  // Rust 侧孤儿连接），先轮询等 settle（250ms 间隔）。等待上限 65s 与 sessions
  // store 的 hostkey 等待契约对齐（后端 60s 超时 + 5s 缓冲）；超时后【不销毁
  // 窗口】——此时 destroy 会销毁 JS 上下文、把孤儿 SSH 会话留在后端，改为
  // 提示用户稍候重试或先在终端取消连接，由用户决定。settle 后照常走断开+销毁。
  const CONNECT_SETTLE_TIMEOUT_MS = 65000;
  async function closeAfterConfirm() {
    const deadline = Date.now() + CONNECT_SETTLE_TIMEOUT_MS;
    while (
      (store.sessions || []).some(s => s.status === 'connecting')
      && Date.now() < deadline
    ) {
      await new Promise(resolve => setTimeout(resolve, 250)); // fact-guard:allow no-fixed-wait-frontend settle 轮询间隔：while 头每轮实测 sessions 状态，65s deadline 封顶，非就绪信号
    }
    if ((store.sessions || []).some(s => s.status === 'connecting')) {
      store.announce?.('连接仍在进行中，请稍候重试，或先在终端取消连接', { level: 'warn' });
      return;
    }
    const targets = (store.sessions || []).filter(
      s => s.status === 'connected' || s.status === 'connecting'
    );
    await Promise.allSettled(targets.map(s => store.disconnectSession(s.sessionId)));
    await destroyWindow();
  }

  // destroy 绕过 close-requested（防确认弹窗递归）；失败仅记录，不静默吞
  async function destroyWindow() {
    try {
      await (getTauriWindow() as CloseableTauriWindow | null)?.destroy?.();
    } catch (error) {
      console.error('[assetWindowClose] destroy window failed:', error);
    }
  }

  return { requestCloseAssetWindow, destroyWindow };
}
