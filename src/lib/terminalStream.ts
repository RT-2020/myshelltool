/**
 * terminalStream — SSH 连接建立与输出流接线（ssh_connect / ssh-output /
 * ssh-closed 监听 / 自动重连调度 / bash cwd 集成注入），从 sessions store
 * 按域拆出。会话条目与终端实例由 store / terminalLifecycle 持有，本模块
 * 只做「连接 + 流」：attachSessionStream（首连/手动/自动重连共用）、
 * registerSessionStream（每次连接建立时重建监听与解码器）、
 * scheduleSessionReconnect（仅远端异常关闭触发，稳定窗口计数见
 * useAutoReconnect 头注释）。
 *
 * context 传参（先例 lib/modalSubmit）：store 在 setup 组装一次 ctx。
 */
import type { Ref } from 'vue';
import type { NormalizedConnectionAsset, NotifyOptions, SshConnectResult } from '@/types/domain';
import { invokeBackend, listenBackendEvent } from '@/services/backend';
import { errorMessage } from '@/lib/errorMessage';
import { createOscParser } from '@/lib/oscParser';
import type { SessionEntry } from '@/lib/terminalTypes';

let boundCtx: TerminalStreamContext | null = null;

/** 绑定 store 上下文（sessions store setup 时调用一次；绑定式理由见 terminalLifecycle）。 */
export function bindTerminalStreamContext(ctx: TerminalStreamContext): void {
  boundCtx = ctx;
}

function sc(): TerminalStreamContext {
  if (!boundCtx) throw new Error('terminalStream 未绑定 context（sessions store 未初始化）');
  return boundCtx;
}

export interface TerminalStreamContext {
  sessions: Ref<SessionEntry[]>;
  activeSessionId: Ref<string | null>;
  /** 全量资产（重连前按 id 重解析最新认证参数） */
  getAssets(): NormalizedConnectionAsset[] | null;
  onSessionConnected(assetId?: string | null): void;
  onSessionClosed(assetId?: string | null): void;
  syncTerminalCwd(assetId?: string | null, path?: string): void;
  announce(message: string, opts?: NotifyOptions): void;
  showOnlyActiveTerminal(): void;
}

// 对已有 session 建立 SSH 连接 + 事件接线（首次连接 / 自动重连 / 手动重连共用）。
// 复用 termDiv/term 不销毁；失败返回 false 并置 status='error' + connectError。
export async function attachSessionStream(session: SessionEntry) {
  const ctx = sc();
  // saveAsset 成功后会整体重建 assets 数组（新对象），session.asset 是建连时
  // 捕获的旧快照——重连（reauth 换密码 / 编辑连接后重试 / 自动重连）前按 id
  // 重新解析最新资产，否则仍会用旧认证参数连接（如已切 Password 仍走 PrivateKey）。
  const latestAssets = ctx.getAssets();
  const freshAsset = Array.isArray(latestAssets)
    ? latestAssets.find(item => item && item.id === session.asset.id)
    : null;
  if (freshAsset) session.asset = freshAsset;
  const asset = session.asset;
  // 清理上一次连接的监听（重连时旧 ssh-output-/ssh-closed- 监听必须先解绑）。
  // unlisten 已 async 化：await 完成，rejection 吞掉不阻断重连。
  if (typeof session.unlisten === 'function') {
    try { await session.unlisten().catch(() => null); } catch (_) { /* noop */ }
    session.unlisten = null;
  }
  const prevRealId = String(session.sessionId).startsWith('pending-') ? null : session.sessionId;
  // 覆盖 sessionId 前记录旧值：重连场景 activeSessionId 仍指向旧 id，成功后需
  // 据此恢复激活态，否则 activeSession 严格 find 落空 → 终端区空白/错误卡片消失。
  const previousSessionId = session.sessionId;
  session.sessionId = 'pending-' + asset.id + '-' + Date.now();
  if (prevRealId) await invokeBackend('ssh_disconnect', { sessionId: prevRealId }).catch(() => null);
  session.status = 'connecting';
  session.connectError = null;
  const cols = session.term.cols && session.term.cols > 0 ? session.term.cols : 80;
  const rows = session.term.rows && session.term.rows > 0 ? session.term.rows : 24;
  let realSessionId: string | null = null;
  try {
    const result = await invokeBackend<SshConnectResult>('ssh_connect', {
      host: asset.host,
      port: asset.port,
      username: asset.username,
      password: '',
      credentialId: asset.credential_id || null,
      authMethod: asset.auth_method,
      privateKeyPath: asset.private_key_path,
      passphrase: null,
      passphraseCredentialId: asset.passphrase_credential_id || null,
      privateKeyCredentialId: asset.private_key_credential_id || null,
      cols,
      rows
    });
    if (!result.connected) {
      // 失败保留 session：status='error' + connectError，错误卡片展示后可重试/编辑/关闭
      session.status = 'error';
      session.connectError = result.error || 'unknown';
      session.term.writeln('\x1b[31mConnection failed: ' + (result.error || 'unknown') + '\x1b[0m\r\n');
      ctx.announce('连接失败：' + asset.name + (result.error ? '（' + result.error + '）' : ''), { level: 'error' });
      return false;
    }
    realSessionId = result.session_id;
    // 竞态守卫：连接期间会话可能已被 cancelConnect/removeSessionEntry 移除，
    // 成功后立即断开新会话，避免后端遗留孤儿 SSH 会话（无监听器可清理）。
    if (!ctx.sessions.value.includes(session)) {
      await invokeBackend('ssh_disconnect', { sessionId: realSessionId }).catch(() => null);
      return false;
    }
    const wasActive = ctx.activeSessionId.value === previousSessionId;
    session.sessionId = realSessionId;
    if (wasActive) ctx.activeSessionId.value = realSessionId;
    session.status = 'connected';
    await registerSessionStream(session);
    ctx.showOnlyActiveTerminal();
    // 通知文件面板：该资产已连接，空态时自动加载远程目录
    try {
      ctx.onSessionConnected(session.asset?.id);
    } catch (_) { /* noop */ }
    // 注入 bash cwd 上报（OSC 7），使文件面板跟随终端 cd
    injectShellCwdIntegration(session);
    // 更新对应 asset 的 last_connected（纯显示元数据，无竞态——不写 status，
    // 运行时连接态一律从 session.status 派生）。workbench bridge 暴露
    // assets() 返回 assetsStore.assets 数组。
    try {
      const assetsList = ctx.getAssets();
      const target = Array.isArray(assetsList) ? assetsList.find(a => a && a.id === asset.id) : null;
      if (target) target.last_connected = new Date().toLocaleString('zh-CN');
    } catch (_) { /* 显示元数据更新失败不影响连接 */ }
    ctx.announce('已连接：' + asset.name, { level: 'success' });
    return true;
  } catch (error) {
    if (realSessionId) await invokeBackend('ssh_disconnect', { sessionId: realSessionId }).catch(() => null);
    session.status = 'error';
    session.connectError = errorMessage(error);
    session.term.writeln('\x1b[31mError: ' + errorMessage(error) + '\x1b[0m\r\n');
    ctx.announce('连接失败：' + errorMessage(error), { level: 'error' });
    return false;
  }
}

// 注册 output / closed 监听（每次连接建立时调用；decoder/oscParser 新建，
// 避免上一次连接遗留的流式解码状态污染新连接）。
export async function registerSessionStream(session: SessionEntry) {
  const ctx = sc();
  const { asset, term } = session;
  const realSessionId = session.sessionId;
  const decoder = new TextDecoder('utf-8', { stream: true } as unknown as TextDecoderOptions);
  session.decoder = decoder;
  const oscParser = createOscParser(
    title => { session.oscTitle = title; },
    cwd => {
      // 终端 cd → 文件面板跟随（仅当面板展示的就是本会话资产时生效）
      try {
        ctx.syncTerminalCwd(session.asset?.id, cwd);
      } catch (_) { /* noop */ }
    }
  );
  const outputUnlisten = await listenBackendEvent('ssh-output-' + realSessionId, event => {
    const data = event.payload as number[] | null | undefined;
    if (data && data.length > 0) {
      const text = decoder.decode(new Uint8Array(data));
      oscParser.feed(text);
      term.write(text);
    }
  });
  const closedUnlisten = await listenBackendEvent('ssh-closed-' + realSessionId, event => {
    const reason = typeof event.payload === 'string' && event.payload ? event.payload : 'unknown';
    session.status = 'disconnected';
    term.writeln('\r\n\x1b[31m[myshelltool] 远程连接已关闭 (' + reason + ')。\x1b[0m');
    ctx.announce('远程连接已关闭：' + asset.name + '（' + reason + '）', { level: 'warn' });
    // 通知文件面板清空该资产的目录（自动重连成功后会自动重新加载）
    try {
      ctx.onSessionClosed(asset.id);
    } catch (_) { /* noop */ }
    scheduleSessionReconnect(session, reason);
  });
  // 组合句柄 async 化：跨窗口迁移路径需 await 两个 unlisten 完成后再移除
  // session（防旧窗口残留监听造成双写）；现有调用方不 await 也不破坏。
  session.unlisten = async () => {
    await Promise.all([outputUnlisten(), closedUnlisten()]);
  };
}

// 自动重连调度：仅对远端异常关闭（非 disconnected-by-user）触发；复用同一 session
// 重走 ssh_connect。manualDisconnect / 会话已移除双守卫防与 cancelConnect 竞态。
export function scheduleSessionReconnect(session: SessionEntry, reason: string) {
  const ctx = sc();
  if (session.manualDisconnect || !session.autoReconnect) return;
  // 局部捕获：闭包内 TS 不继承外层可选链窄化（同一对象引用，行为不变）
  const autoReconnect = session.autoReconnect;
  if (reason === 'disconnected-by-user') return;
  const doReconnect = async () => {
    if (session.manualDisconnect || !ctx.sessions.value.includes(session)) return;
    const ok = await attachSessionStream(session);
    if (ok) {
      // v2.6：**不立即清零计数**。建连成功 ≠ 会话能维持——认证通过但随即被关的
      // 服务器（nologin / shell 立即退出 / 建连后数秒断网）会让「成功→清零→又断」
      // 无限循环（1s 一次登录风暴）。改为启动稳定窗口：窗口内不再断开才归零，
      // 窗口内又断则计数保留、退避继续往后走，4 次后如实报「请手动重连」。
      const windowMs = autoReconnect.markConnected();
      session.reconnectAttempt = 0;
      ctx.announce('已恢复连接：' + session.asset.name, { level: 'success' });
      // 稳定窗口结束后再确认一次（用户能据此区分「真的稳了」与「又掉了」）
      setTimeout(() => {
        if (session.manualDisconnect || !ctx.sessions.value.includes(session)) return;
        if (session.status === 'connected') {
          ctx.announce('连接已稳定：' + session.asset.name, { level: 'success' });
        }
      }, windowMs);
    } else if (!session.manualDisconnect && ctx.sessions.value.includes(session)) {
      // 本次失败 → 排队下一次尝试（onAttempt 更新计数，4 次后 onExhausted）
      autoReconnect.schedule(doReconnect);
    }
  };
  autoReconnect.schedule(doReconnect);
}

function injectShellCwdIntegration(session: SessionEntry) {
  const encoder = new TextEncoder();
  const mainLine =
    " eval 'if [ -n \"$BASH_VERSION\" ]; then" +
    ' __mt_cwd(){ __mt_p="${PWD//%/%25}"; printf \'\\\'\'\\033]7;file://%s\\007\'\\\'\' "${__mt_p// /%20}"; };' +
    ' case ";$PROMPT_COMMAND;" in *"__mt_cwd"*) ;; *) PROMPT_COMMAND="__mt_cwd;$PROMPT_COMMAND";; esac;' +
    ' __mt_cwd; fi\'; stty echo; test -n "$BASH_VERSION" && printf \'%b\' \'\\e[1A\\e[G\\e[J\'';
  invokeBackend('ssh_write', {
    sessionId: session.sessionId,
    data: Array.from(encoder.encode(mainLine + '\n'))
  }).catch(() => null);
}
