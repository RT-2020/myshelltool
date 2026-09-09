/**
 * assetWindows — 独立资产工作台窗口（Phase 1-A）的 label 工具与开窗封装。
 *
 * 每个资产对应一个独立 WebviewWindow（label 由 asset.id 派生，确定性），
 * 窗口内容为 `/index.html?win=asset&assetId=<id>`（App.vue 按 query 分支渲染
 * AssetWindowShell）。已存在同 label 窗口时聚焦而非重复创建。
 */
import {
  createTauriWebviewWindow,
  getExistingTauriWebviewWindow,
  isTauriRuntime
} from '../services/backend.js';

// Tauri 窗口 label 仅允许 [a-zA-Z0-9-/:_]，其余字符替换为 '-'（确定性映射）
export function sanitizeWindowLabel(id) {
  return String(id ?? '').replace(/[^a-zA-Z0-9-/:_]/g, '-');
}

export function assetWindowLabel(asset) {
  return 'asset-' + sanitizeWindowLabel(asset?.id);
}

// opts.adoptSessionId：跨窗口会话迁移（sessionHandoff）时携带——新窗口按该 id
// 从 Rust 侧 handoff 中转（session_handoff_take）接管会话（adopt 分支见
// assetWindowBoot）；窗口已存在时忽略（已存在窗口经 TEAROFF 事件监听路径接管，
// 不走 URL 参数）。
export async function openAssetWindow(asset, { adoptSessionId } = {}) {
  if (!isTauriRuntime()) return;
  const label = assetWindowLabel(asset);
  // getByLabel 是 async（见 backend.js 注释），必须 await——否则拿到 Promise 恒 truthy，
  // 首次开窗也会误走聚焦分支。
  const existing = await getExistingTauriWebviewWindow(label);
  if (existing) {
    // 已开窗：聚焦 + 还原最小化（失败静默——窗口可能正被系统动画接管）
    existing.setFocus?.()?.catch?.(() => null);
    existing.unminimize?.()?.catch?.(() => null);
    return;
  }
  try {
    const win = await createTauriWebviewWindow(label, {
      url: '/index.html?win=asset&assetId=' + encodeURIComponent(asset.id)
        + (adoptSessionId ? '&adopt=' + encodeURIComponent(adoptSessionId) : ''),
      title: asset.name + ' · myshelltool',
      width: 1200,
      height: 760,
      minWidth: 900,
      minHeight: 600,
      center: true,
      decorations: false,
      resizable: true,
      skipTaskbar: false
    });
    // Tauri 2 构造函数不抛创建失败：错误经实例的 tauri://error 事件上报（官方模式），
    // 这里记录到 console 供排查（IPC 权限缺失/label 冲突等）。
    win?.once?.('tauri://error', event => {
      console.error('[assetWindows] window creation error:', event?.payload || event);
    });
  } catch (error) {
    // 报错后 rethrow：调用方（主窗口侧）可 announce 给用户
    console.error('[assetWindows] create window failed:', error);
    throw error;
  }
}

// 判定点（CSS 像素）是否在当前视口外。多显示器场景 clientX/Y 以本屏视口为
// 基准（可为负/超界），Phase 2 拖出资产判定用。
export function isPointOutsideViewport(clientX, clientY) {
  return clientX < 0 || clientY < 0 || clientX > window.innerWidth || clientY > window.innerHeight;
}
