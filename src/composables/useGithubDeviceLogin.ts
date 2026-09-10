import { onUnmounted, ref } from 'vue';
import { invokeBackend, isTauriRuntime } from '@/services/backend';
import { useAssetsStore } from '@/stores/assets';
import { useSyncStore } from '@/stores/sync';

/**
 * useGithubDeviceLogin — GitHub OAuth Device Flow 登录状态机（后端 sync_oauth.rs）。
 *
 * 不依赖 workbench 壳 store：内部直连 assets store（refreshGithubPatStatus）与
 * sync store（refreshStatus），先例：resourceMonitor store 由 panel 直连子 store。
 *
 * 轮询纪律：链式 setTimeout（上一次 poll 返回后再排下一次），不用固定间隔
 * 重复定时器——请求超时 10s > interval 5s，慢网下固定间隔会让请求重叠，
 * 持续触发 slow_down。倒计时同理用 setTimeout 链。
 * 倒计时同理用 setTimeout 链。
 *
 * 单流程假设（已接受的边界行为）：后端 session 是单槽，多个 PatConfigCard 实例
 * （GlobalModals 与同步面板可同时挂载）各自发起登录时，后发覆盖先发；先发实例靠
 * poll 响应的 userCode 与发起时记下的 userCode 比对，不一致即静默退出（phase=idle），
 * 不再展示已失效的验证码。
 */

/** 登录状态机相位。 */
export type DeviceLoginPhase = 'idle' | 'code' | 'pending' | 'success' | 'denied' | 'expired' | 'error';

/** sync_oauth_start 的响应字段（按本文件实际用法定义）。 */
interface SyncOauthStartResult {
  userCode: string;
  verificationUri: string;
  interval?: number;
  expiresIn?: number;
}

/** sync_oauth_poll 的响应字段（按本文件实际用法定义；status 见 Device Flow 协议 + slow_down）。 */
interface SyncOauthPollResult {
  status?: string;
  user_code?: string; // 后端 poll 响应用 snake_case（与 start 的 userCode 不一致，按实际契约保留）
  interval?: number;
}

export function useGithubDeviceLogin() {
  const assetsStore = useAssetsStore();
  const syncStore = useSyncStore();

  // idle | code（请求设备码中）| pending（等待浏览器授权）| success | denied | expired | error
  const phase = ref<DeviceLoginPhase>('idle');
  const userCode = ref('');
  const verificationUri = ref('');
  const countdownSec = ref(0);
  const errorMessage = ref('');

  let pollTimer: ReturnType<typeof setTimeout> | null = null;
  let countdownTimer: ReturnType<typeof setTimeout> | null = null;
  /** 当前轮询间隔（秒）。slow_down 时用响应下发的值，缺省则 +5。 */
  let pollIntervalSec = 5;
  /** 发起时记下的 userCode，poll 比对用（session 被覆盖检测）；非空也作为 stop 时发 cancel 的守卫。 */
  let expectedUserCode = '';
  /** 代次守卫：stop()/新 start 会自增，挂起中的旧 start resolve 后代次不符即静默放弃。 */
  let epochCounter = 0;

  // ─── 外链打开（照 SyncPatGuide.vue 的「动态 import + runtime 检测」范式）───
  // 非 Tauri runtime（浏览器预览模式）走 window.open；Tauri 走 opener 插件。
  // 动态 import 避免 npm run dev 下静态 import Tauri 插件即崩。
  async function openExternal(url: string): Promise<void> {
    if (!isTauriRuntime()) {
      window.open(url, '_blank', 'noopener');
      return;
    }
    try {
      const { openUrl } = await import('@tauri-apps/plugin-opener');
      await openUrl(url);
    } catch (e) {
      // opener 失败兜底（权限缺失等），至少不让用户卡死
      // eslint-disable-next-line no-console
      console.warn('[sync-oauth] opener failed, fallback to window.open:', e);
      window.open(url, '_blank', 'noopener');
    }
  }

  function stopTimers() {
    if (pollTimer) { clearTimeout(pollTimer); pollTimer = null; }
    if (countdownTimer) { clearTimeout(countdownTimer); countdownTimer = null; }
  }

  /** 停止本实例：清 timer 并作废挂起中的 start；已持有设备码（expectedUserCode 非空）才取消后端 session。 */
  function stop() {
    ++epochCounter;
    stopTimers();
    if (expectedUserCode) {
      // cancel 失败可忽略：session 有本地过期时间兜底，且新 start 会直接覆盖单槽
      invokeBackend('sync_oauth_cancel').catch(() => {});
    }
  }

  // ─── 倒计时（setTimeout 链）：归零 → expired ───
  function tickCountdown() {
    countdownSec.value -= 1;
    if (countdownSec.value <= 0) {
      countdownSec.value = 0;
      stopTimers();
      phase.value = 'expired';
      return;
    }
    countdownTimer = setTimeout(tickCountdown, 1000);
  }

  // ─── 轮询（setTimeout 链）───
  function schedulePoll(delaySec: number) {
    pollTimer = setTimeout(runPoll, delaySec * 1000);
  }

  async function runPoll() {
    if (phase.value !== 'pending') return; // 已停止或被新登录覆盖
    try {
      handlePollResult(await invokeBackend<SyncOauthPollResult>('sync_oauth_poll'));
    } catch (error) {
      // 网络错误停止轮询（防疯狂重试）；用户可点重新登录起新流程
      stopTimers();
      phase.value = 'error';
      errorMessage.value = error instanceof Error ? error.message : String(error);
    }
  }

  function handlePollResult(result: SyncOauthPollResult) {
    // userCode 比对：后端 session 被另一处发起的登录覆盖 → 静默退出本实例
    if (typeof result.user_code === 'string' && result.user_code !== expectedUserCode) {
      stopTimers();
      expectedUserCode = '';
      phase.value = 'idle';
      return;
    }
    switch (result.status) {
      case 'pending':
        schedulePoll(pollIntervalSec);
        break;
      case 'slow_down':
        // 优先用响应下发的新 interval；无该字段才本地 +5s
        pollIntervalSec = (typeof result.interval === 'number' && result.interval > 0)
          ? result.interval
          : pollIntervalSec + 5;
        schedulePoll(pollIntervalSec);
        break;
      case 'denied':
      case 'expired':
        stopTimers();
        expectedUserCode = '';
        phase.value = result.status;
        break;
      case 'success':
        stopTimers();
        expectedUserCode = '';
        phase.value = 'success';
        afterSuccess();
        break;
      default:
        stopTimers();
        phase.value = 'error';
        errorMessage.value = 'GitHub 登录返回未知状态：' + (result.status || '');
    }
  }

  /** token 已由后端写 SecretStore；此处只刷新状态展示（assets + sync 面板/状态栏）。 */
  async function afterSuccess() {
    try {
      await assetsStore.refreshGithubPatStatus();
    } catch {
      // 状态刷新失败不影响登录结果本身（token 已安全落库），UI 已置 success
      // eslint-disable-next-line no-console
      console.warn('[sync-oauth] refreshGithubPatStatus failed');
    }
    try {
      await syncStore.refreshStatus();
    } catch {
      // 同上：sync_status 拉取失败可忽略，面板下次打开会重拉
      // eslint-disable-next-line no-console
      console.warn('[sync-oauth] sync refreshStatus failed');
    }
  }

  /**
   * 发起登录（防重入：首行 stop 清旧流程）。浏览器预览模式下 invokeBackend 会
   * reject → phase=error，errorMessage 展示「需要 Tauri runtime」提示。
   * epoch 代次守卫：sync_oauth_start 挂起期间被 stop()/新 start 接管时，
   * 旧请求 resolve 后静默放弃（不设 timer、不改 phase、不起轮询链）。
   */
  async function start() {
    stop();
    const myEpoch = ++epochCounter;
    errorMessage.value = '';
    userCode.value = '';
    verificationUri.value = '';
    phase.value = 'code';
    try {
      const result = await invokeBackend<SyncOauthStartResult>('sync_oauth_start', { provider: 'github' });
      if (myEpoch !== epochCounter) return;
      userCode.value = result.userCode;
      expectedUserCode = result.userCode;
      verificationUri.value = result.verificationUri;
      pollIntervalSec = result.interval || 5;
      countdownSec.value = result.expiresIn || 900;
      phase.value = 'pending';
      openExternal(result.verificationUri); // 异步开浏览器，不阻塞轮询启动
      countdownTimer = setTimeout(tickCountdown, 1000);
      schedulePoll(pollIntervalSec);
    } catch (error) {
      if (myEpoch !== epochCounter) return;
      phase.value = 'error';
      errorMessage.value = error instanceof Error ? error.message : String(error);
    }
  }

  /** 设备码面板的「打开浏览器授权」按钮（重复点击重开授权页）。 */
  function openVerificationPage() {
    if (verificationUri.value) openExternal(verificationUri.value);
  }

  onUnmounted(stop);

  return { phase, userCode, verificationUri, countdownSec, errorMessage, start, stop, openVerificationPage };
}
