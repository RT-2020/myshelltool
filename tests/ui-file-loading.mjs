// UI file loading test
//
// Verifies that slow remote file operations show an explicit loading overlay.
// This mocks the Tauri bridge and delays the directory listing command, so the
// test covers the real Pinia action and FileColumn overlay without requiring a
// real SSH server.

import { chromium } from 'playwright';

const baseUrl = process.env.MYSHELLTOOL_BASE_URL ?? 'http://127.0.0.1:41234/';

const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });

  await page.addInitScript(() => {
    const remoteEntries = [
      {
        name: 'app.log',
        path: '/srv/app/releases/app.log',
        kind: 'file',
        size: 128,
        modified: '1710000000',
        permissions: '644',
        user: 'deploy',
        group: 'deploy'
      }
    ];

    window.__MST_FILE_LOADING_MOCK = {
      listStarted: 0,
      downloadStarted: 0,
      releaseList: null,
      releaseDownload: null,
      invokeCalls: []
    };

    window.__TAURI__ = {
      core: {
        invoke: (cmd, args = {}) => {
          window.__MST_FILE_LOADING_MOCK.invokeCalls.push({ cmd, args });

          if (cmd === 'backend_status') return Promise.resolve({ ready: true, mode: 'tauri-core' });
          if (cmd === 'get_credential_status') return Promise.resolve({ exists: false });
          if (cmd === 'tunnel_list') return Promise.resolve([]);
          if (cmd === 'list_connection_assets') {
            return Promise.resolve({
              source: 'mock',
              count: 1,
              groups: [],
              assets: [
                {
                  id: 'slow-host',
                  name: 'Slow SSH',
                  host: 'slow.example.test',
                  port: 22,
                  username: 'deploy',
                  auth_method: 'Password',
                  group: '未分组',
                  tags: ['web'],
                  status: 'Idle',
                  last_connected: '从未',
                  credential_id: 'slow-host:password',
                  passphrase_credential_id: null,
                  private_key_path: null
                }
              ]
            });
          }
          if (cmd === 'ssh_connect') {
            return Promise.resolve({ session_id: 'session-slow-host' });
          }
          if (cmd === 'sftp_list_dir') {
            window.__MST_FILE_LOADING_MOCK.listStarted += 1;
            return new Promise(resolve => {
              window.__MST_FILE_LOADING_MOCK.releaseList = () => resolve({
                path: args.path || '/srv/app/releases',
                entries: remoteEntries
              });
            });
          }
          // 目录选择对话框：下载已改为「先选保存目录，再由后端流式写入该目录下的
          // 同名文件」，因此必须先应答 dialog|open(directory)，runDownload 才会发出
          // 下载命令。
          if (cmd === 'plugin:dialog|open') {
            return Promise.resolve('C:\\Users\\tester');
          }
          // 下载命令不再返回文件字节（后端直接写本地文件），只需挂起以观察
          // 「传输进行中不阻塞文件列表浏览」这一 S2 行为。
          if (cmd === 'sftp_download_to_file') {
            window.__MST_FILE_LOADING_MOCK.downloadStarted += 1;
            return new Promise(resolve => {
              window.__MST_FILE_LOADING_MOCK.releaseDownload = () => resolve(null);
            });
          }
          if (cmd === 'fs_local_home_dir') return Promise.resolve('C:\\Users\\tester');
          if (cmd === 'fs_local_list_dir') return Promise.resolve({ path: 'C:\\Users\\tester', entries: [] });
          return Promise.resolve(null);
        }
      },
      event: {
        listen: () => Promise.resolve(() => {})
      },
      webviewWindow: { getCurrentWebviewWindow: () => ({ listen: undefined }) },
      window: { getCurrentWindow: () => ({ listen: undefined }) }
    };
  });

  await page.goto(baseUrl, { waitUntil: 'networkidle' });

  await page.waitForFunction(
    () => document.querySelector('#app')?.__vue_app__?.config?.globalProperties?.$pinia?._s?.has('workbench'),
    { timeout: 5000 }
  );

  await page.evaluate(() => {
    const pinia = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia;
    // v0.20：刷新已无「无会话回落独立连接」分支（断开后点刷新静默重连属缺陷，
    // 已删除）——本测试验证的列表 loading/busy 禁用现在只属于会话分支，注入
    // 一个最小 connected 会话（refreshRemoteFiles 只消费 sessionId/asset.id）。
    pinia._s.get('sessions').sessions.push({
      sessionId: 'session-slow-host',
      status: 'mock-ready',
      asset: { id: 'slow-host', name: 'Slow SSH', host: 'slow.example.test', port: 22, username: 'deploy' }
    });
    pinia._s.get('workbench').setTab('files');
  });

  await page.waitForSelector('.file-pane-remote', { timeout: 5000 });
  const alreadyLoading = await page.evaluate(() => window.__MST_FILE_LOADING_MOCK.listStarted > 0);
  if (!alreadyLoading) {
    await page.getByLabel('刷新远程目录').click();
  }

  await page.waitForFunction(() => window.__MST_FILE_LOADING_MOCK.listStarted > 0, { timeout: 5000 });

  const overlay = page.locator('.file-pane-remote .file-loading-overlay').first();
  await overlay.waitFor({ state: 'visible', timeout: 5000 });
  const overlayText = await overlay.textContent();
  if (!overlayText?.includes('正在读取远程目录')) {
    throw new Error(`remote loading overlay text missing or wrong: ${overlayText}`);
  }

  const refreshDisabled = await page.getByLabel('刷新远程目录').isDisabled();
  if (!refreshDisabled) throw new Error('remote refresh button should be disabled while remote files load');

  await page.evaluate(() => window.__MST_FILE_LOADING_MOCK.releaseList?.());
  await overlay.waitFor({ state: 'hidden', timeout: 5000 });

  await page.evaluate(() => {
    const pinia = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia;
    const workbench = pinia._s.get('workbench');
    const sessions = pinia._s.get('sessions');
    const files = pinia._s.get('files');
    const asset = workbench.selectedAsset;
    sessions.sessions.push({
      sessionId: 'session-slow-host',
      asset,
      status: 'mock-ready',
      createdAt: Date.now()
    });
    sessions.activeSessionId = 'session-slow-host';
    files.downloadEntry(files.remoteEntries[0]);
  });
  await page.waitForFunction(() => window.__MST_FILE_LOADING_MOCK.downloadStarted > 0, { timeout: 5000 });

  // S2 行为变更：传输与浏览 busy 解耦——下载期间文件列表可继续浏览，
  // 不再显示阻塞性 loading 遮罩；进行中状态改由传输队列（状态栏胶囊/抽屉）表达。
  await page.waitForFunction(() => {
    const files = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('files');
    return files.activeTransfers.length > 0;
  }, { timeout: 5000 });
  if (!(await overlay.isHidden())) {
    throw new Error('download must NOT lock the file list with a blocking overlay (S2 decoupled transfer busy from browse busy)');
  }

  await page.evaluate(() => window.__MST_FILE_LOADING_MOCK.releaseDownload?.());
  await page.waitForFunction(() => {
    const files = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('files');
    return files.activeTransfers.length === 0;
  }, { timeout: 5000 });

  console.log('File loading UI test passed');
} finally {
  await browser.close();
}
