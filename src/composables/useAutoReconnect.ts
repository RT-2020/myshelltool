// 指数退避自动重连：1s → 2s → 5s → 15s，最多 4 次。
//
// **稳定性判据（v2.6 修）**：计数不再「连接一成功就清零」，而是「连接保持存活
// ≥ 一个稳定窗口才清零」。原因：`ssh_connect` 返回成功只证明 TCP+认证通过，
// 不证明会话能维持——认证成功但立刻被关的服务器（nologin、登录 shell 立即退出、
// 建连后 1-3 秒断网）会让「成功→清零→又断→从 1s 重来」无限循环，4 次上限形同
// 虚设，并对端形成每 1-2 秒一次的登录风暴（易触发 fail2ban 锁账号）。
// 现在这类机器会按 1s→2s→5s→15s 走完 4 次后如实报「重连失败，请手动重连」。

/** 默认稳定窗口：连接保持存活超过它才算「真的恢复了」，计数归零。 */
export const DEFAULT_STABILITY_WINDOW_MS = 20000;
const DELAYS = [1000, 2000, 5000, 15000];

/** 单次重连尝试的描述（onAttempt 回调参数）。 */
export interface ReconnectAttemptInfo {
  attempt: number;
  total: number;
  delay: number;
}

export interface UseAutoReconnectOptions {
  onAttempt?: (info: ReconnectAttemptInfo) => void;
  onExhausted?: () => void;
  /** 会话持续存活多久算稳定（毫秒）。测试可注入小值以缩短等待。 */
  stabilityWindowMs?: number;
}

export function useAutoReconnect({
  onAttempt,
  onExhausted,
  stabilityWindowMs = DEFAULT_STABILITY_WINDOW_MS
}: UseAutoReconnectOptions = {}) {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let stabilityTimer: ReturnType<typeof setTimeout> | null = null;
  let attemptCount = 0;
  let cancelled = false;

  function schedule(reconnectFn: (attempt: number) => void) {
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

  /**
   * 连接（或重连）成功后调用：**启动稳定窗口**，窗口内没有再次断开才把计数归零。
   * 窗口内又断了（`schedule` 会清掉窗口）→ 计数保留，退避继续往后走。
   * 返回窗口长度，调用方可据此给用户提示（如「20 秒后确认恢复」）。
   */
  function markConnected(): number {
    if (stabilityTimer) clearTimeout(stabilityTimer);
    stabilityTimer = setTimeout(() => {
      stabilityTimer = null;
      attemptCount = 0;
    }, stabilityWindowMs);
    return stabilityWindowMs;
  }

  function cancel() {
    cancelled = true;
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    // 清掉稳定窗口：断线/取消后残留的窗口会在新连接上误判「已稳定」
    if (stabilityTimer) {
      clearTimeout(stabilityTimer);
      stabilityTimer = null;
    }
  }

  function reset() {
    cancel();
    attemptCount = 0;
  }

  function isScheduled() {
    return timer !== null && !cancelled;
  }

  /** 当前累计的重连尝试次数（测试与状态展示用）。 */
  function getAttemptCount() {
    return attemptCount;
  }

  return { schedule, cancel, reset, isScheduled, markConnected, getAttemptCount };
}
