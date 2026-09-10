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
} from '../services/backend';

// tauri.d.ts 只声明了 backend.ts 实际用到的窗口方法；本文件的 setFocus /
// unminimize / once 以局部交叉类型补齐（运行时可选调用，与原防御式写法一致）。
type FocusableWebviewWindow = TauriWebviewWindow & {
  setFocus?: () => Promise<void>;
  unminimize?: () => Promise<void>;
};

type ListenableWebviewWindow = TauriWebviewWindow & {
  once?: (event: string, handler: (event: TauriEvent) => void) => Promise<unknown>;
};

/** 开窗所需的最小资产形状（真实资产为其结构子集）。 */
export interface AssetLike {
  id: string;
  name: string;
}

// djb2 8 位十六进制短哈希（确定性、无依赖）：id 含 Tauri label 白名单外字符
// 时追加，保证 sanitize 后不同 id 不碰撞（见 sanitizeWindowLabel 注释）。
function idHash8(id: string): string {
  let h = 5381;
  for (let i = 0; i < id.length; i += 1) {
    h = ((h << 5) + h + id.charCodeAt(i)) >>> 0;
  }
  return h.toString(16).padStart(8, '0');
}

// Tauri 窗口 label 仅允许 [a-zA-Z0-9-/:_]。纯替换式映射非单射：外部来源 id
// （normalizeAsset 原样接受）可携带任意字符，"web server" 与 "web-server" 会
// 映射到同一 label，点资产 B 的「独立窗口」却聚焦到 A 的窗口。防碰撞策略：
// 安全字符原样保留；【发生替换时】追加原 id 的短哈希后缀（哈希十六进制 +
// '-' 均在白名单内）。注意：此策略下含特殊字符的 id 对应 label 会与旧版不同
// ——已开的旧 label 窗口不会被新 label 命中，但窗口不持久化恢复（AGENTS.md
// 已声明），重启后自愈，属可接受的一次性偏移。
export function sanitizeWindowLabel(id: unknown): string {
  const raw = String(id ?? '');
  const sanitized = raw.replace(/[^a-zA-Z0-9-/:_]/g, '-');
  return sanitized === raw ? sanitized : `${sanitized}-${idHash8(raw)}`;
}

// 单元自检（无 JS 单测框架，按任务约定放同文件；仅 DEV 模式执行）：验证两组
// 旧映射下会碰撞的 id 现在产生不同 label，且安全 id 保持原样不追加哈希。
export function selfTestSanitizeWindowLabel(): string[] {
  const failures: string[] = [];
  const cases: Array<[string, string]> = [
    ['web server', 'web-server'],
    ['web?server', 'web-server']
  ];
  for (const [a, b] of cases) {
    if (sanitizeWindowLabel(a) === sanitizeWindowLabel(b)) {
      failures.push(`collision: "${a}" vs "${b}" -> ${sanitizeWindowLabel(a)}`);
    }
  }
  if (sanitizeWindowLabel('web-server') !== 'web-server') {
    failures.push('safe id should pass through unchanged');
  }
  return failures;
}

if (import.meta.env.DEV) {
  const failures = selfTestSanitizeWindowLabel();
  for (const f of failures) console.error('[assetWindows] label self-test failed:', f);
}

export function assetWindowLabel(asset?: { id?: unknown } | null): string {
  return 'asset-' + sanitizeWindowLabel(asset?.id);
}

// opts.adoptSessionId：跨窗口会话迁移（sessionHandoff）时携带——新窗口按该 id
// 从 Rust 侧 handoff 中转（session_handoff_take）接管会话（adopt 分支见
// assetWindowBoot）；窗口已存在时忽略（已存在窗口经 TEAROFF 事件监听路径接管，
// 不走 URL 参数）。
export async function openAssetWindow(asset: AssetLike | null | undefined, { adoptSessionId }: { adoptSessionId?: string } = {}): Promise<void> {
  if (!isTauriRuntime()) return;
  const label = assetWindowLabel(asset);
  // getByLabel 是 async（见 backend.js 注释），必须 await——否则拿到 Promise 恒 truthy，
  // 首次开窗也会误走聚焦分支。
  const existing = await getExistingTauriWebviewWindow(label);
  if (existing) {
    const focusable = existing as FocusableWebviewWindow;
    // 已开窗：聚焦 + 还原最小化（失败静默——窗口可能正被系统动画接管）
    focusable.setFocus?.()?.catch?.(() => null);
    focusable.unminimize?.()?.catch?.(() => null);
    return;
  }
  try {
    // 调用方保证 asset 非空（sessionHandoff/ConnectionSidebar 均先判会话存在）；
    // 非空断言保持与原 JS 一致的直接解引用行为
    const win = await createTauriWebviewWindow(label, {
      url: '/index.html?win=asset&assetId=' + encodeURIComponent(asset!.id)
        + (adoptSessionId ? '&adopt=' + encodeURIComponent(adoptSessionId) : ''),
      title: asset!.name + ' · myshelltool',
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
    (win as ListenableWebviewWindow)?.once?.('tauri://error', event => {
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
export function isPointOutsideViewport(clientX: number, clientY: number): boolean {
  return clientX < 0 || clientY < 0 || clientX > window.innerWidth || clientY > window.innerHeight;
}
