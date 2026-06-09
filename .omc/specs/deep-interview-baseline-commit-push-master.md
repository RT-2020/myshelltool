# Deep Interview Spec: 首个基线提交并推送 master

## Metadata
- Interview ID: next-stage-myshelltool-2026-06-09
- Rounds: 5 + Round 0 topology
- Final Ambiguity Score: 9.5%
- Type: brownfield
- Generated: 2026-06-09
- Threshold: 20%
- Threshold Source: default
- Initial Context Summarized: no
- Status: PASSED / pending approval

## Clarity Breakdown
| Dimension | Score | Weight | Weighted |
|-----------|-------|--------|----------|
| Goal Clarity | 0.97 | 0.35 | 0.340 |
| Constraint Clarity | 0.86 | 0.25 | 0.215 |
| Success Criteria | 0.84 | 0.25 | 0.210 |
| Context Clarity | 0.93 | 0.15 | 0.140 |
| **Total Clarity** | | | **0.905** |
| **Ambiguity** | | | **0.095** |

## Topology
| Component | Status | Description | Coverage / Deferral Note |
|-----------|--------|-------------|--------------------------|
| 首个基线提交 | active | 把已验证的原型、Rust core、Tauri shell、测试和 OMC 规格作为首个 commit 固化，并推送当前 `master` 分支到 `https://github.com/RT-2020/myshelltool.git`。 | 本规格覆盖 commit 候选、排除项、验证门槛、remote、branch 和 push 验收。 |
| Windows Tauri 编译链修复 | deferred | 解决本机 `windres` / MSVC / linker 环境，让 Tauri cargo check/build 变成可通过门槛。 | 用户选择首个基线提交作为下一阶段主目标。 |
| 本地安全存储 token 占位 | deferred | 把当前仅显示已配置状态的 token 流程接到 Tauri 侧安全存储抽象，但仍不写入真实 token 明文。 | 用户选择首个基线提交作为下一阶段主目标。 |
| 连接资产持久化 | deferred | 把当前 sample assets 变成可创建、编辑、保存、加载的本地资产模型，先不做真实 SSH。 | 用户选择首个基线提交作为下一阶段主目标。 |
| 最小 SSH/SFTP 能力纵切 | deferred | 在已有 core/command bridge 基础上开始接入真实连接能力的最小闭环。 | 用户选择首个基线提交作为下一阶段主目标。 |

## Goal
创建 myshelltool 的首个本地基线提交，并把当前 `master` 分支推送到 GitHub 远端 `https://github.com/RT-2020/myshelltool.git`，使当前已验证的 Tauri-ready 前端原型、Rust core、Tauri command surface、前端后端桥接、v4 居中窗口和提交前验证记录成为可追踪的远端基线。

## Constraints
- 执行前必须重新验证提交候选和排除项，不能提交 `.claude/`、`.omc/state/`、`.omc/sessions/`、`.omc/logs/`、`.omc/research/`、`.omc/notepad.md`、`.omc/project-memory.json`、`dist/`、`node_modules/`、`src-tauri/target/` 或 `target/`。
- 必须保持当前分支策略：推送 `master`，不改名为 `main`。
- 远端必须为 `https://github.com/RT-2020/myshelltool.git`，remote 名称使用 `origin`。
- 不得输出、写入、记录或提交用户曾贴出的真实 GitHub PAT/token。
- 如 push 需要认证且当前环境无法完成认证，应停止并要求用户在会话中自行执行认证命令，不得要求用户把 token 粘贴给 assistant。
- 不做本轮延期组件：不修 Windows Tauri 编译链、不实现本地安全存储、不做资产持久化、不接入真实 SSH/SFTP。

## Non-Goals
- 不创建或切换 `main` 分支。
- 不 force push。
- 不修改远端仓库设置、默认分支、issues、PR 或 release。
- 不引入真实凭据持久化。
- 不新增业务功能。

## Acceptance Criteria
- [ ] `git status --short --ignored` 显示提交候选清晰，且敏感/构建/本地状态目录处于 ignored 状态。
- [ ] `node --check src/main.js && node --check tests/ui-smoke.mjs` 成功通过。
- [ ] JSON 校验成功：`package.json`、`src-tauri/tauri.conf.json`、`.omc/prd.json`。
- [ ] `cargo metadata --manifest-path src-tauri/Cargo.toml --no-deps --format-version 1` 成功通过。
- [ ] GitHub token 前缀扫描排除 `node_modules` 后无匹配。
- [ ] `npm run build` 成功通过。
- [ ] `npm run test:core` 成功通过。
- [ ] `npm run test:ui` 成功通过。
- [ ] 创建首个 commit，commit 内容只包含提交候选中的业务基线和可保留 OMC 规格/进度文件。
- [ ] 若 `origin` 不存在，则添加 `origin` 为 `https://github.com/RT-2020/myshelltool.git`；若存在且指向不同 URL，停止并询问用户。
- [ ] `git push -u origin master` 成功完成，或因认证缺失停止并给出用户可自行运行的认证/推送建议。
- [ ] 推送后 `git status --short` 不包含已提交文件的未暂存/未跟踪残留； ignored 目录仍未进入提交。

## Assumptions Exposed & Resolved
| Assumption | Challenge | Resolution |
|------------|-----------|------------|
| 下一阶段应该继续功能开发 | 先比较首个基线提交、编译链修复、安全存储、资产持久化、SSH/SFTP 纵切 | 用户选择首个基线提交作为下一阶段主目标。 |
| commit 后是否 push | 询问只本地 commit、commit+main、commit+push、只写规格 | 用户选择 commit+push。 |
| 推送分支是 `main` 还是 `master` | 用户粘贴 GitHub 初始化命令包含 `main`，但前一轮说推送 `master` | 用户最终确认推送 `master`。 |
| remote 是否已存在 | 读取仓库事实：`git remote -v` 无输出 | 本轮需添加 `origin` 指向用户确认的 GitHub URL。 |

## Technical Context
- 当前仓库已有前端静态原型：`src/index.html`、`src/main.js`、`src/styles.css`。
- Tauri command surface 在 `src-tauri/src/lib.rs`。
- Rust core 模型、样例资产和 token 摘要在 `crates/myshelltool-core/src/lib.rs`。
- UI smoke 测试在 `tests/ui-smoke.mjs`。
- npm 脚本在 `package.json`。
- 当前仓库事实：无 commit、无 remote、当前分支为 `master`。
- 当前仍为原型/占位的能力：浏览器 fallback、样例资产、token 仅状态不落盘、无真实 SSH/SFTP/持久化。

## Ontology (Key Entities)
| Entity | Type | Fields | Relationships |
|--------|------|--------|---------------|
| 基线提交 | delivery artifact | commit message, staged files, verification evidence | contains 提交候选; precedes 远端 push |
| 提交候选 | file set | `.gitignore`, `.omc` specs/progress, `crates/`, `src-tauri/`, `src/`, `tests/`, package files, Vite config | included by 基线提交 |
| 排除项 | ignored file set | `.claude/`, `.omc/state/`, `.omc/sessions/`, build outputs, dependency dirs, Rust target | excluded from 基线提交 |
| 验证套件 | quality gate | JS syntax, JSON parse, cargo metadata, token scan, build, core tests, UI smoke | must pass before commit/push |
| 本地 commit | git operation | first commit, branch `master` | pushed by 远端 push |
| 远端 push | git operation | `git push -u origin master` | sends master 分支 to GitHub origin URL |
| GitHub origin URL | external system | `https://github.com/RT-2020/myshelltool.git` | remote named `origin` |
| master 分支 | git branch | current branch, no rename to main | source branch for push |

## Ontology Convergence
| Round | Entity Count | New | Changed | Stable | Stability Ratio |
|-------|--------------|-----|---------|--------|-----------------|
| 1 | 6 | 6 | - | - | N/A |
| 2 | 8 | 2 | 0 | 6 | 75% |
| 3 | 8 | 0 | 1 (`main` → `master`) | 7 | 100% |
| 4 | 9 | 1 (`GitHub origin URL`) | 0 | 8 | 100% |
| 5 | 8 | 0 | 1 (分支策略收敛为 `master`) | 7 | 100% |

## Interview Transcript
<details>
<summary>Full Q&A</summary>

### Round 0
**Q:** 我把“下一阶段”读成 5 个可独立成功/失败的候选组件：首个基线提交、Windows Tauri 编译链修复、本地安全存储 token 占位、连接资产持久化、最小 SSH/SFTP 能力纵切。这个拓扑对吗？

**A:** 拓扑正确。

### Round 1
**Q:** 下一阶段只能优先完成一个主目标时，你最希望先完成哪一个？

**A:** 首个基线提交。

**Ambiguity:** 35.4%。

### Round 2
**Q:** 这次首个基线提交应该采用哪种边界？

**A:** commit+push。

**Ambiguity:** 22.7%。

### Round 3
**Q:** 首个基线提交和 push 的边界应该是什么？当前事实：`git remote -v` 无输出，当前分支是 `master`，仓库 `no-commits`。

**A:** 推送 master 就行。

**Ambiguity:** 21.1%。

### Round 4
**Q:** 请确认 push master 的远端目标是什么？

**A:** 使用 GitHub 仓库 `https://github.com/RT-2020/myshelltool.git`。用户粘贴的 GitHub 初始化命令包含 README 初始化、`git init`、`first commit`、`branch -M main`、`remote add origin` 和 `push -u origin main`。

**Ambiguity:** 20.4%。

### Round 5
**Q:** 最终首个基线提交应该推送哪个分支？

**A:** 推送 master。

**Ambiguity:** 9.5%。

</details>
