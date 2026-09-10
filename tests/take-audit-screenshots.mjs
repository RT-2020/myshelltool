import { chromium } from 'playwright';
import path from 'node:path';
import fs from 'node:fs';

const outDir = path.resolve('tests/screenshots');
if (!fs.existsSync(outDir)) {
  fs.mkdirSync(outDir, { recursive: true });
}

const baseUrl = process.env.MYSHELLTOOL_BASE_URL ?? 'http://127.0.0.1:41234/';
const browser = await chromium.launch({ headless: true });

try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });

  await page.addInitScript(() => {
    window.__MST_MOCK = {
      listeners: new Map(),
      invokeCalls: [],
    };
    window.__TAURI__ = {
      core: {
        invoke: (cmd, args = {}) => {
          window.__MST_MOCK.invokeCalls.push({ cmd, args });
          if (cmd === 'backend_status') return Promise.resolve({ ready: true, mode: 'tauri-core' });
          if (cmd === 'get_credential_status') return Promise.resolve({ exists: false });
          if (cmd === 'tunnel_list') return Promise.resolve([]);
          if (cmd === 'mcp_status') {
            return Promise.resolve({
              serverName: 'myshelltool-mcp',
              serverVersion: '1.4.0',
              endpoint: 'http://127.0.0.1:41235/mcp',
              dataDir: 'C:\\Users\\tester\\.myshelltool',
              probe: { ok: true, probedAt: Date.now() },
              tools: [
                { name: 'execute_command', description: '执行命令' },
                { name: 'read_file', description: '读取文件' }
              ],
              resources: [],
              prompts: []
            });
          }
          if (cmd === 'sync_status') {
            return Promise.resolve({ configured: true, lastSync: '10 分钟前', gistMask: 'gist_123***' });
          }
          if (cmd === 'list_connection_assets') {
            return Promise.resolve({
              source: 'mock',
              count: 4,
              groups: ['生产环境', '测试环境/Kubernetes'],
              assets: [
                {
                  id: 'host-1',
                  name: 'Prod Web 01',
                  host: '10.0.1.10',
                  port: 22,
                  username: 'root',
                  auth_method: 'Password',
                  group: '生产环境',
                  tags: ['web', 'nginx'],
                  status: 'Connected',
                  last_connected: '刚刚',
                  credential_id: 'host-1:password'
                },
                {
                  id: 'host-2',
                  name: 'Prod DB Master',
                  host: '10.0.1.20',
                  port: 22,
                  username: 'admin',
                  auth_method: 'PrivateKey',
                  group: '生产环境',
                  tags: ['db', 'mysql'],
                  status: 'Idle',
                  last_connected: '2小时前'
                },
                {
                  id: 'host-3',
                  name: 'K8s Worker Node',
                  host: '192.168.1.105',
                  port: 2222,
                  username: 'ubuntu',
                  auth_method: 'Password',
                  group: '测试环境/Kubernetes',
                  tags: ['k8s'],
                  status: 'Warning',
                  last_connected: '昨天'
                },
                {
                  id: 'host-4',
                  name: 'Dev Bastion',
                  host: '192.168.1.2',
                  port: 22,
                  username: 'dev',
                  auth_method: 'Password',
                  group: '未分组',
                  tags: ['gateway'],
                  status: 'Idle',
                  last_connected: '从未'
                }
              ]
            });
          }
          if (cmd === 'ssh_connect') return Promise.resolve({ session_id: 'session-1' });
          if (cmd === 'fs_local_home_dir') return Promise.resolve('D:\\Projects');
          if (cmd === 'fs_local_list_dir') {
            return Promise.resolve({
              path: 'D:\\Projects',
              entries: [
                { name: 'src', kind: 'directory', size: 0, modified: '2026-03-01 10:00' },
                { name: 'package.json', kind: 'file', size: 1024, modified: '2026-03-02 12:30' },
                { name: 'README.md', kind: 'file', size: 2048, modified: '2026-03-02 14:20' }
              ]
            });
          }
          if (cmd === 'sftp_list_dir') {
            return Promise.resolve({
              path: '/var/www/html',
              entries: [
                { name: 'config', kind: 'directory', size: 0, modified: '2026-03-01' },
                { name: 'index.html', kind: 'file', size: 4096, modified: '2026-03-02' },
                { name: 'app.js', kind: 'file', size: 18432, modified: '2026-03-03' }
              ]
            });
          }
          return Promise.resolve(null);
        }
      },
      event: {
        listen: (eventName, handler) => {
          window.__MST_MOCK.listeners.set(eventName, handler);
          return Promise.resolve(() => window.__MST_MOCK.listeners.delete(eventName));
        }
      },
      webviewWindow: { getCurrentWebviewWindow: () => ({ listen: () => {} }) },
      window: { getCurrentWindow: () => ({ listen: () => {} }) }
    };
  });

  await page.goto(baseUrl, { waitUntil: 'networkidle' });
  await page.waitForFunction(
    () => document.querySelector('#app')?.__vue_app__?.config?.globalProperties?.$pinia?._s?.has('workbench'),
    { timeout: 10000 }
  );
  // 状态等待：侧栏区域真实渲染后再截图（替代固定 1s 等待）
  await page.waitForSelector('[data-region="sidebar"]', { timeout: 5000 });

  // 1. 浅色模式 - 默认工作台
  await page.screenshot({ path: path.join(outDir, '01_workbench_light.png') });

  // 2. 切换到深色模式
  await page.evaluate(() => {
    const pinia = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia;
    const ui = pinia._s.get('ui');
    ui.setTheme('dark');
  });
  // 状态等待：主题属性真实落到 <html> 后再截图
  await page.waitForFunction(() => document.documentElement.dataset.theme === 'dark', { timeout: 5000 });
  await page.screenshot({ path: path.join(outDir, '02_workbench_dark.png') });

  // 3. 资产弹窗
  await page.evaluate(() => {
    const pinia = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia;
    const workbench = pinia._s.get('workbench');
    workbench.modal = {
      type: 'assetEditor',
      asset: {
        id: 'host-1',
        name: 'Prod Web 01',
        host: '10.0.1.10',
        port: 22,
        username: 'root',
        authMethod: 'Password',
        group: '生产环境',
        tags: ['web', 'nginx']
      }
    };
  });
  await page.waitForSelector('#modalLayer.open', { timeout: 5000 });
  await page.screenshot({ path: path.join(outDir, '03_modal_asset_dark.png') });

  // 4. 设置弹窗
  await page.evaluate(() => {
    const pinia = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia;
    const workbench = pinia._s.get('workbench');
    workbench.modal = { type: 'settings', tab: 'appearance' };
  });
  // modalLayer 已开（03），settings 弹窗用 store 状态区分等待
  await page.waitForFunction(() => {
    const wb = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('workbench');
    return wb?.modal?.type === 'settings';
  }, { timeout: 5000 });
  await page.screenshot({ path: path.join(outDir, '04_modal_settings_dark.png') });

  // 5. 设置弹窗 - MCP Tab
  await page.evaluate(() => {
    const pinia = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia;
    const workbench = pinia._s.get('workbench');
    workbench.modal = { type: 'settings', tab: 'mcp' };
  });
  await page.waitForFunction(() => {
    const wb = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('workbench');
    return wb?.modal?.type === 'settings' && wb?.modal?.tab === 'mcp';
  }, { timeout: 5000 });
  await page.screenshot({ path: path.join(outDir, '05_modal_settings_mcp_dark.png') });

  // 6. 新建分组弹窗
  await page.evaluate(() => {
    const pinia = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia;
    const workbench = pinia._s.get('workbench');
    workbench.modal = { type: 'createGroup' };
  });
  await page.waitForFunction(() => {
    const wb = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('workbench');
    return wb?.modal?.type === 'createGroup';
  }, { timeout: 5000 });
  await page.screenshot({ path: path.join(outDir, '06_modal_create_group_dark.png') });

  // 7. 关闭弹窗，切回浅色，查看文件面板
  await page.evaluate(() => {
    const pinia = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia;
    const workbench = pinia._s.get('workbench');
    const ui = pinia._s.get('ui');
    workbench.modal = { type: null, asset: null };
    ui.setTheme('light');
  });
  // 状态等待：弹窗已关 + 主题已切回 light，两状态同时成立
  await page.waitForFunction(() => {
    const wb = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('workbench');
    return wb?.modal?.type == null && document.documentElement.dataset.theme === 'light';
  }, { timeout: 5000 });
  await page.screenshot({ path: path.join(outDir, '07_right_monitor_light.png') });

  // 8. 传输抽屉打开
  await page.evaluate(() => {
    const pinia = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia;
    const files = pinia._s.get('files');
    files.transferQueue = [
      { id: 't-1', name: 'app-release-v1.4.0.tar.gz', direction: 'upload', transferred: 52428800, total: 104857600, percent: 50, status: 'running', speed: 1048576, eta: 50 },
      { id: 't-2', name: 'nginx.conf', direction: 'download', transferred: 4096, total: 4096, percent: 100, status: 'done' },
      { id: 't-3', name: 'large-backup.sql', direction: 'upload', transferred: 10485760, total: 524288000, percent: 2, status: 'error', error: '连接超时，请重试' }
    ];
    files.transferDrawerOpen = true;
  });
  // 状态等待：抽屉打开状态真实生效后再截图
  await page.waitForFunction(() => {
    const files = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('files');
    return files?.transferDrawerOpen === true;
  }, { timeout: 5000 });
  await page.screenshot({ path: path.join(outDir, '08_transfer_drawer_light.png') });

  console.log('All screenshots taken successfully');
} finally {
  await browser.close();
}
