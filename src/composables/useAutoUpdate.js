// 应用内自动更新（v1.7 发版链路，v1.8 接入设置面板手动检查）。
//
// 设计：状态以 ref 暴露，供设置面板「关于与更新」tab 响应式渲染按钮状态机；
// 同时复用状态栏 statusMessage（已渲染为可点击按钮）承载更新提示，
// 不引入新弹窗/toast 组件，符合 AGENTS.md「禁止造轮子」红线。
//
// 状态机：idle →（init/check 检查）→ checking → available（状态栏「发现新版本 vX，点击更新」）
//   → 用户点击 → downloading（显示「更新下载中…」）→ installed（自动重启）
//   → 任一步失败 → error（显示原因，可再点重试）。
//
// 浏览器预览模式（npm run dev，无 Tauri runtime）：静默 no-op，不报错。
// 检查失败（无网络 / endpoint 未就绪）：init 静默（启动不打扰），check 提示（用户主动触发）。

import { ref } from 'vue';
import { isTauriRuntime, invokeBackend } from '../services/backend.js';

// 仅 desktop runtime 动态拉取 updater JS API（避开浏览器预览模式缺模块）。
async function loadUpdater() {
  const mod = await import('@tauri-apps/plugin-updater');
  return mod.check;
}

export function useAutoUpdate({ announce }) {
  // 状态：'idle' | 'checking' | 'available' | 'downloading' | 'up_to_date' | 'error'
  // installed 态进程会直接重启，无需单独 UI 态。
  const state = ref('idle');
  const newVersion = ref('');
  const errorMessage = ref('');
  const downloadProgress = ref(0);
  const lastCheckedAt = ref(null);
  let pendingUpdate = null; // 抓到的 Update 对象，点击时复用（非响应式，仅内部用）

  function setState(next, message, opts) {
    state.value = next;
    if (message !== undefined) announce(message, opts);
  }

  // 执行一次更新检查。silent=true 时失败静默（启动 init 用），
  // silent=false 时失败提示（用户主动 check 用，反馈更明确）。
  async function runCheck(silent) {
    if (!isTauriRuntime()) return; // 浏览器预览模式无更新能力
    state.value = 'checking';
    errorMessage.value = '';
    downloadProgress.value = 0;
    try {
      const check = await loadUpdater();
      const update = await check();
      lastCheckedAt.value = new Date();
      if (!update) {
        // 已是最新。silent 模式不提示（避免启动刷屏），主动检查时给明确 toast 反馈。
        state.value = 'up_to_date';
        if (!silent) announce('当前已是最新版本', { level: 'info' });
        return;
      }
      pendingUpdate = update;
      newVersion.value = update.version;
      setState('available', `发现新版本 v${newVersion.value}，可点击设置或状态栏进行更新`, { level: 'success' });
    } catch (err) {
      state.value = 'error';
      errorMessage.value = err?.message || String(err);
      lastCheckedAt.value = new Date();
      if (silent) {
        // 启动检查失败（无网络 / endpoint 未就绪 / 公钥未配置）不刷屏，仅 console 留痕。
        console.warn('[auto-update] 启动检查失败：', errorMessage.value);
      } else {
        announce(`检查更新失败：${errorMessage.value}（点击重试）`, { level: 'error' });
      }
    }
  }

  // 启动检查：应用 onMounted 后调用，silent 失败静默。
  async function init() {
    return runCheck(true);
  }

  // 手动检查：设置面板「检查更新」按钮调用，失败有反馈。
  async function check() {
    return runCheck(false);
  }

  // 用户点击状态栏消息或设置面板主按钮时调用。
  async function onClick() {
    if (state.value === 'available' && pendingUpdate) {
      await downloadAndInstall();
    } else if (state.value === 'error') {
      announce('正在重新检查更新…', { level: 'info' });
      await runCheck(false);
    }
    // downloading / idle / checking 态点击：无操作（避免重复触发）
  }

  async function downloadAndInstall() {
    if (!pendingUpdate) return;
    setState('downloading', '更新下载中…（完成后将自动重启）', { level: 'info' });
    downloadProgress.value = 0;
    try {
      let downloaded = 0;
      let total = 0;
      await pendingUpdate.downloadAndInstall(event => {
        switch (event.event) {
          case 'Started':
            total = event.data.contentLength ?? 0;
            break;
          case 'Progress':
            downloaded += event.data.chunkLength ?? 0;
            if (total > 0) {
              const pct = Math.min(100, Math.round((downloaded / total) * 100));
              downloadProgress.value = pct;
              announce(`更新下载中… ${pct}%`);
            }
            break;
          case 'Finished':
            downloadProgress.value = 100;
            announce('下载完成，正在安装并准备重启…', { level: 'info' });
            break;
        }
      });

      // Tauri v2 Update 实例上无 relaunch 方法，通过后端 app_relaunch 命令优雅重启。
      try {
        await invokeBackend('app_relaunch');
      } catch (relaunchErr) {
        console.warn('[auto-update] app_relaunch 失败，尝试 fallback：', relaunchErr);
        if (typeof pendingUpdate.relaunch === 'function') {
          await pendingUpdate.relaunch();
        } else {
          announce('更新安装完成，请手动重启应用以生效。', { level: 'success' });
        }
      }
    } catch (err) {
      errorMessage.value = err?.message || String(err);
      setState('error', `更新失败：${errorMessage.value}（点击重试）`, { level: 'error' });
      console.warn('[auto-update] 下载安装失败：', err);
    }
  }

  return {
    state,
    newVersion,
    errorMessage,
    downloadProgress,
    lastCheckedAt,
    init,
    check,
    onClick,
    downloadAndInstall
  };
}
