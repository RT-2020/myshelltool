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
import { isTauriRuntime } from '../services/backend.js';

// 仅 desktop runtime 动态拉取 updater JS API（避开浏览器预览模式缺模块）。
async function loadUpdater() {
  const mod = await import('@tauri-apps/plugin-updater');
  return mod.check;
}

export function useAutoUpdate({ announce }) {
  // 状态：'idle' | 'checking' | 'available' | 'downloading' | 'error'
  // installed 态进程会直接重启，无需 UI 态。
  // v1.8 起改为 ref，供设置面板响应式读取（此前是闭包 let，外部无法感知）。
  const state = ref('idle');
  const newVersion = ref('');
  let pendingUpdate = null; // 抓到的 Update 对象，点击时复用（非响应式，仅内部用）

  function setState(next, message) {
    state.value = next;
    if (message !== undefined) announce(message);
  }

  // 执行一次更新检查。silent=true 时失败静默（启动 init 用），
  // silent=false 时失败提示（用户主动 check 用，反馈更明确）。
  async function runCheck(silent) {
    if (!isTauriRuntime()) return; // 浏览器预览模式无更新能力
    state.value = 'checking';
    try {
      const check = await loadUpdater();
      const update = await check();
      if (!update) {
        // 已是最新。silent 模式不提示（避免启动刷屏），主动检查时给个明确反馈。
        state.value = 'idle';
        if (!silent) announce('当前已是最新版本');
        return;
      }
      pendingUpdate = update;
      newVersion.value = update.version;
      setState('available', `发现新版本 v${newVersion.value}，点击状态栏更新`);
    } catch (err) {
      state.value = 'error';
      if (silent) {
        // 启动检查失败（无网络 / endpoint 未就绪 / 公钥未配置）不刷屏，仅 console 留痕。
        console.warn('[auto-update] 启动检查失败：', err?.message || err);
      } else {
        announce(`检查更新失败：${err?.message || err}（点击重试）`);
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

  // 用户点击状态栏消息时调用。按当前状态分派动作。
  async function onClick() {
    if (state.value === 'available' && pendingUpdate) {
      await downloadAndInstall();
    } else if (state.value === 'error') {
      // 出错后点击 = 重试一次检查。
      announce('正在重新检查更新…');
      await runCheck(false);
    }
    // downloading / idle / checking 态点击：无操作（避免重复触发）
  }

  async function downloadAndInstall() {
    setState('downloading', '更新下载中…（完成后将自动重启）');
    try {
      let downloaded = 0;
      let total = 0;
      await pendingUpdate.downloadAndInstall(event => {
        switch (event.event) {
          case 'Started': total = event.data.contentLength ?? 0; break;
          case 'Progress':
            downloaded += event.data.chunkLength ?? 0;
            if (total > 0) {
              const pct = Math.min(100, Math.round((downloaded / total) * 100));
              announce(`更新下载中… ${pct}%`);
            }
            break;
          // 'Finished' 后 install 自动执行，随后 relaunch。
        }
      });
      await pendingUpdate.relaunch();
      // relaunch 后当前进程退出，下面的代码一般不会执行到。
    } catch (err) {
      setState('error', `更新失败：${err?.message || err}（点击重试）`);
      console.warn('[auto-update] 下载安装失败：', err);
    }
  }

  return { state, newVersion, init, check, onClick };
}
