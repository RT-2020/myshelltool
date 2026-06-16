# AGENTS.md

> 本文件是 **AI Coding Agent 的项目上下文单一信息源（Single Source of Truth）**。
> 所有 AI 编码工具（Claude Code、Cursor、GitHub Copilot、ZCode/Coding Plan 等）应首先读本文件。
> 工具专用的派生文件（`CLAUDE.md`、`.github/copilot-instructions.md`、`.cursor/rules/*.mdc`）均引用本文件，避免内容漂移。
>
> 维护原则：**当架构、命令、约定变化时，先改本文件，再同步派生文件**。不要在派生文件里写与本项目无关的通用建议。

---

## 0. 你是什么 / 怎么协作

你是一个 **长期协作的工程成员**，不是一次性问答工具。在本仓库中工作时：

- **先读后写**：改动前先理解现有代码结构与约定，复用已有实现，避免重复造轮子。
- **复杂任务先规划**：涉及多文件、架构决策的任务，先用 Plan 模式产出执行计划并获得确认，再实现。
- **改完即验**：每次实现后跑构建/测试（见 §5），把验证结果如实报告，不要把"应该能过"说成"已通过"。
- **遵循本文件**：命名、目录、状态管理、IPC 约定（见 §4 / §6 / §7）是硬约束，违反会破坏一致性。
- **不造 ai-slop**：写代码前先搜现有实现；文件不过大；遵循工程质量红线（见下方「质量红线」）。

---

## 质量红线（硬约束，违反必须整改）

> 完整说明与本项目反模式实证见 [`docs/llm-engineering-guidelines.md`](./docs/llm-engineering-guidelines.md)。以下为可量化阈值，每次提交前对照。

**文件大小硬上限**（超出触发拆分）：

| 类型 | 软警告 | 硬上限 |
|---|---|---|
| Vue SFC `.vue` | 300 行 | **500 行** |
| Pinia store `.js` | 300 行 | **500 行** |
| Rust 模块 `.rs` | 400 行 | **800 行** |

> ⚠️ 当前已超标的文件（重构候选）：`ssh.rs`(1548)、`sessions.js`(810)、`FileColumn.vue`(768)、`files.js`(612)、`GlobalModals.vue`(512)。新增功能时优先考虑拆分这些文件，而非继续往里堆。

**禁止事项**：
- ❌ 重复造轮子：写新逻辑/样式/正则前必须先搜（grep/Grep/Explore），已有则复用。
- ❌ 空catch {}：错误至少 `announce` 或显式注释「为何可忽略」。
- ❌ `window.alert` / `window.confirm` / `window.prompt`：用内联校验 + `GlobalModals`。
- ❌ 硬编码颜色/z-index/间距：用 `var(--token)`（见 `src/styles/_tokens.scss`）。
- ❌ 同一概念多份实现：连接状态等用权威定义点（见指南 §5）。
- ❌ 死代码：未被 import 的模块确认后删除。

---

## 1. 项目概览

**myshelltool** — Windows 桌面 SSH 运维客户端。Tauri 2 桌面框架：Rust 后端 + Vue 3 前端，通过 IPC（`invoke`）打通。

- **目标用户**：运维 / 后端开发者，需要管理多台 SSH 主机的连接、终端、文件、隧道、资源监控。
- **平台**：Windows 优先（NSIS 安装包）；Rust 跨平台但前端/构建按 Windows 校准。
- **License**：MIT。

---

## 2. 技术栈（实际，以 package.json / Cargo.toml 为准）

| 层 | 技术 | 版本约束 |
|---|---|---|
| 桌面框架 | **Tauri 2** | `@tauri-apps/cli ^2.9.5` |
| 后端 | **Rust** | `russh 0.49`、`russh-sftp 2.x`、`tokio`（rt-multi-thread/net/sync） |
| 前端框架 | **Vue 3**（`<script setup>` + Composition API） | `vue ^3.5.38` |
| 状态管理 | **Pinia 3**（setup store 风格） | `pinia ^3.0.4` |
| 图标 | **lucide-vue-next** | `^0.460.0` |
| 终端 | **xterm.js 6** + addon-fit/search/web-links/webgl | `@xterm/xterm ^6` |
| 远程编辑 | Monaco Editor 0.52（CDN 加载） | — |
| 样式 | **SCSS + 设计 token 系统**（无 Tailwind） | `sass ^1.101`，自定义 `_tokens.scss` |
| 构建 | **Vite 7** | `vite ^7.2.7`，root=`src/` |
| 测试 | Playwright（UI smoke）+ `cargo test`（core 单元测试） | `playwright ^1.60` |

> ⚠️ **历史不一致提醒**：旧文档（README 早期版本、`.omc/project-memory.json`）曾写"前端 Vanilla JS 无框架"——**这是过时信息**。项目在 Wave 1–5 重构后已全面 Vue 3 + Pinia 化。以本文件与 `package.json` 为准。

---

## 3. 目录结构（实际）

```
myshelltool/
├── src/                        # 前端（Vite root）
│   ├── index.html              # 主页面（Vite 入口 HTML）
│   ├── main.js                 # 应用入口：createApp(App).use(createPinia()).mount('#app')
│   ├── App.vue                 # 根组件：5 区域布局 + onMounted 调 store.initialize() 启动加载
│   ├── components/
│   │   ├── shell/              # 外壳：AppShellLayout / AppTitleBar / AppStatusBar
│   │   │                       #        ConnectionSidebar / AssetGroupNode(递归)
│   │   │                       #        GlobalModals(弹窗中枢) / OpsSummaryPanel
│   │   ├── terminal/           # 终端：TerminalSurface / TerminalTabs / TerminalToolbar
│   │   ├── files/              # 文件：FileSurface / FileColumn / ...
│   │   ├── resource-monitor/   # 资源监控：Cpu/Memory/Network/Disk 图表 + chart-utils
│   │   └── ui/                 # 基础组件库：App{Input,Button,Select,Modal,Drawer,
│   │                           #            ContextMenu,Tooltip,Table,Tab,Progress,...}
│   │                           #   index.js barrel 导出全部
│   ├── stores/                 # Pinia stores（6 领域 + 1 编排壳）
│   │   ├── workbench.js        # 编排壳：re-export 子 store，initialize() 启动加载
│   │   ├── sessions.js         # 活跃 SSH 会话 + 终端生命周期
│   │   ├── assets.js           # 连接资产 CRUD + 分组树
│   │   ├── files.js            # SFTP 文件 + 传输队列
│   │   ├── tunnels.js          # SSH 隧道/端口转发
│   │   ├── ui.js               # UI 状态：主题/tab/modal/搜索
│   │   └── resourceMonitor.js  # 资源监控轮询 + 事件订阅
│   ├── composables/            # useTheme / useClipboard / useTerminalConfig /
│   │                           # useTerminalShortcuts / useAutoReconnect
│   ├── lib/                    # terminalController / terminalThemes / dangerousCommands
│   ├── services/
│   │   └── backend.js          # Tauri IPC 桥：invokeBackend / listenBackendEvent /
│   │                           #                 normalizeAsset / slugify
│   └── styles/                 # SCSS：_tokens(设计token) / _base / _utilities / main
├── src-tauri/                  # Tauri/Rust 后端
│   ├── src/
│   │   ├── main.rs             # 二进制入口（tauri::run 壳）
│   │   ├── lib.rs              # AppState + 资产/凭据命令 + generate_handler 注册
│   │   ├── ssh.rs              # SSH/SFTP/隧道核心（SshSessionManager + russh Handler）
│   │   ├── resource_monitor.rs # 远程 CPU/mem/net/disk 轮询（SshCommand::MonitorExec）
│   │   └── fs_local.rs         # 本地文件系统命令
│   ├── capabilities/default.json
│   └── tauri.conf.json         # com.redtei.myshelltool，1366×800，withGlobalTauri:true
├── crates/
│   └── myshelltool-core/       # 共享核心库（无 Tauri 依赖，可独立 cargo test）
│       └── src/lib.rs          # ConnectionAsset / SecretStore / 资产持久化 / 资产校验
├── tests/
│   ├── ui-smoke.mjs            # UI 冒烟：5 区域 + 资源监控占位（Playwright）
│   └── ui-host-key.mjs         # Host key 验证流程 UI 测试
├── docs/                       # 文档
├── .claude/                    # Claude Code 配置（见 .gitignore，部分本地）
├── .omc/ .omx/                 # 其他 agent 工具状态（见 .gitignore）
├── package.json                # npm scripts（见 §5）
├── vite.config.js              # root=src，@→/src alias，scss loadPaths，port 41234 strict
└── AGENTS.md                   # ← 本文件
```

---

## 4. 约定（硬约束，违反会破坏一致性）

### 4.1 前端
- **Vue 3 `<script setup>`** + Composition API。**禁止** Options API。
- **状态用 Pinia setup store**（`defineStore('x', () => {...})` 返回 refs/computeds/actions）。
- **图标统一用 `lucide-vue-next`**，不要引入其他图标库或内联 SVG（资源监控图表除外，用自绘 SVG）。
- **样式用 SCSS + 设计 token**（`@/styles/_tokens.scss`）。组件内 `<style scoped lang="scss">` 顶部 `@use '@/styles/_tokens' as *;`。
  - 颜色/间距/圆角/阴影/动效/z-index **必须用 `var(--xxx)` token**，不要硬编码。z-index 用 `var(--z-base|dropdown|sticky|drawer|modal|toast|tooltip)`。
  - 新增 token 加到 `_tokens.scss` 的 map 并暴露为 `:root` CSS 变量。
- **路径别名 `@`** → `src/`（vite.config.js 配置）。import 用 `@/stores/...`、`@/components/...`。
- **组件分层**：`ui/` 是无业务的基础组件（`App*` 命名，barrel 导出）；业务组件按域放 `shell/`、`terminal/`、`files/`、`resource-monitor/`。
- **通用操作入口模式**：右键菜单用 `AppContextMenu`（`items: [{label, action, danger, separator, disabled}]`），参照 `FileSurface.vue` 用法。危险操作用 `danger: true`（红色）或 `AppButton variant="danger"`。
- **弹窗**：业务弹窗统一走 `GlobalModals.vue`（`store.modal = { type, ...payload }`），按 `modal.type` 分支。新增 type 需同步改 `modalTitle` / `submitModal` / `watch`。保留 legacy 选择器（`#modalLayer`/`#modalBody`/`.modal-actions .btn.danger`）以兼容测试。

### 4.2 状态管理（跨 store 桥接）
- **`workbench.js` 是编排壳**：实例化 6 个子 store，`initialize()` 编排启动加载，用 plain-object 返回 + `computed()` 包裹子 store 的响应式 state（**不要直接暴露子 store 的 ref**，会丢响应性）。
- **跨 store 依赖用 lazy bridge**：子 store 通过 `attachWorkbench(bridge)` 注入跨 store 访问（如 assets store 调 workbench.announce / workbench.modal）。**禁止循环 import**。
- **新 action 加到子 store**，再在 `workbench.js` return 块 re-export（参照 `saveAsset`/`deleteAsset` 模式）。

### 4.3 Rust 后端
- **所有 Tauri 命令用 `State<'_, AppState>` 统一解析**（ADR v3 Option A 重构后）。**禁止** 双 `manage` hack。
- **命令参数 camelCase**（前端 `invokeBackend('ssh_connect', {credentialId})`）。Rust 端字段用 `#[serde(alias = "camelCase")]` 兼容。
- **新增命令**：在 `src-tauri/src/lib.rs` 加 `#[tauri::command]`，并在 `tauri::generate_handler![...]` 注册（漏注册 = 前端调用报 "command not found"）。
- **持久化逻辑放 `crates/myshelltool-core`**（无 Tauri 依赖，可独立单测）；`src-tauri` 只做命令封装 + State 读取。
- **凭据（密码/passphrase）只走 `SecretStore`**，绝不写进资产 JSON / 日志 / 错误信息。

---

## 5. 构建与测试命令（执行环境）

```bash
# 前端依赖
npm install

# —— 开发 ——
npm run dev          # Vite 浏览器预览（127.0.0.1:41234）。无 SSH/文件功能（缺 Tauri runtime）
npm run tauri:dev    # Tauri 桌面开发模式（完整功能）。SSH 类功能只能在此验证

# —— 构建 ——
npm run build        # 仅前端 Vite 构建（验证 Vue/SCSS 编译）
npm run tauri:build  # 完整桌面安装包（Windows NSIS）

# —— 测试 ——
npm run test:core    # Rust core 单元测试：cargo test --manifest-path crates/myshelltool-core/Cargo.toml
npm run test:ui      # UI 冒烟：node tests/ui-smoke.mjs && node tests/ui-host-key.mjs（需先 npm run dev 起服务）

# —— 后端单独验证 ——
cd src-tauri && cargo build       # 验证 Rust 编译
cd src-tauri && cargo check       # 更快的类型检查
```

**改完代码必须跑的验证（开发闭环）**：
- 改前端 → `npm run build`（验证编译）+ 如改了交互 `npm run test:ui`。
- 改 Rust → `cd src-tauri && cargo build`。
- 改 core 持久化 → `npm run test:core`。
- **如实报告结果**：贴 exit code / 关键输出，失败就说失败。

---

## 6. IPC 契约（前端 ↔ Rust，关键命令清单）

前端通过 `src/services/backend.js` 的 `invokeBackend(command, args)` / `listenBackendEvent(event, handler)` 调用 Rust。**非 Tauri runtime 会抛错**（浏览器预览模式无 SSH 功能）。

### 命令分组（在 `src-tauri/src/lib.rs` 与 `ssh.rs` 注册）

**资产持久化**（`lib.rs`，存 `<app_data_dir>/connection-assets.json`）
- `list_connection_assets` → `{ source, count, assets, groups }`
- `save_connection_asset({ asset })` → upsert，返回更新后的列表
- `delete_connection_asset({ id })` → 删除 + 容错清理关联凭据
- `rename_asset_group({ oldPath, newPath })` / `dissolve_asset_group({ path })` / `create_asset_group({ path })` → 分组批量操作

**凭据**（`lib.rs`，存 `<app_data_dir>/credentials/<id>.cred`）
- `save_credential({ id, secret })` / `get_credential_status({ id })` / `delete_credential({ id })`
- 凭据 id 约定：`<assetId>:<kind>`（kind = `password` | `passphrase`）

**SSH 会话/终端**（`ssh.rs`）
- `ssh_connect({...})` → 返回 `session_id`；参数含 host/port/username/authMethod/credentialId/privateKeyPath 等
- `ssh_list_directory` → 一次性 exec `find`（走独立连接，非当前会话）
- `ssh_write` / `ssh_resize` / `ssh_disconnect` / `ssh_confirm_host_key` / `ssh_keyboard_response`

**SFTP**（`ssh.rs`）
- `sftp_list_dir` / `sftp_read_file` / `sftp_write_file`
- 分块上传：`sftp_upload_start` / `sftp_upload_chunk` / `sftp_upload_finalize`（防 IPC OOM）
- `sftp_download_with_progress` / `sftp_mkdir` / `sftp_rename` / `sftp_remove` / `sftp_stat`

**隧道**（`ssh.rs`，仅内存）
- `tunnel_create` / `tunnel_start` / `tunnel_stop` / `tunnel_list` / `tunnel_delete`

**资源监控**（`resource_monitor.rs`）
- `resource_monitor_start` / `_stop` / `_snapshot` / `_list_active`

**本地文件**（`fs_local.rs`）
- `fs_local_home_dir` / `fs_local_list_dir` / `fs_local_mkdir` / `fs_local_delete` / `fs_local_rename`

### 事件（Rust → 前端，`listenBackendEvent`）
- `sftp-transfer-progress`、`ssh-host-key-verify`、`ssh-keyboard-interactive`、`ssh-session-status`

---

## 7. 数据模型

### ConnectionAsset（连接资产，`crates/myshelltool-core/src/lib.rs`）
```rust
id, name, host, port(u16), username, auth_method(Password|PrivateKey|Token),
private_key_path(Option), group(String, '/' 分隔多级路径如 "生产/数据库"),
tags(Vec<String>), status(Connected|Warning|Idle), last_connected,
credential_id(Option), passphrase_credential_id(Option)
```
- 前端经 `normalizeAsset()`（`backend.js`）规整：默认 port=22、group=「未分组」、status=Idle。
- **分组不是独立实体**，是 `asset.group` 字符串字段（`/` 分隔层级）。空分组用 `ConnectionAssetStore.groups: Vec<String>` 单独持久化。
- 「未分组」是保留顶级，不可重命名/解散。

### 持久化分层
- **资产元数据** → `connection-assets.json`（JSON-on-disk，无密钥）
- **凭据** → `credentials/<id>.cred`（弱 XOR 混淆，非加密）
- **known_hosts** → `known_hosts.json`
- **活跃会话/隧道/SFTP 缓存** → 纯内存（重启丢失）

---

## 8. 安全设计红线

- 凭据（密码/私钥/passphrase）**只走 SecretStore**，不进资产 JSON、日志、错误信息、前端 console。
- 危险文件操作（删除、覆盖）**必须弹窗确认**。
- Host key 变更默认**阻止连接并警告**。
- 远程命令执行/隧道监听 `0.0.0.0` 需安全审视。

---

## 9. 已知边界 / Follow-ups（改这些时要留意）

- `sftp_download_with_progress` 仍返回整块 `Vec<u8>`（upload 已分块，download 待改造）。
- `start_remote_forward` 是返回 Err 的桩（local/dynamic SOCKS5 已实现）。
- `sanitize_credential_id` 过滤 `:` 和 `.`（如 `192.168.2.2:password` → `192-168-2-2password`）。
- Windows 上 `cargo build` 偶因 build script（windres）阻断，用 `cargo check` 兜底。
- `workbench.js` 仍是 re-export 壳（Wave 5+ 计划精简）。

---

## 10. Git / 提交

- 默认分支 `master`。提交前确认在正确分支；用户没要求时**不要自动 commit/push**。
- 改动尽量聚焦，提交信息描述清楚"做了什么 + 为什么"。

---

## 11. 给 Agent 的快速决策清单

收到任务时按此顺序判断：
1. **是读/研究类**？→ 用 Explore 子 agent 并行搜索，给出结论而非文件堆。
2. **涉及多文件/架构决策**？→ 先进 Plan 模式，产出计划并经确认。
3. **有现成实现可复用**？→ 复用（查 `ui/index.js`、`workbench.js` re-export、`backend.js` normalize*）。
4. **要改 Rust 命令**？→ 别忘了在 `generate_handler!` 注册。
5. **要加跨 store 逻辑**？→ 加子 store action + `workbench.js` re-export，用 lazy bridge。
6. **改完**？→ 跑 §5 的验证，如实报告 exit code。
