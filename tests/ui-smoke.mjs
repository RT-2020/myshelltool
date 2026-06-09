import { chromium } from 'playwright';

const baseUrl = process.env.MYSHELLTOOL_BASE_URL ?? 'http://127.0.0.1:5175/';

const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  await page.goto(baseUrl, { waitUntil: 'networkidle' });
  await page.evaluate(() => localStorage.clear());
  await page.reload({ waitUntil: 'networkidle' });

  const title = await page.title();
  if (!title.includes('myshelltool')) throw new Error(`unexpected title: ${title}`);

  const windowBox = await page.locator('.window').boundingBox();
  if (!windowBox) throw new Error('window shell missing');
  if (Math.abs(windowBox.width - 1180) > 2 || Math.abs(windowBox.height - 760) > 2) {
    throw new Error(`unexpected window size: ${windowBox.width}x${windowBox.height}`);
  }
  const centeredX = Math.abs(windowBox.x - (1440 - windowBox.width) / 2) <= 2;
  const centeredY = Math.abs(windowBox.y - (1000 - windowBox.height) / 2) <= 2;
  if (!centeredX || !centeredY) throw new Error(`window is not centered: ${windowBox.x},${windowBox.y}`);

  await page.click('#themeToggle');
  const theme = await page.locator('html').getAttribute('data-theme');
  if (theme !== 'light') throw new Error(`theme toggle failed: ${theme}`);

  await page.click('#assetToggle');
  const assets = await page.locator('html').getAttribute('data-assets');
  if (assets !== 'collapsed') throw new Error(`asset toggle failed: ${assets}`);

  await page.waitForFunction(() => document.getElementById('backendStatus')?.textContent?.includes('browser-preview'));
  const backendStatus = await page.locator('#backendStatus').textContent();
  if (!backendStatus?.includes('已连接 · browser-preview')) {
    throw new Error(`backend fallback status missing: ${backendStatus}`);
  }
  const assetSource = await page.locator('#assetSource').textContent();
  if (!assetSource?.includes('browser-preview local assets') || !assetSource.includes('8 项')) {
    throw new Error(`asset fallback source missing: ${assetSource}`);
  }

  await page.click('[data-tab="terminal"]');
  if (!(await page.locator('[data-panel="terminal"].active').count())) {
    throw new Error('terminal tab not active');
  }

  await page.click('[data-panel="terminal"].active [data-tab-target="files"]');
  if (!(await page.locator('[data-panel="files"].active').count())) {
    throw new Error('files tab target failed');
  }

  await page.click('#assetToggle');
  const expandedAssets = await page.locator('html').getAttribute('data-assets');
  if (expandedAssets !== 'expanded') throw new Error(`asset expand failed: ${expandedAssets}`);

  await page.fill('#connectionFilter', 'redis');
  const visibleHosts = await page.locator('[data-host]:visible').count();
  if (visibleHosts !== 1) throw new Error(`connection filter expected 1 visible host, got ${visibleHosts}`);

  await page.click('[data-host*="cache-redis"]');
  const contextTitle = await page.locator('#contextTitle').textContent();
  if (!contextTitle?.includes('cache-redis-02')) throw new Error(`context update failed: ${contextTitle}`);

  await page.fill('#connectionFilter', '');
  await page.click('[data-asset-create]');
  if (!(await page.locator('#modalLayer.open').count())) {
    throw new Error('asset editor modal did not open');
  }
  await page.fill('[data-asset-field="name"]', 'qa-local-dev');
  await page.fill('[data-asset-field="host"]', '192.168.56.24');
  await page.fill('[data-asset-field="username"]', 'qa');
  await page.fill('[data-asset-field="group"]', '测试环境');
  await page.fill('[data-asset-field="tags"]', 'qa, local');
  await page.click('#modalPrimary');
  await page.waitForFunction(() => document.getElementById('assetSource')?.textContent?.includes('9 项'));
  await page.fill('#connectionFilter', 'qa-local');
  const qaHosts = await page.locator('[data-host]:visible').count();
  if (qaHosts !== 1) throw new Error(`created asset filter expected 1 visible host, got ${qaHosts}`);
  await page.click('[data-host*="qa-local"]');
  const qaContextTitle = await page.locator('#contextTitle').textContent();
  if (!qaContextTitle?.includes('qa-local-dev')) throw new Error(`created asset context missing: ${qaContextTitle}`);
  await page.reload({ waitUntil: 'networkidle' });
  await page.waitForFunction(() => document.getElementById('assetSource')?.textContent?.includes('9 项'));
  await page.fill('#connectionFilter', 'qa-local');
  const persistedHosts = await page.locator('[data-host]:visible').count();
  if (persistedHosts !== 1) throw new Error(`persisted asset expected 1 visible host, got ${persistedHosts}`);

  await page.click('[data-modal="tokenConfig"]');
  if (!(await page.locator('#modalLayer.open').count())) {
    throw new Error('token modal did not open');
  }
  const modalText = await page.locator('#modalBody').textContent();
  if (!modalText?.includes('本地安全存储')) {
    throw new Error('token modal missing local secure storage text');
  }
  await page.fill('[data-sync-token]', 'preview-token-for-smoke-test');
  await page.click('#modalPrimary');
  const tokenStatus = await page.locator('[data-token-storage-status]').textContent();
  if (!tokenStatus?.includes('本地安全存储：已配置')) {
    throw new Error(`token storage status missing: ${tokenStatus}`);
  }
  const tokenValue = await page.locator('[data-sync-token]').inputValue();
  if (tokenValue) throw new Error('token input was not cleared after save');
  await page.keyboard.press('Escape');

  await page.click('[data-tab="tunnels"]');
  await page.click('tr[data-tunnel-row]:has-text("admin-remote") [data-toggle-tunnel]');
  const status = await page.locator('tr[data-tunnel-row]:has-text("admin-remote") [data-status-pill]').textContent();
  if (!status?.includes('Running')) throw new Error(`tunnel toggle failed: ${status}`);

  console.log('UI smoke test passed');
} finally {
  await browser.close();
}
