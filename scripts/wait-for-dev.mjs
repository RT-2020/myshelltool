#!/usr/bin/env node
/**
 * wait-for-dev —— 等待 Vite dev server 就绪（CI 用）。
 * 轮询真实 HTTP 可达信号，不赌固定毫秒（同 no-fixed-wait 门禁精神）。
 * 用法：node scripts/wait-for-dev.mjs [url]
 */
import process from 'node:process';

const url = process.argv[2] || 'http://127.0.0.1:41234';
const deadline = Date.now() + 90_000;
let attempt = 0;

while (Date.now() < deadline) {
  attempt += 1;
  try {
    const res = await fetch(url, { signal: AbortSignal.timeout(3000) });
    if (res.ok) {
      console.log(`✔ dev server 就绪（${url}，第 ${attempt} 次探测）`);
      process.exit(0);
    }
  } catch {
    // 服务未起/连接被拒：探测-等待-再探测，继续
  }
  await new Promise(r => setTimeout(r, 1000));
}
console.error(`✗ 等待 dev server 超时（${url}，90s 内 ${attempt} 次探测均不可达）`);
process.exit(1);
