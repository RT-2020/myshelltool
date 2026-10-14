/**
 * transferConcurrency — 文件传输并发控制与异步任务池。
 *
 * 职责：
 * 1. 管理全局并发传输偏好（localStorage 持久化，范围 1 ~ 5，默认 3）；
 * 2. 提供受限并发调度器 runWithConcurrencyPool（取消感知 + 单项失败不炸批）；
 * 3. 同目标互斥链 serializeOnTarget（并发下「探测→决策→写入」的原子性）。
 */
import { ref } from 'vue';

const STORAGE_KEY = 'myshelltool_transfer_concurrency';
export const DEFAULT_CONCURRENCY = 3;
export const MIN_CONCURRENCY = 1;
export const MAX_CONCURRENCY = 5;

function clampConcurrency(val: unknown): number {
  const num = typeof val === 'number' ? val : parseInt(String(val), 10);
  if (isNaN(num)) return DEFAULT_CONCURRENCY;
  return Math.max(MIN_CONCURRENCY, Math.min(MAX_CONCURRENCY, num));
}

function loadInitialConcurrency(): number {
  if (typeof localStorage === 'undefined') return DEFAULT_CONCURRENCY;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === null) return DEFAULT_CONCURRENCY;
    return clampConcurrency(raw);
  } catch {
    return DEFAULT_CONCURRENCY;
  }
}

/** 响应式并发数，供 UI 双向绑定与实时读取。 */
export const transferConcurrency = ref<number>(loadInitialConcurrency());

/** 获取当前配置的并发数（纯数值）。 */
export function getTransferConcurrency(): number {
  return clampConcurrency(transferConcurrency.value);
}

/** 更新并发数并持久化到 localStorage。 */
export function setTransferConcurrency(val: number): void {
  const clamped = clampConcurrency(val);
  transferConcurrency.value = clamped;
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(STORAGE_KEY, String(clamped));
    }
  } catch {
    /* localStorage 不可用时静默 */
  }
}

/**
 * 通用受限并发调度池（Worker Pool）。
 *
 * 取消：isCancelled 返回 true 后不再派发新任务（已在跑的照常完成——真正的
 * 中止由传输层旗标负责，池只管派发）。
 *
 * 异常：单个 worker 抛错不拖垮整个池——对应结果位留 undefined，其余任务继续
 * 派发执行。worker 失败的「用户可见落地」是调用方的职责（传输域的失败已在
 * run* 执行体内 announce 并标记队列错误态）；池侧仅 console.warn 留线索，
 * 不重复报错也不静默吞掉。
 */
export async function runWithConcurrencyPool<T, R>(
  items: T[],
  concurrencyLimit: number,
  worker: (item: T, index: number) => Promise<R>,
  isCancelled?: () => boolean
): Promise<(R | undefined)[]> {
  const limit = Math.max(1, Math.min(MAX_CONCURRENCY, concurrencyLimit));
  const results: (R | undefined)[] = new Array(items.length);
  let nextIndex = 0;

  async function runner(): Promise<void> {
    while (nextIndex < items.length) {
      if (isCancelled?.()) break;
      const currentIndex = nextIndex++;
      const item = items[currentIndex];
      try {
        results[currentIndex] = await worker(item, currentIndex);
      } catch (err) {
        results[currentIndex] = undefined;
        console.warn('[transferConcurrency] worker failed:', err);
      }
    }
  }

  // 启动不超过 limit 且不超过 items.length 的 worker 实例
  const activeWorkers = Math.min(limit, items.length);
  const workerPromises: Promise<void>[] = [];
  for (let i = 0; i < activeWorkers; i++) {
    workerPromises.push(runner());
  }

  await Promise.all(workerPromises);
  return results;
}

// ============================================================
// 同目标互斥链（B2 并发修复，P0）
// ============================================================

/**
 * 目标锁 key：会话 + 目标全路径。NUL 分隔——文件系统路径不允许含 NUL，
 * 不会与任何真实路径拼接形态碰撞（换行/斜杠都可能是路径字符）。
 */
export function transferTargetLockKey(sessionId: string, target: string): string {
  return sessionId + '\u0000' + target;
}

const targetLocks = new Map<string, Promise<void>>();

/**
 * 按目标路径串行化任务：同一会话内对同一目标（上传=远端全路径，下载=本地
 * 全路径）的「探测→决策→写入」必须互斥。
 *
 * 为什么必须：SFTP create 是 O_CREAT|O_TRUNC——上传一开始就截断目标；下载
 * 侧本地文件同样是 create+truncate 打开。并发批次/跨批次/手动触发若对同一
 * 目标交错写入，内容静默损坏且无任何报错。串行化后，后到者的探测必然看到
 * 先到者的最终落盘状态（半截文件也是「已存在」），从而正确走冲突三选。
 *
 * 语义：task 的错误照常抛给调用方（并发池按单项失败吸收）；Map 里只存吞错
 * 版——链上不允许 unhandled rejection，且前驱失败不阻塞后继排队（后到者
 * 仍应探测一次，由它自己的探测结果决定行为）。不同目标的任务互不影响，
 * 并发池照常并行。
 */
export function serializeOnTarget<T>(target: string, task: () => Promise<T>): Promise<T> {
  const prev = targetLocks.get(target) ?? Promise.resolve();
  const next = prev.then(task);
  // 吞错版（双分支归一为 void，保持 Map 的 Promise<void> 类型）：链上不允许
  // unhandled rejection，前驱失败也不阻塞后继排队
  const stored = next.then(
    () => {},
    () => {}
  );
  targetLocks.set(target, stored);
  stored.finally(() => {
    // 链尾自清理：仅当链尾仍是自己（无后继排队）时删除，防止误删后来者挂上的链
    if (targetLocks.get(target) === stored) targetLocks.delete(target);
  });
  return next;
}
