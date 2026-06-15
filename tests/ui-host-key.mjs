// UI host key 验证流程自动化测试
//
// 通过 Playwright + mock window.__TAURI__ 验证：
// 1. App 启动后 host key listener 被注册
// 2. backend emit 'ssh-host-key-verify' 事件 → handler 触发 → modal 显示
// 3. 用户点"确认" → invoke('ssh_confirm_host_key', {requestId, accepted:true}) 被调用
// 4. 用户点"拒绝" → invoke('ssh_confirm_host_key', {requestId, accepted:false}) 被调用
//
// 不需要真实 SSH 服务器——mock Tauri runtime 捕获 invoke/listen 调用即可。

import { chromium } from 'playwright';

const baseUrl = process.env.MYSHELLTOOL_BASE_URL ?? 'http://127.0.0.1:41234/';

const browser = await chromium.launch({ headless: true });
const failures = [];

function assert(condition, message) {
  if (!condition) failures.push(message);
}

try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });

  await page.addInitScript(() => {
    window.__MST_MOCK = {
      listeners: new Map(),
      invokeCalls: [],
    };
    window.__TAURI__ = {
      core: {
        invoke: (cmd, args) => {
          window.__MST_MOCK.invokeCalls.push({ cmd, args });
          if (cmd === 'ssh_connect') {
            return Promise.resolve({ session_id: '', connected: false, error: 'mock' });
          }
          if (cmd === 'backend_status') return Promise.resolve({ ready: true, mode: 'tauri-core' });
          if (cmd === 'list_connection_assets') return Promise.resolve({ source: 'mock', count: 0, assets: [] });
          return Promise.resolve(null);
        },
      },
      event: {
        listen: (eventName, handler) => {
          window.__MST_MOCK.listeners.set(eventName, handler);
          return Promise.resolve(() => window.__MST_MOCK.listeners.delete(eventName));
        },
      },
      webviewWindow: { getCurrentWebviewWindow: () => ({ listen: undefined }) },
      window: { getCurrentWindow: () => ({ listen: undefined }) },
    };
  });

  await page.goto(baseUrl, { waitUntil: 'networkidle' });

  // 1. listener 注册（轮询 6s）
  let registered = false;
  for (let i = 0; i < 30; i++) {
    registered = await page.evaluate(() => window.__MST_MOCK?.listeners?.has('ssh-host-key-verify') ?? false);
    if (registered) break;
    await new Promise(r => setTimeout(r, 200));
  }
  if (!registered) {
    const state = await page.evaluate(() => ({
      listeners: Array.from(window.__MST_MOCK?.listeners?.keys() ?? []),
      hasTauri: !!window.__TAURI__,
      hasMock: !!window.__MST_MOCK,
    }));
    throw new Error(`listener NOT registered after 6s. State: ${JSON.stringify(state)}`);
  }

  // 2. 触发 host key 事件
  await page.evaluate(() => {
    const handler = window.__MST_MOCK.listeners.get('ssh-host-key-verify');
    handler({
      payload: {
        request_id: 'test-req-001',
        host_port: '192.168.2.2:22',
        key_type: 'ssh-ed25519',
        fingerprint: 'SHA256:abc',
        is_changed: false,
      },
    });
  });

  // 3. modal 打开
  await page.waitForSelector('#modalLayer.open', { timeout: 5000 });
  const modalText = await page.locator('#modalBody').textContent();
  assert(
    modalText?.includes('192.168.2.2:22') || modalText?.includes('主机密钥'),
    `host key modal should show host info`
  );

  // 4. 点确认（modalPrimary）
  const beforeAccept = await page.evaluate(() => window.__MST_MOCK.invokeCalls.length);
  await page.click('#modalPrimary');
  for (let i = 0; i < 25; i++) {
    const cnt = await page.evaluate(() => window.__MST_MOCK.invokeCalls.length);
    if (cnt > beforeAccept) break;
    await new Promise(r => setTimeout(r, 200));
  }
  const acceptCall = await page.evaluate(() => {
    const calls = window.__MST_MOCK.invokeCalls;
    return calls[calls.length - 1];
  });
  assert(
    acceptCall?.cmd === 'ssh_confirm_host_key' && acceptCall?.args?.accepted === true,
    `accept → ssh_confirm_host_key accepted:true (got: ${JSON.stringify(acceptCall)})`
  );

  // 5. 再次触发并点拒绝（.btn.danger）
  await page.evaluate(() => {
    const handler = window.__MST_MOCK.listeners.get('ssh-host-key-verify');
    handler({
      payload: {
        request_id: 'test-req-002',
        host_port: '192.168.2.2:22',
        key_type: 'ssh-ed25519',
        fingerprint: 'SHA256:abc',
        is_changed: false,
      },
    });
  });
  await page.waitForFunction(
    () => {
      const app = document.querySelector('#app').__vue_app__;
      const pinia = app.config.globalProperties.$pinia;
      const store = pinia._s.get('workbench');
      return store?.modal?.type === 'hostKeyVerify';
    },
    { timeout: 5000 }
  );
  const beforeReject = await page.evaluate(() => window.__MST_MOCK.invokeCalls.length);
  await page.click('.modal-actions .btn.danger');
  await page.waitForTimeout(500);
  const afterReject = await page.evaluate(() => window.__MST_MOCK.invokeCalls.length);
  assert(afterReject > beforeReject, `reject click should trigger invoke (${beforeReject} → ${afterReject})`);
  const rejectCall = await page.evaluate(() => {
    const calls = window.__MST_MOCK.invokeCalls;
    return calls[calls.length - 1];
  });
  assert(
    rejectCall?.cmd === 'ssh_confirm_host_key' && rejectCall?.args?.accepted === false,
    `reject → ssh_confirm_host_key accepted:false (got: ${JSON.stringify(rejectCall)})`
  );

  console.log(
    failures.length === 0
      ? 'Host key UI test passed'
      : `Host key UI test failures:\n - ${failures.join('\n - ')}`
  );
} catch (error) {
  failures.push('Exception: ' + error.message);
  console.log('Host key UI test failed:', error.message);
} finally {
  await browser.close();
}

process.exit(failures.length === 0 ? 0 : 1);
