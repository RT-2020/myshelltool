/**
 * useModalDismiss — 弹窗的 Esc 关闭语义（从 GlobalModals 抽出，拆分第一轮）。
 *
 * 行为契约（S2，v2.3 拆分；v0.22 修订遮罩语义）：
 * - Esc：hostKeyVerify / mcpApproval → 对应的 deny（安全语义：Esc=拒绝，键盘操作
 *   明确无误触风险，保留 fail-secure）；其余 → closeModal
 * - 遮罩空白点击：一律不关闭（v0.22 起全类型禁用——表单型弹窗误触遮罩会丢已填
 *   内容；关闭只走右上角 × / 确认 / 取消按钮，Esc 保留）
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
): void {
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
}
