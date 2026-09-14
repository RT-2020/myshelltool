#!/usr/bin/env node
// gen-changelog-history.mjs — 遍历全部 v* tag，生成应用内更新日志数据源
// src/generated/changelog.json（设置面板「更新日志」区块离线可读，零网络依赖）。
//
// 用法：node scripts/gen-changelog-history.mjs（npm run build 首步自动跑）
//
// 数据流：本脚本在构建时生成 → 产物提交进 git（dev/tauri:dev 不走 build 也有文件）；
// 发版 CI（release.yml）checkout 到 tag 后跑 npm run tauri:build → beforeBuildCommand
// 走 npm run build → 重新生成含最新 tag 的全历史 → 打进安装包。因此 git 里这份
// 「落后一版」没关系，安装包里的永远是构建时刻的完整版。
//
// 空区间 tag（区间内无 commit）跳过并 warn，不让单条坏数据阻断整个构建。

import { execSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolvePrevTag, collectNotes } from './gen-changelog.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');
const outFile = resolve(root, 'src/generated/changelog.json');

// v 开头 + 至少一段数字的 tag 才算版本 tag（v0.15.1 / v1.2 形态）。
const isVersionTag = (tag) => /^v\d+(\.\d+)*$/.test(tag);

// 语义化排序比较：按 '.' 分段数字比较（无 semver 依赖，tag 形态规整）。
const compareVersionTag = (a, b) => {
  const pa = a.slice(1).split('.').map(Number);
  const pb = b.slice(1).split('.').map(Number);
  const len = Math.max(pa.length, pb.length);
  for (let i = 0; i < len; i++) {
    const da = pa[i] ?? 0;
    const db = pb[i] ?? 0;
    if (da !== db) return da - db;
  }
  return 0;
};

// tag 指向 commit 的日期（committer date，YYYY-MM-DD）——annotated tag 用
// `git log -1 <tag>` 也会解析到 commit；拿不到时留空（仅展示用，不阻断）。
function tagDate(tag) {
  try {
    return execSync(`git log -1 --format=%cs ${tag}`, {
      cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe']
    }).trim();
  } catch {
    return '';
  }
}

// pattern 不加引号：execSync 在 Windows 经 cmd.exe 执行，单引号不会被 shell
// 剥离（'v*' 连引号传给 git → 匹配不到任何 tag）；cmd.exe 也不做 glob 展开，
// 裸 v* 在 cmd 与 POSIX sh 下都原样到达 git，行为一致。
const tags = execSync('git tag -l v*', { cwd: root, encoding: 'utf8' })
  .split('\n').map(t => t.trim()).filter(Boolean).filter(isVersionTag)
  .sort(compareVersionTag);

const entries = [];
for (const tag of tags) {
  // 与 release.yml 同口径：区间 = 上个 tag（不含）→ 本 tag。~1 父提交语法
  // 防 Windows cmd.exe 吃 ^ 转义符（见 gen-changelog.mjs resolvePrevTag 注释）。
  const fromTag = resolvePrevTag(`${tag}~1`);
  let notes;
  try {
    notes = collectNotes({ fromTag, toRef: tag, title: tag });
  } catch (e) {
    console.error(`❌ 生成 ${tag} 说明失败：${e.message}`);
    process.exit(1);
  }
  if (notes === null) {
    console.warn(`⚠ ${fromTag ? `${fromTag}..` : '仓库起点..'}${tag} 区间无 commit，跳过该版本`);
    continue;
  }
  entries.push({ version: tag.slice(1), tag, date: tagDate(tag), notes });
}

if (entries.length === 0) {
  console.warn('⚠ 没有任何可用的版本条目——changelog.json 不更新（保留旧文件，避免空数据覆盖）');
  process.exit(0);
}

mkdirSync(dirname(outFile), { recursive: true });
// 最新版本在前（前端按序渲染，当前版本匹配靠 entries 里的 version 字段而非下标）。
const latest = entries[entries.length - 1]; // reverse 前先记：push 顺序从旧到新
const payload = { generatedAt: new Date().toISOString(), entries: entries.reverse() };
writeFileSync(outFile, JSON.stringify(payload, null, 2) + '\n', 'utf8');
console.log(`✅ 已写入 src/generated/changelog.json（${entries.length} 个版本，最新 ${latest.tag}）`);
