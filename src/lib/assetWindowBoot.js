/**
 * assetWindowBoot — 独立资产窗口启动后的引导（Phase 1-A + v2.4 adopt 分支）。
 *
 * 两条路径：
 * 1. adopt（URL 带 &adopt=<sessionId>，来自跨窗口 tab 迁移）：从 Rust 内存中转
 *    读迁移数据（scrollback + oscTitle，take 即原子删除）→ sessionsStore.
 *    adoptSession 接管既有 SSH 连接（不重连）；失败/过期则断开 Rust 侧连接防
 *    孤儿，落入正常连接流程。
 * 2. 常规：按 assetId 选中资产并发起连接。找不到资产时 announce 后直接返回，
 *    绝不 fallback 到 selectedAsset（避免误连主窗口上次选中的另一台主机）。
 */
import { useSessionsStore } from '../stores/sessions.js';
import { invokeBackend } from '../services/backend.js';
import { readHandoffData } from './sessionHandoff.js';

export async function bootAssetWindow(store, { assetId, adoptSessionId } = {}) {
  const exists = (store.assets || []).some(asset => asset?.id === assetId);
  if (!exists) {
    store.announce('资产不存在或已被删除，无法连接');
    return;
  }
  store.selectAsset(assetId, false);
  // assets.selectAsset 对不存在 id 是静默 return——校验选中结果再连接，
  // 防止 assets 列表在初始化后、boot 前被并发修改的边界
  if (store.selectedAssetId !== assetId) {
    store.announce('资产不存在或已被删除，无法连接');
    return;
  }
  if (adoptSessionId) {
    const data = await readHandoffData(adoptSessionId);
    if (data) {
      // pinia 已随 App.vue 安装（boot 在 initialize 完成后调用），与主实例同 store
      const sessionsStore = useSessionsStore();
      // try/catch 防护：App.vue 的 initialize 链尾 .catch(() => null) 会吞掉
      // adoptSession 内部抛错，导致无提示也不重连——此处接住转走兜底路径
      let ok = false;
      try {
        ok = await sessionsStore.adoptSession(data);
      } catch (error) {
        console.error('[assetWindowBoot] adopt failed:', error);
      }
      if (ok) {
        // 面板上下文：adoptSession 内 setActiveSession 已触发 syncAssetSelection，
        // 外部再调一次幂等无害，保留以确保面板就绪
        store.selectAsset(assetId, false);
        return;
      }
    } else {
      // 数据缺失/过期：会话可能已被本窗口并发 TEAROFF 接管（同资产窗口已开、
      // 事件先于 boot 到达并 take 走数据）——已在则直接返回，不重连不断开
      const sessionsStore = useSessionsStore();
      if (sessionsStore.sessions.some(s => s.sessionId === adoptSessionId)) return;
    }
    // 失败/过期：断开防 Rust 侧孤儿连接（评审 Issue 8），落入正常连接流程
    invokeBackend('ssh_disconnect', { sessionId: adoptSessionId }).catch(() => null);
    store.announce('会话迁移失败，已尝试重新连接', { level: 'warn' });
  }
  await store.connectSelected();
}
