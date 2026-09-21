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
// 在非 Tauri runtime 下显示。
// 注意：CI 只跑 npm run build（编译门禁），不跑本测试——UI 测试仅本地手动跑
// （先 npm run dev 起服务，再 npm run test:ui）。

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

  // Wave 5: 5 区域可见性断言（AC22）
  const regions = ['titlebar', 'sidebar', 'center-top', 'center-bottom', 'right', 'statusbar'];
  for (const region of regions) {
    const el = page.locator(`[data-region="${region}"]`).first();
    const visible = await el.isVisible().catch(() => false);
    if (!visible) throw new Error(`5-region layout: data-region="${region}" not visible`);
  }

  // Wave 5: 右侧栏资源监控占位（AC22 — 浏览器预览模式显示"需要桌面端"）
  const rmPanel = page.locator('[data-region="resource-monitor"]').first();
  const rmVisible = await rmPanel.isVisible().catch(() => false);
  if (!rmVisible) throw new Error('resource-monitor panel not visible');
  const rmText = await rmPanel.textContent();
  if (!rmText?.includes('需要桌面端')) {
    throw new Error(`resource-monitor should show "需要桌面端" in preview mode (got: ${rmText})`);
  }

  // Wave 5: 运维摘要面板渲染（AC10）
  const opsPanel = page.locator('[data-region="ops-summary"]').first();
  const opsVisible = await opsPanel.isVisible().catch(() => false);
  if (!opsVisible) throw new Error('ops-summary panel not visible');

  // —— 窄视口源头自适应（2026-09-21）：无横向溢出 + 两级自动折叠 ——
  // 红线背景：workbench-shell 曾有 min-width:1280px，<1280 视口 scrollWidth>clientWidth
  // 必溢出裁右栏；契约改为 ≥800 宽自适应（断点见 workbench-shell.narrow.scss /
  // useAdaptiveLayout.ts）。newPage 各自独立 context，localStorage 互不影响。
  for (const vw of [{ width: 960, height: 600 }, { width: 800, height: 560 }]) {
    const np = await browser.newPage({ viewport: vw });
    await np.goto(baseUrl, { waitUntil: 'networkidle' });
    await np.waitForSelector('.desktop-only-banner', { timeout: 5000 });
    // 溢出判定量 shell 自身盒宽而非 scrollWidth：body overflow:hidden 会把视口
    // 传播为不可滚动，documentElement.scrollWidth 恒等于 clientWidth（假阴性，
    // 2026-09-21 实测）。shell 盒宽 > 视口 = 布局地板回潮 = 右栏被裁。
    const noOverflow = await np.evaluate(() => {
      const shell = document.querySelector('.workbench-shell');
      return Boolean(shell) && shell.getBoundingClientRect().width <= window.innerWidth + 0.5;
    });
    if (!noOverflow) throw new Error(`narrow ${vw.width}px: horizontal overflow (shell wider than viewport)`);
    // 关键区域仍可见（右栏 <1024 自动折叠为 0 宽、侧栏 <860 收 44px rail，均不断言可见性）
    for (const region of ['titlebar', 'center-top', 'center-bottom', 'statusbar']) {
      const visible = await np.locator(`[data-region="${region}"]`).first().isVisible().catch(() => false);
      if (!visible) throw new Error(`narrow ${vw.width}px: data-region="${region}" not visible`);
    }
    // 自动折叠态（dataset）：<1024 右栏折叠；<860 侧栏也折叠
    const rightState = await np.evaluate(() => document.documentElement.dataset.right);
    if (rightState !== 'collapsed') {
      throw new Error(`narrow ${vw.width}px: right rail should be auto-collapsed (got: ${rightState})`);
    }
    const sidebarExpected = vw.width < 860 ? 'collapsed' : 'expanded';
    const sidebarState = await np.evaluate(() => document.documentElement.dataset.assets);
    if (sidebarState !== sidebarExpected) {
      throw new Error(`narrow ${vw.width}px: sidebar expected "${sidebarExpected}" (got: ${sidebarState})`);
    }
    await np.close();
  }

  console.log('UI smoke test passed (Tauri-runtime-gated smoke + 5-region + resource-monitor placeholder + narrow-viewport adaptive)');
} finally {
  await browser.close();
}
