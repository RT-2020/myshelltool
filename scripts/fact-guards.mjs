#!/usr/bin/env node
/**
 * fact-guards —— 「靠猜测代替事实 / 硬编码」机械门禁（docs/llm-engineering-guidelines.md §7）
 *
 * 文档是提醒，门禁才是约束：每条规则都对应一次真实事故，规则即该事故的回归防线。
 * 随 `npm run build` 执行 → CI（.github/workflows/ci.yml 跑 npm run build）与发版
 * （tauri beforeBuildCommand = npm run build）自动生效，违规无法合入。
 *
 * 用法：
 *   node scripts/fact-guards.mjs              扫描 src / src-tauri / core / tests / scripts
 *   node scripts/fact-guards.mjs --self-test  只跑规则自检（正反样例）
 *
 * 豁免语法：行尾注释 `// fact-guard:allow <rule-id> <理由≥8字符>`。rule-id 必须与
 * 该行命中的规则一致、理由去空白后 ≥8 字符、且豁免标记必须写在注释里（字符串
 * 字面量内的 allow 标记无效）。每条规则的豁免数量会在输出中统计，供 review 留意。
 * 增长策略（规则先行）：每出现一次新的同类事故，先在本文件加规则 + 正反样例，
 * 再改代码修复——保证同类错误第二次出现会被机器拦下。
 */
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const TARGETS = {
  app: {
    roots: ['src'],
    exts: new Set(['.ts', '.tsx', '.vue', '.js', '.jsx', '.mjs', '.mts', '.cts'])
  },
  rust: {
    roots: ['src-tauri/src', 'crates/myshelltool-core/src'],
    exts: new Set(['.rs'])
  },
  tests: { roots: ['tests'], exts: new Set(['.mjs', '.js', '.ts']) },
  // 门禁自身也守规矩。唯一例外是本文件：正反样例字符串本身就是「故意违规」的
  // 代码文本，扫自己必然自伤；样例的正确性由 --self-test 保障，故排除自身。
  scripts: {
    roots: ['scripts'],
    exts: new Set(['.mjs', '.js', '.ts']),
    exclude: ['scripts/fact-guards.mjs']
  },
  // build.rs 位于 src-tauri 根目录（不在 src-tauri/src 下），单文件 target。
  buildRs: { files: ['src-tauri/build.rs'] }
};

/**
 * 豁免判定（收紧版）：`// fact-guard:allow <rule-id> <理由>`。
 * - 必须出现在注释内（// # /* <!-- 之后）——行内字符串里的 allow 标记不算豁免；
 * - rule-id 必须与该行命中的规则一致（防止从别处复制豁免后静默放行错规则）；
 * - 理由去空白后长度 ≥8（单字符/空理由不算）。
 */
function allowFor(line, ruleId) {
  const m = line.match(/(?:\/\/|#|\/\*|<!--)\s*fact-guard:allow\s+(\S+)\s*(.*)$/);
  if (!m || m[1] !== ruleId) return false;
  return m[2].trim().replace(/\s+/g, '').length >= 8;
}

const RULES = [
  {
    id: 'no-guessed-home-path',
    title: '远程家目录必须由服务器给出，禁止拼接猜测路径',
    fix: '空 path 交给后端 SFTP canonicalize(".") 解析（指南 §7 形态 A）。事故：/home/root 不存在导致目录加载失败。',
    targets: ['app'],
    pattern: /["'`]\/home\//,
    samples: {
      bad: [
        "const p = '/home/' + asset.username;",
        'const p = `/home/${asset.username}`;'
      ],
      good: ['const p = await invokeBackend("sftp_list_dir", { path: "" });']
    }
  },
  {
    id: 'no-drive-hardcoded-system-path',
    title: '禁止硬编码盘符系统路径（黑名单必须覆盖任意盘符）',
    fix: '按 <盘符>:\\windows\\ 等前缀规则匹配任意盘符，而非字面 c:\\windows。事故：系统装 D 盘时保护失守。',
    targets: ['app', 'rust'],
    pattern: /["'`][a-zA-Z]:[\\/]{1,2}(windows|program files|programdata)/i,
    // Rust 单测里的反例路径（assert!(is_sensitive_path("D:/Windows/..."))）是合法用例
    skipLine: /\bassert/,
    samples: {
      bad: [
        'if (p.startsWith("C:\\\\Windows")) return true;',
        'if (p.startsWith(`D:\\\\ProgramData`)) return true;'
      ],
      good: [
        'const SYSTEM_DIRS = ["windows", "program files", "programdata"];',
        'assert!(is_sensitive_path(Path::new("D:/Windows/System32")));'
      ]
    }
  },
  {
    id: 'no-gnu-only-flags',
    title: '禁止 GNU 专有命令选项（BusyBox/BSD 上必挂）',
    fix: 'find -printf → 走 SFTP read_dir；df -B1 → df -k。事故：Alpine/BusyBox 上目录列表整条失败。',
    targets: ['rust'],
    // 引号锚定：解释性注释（含反引号示例）不误报
    pattern: /["'][^"'\n]*(?:-printf|-B1)\b/,
    samples: {
      bad: ['let cmd = format!("find {} -maxdepth 1 -printf %f", q);'],
      good: ['// 曾用 find -printf（GNU-only）已弃用', 'let cmd = "LC_ALL=C df -P -k /";']
    }
  },
  {
    id: 'no-blocking-sleep',
    title: '禁止固定 sleep 充当就绪信号/重试等待（时序猜测）',
    fix: '用协议信号（握手/输出到达）或带超时的真实探测；重试用退避调度器（指南 §7 形态 D）。tokio 的固定 sleep 与 thread::sleep 同罪。',
    targets: ['rust'],
    pattern: /\b(?:thread|tokio::time)::sleep\s*\(/,
    samples: {
      bad: [
        'std::thread::sleep(Duration::from_millis(300));',
        'tokio::time::sleep(Duration::from_millis(300)).await;'
      ],
      good: ['tokio::time::timeout(Duration::from_secs(2), fut).await']
    }
  },
  {
    id: 'locale-pinned-df',
    title: '解析 df 输出必须同行锁 LC_ALL=C（表头随 locale 变化）',
    fix: '命令串加 LC_ALL=C 前缀（如 "LC_ALL=C df -P -k /" 或 "export LC_ALL=C; ..."）。事故：中文 locale 下按英文表头切分，磁盘统计静默归零。',
    targets: ['rust'],
    // 命令起始锚定（行首/引号后/分号管道后）：说明文案（"…（执行 df -h）"）与
    // 注释（`// … (df -P 首行)`）不误报；单测样例数据用 fact-guard:allow 豁免。
    pattern: /(?:^|["'`;|&])\s*df\s+-[a-zA-Z0-9]/,
    unless: /LC_ALL=C/,
    skipLine: /\bassert/,
    samples: {
      bad: [
        'pub const CMD_DISK_USAGE: &str = "df -h";',
        '"uptime; free -m; df -h",'
      ],
      good: [
        'let cmd = "LC_ALL=C df -P -k /";',
        '"查询指定资产的磁盘使用情况（执行 df -h）",',
        '// df anchor: the POSIX header "Filesystem" (df -P 输出首行).'
      ]
    }
  },
  {
    id: 'no-host-only-identity',
    title: '实体身份必须用完整键（username+host+port），部分键=碰撞',
    fix: 'slugify(`${username}-${host}`)（端口非 22 时再入 id）。事故：同主机不同用户名互相覆盖资产且沿用错误凭据。',
    targets: ['app'],
    pattern: /slugify\(\s*[A-Za-z_.]*(?:\?\.)?[A-Za-z_.]*host\s*\)/,
    samples: {
      bad: [
        'const tempId = slugify(suggestion.host);',
        'const tempId = slugify(asset?.host);'
      ],
      good: [
        "const base = slugify(name || host || 'asset');",
        "const id = slugify(item?.name || item?.host || 'asset');"
      ]
    }
  },
  {
    id: 'no-fixed-wait-in-tests',
    title: 'UI 测试禁止固定毫秒等待（flaky 根源：快机器断言早于渲染、慢机器超时）',
    fix: '用 page.waitForFunction / waitForSelector 等待可观测状态到达（DOM 属性、store 公开状态、mock 调用计数），而非赌固定毫秒。事故：waitForTimeout(500) 后断言 invoke 次数，慢 CI 上必 flaky。',
    targets: ['tests'],
    pattern: /waitForTimeout\s*\(/,
    samples: {
      bad: ['await page.waitForTimeout(500);'],
      good: [
        "await page.waitForFunction(() => document.documentElement.dataset.theme === 'dark');"
      ]
    }
  }
];

/**
 * 单行判定。返回 'clean' | 'comment' | 'skip' | 'unless' | 'allowed' | 'violation'。
 * 顺序：注释行 > 跳过 > 命中 > unless 反证 > 豁免 > 违规。
 */
function judgeLine(rule, line) {
  // 注释行不执行任何逻辑，一律不判（规则只约束代码）
  if (/^\s*(\/\/|\/\*|\*)/.test(line)) return 'comment';
  if (rule.skipLine && rule.skipLine.test(line)) return 'skip';
  if (!rule.pattern.test(line)) return 'clean';
  if (rule.unless && rule.unless.test(line)) return 'unless';
  if (allowFor(line, rule.id)) return 'allowed';
  return 'violation';
}

function walk(dir, exts, out) {
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch (e) {
    // 不静默吞错：打印后由调用方的「root 非空断言」兜底（目录缺失 = 0 文件 = exit 1）。
    console.error(`⚠ 无法读取目录 ${dir}：${e.message}`);
    return;
  }
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, exts, out);
    else if (exts.has(path.extname(entry.name))) out.push(full);
  }
}

/** 收集各 target 的文件列表。A1 防线：root 扫到 0 个文件 = 目录改名/拼错/过滤全灭，
 *  必须当场失败——否则门禁会在空集合上宣称「全部通过」而静默失效。 */
function collectFiles() {
  const filesByTarget = {};
  for (const [name, t] of Object.entries(TARGETS)) {
    const files = [];
    if (t.roots) {
      for (const root of t.roots) {
        const before = files.length;
        walk(path.join(ROOT, root), t.exts, files);
        if (files.length === before) {
          console.error(
            `✗ fact-guards：target [${name}] 的 root "${root}" 扫描到 0 个文件——` +
              '目录不存在/已改名，或扩展名过滤全灭。门禁拒绝在空集合上宣称通过。'
          );
          process.exit(1);
        }
      }
    }
    if (t.files) {
      for (const rel of t.files) {
        const abs = path.join(ROOT, rel);
        if (!fs.existsSync(abs)) {
          console.error(`✗ fact-guards：target [${name}] 的单文件 "${rel}" 不存在——路径拼错？`);
          process.exit(1);
        }
        files.push(abs);
      }
    }
    if (t.exclude) {
      const excl = new Set(t.exclude.map(rel => path.join(ROOT, rel)));
      filesByTarget[name] = files.filter(f => !excl.has(f));
    } else {
      filesByTarget[name] = files;
    }
  }
  return filesByTarget;
}

function selfTest() {
  let failed = 0;
  for (const rule of RULES) {
    for (const line of rule.samples.bad) {
      if (judgeLine(rule, line) !== 'violation') {
        console.log(`  ✗ 自检失败 [${rule.id}] 应命中但未命中: ${line}`);
        failed += 1;
      }
    }
    for (const line of rule.samples.good) {
      if (judgeLine(rule, line) === 'violation') {
        console.log(`  ✗ 自检失败 [${rule.id}] 不应命中但命中: ${line}`);
        failed += 1;
      }
    }
  }
  return failed;
}

function scan() {
  const filesByTarget = collectFiles();
  // 每个 target 的文件计数随结果打印——「扫描面缩小」必须在输出里可见
  for (const [name, files] of Object.entries(filesByTarget)) {
    console.log(`· target ${name}：${files.length} 个文件`);
  }

  const violations = [];
  const allowCount = {};
  for (const rule of RULES) {
    for (const target of rule.targets) {
      for (const file of filesByTarget[target]) {
        const lines = fs.readFileSync(file, 'utf8').split(/\r?\n/);
        for (let i = 0; i < lines.length; i += 1) {
          const verdict = judgeLine(rule, lines[i]);
          if (verdict === 'allowed') {
            allowCount[rule.id] = (allowCount[rule.id] ?? 0) + 1;
          } else if (verdict === 'violation') {
            violations.push({ rule, file: path.relative(ROOT, file), line: i + 1, text: lines[i].trim() });
          }
        }
      }
    }
  }

  const exemptions = Object.entries(allowCount).filter(([, n]) => n > 0);
  if (exemptions.length > 0) {
    console.log(`ℹ 豁免统计（review 请留意）：${exemptions.map(([id, n]) => `${id} ${n} 处`).join('；')}`);
  }

  const totalFiles = new Set(Object.values(filesByTarget).flat()).size;
  if (violations.length === 0) {
    console.log(`✔ fact-guards：${RULES.length} 条规则全部通过（扫描 ${totalFiles} 个文件）`);
    return 0;
  }

  console.log(`✗ fact-guards：发现 ${violations.length} 处违规\n`);
  for (const { rule, file, line, text } of violations) {
    console.log(`[${rule.id}] ${rule.title}`);
    console.log(`  ${file}:${line}`);
    console.log(`    ${text}`);
    console.log(`  修法：${rule.fix}\n`);
  }
  console.log('豁免：确属误报/合法用例，可在该行行尾注释加 `// fact-guard:allow <rule-id> <理由≥8字符>`（rule-id 须与命中规则一致）。');
  return 1;
}

const selfTestOnly = process.argv.includes('--self-test');
console.log('fact-guards —— 靠猜测代替事实门禁（指南 §7）\n');
const selfTestFailures = selfTest();
if (selfTestFailures > 0) {
  console.log(`\n规则自检失败 ${selfTestFailures} 项——规则已腐烂，请先修 scripts/fact-guards.mjs`);
  process.exit(1);
}
console.log(`✔ 规则自检通过（${RULES.length} 条规则，正反样例共 ${RULES.reduce((n, r) => n + r.samples.bad.length + r.samples.good.length, 0)} 项）\n`);
if (!selfTestOnly) process.exit(scan());
