import { invokeBackend } from '@/services/backend';
import { useWorkbenchStore } from '@/stores/workbench';
import { useSyncStore } from '@/stores/sync';

/**
 * useSyncRecoveryPassword — 「应用生成的恢复密码」的读写（v2.7）。
 *
 * 为什么不放在 sync store 里：store 已到 500 行硬上限（AGENTS.md 质量红线），
 * 而这两个动作只服务"首次设置"与"换机恢复"两处界面，属于**局部的凭据交互**，
 * 不是全局同步状态 —— 提到 composable 既让 store 回落，也让"明文只在需要时读一次"
 * 这件事留在用它的组件附近（先例：useGithubDeviceLogin 直接拿 store + invokeBackend）。
 *
 * 明文纪律：生成/读回的值只交给调用方组件短暂持有（填表单 / 显示一次），
 * 不写进 store、不进 localStorage、不进日志。
 */
export function useSyncRecoveryPassword() {
  const store = useWorkbenchStore();
  const syncStore = useSyncStore();

  /**
   * 生成高熵恢复密码并保存到本机安全存储（后端 DPAPI），返回明文。
   * 失败返回 null 并给出可见反馈（不静默）。
   */
  async function generate(): Promise<string | null> {
    try {
      const password = await invokeBackend<string>('sync_generate_recovery_password');
      await syncStore.refreshStatus(); // 让「查看恢复密码」入口立刻出现
      return password;
    } catch (error) {
      store.announce(`生成恢复密码失败：${(error as Error | undefined)?.message || error}`, { level: 'error' });
      return null;
    }
  }

  /**
   * 读回已保存的恢复密码（用户点「查看」时才读一次）。
   * 用户手输的密码从未被保存 → 后端返回 null，调用方需如实说明而不是显示空白。
   */
  async function reveal(): Promise<string | null> {
    try {
      return await invokeBackend<string | null>('sync_reveal_recovery_password');
    } catch (error) {
      store.announce(`读取恢复密码失败：${(error as Error | undefined)?.message || error}`, { level: 'error' });
      return null;
    }
  }

  return { generate, reveal };
}
