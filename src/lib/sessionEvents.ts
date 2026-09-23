/**
 * sessionEvents — 后端事件接线（host key / keyboard interactive / 会话状态）+
 * 一次性连接登记表 + prompt 解析回传，从 sessions store 按域拆出（第二刀）。
 *
 * 路由契约（跨窗口）：Rust app.emit 是全局广播，各窗口独立 Pinia 实例都收到
 * 同一份事件；认领条件 = 本窗口有 connecting 会话或在途一次性连接，不认领即
 * 静默跳过（属正确的窗口路由，不是吞错误）。绑定式 context；绑定即注册
 * hostKey/keyboard 监听与 58s 超时 watcher（detached effectScope，防嵌套
 * 实例化时 activeEffect 为 null 崩溃——原注释语义保留）。
 */
import { effectScope, watch, type Ref } from 'vue';
import type {
  HostKeyVerifyPayload,
  KeyboardInteractivePayload,
  ModalState,
  NormalizedConnectionAsset,
  NotifyOptions
} from '@/types/domain';
import { invokeBackend, isTauriRuntime, listenBackendEvent } from '@/services/backend';
import { errorMessage } from '@/lib/errorMessage';
import type { SessionEntry } from '@/lib/terminalTypes';

// 事件 channel 常量（原 workbench.js:12-14）。传输进度事件归 files store。
const HOST_KEY_VERIFY_EVENT = 'ssh-host-key-verify';
const KEYBOARD_INTERACTIVE_EVENT = 'ssh-keyboard-interactive';
// 统一会话状态事件：Rust 在 连接成功 / 远端关闭 / 用户断开 三处 emit。
const SESSION_STATUS_EVENT = 'ssh-session-status';

export interface SessionEventsContext {
  sessions: Ref<SessionEntry[]>;
  hostKeyPrompt: Ref<HostKeyVerifyPayload | null>;
  keyboardPrompt: Ref<KeyboardInteractivePayload | null>;
  setModal(modal: ModalState): void;
  selectedAsset(): NormalizedConnectionAsset | null;
  announce(message: string, opts?: NotifyOptions): void;
}

let bound: SessionEventsContext | null = null;
let hostKeyUnlisten: TauriUnlistenFn | null = null;
let keyboardUnlisten: TauriUnlistenFn | null = null;
let statusUnlisten: TauriUnlistenFn | null = null;
let hostKeyTimeout: ReturnType<typeof setTimeout> | null = null;
let _ensureHostKeyListeners: (() => Promise<void>) | null = null;

function ec(): SessionEventsContext {
  if (!bound) throw new Error('sessionEvents 未绑定 context（sessions store 未初始化）');
  return bound;
}

/**
 * 绑定 store 上下文并立即注册 hostKey/keyboard 监听与 58s 超时 watcher。
 * 58s watcher 用 detached effectScope：sessions store 被嵌套实例化时外层
 * scope 未激活，watch() 会因 activeEffect === null 崩溃（原注释语义保留）。
 */
export function bindSessionEventsContext(ctx: SessionEventsContext): void {
  bound = ctx;

  const sessionScope = effectScope(true);
  sessionScope.run(() => {
    _ensureHostKeyListeners = async function ensureHostKeyListeners() {
      if (typeof window === 'undefined') return;
      try {
        if (!hostKeyUnlisten) {
          hostKeyUnlisten = await listenBackendEvent(HOST_KEY_VERIFY_EVENT, onHostKeyVerifyEvent);
        }
        if (!keyboardUnlisten) {
          keyboardUnlisten = await listenBackendEvent(KEYBOARD_INTERACTIVE_EVENT, onKeyboardInteractiveEvent);
        }
      } catch (error) {
        // eslint-disable-next-line no-console
        console.warn('host key / keyboard listener registration deferred:', errorMessage(error));
      }
    };

    // 立即注册（原 workbench.js:94-96）
    if (typeof window !== 'undefined') {
      _ensureHostKeyListeners();
    }

    // hostKeyPrompt 58s 自动清理（v0.20 修复：必须**早于**后端 60s 超时——
    // 原为 65s（60s+5s 缓冲），但缓冲加在了后端之后，制造出 60~65s 的
    // 「弹窗仍可点、确认必失败」窗口（真机验收 2026-09-23 实测踩中）。
    // 改为 58s：超时即关窗并提示重连，用户在弹窗可点期间确认必然有效。
    // CRITICAL Critic 改进 2：watcher + hostKeyTimeout 闭包必须随 sshConfirmHostKey 一起迁移
    watch(ctx.hostKeyPrompt, prompt => {
      if (hostKeyTimeout) {
        clearTimeout(hostKeyTimeout);
        hostKeyTimeout = null;
      }
      if (prompt) {
        hostKeyTimeout = setTimeout(() => {
          if (ctx.hostKeyPrompt.value) {
            ctx.hostKeyPrompt.value = null;
            ctx.setModal({ type: null, asset: null });
            ctx.announce('主机密钥验证超时（58秒未响应，连接已被中止），请重新连接', { level: 'warn' });
          }
        }, 58000);
      }
    });
  });
}

// ============================================================
// Listeners — 三个 unlisten 句柄（CRITICAL Critic 改进 2/4）
// ============================================================
// 跨窗口事件路由守卫（多 WebviewWindow / 每窗口独立 Pinia 实例）：
// Rust 侧 app.emit 是全局广播，所有窗口都会收到同一份 host key /
// keyboard 事件。认领条件 = 本窗口有 status==='connecting' 的会话——
// **不认领即代表事件属于别的窗口**，静默跳过是正确的路由行为（不是吞错误）。
// 守卫对本窗口 connecting 会话恒通过，单窗口使用路径行为不变。
// （v0.20 2026-09-23：原「在途一次性连接」条件随 ssh_list_directory 回落分支
// 删除——文件面板不再有会话外的连接，host key 事件只可能来自真实会话。）
export function ownsConnectPrompt(hostPort?: string | null) {
  if (!hostPort) {
    // keyboard 事件 payload 无 host 字段，无法精确路由：本窗口有 connecting
    // 会话即认领。残留边界：两窗口同时 connecting（尤其同一 host）时无法区分
    // 归属，可能双弹框——已知边界，暂不修。
    return ec().sessions.value.some(s => s.status === 'connecting');
  }
  // host key 事件按主机路由：payload.host_port 形如 "host:port"（Rust
  // HostKeyVerifyEvent 无 rename_all，序列化字段名即 host_port）。优先
  // host:port 全等（asset.port 经 normalizeAsset 默认 22），回退去端口
  // 等值比较。
  return ec().sessions.value.some(s => {
    if (s.status !== 'connecting') return false;
    const a = s.asset;
    if (!a?.host) return false;
    if (a.port && hostPort === `${a.host}:${a.port}`) return true;
    return String(hostPort).replace(/:\d+$/, '') === a.host;
  });
}

// host key / keyboard 共用 handler：ensureHostKeyListeners 与
// setupEventListeners 两个注册点引用同一份，路由守卫只写一处。
export function onHostKeyVerifyEvent(event: TauriEvent) {
  const payload = (event?.payload || null) as HostKeyVerifyPayload | null;
  if (!ownsConnectPrompt(payload?.host_port)) return;
  ec().hostKeyPrompt.value = payload;
  // 展示用资产：host key 事件只来自真实会话的连接（回落分支已删），用本窗口
  // 选中资产（多窗口下 connecting 会话即本窗口发起，归属一致）。
  ec().setModal({ type: 'hostKeyVerify', asset: ec().selectedAsset() });
}

export function onKeyboardInteractiveEvent(event: TauriEvent) {
  if (!ownsConnectPrompt()) return;
  ec().keyboardPrompt.value = event.payload as KeyboardInteractivePayload;
  ec().setModal({ type: 'keyboardInteractive', asset: ec().selectedAsset() });
}

// 用 detached effect scope 注册 hostKeyPrompt watcher + 立即触发的
// ensureHostKeyListeners 调用。当 sessions store 被嵌套实例化（如
// workbench setup 期间调用 useSessionsStore()）时，外层 effect scope
// 还未激活，Vue 的 watch() 会因 activeEffect === null 而崩溃。
// detached scope 把这些副作用从父 scope 解绑，避免 reactivity 报错。
export function ensureHostKeyListeners(): Promise<void> | undefined {
  if (_ensureHostKeyListeners) return _ensureHostKeyListeners();
}


export async function setupEventListeners() {
  if (!isTauriRuntime()) return;

  // 传输进度事件（sftp-transfer-progress）不在本 store 监听：它是全局广播，
  // files store 的 setupEventListeners 已注册同名监听并直接调用自己的
  // updateTransferProgress（transferQueue 归 files 所有）。此处原先的第二份监听是
  // Wave 2.2 拆分过渡期留下的，经 workbench bridge 转发到的是同一个函数 ——
  // 结果每条进度事件都要跑两遍队列查找与速度/ETA 计算，多窗口下再乘以窗口数，故移除。
  if (!hostKeyUnlisten) {
    hostKeyUnlisten = await listenBackendEvent(HOST_KEY_VERIFY_EVENT, onHostKeyVerifyEvent);
  }
  if (!keyboardUnlisten) {
    keyboardUnlisten = await listenBackendEvent(KEYBOARD_INTERACTIVE_EVENT, onKeyboardInteractiveEvent);
  }
  // 统一会话状态监听：Rust emit 后更新 session.status（权威源，乐观更新外
  // 的确认/补充；disconnected 让侧栏圆点等派生 UI 即时变灰）。
  if (!statusUnlisten) {
    statusUnlisten = await listenBackendEvent(SESSION_STATUS_EVENT, event => {
      const { session_id, status } = (event.payload || {}) as { session_id?: string; status?: string };
      if (!session_id || !status) return;
      // 按真实 session_id 匹配；connected 事件早于 pending→真实 id 切换到达时
      // 无法命中（此时 connectSelected 的乐观更新已先行设置 status，无需补刀）。
      const session = ec().sessions.value.find(s => s.sessionId === session_id);
      if (session) session.status = status;
    });
  }
}

export async function disposeEventListeners() {
  if (hostKeyTimeout) { clearTimeout(hostKeyTimeout); hostKeyTimeout = null; }
  if (typeof hostKeyUnlisten === 'function') { await hostKeyUnlisten(); hostKeyUnlisten = null; }
  if (typeof keyboardUnlisten === 'function') { await keyboardUnlisten(); keyboardUnlisten = null; }
  if (typeof statusUnlisten === 'function') { await statusUnlisten(); statusUnlisten = null; }
}

// ============================================================

// ============================================================
// Host key / keyboard resolve
// ============================================================
export async function resolveHostKeyPrompt(requestId: string, accepted: boolean) {
  try {
    await invokeBackend('ssh_confirm_host_key', { requestId, accepted });
  } catch (error) {
    ec().announce('主机密钥响应失败：' + errorMessage(error));
  }
  ec().hostKeyPrompt.value = null;
  if (!accepted) ec().setModal({ type: null, asset: null });
}

export async function resolveKeyboardPrompt(requestId: string, responses: string[]) {
  try {
    await invokeBackend('ssh_keyboard_response', { requestId, responses });
  } catch (error) {
    ec().announce('键盘交互响应失败：' + errorMessage(error));
  }
  ec().keyboardPrompt.value = null;
}
