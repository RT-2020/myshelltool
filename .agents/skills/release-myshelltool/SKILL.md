---
name: release-myshelltool
description: 发布 myshelltool（Tauri 2 桌面 SSH 客户端）的新版本。当用户说「发版」「发布新版本」「release」「打 tag 发版」「bump 到 vX.Y.Z」「升级版本并发布」时使用此技能。它把版本号 bump、打 tag、推送触发 GitHub Actions、监控构建、验证产物这一整套流程一次走完，并内置了本项目真实踩过的坑（createUpdaterArtifacts 字段名、.sig 未生成、tag 重打等）的排查指引。只要用户想给本项目发新版本，就用这个技能，不要临时拼凑流程。
---

# 发布 myshelltool 新版本

本项目是 Tauri 2 桌面应用（Windows NSIS），发版链路已配好：
推送 `v*` tag → GitHub Actions（`.github/workflows/release.yml`）自动构建 → 发布 Release。

## 前置条件（已一次性配好，正常发版无需重做）

如需验证或重置，检查这几项：
1. **GitHub Secrets**（仓库 Settings → Secrets and variables → Actions）必须存在：
   - `TAURI_SIGNING_PRIVATE_KEY` — 签名私钥
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — 私钥密码
2. **公钥**已写入 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`。
3. **`bundle.createUpdaterArtifacts: true`** 必须在 tauri.conf.json 里（否则 Tauri 不生成 .sig，见下方「已知坑」）。
4. workflow 文件在 `.github/workflows/{release,ci}.yml`。

---

## 发版主流程（6 步）

发版就是改版本号 → 生成发布说明 → 提交 → 打 tag → 推送 → 等产物。每一步都要做对。

### 步骤 1：bump 版本号（用脚本，一次改 4 个文件 + 刷新 Cargo.lock）

**不要手工逐个改 4 个文件**——用 `scripts/bump-version.mjs`，它保证 4 个文件版本号原子一致 + 自动刷新 Cargo.lock + 校验格式，避免版本漂移。

```bash
node scripts/bump-version.mjs X.Y.Z --commit
# 例如: node scripts/bump-version.mjs 0.6.0 --commit
```

`--commit` 会自动执行 `git add` + `git commit -m "chore: bump vX.Y.Z"`，只 add 版本相关文件（4 个 manifest + 2 个 lockfile），不误带工作区其他未完成改动。

**手动核对（脚本已内置校验，但发版是关键操作，复查一遍）**：
```bash
grep '"version"' package.json src-tauri/tauri.conf.json
grep '^version' src-tauri/Cargo.toml crates/myshelltool-core/Cargo.toml
```
四个值必须完全相同。脚本原理与失败处理见文末「附录：脚本说明」。

### 步骤 2：（可选）生成发布说明

`release.yml` 已开 `generate_release_notes: true`，GitHub 会自动生成基础发布说明。若想要**按 feat/fix 分组的中文结构化说明**，用 `scripts/gen-changelog.mjs` 生成后手动粘贴到 Release：

```bash
node scripts/gen-changelog.mjs --for vX.Y.Z
# 生成 vX.Y.Z 相对上个 tag 的结构化 changelog，输出到 stdout
# 加 -o notes.md 可写入文件
```

输出形如：
```
## v0.6.0
### ✨ 新功能
- **sync**: 自动同步定时任务
### 🐛 修复
- **terminal**: 重连后光标位置错乱
```

可粘贴到发布后的 GitHub Release body，或追加到 `CHANGELOG.md`。**此步可选**——跳过也能正常发版，GitHub 默认说明够用。

### 步骤 3：本地验证 config schema（必做，防坑）

```bash
cd src-tauri && cargo check
```

这步在 build.rs 阶段校验 `tauri.conf.json` 的 schema，能在本地 1-2 分钟内暴露配置错误（如字段名拼错），比等 CI 跑 8 分钟才发现快得多。**发版前必做。**

### 步骤 4：打 tag 并推送

tag 必须指向刚提交的 commit，所以先 push master 再 push tag（或一起）：
```bash
git tag vX.Y.Z
git push origin master --tags
```

推送 tag 这一步会**同时触发**两个 workflow：
- `release.yml`（完整构建发版，约 8-12 分钟）← 这个是发版主体
- `ci.yml`（master commit 触发的构建守护）← 顺带跑，必然绿

### 步骤 5：监控 release.yml

用 GitHub REST API 查进度（**不需要认证**就能查公开仓库的 run 状态，比浏览器抓 DOM 稳定得多）：

```bash
# 查最近的 run（拿到 release 的 run id）
curl -s "https://api.github.com/repos/RT-2020/myshelltool/actions/runs?per_page=3" \
  | node -e "const j=JSON.parse(require('fs').readFileSync(0,'utf8'));j.workflow_runs.forEach(r=>console.log(r.name,'['+r.head_branch+']',r.status,r.conclusion,'run '+r.id))"

# 查某个 run 的步骤进度（替换 RUN_ID）
curl -s "https://api.github.com/repos/RT-2020/myshelltool/actions/runs/RUN_ID/jobs" \
  | node -e "const j=JSON.parse(require('fs').readFileSync(0,'utf8'));const job=j.jobs[0];console.log(job.status,job.conclusion);job.steps.forEach(s=>console.log((s.conclusion==='success'?'✅':s.conclusion==='failure'?'❌':'⏳')+' '+s.name))"
```

Build 步骤是关键且最耗时（8-12 分钟，完整 release 编译 + NSIS 打包 + 签名）。轮询间隔用 `sleep 180` 或 `sleep 240`，不要频繁查。

### 步骤 6：验证产物

Release 成功后，查 release 的附件（4 个文件必须都在）：
```bash
curl -s "https://api.github.com/repos/RT-2020/myshelltool/releases/tags/vX.Y.Z" \
  | node -e "const r=JSON.parse(require('fs').readFileSync(0,'utf8'));console.log('状态:',r.draft?'草稿':'已发布');r.assets.forEach(a=>console.log('  '+a.name))"
```

**必须出现这 4 个文件**，少一个都说明链路有问题：
- `myshelltool_X.Y.Z_x64-setup.exe` — NSIS 安装包
- `myshelltool_X.Y.Z_x64-setup.exe.sig` — **签名文件（自动更新校验用）**
- `myshelltool-X.Y.Z-portable.zip` — 便携版
- `latest.json` — 应用内自更新清单

---

## 已知坑与排查（本项目实战踩过）

按错误现象对照排查。这三类坑都是真实踩过的，优先对照。

### 坑 1：`.sig` 未生成 / Locate bundle artifacts 步骤失败

**症状**：Build 步骤成功，但后续 `Locate bundle artifacts` 报「未找到 .sig 签名文件」。

**根因**：`tauri.conf.json` 的 `bundle` 块缺 `createUpdaterArtifacts: true`。没有这个字段，Tauri 即使配了 updater + 签名密钥也不会生成签名 sidecar 和 latest.json。

**修复**：确认 `bundle.createUpdaterArtifacts` 为 `true`：
```json
"bundle": {
  "active": true,
  "targets": ["nsis"],
  "createUpdaterArtifacts": true,
  ...
}
```

### 坑 2：字段名拼错 → schema 拒绝

**症状**：Build 步骤直接失败，报错：
```
Error `tauri.conf.json` error on `bundle`: Additional properties are not allowed ('createUpdateArtifacts' was unexpected)
```

**根因**：字段名拼错了。正确是 **`createUpdaterArtifacts`**（中间有 `r`：Updater）。容易误写成 `createUpdateArtifacts`（少 r）。

**验证**：改完字段名后，本地跑一次 `cd src-tauri && cargo check`——build.rs 会校验 config schema，拼错会立即暴露（比等 CI 跑 8 分钟快得多）。**发版前必做这步验证。**

### 坑 3：tag 已存在，重推不触发 workflow

**症状**：改完 bug 想重新发版，但推 tag 后没有新的 release run。

**根因**：同名 tag 已存在于远端，GitHub 不会对已存在的 tag 重新触发 workflow。

**修复**：删掉远端和本地的旧 tag，重新打在新 commit 上：
```bash
git tag -d vX.Y.Z                              # 删本地
git push origin :refs/tags/vX.Y.Z              # 删远端
git push origin master                          # 先推修复 commit
git tag vX.Y.Z                                  # 在新 commit 上重打
git push origin vX.Y.Z                          # 推 tag 触发 workflow
```

### 其他常见信号

- **CI（ci.yml）红屏但 release.yml 没问题**：CI 只是构建守护，红屏不阻塞发版。但建议修，因为 release.yml 在 tag 上跑，CI 在 master 跑，代码同一份。
- **Build 步骤失败含 os error 32**：本地才有，是 DLL 被进程占用（`tasklist | grep myshelltool` 找占用进程）；CI 上不会遇到。

---

## 一键核对脚本

发版前快速自检配置链路是否完整（任一项输出为空就说明配置被改坏了）：
```bash
cd D:/Project/PersonGithubProject/myshelltool
echo "1. createUpdaterArtifacts:"; grep -c "createUpdaterArtifacts.*true" src-tauri/tauri.conf.json
echo "2. updater.pubkey 已配:"; grep -c "pubkey" src-tauri/tauri.conf.json
echo "3. updater 插件已注册:"; grep -c "tauri_plugin_updater" src-tauri/src/lib.rs
echo "4. workflows 存在:"; ls .github/workflows/
echo "5. 当前版本:"; grep '"version"' package.json src-tauri/tauri.conf.json | head -1
```

---

## 完整发版示例（参考）

假设要发 0.6.0：
```bash
# 1. bump 版本号 + 自动提交（脚本改 4 文件 + 刷 Cargo.lock + git commit）
node scripts/bump-version.mjs 0.6.0 --commit
# 2. （可选）生成结构化发布说明，待会儿粘贴到 Release
node scripts/gen-changelog.mjs --for v0.6.0 -o notes-v0.6.0.md
# 3. 本地验证 config schema（必做，防坑 2）
cd src-tauri && cargo check && cd ..
# 4. 打 tag 推送（触发 release.yml + ci.yml）
git tag v0.6.0
git push origin master --tags
# 5. 轮询 release.yml 直到 conclusion: success（约 8-12 分钟）
# 6. 验证 4 个产物文件齐全
# 7. （可选）把 notes-v0.6.0.md 内容粘贴到 GitHub Release 的 body
```

---

## 附录：脚本说明

### `scripts/bump-version.mjs` — 版本号 bump

一次性把 4 个文件版本号改成同一个值 + 刷新 Cargo.lock。

```bash
node scripts/bump-version.mjs <版本号> [--commit]
```

- **为什么需要它**：4 个文件（package.json / tauri.conf.json / 两个 Cargo.toml）手动改容易漏，版本漂移会让 CI 报错或产物版本号错乱。脚本保证原子一致。
- **内置校验**：semver 格式校验（`0.6` 会被拒，必须 `0.6.0`）；每个文件替换后对比原文，未匹配到立即报错而非静默跳过。
- **`--commit`**：自动 `git add`（只 add 4 个 manifest + 2 个 lockfile）+ `git commit -m "chore: bump vX.Y.Z"`。
- **Cargo.lock 同步**：跑 `cargo update -p myshelltool --precise X.Y.Z` 刷新 lockfile，避免 manifest/lockfile 不符。失败不致命（下次 build 自动修正）。
- **原理**：每个文件用针对性正则替换（按文件结构调整），不是粗暴全文替换，避免误伤其他数字。

### `scripts/gen-changelog.mjs` — 发布说明生成

从两个 tag 之间的 conventional commits 生成按类型分组（feat/fix/...）的中文发布说明。

```bash
node scripts/gen-changelog.mjs [--from <tag>] [--to <ref>] [--for <版本>] [-o <文件>]
```

- **`--for vX.Y.Z`**（最常用）：生成 vX.Y.Z 的发布说明，自动找上一个 tag 作为起点。
- **`--from A --to B`**：自定义区间（`B` 默认 HEAD）。
- **解析规则**：识别 `type(scope)?: description` 格式。本项目历史 commit 是这个风格（`feat(sync):`、`fix(release):` 等）。兼容非标准写法（如 `feat(sync) PR-4:` 没冒号也认）。
- **输出**：默认 stdout；`-o file.md` 写文件。markdown 格式，带 emoji 分组标题（✨ 新功能 / 🐛 修复 / 🔧 杂项 ...）。
- **与 release.yml 的关系**：release.yml 已开 `generate_release_notes: true`（GitHub 自动生成）。本脚本是**增强版**——对中文 conventional commit 分组更清晰，可手动粘贴到 Release body 替换默认说明，或追加到 CHANGELOG.md。**两套机制并存，本脚本可选。**
