// editor-large-bench — 大文件编辑器上限实测（一次性 dev 基准，非回归测试）
//
// 目的：为「完整编辑上限能从 2 MiB 放宽到多少」提供实测数据。
// 方法：复用 ui-editor 的脚本化 mock 后端模式，fs_local_read_text 在页面内
//       生成日志风格文本（行长 60~200 混 0.5% 长行 3~8KB），驱动真实
//       EditorSurface + CodeMirror 6，量化：
//   - 打开耗时（openTarget → tab ready，含 rope 构建）
//   - JS 堆峰值（ready 时，源串+doc 并存）与稳定值（释放缓存 + gc 后）
//   - 平滑滚动帧间隔 p50/p95/max、顶→50% 大跳转耗时
//   - 键盘输入期间最大帧间隔（输入事务延迟）
//   - 保存耗时（含 doc.toString() 全文取回 —— 大文件保存的真实成本）
// 边界（如实交底）：mock 跳过 Tauri IPC 序列化与 SFTP 传输（那是常数开销，
//   不影响交互/内存结论）；headless Chromium 与 WebView2 同内核同 V8，量级可比。
//
// 跑法：npm run dev 起服务后 node tests/editor-large-bench.mjs

import { chromium } from 'playwright';

const baseUrl = process.env.MYSHELLTOOL_BASE_URL ?? 'http://127.0.0.1:41234/';

// 档位：plaintext 六档 + json 两档（json 是最重的语言模式之一，看语言开销斜率）
// MYSHELLTOOL_BENCH="1" / "1,32" 可过滤只跑指定 MiB 档（sanity 用）
const ALL_RUNS = [
  { mib: 1, json: false }, { mib: 32, json: false }, { mib: 64, json: false },
  { mib: 100, json: false }, { mib: 200, json: false }, { mib: 300, json: false },
  { mib: 1, json: true }, { mib: 8, json: true }, { mib: 16, json: true }, { mib: 32, json: true },
];
const FILTER = process.env.MYSHELLTOOL_BENCH;
const LANG_FILTER = process.env.MYSHELLTOOL_BENCH_LANG; // 'text' | 'json'
const RUNS = ALL_RUNS.filter(r =>
  (!FILTER || FILTER.split(',').map(s => parseInt(s, 10)).includes(r.mib))
  && (!LANG_FILTER || (LANG_FILTER === 'json') === r.json)
);

const MOCK_INIT = `
window.__tauriMock = {
  calls: [],
  invokeSpecs: {
    backend_status: { value: { ready: true, mode: 'desktop' } },
    tunnel_list: { value: [] },
    get_credential_status: { value: { exists: true } },
    fs_local_home_dir: { value: 'C:\\\\Users\\\\test' },
    resource_monitor_list_active: { value: [] },
    list_connection_assets: { value: { source: 'local', count: 0, assets: [], groups: [] } },
    editor_draft_get: { value: null },
    editor_draft_delete: { value: null },
  },
  bench: { mib: 0, json: false, cache: null, genMs: 0, lines: 0, bytes: 0 },
};
window.__TAURI__ = {
  core: {
    invoke: async (command, args) => {
      const m = window.__tauriMock;
      // write 命令只记摘要，避免几百 MB 的 content 再驻留一份污染内存测量
      if (command === 'fs_local_write_text') {
        m.calls.push({ command, args: { contentLength: args.content.length, encoding: args.encoding, eol: args.eol } });
        return { status: 'written', size: args.content.length, modified: String(Date.now()) };
      }
      m.calls.push({ command, args });
      if (command === 'fs_local_read_text') {
        const b = m.bench;
        if (!b.cache) {
          const t0 = performance.now();
          const target = b.mib * 1024 * 1024;
          if (b.json) {
            // 整体合法的 pretty JSON（数组展开多行）。注意不能用 NDJSON：saveTab 对 json
            // 做整体语法校验，NDJSON 不合法 → 弹「语法检查未通过」对话框 → 无头测试无人应答永久挂起
            const item = i => ({ ts: '2026-10-16T12:00:00.123Z', lvl: 'INFO', w: i % 16, path: '/api/v1/items/' + i, status: 200, ms: i % 999, pad: 'x'.repeat(i % 80) });
            const items = [];
            let est = 0, j = 0;
            while (est < target) {
              const it = item(j);
              est += JSON.stringify(it, null, 2).length + 2;
              items.push(it);
              j++;
            }
            b.cache = JSON.stringify(items, null, 2);
            b.bytes = b.cache.length;
            b.lines = j * 9; // pretty 展开约每对象 9 行（近似，供参考）
          } else {
            const parts = [];
            let bytes = 0, i = 0;
            while (bytes < target) {
              const long = (i % 200 === 0);
              const len = long ? 3000 + (i % 5000) : 60 + (i % 140);
              const line = ('2026-10-16T12:00:00.123 INFO [worker-' + (i % 16) + '] handled path=/api/v1/items/' + i + ' status=200 ms=' + (i % 999) + ' ').padEnd(len, 'x').slice(0, len);
              parts.push(line);
              bytes += line.length + 1;
              i++;
            }
            b.cache = parts.join('\\n');
            b.lines = i;
            b.bytes = bytes;
          }
          b.genMs = performance.now() - t0;
        }
        return { content: b.cache, encoding: 'utf-8', hasBom: false, eol: 'lf', size: m.bench.bytes, modified: '100', readOnly: false };
      }
      const spec = m.invokeSpecs[command];
      if (!spec) throw new Error('mock backend: no handler for ' + command);
      if (spec.error) throw new Error(spec.error);
      return spec.value;
    },
  },
  event: {
    listen: async (name, handler) => {
      (window.__tauriMock.eventHandlers ||= {})[name] ||= [];
      window.__tauriMock.eventHandlers[name].push(handler);
      return () => {};
    },
    emit: async () => {},
  },
  window: { getCurrentWindow: () => ({ label: 'main' }) },
};
`;

const fmt = (v, unit = '') => (v == null ? '-' : (typeof v === 'number' ? Math.round(v) : v) + unit);

async function runOne(browser, cfg) {
  const result = { ...cfg, error: null };
  let page = null;
  try {
    page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
    await page.addInitScript(MOCK_INIT);
    await page.goto(baseUrl, { waitUntil: 'networkidle' });
    await page.waitForFunction(() => window.__myshelltool?.editor, null, { timeout: 15000 });

    // warmup：先开一个 ~1KB tab（触发 EditorSurface/EditorHost chunk 懒加载与组件挂载），
    // 不关闭（close 有 dirty 弹窗路径，避免引入无关复杂度）
    await page.evaluate(() => {
      const b = window.__tauriMock.bench;
      b.mib = 0.001; b.json = false; b.cache = null;
    });
    await page.evaluate(async () => {
      await window.__myshelltool.editor.openTarget({ kind: 'local', assetId: null, path: 'C:\\warmup\\w.txt' });
    });
    await page.waitForFunction(() => window.__tauriMock.calls.some(c => c.command === 'fs_local_read_text'), null, { timeout: 15000 });

    // ── 正式档：重置缓存为目标大小 ──
    await page.evaluate(cfgB => { window.__tauriMock.bench.mib = cfgB.mib; window.__tauriMock.bench.json = cfgB.json; window.__tauriMock.bench.cache = null; }, cfg);

    const name = cfg.json ? 'bench.json' : 'bench.log';
    // 打开 + 等 ready（页面内轮询，60s 上限由外层 waitForFunction 控制）
    const openPromise = page.evaluate(async fileName => {
      const ed = window.__myshelltool.editor;
      const t0 = performance.now();
      await ed.openTarget({ kind: 'local', assetId: null, path: 'C:\\bench\\' + fileName });
      const deadline = performance.now() + 110000;
      while (!(ed.tabs.length >= 2 && ed.tabs[ed.tabs.length - 1].status === 'ready')) {
        if (performance.now() > deadline) throw new Error('tab not ready in 110s');
        await new Promise(r => setTimeout(r, 50));
      }
      const last = ed.tabs[ed.tabs.length - 1];
      // 等 CM DOM 挂上 + 双帧稳定
      while (!document.querySelector('.cm-editor')) {
        if (performance.now() > deadline) throw new Error('cm-editor DOM not mounted');
        await new Promise(r => setTimeout(r, 50));
      }
      await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
      window.__openMs = performance.now() - t0;
      window.__tabId = last.id;
      window.__heapReady = performance.memory ? Math.round(performance.memory.usedJSHeapSize / 1048576) : -1;
      // 源串留在 bench.cache：保存前要用它恢复被键入测试改坏的语法（json 校验拦截会弹窗挂死无头测试）
      window.__orig = window.__tauriMock.bench.cache;
    }, name);
    await Promise.race([
      openPromise,
      new Promise((_, rej) => setTimeout(() => rej(new Error('open timeout 120s')), 120000)),
    ]);
    await page.waitForTimeout(100); // fact-guard:allow no-fixed-wait-in-tests bench指标读取前的渲染稳定等待，上方针对open的120s race已是真等待

    const meta = await page.evaluate(() => ({
      genMs: window.__tauriMock.bench.genMs,
      lines: window.__tauriMock.bench.lines,
      bytes: window.__tauriMock.bench.bytes,
      openMs: window.__openMs,
      heapReady: window.__heapReady,
    }));
    Object.assign(result, meta);

    // ── 滚动：平滑滚动（30s 预算，慢帧档提前止步并如实记录帧数）+ 大跳转 ──
    await page.evaluate(async () => {
      const sc = document.querySelector('.cm-scroller');
      if (!sc) throw new Error('no .cm-scroller');
      window.__scrollHeight = sc.scrollHeight;
      const times = [];
      let running = true;
      const rec = () => { if (!running) return; times.push(performance.now()); requestAnimationFrame(rec); };
      requestAnimationFrame(rec);
      const scrollT0 = performance.now();
      let steps = 0;
      // 每步等一帧，但 10s 等不到（语言模式极慢帧/主线程长任务）就止步，防挂死
      const nextFrame = () => Promise.race([
        new Promise(r => requestAnimationFrame(r)),
        new Promise(r => setTimeout(r, 10000)),
      ]);
      while (steps < 300 && performance.now() - scrollT0 < 30000) {
        sc.scrollTop += 400;
        await nextFrame();
        steps++;
      }
      window.__scrollSteps = steps;
      sc.scrollTop = 0;
      await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
      const t0 = performance.now();
      sc.scrollTop = Math.floor(sc.scrollHeight / 2);
      await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
      window.__jumpMs = performance.now() - t0;
      running = false;
      const gaps = [];
      for (let i = 1; i < times.length; i++) gaps.push(times[i] - times[i - 1]);
      gaps.sort((a, b) => a - b);
      const pick = p => gaps.length ? gaps[Math.min(gaps.length - 1, Math.floor(gaps.length * p))] : 0;
      window.__scroll = { frames: gaps.length, p50: pick(0.5), p95: pick(0.95), max: gaps[gaps.length - 1] || 0 };
    });
    Object.assign(result, await page.evaluate(() => ({
      scrollP50: window.__scroll.p50, scrollP95: window.__scroll.p95, scrollMax: window.__scroll.max,
      scrollSteps: window.__scrollSteps, jumpMs: window.__jumpMs, scrollHeight: window.__scrollHeight,
    })));

    // ── 输入延迟：click 聚焦后连续键入，测最大帧间隔 ──
    await page.click('.cm-content');
    await page.evaluate(() => {
      window.__typeTimes = [];
      window.__typing = true;
      const rec = () => { if (!window.__typing) return; window.__typeTimes.push(performance.now()); requestAnimationFrame(rec); };
      requestAnimationFrame(rec);
    });
    await page.keyboard.type('bench typing 0123456789', { delay: 8 });
    result.typeMaxGap = await page.evaluate(() => {
      window.__typing = false;
      const t = window.__typeTimes;
      let max = 0;
      for (let i = 1; i < t.length; i++) max = Math.max(max, t[i] - t[i - 1]);
      return max;
    });

    // ── 保存：先恢复原文（键入测试会改坏 json 语法 → 校验弹窗挂死无头测试；
    //    replaceContent 的恢复成本在计时外），再计时 saveTab（含语法校验 + doc.toString 全文取回）──
    const dirtyOk = await page.evaluate(() => {
      const ed = window.__myshelltool.editor;
      const tab = ed.tabs.find(t => t.id === window.__tabId);
      return tab ? tab.dirty : false;
    });
    if (dirtyOk) {
      result.saveMs = await page.evaluate(async () => {
        const ed = window.__myshelltool.editor;
        ed.replaceContent(window.__tabId, window.__orig);
        const t0 = performance.now();
        const ok = await ed.saveTab(window.__tabId);
        if (!ok) throw new Error('saveTab returned false; modal=' + JSON.stringify(window.__myshelltool.workbench.modal?.payload?.title ?? null));
        return performance.now() - t0;
      });
    }

    // ── 稳定内存：保存完成后释放源缓存（含 __orig）+ gc ──
    result.heapStable = await page.evaluate(() => {
      window.__tauriMock.bench.cache = null;
      window.__orig = null;
      if (window.gc) { window.gc(); window.gc(); window.gc(); }
      return performance.memory ? Math.round(performance.memory.usedJSHeapSize / 1048576) : -1;
    });
  } catch (err) {
    result.error = String(err.message || err).slice(0, 160);
  } finally {
    await page.close().catch(() => {});
  }
  return result;
}

const browser = await chromium.launch({ headless: true, args: ['--js-flags=--expose-gc'] });
const results = [];
try {
  for (const cfg of RUNS) {
    process.stderr.write(`[run] ${cfg.mib} MiB ${cfg.json ? 'json' : 'plaintext'} ... `);
    // 整档 5 分钟硬兜底：任何环节挂死（极慢语言模式帧等）都强断记 error，不拖垮整批
    const r = await Promise.race([
      runOne(browser, cfg),
      new Promise(resolve => setTimeout(() => resolve({ ...cfg, error: 'hard timeout 300s' }), 300000)),
    ]);
    results.push(r);
    process.stderr.write(r.error ? 'FAILED: ' + r.error + '\n' : 'ok\n');
  }
} finally {
  await browser.close().catch(() => {});
}

// ── 汇总表 ──
const header = ['MiB', 'lang', 'lines', 'genMs', 'openMs', 'heapPk', 'heapSt', 'scrP50', 'scrP95', 'scrMax', 'steps', 'jumpMs', 'typeMax', 'saveMs', 'err'];
const rows = results.map(r => [
  r.mib, r.json ? 'json' : 'text', fmt(r.lines), fmt(r.genMs), fmt(r.openMs), fmt(r.heapReady), fmt(r.heapStable),
  fmt(r.scrollP50), fmt(r.scrollP95), fmt(r.scrollMax), fmt(r.scrollSteps), fmt(r.jumpMs), fmt(r.typeMaxGap), fmt(r.saveMs),
  r.error ? r.error.slice(0, 40) : '',
]);
const widths = header.map((h, i) => Math.max(h.length, ...rows.map(row => String(row[i] ?? '').length)));
const line = (cells) => '| ' + cells.map((c, i) => String(c ?? '').padEnd(widths[i])).join(' | ') + ' |';
console.log(line(header));
console.log('|' + widths.map(w => '-'.repeat(w + 2)).join('|') + '|');
for (const row of rows) console.log(line(row));
console.log('\n单位：时间 ms，内存 MiB（usedJSHeapSize）。scrP50/P95/Max = 平滑滚动帧间隔分位；jumpMs = 顶→50% 大跳转；typeMax = 键入期间最大帧间隔（含 CDP 输入开销的相对值）；saveMs = 小编辑后保存全程（含 doc.toString 全文取回）。heapPk = ready 时（源串+doc 并存），heapSt = 释放源缓存+gc 后。');
