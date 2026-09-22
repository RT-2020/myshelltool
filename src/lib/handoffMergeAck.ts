/**
 * handoffMergeAck — MERGE_ACK 等待机制（v0.20 自 sessionHandoff 拆出，S2 的
 * 一刀；逻辑原样迁移）：asset 窗口 push 后挂起等主窗口回执，超时兜底 +
 * 宽限轮询（late-ack 自我纠正）。pushSessionToMainWindow 与事件装配
 * （settleMergeAck）经值 import 消费。
 */
interface MergeAckResult {
  ok: boolean;
  reason?: string;
}

interface PendingAckEntry {
  resolve: (result: MergeAckResult) => void;
  timer: ReturnType<typeof setTimeout> | null;
}

const pendingAcks = new Map<string, PendingAckEntry>(); // sessionId -> { resolve, timer }

/**
 * 主窗口接管回执的等待上限（v2.6：6s → 20s，另加 10s 宽限轮询）。
 *
 * 旧值 6000ms 把「ACK 还没来」当成「主窗口没接管」：主窗口最小化时
 * `adoptSession` 里的 requestAnimationFrame 被节流、首次 adopt 还要冷加载 xterm 与
 * 分帧回放，很容易超过 6s → 弹「移回失败」但主窗口稍后**仍然接管成功**，变成两个窗口
 * 同时监听同一会话（用户之后关 asset 窗口还会把主窗口正在用的连接断掉）。
 * 20s + 宽限轮询覆盖冷启动与节流场景；已知残留：ACK 在宽限期之后才到时本窗口不会
 * 自动移交（超时文案已改为「尚未确认」并说明可从主窗口继续使用，不再谎报失败）。
 */
const MERGE_ACK_TIMEOUT_MS = 20000;

export async function waitMergeAck(sessionId: string, timeoutMs = MERGE_ACK_TIMEOUT_MS): Promise<MergeAckResult> {
  return new Promise(resolve => {
    const entry: PendingAckEntry = { resolve, timer: null };
    entry.timer = setTimeout(() => {
      pendingAcks.delete(sessionId);
      resolve({ ok: false, reason: 'timeout' });
    }, timeoutMs);
    pendingAcks.set(sessionId, entry);
  });
}

export function settleMergeAck(sessionId: string, result: MergeAckResult): boolean {
  const entry = pendingAcks.get(sessionId);
  if (!entry) return false;
  pendingAcks.delete(sessionId);
  clearTimeout(entry.timer!);
  entry.resolve(result);
  return true;
}

/** 是否仍在等这条会话的 MERGE_ACK（未超时/未被 settle）。 */
export function isMergeAckPending(sessionId: string): boolean {
  return pendingAcks.has(sessionId);
}

/**
 * 等待 ACK 的 Promise **或** 一个宽限轮询（谁先有结果用谁）。
 *
 * 为什么需要轮询：ACK 到达时 `settleMergeAck` 会 resolve 原 Promise；但如果上游因
 * 别的原因提前放弃了那个 Promise（旧实现的 6s 超时即如此），late ACK 就没有人接。
 * 宽限期版本让「主窗口慢但在推进」的场景能自我纠正。
 */
export function waitMergeAckWithGrace(sessionId: string, primary: Promise<MergeAckResult>, graceMs = 10000): Promise<MergeAckResult> {
  return new Promise(resolve => {
    let done = false;
    const finish = (result: MergeAckResult) => {
      if (done) return;
      done = true;
      clearInterval(poll);
      resolve(result);
    };
    const poll = setInterval(() => {
      // ACK 在宽限期内到达 → 主窗口确实接管了；原 Promise 可能已被超时弃用
      if (!pendingAcks.has(sessionId)) finish({ ok: true, reason: 'late-ack' });
    }, 250);
    void primary.then(finish);
    setTimeout(() => finish({ ok: false, reason: 'timeout' }), graceMs);
  });
}
