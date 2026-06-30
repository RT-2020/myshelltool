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

import { execSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');

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
let toRef = explicitTo;

if (!fromTag) {
  // --for v0.6.0 或默认：找相对于目标的上一个 tag。
  // 用 git describe 拿「离目标最近的 tag」，跳过目标本身。
  try {
    fromTag = execSync(`git describe --tags --abbrev=0 ${forVersion ? forVersion + '^' : 'HEAD^'}`, {
      cwd: root, encoding: 'utf8'
    }).trim();
  } catch {
    // 没有 tag 的情况：从仓库起点开始
    fromTag = null;
  }
}

// ─── 取 commit 列表 ───
// 格式：%s（subject，即 commit 第一行标题）。范围 fromTag..toRef。
const range = fromTag ? `${fromTag}..${toRef}` : toRef;
let commits;
try {
  commits = execSync(
    `git log ${range} --no-merges --pretty=format:"%s"`,
    { cwd: root, encoding: 'utf8' }
  ).trim();
} catch (e) {
  console.error(`❌ 读取 commit 失败（区间 ${range}）：` + e.message);
  process.exit(1);
}

if (!commits) {
  console.error(`⚠ 区间 ${range} 内没有 commit`);
  process.exit(0);
}

const commitLines = commits.split('\n');

// ─── conventional commit 解析 ───
// 匹配 type(scope)?: description。scope 和冒号可选（兼容本项目历史里
// 形如 "feat(sync) PR-4: ..." 这种没有连字符的非标准写法）。
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

// ─── 渲染 markdown ───
const targetVersion = forVersion || (toRef === 'HEAD' ? '（未发布）' : toRef);
const lines = [];
lines.push(`## ${targetVersion}`);
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

const output = lines.join('\n');

// ─── 输出 ───
if (outFile) {
  writeFileSync(resolve(root, outFile), output, 'utf8');
  console.log(`✅ 已写入 ${outFile}`);
} else {
  console.log(output);
}
