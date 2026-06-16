# CLAUDE.md

> **本项目使用 Claude Code（或兼容 agent）开发。完整项目上下文见 [`AGENTS.md`](./AGENTS.md)。**
>
> 本文件仅补充 Claude Code 工作流相关的提示；所有技术栈、目录结构、约定、命令、IPC 契约、数据模型、安全红线**一律以 `AGENTS.md` 为准**。请先完整阅读 `AGENTS.md`。

## 工作流

1. **先读 `AGENTS.md`**，再开始任何改动。
2. **复杂任务（多文件 / 架构决策）**：使用 Plan 模式（`EnterPlanMode`），产出计划并用 `ExitPlanMode` 请求确认后再实现。不要跳过规划直接大改。
3. **不确定的需求**：用 `AskUserQuestion` 澄清，不要替用户做关键假设。
4. **代码探索**：优先用 Explore 子 agent 并行搜索（多个独立搜索放一条消息），拿到结论而非文件堆。
5. **改完必验**：按 `AGENTS.md` §5 跑构建/测试，**如实报告 exit code 与关键输出**。

## 项目特定注意（与 AGENTS.md 一致，此处仅强调高频踩坑点）

- **前端是 Vue 3 + Pinia**，不是 Vanilla JS（旧 README 曾写错）。
- **新增 Tauri 命令**：`lib.rs` 加 `#[tauri::command]` **并**在 `generate_handler![]` 注册，漏注册会 "command not found"。
- **跨 store 逻辑**：加子 store action + `workbench.js` re-export，用 `attachWorkbench` lazy bridge，**禁止循环 import**。`workbench.js` 暴露子 store 响应式 state 必须用 `computed()` 包裹。
- **样式**：硬编码颜色/z-index 会被 review 打回，用 `var(--token)`（见 `src/styles/_tokens.scss`）。
- **凭据红线**：密码/passphrase 只走 `SecretStore`，不进 JSON/日志/console。

## 不要做的事

- 不要自动 `git commit` / `git push`（除非用户明确要求）。
- 不要重写已工作的代码做"风格优化"（先确认是否有真实收益）。
- 不要在派生 agent 文件（本文件、`copilot-instructions.md`、`.cursor/rules/*`）里写与 `AGENTS.md` 冲突或漂移的内容——改架构时改 `AGENTS.md` 这个唯一信息源。

## 工程质量（防 ai-slop / 大文件 / 扩展性差）

**完整规则见 [`docs/llm-engineering-guidelines.md`](./docs/llm-engineering-guidelines.md)**（含本项目反模式实证：`ssh.rs` 1548 行、`sessions.js` 810 行、`terminalController.js` 死代码、tag 正则 ×3、`.dot` CSS ×4、`window.alert` 绕过 GlobalModals 等）。`AGENTS.md` 顶部「质量红线」节有可量化阈值摘要。

高频踩坑（务必避免）：
- **先搜后写**：写新逻辑/样式/正则前 grep 现有实现，已有就复用，不另写。
- **文件不过大**：Vue/store ≤500 行、Rust ≤800 行；当前 `ssh.rs`/`sessions.js`/`FileColumn.vue`/`files.js`/`GlobalModals.vue` 已超标，往里加功能前先评估拆分。
- **空 catch / window.alert 禁用**：错误走 `announce` 或显式注释，校验/确认走 `GlobalModals`。
- **加功能改几处**：能注册表就别 switch 链（新增 modal 别只改一个 switch 漏了其他）。
