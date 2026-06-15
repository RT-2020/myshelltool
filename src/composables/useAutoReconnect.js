// 指数退避自动重连：1s → 2s → 5s → 15s，最多 4 次。
// 网络恢复后第一次成功即重置计数；4 次失败后通知 onExhausted 提示手动重连。

const DELAYS = [1000, 2000, 5000, 15000];

export function useAutoReconnect({ onAttempt, onExhausted } = {}) {
  let timer = null;
  let attemptCount = 0;
  let cancelled = false;

  function schedule(reconnectFn) {
    cancel();
    cancelled = false;
    if (attemptCount >= DELAYS.length) {
      attemptCount = 0;
      onExhausted?.();
      return;
    }
    const delay = DELAYS[attemptCount];
    attemptCount += 1;
    onAttempt?.({ attempt: attemptCount, total: DELAYS.length, delay });
    timer = setTimeout(() => {
      if (cancelled) return;
      reconnectFn(attemptCount);
    }, delay);
  }

  function cancel() {
    cancelled = true;
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function reset() {
    cancel();
    attemptCount = 0;
  }

  function isScheduled() {
    return timer !== null && !cancelled;
  }

  return { schedule, cancel, reset, isScheduled };
}
