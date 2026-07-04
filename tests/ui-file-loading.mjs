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
      releaseList: null,
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
          if (cmd === 'ssh_list_directory' || cmd === 'sftp_list_dir') {
            window.__MST_FILE_LOADING_MOCK.listStarted += 1;
            return new Promise(resolve => {
              window.__MST_FILE_LOADING_MOCK.releaseList = () => resolve({
                path: args.path || '/srv/app/releases',
                entries: remoteEntries
              });
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

  console.log('File loading UI test passed');
} finally {
  await browser.close();
}
