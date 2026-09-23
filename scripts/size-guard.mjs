#!/usr/bin/env node
/**
 * size-guard —— 文件大小红线机械门禁（AGENTS.md「质量红线」限额表）
 *
 * 背景：限额原靠 docs/architecture-log.md 手工追踪，2026-09 已两次回潮
 * （09-13「零命中」声明后被三轮功能迭代重新推过线，SettingsPanelContent.vue
 * 从未被跟踪）。本脚本把限额变成 ratchet（棘轮，只进不退）：
 *   - 表外文件越过硬上限 → 失败（新增超标即拦截）；
 *   - RATCHET 表内的存量超标文件**只许缩不许涨**（超过登记基线 → 失败）；
 *   - 表内文件缩回硬上限以内 → 也失败，提示从表中除名（锁定拆分成果，防回弹）；
 *   - 表内文件消失（改名/删除）→ 失败，提示同步除名（防豁免表腐化成空许可）。
 * 软警告区（≥软线未过硬线）只打印不失败，供拆分排期参考。
 *
 * 随 `npm run build` 执行 → CI 与发版自动生效（与 fact-guards 同链路）。
 *
 * 用法：
 *   node scripts/size-guard.mjs              扫描全部限额类别
 *   node scripts/size-guard.mjs --self-test  只跑判定逻辑自检
 */
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url));

/**
 * 限额类别。覆盖复杂度集中处；有意不覆盖：
 * - src/types/**：类型声明文件，行数≠复杂度；
 * - src/generated/**：构建产物（changelog.json）；
 * - tests/、scripts/：测试与工具脚本不进限额。
 */
const CATEGORIES = [
  { name: 'vue', roots: ['src'], exts: new Set(['.vue']), hard: 500, soft: 300 },
  { name: 'store-ts', roots: ['src/stores'], exts: new Set(['.ts']), hard: 500, soft: 300 },
  // lib/composables/services 原不在限额表内（2026-09 评审发现的监控盲区）：
  // filePanel 758 / fileTransfers 673 / sessionHandoff 629 已按同一 500 尺度超标。
  { name: 'lib-ts', roots: ['src/lib', 'src/composables', 'src/services'], exts: new Set(['.ts']), hard: 500, soft: 300 },
  { name: 'rust', roots: ['src-tauri/src', 'crates/myshelltool-core/src'], exts: new Set(['.rs']), hard: 800, soft: 400 }
];

/**
 * 存量超标豁免表（ratchet 基线，2026-09-22 实测 wc -l）。
 * 每条必须带拆分去向；行数只许下调（拆小了就收紧基线），不许上调。
 * 拆回硬上限内的文件必须从此表除名——本脚本会对「表内但已达标」报错。
 */
const RATCHET = {
  // v0.20 双因子修复（2026-09-23 真机验收）后 832 行——认证链（resolved_password
  // 解析 + 顺序因子策略）随修复扩容。拆分去向：认证链抽 ssh/session_auth.rs
  // （与 keyboard.rs 同级先例），下轮触碰 session.rs 时执行。
  'src-tauri/src/ssh/session.rs': { baseline: 832, note: '认证链抽 session_auth.rs' },
};

function toRel(abs) {
  return path.relative(ROOT, abs).split(path.sep).join('/');
}

/** 与 wc -l 语义一致：统计换行符数（文件结尾无换行时补足最后一行）。 */
export function countLines(content) {
  if (content === '') return 0;
  const lines = content.split(/\r?\n/);
  if (lines[lines.length - 1] === '') lines.pop();
  return lines.length;
}

function categoryFor(rel) {
  const ext = path.extname(rel);
  for (const cat of CATEGORIES) {
    if (!cat.exts.has(ext)) continue;
    if (cat.roots.some(root => rel === root || rel.startsWith(root + '/'))) return cat;
  }
  return null;
}

/**
 * 判定一个文件的行数。返回：
 *   { status: 'ok' }
 *   { status: 'soft' }                                   软警告（不失败）
 *   { status: 'over', ... }                              表外超标（失败）
 *   { status: 'grew', ... } / { status: 'shrank' }       表内涨/缩（涨失败、缩提示）
 *   { status: 'stale-passed' }                           表内但已回落限内（失败，除名锁定成果）
 */
export function judge(rel, lineCount) {
  const cat = categoryFor(rel);
  if (!cat) return { status: 'ok', reason: 'no-category' };
  const exempt = RATCHET[rel];
  if (exempt) {
    if (lineCount <= cat.hard) {
      return { status: 'stale-passed', cat, lineCount, baseline: exempt.baseline };
    }
    if (lineCount > exempt.baseline) {
      return { status: 'grew', cat, lineCount, baseline: exempt.baseline, plan: exempt.plan };
    }
    if (lineCount < exempt.baseline) {
      return { status: 'shrank', cat, lineCount, baseline: exempt.baseline };
    }
    return { status: 'ok', reason: 'ratchet-held', cat, lineCount };
  }
  if (lineCount > cat.hard) return { status: 'over', cat, lineCount };
  if (lineCount >= cat.soft) return { status: 'soft', cat, lineCount };
  return { status: 'ok', cat, lineCount };
}

function selfTest() {
  let failed = 0;
  const check = (desc, actual, expected) => {
    if (actual !== expected) {
      console.log(`  ✗ 自检失败：${desc}——期望 ${expected}，实际 ${actual}`);
      failed += 1;
    }
  };
  check('表外 vue 超硬上限 → over', judge('src/components/Foo.vue', 501).status, 'over');
  check('表外 vue 贴线 → soft', judge('src/components/Foo.vue', 500).status, 'soft');
  check('表外 vue 正常 → ok', judge('src/components/Foo.vue', 299).status, 'ok');
  check('表外 rs 超硬上限 → over', judge('src-tauri/src/foo.rs', 801).status, 'over');
  check('表外 rs 软区 → soft', judge('src-tauri/src/foo.rs', 400).status, 'soft');
  check('lib-ts 在限额内', judge('src/lib/foo.ts', 501).status, 'over');
  check('types 不在限额内', judge('src/types/domain.ts', 9999).status, 'ok');
  // 表内用例动态取表现存项（除名后硬编码路径会失效——表是活文档）
  const [ratchetPath, ratchetEntry] = Object.entries(RATCHET)[0] ?? [];
  if (ratchetPath && ratchetEntry) {
    check('表内持平 → ok', judge(ratchetPath, ratchetEntry.baseline).status, 'ok');
    check('表内涨一行 → grew', judge(ratchetPath, ratchetEntry.baseline + 1).status, 'grew');
    check('表内缩一行 → shrank', judge(ratchetPath, ratchetEntry.baseline - 1).status, 'shrank');
    // 回落限内用例需要该文件类别的硬上限 ≥ baseline-大量——用第一项若 baseline 已超
    // 硬上限则天然成立；否则构造一个远小于上限的值
    const cat = categoryFor(ratchetPath);
    if (cat && ratchetEntry.baseline > cat.hard) {
      check('表内回落限内 → stale-passed', judge(ratchetPath, cat.hard).status, 'stale-passed');
    } else if (cat) {
      check('表内回落限内 → stale-passed', judge(ratchetPath, cat.soft - 1).status, 'stale-passed');
    }
  }
  check('wc 语义：尾换行不多计', countLines('a\nb\n'), 2);
  check('wc 语义：无尾换行补足', countLines('a\nb'), 2);
  check('wc 语义：空文件', countLines(''), 0);
  return failed;
}

function walk(dir, exts, out) {
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch (e) {
    console.error(`⚠ 无法读取目录 ${dir}：${e.message}`);
    return;
  }
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, exts, out);
    else if (exts.has(path.extname(entry.name))) out.push(full);
  }
}

function scan() {
  const files = [];
  for (const cat of CATEGORIES) {
    for (const root of cat.roots) {
      const before = files.length;
      walk(path.join(ROOT, root), cat.exts, files);
      if (files.length === before) {
        console.error(`✗ size-guard：类别 [${cat.name}] 的 root "${root}" 扫描到 0 个文件——目录不存在/已改名？门禁拒绝在空集合上宣称通过。`);
        return 1;
      }
    }
  }

  const violations = [];
  const soft = [];
  const notes = [];
  const seen = new Set();
  for (const abs of files) {
    const rel = toRel(abs);
    const verdict = judge(rel, countLines(fs.readFileSync(abs, 'utf8')));
    if (verdict.cat) seen.add(rel);
    if (verdict.status === 'over') {
      violations.push({ rel, ...verdict, msg: `超硬上限 ${verdict.cat.hard}（类别 ${verdict.cat.name}）` });
    } else if (verdict.status === 'grew') {
      violations.push({ rel, ...verdict, msg: `存量超标文件继续上涨：基线 ${verdict.baseline} → 现 ${verdict.lineCount}。拆分去向：${verdict.plan}` });
    } else if (verdict.status === 'stale-passed') {
      violations.push({ rel, ...verdict, msg: `已回落到硬上限内（${verdict.lineCount} ≤ ${verdict.cat.hard}），请从 RATCHET 表除名以锁定成果（防回弹）` });
    } else if (verdict.status === 'shrank') {
      notes.push(`${rel}：${verdict.baseline} → ${verdict.lineCount}（可把基线收紧到 ${verdict.lineCount}）`);
    } else if (verdict.status === 'soft') {
      soft.push({ rel, ...verdict });
    }
  }
  // 表内文件消失 = 豁免表腐化（改名后旧条目静默失效，新名字即裸奔）
  for (const rel of Object.keys(RATCHET)) {
    if (!fs.existsSync(path.join(ROOT, rel))) {
      violations.push({ rel, msg: 'RATCHET 表内文件已不存在——请同步除名（改名/删除后豁免即成空许可）' });
    }
  }

  console.log(`· 扫描 ${files.length} 个文件（${CATEGORIES.map(c => c.name).join(' / ')}），RATCHET 表 ${Object.keys(RATCHET).length} 条`);
  if (notes.length > 0) {
    console.log(`ℹ 基线可收紧：\n${notes.map(n => `  ${n}`).join('\n')}`);
  }
  if (soft.length > 0) {
    soft.sort((a, b) => b.lineCount - a.lineCount);
    console.log(`ℹ 软警告区（不失败，供拆分排期参考）：\n${soft.map(s => `  ${s.lineCount}  ${s.rel}`).join('\n')}`);
  }
  if (violations.length === 0) {
    console.log('✔ size-guard：全部通过');
    return 0;
  }
  console.log(`\n✗ size-guard：${violations.length} 处违规\n`);
  for (const v of violations) {
    console.log(`  ${v.rel}${v.lineCount != null ? `（${v.lineCount} 行）` : ''}`);
    console.log(`    ${v.msg}\n`);
  }
  console.log('修法：拆分（见 architecture-log.md 的拆分方向），或确属误加表时在 RATCHET 表登记基线与拆分去向（只许缩不许涨）。');
  return 1;
}

console.log('size-guard —— 文件大小红线门禁（AGENTS.md 限额表，ratchet 制）\n');
const failures = selfTest();
if (failures > 0) {
  console.log(`\n判定逻辑自检失败 ${failures} 项——请先修 scripts/size-guard.mjs`);
  process.exit(1);
}
console.log('✔ 判定逻辑自检通过\n');
if (!process.argv.includes('--self-test')) process.exit(scan());
