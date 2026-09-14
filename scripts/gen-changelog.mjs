#!/usr/bin/env node
// gen-changelog.mjs — 从两个 tag（或 tag → HEAD）之间的 conventional commits
// 生成结构化发布说明（按 feat/fix/其他 分组，中文友好）。
//
// 用法：
//   node scripts/gen-changelog.mjs                              # 上个 tag → HEAD
//   node scripts/gen-changelog.mjs --from v0.5.0                # 指定起点
//   node scripts/gen-changelog.mjs --from v0.5.0 --to v0.6.0    # 指定区间
//   node scripts/gen-changelog.mjs --for v0.6.0                 # 便捷：生成 v0.6.0 说明（自动找上个 tag）
//   node scripts/gen-changelog.mjs --for v0.6.0 -o notes.md     # 写入文件（默认输出到 stdout）
//
// 为何不用 release.yml 里已有的 generate_release_notes:true：
// GitHub 的默认生成是「按 commit 标题平铺 + PR 列表」，对中文 conventional commit
// 不分组、不折叠，发布说明可读性差。本脚本按类型聚合，让用户一眼看清「这次新加了啥 / 修了啥」。
// 输出可作为 GitHub Release body 粘贴，或追加到 CHANGELOG.md。
//
// gen-changelog-history.mjs 复用本文件的 resolvePrevTag / collectNotes 生成
// 全历史 changelog.json（应用内更新日志区块的数据源）；CLI 行为保持不变。

import { execSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');
const isDirectRun = () => {
  try {
    return process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url;
  } catch {
    return false;
  }
};

// ─── 供 history 脚本复用的导出 ───

/**
 * resolvePrevTag(ref) — 找 ref 之前（不含 ref 自身）最近的 tag。
 *
 * 用 git describe 拿「离目标最近的 tag」。用 ~1（父提交）而非 ^：execSync 在
 * Windows 经 cmd.exe 执行会吃掉 ^ 转义符，`<tag>^` 变成 `<tag>` → fromTag 算成
 * tag 自身 → 空区间说明。调用方传 `${tag}~1` 这类带 ~ 的 ref。
 * 返回 tag 名，或 null（无任何更早的 tag = 仓库起点）。
 */
export function resolvePrevTag(ref) {
  try {
    return execSync(`git describe --tags --abbrev=0 ${ref}`, {
      cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe']
    }).trim();
  } catch {
    // stderr 已捕获不透传：仓库首个 tag / ref 不存在时 git 的 fatal 输出
    // 会污染调用方构建日志，而「无前驱」本来就是调用方处理的预期分支。
    return null;
  }
}

// conventional commit type → 中文分组标题。分组的顺序语义见 TYPE_ORDER。
const TYPE_LABELS = {
  feat: '✨ 新功能',
  fix: '🐛 修复',
  perf: '⚡ 性能',
  refactor: '♻️ 重构',
  docs: '📚 文档',
  chore: '🔧 杂项',
  test: '🧪 测试',
  build: '📦 构建',
  ci: '🤖 CI',
  style: '💄 样式'
};
// 分组顺序：重要的在前（feat/fix 优先），让用户先看到核心变化。
const TYPE_ORDER = ['feat', 'fix', 'perf', 'refactor', 'docs', 'test', 'build', 'ci', 'style', 'chore', 'other'];

/**
 * collectNotes({ fromTag, toRef, title }) — 生成一个区间的 markdown 发布说明。
 *
 * fromTag 为 null 时区间 = 仓库起点 → toRef。返回 markdown 字符串；
 * 区间内没有 commit 时返回 null（调用方决定如何处置，CLI 是 warn + exit 0）。
 * git log 失败时抛出（CLI catch 后 exit 1）。
 */
export function collectNotes({ fromTag, toRef, title }) {
  const range = fromTag ? `${fromTag}..${toRef}` : toRef;
  let commits;
  try {
    commits = execSync(
      `git log ${range} --no-merges --pretty=format:"%s"`,
      { cwd: root, encoding: 'utf8' }
    ).trim();
  } catch (e) {
    throw new Error(`读取 commit 失败（区间 ${range}）：${e.message}`);
  }
  if (!commits) return null;

  const commitLines = commits.split('\n');

  // 匹配 type(scope)?: description。scope 和冒号可选（兼容本项目历史里
  // 形如 "feat(sync) PR-4: ..." 这种没有连字符的非标准写法）。
  const groups = {};
  for (const line of commitLines) {
    const m = line.match(/^(\w+)(?:\(([^)]+)\))?\s*[:]?\s*(.+)$/);
    if (!m) {
      (groups.other ||= []).push(line);
      continue;
    }
    let [, type, scope, desc] = m;
    // 容错：type 可能是大写或非标准，归一到小写；不在已知类型里的归 other。
    type = type.toLowerCase();
    if (!TYPE_LABELS[type]) type = 'other';
    const label = scope ? `**${scope}**: ${desc}` : desc;
    (groups[type] ||= []).push(label);
  }

  const lines = [];
  lines.push(`## ${title}`);
  lines.push('');
  lines.push(`_区间: ${fromTag || '仓库起点'} → ${toRef}_`);
  lines.push('');

  let hasContent = false;
  for (const type of TYPE_ORDER) {
    const items = groups[type];
    if (!items || items.length === 0) continue;
    hasContent = true;
    const label = TYPE_LABELS[type] || '📦 其他';
    lines.push(`### ${label}`);
    for (const item of items) {
      lines.push(`- ${item}`);
    }
    lines.push('');
  }

  if (!hasContent) {
    lines.push('_（该区间内没有可识别的 conventional commit）_');
    lines.push('');
  }
  return lines.join('\n');
}

// ─── CLI ───

// 直接执行（node scripts/gen-changelog.mjs）时跑 CLI；被 import 时不跑。
if (isDirectRun()) {
  main();
}

function main() {
  // ─── 参数解析 ───
  const args = process.argv.slice(2);
  const getOpt = (name) => {
    const i = args.indexOf(name);
    return i !== -1 && args[i + 1] ? args[i + 1] : null;
  };
  const outFile = getOpt('-o') || getOpt('--output');
  const forVersion = getOpt('--for');
  const explicitFrom = getOpt('--from');
  const explicitTo = getOpt('--to') || 'HEAD';

  // ─── 确定区间 [from, to] ───
  // 优先级：显式 --from > --for 推导的上个 tag > 自动找的上个 tag。
  let fromTag = explicitFrom;
  const toRef = explicitTo;

  if (!fromTag) {
    // --for v0.6.0 或默认：找相对于目标的上一个 tag。
    fromTag = resolvePrevTag(forVersion ? forVersion + '~1' : 'HEAD~1');
    if (!fromTag && forVersion) {
      // 显式指定的 --for tag 必须真实存在且有自己的上一个 tag——否则把「全历史」
      // 当成发布说明输出还 exit 0，等于静默给错误内容盖章。
      console.error(`❌ 无法解析 --for ${forVersion} 的上一个 tag（tag 不存在，或它就是仓库第一个 tag）`);
      console.error('   如确需从仓库起点生成全历史，请去掉 --for 参数显式确认。');
      process.exit(1);
    }
    // 无显式参数时的隐含回退（新仓库没有任何 tag）：fromTag 保持 null = 仓库起点
  }

  const targetVersion = forVersion || (toRef === 'HEAD' ? '（未发布）' : toRef);

  let output;
  try {
    output = collectNotes({ fromTag, toRef, title: targetVersion });
  } catch (e) {
    console.error(`❌ ${e.message}`);
    process.exit(1);
  }

  if (output === null) {
    const range = fromTag ? `${fromTag}..${toRef}` : toRef;
    console.error(`⚠ 区间 ${range} 内没有 commit`);
    process.exit(0);
  }

  // ─── 输出 ───
  if (outFile) {
    writeFileSync(resolve(root, outFile), output, 'utf8');
    console.log(`✅ 已写入 ${outFile}`);
  } else {
    console.log(output);
  }
}
