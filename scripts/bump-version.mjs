#!/usr/bin/env node
// bump-version.mjs — 一次性把 4 个文件的版本号改成同一个值，并刷新 Cargo.lock。
//
// 用法：
//   node scripts/bump-version.mjs 0.6.0          # bump 到 0.6.0
//   node scripts/bump-version.mjs 0.6.0 --commit # bump + 自动 git commit
//
// 为何要脚本化：4 个文件手动改容易漏（package.json / tauri.conf.json /
// 两个 Cargo.toml），版本漂移会导致 CI 因 lockfile 与 manifest 不符而报错。
// 脚本保证原子一致 + 校验格式 + 可选自动提交，发版 skill 也调用它。

import { readFileSync, writeFileSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');

// ─── 参数解析 ───
const args = process.argv.slice(2);
const doCommit = args.includes('--commit');
const version = args.find(a => !a.startsWith('--'));

if (!version) {
  console.error('用法: node scripts/bump-version.mjs <版本号> [--commit]');
  console.error('示例: node scripts/bump-version.mjs 0.6.0');
  process.exit(1);
}

// 语义化版本格式校验（x.y.z 或 x.y.z-pre.build 等），拼错立即拦下。
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error(`错误: 版本号 "${version}" 不符合 semver 格式（应为 x.y.z，如 0.6.0）`);
  process.exit(1);
}

// ─── 4 个目标文件的精确替换规则 ───
// 每项：文件路径 + 一个把「旧版本占位」替换为新版本的正则。
// 用正则而非全文替换，避免误伤同文件里其他数字。
const targets = [
  {
    file: 'package.json',
    replace: content => content.replace(/("version"\s*:\s*")\d+\.\d+\.\d+[^"]*(")/, `$1${version}$2`)
  },
  {
    file: 'src-tauri/tauri.conf.json',
    replace: content => content.replace(/("version"\s*:\s*")\d+\.\d+\.\d+[^"]*(")/, `$1${version}$2`)
  },
  {
    file: 'src-tauri/Cargo.toml',
    // 只改 [package] name="myshelltool" 下紧跟的 version 行
    replace: content => content.replace(
      /(name\s*=\s*"myshelltool"\s*\n\s*version\s*=\s*")\d+\.\d+\.\d+[^"]*(")/,
      `$1${version}$2`
    )
  },
  {
    file: 'crates/myshelltool-core/Cargo.toml',
    replace: content => content.replace(
      /(name\s*=\s*"myshelltool-core"\s*\n\s*version\s*=\s*")\d+\.\d+\.\d+[^"]*(")/,
      `$1${version}$2`
    )
  }
];

// ─── 执行替换 + 逐个校验是否真的改到了 ───
console.log(`Bump 版本号 → ${version}\n`);
for (const t of targets) {
  const fullPath = resolve(root, t.file);
  const before = readFileSync(fullPath, 'utf8');
  const after = t.replace(before);

  // 校验：替换后内容必须与原文不同，否则说明文件里没匹配到版本字段
  // （字段名变了 / 文件被重构了），立即报错而非静默跳过。
  if (before === after) {
    console.error(`❌ ${t.file}: 未找到版本字段，文件结构可能变了，请人工检查`);
    process.exit(1);
  }

  writeFileSync(fullPath, after);
  console.log(`  ✓ ${t.file}`);
}

// ─── 刷新 Cargo.lock：Rust manifest 版本变了，lockfile 必须同步，否则
// cargo build 会因 lockfile 与 manifest 不符而警告或重算。───
console.log('\n刷新 Cargo.lock...');
try {
  execSync('cargo update -p myshelltool --precise ' + version, {
    cwd: resolve(root, 'src-tauri'),
    stdio: 'inherit'
  });
  execSync('cargo update -p myshelltool-core --precise ' + version, {
    cwd: resolve(root, 'src-tauri'),
    stdio: 'inherit'
  });
  console.log('  ✓ Cargo.lock 已同步');
} catch (e) {
  // cargo update 失败不致命（lockfile 会在下次 build 时自动修正），只提示。
  console.warn('  ⚠ cargo update 失败（不致命，下次 build 会自动修正）：' + e.message);
}

// ─── 可选：自动 git commit ───
if (doCommit) {
  console.log('\n提交改动...');
  try {
    // 只 add 改过的文件，避免把工作区其他未完成的改动一起带进版本提交。
    const files = targets.map(t => t.file).concat(['src-tauri/Cargo.lock', 'crates/myshelltool-core/Cargo.lock']);
    execSync(`git add ${files.map(f => `"${f}"`).join(' ')}`, { cwd: root, stdio: 'inherit' });
    execSync(`git commit -m "chore: bump v${version}"`, { cwd: root, stdio: 'inherit' });
    console.log(`  ✓ 已提交 "chore: bump v${version}"`);
  } catch (e) {
    console.error('  ❌ git 提交失败：' + e.message);
    process.exit(1);
  }
}

console.log(`\n✅ 完成。下一步: git tag v${version} && git push origin master --tags`);
if (!doCommit) {
  console.log('   （本脚本未带 --commit，改动已写盘但未提交，请自行检查后提交）');
}
