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
- **发版走技能**：用户说「发版」「打 tag 发版」「bump 到 vX.Y.Z」等时，用项目内置技能 `release-myshelltool`（`.agents/skills/release-myshelltool/SKILL.md`），它沉淀了版本 bump → 打 tag → 触发 GitHub Actions → 验证产物的完整流程与本仓库真实踩过的坑（如 `createUpdaterArtifacts` 字段名、`.sig` 未生成、tag 重打）。不要临时拼凑发版步骤。

---

## 质量红线（硬约束，违反必须整改）

> 完整说明与本项目反模式实证见 [`docs/llm-engineering-guidelines.md`](./docs/llm-engineering-guidelines.md)。以下为可量化阈值，每次提交前对照。

**文件大小硬上限**（超出触发拆分）：

| 类型 | 软警告 | 硬上限 |
|---|---|---|
| Vue SFC `.vue` | 300 行 | **500 行** |
| Pinia store `.js` | 300 行 | **500 行** |
| Rust 模块 `.rs` | 400 行 | **800 行** |

> ⚠️ 当前已超标的文件（重构候选）+ Soft-warn 监视清单，见 [`docs/architecture-log.md`](./docs/architecture-log.md) 的 Baseline snapshot（用 `wc -l` 实测维护）。新增功能时优先考虑拆分这些文件，而非继续往里堆。手动清单易漂移，以 architecture-log 为唯一信息源。

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
│   │   ├── shell/              # 外壳：ConnectionSidebar / AssetGroupNode(递归) /
│   │   │                       #        GlobalModals(弹窗中枢) / OpsSummaryPanel / RightSidebar
│   │   ├── workbench/          # 实际运行的外壳：WorkbenchShell（标题栏/状态栏/拖拽条）
│   │   ├── terminal/           # 终端：TerminalSurface / TerminalTabs / TerminalToolbar
│   │   ├── files/              # 文件：FileSurface / FileColumn / ...
│   │   ├── resource-monitor/   # 资源监控：Cpu/Memory/Network/Disk 图表 + chart-utils
│   │   └── ui/                 # 基础组件库：App{Input,Button,Select,Modal,Drawer,
│   │                           #            ContextMenu,Tooltip,Table,Tab,Progress,...}
│   │                           #   index.js barrel 导出全部
│   ├── stores/                 # Pinia stores（8 个：7 领域 + 1 编排壳）
│   │   ├── workbench.js        # 编排壳：实例化 7 个子 store（不含 resourceMonitor），initialize() 启动加载
│   │   ├── sessions.js         # 活跃 SSH 会话 + 终端生命周期
│   │   ├── assets.js           # 连接资产 CRUD + 分组树
│   │   ├── files.js            # SFTP 文件 + 传输队列
│   │   ├── tunnels.js          # SSH 隧道/端口转发
│   │   ├── ui.js               # UI 状态：主题/tab/modal/搜索
│   │   ├── resourceMonitor.js  # 资源监控轮询 + 事件订阅（不经 workbench，由 panel 直接 use）
│   │   ├── mcp.js              # 【v1.2】MCP 探测状态 + 配置引导（refresh 触发探测，无事件监听）
│   │   └── sync.js             # 【v1.3】Gist 资产同步（push/pull/冲突解决/状态展示）
│   ├── composables/            # useTheme / useClipboard / useTerminalConfig /
│   │                           # useAutoReconnect / usePanelResize / useAutoUpdate
│   ├── lib/                    # terminalThemes / dangerousCommands / terminalGuards /
│   │                           # transferUtils（S2/S3 新增的纯函数模块）
│   ├── services/
│   │   └── backend.js          # Tauri IPC 桥：invokeBackend / listenBackendEvent /
│   │                           #                 normalizeAsset / slugify
│   └── styles/                 # SCSS：_tokens(设计token) / _base / _utilities / main
├── src-tauri/                  # Tauri/Rust 后端
│   ├── src/
│   │   ├── main.rs             # 二进制入口（tauri::run 壳）
│   │   ├── lib.rs              # AppState + 资产/凭据命令 + generate_handler 注册 + mcp_status
│   │   ├── ssh.rs              # SSH/SFTP/隧道核心（SshSessionManager + russh Handler）
│   │   ├── resource_monitor.rs # 远程 CPU/mem/net/disk 轮询（SshCommand::MonitorExec）
│   │   ├── fs_local.rs         # 本地文件系统命令
│   │   ├── sync.rs             # 【v1.3】Gist 同步命令层（push/pull/conflict，粘合 core sync + reqwest）
│   │   ├── dpapi_codec.rs      # 【v1.3】DPAPI 凭据编解码（Windows CryptProtectData，cfg(windows)）
│   │   ├── dangerous_commands.rs # 危险命令检测（D5，lib.rs 注册为 mod）：白/黄/黑/Unknown 四层分类，GUI 与 MCP 共享单点真相（fail-secure 默认拒）
│   │   ├── bin/mcp.rs          # 【v1.4 已删】原 myshelltool-mcp 独立 console bin，内嵌后取消双二进制
│   │   └── mcp/                # MCP server 接入模块（v1.4 内嵌 GUI / Streamable HTTP transport）
│   │       ├── http_server.rs  # 【v1.4】Streamable HTTP server：axum + rmcp，绑定 127.0.0.1:41235/mcp
│   │       ├── server.rs       # rmcp ServerHandler 实现（transport 无关，9 工具/4 资源/3 prompts）
│   │       ├── tools.rs        # MCP Tools 实现 + exec_on_asset（v1.4 直走 headless，会话复用记 follow-up）
│   │       ├── approval.rs     # 审批判定：白名单放行 / elicitation / 进程内拒绝（v1.4 删 pipe 降级）
│   │       ├── probe.rs        # 【v1.4】HTTP 健康检查：向自己的 endpoint 发 initialize 握手（不再 spawn 子进程）
│   │       ├── pipe.rs         # 【v1.4 已删】原 named pipe 桥接（双进程时复用 GUI 会话），内嵌后无需
│   │       ├── resources.rs    # 3 静态资源 + 1 template（assets/sessions/known-hosts/session-log）
│   │       └── prompts.rs      # 3 诊断 prompt（diagnose_server/audit_security/cleanup_disk）
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
- **`workbench.js` 是编排壳**：实例化 7 个子 store（sessions/files/tunnels/assets/ui/mcp/sync），`initialize()` 编排启动加载，用 plain-object 返回 + `computed()` 包裹子 store 的响应式 state（**不要直接暴露子 store 的 ref**，会丢响应性）。**注意 `resourceMonitor.js` 不经 workbench 编排**——它由 `ResourceMonitorPanel.vue` 直接 `useResourceMonitorStore()` 使用（独立轮询生命周期，与全局初始化解耦）。
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

**Gist 资产同步**（`sync.rs`，v1.3）
- `sync_status` → 同步配置状态（是否已配置 / 上次同步时间 / gist_id 掩码）
- `sync_setup({ masterPassword, gistId? })` → 首次设置（主密码派生密钥 + 可选拉取已有 Gist）
- `sync_push({ masterPassword })` / `sync_pull({ masterPassword })` → 加密推送 / 拉取解密（pull 返回 Conflict 时前端弹框）
- `sync_resolve_conflict({ masterPassword, choice })` → 冲突解决（local_overwrite / remote_overwrite）
- `sync_reset_master_password({ old, new })` / `sync_clear`

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

**MCP 服务**（`lib.rs` 调 `mcp/http_server.rs` + `mcp/probe.rs`）
- `mcp_status` → 【v1.4】HTTP 健康检查 + 聚合能力清单。返回 `McpStatus { serverName, serverVersion, endpoint, dataDir, probe: McpProbeResult, tools[], resources[], prompts[] }`。`probe.ok` 是状态灯唯一信号源（向自己的 HTTP endpoint 发 initialize 握手，不再 spawn 子进程）。`endpoint` 是 MCP HTTP URL（如 `http://127.0.0.1:41235/mcp`），供用户配置 MCP host。
- 【v1.4 已删】`mcp_approval_resolve`（原 v1.1 pipe 审批回传，内嵌后无 pipe）

### 事件（Rust → 前端，`listenBackendEvent`）
- `sftp-transfer-progress`、`ssh-host-key-verify`、`ssh-keyboard-interactive`、`ssh-session-status`
- 【v1.4 已删】`mcp-approval-verify`（原 v1.1 pipe 审批委托事件，内嵌后无 pipe）

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

### McpStatus / McpProbeResult（MCP 健康检查，`lib.rs` + `mcp/probe.rs` + `mcp/http_server.rs`）
- **设计取舍（v1.4）**：MCP server 内嵌 GUI 进程，用 Streamable HTTP transport 对外暴露（`http://127.0.0.1:41235/mcp`）。状态灯 = HTTP 健康检查：GUI 每次 `mcp_status` 向自己的 endpoint 发 MCP `initialize` 握手，成功即「可用」。**不再 spawn 子进程**（v1.2 的一次性 spawn 已废弃，根治僵尸进程 + os error 32）。
- **v1.4 架构变化**：取消双二进制（删 `bin/mcp.rs` + `pipe.rs`），MCP server 直接跑在 GUI 进程内，SSH 会话/资产/审批同进程访问。任何合规 MCP host（Claude Code / Cursor）经 HTTP URL 连入。
- `McpProbeResult { ok: bool, reason?, detail?, exePath, serverInfo?, probedAt }`
  - `reason` 失败分类码：`endpoint_not_found`（server 未启动/未写 endpoint 配置）/ `http_error`（连不上）/ `timeout`（2s）/ `bad_protocol`（握手响应异常）
  - `exePath`：v1.4 语义改为 HTTP endpoint URL（字段名保留兼容前端 mcp.js）
- 前端 `mcp.js` 的 `clientConnected` computed 读 `probe.ok`（命名保留是为避免连锁改名，语义已是健康检查结果）。

---

## 8. 安全设计红线

- 凭据（密码/私钥/passphrase）**只走 SecretStore**，不进资产 JSON、日志、错误信息、前端 console。
- 危险文件操作（删除、覆盖）**必须弹窗确认**。
- Host key 变更默认**阻止连接并警告**。
- 远程命令执行/隧道监听 `0.0.0.0` 需安全审视。
- **【v1.6】同步主密码绝不落盘**：资产同步的主密码（master password）在派生 AES 密钥后即丢弃，绝不存盘。自动同步功能用「会话密钥 + DPAPI 保护」绕过每次输密码：首次启用时用主密码派生固定 AES key（Argon2id，确定性），该 key 经 DPAPI（User scope，绑定 Windows 用户登录态）加密后存 SecretStore（credential id = `sync-session-key`）。离机即失效，非 Windows 或 DPAPI 失败时降级为手动主密码模式（不静默失败）。

---

## 9. 已知边界 / Follow-ups（改这些时要留意）

- `sftp_download_with_progress` 仍返回整块 `Vec<u8>`（upload 已分块，download 待改造）。
- `start_remote_forward` 是返回 Err 的桩（local/dynamic SOCKS5 已实现）。
- `sanitize_credential_id` 过滤 `:` 和 `.`（如 `192.168.2.2:password` → `192-168-2-2password`）。
- Windows 上 `cargo build` 偶因 build script（windres）阻断，用 `cargo check` 兜底；`cargo test` 的 src-tauri 测试二进制会因 Tauri runtime DLL 缺失报 `STATUS_ENTRYPOINT_NOT_FOUND`，用 `cargo check --tests` 验证测试可编译。
- `workbench.js` 仍是 re-export 壳（Wave 5+ 计划精简）。
- **【v1.4】MCP 内嵌 GUI（Streamable HTTP）**：MCP server 跑在 GUI 进程内，绑定 `127.0.0.1:41235/mcp`（占用则 +1，写 `<data_dir>/mcp-endpoint.json`）。取消双二进制（删 `bin/mcp.rs` + `pipe.rs`），根治 v1.2 的僵尸进程 + os error 32 + NSIS 打包缺口。MCP server 随 GUI 启停（`CancellationToken` 控制 graceful shutdown）。
- **【v1.4】MCP 端口策略**：默认 41235，被占用则 +1 重试最多 10 次，**只监听 localhost**（§8 安全红线）。实际端口写 mcp-endpoint.json，前端 `mcp_status.endpoint` 返回。
- **【v1.4 follow-up】会话复用**：`tools.rs::exec_on_asset` 当前直走 headless 建连（删了 v1.1 pipe 复用分支）。后续可注入 GUI 的 `Arc<AsyncMutex<SshSessionManager>>` 到 McpToolContext，命中已建立会话时直接复用（同进程访问，比 pipe 更简单）。
- **【v1.4 follow-up】GUI 弹窗审批**：`server.rs::degrade_to_pipe_or_reject` 当前对不支持 elicitation 的客户端直接 fail-secure 拒绝（删了 v1.1 pipe 降级）。后续可注入 AppHandle，实现同进程 GUI 弹窗审批（像 ssh.rs host-key 验证那样 emit 前端）。

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
