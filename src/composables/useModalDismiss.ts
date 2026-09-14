/**
 * useModalDismiss — 弹窗的 Esc / 遮罩关闭语义（从 GlobalModals 抽出，拆分第一轮）。
 *
 * 行为契约（S2，v2.3 拆分 Esc 与遮罩点击，迁移时保持逐句一致）：
 * - Esc：hostKeyVerify / mcpApproval → 对应的 deny（安全语义：Esc=拒绝，键盘操作
 *   明确无误触风险，保留 fail-secure）；其余 → closeModal
 * - 遮罩空白点击：hostKeyVerify / mcpApproval → 完全不响应（防误触点空白即拒绝
 *   审批/断开连接，只能通过弹窗内按钮操作）；其余 → closeModal
 *
 * 注册时机：watch 弹窗类型——打开注册、关闭移除，避免全局常驻监听；
 * 组件卸载兜底移除（防 HMR/路由切换泄漏）。
 */
import { onBeforeUnmount, watch, type Ref } from 'vue';
import type { ModalState } from '@/types/domain';

export interface ModalDismissHandlers {
  closeModal: () => void;
  denyHostKey: () => void;
  denyMcpApproval: () => void;
}

export function useModalDismiss(
  modalType: Ref<ModalState['type']>,
  handlers: ModalDismissHandlers
): { onBackdropClick: () => void } {
  function onBackdropClick() {
    const type = modalType.value;
    if (!type) return;
    // 安全审批类弹窗：遮罩点击不产生任何效果（防误操作）
    if (type === 'hostKeyVerify' || type === 'mcpApproval') return;
    handlers.closeModal();
  }

  function dismissByEsc() {
    const type = modalType.value;
    if (!type) return;
    if (type === 'hostKeyVerify') { handlers.denyHostKey(); return; }
    if (type === 'mcpApproval') { handlers.denyMcpApproval(); return; }
    handlers.closeModal();
  }

  function onKeydownEsc(event: KeyboardEvent) {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    dismissByEsc();
  }

  watch(modalType, type => {
    if (type) window.addEventListener('keydown', onKeydownEsc);
    else window.removeEventListener('keydown', onKeydownEsc);
  });
  onBeforeUnmount(() => window.removeEventListener('keydown', onKeydownEsc));

  return { onBackdropClick };
}
