#!/usr/bin/env node
/**
 * ci-failure-screenshot —— UI 测试失败后的现场快照（CI 用，best-effort）。
 * 测试失败时 dev server 仍在后台运行，重新打开首页截一张初始状态图，
 * 与 vite-dev.log 一起作为 artifact 上传。价值有限但成本近零；
 * 真正的高价值证据应由各测试脚本自身的断言信息提供。
 */
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const url = process.argv[2] || 'http://127.0.0.1:41234';
const outDir = path.join(ROOT, 'tests', 'screenshots');
fs.mkdirSync(outDir, { recursive: true });

const browser = await chromium.launch();
try {
  const page = await browser.newPage();
  await page.goto(url, { waitUntil: 'load', timeout: 15_000 });
  await page.waitForSelector('body', { timeout: 10_000 });
  const file = path.join(outDir, 'ci-failure.png');
  await page.screenshot({ path: file, fullPage: true });
  console.log(`✔ 失败现场快照已写入 ${path.relative(ROOT, file)}`);
} finally {
  await browser.close();
}
