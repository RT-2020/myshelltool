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
//   2. 流式上传全链路（fs_local_stat → 单条 sftp_upload_from_file → 队列收敛 done）
//   2b. 上传取消链（cancelTransfer → sftp_upload_cancel → 后端中止 reject → 队列 cancelled）
//   2c. 上传收尾刷新当前面板目录（深层目录上传后面板不跳回服务器默认目录/家目录）
//   2d. UploadProgressStrip：自动出现/展开详情/关闭不取消/新批次重现/独立取消+重试入口
//   3. 覆盖确认链（probe 命中存在 → 弹窗确认 → 覆盖上传）
//   3b/3c/3d. 冲突三选（B2）：重命名（撞名报错+换名改路径）/ 跳过 / 应用到剩余
//   3e/3f/3g. 目录整传（B2）：目录上传（mkdir+子目录路径+新建目录跳过探测）/
//   目录下载（本地建目录+落点）/ 目录批次取消（当前中止+剩余不入队）
//   3h. 上传完成 toast 聚合（B2 后续：多文件合并一条 + 单文件原文案）
//   3i/3j. 并发专项（B2 并发修复）：同名双写防护（同目标互斥链：第二次探测
//   必在第一次落盘后 + 冲突窗拦截）/ 并发上限生效（在途峰值 ≤ 配置）+
//   单项失败不炸批（failed 如实计数）
//   4. host key 事件 → 弹窗 → resolve 回传
//   5. 资产编辑器直输新分组（AppCombobox）：菜单「新建」提示行 + blur 提交 +
//      save_connection_asset 收到完整多级路径（后端 upsert 收口分组声明）

import { chromium } from 'playwright';

const baseUrl = process.env.MYSHELLTOOL_BASE_URL ?? 'http://127.0.0.1:41234/';

/** 注入到页面的 mock 后端：invoke 按 command 分发表，listen 捕获 handler 供测试 emit。 */
// mock 后端是**数据驱动**的（函数无法跨 evaluate 序列化）：
// spec = { value } 静态返回 | { error } 抛错 | { dynamic: 'connect' } 生成递增 session id
//      | { dynamic: 'upload-gate' } 挂起上传直到被取消 | { dynamic: 'cancel-upload' } 解除挂起并 reject。
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
  uploadGates: {},
  dirMap: {},
  statMap: {},
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
      // 上传闸门：模拟「在途大文件上传」——invoke 挂起，直到 sftp_upload_cancel 解除
      if (spec.dynamic === 'upload-gate') {
        return new Promise((resolve, reject) => {
          window.__tauriMock.uploadGates[args.transferId] = { resolve, reject };
        });
      }
      if (spec.dynamic === 'cancel-upload') {
        const gate = window.__tauriMock.uploadGates[args.transferId];
        if (gate) {
          delete window.__tauriMock.uploadGates[args.transferId];
          gate.reject(new Error('upload cancelled by user'));
        }
        return null;
      }
      // 目录列表回显：请求哪个目录就返回哪个目录（模拟后端真实语义——
      // sftp_list_dir 的 result.path 是服务器解析后的绝对路径）。
      if (spec.dynamic === 'list-echo') {
        return { path: args.path || '/root', entries: [] };
      }
      // B2 目录整传：按路径映射返回目录内容（dirMap[path] = entries）。
      if (spec.dynamic === 'dir-map') {
        const map = window.__tauriMock.dirMap || {};
        return { path: args.path || '', entries: map[args.path] || [] };
      }
      // B2 冲突三选探测：按路径映射 stat 结果（undefined = null = 不存在，
      // 'error' = 抛错模拟无法确认）。
      if (spec.dynamic === 'stat-map') {
        const map = window.__tauriMock.statMap || {};
        const hit = map[args.path];
        if (hit === 'error') throw new Error('stat failed (mock): ' + args.path);
        return hit === undefined ? null : hit;
      }
      // B2 并发测试（3i 同名双写防护）：上传即落盘——upload invoke 把
      // remotePath 记入 statMap，模拟 SFTP create(O_TRUNC) 的「上传开始目标
      // 即存在」语义（半截文件 stat 也命中，与真实后端一致）。
      if (spec.dynamic === 'upload-mark') {
        (window.__tauriMock.statMap ||= {})[args.remotePath] = {
          name: String(args.remotePath).split('/').pop(), kind: 'file', size: 1
        };
        return null;
      }
      // B2 并发测试（3j 并发上限）：延迟 resolve 的慢速上传，记录在途并发峰值。
      if (spec.dynamic === 'upload-slow') {
        window.__tauriMock.uploadInFlight = (window.__tauriMock.uploadInFlight || 0) + 1;
        window.__tauriMock.uploadPeak = Math.max(window.__tauriMock.uploadPeak || 0, window.__tauriMock.uploadInFlight);
        return new Promise(resolve => setTimeout(() => {
          window.__tauriMock.uploadInFlight -= 1;
          resolve(null);
        }, 80));
      }
      // B2 并发测试（3j 池吞错不炸批）：按 remotePath 注定的失败上传。
      if (spec.dynamic === 'upload-fail-marked') {
        if (String(args.remotePath).includes('boom')) throw new Error('mock upload failure for ' + args.remotePath);
        return null;
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
  // ════ 场景 2：流式上传全链路（stat → 单条 sftp_upload_from_file → done）════
  await fixtureBackend(page, {
    'fs_local_stat': spec({ name: 'big-upload.bin', path: 'C:\\fake\\big-upload.bin', kind: 'file', size: 10 * 1024 * 1024, modified: '', permissions: null, user: null, group: null }),
    'sftp_upload_from_file': spec(null),
    'sftp_stat': spec(null), // 目标不存在 → 不弹覆盖确认
    'sftp_list_dir': spec({ path: '/root', entries: [] }),
  });
  const uploaded = await page.evaluate(async () => {
    await window.__myshelltool.files.uploadLocalPaths(['C:\\fake\\big-upload.bin']);
    return true;
  });
  if (!uploaded) throw new Error('uploadLocalPaths rejected');
  await page.waitForFunction(() => window.__tauriMock.calls.some(c => c.command === 'sftp_upload_from_file'), null, { timeout: 20000 });
  const upCalls = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'sftp_upload_from_file'));
  if (upCalls.length !== 1) throw new Error('expected exactly one streaming upload call, got ' + upCalls.length);
  const upArgs = upCalls[0].args;
  if (upArgs.localPath !== 'C:\\fake\\big-upload.bin') throw new Error('localPath wrong: ' + JSON.stringify(upArgs));
  if (upArgs.remotePath !== '/root/big-upload.bin') throw new Error('remotePath wrong: ' + JSON.stringify(upArgs));
  if (!upArgs.transferId || !String(upArgs.sessionId).startsWith('real-sess-')) throw new Error('upload args incomplete: ' + JSON.stringify(upArgs));
  // 字节不经 IPC：不应再出现任何分块命令
  const legacyChunks = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command.startsWith('sftp_upload_') && c.command !== 'sftp_upload_from_file' && c.command !== 'sftp_upload_cancel').length);
  if (legacyChunks !== 0) throw new Error('legacy chunked upload commands still called: ' + legacyChunks);
  // invoke 成功 + 无进度事件（mock 不发）→ 兜底路径也应把队列项收敛为 done
  const doneStatus = await page.evaluate(id => window.__myshelltool.files.transferQueue.find(i => i.id === id)?.status, upArgs.transferId);
  if (doneStatus !== 'done') throw new Error('upload queue item not done: ' + doneStatus);

console.error('[step] scenario-2b');
  // ════ 场景 2b：上传取消链（cancelTransfer → sftp_upload_cancel → cancelled）════
  await fixtureBackend(page, {
    'fs_local_stat': spec({ name: 'slow.bin', path: 'C:\\fake\\slow.bin', kind: 'file', size: 5 * 1024 * 1024, modified: '', permissions: null, user: null, group: null }),
    'sftp_upload_from_file': { dynamic: 'upload-gate' },
    'sftp_upload_cancel': { dynamic: 'cancel-upload' },
    'sftp_stat': spec(null),
    'sftp_list_dir': spec({ path: '/root', entries: [] }),
  });
  await page.evaluate(() => {
    window.__myshelltool.files.uploadLocalPaths(['C:\\fake\\slow.bin']).catch(e => console.error('[test] cancelled upload rejected: ' + e.message));
  });
  await page.waitForFunction(() => window.__tauriMock.calls.some(c => c.command === 'sftp_upload_from_file' && String(c.args.localPath).includes('slow')), null, { timeout: 5000 });
  const slowTransferId = await page.evaluate(() => window.__tauriMock.calls.find(c => c.command === 'sftp_upload_from_file' && String(c.args.localPath).includes('slow')).args.transferId);
  await page.evaluate(id => window.__myshelltool.files.cancelTransfer(id), slowTransferId);
  await page.waitForFunction(id => window.__myshelltool.files.transferQueue.find(i => i.id === id)?.status === 'cancelled', slowTransferId, { timeout: 5000 });
  const cancelCallCount = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'sftp_upload_cancel').length);
  if (cancelCallCount !== 1) throw new Error('sftp_upload_cancel not called exactly once: ' + cancelCallCount);

console.error('[step] scenario-2c');
  // ════ 场景 2c：上传批次收尾刷新「当前面板目录」，绝不跳回服务器默认目录 ════
  // 回归（2026-09-21 用户视频实测）：修复前批次收尾 refreshRemoteFiles(null) 直通
  // 空 path，后端 canonicalize(".") 解析出登录家目录——用户在深层目录上传成功/
  // 失败后，面板都被拽回「第一次进服务器的路径」。同理波及刷新按钮/「刷新当前目录」
  // 右键/编辑器保存后刷新。断言三层：上传目标 = 深层目录；收尾刷新请求的 path =
  // 当前目录（非空串）；面板最终仍停在当前目录。
  await fixtureBackend(page, {
    'fs_local_stat': spec({ name: 'deep-upload.bin', path: 'C:\\fake\\deep-upload.bin', kind: 'file', size: 1024, modified: '', permissions: null, user: null, group: null }),
    'sftp_upload_from_file': spec(null),
    'sftp_stat': spec(null),
    'sftp_list_dir': { dynamic: 'list-echo' },
  });
  await page.evaluate(() => window.__myshelltool.files.navigateRemotePath('/var/log'));
  await page.waitForFunction(() => window.__myshelltool.files.remotePath === '/var/log', null, { timeout: 5000 });
  const listCountBeforeDeep = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'sftp_list_dir').length);
  await page.evaluate(() => window.__myshelltool.files.uploadLocalPaths(['C:\\fake\\deep-upload.bin']));
  await page.waitForFunction(
    () => window.__tauriMock.calls.some(c => c.command === 'sftp_upload_from_file' && c.args.remotePath === '/var/log/deep-upload.bin'),
    null,
    { timeout: 10000 }
  );
  await page.waitForFunction(count => window.__tauriMock.calls.filter(c => c.command === 'sftp_list_dir').length > count, listCountBeforeDeep, { timeout: 10000 });
  const deepUpload = await page.evaluate(() => window.__tauriMock.calls.find(c => c.command === 'sftp_upload_from_file' && String(c.args.remotePath).endsWith('deep-upload.bin'))?.args?.remotePath);
  if (deepUpload !== '/var/log/deep-upload.bin') throw new Error('deep upload target wrong: ' + deepUpload);
  const deepRefreshPaths = await page.evaluate(count => window.__tauriMock.calls.filter(c => c.command === 'sftp_list_dir').slice(count).map(c => c.args.path), listCountBeforeDeep);
  if (!deepRefreshPaths.length || !deepRefreshPaths.every(p => p === '/var/log')) {
    throw new Error('batch-end refresh left current dir (home-jump regression): ' + JSON.stringify(deepRefreshPaths));
  }
  const deepFinalPath = await page.evaluate(() => window.__myshelltool.files.remotePath);
  if (deepFinalPath !== '/var/log') throw new Error('panel jumped after upload: ' + deepFinalPath);

console.error('[step] scenario-2d');
  // ════ 场景 2d：UploadProgressStrip（上传区下方进度提示条）════
  // 验收链路：开始上传自动出现（收起态=状态+百分比+概要）→ 点主体展开详情
  // （文件名/大小/独立「取消上传」按钮）→ 点 ✕ 只关提示**不取消**（后台闸门
  // 仍挂起、无 sftp_upload_cancel 调用）→ 放行后队列在后台收敛 done → 新批次
  // 自动重现 → 行内取消按钮走 sftp_upload_cancel 且状态收敛 cancelled + 重试入口。
  await fixtureBackend(page, {
    'fs_local_stat': spec({ name: 'strip.bin', path: 'C:\\fake\\strip.bin', kind: 'file', size: 4096, modified: '', permissions: null, user: null, group: null }),
    'sftp_upload_from_file': { dynamic: 'upload-gate' },
    'sftp_upload_cancel': { dynamic: 'cancel-upload' },
    'sftp_stat': spec(null),
    'sftp_list_dir': { dynamic: 'list-echo' },
  });
  await page.evaluate(() => {
    window.__myshelltool.files.uploadLocalPaths(['C:\\fake\\strip.bin']).catch(e => console.error('[test] strip upload rejected: ' + e.message));
  });
  await page.waitForFunction(() => document.querySelector('[data-up-progress]'), null, { timeout: 5000 });
  const stripCollapsed = await page.evaluate(() => ({
    toggle: !!document.querySelector('[data-up-progress-toggle]'),
    close: !!document.querySelector('[data-up-progress-close]'),
    text: document.querySelector('[data-up-progress]')?.textContent || ''
  }));
  if (!stripCollapsed.toggle || !stripCollapsed.close) throw new Error('strip collapsed controls missing: ' + JSON.stringify(stripCollapsed));
  if (!stripCollapsed.text.includes('上传中') || !stripCollapsed.text.includes('strip.bin')) {
    throw new Error('strip collapsed state wrong: ' + stripCollapsed.text);
  }
  // 展开详情：aria-expanded 翻转 + 行内容（文件名/大小/取消按钮）
  await page.evaluate(() => document.querySelector('[data-up-progress-toggle]').click());
  await page.waitForFunction(() => document.querySelector('[data-up-progress-toggle]')?.getAttribute('aria-expanded') === 'true', null, { timeout: 3000 });
  const stripDetail = await page.evaluate(() => ({
    rows: document.querySelectorAll('#ups-detail .ups-row').length,
    text: document.getElementById('ups-detail')?.textContent || '',
    cancelBtn: !!document.querySelector('#ups-detail [aria-label^="取消上传"]')
  }));
  if (stripDetail.rows !== 1) throw new Error('strip detail rows wrong: ' + stripDetail.rows);
  if (!stripDetail.text.includes('4 KB') || !stripDetail.text.includes('strip.bin')) {
    throw new Error('strip detail content wrong: ' + stripDetail.text);
  }
  if (!stripDetail.cancelBtn) throw new Error('strip detail cancel button missing');
  // 关闭提示 ≠ 取消：闸门仍挂起、无新增 cancel 调用（calls 表跨场景累计，取差值）
  const cancelCallsBeforeClose = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'sftp_upload_cancel').length);
  await page.evaluate(() => document.querySelector('[data-up-progress-close]').click());
  await page.waitForFunction(() => !document.querySelector('[data-up-progress]'), null, { timeout: 3000 });
  const cancelCallsAfterClose = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'sftp_upload_cancel').length);
  if (cancelCallsAfterClose !== cancelCallsBeforeClose) throw new Error('closing hint triggered upload cancel: ' + (cancelCallsAfterClose - cancelCallsBeforeClose));
  await page.evaluate(() => {
    const gates = window.__tauriMock.uploadGates;
    const id = Object.keys(gates)[0];
    gates[id]?.resolve(null);
  });
  await page.waitForFunction(() => window.__myshelltool.files.transferQueue.some(i => i.name === 'strip.bin' && i.status === 'done'), null, { timeout: 5000 });
  const stillClosed = await page.evaluate(() => !document.querySelector('[data-up-progress]'));
  if (!stillClosed) throw new Error('strip reappeared after dismiss without new batch');
  // 新批次自动重现 + 行内独立取消：sftp_upload_cancel 恰好一次，状态收敛 cancelled
  await page.evaluate(() => {
    window.__myshelltool.files.uploadLocalPaths(['C:\\fake\\strip.bin']).catch(e => console.error('[test] strip upload 2 rejected: ' + e.message));
  });
  await page.waitForFunction(() => document.querySelector('[data-up-progress]'), null, { timeout: 5000 });
  await page.evaluate(() => document.querySelector('[data-up-progress-toggle]').click());
  await page.waitForFunction(() => document.querySelector('[data-up-progress-toggle]')?.getAttribute('aria-expanded') === 'true', null, { timeout: 3000 });
  const rowCancelPresent = await page.evaluate(() => {
    const btn = document.querySelector('#ups-detail [aria-label^="取消上传"]');
    btn?.click();
    return !!btn;
  });
  if (!rowCancelPresent) throw new Error('per-row cancel button missing');
  await page.waitForFunction(() => window.__myshelltool.files.transferQueue.some(i => i.name === 'strip.bin' && i.status === 'cancelled'), null, { timeout: 5000 });
  const cancelCallsTotal = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'sftp_upload_cancel').length);
  if (cancelCallsTotal !== cancelCallsAfterClose + 1) throw new Error('per-row cancel did not call sftp_upload_cancel exactly once: ' + (cancelCallsTotal - cancelCallsAfterClose));
  const retryPresent = await page.evaluate(() => !!document.querySelector('#ups-detail [aria-label^="重试上传"]'));
  if (!retryPresent) throw new Error('retry entry missing after cancel');

console.error('[step] scenario-3');
  // ════ 场景 3：覆盖确认链（probe 命中 → modal → 确认 → 流式上传）════
  await fixtureBackend(page, {
    'sftp_stat': spec({ name: 'over.txt', kind: 'file', size: 1 }),  // 已存在
    'fs_local_stat': spec({ name: 'over.txt', path: 'C:\\fake\\over.txt', kind: 'file', size: 3, modified: '', permissions: null, user: null, group: null }),
    'sftp_upload_from_file': spec(null),
    'sftp_list_dir': spec({ path: '/root', entries: [] }),
  });
  // 面板显式回 /root：2c 把面板留在了 /var/log，覆盖链断言沿用 /root 目标
  await page.evaluate(() => window.__myshelltool.files.navigateRemotePath('/root'));
  await page.waitForFunction(() => window.__myshelltool.files.remotePath === '/root', null, { timeout: 5000 });
  // uploadLocalPaths 在覆盖确认处挂起等用户选择——不能在 evaluate 里 await 它
  // （evaluate 永不返回 = 测试死锁）。fire-and-forget，由下方确认步骤放行。
  await page.evaluate(() => {
    window.__myshelltool.files.uploadLocalPaths(['C:\\fake\\over.txt']).catch(e => console.error('[test] overwrite upload rejected: ' + e.message));
  });
  await page.waitForFunction(() => window.__myshelltool.workbench.modal.type === 'confirmFileOverwrite', null, { timeout: 5000 });
  await page.evaluate(() => window.__myshelltool.files.confirmFileOverwrite());
  await page.waitForFunction(
    () => window.__tauriMock.calls.some(c => c.command === 'sftp_upload_from_file' && c.args.remotePath === '/root/over.txt'),
    null,
    { timeout: 10000 }
  );

console.error('[step] scenario-3b');
  // ════ 场景 3b：冲突三选之「重命名」——新名撞名报错不落盘，换名成功改路径 ════
  await fixtureBackend(page, {
    'sftp_stat': { dynamic: 'stat-map' },
    'fs_local_stat': spec({ name: 'over2.txt', path: 'C:\\fake\\over2.txt', kind: 'file', size: 3, modified: '', permissions: null, user: null, group: null }),
    'sftp_upload_from_file': spec(null),
    'sftp_list_dir': spec({ path: '/root', entries: [] }),
  });
  await page.evaluate(() => {
    window.__tauriMock.statMap = {
      '/root/over2.txt': { name: 'over2.txt', kind: 'file', size: 1 },
      '/root/taken.txt': { name: 'taken.txt', kind: 'file', size: 1 },
      // '/root/renamed.txt' 不在表内 = 不存在
    };
  });
  await page.evaluate(() => {
    window.__myshelltool.files.uploadLocalPaths(['C:\\fake\\over2.txt']).catch(e => console.error('[test] rename upload rejected: ' + e.message));
  });
  await page.waitForFunction(() => window.__myshelltool.workbench.modal.type === 'confirmFileOverwrite', null, { timeout: 5000 });
  // 第一次尝试：新名 taken.txt 已存在 → 弹窗保持 + 内联错误，不产生上传
  await page.evaluate(() => window.__myshelltool.files.renameFileOverwrite('taken.txt'));
  await page.waitForFunction(() => (document.querySelector('#modalBody')?.textContent || '').includes('新名称的目标也已存在'), null, { timeout: 5000 });
  if (await page.evaluate(() => window.__myshelltool.workbench.modal.type) !== 'confirmFileOverwrite') throw new Error('modal closed on rename error');
  // 第二次尝试：renamed.txt 可用 → settle rename，上传目标改路径
  await page.evaluate(() => window.__myshelltool.files.renameFileOverwrite('renamed.txt'));
  await page.waitForFunction(
    () => window.__tauriMock.calls.some(c => c.command === 'sftp_upload_from_file' && c.args.remotePath === '/root/renamed.txt'),
    null,
    { timeout: 10000 }
  );
  const renameQueueName = await page.evaluate(() => window.__myshelltool.files.transferQueue.filter(i => i.status === 'done').some(i => i.name === 'renamed.txt'));
  if (!renameQueueName) throw new Error('renamed queue item missing');

console.error('[step] scenario-3c');
  // ════ 场景 3c：冲突三选之「跳过」——取消确认后不上传、批次不炸 ════
  await fixtureBackend(page, {
    'sftp_stat': { dynamic: 'stat-map' },
    'fs_local_stat': spec({ name: 'skipme.txt', path: 'C:\\fake\\skipme.txt', kind: 'file', size: 3, modified: '', permissions: null, user: null, group: null }),
    'sftp_upload_from_file': spec(null),
    'sftp_list_dir': spec({ path: '/root', entries: [] }),
  });
  await page.evaluate(() => {
    window.__tauriMock.statMap = { '/root/skipme.txt': { name: 'skipme.txt', kind: 'file', size: 1 } };
  });
  const uploadCallsBeforeSkip = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'sftp_upload_from_file').length);
  await page.evaluate(() => {
    window.__myshelltool.files.uploadLocalPaths(['C:\\fake\\skipme.txt']).catch(e => console.error('[test] skip upload rejected: ' + e.message));
  });
  await page.waitForFunction(() => window.__myshelltool.workbench.modal.type === 'confirmFileOverwrite', null, { timeout: 5000 });
  // 快照必须在 settle 之前取：跳过后批次收尾刷新在毫秒级发生，事后再取会错过增量
  const listCallsBeforeSkipSettle = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'sftp_list_dir').length);
  await page.evaluate(() => window.__myshelltool.files.cancelFileOverwrite());
  await page.waitForFunction(() => window.__myshelltool.workbench.modal.type === null, null, { timeout: 5000 });
  // 等批次收尾的可观测终态（收尾必刷当前面板目录），再断言上传未发生
  await page.waitForFunction(
    count => window.__tauriMock.calls.filter(c => c.command === 'sftp_list_dir').length > count,
    listCallsBeforeSkipSettle,
    { timeout: 10000 }
  );
  const uploadCallsAfterSkip = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'sftp_upload_from_file').length);
  if (uploadCallsAfterSkip !== uploadCallsBeforeSkip) throw new Error('skip decision still uploaded the file');

console.error('[step] scenario-3d');
  // ════ 场景 3d：「应用到本批剩余冲突」——首个确认后同批第二个不再弹窗 ════
  await fixtureBackend(page, {
    'sftp_stat': { dynamic: 'stat-map' },
    'sftp_upload_from_file': spec(null),
    'sftp_list_dir': spec({ path: '/root', entries: [] }),
  });
  await page.evaluate(() => {
    // fs_local_stat 也要按路径返回两个文件条目：与 sftp_stat 共用 stat-map
    // dynamic（本地/远端路径不撞名，一张表即可）。
    window.__tauriMock.invokeSpecs['fs_local_stat'] = { dynamic: 'stat-map' };
    window.__tauriMock.statMap = {
      'C:\\fake\\f1.txt': { name: 'f1.txt', path: 'C:\\fake\\f1.txt', kind: 'file', size: 3 },
      'C:\\fake\\f2.txt': { name: 'f2.txt', path: 'C:\\fake\\f2.txt', kind: 'file', size: 4 },
      '/root/f1.txt': { name: 'f1.txt', kind: 'file', size: 1 },
      '/root/f2.txt': { name: 'f2.txt', kind: 'file', size: 1 },
    };
  });
  await page.evaluate(() => {
    window.__myshelltool.files.uploadLocalPaths(['C:\\fake\\f1.txt', 'C:\\fake\\f2.txt']).catch(e => console.error('[test] applyAll upload rejected: ' + e.message));
  });
  await page.waitForFunction(() => window.__myshelltool.workbench.modal.type === 'confirmFileOverwrite', null, { timeout: 5000 });
  // 勾选「应用到本批剩余冲突」后确认覆盖
  await page.evaluate(() => {
    const box = document.querySelector('.overwrite-apply input');
    if (!box) throw new Error('applyAll checkbox missing (batch item should render it)');
    box.click();
  });
  await page.evaluate(() => window.__myshelltool.files.confirmFileOverwrite());
  await page.waitForFunction(
    () => window.__tauriMock.calls.some(c => c.command === 'sftp_upload_from_file' && c.args.remotePath === '/root/f1.txt'),
    null,
    { timeout: 10000 }
  );
  // 第二个文件沿用批次默认（不再弹窗）：modal 保持关闭且 f2 也完成上传
  await page.waitForFunction(
    () => window.__tauriMock.calls.some(c => c.command === 'sftp_upload_from_file' && c.args.remotePath === '/root/f2.txt'),
    null,
    { timeout: 10000 }
  );
  if (await page.evaluate(() => window.__myshelltool.workbench.modal.type) !== null) {
    throw new Error('applyAll did not suppress second conflict modal');
  }

console.error('[step] scenario-3e');
  // ════ 场景 3e：目录上传——递归收集 + mkdir 幂等链 + 子目录文件路径正确 ════
  await fixtureBackend(page, {
    'fs_local_list_dir': { dynamic: 'dir-map' },
    'sftp_stat': { dynamic: 'stat-map' },
    'sftp_mkdir': spec(null),
    'sftp_upload_from_file': spec(null),
    'sftp_list_dir': spec({ path: '/root', entries: [] }),
  });
  await page.evaluate(() => {
    window.__tauriMock.statMap = {};
    window.__tauriMock.dirMap = {
      'C:\\fake\\proj': [
        { name: 'a.txt', path: 'C:\\fake\\proj\\a.txt', kind: 'file', size: 10 },
        { name: 'sub', path: 'C:\\fake\\proj\\sub', kind: 'directory', size: 0 },
        { name: 'link', path: 'C:\\fake\\proj\\link', kind: 'symlink', size: 0 },
      ],
      'C:\\fake\\proj\\sub': [
        { name: 'b.txt', path: 'C:\\fake\\proj\\sub\\b.txt', kind: 'file', size: 20 },
      ],
    };
  });
  // B2 后续（新建目录跳过逐文件探测）：uploadDirectory 前取 sftp_stat 计数，
  // 批次完成后断言零新增——mkdir 首次成功的目录必为空，其下文件不再探测。
  const statCallsBefore3e = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'sftp_stat').length);
  await page.evaluate(() => window.__myshelltool.files.uploadDirectory('C:\\fake\\proj'));
  const mkdirCalls3e = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'sftp_mkdir').map(c => c.args.path));
  if (!mkdirCalls3e.includes('/root/proj') || !mkdirCalls3e.includes('/root/proj/sub')) {
    throw new Error('dir upload mkdir wrong: ' + JSON.stringify(mkdirCalls3e));
  }
  const upPaths3e = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'sftp_upload_from_file').map(c => c.args.remotePath));
  if (!upPaths3e.includes('/root/proj/a.txt') || !upPaths3e.includes('/root/proj/sub/b.txt')) {
    throw new Error('dir upload file targets wrong: ' + JSON.stringify(upPaths3e));
  }
  const statCallsAfter3e = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'sftp_stat').length);
  if (statCallsAfter3e !== statCallsBefore3e) {
    throw new Error('fresh-dir files must skip per-file conflict probe: +' + (statCallsAfter3e - statCallsBefore3e) + ' sftp_stat calls');
  }

console.error('[step] scenario-3f');
  // ════ 场景 3f：目录下载——远端递归 + 本地建目录 + 逐文件落正确子目录 ════
  await fixtureBackend(page, {
    'sftp_list_dir': { dynamic: 'dir-map' },
    'fs_local_stat': spec(null), // 本地目标不存在 → 不弹冲突窗
    'fs_local_mkdir': spec(null),
    'sftp_download_to_file': spec(null),
  });
  await page.evaluate(() => {
    window.__tauriMock.dirMap = {
      '/root/rproj': [
        { name: 'a.txt', path: '/root/rproj/a.txt', kind: 'file', size: 10 },
        { name: 'sub', path: '/root/rproj/sub', kind: 'directory', size: 0 },
      ],
      '/root/rproj/sub': [
        { name: 'b.txt', path: '/root/rproj/sub/b.txt', kind: 'file', size: 20 },
      ],
    };
  });
  await page.evaluate(() => window.__myshelltool.files.downloadDirectory(
    { name: 'rproj', path: '/root/rproj', kind: 'directory', size: 0, modified: '', permissions: null, user: null, group: null },
    'D:\\dest'
  ));
  const localMkdir3f = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'fs_local_mkdir').map(c => c.args.path));
  if (!localMkdir3f.includes('D:\\dest\\rproj') || !localMkdir3f.includes('D:\\dest\\rproj\\sub')) {
    throw new Error('dir download local mkdir wrong: ' + JSON.stringify(localMkdir3f));
  }
  const down3f = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'sftp_download_to_file').map(c => c.args.localPath));
  if (!down3f.includes('D:\\dest\\rproj\\a.txt') || !down3f.includes('D:\\dest\\rproj\\sub\\b.txt')) {
    throw new Error('dir download file targets wrong: ' + JSON.stringify(down3f));
  }

console.error('[step] scenario-3g');
  // ════ 场景 3g：目录批次取消——当前文件中止 + 剩余文件不再入队 ════
  await fixtureBackend(page, {
    'fs_local_list_dir': { dynamic: 'dir-map' },
    'sftp_stat': { dynamic: 'stat-map' },
    'sftp_mkdir': spec(null),
    'sftp_upload_from_file': { dynamic: 'upload-gate' },
    'sftp_upload_cancel': { dynamic: 'cancel-upload' },
    'sftp_list_dir': spec({ path: '/root', entries: [] }),
  });
  await page.evaluate(() => {
    window.__tauriMock.statMap = {};
    window.__tauriMock.dirMap = {
      'C:\\fake\\cproj': [
        { name: 'x1.bin', path: 'C:\\fake\\cproj\\x1.bin', kind: 'file', size: 100 },
        { name: 'x2.bin', path: 'C:\\fake\\cproj\\x2.bin', kind: 'file', size: 100 },
      ],
    };
  });
  await page.evaluate(() => {
    window.__myshelltool.files.uploadDirectory('C:\\fake\\cproj').catch(e => console.error('[test] cancelled dir upload rejected: ' + e.message));
  });
  await page.waitForFunction(
    () => window.__tauriMock.calls.some(c => c.command === 'sftp_upload_from_file' && c.args.remotePath === '/root/cproj/x1.bin'),
    null,
    { timeout: 5000 }
  );
  const cancelledTransferId = await page.evaluate(
    () => window.__tauriMock.calls.find(c => c.command === 'sftp_upload_from_file' && c.args.remotePath === '/root/cproj/x1.bin').args.transferId
  );
  await page.evaluate(() => window.__myshelltool.files.cancelDirBatch(window.__myshelltool.files.latestActiveDirBatch()));
  await page.waitForFunction(
    id => window.__myshelltool.files.transferQueue.find(i => i.id === id)?.status === 'cancelled',
    cancelledTransferId,
    { timeout: 5000 }
  );
  // 等批次注册表清空（uploadDirectory 的 finally），此时取消结果已成定局
  await page.waitForFunction(() => window.__myshelltool.files.latestActiveDirBatch() === null, null, { timeout: 10000 });
  // 并发批次语义（B2 并发修复）：x2 可能与 x1 同时在途（upload 已发起），取消
  // 旗标把两者一并中止——判据从串行时代的「upload 恰好 1 次」改为终态：
  // 该批次所有队列项必须收敛 cancelled，无一漏成 done/error。
  const cprojItems3g = await page.evaluate(() =>
    window.__myshelltool.files.transferQueue
      .filter(i => String(i.remotePath || '').startsWith('/root/cproj/'))
      .map(i => ({ name: i.name, status: i.status }))
  );
  if (!cprojItems3g.length || cprojItems3g.some(i => i.status !== 'cancelled')) {
    throw new Error('cancelled dir batch left non-cancelled items: ' + JSON.stringify(cprojItems3g));
  }

console.error('[step] scenario-3h');
  // ════ 场景 3h：上传完成 toast 聚合（B2 后续：N 文件不再 N 条刷屏）════
  // 双文件同批 → 1.5s 窗口合并为一条「已上传 2 个文件 → /agg」；
  // 单文件 → 保持原「已上传：name」文案。
  await fixtureBackend(page, {
    'sftp_stat': { dynamic: 'stat-map' },
    'sftp_upload_from_file': spec(null),
    // list-echo：导航 /agg 后回显 /agg（静态 /root 会把面板路径拽回去）
    'sftp_list_dir': { dynamic: 'list-echo' },
  });
  await page.evaluate(() => {
    window.__tauriMock.invokeSpecs['fs_local_stat'] = { dynamic: 'stat-map' };
    window.__tauriMock.statMap = {
      'C:\\fake\\c1.txt': { name: 'c1.txt', path: 'C:\\fake\\c1.txt', kind: 'file', size: 3 },
      'C:\\fake\\c2.txt': { name: 'c2.txt', path: 'C:\\fake\\c2.txt', kind: 'file', size: 4 },
      'C:\\fake\\solo.txt': { name: 'solo.txt', path: 'C:\\fake\\solo.txt', kind: 'file', size: 5 },
      // 远端目标路径不在表内 = 不存在（stat-map 默认 miss → null → 无冲突）
    };
  });
  // 用独占目录 /agg：/root 作为 key 已被前序场景在 1.5s 窗口内累积过记录
  // （跨场景连发会合并计数——与下载侧聚合同语义），独占目录使断言计数确定。
  await page.evaluate(() => window.__myshelltool.files.navigateRemotePath('/agg'));
  await page.waitForFunction(() => window.__myshelltool.files.remotePath === '/agg', null, { timeout: 5000 });
  await page.evaluate(() => {
    window.__myshelltool.files.uploadLocalPaths(['C:\\fake\\c1.txt', 'C:\\fake\\c2.txt']).catch(e => console.error('[test] agg upload rejected: ' + e.message));
  });
  // 聚合窗口 1.5s 后 flush——等合并 toast 出现（不赌毫秒，等可观测终态）
  await page.waitForFunction(() => {
    return (window.__myshelltool.workbench.toasts || []).some(x => (x.message || '').includes('已上传 2 个文件 → /agg'));
  }, null, { timeout: 8000 });
  // 逐文件 toast 不应出现（聚合的意义所在）
  const perFileToasts3h = await page.evaluate(() => (window.__myshelltool.workbench.toasts || []).filter(x => (x.message || '') === '已上传：c1.txt' || (x.message || '') === '已上传：c2.txt').length);
  if (perFileToasts3h !== 0) throw new Error('per-file upload toasts leaked past aggregator: ' + perFileToasts3h);
  // 单文件路径：保持原文案
  await page.evaluate(() => {
    window.__myshelltool.files.uploadLocalPaths(['C:\\fake\\solo.txt']).catch(e => console.error('[test] solo upload rejected: ' + e.message));
  });
  await page.waitForFunction(() => {
    return (window.__myshelltool.workbench.toasts || []).some(x => (x.message || '') === '已上传：solo.txt');
  }, null, { timeout: 8000 });

console.error('[step] scenario-3i');
  // ════ 场景 3i：同名双写防护（B2 并发修复 P0）════
  // 同批两个不同本地路径的同名文件上传到同一远端目录：同目标互斥链保证
  // 第二个文件的探测发生在第一个落盘之后 → 弹冲突窗走三选，而不是两条
  // 传输交错写坏远端文件。机制断言用 calls 时序（第二次 stat 的序号必须
  // 在第一次 upload 之后）+ 行为断言（弹窗 + 跳过后 upload 恰好 1 次）。
  await fixtureBackend(page, {
    'fs_local_stat': { dynamic: 'stat-map' },
    'sftp_stat': { dynamic: 'stat-map' },
    'sftp_upload_from_file': { dynamic: 'upload-mark' },
    'sftp_list_dir': spec({ path: '/root', entries: [] }),
  });
  // 3h 末尾把面板导航到了 /agg——先回 /root，使同名目标确定落在 /root/f.txt
  await page.evaluate(() => window.__myshelltool.files.navigateRemotePath('/root'));
  await page.waitForFunction(() => window.__myshelltool.files.remotePath === '/root', null, { timeout: 5000 });
  await page.evaluate(() => {
    window.__tauriMock.statMap = {
      'C:\\fake\\dup\\f.txt': { name: 'f.txt', path: 'C:\\fake\\dup\\f.txt', kind: 'file', size: 5 },
      'C:\\fake\\other\\f.txt': { name: 'f.txt', path: 'C:\\fake\\other\\f.txt', kind: 'file', size: 6 },
      // /root/f.txt 初始不存在（miss → null → 第一个文件直接传）
    };
  });
  await page.evaluate(() => {
    window.__testPromise = window.__myshelltool.files.uploadLocalPaths(['C:\\fake\\dup\\f.txt', 'C:\\fake\\other\\f.txt']);
  });
  // 第一个文件无冲突直接上传
  await page.waitForFunction(
    () => window.__tauriMock.calls.some(c => c.command === 'sftp_upload_from_file' && c.args.remotePath === '/root/f.txt'),
    null, { timeout: 5000 }
  );
  // 第二个同名文件必须弹冲突窗（探测看到第一个的落盘），而非直接再传
  await page.waitForFunction(() => window.__myshelltool.workbench.modal.type === 'confirmFileOverwrite', null, { timeout: 5000 });
  // 机制断言：第二次探测 /root/f.txt 的调用序号在第一次上传之后（同目标串行化）
  const orderOk = await page.evaluate(() => {
    const calls = window.__tauriMock.calls;
    const statIdx = calls.map((c, i) => ({ c, i })).filter(x => x.c.command === 'sftp_stat' && x.c.args.path === '/root/f.txt').map(x => x.i);
    const upIdx = calls.map((c, i) => ({ c, i })).filter(x => x.c.command === 'sftp_upload_from_file' && x.c.args.remotePath === '/root/f.txt').map(x => x.i);
    return statIdx.length >= 2 && upIdx.length >= 1 && statIdx[1] > upIdx[0];
  });
  if (!orderOk) throw new Error('same-target serialization broken: second probe must follow first upload');
  // 用户选择跳过（取消按钮 = skip 语义）→ 第二个文件不传，批次正常收尾
  await page.evaluate(() => window.__myshelltool.files.cancelFileOverwrite());
  await page.evaluate(() => window.__testPromise);
  const dupUploadCount = await page.evaluate(
    () => window.__tauriMock.calls.filter(c => c.command === 'sftp_upload_from_file' && c.args.remotePath === '/root/f.txt').length
  );
  if (dupUploadCount !== 1) throw new Error('duplicate upload leaked past conflict gate: ' + dupUploadCount);

console.error('[step] scenario-3j');
  // ════ 场景 3j：并发上限生效 + 单项失败不炸批（B2 并发修复）════
  // 5 个不同名文件（默认并发 3）：慢速上传记录在途峰值，断言 2 ≤ peak ≤ 3
  //（上限不被冲破；也不是假并发的串行）。
  await fixtureBackend(page, {
    'fs_local_stat': { dynamic: 'stat-map' },
    'sftp_stat': { dynamic: 'stat-map' },
    'sftp_upload_from_file': { dynamic: 'upload-slow' },
    'sftp_list_dir': spec({ path: '/root', entries: [] }),
  });
  await page.evaluate(() => {
    window.__tauriMock.statMap = {};
    window.__tauriMock.uploadPeak = 0;
    window.__tauriMock.uploadInFlight = 0;
    const paths = [];
    for (let i = 0; i < 5; i++) {
      const p = 'C:\\fake\\par\\p' + i + '.txt';
      paths.push(p);
      window.__tauriMock.statMap[p] = { name: 'p' + i + '.txt', path: p, kind: 'file', size: 1 };
    }
    window.__testPromise = window.__myshelltool.files.uploadLocalPaths(paths);
  });
  const summaryPar = await page.evaluate(() => window.__testPromise.then(() =>
    window.__myshelltool.files.transferQueue
      .filter(i => /^p\d\.txt$/.test(i.name))
      .map(i => i.status)
  ));
  if (summaryPar.length !== 5 || summaryPar.some(s => s !== 'done')) {
    throw new Error('parallel batch incomplete: ' + JSON.stringify(summaryPar));
  }
  const peak = await page.evaluate(() => window.__tauriMock.uploadPeak);
  if (peak > 3) throw new Error('concurrency limit exceeded: peak ' + peak + ' > 3');
  if (peak < 2) throw new Error('pool not actually parallel: peak ' + peak + ' (expected overlap)');
  // 单项失败不炸批：4 个文件中 1 个注定失败，其余照传，批次正常收尾且
  // summary.failed 如实计数（失败不并入 uploaded 虚报完成数）。
  await fixtureBackend(page, {
    'fs_local_stat': { dynamic: 'stat-map' },
    'sftp_stat': { dynamic: 'stat-map' },
    'sftp_upload_from_file': { dynamic: 'upload-fail-marked' },
    'sftp_list_dir': spec({ path: '/root', entries: [] }),
  });
  await page.evaluate(() => {
    window.__tauriMock.statMap = {};
    const paths = [];
    for (let i = 0; i < 4; i++) {
      const name = (i === 2 ? 'boom' : 'ok') + i;
      const p = 'C:\\fake\\mix\\' + name + '.txt';
      paths.push(p);
      window.__tauriMock.statMap[p] = { name: name + '.txt', path: p, kind: 'file', size: 1 };
    }
    window.__testPromise = window.__myshelltool.files.uploadLocalPaths(paths);
  });
  const summaryMix = await page.evaluate(() => window.__testPromise.then(() =>
    window.__myshelltool.files.transferQueue
      .filter(i => /^(ok|boom)\d\.txt$/.test(i.name))
      .map(i => ({ name: i.name, status: i.status }))
  ));
  const doneMix = summaryMix.filter(i => i.status === 'done');
  const errMix = summaryMix.filter(i => i.status === 'error');
  if (doneMix.length !== 3 || errMix.length !== 1 || errMix[0].name !== 'boom2.txt') {
    throw new Error('single failure must not abort batch: ' + JSON.stringify(summaryMix));
  }

console.error('[step] scenario-4');
  // ════ 场景 4：host key 事件 → 弹窗 → resolve 回传 ════
  await fixtureBackend(page, { 'ssh_confirm_host_key': spec(null) });
  // 跨窗口路由守卫：本窗口无 connecting 会话时不认领 hostkey 事件（正确行为）。
  // host key 事件只来自真实会话连接，把本窗口会话置为 connecting 即具备认领资格。
  await page.evaluate(() => {
    const s = window.__myshelltool.sessions.sessions[0];
    s.status = 'connecting';
  });
  await emitEvent(page, 'ssh-host-key-verify', {
    request_id: 'rk-1', host_port: '127.0.0.1:2222', key_type: 'ssh-ed25519',
    fingerprint: 'AA:BB', is_changed: false
  });
  await page.waitForFunction(() => window.__myshelltool.workbench.modal.type === 'hostKeyVerify', null, { timeout: 5000 });
  await page.evaluate(() => window.__myshelltool.sessions.resolveHostKeyPrompt('rk-1', true));
  await page.waitForFunction(() => window.__tauriMock.calls.some(c => c.command === 'ssh_confirm_host_key' && c.args.accepted === true), null, { timeout: 5000 });

console.error('[step] scenario-5');
  // ════ 场景 5：资产编辑器直输新分组（AppCombobox 可选可输，2026-10 分组重做）════
  // 直输不存在的多级路径 → 菜单出「新建」提示行；点保存（blur 先于 click 提交
  // 输入值）→ save_connection_asset 收到完整分组路径（后端 upsert 收口声明）。
  await fixtureBackend(page, {
    'save_credential': spec(null),
    'save_connection_asset': spec({ source: 'local', count: 1, assets: [ASSET], groups: ['生产/数据库/直输'] }),
  });
  await page.evaluate(() => {
    window.__myshelltool.workbench.modal = { type: 'assetEditor', asset: null, payload: null };
  });
  await page.waitForSelector('[data-asset-field="group"] input', { timeout: 5000 });
  // 必填：名称 + 密码（Password 认证首存校验，modalSubmit 拦截空密码）
  await page.fill('[data-asset-field="name"] input', '直输分组机');
  await page.fill('[data-asset-field="password"] input', 'pw-123');
  // 分组 combobox：focus 开菜单 → 直输多级路径 → 末行出现「新建「…」」
  const groupInput = '[data-asset-field="group"] input';
  await page.click(groupInput);
  await page.fill(groupInput, '生产/数据库/直输');
  await page.waitForSelector('.app-combobox-create', { timeout: 3000 });
  const createRowText = await page.textContent('.app-combobox-create');
  if (!createRowText.includes('生产/数据库/直输')) {
    throw new Error('combobox create row missing typed path: ' + createRowText);
  }
  await page.click('#modalPrimary');
  await page.waitForFunction(() => window.__tauriMock.calls.some(c => c.command === 'save_connection_asset'), null, { timeout: 5000 });
  const savedGroup = await page.evaluate(() =>
    window.__tauriMock.calls.find(c => c.command === 'save_connection_asset').args.asset.group
  );
  if (savedGroup !== '生产/数据库/直输') {
    throw new Error('typed group not committed on save: ' + JSON.stringify(savedGroup));
  }
  // 保存成功后弹窗收敛（失败会留在原地并显示 formError）
  await page.waitForFunction(() => window.__myshelltool.workbench.modal.type === null, null, { timeout: 5000 });

  console.log('IPC flow tests passed (connect+stream+reconnect / streaming upload / upload cancel / refresh-current-dir after upload / upload progress strip / overwrite 3-way + rename/skip/apply-all / dir upload+download+batch-cancel / host-key resolve / asset-editor typed-group create)');
} finally {
  await browser.close();
}
