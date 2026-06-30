// 应用内自动更新（v1.7 发版链路）。
//
// 设计：复用状态栏 statusMessage（已渲染为可点击按钮）承载更新提示，
// 不引入新弹窗/toast 组件，符合 AGENTS.md「禁止造轮子」红线。
//
// 状态机：idle →（启动检查）→ available（状态栏显示「发现新版本 vX，点击更新」）
//   → 用户点击 → downloading（显示「更新下载中…」）→ installed（自动重启）
//   → 任一步失败 → error（显示原因，可再点重试）。
//
// 浏览器预览模式（npm run dev，无 Tauri runtime）：静默 no-op，不报错。
// 检查失败（无网络 / endpoint 未就绪）也静默，避免启动时刷屏。

import { isTauriRuntime } from '../services/backend.js';

// 仅 desktop runtime 动态拉取 updater JS API（避开浏览器预览模式缺模块）。
async function loadUpdater() {
  const mod = await import('@tauri-apps/plugin-updater');
  return mod.check;
}

export function useAutoUpdate({ announce, getVersion }) {
  // 状态：'idle' | 'available' | 'downloading' | 'error'
  // installed 态进程会直接重启，无需 UI 态。
  let state = 'idle';
  let pendingUpdate = null; // 抓到的 Update 对象，点击时复用
  let newVersion = '';

  function setState(next, message) {
    state = next;
    if (message !== undefined) announce(message);
  }

  // 启动检查：应用 onMounted 后调用。失败静默（不打扰用户）。
  async function init() {
    if (!isTauriRuntime()) return; // 浏览器预览模式无更新能力
    try {
      const check = await loadUpdater();
      const update = await check();
      if (!update) return; // 已是最新，不提示
      pendingUpdate = update;
      newVersion = update.version;
      setState('available', `发现新版本 v${newVersion}，点击状态栏更新`);
    } catch (err) {
      // 启动检查失败（无网络 / endpoint 未就绪 / 公钥未配置）不刷屏，仅 console 留痕。
      console.warn('[auto-update] 启动检查失败：', err?.message || err);
    }
  }

  // 用户点击状态栏消息时调用。按当前状态分派动作。
  async function onClick() {
    if (state === 'available' && pendingUpdate) {
      await downloadAndInstall();
    } else if (state === 'error') {
      // 出错后点击 = 重试一次检查。
      announce('正在重新检查更新…');
      await init();
    }
    // downloading / idle 态点击：无操作（避免重复触发）
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

  return { init, onClick };
}
