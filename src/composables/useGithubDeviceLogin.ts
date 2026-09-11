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
 * 重复定时器——请求超时 > interval，慢网下固定间隔会让请求重叠，持续触发 slow_down。
 * 倒计时同理用 setTimeout 链。
 *
 * 【v2.7 传输故障不再中断登录】后端把「本次没问到」（网络/代理抖动、429/5xx、
 * 非 JSON 响应）与「GitHub 拒绝授权」分开返回：前者是 `unstable`，设备码在 GitHub
 * 端仍然有效，本状态机据此**退避自动重试**（5/10/20/30s，且不低于 GitHub 下发的
 * interval），并在卡片上显示降级提示；只有 `failed` / `denied` / `expired` 才是终态。
 * 重试次数不设上限：设备码有效期（倒计时 + 后端 expires_at）才是权威边界，一个
 * 拍脑袋的「重试 N 次就放弃」会在用户正在浏览器里输验证码时把流程掐掉。
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
  /** status='unstable'/'failed' 时的人话原因（含传输层 cause 链）。 */
  reason?: string;
}

/** 传输故障退避表（秒）：第 N 次连续失败用第 N 项，超出取末项。 */
const RETRY_BACKOFF_SEC = [5, 10, 20, 30];

export function useGithubDeviceLogin() {
  const assetsStore = useAssetsStore();
  const syncStore = useSyncStore();

  // idle | code（请求设备码中）| pending（等待浏览器授权）| success | denied | expired | error
  const phase = ref<DeviceLoginPhase>('idle');
  const userCode = ref('');
  const verificationUri = ref('');
  const countdownSec = ref(0);
  const errorMessage = ref('');
  /** 是否处于「传输故障自动重试」降级态（卡片展示提示用）。 */
  const retrying = ref(false);
  /** 连续传输失败次数（成功一次即归零）。 */
  const retryCount = ref(0);
  /** 最近一次传输失败的原因（后端已给可读描述）。 */
  const retryReason = ref('');

  let pollTimer: ReturnType<typeof setTimeout> | null = null;
  let countdownTimer: ReturnType<typeof setTimeout> | null = null;
  /** 当前轮询间隔（秒）。slow_down 时用响应下发的值，缺省则 +5。 */
  let pollIntervalSec = 5;
  /** 发起时记下的 userCode，poll 比对用（session 被覆盖检测）；非空也作为 stop 时发 cancel 的守卫。 */
  let expectedUserCode = '';
  /** 代次守卫：stop()/新 start 会自增，挂起中的旧 start/poll resolve 后代次不符即静默放弃。 */
  let epochCounter = 0;
  /** 连续传输失败次数（与 retryCount 同步，供退避计算；成功一次归零）。 */
  let consecutiveFailures = 0;

  // ─── 外链打开（「动态 import + runtime 检测」范式，原 SyncPatGuide 沉淀）───
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

  /** 退避/重试态归零（任一次拿到协议响应即调用，成功或失败都算「链路通了」）。 */
  function resetRetryState() {
    consecutiveFailures = 0;
    retrying.value = false;
    retryCount.value = 0;
    retryReason.value = '';
  }

  /** 停止本实例：清 timer 并作废挂起中的 start/poll；已持有设备码（expectedUserCode 非空）才取消后端 session。 */
  function stop() {
    ++epochCounter;
    stopTimers();
    resetRetryState();
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
      resetRetryState();
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
    pollTimer = null;
    // 代次快照：本次网络往返期间若发生 stop()/重新发起，响应一律作废——
    // 否则「旧流程的响应」会改掉「新流程」的状态（曾把新登录打回 idle）。
    const myEpoch = epochCounter;
    let result: SyncOauthPollResult;
    try {
      result = await invokeBackend<SyncOauthPollResult>('sync_oauth_poll');
    } catch (error) {
      // 后端命令本身失败（内部错误/非 Tauri runtime）：同样按可重试故障处理，
      // 由退避链决定什么时候放弃（权威边界是设备码有效期）。
      if (myEpoch === epochCounter && phase.value === 'pending') {
        handlePollFailure(error instanceof Error ? error.message : String(error));
      }
      return;
    }
    if (myEpoch !== epochCounter || phase.value !== 'pending') return;
    handlePollResult(result);
  }

  /**
   * 传输故障（`unstable` 或 IPC 失败）处理：**不终止登录**，退避后重排一次轮询。
   *
   * 为什么不用上限次数：设备码在 GitHub 端有效 900s，重试不会重复消费它；用户可能
   * 正在浏览器里输验证码，此时因为「重试次数用尽」把流程作废是纯粹的损失。
   * 真正的边界是倒计时（后端 expires_at 同源）——归零即 expired，链路坏到底也不会卡死。
   */
  function handlePollFailure(reason: string) {
    if (phase.value !== 'pending') return;
    consecutiveFailures += 1;
    retryCount.value = consecutiveFailures;
    retrying.value = true;
    retryReason.value = reason || '网络请求失败';
    const backoff = RETRY_BACKOFF_SEC[Math.min(consecutiveFailures - 1, RETRY_BACKOFF_SEC.length - 1)];
    // 不低于 GitHub 下发的 interval：退避只能更慢，不能更快（否则触发 slow_down）
    schedulePoll(Math.max(pollIntervalSec, backoff));
  }

  function handlePollResult(result: SyncOauthPollResult) {
    if (phase.value !== 'pending') return;
    // userCode 比对：后端 session 被另一处发起的登录覆盖 → 静默退出本实例
    if (typeof result.user_code === 'string' && result.user_code !== expectedUserCode) {
      stopTimers();
      resetRetryState();
      expectedUserCode = '';
      phase.value = 'idle';
      return;
    }
    // unstable 要**保留**连续失败计数（否则退避永远停在第一档）；其余响应说明链路是通的，计数归零。
    if (result.status !== 'unstable') resetRetryState();
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
      case 'unstable':
        // 后端判定「本次没问到」（网络/代理抖动、429/5xx、非 JSON 响应）→ 退避重试
        handlePollFailure(result.reason || '网络连接不稳定');
        break;
      case 'superseded':
        // 后端已无本流程的 session（被覆盖/已取消/应用重启过）→ 静默回初始态
        stopTimers();
        expectedUserCode = '';
        phase.value = 'idle';
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
      case 'failed':
        stopTimers();
        expectedUserCode = '';
        phase.value = 'error';
        errorMessage.value = result.reason || 'GitHub 登录失败';
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

  return {
    phase,
    userCode,
    verificationUri,
    countdownSec,
    errorMessage,
    retrying,
    retryCount,
    retryReason,
    start,
    stop,
    openVerificationPage
  };
}
