/**
 * toastQueue — toast 通知队列内核（v0.20 自 ui store 拆出，S2 的一刀）。
 *
 * 语义保持不变（从 ui.ts 原样迁移）：
 * - notify 一律写状态栏一行字；仅带 level 的进 toast 队列（announce 不带 level）；
 * - 分级时长：info/success 3.5s、warn/error 6s、带 action 8s（opts.duration 覆盖）；
 * - 上限 5 条，超出丢最早并清其 timer。
 *
 * 绑定式 ctx（先例 autoSyncQueue/editorFlows）：statusMessage 由 ui store 注入
 * （状态栏一行字仍归 ui store 所有），store 只持 toasts/notify/dismissToast 的
 * 同名转发——AppToastHost/workbench re-export 零改动。
 */
import { ref, type Ref } from 'vue';
import type { NotifyOptions, ToastItem } from '@/types/domain';

const TOAST_LEVELS: readonly string[] = ['info', 'success', 'warn', 'error'];
const MAX_TOASTS = 5;

export function createToastQueue(statusMessage: Ref<string>) {
  const toasts = ref<ToastItem[]>([]);
  let toastSeq = 0;
  const toastTimers = new Map<number, ReturnType<typeof setTimeout>>();

  function clearToastTimer(id: number) {
    const timer = toastTimers.get(id);
    if (timer !== undefined) {
      clearTimeout(timer);
      toastTimers.delete(id);
    }
  }

  function notify(message: string, opts: NotifyOptions = {}): number | null {
    // 状态栏保持原有行为：任何 notify 都写入底部一行文字
    statusMessage.value = message;
    // 仅带 level 的通知进 toast 队列（announce 不带 level，保持旧行为）
    if (!opts.level || !TOAST_LEVELS.includes(opts.level)) return null;
    const id = ++toastSeq;
    const toast: ToastItem = {
      id,
      level: opts.level,
      message,
      action: opts.action ?? null,
      actions: opts.actions
    };
    toasts.value.push(toast);
    // 上限 5 条，超出移除最早一条并清理其 timer
    if (toasts.value.length > MAX_TOASTS) {
      clearToastTimer(toasts.value[0].id);
      toasts.value.shift();
    }
    scheduleToastDismiss(id, opts);
    return id;
  }

  function scheduleToastDismiss(id: number, opts: NotifyOptions) {
    // 默认时长：info/success 3500ms、warn/error 6000ms、带 action（含 actions 多按钮）8000ms；
    // opts.duration 可覆盖（下载完成 toast 用 10s 给足点击窗口）。带 action 也可超时消失（保持简单）。
    let duration: number | undefined = opts.duration;
    if (typeof duration !== 'number') {
      const hasAction = Boolean(opts.action) || Boolean(opts.actions && opts.actions.length);
      duration = hasAction
        ? 8000
        : (opts.level === 'warn' || opts.level === 'error' ? 6000 : 3500);
    }
    clearToastTimer(id);
    toastTimers.set(id, setTimeout(() => dismissToast(id), duration));
  }

  function dismissToast(id: number) {
    clearToastTimer(id);
    const idx = toasts.value.findIndex(t => t.id === id);
    if (idx !== -1) toasts.value.splice(idx, 1);
  }

  return { toasts, notify, dismissToast };
}
