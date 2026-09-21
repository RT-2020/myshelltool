/**
 * autoSyncQueue — 自动同步串行推送队列（从 stores/sync.ts 按域拆出）。
 *
 * 串行化的理由（并发 = 静默丢远端保存）：并发 push 会让后端各自 load_sync_state
 * 算出**同一个 new_rev**（后端 sync 无 CAS，Gist PATCH 也无），晚到的旧载荷覆盖
 * 新载荷 → 远端少一次保存，而本地 local_rev/last_synced_at 已记为「已同步」，
 * 换机器也看不出冲突。因此自动 push 绝不并发：在途时只置 pending 标记，当前
 * push 结束后重放一次（合并多次资产写为一次 push）。
 *
 * 绑定式 context（先例 lib/terminalLifecycle）：sync store setup 时 bind 一次；
 * lib 不反向 import Pinia。
 */
import { invokeBackend } from '@/services/backend';
import { errorMessage } from '@/lib/errorMessage';
import type { NotifyOptions } from '@/types/domain';

export interface AutoSyncQueueContext {
  /** push 成功后刷新 sync_status（store action 透传）。 */
  refreshStatus(): Promise<void>;
  /** 清除「远端有更新」徽章（store ref 写入透传）。 */
  clearRemoteUpdates(): void;
  /** 结果播报（store 的 workbench bridge 透传；桥未注入时 undefined 即不播）。 */
  announce?(message: string, opts?: NotifyOptions): unknown;
}

let bound: AutoSyncQueueContext | null = null;

export function bindAutoSyncQueueContext(ctx: AutoSyncQueueContext): void {
  bound = ctx;
}

function qc(): AutoSyncQueueContext {
  if (!bound) throw new Error('autoSyncQueue 未绑定 context（sync store 未初始化）');
  return bound;
}

/** 在途标记 + 重放标记（模块级单例——自动推送队列全局只有一条，与 sync store 同生命周期）。 */
let autoPushInFlight = false;
let autoPushPending = false;

/**
 * 入队一次自动推送。调用方（sync store 的 autoPushIfEnabled）负责触发前置守卫
 * （autoSyncEnabled / 未解决冲突 / Tauri runtime），本函数只管串行化执行与结果播报：
 * 成功 toast 只在队列排空后弹一条；全失败则不弹成功（失败逐条 announce error，
 * 不打断用户——典型：冲突，让用户手动处理）。
 */
export function enqueueAutoPush() {
  if (autoPushInFlight) {
    // 合并：在途 push 结束后重放一次，不并发、也不静默丢弃
    autoPushPending = true;
    return;
  }
  autoPushInFlight = true;
  void (async () => {
    try {
      let rerun = true;
      // 合并多次资产写为一次 push
      let anySuccess = false;
      while (rerun) {
        rerun = false;
        try {
          await invokeBackend('sync_push', { masterPassword: '' });
          anySuccess = true;
          await qc().refreshStatus();
          qc().clearRemoteUpdates();
        } catch (error) {
          qc().announce?.('自动同步失败：' + errorMessage(error) + '（请到同步面板处理）', { level: 'error' });
        }
        if (autoPushPending) { autoPushPending = false; rerun = true; }
      }
      if (anySuccess) {
        qc().announce?.('✓ 资产已自动同步到云端', { level: 'success' });
      }
    } finally {
      autoPushInFlight = false;
    }
  })();
}
