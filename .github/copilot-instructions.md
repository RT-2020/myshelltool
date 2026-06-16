# GitHub Copilot Instructions

> **完整项目上下文见仓库根目录 [`AGENTS.md`](../AGENTS.md)。本文件不重复其内容。**
>
> GitHub Copilot（Chat / Workspace / coding agent）在本仓库工作时，请遵循 `AGENTS.md`。以下是与 Copilot 工作流最相关的要点。

## 必读

开始改动前先读 [`AGENTS.md`](../AGENTS.md)（注意顶部「质量红线」节）与 [`docs/llm-engineering-guidelines.md`](../docs/llm-engineering-guidelines.md)（工程质量最佳实践 + 本项目反模式实证）。特别是：技术栈（§2）、目录结构（§3）、约定（§4）、构建/测试命令（§5）、IPC 契约（§6）、数据模型（§7）、安全红线（§8）。

## 高频踩坑点（Copilot 易出错处）

1. **前端是 Vue 3 + Pinia**，不是 Vanilla JS（旧 README 曾写错，以 `AGENTS.md` / `package.json` 为准）。生成组件用 `<script setup>` + Composition API。
2. **样式用 SCSS 设计 token**：颜色/间距/z-index 用 `var(--token)`（`src/styles/_tokens.scss`），不要硬编码。不要建议引入 Tailwind。
3. **图标统一 `lucide-vue-next`**，不要建议其他图标库。
4. **新增 Tauri 命令**必须两步：`#[tauri::command]` + 在 `generate_handler![]` 注册。
5. **跨 store 逻辑**：加子 store action + `workbench.js` re-export + lazy bridge，禁止循环 import。
6. **凭据红线**：密码/passphrase 只走 `SecretStore`，不进 JSON/日志/console。Copilot 生成示例时不要把明文密码写进资产对象。
7. **路径别名** `@` → `src/`。

## 工程质量红线（防 ai-slop / 大文件 / 扩展性差）

**完整规则见 [`docs/llm-engineering-guidelines.md`](../docs/llm-engineering-guidelines.md)**（含本项目反模式实证）。Copilot 尤其易犯以下错误，务必避免：

- **先搜后写**：生成新逻辑/样式/正则前先搜现有实现，已有就复用。本项目实证：tag 正则曾 ×3、`.dot` CSS ×4、`terminalController.js` 死代码。
- **文件不过大**：Vue/store ≤500 行、Rust ≤800 行。`ssh.rs`(1548)、`sessions.js`(810)、`GlobalModals.vue`(512) 已超标，往里加功能前先评估拆分。
- **禁空 catch / window.alert**：错误走 `announce` 或显式注释；校验/确认走 `GlobalModals`，不用浏览器原生弹窗。
- **别用 switch 链**：新增分支 >5 个用注册表（`GlobalModals` 加 modal 曾要改 4 处）。
- **死代码即删**：未被 import 的模块确认后删除。

## 改完必验（不要只说"应该能过"）

- 前端：`npm run build`
- Rust：`cd src-tauri && cargo build`（或 `cargo check`）
- core 单测：`npm run test:core`
- UI 冒烟（需先 `npm run dev`）：`npm run test:ui`

如实报告 exit code 与关键输出。

## 不要做

- 不自动 `git commit` / `git push`（除非用户明确要求）。
- 不为"风格统一"重写已工作的代码（先确认有真实收益）。
- 不在本文件或 `.cursor/rules/*` 写与 `AGENTS.md` 漂移的内容——架构变更改 `AGENTS.md` 这个唯一信息源。
