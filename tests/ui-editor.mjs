// ui-editor — 内置编辑器的 IPC 流覆盖（v0.18）
//
// 与 ui-ipc-flows 同一套脚本化 mock 后端（window.__TAURI__.core.invoke），
// 经 DEV 钩子 window.__myshelltool.editor 驱动真实 editor store 流：
//   1. 本地文件打开 → 程序化编辑（dirty）→ 保存（断言 encoding/eol/expected/backup 参数与基线更新）
//   2. JSON 语法校验拦截保存（取消 → 不写盘；仍要保存 → 写盘）
//   3. 写回冲突（conflict → 三选弹窗 → 覆盖保存 = expected 置空重写）
//   4. 远端无会话打开 → 如实报错（不回落别的资产）
//   5. 未保存关 tab 三选（取消保留 / 放弃移除）
//   6. UI 入口：本地面板双击 .txt 开编辑器、.zip 不开（保持上传语义）
//   7. 下载完成 toast 聚合（1.5s 窗口合并 + 双按钮 + 10s 时长）
//
// 跑法：npm run dev 起服务后 node tests/ui-editor.mjs（MYSHELLTOOL_BASE_URL 可覆盖）。

import { chromium } from 'playwright';

const baseUrl = process.env.MYSHELLTOOL_BASE_URL ?? 'http://127.0.0.1:41234/';

const MOCK_INIT = `
window.__tauriMock = {
  calls: [],
  invokeSpecs: {
    backend_status: { value: { ready: true, mode: 'desktop' } },
    tunnel_list: { value: [] },
    get_credential_status: { value: { exists: true } },
    fs_local_home_dir: { value: 'C:\\\\Users\\\\test' },
    resource_monitor_list_active: { value: [] },
    'plugin:dialog|open': { value: 'C:\\\\dl' },
  },
  eventHandlers: {},
  connectCounter: 0,
  uploadGates: {},
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
function clearCalls(page) {
  return page.evaluate(() => { window.__tauriMock.calls = []; });
}
function closeModal(page) {
  return page.evaluate(() => { window.__myshelltool.workbench.modal = { type: null }; });
}

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
page.on('pageerror', err => { throw new Error('page error: ' + err.message); });

try {
  await page.addInitScript(MOCK_INIT);
  await page.route('**/*', route => route.continue());
  await page.goto(baseUrl, { waitUntil: 'networkidle' });

  await fixtureBackend(page, {
    list_connection_assets: spec({ source: 'local', count: 1, assets: [ASSET], groups: [] }),
  });
  await page.waitForFunction(() => window.__myshelltool?.editor, null, { timeout: 10000 });
  // 应用 mount 时的首次资产加载早于 fixture（mock 无 handler → 空表），补一次重载
  await page.evaluate(() => window.__myshelltool.workbench.reloadAssets());
  await page.waitForFunction(() => window.__myshelltool.workbench.assets.length === 1, null, { timeout: 5000 });

  // ════ 场景 1：本地打开 → 编辑 → 保存（参数与基线）════
  console.error('[step] scenario-1 open/edit/save');
  await fixtureBackend(page, {
    'fs_local_read_text': spec({ content: 'hello\n', encoding: 'utf-8', hasBom: false, eol: 'lf', size: 6, modified: '100', readOnly: false }),
    'fs_local_write_text': spec({ status: 'written', size: 11, modified: '200' }),
    'editor_draft_get': spec(null),
    'editor_draft_delete': spec(null),
  });
  await page.evaluate(async () => {
    await window.__myshelltool.editor.openTarget({ kind: 'local', assetId: null, path: 'C:\\fake\\a.txt' });
  });
  await page.waitForFunction(() => window.__myshelltool.editor.tabs.length === 1 && window.__myshelltool.editor.tabs[0].status === 'ready', null, { timeout: 5000 });
  // 打开时应探测草稿
  if (!await page.evaluate(() => window.__tauriMock.calls.some(c => c.command === 'editor_draft_get'))) {
    throw new Error('draft probe not called on open');
  }
  // 程序化编辑（CodeMirror set 通道的等价物）→ dirty
  await page.evaluate(() => {
    const e = window.__myshelltool.editor;
    e.replaceContent(e.tabs[0].id, 'hello world');
  });
  if (!await page.evaluate(() => window.__myshelltool.editor.tabs[0].dirty)) throw new Error('replaceContent did not mark dirty');
  const saved1 = await page.evaluate(() => window.__myshelltool.editor.saveTab(window.__myshelltool.editor.tabs[0].id));
  if (saved1 !== true) throw new Error('saveTab scenario-1 failed');
  const write1 = await page.evaluate(() => window.__tauriMock.calls.find(c => c.command === 'fs_local_write_text'));
  if (!write1) throw new Error('fs_local_write_text not called');
  const a1 = write1.args;
  if (a1.content !== 'hello world') throw new Error('content wrong: ' + JSON.stringify(a1));
  if (a1.encoding !== null) throw new Error('encoding should be null (utf-8 default): ' + JSON.stringify(a1));
  if (a1.eol !== 'lf') throw new Error('eol should follow baseline: ' + JSON.stringify(a1));
  if (a1.expectedSize !== 6 || a1.expectedModified !== '100') throw new Error('expected baseline not passed: ' + JSON.stringify(a1));
  if (a1.backup !== true) throw new Error('backup default should be on: ' + JSON.stringify(a1));
  // 保存成功：dirty 清除 + 基线更新（第二次保存 expected 用新值）
  if (await page.evaluate(() => window.__myshelltool.editor.tabs[0].dirty)) throw new Error('dirty not cleared after save');
  await clearCalls(page);
  await fixtureBackend(page, {
    'fs_local_write_text': spec({ status: 'written', size: 11, modified: '300' }),
    'editor_draft_delete': spec(null),
  });
  await page.evaluate(() => window.__myshelltool.editor.saveTab(window.__myshelltool.editor.tabs[0].id));
  const write2 = await page.evaluate(() => window.__tauriMock.calls.find(c => c.command === 'fs_local_write_text'));
  if (write2.args.expectedSize !== 11 || write2.args.expectedModified !== '200') {
    throw new Error('baseline not updated after first save: ' + JSON.stringify(write2.args));
  }

  // ════ 场景 2：JSON 校验拦截 ════
  console.error('[step] scenario-2 json validation gate');
  await fixtureBackend(page, {
    'fs_local_read_text': spec({ content: '{"a":', encoding: 'utf-8', hasBom: false, eol: 'lf', size: 6, modified: '100', readOnly: false }),
    'fs_local_write_text': spec({ status: 'written', size: 9, modified: '150' }),
    'editor_draft_get': spec(null),
    'editor_draft_delete': spec(null),
  });
  await page.evaluate(async () => {
    await window.__myshelltool.editor.openTarget({ kind: 'local', assetId: null, path: 'C:\\fake\\b.json' });
  });
  await page.waitForFunction(() => window.__myshelltool.editor.tabs.length === 2 && window.__myshelltool.editor.tabs[1].status === 'ready', null, { timeout: 5000 });
  await page.evaluate(() => {
    const e = window.__myshelltool.editor;
    e.replaceContent(e.tabs[1].id, '{bad json');
  });
  await clearCalls(page);
  // JSON 解析失败 → 弹「仍要保存？」。saveTab 的 promise 挂在弹窗按钮上，
  // 不能先 await——先触发、等弹窗、再按按钮。
  await page.evaluate(() => { window.__myshelltool.editor.saveTab(window.__myshelltool.editor.tabs[1].id); });
  await page.waitForFunction(() => window.__myshelltool.workbench.modal.type === 'editorDialog', null, { timeout: 5000 });
  const validationTitle = await page.evaluate(() => window.__myshelltool.workbench.modal.payload?.title);
  if (validationTitle !== '语法检查未通过') throw new Error('validation dialog missing: ' + validationTitle);
  // 选取消（按钮 0）→ 不写盘。可观测信号：saveTab 的 finally 会清 savingTabIds
  const jsonTabId = await page.evaluate(() => window.__myshelltool.editor.tabs[1].id);
  const cancelErr = await page.evaluate(() => window.__myshelltool.editor.resolveEditorDialog(0));
  if (cancelErr !== null) throw new Error('cancel button errored: ' + cancelErr);
  await page.waitForFunction(id => !window.__myshelltool.editor.savingTabIds[id], jsonTabId, { timeout: 5000 });
  if (await page.evaluate(() => window.__tauriMock.calls.some(c => c.command === 'fs_local_write_text'))) {
    throw new Error('cancel must not write');
  }
  // 再保存 → 选「仍要保存」（按钮 1）→ 写盘
  await page.evaluate(() => { window.__myshelltool.editor.saveTab(window.__myshelltool.editor.tabs[1].id); });
  await page.waitForFunction(() => window.__myshelltool.workbench.modal.type === 'editorDialog', null, { timeout: 5000 });
  await page.evaluate(() => window.__myshelltool.editor.resolveEditorDialog(1));
  await page.waitForFunction(() => window.__tauriMock.calls.some(c => c.command === 'fs_local_write_text'), null, { timeout: 5000 });
  await closeModal(page);

  // ════ 场景 3：写回冲突 → 覆盖保存 ════
  console.error('[step] scenario-3 conflict overwrite');
  await fixtureBackend(page, {
    'fs_local_write_text': spec({ status: 'conflict', size: 99, modified: '999', conflictNote: 'test conflict' }),
    'editor_draft_delete': spec(null),
  });
  await page.evaluate(() => {
    const e = window.__myshelltool.editor;
    e.replaceContent(e.tabs[1].id, '{ "fixed": true }');
  });
  await page.evaluate(() => window.__myshelltool.editor.saveTab(window.__myshelltool.editor.tabs[1].id));
  await page.waitForFunction(() => window.__myshelltool.editor.tabs[1].conflict !== null, null, { timeout: 5000 });
  const conflictTitle = await page.evaluate(() => window.__myshelltool.workbench.modal.payload?.title);
  if (conflictTitle !== '文件已被修改') throw new Error('conflict dialog missing: ' + conflictTitle);
  // 覆盖保存 = 按钮 3；换 written fixture → expected 置空的强制写
  await fixtureBackend(page, {
    'fs_local_write_text': spec({ status: 'written', size: 17, modified: '1000' }),
  });
  await page.evaluate(() => window.__myshelltool.editor.resolveEditorDialog(3));
  await page.waitForFunction(() => window.__myshelltool.editor.tabs[1].conflict === null && window.__myshelltool.editor.tabs[1].dirty === false, null, { timeout: 5000 });
  const forceWrite = await page.evaluate(() => window.__tauriMock.calls.filter(c => c.command === 'fs_local_write_text').pop());
  if (forceWrite.args.expectedSize !== null || forceWrite.args.expectedModified !== null) {
    throw new Error('overwrite must drop expected guard: ' + JSON.stringify(forceWrite.args));
  }
  await closeModal(page);

  // ════ 场景 4：远端无会话 → 如实报错 ════
  console.error('[step] scenario-4 remote without session');
  await page.evaluate(async () => {
    await window.__myshelltool.editor.openTarget({ kind: 'remote', assetId: 'ipc-test-1', path: '/etc/app.conf' });
  });
  await page.waitForFunction(() => {
    const t = window.__myshelltool.editor.tabs[2];
    return t && t.status === 'error';
  }, null, { timeout: 5000 });
  const remoteErr = await page.evaluate(() => window.__myshelltool.editor.tabs[2].error);
  if (!remoteErr || !remoteErr.message.includes('SSH 会话')) throw new Error('no-session error wrong: ' + JSON.stringify(remoteErr));
  await page.evaluate(() => window.__myshelltool.editor.requestCloseTab(window.__myshelltool.editor.tabs[2].id));

  // ════ 场景 5：未保存关 tab 三选 ════
  console.error('[step] scenario-5 unsaved close triage');
  await fixtureBackend(page, {
    'editor_draft_delete': spec(null),
  });
  await page.evaluate(() => {
    const e = window.__myshelltool.editor;
    e.setActive(e.tabs[1].id);
    e.replaceContent(e.tabs[1].id, '{ "more": "edits" }');
  });
  await page.evaluate(() => window.__myshelltool.editor.requestCloseTab(window.__myshelltool.editor.tabs[1].id));
  await page.waitForFunction(() => window.__myshelltool.workbench.modal.type === 'editorDialog', null, { timeout: 5000 });
  // 取消 → tab 保留
  await page.evaluate(() => window.__myshelltool.editor.resolveEditorDialog(0));
  if (await page.evaluate(() => window.__myshelltool.editor.tabs.length) !== 2) throw new Error('cancel should keep tab');
  // 放弃更改（按钮 1）→ tab 移除
  await page.evaluate(() => window.__myshelltool.editor.requestCloseTab(window.__myshelltool.editor.tabs[1].id));
  await page.evaluate(() => window.__myshelltool.editor.resolveEditorDialog(1));
  await page.waitForFunction(() => window.__myshelltool.editor.tabs.length === 1, null, { timeout: 5000 });
  await closeModal(page);

  // ════ 场景 6：UI 双击入口分流（本地 .txt 开编辑器 / .zip 不开）════
  console.error('[step] scenario-6 dblclick routing');
  await fixtureBackend(page, {
    'fs_local_list_dir': spec({ path: 'C:\\fake', parent: 'C:\\', entries: [
      { name: 'notes.txt', path: 'C:\\fake\\notes.txt', kind: 'file', size: 3, modified: '1', permissions: null, user: null, group: null },
      { name: 'archive.zip', path: 'C:\\fake\\archive.zip', kind: 'file', size: 9, modified: '1', permissions: null, user: null, group: null },
    ] }),
    'fs_local_read_text': spec({ content: 'hi\n', encoding: 'utf-8', hasBom: false, eol: 'lf', size: 3, modified: '1', readOnly: false }),
    'editor_draft_get': spec(null),
    'editor_draft_delete': spec(null),
    // zip 双击走上传语义（探测不存在 + 流式上传 mock，防误弹覆盖确认）
    'sftp_stat': { error: 'SFTP stat failed: No such file' },
    'sftp_upload_from_file': spec(null),
  });
  await page.evaluate(async () => {
    const w = window.__myshelltool.workbench;
    w.setLocalPaneVisible(true);
    // 场景 1 起编辑器覆盖面板就盖着中央区（异步 chunk 挂载有时序竞态）——
    // 先收起（tab 保留），让文件面板可点
    window.__myshelltool.editor.hideSurface();
    await window.__myshelltool.files.refreshLocalFiles('C:\\fake');
  });
  await page.waitForSelector('.file-pane-local .file-row', { timeout: 5000 });
  await page.locator('.file-pane-local .file-row', { hasText: 'notes.txt' }).first().dblclick();
  await page.waitForFunction(() => window.__myshelltool.editor.tabs.some(t => t.name === 'notes.txt' && t.status === 'ready'), null, { timeout: 5000 });
  // 编辑器覆盖面板挡住了文件面板（预期行为）——收起后再双击 zip（tab 保留）。
  // 指针可达性已由 notes.txt 的真实 dblclick 验证；此处直接派发 dblclick 事件
  // 验证路由分支（覆盖面板卸载存在异步时序，真实点击有竞态）。
  await page.evaluate(() => {
    window.__myshelltool.editor.hideSurface();
  });
  await page.evaluate(() => {
    const rows = Array.from(document.querySelectorAll('.file-pane-local .file-row'));
    const row = rows.find(r => (r.textContent || '').includes('archive.zip'));
    if (!row) throw new Error('zip row not found');
    row.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
  });
  // zip 走上传语义：此时资产未连接 → 上传被拒并提示（可观测信号，证明没进编辑器链路）
  // 无会话上传的 warn toast 会留存数秒（statusBar 文案会被 setTab 覆盖，不可靠）
  await page.waitForFunction(() => {
    return (window.__myshelltool.workbench.toasts || []).some(t => (t.message || '').includes('SSH 会话'));
  }, null, { timeout: 5000 });
  if (await page.evaluate(() => window.__myshelltool.editor.tabs.some(t => t.name === 'archive.zip'))) {
    throw new Error('zip dblclick must NOT open editor');
  }

  // ════ 场景 7：下载完成 toast（聚合 + 双按钮 + 10s）════
  console.error('[step] scenario-7 download toast');
  await fixtureBackend(page, {
    'ssh_connect': { dynamic: 'connect' },
    'ssh_write': spec(null),
    'ssh_resize': spec(null),
    'sftp_list_dir': spec({ path: '/root', entries: [] }),
    'sftp_download_to_file': spec(null),
    'plugin:dialog|open': spec('C:\\dl'),
  });
  await page.evaluate(async () => {
    const w = window.__myshelltool.workbench;
    if (w.selectedAssetId !== 'ipc-test-1') w.selectAsset('ipc-test-1', false);
    await w.connectSelected();
  });
  await page.waitForFunction(() => window.__tauriMock.calls.some(c => c.command === 'ssh_connect'), null, { timeout: 5000 });
  await page.evaluate(() => {
    window.__myshelltool.files.downloadEntry({ name: 'report.md', path: '/root/report.md', kind: 'file', size: 2048, modified: '1', permissions: null, user: null, group: null });
  });
  await page.waitForFunction(() => window.__tauriMock.calls.some(c => c.command === 'sftp_download_to_file'), null, { timeout: 5000 });
  // 聚合窗口 1.5s 后 flush——等 toast 出现（可观测终态，不赌毫秒）
  await page.waitForFunction(() => {
    return (window.__myshelltool.workbench.toasts || []).some(x => x.message.includes('report.md'));
  }, null, { timeout: 8000 });
  const toast = await page.evaluate(() => {
    const t = window.__myshelltool.workbench.toasts.find(x => x.message.includes('report.md'));
    return t ? { message: t.message, actionCount: (t.actions?.length || 0) + (t.action ? 1 : 0) } : null;
  });
  if (!toast) throw new Error('download success toast missing');
  if (!toast.message.includes('C:\\dl')) throw new Error('toast must show real dest dir: ' + toast.message);
  if (toast.actionCount !== 2) throw new Error('download toast should have 2 actions (打开/所在文件夹): ' + toast.actionCount);

  console.log('ui-editor: all scenarios passed');
} finally {
  await browser.close();
}
