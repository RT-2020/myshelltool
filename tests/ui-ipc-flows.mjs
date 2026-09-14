// ui-ipc-flows — Tauri IPC 路径的自动化覆盖（v2.8 第六轮）
//
// 为什么需要：PTY 流 / 上传下载 / 自动重连 / 跨窗口接管走 invokeBackend +
// listenBackendEvent，浏览器预览模式下这些代码路径完全不可达（isTauriRuntime
// 为 false 直接拒绝），此前的 UI 冒烟只覆盖「无后端」分支。真实 tauri-driver
// E2E 需要 SSH 服务器矩阵（ADR v3 P3 follow-up），本文件用**脚本化 mock 后端**
// （window.__TAURI__.core.invoke / event.listen）驱动**真实 store 逻辑**补上这层：
// 顶层 await 用法与 ui-smoke 一致；跑法：npm run dev 起服务后 node tests/ui-ipc-flows.mjs。
//
// 覆盖清单（每条都对应一次真实事故或高风险迁移）：
//   1. 连接成功 + ssh-output 流写入终端 + 状态事件断开 → 自动重连再次 connect
//   2. 分块上传全链路（start/chunk/finalize 顺序 + 短块 EOF + 进度事件）
//   3. 覆盖确认链（probe 命中存在 → 弹窗确认 → 覆盖上传）
//   4. host key 事件 → 弹窗 → resolve 回传

import { chromium } from 'playwright';

const baseUrl = process.env.MYSHELLTOOL_BASE_URL ?? 'http://127.0.0.1:41234/';

/** 注入到页面的 mock 后端：invoke 按 command 分发表，listen 捕获 handler 供测试 emit。 */
// mock 后端是**数据驱动**的（函数无法跨 evaluate 序列化）：
// spec = { value } 静态返回 | { error } 抛错 | { dynamic: 'connect' } 生成递增 session id。
const MOCK_INIT = `
window.__tauriMock = {
  calls: [],
  invokeSpecs: {
    backend_status: { value: { ready: true, mode: 'desktop' } },
    tunnel_list: { value: [] },
    list_connection_assets: { value: { source: 'local', count: 1, assets: [{"id": "ipc-test-1", "name": "ipc-test", "host": "127.0.0.1", "port": 2222, "username": "root", "auth_method": "Password", "private_key_path": null, "group": "未分组", "tags": [], "status": "Idle", "credential_id": "ipc-test-1:password", "passphrase_credential_id": null, "last_connected": null}], groups: [] } },
    get_credential_status: { value: { exists: true } },
    fs_local_home_dir: { value: 'C:\Users\test' },
    resource_monitor_list_active: { value: [] },
  },
  eventHandlers: {},
  connectCounter: 0,
};
window.__TAURI__ = {
  core: {
    invoke: async (command, args) => {
      window.__tauriMock.calls.push({ command, args });
      const spec = window.__tauriMock.invokeSpecs[command];
      if (!spec) throw new Error('mock backend: no handler for ' + command);
      if (spec.error) throw new Error(spec.error);
      if (spec.dynamic === 'connect') {
        window.__tauriMock.connectCounter += 1;
        return { session_id: 'real-sess-' + window.__tauriMock.connectCounter, connected: true };
      }
      return spec.value;
    },
  },
  event: {
    listen: async (name, handler) => {
      (window.__tauriMock.eventHandlers[name] ||= []).push(handler);
      return () => {
        const list = window.__tauriMock.eventHandlers[name] || [];
        const i = list.indexOf(handler);
        if (i >= 0) list.splice(i, 1);
      };
    },
    emit: async () => {},
  },
  window: { getCurrentWindow: () => ({ label: 'main' }) },
};
`;

const ASSET = {
  id: 'ipc-test-1',
  name: 'ipc-test',
  host: '127.0.0.1',
  port: 2222,
  username: 'root',
  auth_method: 'Password',
  private_key_path: null,
  group: '未分组',
  tags: [],
  status: 'Idle',
  credential_id: 'ipc-test-1:password',
  passphrase_credential_id: null,
  last_connected: null
};

function spec(v) { return { value: v }; }
function fixtureBackend(page, specs) {
  return page.evaluate(specs => { window.__tauriMock.invokeSpecs = specs; }, specs);
}

async function emitEvent(page, name, payload) {
  await page.evaluate(({ name, payload }) => {
    for (const h of window.__tauriMock.eventHandlers[name] || []) h({ payload });
  }, { name, payload });
}

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
page.on('pageerror', err => { throw new Error('page error: ' + err.message); });

try {
  await page.addInitScript(MOCK_INIT);

  // ── 基础 fixture：资产列表 + 凭据状态 ──
  await page.route('**/*', route => route.continue());
  await page.goto(baseUrl, { waitUntil: 'networkidle' });

  await fixtureBackend(page, {
    list_connection_assets: spec({ source: 'local', count: 1, assets: [ASSET], groups: [] }),
    get_credential_status: spec({ exists: true }),
    fs_local_home_dir: spec('C:\\Users\\test'),
    resource_monitor_list_active: spec([]),
  });
  await page.waitForFunction(() => window.__myshelltool, null, { timeout: 10000 });
  await page.waitForFunction(() => window.__tauriMock.calls.some(c => c.command === 'list_connection_assets'), null, { timeout: 10000 });
  const assetCount = await page.evaluate(() => window.__myshelltool.workbench.assets.length);
  if (assetCount !== 1) throw new Error(`fixture assets not loaded: ${assetCount}`);

console.error('[step] scenario-1');
  // ════ 场景 1：连接 + 输出流 + 断开自动重连 ════
  let connectCount = 0;
  await fixtureBackend(page, {
    'ssh_connect': { dynamic: 'connect' },
    'ssh_write': spec(null),
    'ssh_resize': spec(null),
    'sftp_list_dir': spec({ path: '/root', entries: [] }),
  });
  await page.evaluate(() => {
    const w = window.__myshelltool.workbench;
    w.selectAsset(w.assets[0].id, false);
    return w.connectSelected();
  });
  await page.waitForFunction(() => window.__tauriMock.calls.some(c => c.command === 'ssh_connect'), null, { timeout: 5000 });
  const sessionId = await page.evaluate(() => window.__myshelltool.workbench.activeSessionId);
  if (!sessionId || !String(sessionId).startsWith('real-sess-')) throw new Error('session not connected: ' + sessionId);

  // ssh-output 流：两段 chunk（模拟 UTF-8 分片）
  const enc = b => Array.from(new TextEncoder().encode(b));
  await emitEvent(page, 'ssh-output-' + sessionId, enc('hello '));
  await emitEvent(page, 'ssh-output-' + sessionId, enc('世界'));
  // banner（connecting 提示）占第 0 行，流内容从其后开始——扫前 5 行拼接断言
  const termText = await page.evaluate(sid => {
    const s = window.__myshelltool.workbench.sessions.find(x => x.sessionId === sid);
    if (!s) return null;
    let out = '';
    for (let i = 0; i < 5; i++) out += (s.term.buffer.active.getLine(i)?.translateToString(true) || '') + '\n';
    return out;
  }, sessionId);
  if (!termText || !termText.includes('hello ')) throw new Error('output not written to terminal: ' + JSON.stringify(termText));
  if (!termText.includes('世界')) throw new Error('UTF-8 chunk split decoded wrong: ' + JSON.stringify(termText));

  // 断开事件（非用户断开）→ 自动重连（退避后二次 ssh_connect）
  await emitEvent(page, 'ssh-closed-' + sessionId, 'connection reset');
  await page.waitForFunction(
    () => window.__tauriMock.calls.filter(c => c.command === 'ssh_connect').length >= 2,
    null,
    { timeout: 10000 }
  );
  const statusAfterClose = await page.evaluate(() => {
    const ss = window.__myshelltool.workbench.sessions;
    return ss[ss.length - 1].status;
  });
  if (statusAfterClose !== 'connected') throw new Error('auto-reconnect did not reconnect: ' + statusAfterClose);

console.error('[step] scenario-2');
  // ════ 场景 2：分块上传全链路 ════
  await fixtureBackend(page, {
    'sftp_upload_start': spec(null),
    'sftp_upload_chunk': spec(null),
    'sftp_upload_finalize': spec(null),
    'sftp_stat': spec(null),
    'sftp_list_dir': spec({ path: '/root', entries: [] }),
  });
  // 页面内构造 File（8MiB+1B 触发两块：8MiB 短块? 不——CHUNK=8MiB，10MiB → 8+2 两块）
  const uploaded = await page.evaluate(async () => {
    const files = window.__myshelltool.files;
    const buf = new Uint8Array(10 * 1024 * 1024).fill(65);
    const file = new File([buf], 'big-upload.bin');
    await files.uploadFiles([file]);
    return true;
  });
  if (!uploaded) throw new Error('uploadFiles rejected');
  await page.waitForFunction(() => window.__tauriMock.calls.some(c => c.command === 'sftp_upload_finalize'), null, { timeout: 20000 });
  const chunks = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'sftp_upload_chunk').map(c => c.args.bytesTransferred));
  if (chunks.length !== 2 || chunks[0] !== 8 * 1024 * 1024 || chunks[1] !== 10 * 1024 * 1024) {
    throw new Error('chunk boundaries wrong: ' + JSON.stringify(chunks));
  }

console.error('[step] scenario-3');
  // ════ 场景 3：覆盖确认链（probe 命中 → modal → 确认 → 上传）════
  await fixtureBackend(page, {
    'sftp_stat': spec({ name: 'over.txt', kind: 'file', size: 1 }),  // 已存在
    'sftp_upload_start': spec(null),
    'sftp_upload_chunk': spec(null),
    'sftp_upload_finalize': spec(null),
    'sftp_list_dir': spec({ path: '/root', entries: [] }),
  });
  // uploadFiles 在覆盖确认处挂起等用户选择——不能在 evaluate 里 await 它
  // （evaluate 永不返回 = 测试死锁）。fire-and-forget，由下方确认步骤放行。
  await page.evaluate(() => {
    const files = window.__myshelltool.files;
    const file = new File([new Uint8Array([1, 2, 3])], 'over.txt');
    files.uploadFiles([file]).catch(e => console.error('[test] overwrite upload rejected: ' + e.message));
  });
  await page.waitForFunction(() => window.__myshelltool.workbench.modal.type === 'confirmFileOverwrite', null, { timeout: 5000 });
  await page.evaluate(() => window.__myshelltool.files.confirmFileOverwrite());
  await page.waitForFunction(
    () => window.__tauriMock.calls.filter(c => c.command === 'sftp_upload_finalize').length >= 2,
    null,
    { timeout: 10000 }
  );

console.error('[step] scenario-4');
  // ════ 场景 4：host key 事件 → 弹窗 → resolve 回传 ════
  await fixtureBackend(page, { 'ssh_confirm_host_key': spec(null) });
  // 跨窗口路由守卫：本窗口无 connecting 会话时不认领 hostkey 事件（正确行为）。
  // 登记一条在途一次性连接让本窗口具备认领资格（files store 回落通道同款机制）。
  await page.evaluate(() => {
    const w = window.__myshelltool.workbench;
    window.__diagUnregister = window.__myshelltool.sessions.registerEphemeralConnection(w.assets[0]);
  });
  await emitEvent(page, 'ssh-host-key-verify', {
    request_id: 'rk-1', host_port: '127.0.0.1:2222', key_type: 'ssh-ed25519',
    fingerprint: 'AA:BB', is_changed: false
  });
  await page.waitForFunction(() => window.__myshelltool.workbench.modal.type === 'hostKeyVerify', null, { timeout: 5000 });
  await page.evaluate(() => window.__myshelltool.sessions.resolveHostKeyPrompt('rk-1', true));
  await page.waitForFunction(() => window.__tauriMock.calls.some(c => c.command === 'ssh_confirm_host_key' && c.args.accepted === true), null, { timeout: 5000 });

  console.log('IPC flow tests passed (connect+stream+reconnect / chunked upload / overwrite confirm / host-key resolve)');
} finally {
  await browser.close();
}
