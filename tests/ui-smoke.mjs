// UI smoke test (post-US-001/US-005 rewrite)
//
// 旧的 ui-smoke.mjs 和 ui-extended.mjs 强依赖被废弃的浏览器假后端
// （localStorage 资产持久化、sessionStorage token 模拟等）。ADR v3 R8
// 修订明确废弃浏览器预览模式，US-001 已删除 src/services/backend.js
// 的假后端函数。
//
// 真正的 Tauri runtime E2E 需要 tauri-driver + 真实 SSH 服务器矩阵，
// 是项目级后续工作（ADR v3 第 7 节 Follow-ups 已列入 P3）。
// 本文件作为最小 smoke：验证 Vite dev server 启动 + desktop-only-banner
// 在非 Tauri runtime 下显示，让 CI 至少能验证前端 bundle 不崩。

import { chromium } from 'playwright';

const baseUrl = process.env.MYSHELLTOOL_BASE_URL ?? 'http://127.0.0.1:41234/';

const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  await page.goto(baseUrl, { waitUntil: 'networkidle' });

  const title = await page.title();
  if (!title.includes('myshelltool')) throw new Error(`unexpected title: ${title}`);

  // Desktop-only banner 应当显示（Vite dev server 不是 Tauri runtime）
  await page.waitForSelector('.desktop-only-banner', { timeout: 5000 });
  const bannerText = await page.locator('.desktop-only-banner').textContent();
  if (!bannerText?.includes('桌面客户端模式未启动')) {
    throw new Error(`desktop-only-banner text missing or wrong: ${bannerText}`);
  }

  // Window shell 应当仍然渲染
  const windowBox = await page.locator('.window').boundingBox();
  if (!windowBox) throw new Error('window shell missing');

  console.log('UI smoke test passed (Tauri-runtime-gated smoke)');
} finally {
  await browser.close();
}
