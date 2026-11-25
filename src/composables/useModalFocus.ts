import { nextTick, onBeforeUnmount, ref, watch, type Ref } from 'vue';

const FOCUSABLE_SELECTOR = [
  'a[href]',
  'area[href]',
  'input:not([disabled]):not([type="hidden"])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  'button:not([disabled])',
  '[tabindex]:not([tabindex="-1"])'
].join(', ');

export interface UseModalFocusOptions {
  /** 浮层打开状态（响应式 getter）。 */
  open: () => boolean;
  /** 对话框主面板容器（须传 ref，Tab 圈闭在此范围内）。 */
  containerRef: Ref<HTMLElement | null>;
  /** 优先聚焦选择器（默认先查 [data-modal-autofocus]，退回首控件或容器）。 */
  autofocusSelector?: string;
  /** 可选 Esc 关闭回调；若宿主要走独立的 useModalDismiss，可不传。 */
  onEscape?: () => void;
  /** 关闭后是否将焦点还原到触发元素（默认 true）。 */
  restoreFocus?: boolean;
  /** 可选切换键（如 modal type 连续切换而不关窗时触发重新聚焦）。 */
  activeKey?: () => unknown;
}

function getFocusableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    (el) => el.getClientRects().length > 0 && getComputedStyle(el).visibility !== 'hidden'
  );
}

/**
 * 浮层焦点管理 composable（00 §3.4 / 10 A2 / 03 A7）：
 * 打开记触发元素并聚焦首目标；Tab/Shift+Tab 圈闭；关闭还焦触发元素。
 */
export function useModalFocus(options: UseModalFocusOptions) {
  const lastFocus = ref<HTMLElement | null>(null);
  let listenerBound = false;

  function focusInitialTarget() {
    const container = options.containerRef.value;
    if (!container) return;
    const selector = options.autofocusSelector || '[data-modal-autofocus]';
    const preferred = container.querySelector<HTMLElement>(selector);
    if (preferred && !preferred.hasAttribute('disabled') && preferred.getClientRects().length > 0) {
      preferred.focus();
      return;
    }
    // 优先聚焦 body 内容区内控件，再退回非标题关闭钮，最后才是关闭钮/容器
    const bodyEl = container.querySelector<HTMLElement>('[data-modal-body]');
    if (bodyEl) {
      const bodyFocusables = getFocusableElements(bodyEl);
      if (bodyFocusables.length > 0) {
        bodyFocusables[0].focus();
        return;
      }
    }
    const focusables = getFocusableElements(container);
    const nonClose = focusables.find((el) => !el.hasAttribute('data-modal-close'));
    if (nonClose) {
      nonClose.focus();
      return;
    }
    if (focusables.length > 0) {
      focusables[0].focus();
      return;
    }
    if (container.getAttribute('tabindex') !== null) {
      container.focus();
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (!options.open()) return;
    if (e.key === 'Escape' && options.onEscape) {
      e.preventDefault();
      options.onEscape();
      return;
    }
    if (e.key === 'Tab' && options.containerRef.value) {
      const container = options.containerRef.value;
      const focusables = getFocusableElements(container);
      if (focusables.length === 0) {
        if (container.getAttribute('tabindex') !== null) {
          e.preventDefault();
          container.focus();
        }
        return;
      }
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      const active = document.activeElement as HTMLElement | null;
      if (e.shiftKey) {
        if (active === first || !active || !container.contains(active)) {
          e.preventDefault();
          last.focus();
        }
      } else if (active === last || !active || !container.contains(active)) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  function bindKeydown() {
    if (listenerBound || typeof document === 'undefined') return;
    document.addEventListener('keydown', onKeydown);
    listenerBound = true;
  }

  function unbindKeydown() {
    if (!listenerBound || typeof document === 'undefined') return;
    document.removeEventListener('keydown', onKeydown);
    listenerBound = false;
  }

  watch(
    options.open,
    (isOpen) => {
      if (isOpen) {
        lastFocus.value = (typeof document !== 'undefined' ? document.activeElement : null) as HTMLElement | null;
        bindKeydown();
        nextTick(() => {
          focusInitialTarget();
        });
      } else {
        unbindKeydown();
        if (options.restoreFocus !== false && lastFocus.value && lastFocus.value.isConnected) {
          lastFocus.value.focus();
        }
        lastFocus.value = null;
      }
    },
    { immediate: true }
  );

  if (options.activeKey) {
    watch(options.activeKey, () => {
      if (!options.open()) return;
      nextTick(() => {
        focusInitialTarget();
      });
    });
  }

  onBeforeUnmount(() => {
    unbindKeydown();
  });

  return { focusInitialTarget };
}
