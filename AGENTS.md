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
| Vue SFC `.vue`（`<script setup lang="ts">`） | 300 行 | **500 行** |
| Pinia store `.ts` | 300 行 | **500 行** |
| Rust 模块 `.rs` | 400 行 | **800 行** |

> ⚠️ 当前已超标的文件（重构候选）+ Soft-warn 监视清单，见 [`docs/architecture-log.md`](./docs/architecture-log.md) 的 Baseline snapshot（用 `wc -l` 实测维护）。新增功能时优先考虑拆分这些文件，而非继续往里堆。手动清单易漂移，以 architecture-log 为唯一信息源。

**禁止事项**：
- ❌ 重复造轮子：写新逻辑/样式/正则前必须先搜（grep/Grep/Explore），已有则复用。
- ❌ 空catch {}：错误至少 `announce` 或显式注释「为何可忽略」。
- ❌ `window.alert` / `window.confirm` / `window.prompt`：用内联校验 + `GlobalModals`。
- ❌ 硬编码颜色/z-index/间距：用 `var(--token)`（见 `src/styles/_tokens.scss`）。
- ❌ 同一概念多份实现：连接状态等用权威定义点（见指南 §5）。
- ❌ 死代码：未被 import 的模块确认后删除。
- ❌ 靠猜测代替事实：对外部环境（家目录/工具链/locale/时钟域/就绪信号/身份键）不做臆断——协议求证（SFTP canonicalize/stat、pty 终端模式）、解析命令输出加 `LC_ALL=C` + POSIX 选项（写法必须 `env LC_ALL=C <cmd>`，`VAR=值 cmd` 在 csh/tcsh 下整条命令失败）、比较同钟域、身份用完整键、探测三态（真/假/未知按保守处理）。详见指南 §7；**机械门禁**：`npm run lint:facts`（`scripts/fact-guards.mjs`，14 条规则，随 build/CI/发版强制执行）。新增远程能力**默认走 SFTP/协议**，确需 exec 解析输出时注释平台假设与降级策略。
- ❌ 靠形状猜测（形态 E）：用正则/前缀/子串去认结构化对象（命令串/路径/设备树/身份）必被等价写法绕过——命令按 shell 词法分段后**逐段**判定（`df -h; cat /etc/shadow` 不得因前缀 `df` 免审批）、路径先归一再按**组件**判定（`./.ssh/id_rsa`、`\\?\UNC\...`、`/home/x/.ssh/id_rsa` 同罪）、设备按**拓扑**判重（LVM/RAID 的 `dm-*`/`md*` 与物理盘记同一份 IO）。
- ❌ 失败朝宽松方向折叠：涉及审批/凭据/拦截等级/数据覆盖的读盘或解析失败，**只允许回落到更严的一档**（fail-closed）+ 日志 + 损坏文件隔离改名；只有「文件不存在 = 用户无偏好」才可用产品默认档。「读不出来」永远不等于「空数据/不存在」——已两次导致静默丢数据（拦截等级降级为零审批放行、整份审计日志被 1 条覆盖）。

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
| 语言 | **TypeScript**（strict 全量，vue-tsc 类型门禁） | `typescript ^5.9.3` + `vue-tsc ^3.3.11` |
| 状态管理 | **Pinia 3**（setup store 风格） | `pinia ^3.0.4` |
| 图标 | **lucide-vue-next** | `^0.460.0` |
| 终端 | **xterm.js 6** + addon-fit/search/serialize/web-links/webgl | `@xterm/xterm ^6` |
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
│   ├── main.ts                 # 应用入口：createApp(App).use(createPinia()).mount('#app')
│   ├── App.vue                 # 根组件：按 query 分支（?win=asset&assetId= → AssetWindowShell，否则 WorkbenchShell）+ initialize 启动加载
│   ├── components/
│   │   ├── shell/              # 外壳：ConnectionSidebar / AssetGroupNode(递归) /
│   │   │                       #        GlobalModals(弹窗中枢) / OpsSummaryPanel / RightSidebar
│   │   ├── workbench/          # 实际运行的外壳：WorkbenchShell（标题栏/状态栏/拖拽条）/
│   │   │                       # AssetWindowShell（资产独立窗口壳，复用 Terminal/File/RightSidebar）
│   │   ├── terminal/           # 终端：TerminalSurface / TerminalTabs / TerminalToolbar
│   │   ├── files/              # 文件：FileSurface / FileColumn / ...
│   │   ├── resource-monitor/   # 资源监控：Cpu/Memory/Network/Disk 图表 + chart-utils
│   │   └── ui/                 # 基础组件库：App{Input,Button,Select,Modal,Drawer,
│   │                           #            ContextMenu,Tooltip,Table,Tab,Progress,...}
│   │                           #   index.ts barrel 导出全部
│   ├── stores/                 # Pinia stores（8 个：7 领域 + 1 编排壳）
│   │   ├── workbench.ts        # 编排壳：实例化 7 个子 store（不含 resourceMonitor），initialize() 启动加载
│   │   ├── sessions.ts         # 活跃 SSH 会话 + 终端生命周期
│   │   ├── assets.ts           # 连接资产 CRUD + 分组树
│   │   ├── files.ts            # SFTP 文件 + 传输队列
│   │   ├── tunnels.ts          # SSH 隧道/端口转发
│   │   ├── ui.ts               # UI 状态：主题/tab/modal/搜索
│   │   ├── resourceMonitor.ts  # 资源监控轮询 + 事件订阅（不经 workbench，由 panel 直接 use）
│   │   ├── mcp.ts              # 【v1.2】MCP 探测状态 + 配置引导（refresh 触发探测，无事件监听）
│   │   └── sync.ts             # 【v1.3】Gist 资产同步（push/pull/冲突解决/状态展示）
│   ├── composables/            # useTheme / useClipboard / useTerminalConfig /
│   │                           # useAutoReconnect / usePanelResize / useAutoUpdate /
│   │                           # useGithubDeviceLogin（Device Flow 状态机 + 传输故障退避重试）
│   ├── lib/                    # terminalThemes / dangerousCommands / terminalGuards /
│   │                           # transferUtils / assetWindows+assetWindowBoot（资产独立窗口纯函数模块）
│   ├── services/
│   │   └── backend.ts          # Tauri IPC 桥：invokeBackend / listenBackendEvent /
│   │                           #                 normalizeAsset / slugify
│   ├── types/                  # 共享类型定义：tauri.d.ts（Tauri invoke/event 类型 shim）、
│   │                           # domain.ts（跨模块领域类型）
│   └── styles/                 # SCSS：_tokens(设计token) / _base / _utilities / main
├── src-tauri/                  # Tauri/Rust 后端
│   ├── src/
│   │   ├── main.rs             # 二进制入口（tauri::run 壳）
│   │   ├── lib.rs              # AppState + 资产/凭据命令 + generate_handler 注册 + mcp_status
│   │   ├── ssh.rs              # SSH/SFTP/隧道核心（SshSessionManager + russh Handler）
│   │   ├── resource_monitor.rs # 远程 CPU/mem/net/disk 轮询（SshCommand::MonitorExec）
│   │   ├── fs_local.rs         # 本地文件系统命令
│   │   ├── sync.rs             # 【v1.3】Gist 同步命令层（push/pull/conflict，粘合 core sync + reqwest）
│   │   ├── sync_oauth.rs       # 【v1.3】GitHub OAuth Device Flow 登录（v2.7 抗抖动：单例客户端 + 结构化可重试判定）
│   │   ├── dpapi_codec.rs      # 【v1.3】DPAPI 凭据编解码（Windows CryptProtectData，cfg(windows)）
│   │   ├── bin/mcp.rs          # 【v1.4 已删】原 myshelltool-mcp 独立 console bin，内嵌后取消双二进制
│   │   └── mcp/                # MCP server 接入模块（v1.4 内嵌 GUI / Streamable HTTP transport）
│   │       ├── http_server.rs  # 【v1.4】Streamable HTTP server：axum + rmcp，绑定 127.0.0.1:41235/mcp
│   │       ├── server.rs       # rmcp ServerHandler 实现（transport 无关，11 工具/4 资源/3 prompts）
│   │       ├── tools.rs        # MCP Tools 核心分发与系统类工具
│   │       ├── file_policy.rs  # 【v2.2】文件操作安全策略（路径防穿越、敏感凭据判定、操作分级）
│   │       ├── file_tools.rs   # 【v2.2】文件工具集（sftp_list/read/write/upload/download/remove）
│   │       ├── sftp_ops.rs     # 【v2.2】Headless SFTP 底层封装（流式传输、原子写、递归删）
│   │       ├── approval.rs     # 审批判定：白名单放行 / elicitation / 进程内拒绝
│   │       ├── probe.rs        # 【v1.4】HTTP 健康检查：向自己的 endpoint 发 initialize 握手
│   │       ├── resources.rs    # 3 静态资源 + 1 template（assets/sessions/known-hosts/session-log）
│   │       └── prompts.rs      # 3 诊断 prompt（diagnose_server/audit_security/cleanup_disk）
│   ├── capabilities/default.json
│   └── tauri.conf.json         # com.redtei.myshelltool，1366×800，withGlobalTauri:true
├── crates/
│   └── myshelltool-core/       # 共享核心库（无 Tauri 依赖，可独立 cargo test）
│       └── src/
│           ├── lib.rs          # ConnectionAsset / SecretStore / 资产持久化 / 资产校验
│           ├── dangerous_commands.rs # 【v2.6 迁入】白/黄/黑/Unknown 四层命令分类 + 毁灭层（GUI 与 MCP 共享单点真相，fail-secure 默认拒）
│           ├── redact.rs       # 【v2.6】命令文本脱敏（`mysql -pP@ss` 等落日志前的凭据红线，`redact_command`/`redact_excerpt` 从 crate 根再导出）
│           ├── oauth_flow.rs   # 【v2.7】GitHub Device Flow 轮询判定（传输故障/429/5xx/非 JSON = 可重试，协议拒绝 = 终止；纯函数可单测）
│           ├── remote_text.rs  # 【v2.6】远端文件内容编码判定（二进制/非 UTF-8/UTF-8 三态，拒绝对内容做 lossy「解码」）
│           └── shell.rs        # 【v2.6 迁入】shell 命令分段器（引号感知；命令层安检的结构判据，替代整串前缀匹配）
├── tests/
│   ├── ui-smoke.mjs            # UI 冒烟：5 区域 + 资源监控占位（Playwright）
│   └── ui-host-key.mjs         # Host key 验证流程 UI 测试
├── docs/                       # 文档
├── .claude/                    # Claude Code 配置（见 .gitignore，部分本地）
├── .omc/ .omx/                 # 其他 agent 工具状态（见 .gitignore）
├── package.json                # npm scripts（见 §5）
├── vite.config.ts              # root=src，@→/src alias，scss loadPaths，port 41234 strict
└── AGENTS.md                   # ← 本文件
```

---

## 4. 约定（硬约束，违反会破坏一致性）

### 4.1 前端
- **Vue 3 `<script setup lang="ts">`** + Composition API。**禁止** Options API，**禁止**新增无 `lang="ts"` 的 script。
- **TypeScript strict 全量**：props 默认值必须用 `withDefaults`；跨模块共享的领域类型放 `src/types/domain.ts`；`any` 仅限动态边界（IPC payload 等）且须注释原因。
- **状态用 Pinia setup store**（`defineStore('x', () => {...})` 返回 refs/computeds/actions）。
- **图标统一用 `lucide-vue-next`**，不要引入其他图标库或内联 SVG（资源监控图表除外，用自绘 SVG）。
- **样式用 SCSS + 设计 token**（`@/styles/_tokens.scss`）。组件内 `<style scoped lang="scss">` 顶部 `@use '@/styles/_tokens' as *;`。
  - 颜色/间距/圆角/阴影/动效/z-index **必须用 `var(--xxx)` token**，不要硬编码。z-index 用 `var(--z-base|dropdown|sticky|drawer|modal|toast|tooltip)`。
  - 新增 token 加到 `_tokens.scss` 的 map 并暴露为 `:root` CSS 变量。
- **路径别名 `@`** → `src/`（vite.config.ts 配置）。import 用 `@/stores/...`、`@/components/...`。
- **组件分层**：`ui/` 是无业务的基础组件（`App*` 命名，barrel 导出）；业务组件按域放 `shell/`、`terminal/`、`files/`、`resource-monitor/`。
- **通用操作入口模式**：右键菜单用 `AppContextMenu`（`items: [{label, action, danger, separator, disabled}]`），参照 `FileSurface.vue` 用法。危险操作用 `danger: true`（红色）或 `AppButton variant="danger"`。
- **弹窗**：业务弹窗统一走 `GlobalModals.vue`（`store.modal = { type, ...payload }`），按 `modal.type` 分支。新增 type 需同步改 `modalTitle` / `submitModal` / `watch`。保留 legacy 选择器（`#modalLayer`/`#modalBody`/`.modal-actions .btn.danger`）以兼容测试。

### 4.2 状态管理（跨 store 桥接）
- **`workbench.ts` 是编排壳**：实例化 7 个子 store（sessions/files/tunnels/assets/ui/mcp/sync），`initialize()` 编排启动加载，用 plain-object 返回 + `computed()` 包裹子 store 的响应式 state（**不要直接暴露子 store 的 ref**，会丢响应性）。**注意 `resourceMonitor.ts` 不经 workbench 编排**——它由 `ResourceMonitorPanel.vue` 直接 `useResourceMonitorStore()` 使用（独立轮询生命周期，与全局初始化解耦）。
- **跨 store 依赖用 lazy bridge**：子 store 通过 `attachWorkbench(bridge)` 注入跨 store 访问（如 assets store 调 workbench.announce / workbench.modal）。**禁止循环 import**。
- **新 action 加到子 store**，再在 `workbench.ts` return 块 re-export（参照 `saveAsset`/`deleteAsset` 模式）。

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
npm run build        # 前端构建 = 事实门禁（lint:facts）→ vue-tsc 类型检查 → Vite 构建
npm run tauri:build  # 完整桌面安装包（Windows NSIS），beforeBuildCommand 走 npm run build 自动含门禁

# —— 测试 ——
npm run test:core    # Rust core 单元测试：cargo test --manifest-path crates/myshelltool-core/Cargo.toml
npm run test:ui      # UI 冒烟：node tests/ui-smoke.mjs && node tests/ui-host-key.mjs && node tests/ui-file-loading.mjs（需先 npm run dev 起服务）

# —— 静态检查（各自单跑）——
npm run lint:facts   # 「靠猜测代替事实」门禁：scripts/fact-guards.mjs（指南 §7）；build 首步即跑，CI/发版同步生效
npm run type-check   # vue-tsc --build，strict 全量；build 亦含此步

# —— 后端单独验证 ——
cd src-tauri && cargo build       # 验证 Rust 编译
cd src-tauri && cargo check       # 更快的类型检查

# —— 仅类型检查（不产 dist）——
npm run type-check   # vue-tsc --build，strict 全量；build 亦含此步
```

**改完代码必须跑的验证（开发闭环）**：
- 改前端 → `npm run build`（验证编译）+ 如改了交互 `npm run test:ui`。
- 改 Rust → `cd src-tauri && cargo build`。
- 改 core 持久化 → `npm run test:core`。
- **如实报告结果**：贴 exit code / 关键输出，失败就说失败。

---

## 6. IPC 契约（前端 ↔ Rust，关键命令清单）

前端通过 `src/services/backend.ts` 的 `invokeBackend(command, args)` / `listenBackendEvent(event, handler)` 调用 Rust。**非 Tauri runtime 会抛错**（浏览器预览模式无 SSH 功能）。

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
- `sync_status` → 同步配置状态（是否已配置 / 上次同步时间 / gist_id 掩码 / `auto_sync_enabled`＝本机已记住免密密钥 / **`local_has_changes`【v2.7】＝本机有未推送改动**，面板据此决定「推送到云端 / 从云端拉取」谁是主按钮）
- `sync_setup({ masterPassword, gistId? })` → 首次设置（主密码派生密钥 + 可选拉取已有 Gist）。**【v2.7】成功后自动调用 `remember_session_key`**：按该载荷的 salt 重建 key、DPAPI 存盘并置 `auto_sync_enabled = true` —— 主密码只输这一次（或换机时），此后 push/pull 与自动同步全免密
- `sync_push({ masterPassword })` / `sync_pull({ masterPassword })` → 加密推送 / 拉取解密（pull 返回 Conflict 时前端弹框）。`masterPassword` **留空 = 走本机会话密钥**（免密路径，需已启用）；**带密码且本机还没有密钥时顺手记住**（`remember_session_key`，按该载荷的 salt）——「输一次密码」就够开启免密
- `sync_resolve_conflict({ masterPassword, choice })` → 冲突解决（local_overwrite / remote_overwrite）
- `sync_reset_master_password({ old, new })` / `sync_clear`
- **`sync_generate_recovery_password()`**【v2.7】→ 生成 24 位高熵恢复密码（去歧义字符集，`core::recovery_code`）→ 存 SecretStore（id `sync-recovery-password`，DPAPI）→ **返回明文**供界面填入并明示一次。让用户"零输入"也能建备份，且换机时有据可依（不依赖外部密码管理器）
- **`sync_reveal_recovery_password()`**【v2.7】→ 读回已保存的恢复密码（供「换机恢复 → 查看这台电脑的恢复密码」）；用户手输的密码从未保存，此处返回 None 时前端如实说明"应用没有保存它"

**GitHub Device Flow 登录**（`sync_oauth.rs`，v1.3；【v2.7】抗抖动改造）
- `sync_oauth_start({ provider })` → `{ userCode, verificationUri, expiresIn, interval }`（`Err` = 请求设备码失败/未注册 client_id）
- `sync_oauth_poll()` → tag enum `status`：`pending` / `slow_down`（带 `user_code`、【`interval`】）| `denied` | `expired` | `success` | **`unstable{user_code, reason}`**（本次没问到：网络/代理抖动、429/5xx、非 JSON 响应 → 前端退避重试，**设备码仍有效**）| `superseded`（后端已无本流程 session → 前端静默回 idle）| **`failed{reason}`**（GitHub 明确报错 / token 落库失败 → 终态）。`Err` 只留给后端内部错误（锁中毒）
- `sync_oauth_cancel()` → 清内存槽（单槽：新 start 覆盖旧 start）

**SSH 会话/终端**（`ssh.rs`）
- `ssh_connect({...})` → 返回 `session_id`；参数含 host/port/username/authMethod/credentialId/privateKeyPath 等
- `ssh_list_directory` → 一次性 SFTP 列目录（走独立连接开 sftp 子系统，空 path 时 canonicalize 家目录；不依赖远端用户态工具，曾用 GNU-only `find -printf` 在 BusyBox/BSD 上必挂，已弃用）
- `ssh_write` / `ssh_resize` / `ssh_disconnect` / `ssh_confirm_host_key` / `ssh_keyboard_response`

**SFTP**（`ssh.rs`）
- `sftp_list_dir` / `sftp_read_file` / `sftp_write_file`
- `sftp_list_dir` 的 **path 为空串 = 服务器默认目录**：后端 `canonicalize(".")` 解析登录用户真实家目录（root → /root）并在返回的 `path` 字段回传绝对路径；前端不再猜测 home（旧版 `/home/<username>` 对 root 必错、tag 硬编码目录不存在即 No such file，已废弃）
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
- `mcp_get_config` → 【v2】读 MCP 危险命令拦截等级（内存共享配置，不动盘）。
- `mcp_set_config({ level })` → 【v2】切换拦截等级（`minimal` 仅硬拦毁灭性命令、其余直接执行 / `strict` 非白名单一律确认；毁灭性命令两档恒拦）→ 更新共享 Arc（已建 MCP 会话下次调用即生效）→ 落盘 mcp-config.json；无效 level 返 Err。
- `mcp_list_execution_logs({ limit? })` → 【v2】读 MCP 工具执行日志最近条目（timestampMs 倒序，limit 缺省 200）。
- `mcp_clear_execution_logs` → 【v2】清空 MCP 执行日志。
- 【v1.4 已删】`mcp_approval_resolve`（原 v1.1 pipe 审批回传，内嵌后无 pipe）

### 事件（Rust → 前端，`listenBackendEvent`）
- `sftp-transfer-progress`、`ssh-host-key-verify`、`ssh-keyboard-interactive`、`ssh-session-status`
- 【v2.5】`resource-monitor-error`（payload camelCase `sessionId`/`reason`）：远端无 /proc（非 Linux）等不可恢复采样失败，emit 后该会话轮询即停止，前端展示错误态
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
- 前端经 `normalizeAsset()`（`backend.ts`）规整：默认 port=22、group=「未分组」、status=Idle。
- **分组不是独立实体**，是 `asset.group` 字符串字段（`/` 分隔层级）。空分组用 `ConnectionAssetStore.groups: Vec<String>` 单独持久化。
- 「未分组」是保留顶级，不可重命名/解散。

### 持久化分层
- **资产元数据** → `connection-assets.json`（JSON-on-disk，无密钥）
- **凭据** → `credentials/<id>.cred`（弱 XOR 混淆，非加密）
- **同步状态 / 会话密钥** → `sync-state.json`（gist_id / local_rev / auto_sync_enabled / last_synced_at，无秘密）+ `credentials/sync-session-key.cred`（payload = `base64(key):base64(salt)`，DPAPI 保护）。**【v2.7】`sync_setup` 成功即写入**（免密默认化），salt 必须与「该次载荷的 salt」一致（`core::sync::session_key_for_payload`），否则免密路径与主密码路径拿到两把不同的 key
- **恢复密码** → `credentials/sync-recovery-password.cred`（DPAPI）。**【v2.7】只保存应用生成的那份**（用户点「生成强密码」时写入）；用户手输的密码一律不落盘（§8 凭据红线的范围）。`sync_setup` / `sync_reset_master_password` 成功后若发现保存值与本次使用的主密码不同 → **删除**（那份已打不开当前备份，留着会让「查看」给出错值）
- **Gist 同步载荷** → `SyncPayload { version, blob{salt,nonce,ciphertext}, remote_rev }`。**【v2.7】`blob.salt` 必须非空**：会话密钥 = `Argon2id(主密码, salt)`，salt 不随载荷上传时换机后主密码无法重建同一把 key（备份永久解不开）。会话密钥路径与主密码路径共用同一 salt，故**同一份密文两条路都能解**；`crypto::encrypt_with_key` 对空 salt 直接返回 Err（fail-closed，杜绝再产出「只有本机能解」的载荷）。**【v2.7】只支持这一种格式**：`parse_vault_plaintext` 只认 `SyncVaultData`（不再兼容旧版「纯 assets JSON」），`crypto::decrypt` / `decrypt_with_key` 对空 salt 统一返回 `LEGACY_PAYLOAD_REJECTED`（明确说「旧格式不再支持，请重推一次」）——不允许出现「密码路径报错、会话密钥路径却能悄悄解开」的半支持状态
- **known_hosts** → `known_hosts.json`
- **MCP 拦截等级** → `mcp-config.json`（【v2】minimal/strict，缺省 Minimal；与 endpoint 同目录，经 mcp_data_dir() 解析）
- **MCP 执行日志** → `mcp-execution-log.json`（【v2】30 天惰性清理 + 上限 1000 条，append 时触发；tokio Mutex 串行化 + `.tmp`/rename 原子写；不含任何凭据字段）
- **活跃会话/隧道/SFTP 缓存** → 纯内存（重启丢失）

### McpStatus / McpProbeResult（MCP 健康检查，`lib.rs` + `mcp/probe.rs` + `mcp/http_server.rs`）
- **设计取舍（v1.4）**：MCP server 内嵌 GUI 进程，用 Streamable HTTP transport 对外暴露（`http://127.0.0.1:41235/mcp`）。状态灯 = HTTP 健康检查：GUI 每次 `mcp_status` 向自己的 endpoint 发 MCP `initialize` 握手，成功即「可用」。**不再 spawn 子进程**（v1.2 的一次性 spawn 已废弃，根治僵尸进程 + os error 32）。
- **v1.4 架构变化**：取消双二进制（删 `bin/mcp.rs` + `pipe.rs`），MCP server 直接跑在 GUI 进程内，SSH 会话/资产/审批同进程访问。任何合规 MCP host（Claude Code / Cursor）经 HTTP URL 连入。
- `McpProbeResult { ok: bool, reason?, detail?, exePath, serverInfo?, probedAt }`
  - `reason` 失败分类码：`endpoint_not_found`（server 未启动/未写 endpoint 配置）/ `http_error`（连不上）/ `timeout`（2s）/ `bad_protocol`（握手响应异常）
  - `exePath`：v1.4 语义改为 HTTP endpoint URL（字段名保留兼容前端 mcp.ts）
- 前端 `mcp.ts` 的 `clientConnected` computed 读 `probe.ok`（命名保留是为避免连锁改名，语义已是健康检查结果）。

---

## 8. 安全设计红线

- 凭据（密码/私钥/passphrase）**只走 SecretStore**，不进资产 JSON、日志、错误信息、前端 console。
- 危险文件操作（删除、覆盖）**必须弹窗确认**。
- Host key 变更默认**阻止连接并警告**。
- 远程命令执行/隧道监听 `0.0.0.0` 需安全审视。
- **【v1.6】同步主密码绝不落盘**：资产同步的主密码（master password）在派生 AES 密钥后即丢弃，绝不存盘。自动同步功能用「会话密钥 + DPAPI 保护」绕过每次输密码：首次启用时用主密码派生固定 AES key（Argon2id，确定性），该 key 经 DPAPI（User scope，绑定 Windows 用户登录态）加密后存 SecretStore（credential id = `sync-session-key`）。离机即失效，非 Windows 或 DPAPI 失败时降级为手动主密码模式（不静默失败）。**【v2.7】该 key 的派生 salt 随载荷上传**（`blob.salt`，salt 本身不敏感），于是「离机即失效」只针对会话密钥本身，**备份仍可用主密码在任意机器恢复**——修复前 salt 只存本机，自动同步过的备份换机后连主密码都解不开（详见 §9 v2.7 续修）。
- **【v2.7】上述红线的边界（一次有理由的例外）**：`credentials/sync-recovery-password.cred` 会保存**应用自己生成**的高熵恢复密码（用户点「生成强密码」时）。判据是**谁选的秘密**：
  - 应用生成的 24 位随机串**只服务这一份备份**，不存在"复用别处口令"的泄漏面；而它防护的风险（"用户想不出强密码 / 忘了 / 把恢复凭据押在外部密码管理器上"）是真实且更常见的失败。落盘经 DPAPI，且注意：**免密功能本来就已经把由它派生的密钥存在本机**了，多存一份明文密码只多出"可跨机使用"这一项能力 —— 而这正是换机恢复的定义。
  - **用户手输的密码一律不保存**（可能是他别处复用的口令）：`sync_reveal_recovery_password` 在那种情况下返回 None，界面如实说"应用没有保存它，请自行记牢"。
  - 一致性：保存值与实际使用的主密码不符时必须**删除**（否则「查看恢复密码」给出打不开备份的错值，用户抄走后换机报"密码错误"且无从归因）。

---

## 9. 已知边界 / Follow-ups（改这些时要留意）

- `sftp_download_with_progress` 仍返回整块 `Vec<u8>`（upload 已分块，download 待改造）。
- `start_remote_forward` 是返回 Err 的桩（local/dynamic SOCKS5 已实现）。
- `sanitize_credential_id` 是**删除式清洗**：仅保留字母数字与 `-`/`_`，其余字符直接删除（`192.168.2.2:password` → `19216822password`）。已知限制：清洗不可逆、不同 id 可能碰撞（`a:b` 与 `ab` 写同一个 `<id>.cred` 文件）；若改映射规则需迁移既有凭据文件（暂不改）。
- Windows 上 `cargo build` 偶因 build script（windres）阻断，用 `cargo check` 兜底；`cargo test` 的 src-tauri 测试二进制会因 Tauri runtime DLL 缺失报 `STATUS_ENTRYPOINT_NOT_FOUND`，用 `cargo check --tests` 验证测试可编译。
- `workbench.ts`（实测 507 行）是编排壳而非纯 re-export 壳：实例化 7 个子 store + `initialize()` 启动编排，另含 5 个真实路径 helper（`remotePathForAsset`/`parentPath`/`joinPath`/`joinLocalPath`/`parentLocalPath`），return 块 re-export 子 store 的 state/actions。
- **【v1.4】MCP 内嵌 GUI（Streamable HTTP）**：MCP server 跑在 GUI 进程内，绑定 `127.0.0.1:41235/mcp`（占用则 +1，写 `<data_dir>/mcp-endpoint.json`）。取消双二进制（删 `bin/mcp.rs` + `pipe.rs`），根治 v1.2 的僵尸进程 + os error 32 + NSIS 打包缺口。MCP server 随 GUI 启停（`CancellationToken` 控制 graceful shutdown）。
- **【v1.4】MCP 端口策略**：默认 41235，被占用则 +1 重试最多 10 次，**只监听 localhost**（§8 安全红线）。实际端口写 mcp-endpoint.json，前端 `mcp_status.endpoint` 返回。
- **【v1.4 follow-up】会话复用**：`tools.rs::exec_on_asset` 当前直走 headless 建连（删了 v1.1 pipe 复用分支）。后续可注入 GUI 的 `Arc<AsyncMutex<SshSessionManager>>` 到 McpToolContext，命中已建立会话时直接复用（同进程访问，比 pipe 更简单）。
- **【v2】MCP 审批三级降级 + 拦截等级可配置**：v1.5 已实现三级降级（elicitation → AppHandle emit `mcp-tool-approval` GUI 弹窗 + 60s oneshot 超时 → headless/emit 失败才 fail-secure 拒），不再是「不支持 elicitation 直接拒」。v2 起拦截等级用户可配置（`mcp-config.json`，GUI 经 `mcp_get_config`/`mcp_set_config` 读写）：默认 **Minimal** 仅硬拦毁灭性命令（机器报废级：rm 根级删除如 `rm -rf /`、mkfs、dd 写块设备、fork 炸弹、chmod -R 系统目录），其余命令（含 reboot、`rm -rf 目录`、`curl|bash` 等黑名单级）不经确认直接执行（用户明确选择的低摩擦默认，放行记执行日志 `minimal_allowed` 供审计）；**Strict** 非白名单一律人工确认（保留原 fail-secure 语义）。毁灭性命令两档下均直接拒绝、不弹审批（decision 记 `hard_blocked`，命令未执行）；`find` 含 `-delete`/`-exec` 不再白名单放行（落黄层：Minimal 自动放行记日志 / Strict 人工确认）；sftp_remove 已于 v2.3 纳入等级体系，仅根级/核心目录删除恒拦。等级存共享 `Arc<RwLock<McpConfig>>`，改档后已建 MCP 会话下次调用即生效。`ssh_exec` 等工具返回结构化文本（exit_code + stdout + stderr；超 16000 字符自动截断保留头/尾各 8000，提示用 grep/tail 收窄）。
- **【v2】MCP 执行日志**：`server.rs::call_tool` 为真实触发远程执行的工具（ssh_exec/disk_usage/system_status/service_status/sftp_remove）记一条 `mcp-execution-log.json`（哪台服务器/什么命令/什么决策/什么结果），前端 MCP 面板经 `mcp_list_execution_logs`/`mcp_clear_execution_logs` 查看/清空。30 天惰性清理 + 上限 1000 条（append 时触发，无常驻定时任务）；tokio Mutex 串行化 append/clear，`.tmp`+rename 原子写，落盘失败 best-effort 不阻断工具调用。Entry 不含任何凭据字段（§8 红线）。
- **【v2.2】MCP 文件传输全能力与全工具无桩化**：新增 `file_policy.rs`、`file_tools.rs`、`sftp_ops.rs`，提供 `sftp_list`、`sftp_read_file`、`sftp_write_file`、`sftp_upload`、`sftp_download`、`sftp_remove` 全套文件操作通道。`list_sessions` 接入活跃 GUI 会话池，`resource_monitor_snapshot` 接入真实系统资源快照，实现 MCP 工具 100% 无桩化。严格践行安全红线：文件工具（sftp_write_file/sftp_upload/sftp_download/sftp_remove）的审批已纳入拦截等级体系（v2.3）——Minimal 放行记 minimal_allowed 日志 / Strict 人工确认；例外：根级毁灭性删除与本机核心系统目录写恒 HardBlock，敏感凭据文件读取恒审批（凭据红线）；`sftp_read_file` 执行日志强制脱敏，凭据决不落盘。
- **【多窗口】资产独立工作台窗口（Tauri 2 多 WebviewWindow）**：侧栏资产**拖出主窗口边界**释放或右键「在独立窗口打开」→ 创建独立 OS 窗口（label=`asset-<sanitized assetId>`，重复开聚焦已有窗口），加载 `/index.html?win=asset&assetId=`，App.vue 按 query 分支渲染 `AssetWindowShell`（标题栏 + 上终端/下文件 + 完整可收起右栏监控，无左栏）。**每窗口独立 webview/Pinia 实例、共享 Rust 后端**（Rust 零改动）；侧栏拖出/右键入口新建自己的会话。关键约束：① `resourceMonitor.applySnapshot` 按 sessionId 过滤（全局广播事件防双窗口串流）；② sessions 的 host-key/keyboard handler 有 `ownsConnectPrompt` 守卫（防跨窗口弹错资产名的确认框；v2.6 起同时认领「在途一次性连接」登记表，见下方 v2.6 前端事实修正）；③ asset 模式 `initialize({mode:'asset'})` 跳过 MCP/sync 初始化（MCP 审批弹窗只在主窗口）；④ 关窗走 onCloseRequested 确认（connecting 会话先等 settle ≤10s）→ 断开本窗口会话 → destroy；⑤ asset 窗口布局用独立 storageKey（`myshelltool:layout-asset:v1`）、右栏折叠不写共享 localStorage。capabilities windows 含 `asset-*` glob。已知限制：窗口不持久化恢复、资产删除不跨窗口同步、主题跨窗口不实时同步、Esc 取消的窗外拖拽可能误开窗（待实测）。
- **【多窗口·tab 迁移】终端 tab 跨窗口拖出/合并（会话所有权迁移）**：主窗口 connected 终端 tab 拖出窗外释放 → **迁移会话**（不重连）到该资产独立窗口：`src/lib/sessionHandoff.ts` 导出 scrollback（**@xterm/addon-serialize 序列化，带 SGR 颜色/样式**——`exportTerminalScrollback`，末 2000 行、1M 字符预算，超预算头部截断后 prepend `\x1b[0m` 防 delta 式 SGR 断链错色；addon 不可用时回退 `exportTerminalText` 纯文本。曾长期纯文本导出，迁移后历史整体褪成默认前景色、蓝色 prompt 变白，v2.4 修复）经 **Rust 内存中转**（`session_handoff_put`/`session_handoff_take` 命令，AppState `Mutex<HashMap>` + TTL 60s，take 即原子删除——曾用 localStorage 中转但 WebView2 跨窗口不实时共享导致 adopt 恒失败，已废弃）→ `await session.unlisten()`（先解绑防双写）→ `removeSessionEntry`（仅移 UI，不 ssh_disconnect，会话在后端存活）→ 新窗口 URL 带 `&adopt=<sessionId>`，boot 经 `sessionsStore.adoptSession` 重建 xterm（`createTerminalForAsset` 与 connectSelected 共用 helper，每 session 挂 SerializeAddon）+ `registerSessionStream` + 100 行分块 rAF 回放。**独立窗口无 tab 条**（TerminalSurface `showTabs` prop，窗口即会话），回迁主窗口走**标题栏「移回主窗口」按钮**（`pushSessionToMainWindow` push 协议：迁移 + MERGE_PUSH 事件 + 主窗口 adopt，迁移后空窗自动关窗；主窗口已关则拒绝迁移防会话无主）。协议剩两事件（`session-handoff-tearoff`/`-merge-push`，`sourceWindowId` 防自吞，TEAROFF 仅 asset 窗口且 assetId 匹配、MERGE_PUSH 仅主窗口监听）；跨窗口 drop 合并（pull 协议）已随 tab 条删除而移除（WebView2 跨窗口自定义 MIME 不可靠，按钮替代）。已知限制：回放不含 alt 屏与软换行（vim/less 中拖出只还原进 alt 前内容、超宽行拆行）、颜色状态在极端截断时回落默认色；迁移瞬间输出丢失；Esc 取消的窗外拖拽会误触发拆出；溢出折叠 tab 不可拖。
- **【v2.3】文件面板与终端会话生命周期联动**：sessions store 断开/关闭/连接成功时经 workbench bridge 通知 files store（`handleSessionClosed` 清空面板 / `handleSessionConnected` 延迟 600ms 静默自动加载——SFTP 通道就绪前抢跑会报错）。终端 cd 跟随：连接后注入 bash 专属 OSC 7 上报。**无痕注入三件套**：① Rust 端 pty 建连即以 ECHO=0 打开（`ssh.rs` request_pty `Pty::ECHO`），注入行全程不可见——早期「`stty -echo` 两段式」与「等首段输出再注入」闸门在慢 .bashrc 机器都会产生可见噪音/双重回显（MOTD 横幅是 sshd 在 shell 启动前打印的，不能作 shell 就绪信号）；② 单行注入：payload 经 `eval '...'` 包裹、行尾 ` stty echo` 恢复回显（置于 eval 之外，fish 拒Parse 也不丢回显恢复）、`test -n "$BASH_VERSION" && printf '%b' '\e[1A\e[G\e[J'` 擦除多余提示符行（bash 专属：交互 bash 每执行一行命令重画一次 PS1，readline accept 必输出换行故上移一行精确落回提示符行）；③ 前导空格不进 HISTCONTROL=ignoreboth 历史、case 幂等守卫、BASH_VERSION 门控（zsh 跳过）。局限：擦除仅对 bash 启用（zsh/dash/fish 留一个空提示符，宁留不冒险擦真实内容）；多行 PS1 擦除会留残片（已知未修）；`createOscParser` 解析 OSC 7 → `syncTerminalCwd` 跟随切换（400ms 节流、仅面板当前绑定资产生效；跟随加载为 silent——失败仅空态展示+重试，不弹 toast）。注意：Tauri 后端 `Err(String)` 在前端以**字符串** reject（无 `Error.message`），错误处理必须显式取字符串，否则任何后端失败都显示成「未知错误」；SFTP 初始化/read_dir 失败已落 error 日志（`sftp init`/`sftp_list_dir` 前缀）。
- **【v2.5】资源监控失败态（不再发假快照）**：`build_snapshot` 返回 Result——远端无 /proc（非 Linux）导致主体不可信时停止该会话轮询并 emit `resource-monitor-error`（前端 `resourceMonitor.ts` 展示错误态）；次要段（net/diskstats/df）失败则快照照发、`degraded` 字段标注失败段（serde skip，None 不序列化）；`resource_monitor_snapshot` 命令无 handle/无可信快照时返回 None。
- **【v2.5】MCP 系统类工具逐段 rc 回声**：`disk_usage`/`system_status`/`service_status`/`resource_monitor_snapshot` 的命令串在每个子命令后附 `echo rc_<段名>=$?`——复合命令 shell 退出码只反映最后一条子命令（echo），管道段（如 `top | head`）的 rc 反映管道末端，POSIX 无可移植 PIPESTATUS，AI 宿主须按 `rc_*` 行逐段定位真实失败。
- **【v2.5】chmod -R 危险判定统一口径**：`/home`、`/Users` 递归 chmod 不再免拦截（`dangerous_commands.rs` 与前端 `dangerousCommands.ts` 同步收紧，GUI 终端守卫与 MCP 审批同一判定）——安全前缀仅 `/tmp`、`/var/tmp`；毁灭层豁免表另含 `/home`、`/Users`（家目录数据可恢复，走审批链而非两档恒拦）。前缀判定用 `under_safe_prefix`（相等或以 `<safe>/` 开头），`/home2` 等同前缀目录不再误判为安全。
- **【v2.5】同步安全加固**：本地资产 JSON 可读但损坏 → 中止同步并报错（不再静默折叠成空 vault 推 Gist 覆盖远端备份）；资产文件写回失败 → 报错且不推进 `last_synced_at`（防后续 pull 以「安全拉取」误判覆盖）；MCP 执行日志文件损坏 → 改名 `mcp-execution-log.corrupt-<unix秒>.json` 隔离保留证据后从空日志继续。
- **【v2.5】MCP SFTP 原子写兜底**：临时文件 rename 覆盖失败时**保留**临时文件（路径写入错误信息），供人工恢复，不静默丢数据。
- **【v2.5】本地系统目录黑名单加固（fs_local.rs）**：支持 UNC（`\\server\share`）与 `\\?\` verbatim 前缀；路径存在时先 canonicalize 归一再判（封 NTFS 8.3 短名如 `C:\PROGRA~1` 绕过）；家目录不可得时显式报错，不再回退 `.`（工作目录冒充家目录）。
- **【v2.6】事实门禁扩充到 14 条 + 安全判据可测化**：`scripts/fact-guards.mjs` 在 v2.5 的 7 条之上新增 6 条——`no-csh-hostile-env-prefix`（`VAR=值 命令`/`export VAR=值` 在 csh/tcsh 下不是赋值，整条命令失败；统一写 `env LC_ALL=C <cmd>`）、`no-pipeline-rc-echo`（管道后的 `echo rc_*=$?` 是管道末端的码，恒 0，会把「命令不存在」伪装成成功）、`no-fake-type-cast-call`（`(x as unknown as { m() }).m()` 断言后调用不存在的方法，TS 迁移后的新形态）、`no-empty-catch-block`（空 `catch {}` 必须带理由注释）、`no-drive-root-fallback`（字面盘符根当回退/临时目录）、`no-env-path-without-absolute-check`（环境变量当路径根须过 `var_os` + 非空 + 绝对三关）；第二轮又加 `no-unredacted-command-in-log`（命令文本落日志必须过 `redact_command`，见下）。同时把 `dangerous_commands.rs`、`shell.rs`（命令分段器）与 `redact.rs`（命令脱敏）放进 `crates/myshelltool-core`（`src-tauri` 侧 re-export 保持调用点不变）——src-tauri 的测试二进制受 Tauri runtime DLL 限制跑不起来，**安全判据必须在 `npm run test:core` 里真跑**（现 138 项，含白名单绕过/重定向/命令替换/verbatim UNC/私钥相对路径/脱敏正反例/原子写失败保留原文件/远端内容编码三态的回归用例）。
- **【v2.6】审批白名单不再「整串前缀匹配」（形态 E 事故）**：`classify_command` 曾对整串做前缀匹配，`df -h; cat /etc/shadow` 以 `df` 开头即判 `Safe` → Minimal 零审批执行、**Strict 档也免人工确认**。现按 `shell::split_shell_segments` 的词法分段（引号感知、转义分隔符不切段）**逐段**判定：每段都要命中白名单，且出现重定向（`>`/`<`）或命令替换（`$`/反引号）一律不进白名单（落黄层/Unknown → Minimal 记日志 / Strict 人工确认，不误升级为 HardBlock）。只读管道组合（`df -h | head -3`）在各段都在白名单内时仍放行。
- **【v2.6】敏感远端路径的相对形态漏判**：`file_policy::is_sensitive_remote_path` 按 `/.ssh/` 等**段**匹配，而 `sftp_list(path=".")` 返回的条目形如 `./.ssh/id_rsa`，归一后是 `.ssh/id_rsa`（无前导斜杠）→ 整类相对路径漏判、私钥可免审批读取（违反 §8 凭据红线）。现统一补前导 `/` 作为段边界哨兵（绝对路径幂等），并有相对/绝对两套回归用例。
- **【v2.6】MCP 拦截等级与执行日志的 fail-closed 化**：`load_mcp_config` 原先对「读失败/空文件/JSON 非法/未知 level」一律 `unwrap_or_default()` → **Minimal（零审批档）**，用户显式设的 Strict 会因一次断电或手改而静默降级。现三态分明：文件不存在 = 产品默认 Minimal（用户无偏好）；**存在但不可信 = `McpConfig::strict()` + log::error + 改名隔离 `mcp-config.corrupt-<秒>.json`**；写入改 `.tmp`+rename 原子替换（写侧不再自造损坏）。执行日志同理由「读失败当空日志」改为三态（NotFound/Loaded/Unreadable）：**读取失败时 append 拒绝落盘**并把原文件隔离，`clear_entries` 直接返回 Err——避免「读不出来 → 下一次 append 把整份审计历史覆盖成 1 条」。
- **【v2.6】远端环境假设修正**：MCP 命令串改用 `env LC_ALL=C`（`VAR=值 cmd` 在 csh/tcsh 下整条命令失败，FreeBSD root 默认 csh）；`top` 段改「先落临时文件再 head」，`rc_top` 不再是 `head` 的恒 0 码；`audit_security`/`cleanup_disk` prompt 的 Linux/GNU-only 命令（`ss`/`ps --sort`/`sort -h`）补平台分支与「命令失败必须如实报告该步未取得数据」的显式要求；`/proc/diskstats` 统计过滤**栈式设备**（`dm-*`/`md*`/`nbd*`/`zd*`/`drbd*`）——LVM/RAID 与底层物理设备记同一份 IO，旧实现全叠加会把磁盘读写虚高 2-3 倍（判据抽为可单测的 `sum_proc_diskstats`）。
- **【v2.6】前端事实修正**：`ssh_list_directory`（文件面板无活跃会话时的回落通道）会走 ssh.rs 同一交互式 handler 并对未知主机密钥 emit `ssh-host-key-verify`，而跨窗口路由守卫只认 `sessions` 里 `status==='connecting'` 的会话 → 确认框永不出现、后端空等 60s（用户只看到「SSH connect failed」）。现 sessions store 增「在途一次性连接」登记表（`registerEphemeralConnection` 返回注销函数，调用方 `finally` 注销，不往公共 `sessions` 塞幽灵条目），守卫改名 `ownsConnectPrompt` 并同时认领登记表；sync 的 pull/解决冲突/setup(PulledRemote) 成功后经 workbench bridge 调 `assets.reloadAssets()` 真正重载资产内存态（此前组件调用的 `listAssets` 在 workbench 上**不存在**，每次必抛 TypeError 被吞掉）；`usePanelResize` 的 legacy `centerTopH` 补范围 clamp（同函数兄弟字段早已 clamp）。
- **【v2.6 第二轮续修】**：
  - **写入全部原子化**：新增 `myshelltool_core::write_atomic`（同目录临时文件 + rename，临时名带 pid+序号防并发互踩；**失败保留原文件**），替换掉全部截断式 `fs::write`——`connection-assets.json`（本地唯一副本，半截即整个资产列表不可读）、凭据 `.cred`、`sync-state.json`、`sync.rs` 里三处资产写回、`mcp-config.json`、`mcp-execution-log.json`（后两者原本各有一份重复实现，已收敛到同一实现）。`load_sync_state` 的「文件存在但为空」由「首次运行」改判**损坏**并显式报错，不再静默清零 gist_id/last_synced_at。
  - **凭据红线：命令文本落日志前脱敏**：新增 `myshelltool_core::redact_command`（无正则依赖的单趟词法扫描，覆盖 `mysql -pP@ss`、`-p P@ss` 分离式、`curl -u user:pass`、`--password=x` / `--password x`、`GITHUB_TOKEN=…`；只认小写 `-p`，`-P 3306` 端口不误遮，`-uroot` 用户名不误遮）。接进执行日志 `command` 字段、`ssh_exec`/`exec_on_asset`/`MCP call_tool args=` 三处应用日志与 approval 的放行日志；GUI 审批弹窗仍显示**原命令**（透明性优先）。新增门禁 `no-unredacted-command-in-log`（14 条规则）。
  - **凭据同步失败不再报成功**：`collect_sync_credentials` 改返回三态（items / **failed** / missing）——推送前任一凭据读失败即**中止推送**（宁可不推，也不用残缺备份覆盖完整备份）；`restore_sync_credentials` 返回 `(成功数, 失败项)`，pull 结果新增 `credentials_failed` 字段，UI 提示「N 项凭据未能恢复，这些资产连接会认证失败」。
  - **推送前乐观并发检查**：`sync_push` 先读远端载荷的 `remote_rev` 与本地 `local_rev` 比对，不一致即中止并提示先拉取（此前是无条件 PATCH：两台机器后人静默覆盖前人，双方都显示「已是最新」）；远端载荷无法解析 / Gist 已删同样中止而非盲推。
- **【v2.6 第三轮续修】用户点名的 backlog 三条 + 同源 D-6**：
  - **远端内容不再「猜编码」**：新增 `myshelltool_core::remote_text::decode_remote_text` 三态判定——二进制（前 512 字节 NUL 或 PNG/ELF/ZIP/gzip/xz/OLE2/PDF/SQLite/WASM 魔数）→ 拒绝并引导用 `sftp_download`；非 UTF-8（GBK/GB18030/CP1252 等）→ **拒绝并报首个非法字节偏移**（旧实现 `from_utf8_lossy` 把它换成 U+FFFD 却返回成功，用户/审计拿到「看起来正常实则乱码」的内容）；UTF-8 → 返回文本并标注是否带 BOM（`sftp_read_file` 剥掉 BOM 并留 debug 日志）。
  - **自动重连计数改按「会话稳定」归零**：`useAutoReconnect` 新增 `markConnected()`——连接/重连成功后启动**稳定窗口**（默认 20s），窗口内不再断开才把计数清零；窗口内又断则计数保留、退避继续往后走。此前 `ssh_connect` 一返回 ok 就 `reset()`，认证通过但随即被关的服务器（nologin / shell 立即退出 / 建连后 1-3s 断）会「成功→清零→又断」无限循环，4 次上限形同虚设并对端形成 1s 一次登录风暴。稳定窗口结束后若仍 connected 会再提示一次「连接已稳定」。
  - **跨窗口接管回执不再「6 秒即失败」**：MERGE_ACK 等待从 6s 改为 **20s + 10s 宽限轮询**（`waitMergeAckWithGrace`），超时文案由「移回失败」改为「**尚未确认**（主窗口可能仍在接管），会话暂时保留在本窗口」。此前主窗口最小化时 `adoptSession` 的 rAF 被节流必然超时 → 弹「失败」但主窗口稍后仍接管成功 → 两窗口共持同一会话（之后关 asset 窗口会把主窗口在用的连接断掉）。
  - **拖出前就绪门（D-6）**：新增 `handoff-ready` 事件 + `announceHandoffReady()`（App.vue 两分支在监听注册后各上报一次，setupHandoffListeners 全窗口登记）。`tearOffSession` 在目标窗口**已存在**时先等它上报就绪（≤3s），等不到就放弃本次拖出并提示「稍后重试」，不再迁移。此前 `getByLabel` 查得到窗口 ≠ webview 已注册监听（页面加载+boot 有数百 ms~数秒空窗），此刻拖出会让会话从 UI 消失、后端留孤儿连接且无任何提示。
- **【v2.6 未修 backlog】第二轮「靠猜测/硬编码」审计的余项**（均已实证；前三轮共修掉前四条 + 编码假设/重连计数/ACK 超时/拆出就绪门，见上）：
  1. **[监控口径 · 中] 网络统计的重复计数**：`resource_monitor.rs` 的 `/proc/net/dev` 只排除 `lo` —— 隧道接口（`tun*`/`wg*`）与物理网卡、容器 `veth*`/`docker0`/`br-*` 与宿主网卡字节重叠，网速虚高约 2 倍。修法：以 `/proc/net/route` 的默认路由接口（`00000000` 目标行的 ifname）为准，其余接口仅作明细。
  2. **[上游依赖 · 中] 远端**文件名**的非 UTF-8 无法无损回传**：russh-sftp 用 `from_utf8_lossy` 解码目录条目名（源码 `buf.rs`），客户端拿到的是 U+FFFD 版本，据此 rename/remove/download 会 `No such file`。内容侧已修（`remote_text` 三态判定），文件名侧需上游暴露 raw bytes 才能根治；当前缓解：列出的名字本身就是替换符，用户可辨识。
  3. **[门禁覆盖 · 低] 前端固定延迟等待**：`src/` 的 `await new Promise(r => setTimeout(r, N))` 不在现有门禁内（`no-blocking-sleep` 只覆盖 Rust、`no-fixed-wait-in-tests` 只覆盖 `tests/`）。存量 2 处（`files.ts` 退避间隔、`AssetWindowShell.vue` settle 轮询）都属「探测-等待-再探测」的合法节流，可加规则 + 两处显式豁免，把「等待必须有可观测信号」变成需辩护的例外。
  4. **[展示 · 低] `format_modified` 把「早于 UNIX_EPOCH」与「取 mtime 失败」都折叠成空串**：NTFS 支持 1601 起的任意时间戳（归档解包/安装器会写 1601），显示层与「无 mtime」不可区分（前端已把空串显示为 `—`，故仅显示层问题）。
  5. **[一致性 · 低] `MCP call_tool` 的 `output_summary` 未脱敏**：命令文本已脱敏（见上），但远端 stdout 里若含口令（如 `mysql -e "show create user"` 的明文、`env` 输出）仍会进审计日志。修法：对摘要复用 `redact` 的赋值/`-p` 规则或按行过滤敏感键。
  6. **[边界 · 低] MERGE_ACK 在宽限期之后才到达**：本窗口不自动移交（超时文案已改为「尚未确认」并说明可从主窗口继续使用，不再谎报失败）；彻底解决需把「路径 A 未确认即视为失败」改成可续期的等待，或让主窗口在 adopt 前先回一个 `accepted` 两段式回执。
- **【v2.7】GitHub 登录链路抗抖动（系统代理特性 + 判定分层 + 前端退避重试）**：用户报「偶尔登录失败：轮询 GitHub 授权状态失败: error sending request for url (…/login/oauth/access_token)」。三条并存的根因（都要修，缺一条仍会偶发）：① **`src-tauri/Cargo.toml` 的 `reqwest` 是 `default-features = false`，顺带把默认特性里的 `system-proxy` 关掉了**——`Proxy::system()` 于是只读 `HTTP(S)_PROXY` 环境变量、**完全不读 Windows 系统代理**（`HKCU\...\Internet Settings` 的 `ProxyEnable`/`ProxyServer`/`ProxyOverride`，Clash/v2rayN/SSRUNCore 的「系统代理」模式写的就是这里），而 GUI 从资源管理器启动时没有 `*_PROXY` 环境变量 → 所有 GitHub 请求直连（已用 `cargo tree -f "{p} {f}"` 证实特性集为 `__tls,default-tls,json,native-tls`，`hyper-util/client-proxy-system` 未启用）；② `sync_oauth_poll` **每次轮询新建 `reqwest::Client`**，连接池随之丢弃 → 900s 内最多 ~180 次轮询 = 180 次全新 DNS+TCP+TLS 握手（每次都是独立的失败机会），且只有单个 10s 整体超时（无 connect 超时），跨网络/经代理时一次握手就能吃光预算；③ 传输故障与「授权被拒」都折成 `Err`，前端 `catch` 里直接 `stopTimers(); phase='error'` → **一次瞬时抖动作废整个登录**（设备码在 GitHub 端仍有效 900s，用户却要重新走浏览器授权）。修法：Cargo 显式加 `system-proxy`（随之 `mcp/probe.rs` 的 127.0.0.1 健康检查补 `.no_proxy()`，防 MCP 状态灯被送去代理）；`sync_oauth.rs` 改单例客户端（keep-alive + connect 10s / 整体 20s + tcp_keepalive 30s，`OnceLock<Result<Client,String>>` 缓存），`Err` 只留给内部错误（锁中毒），业务结论走 tag enum：`unstable{user_code, reason}`（传输故障/429/5xx/非 JSON 响应）、`superseded`（session 已被覆盖/取消）、`failed{reason}`（GitHub 明确报错 / token 落库失败）；判定逻辑抽到 `crates/myshelltool-core/src/oauth_flow.rs`（纯函数，`npm run test:core` 真跑——铁律：**传输故障一律可重试、协议拒绝一律终止**，未知 error 码是终止不是重试）；前端 `unstable`/IPC 失败 → 5/10/20/30s 退避重发轮询并显示降级提示，**不设重试次数上限**（权威边界是设备码有效期/倒计时，重试不会重复消费设备码），另修两处竞态：poll 响应加 epoch 快照（旧流程的响应不再改新流程状态、不再排野 timer）+ 响应处理前校验 `phase==='pending'`（倒计时归零后到达的响应当前会把它打回 pending 并继续轮询）。错误文案改带 reqwest 的 `source()` 链（`error sending request for url (...)` 只是最外层笼统描述，真原因——超时/连接被拒/读取中断——在链里）；**2xx 正文绝不回显**（可能含 access_token），非 2xx 正文经 `myshelltool_core::redact_excerpt(text, &[device_code], 200)` 先遮设备码再按字符截断。已知边界：`sync_oauth_start` 与 `sync.rs` 的三处 `reqwest::Client::new()`（**无任何超时**）仍是单次尝试，属同源问题，待单独处理。
- **【v2.7 续修】同步加密层四个真问题（主密码可恢复性 + 重置密码丢凭据）**：用户问「登录成功了同步还要再输之前的密码吗」——**要**（账号层 token 与加密层主密码互不替代，这是端到端加密的前提），但顺着这条链查出四处缺陷，均已修：
  - **① 备份不可恢复（高）**：`encrypt_with_key` 把 `blob.salt` 留空，而会话密钥 = `Argon2id(主密码, salt)`、`salt` 只存本机 SecretStore —— 开过「自动同步」的备份换机/重装/换 Windows 用户后**连主密码都解不开**（任何密码都派生不出那把 key），而 UI 明写「主密码解密即可恢复全部资产」。修法：会话密钥的 salt 随载荷写进 `blob.salt`（salt 本就不敏感，password 路径一向公开随密文存），于是**同一份密文既可由本机会话密钥解、也可由主密码在任意机器重建同一 key 解开**；`encrypt_with_key` 对空 salt 直接 Err（fail-closed，杜绝再产出「只有本机能解」的载荷）。**旧格式已整体移除（用户决定：项目仅一位用户，不做适配）**：`decrypt_vault` 不再有 salt 缺省的回退分支、`LEGACY_PAYLOAD_HINT` 与 `decrypt_vault_verified` 已删除（新格式下「解密成功」本身就证明了旧密码正确），core 的 `crypto::decrypt` / `decrypt_with_key` 与 `parse_vault_plaintext` 统一明确拒绝旧载荷（文案指路「在本机点一次『推送到云端』用本地资产重建备份」——push 不做解密，所以这条出路总是可行）。回归防线：`crypto.rs` 的 `key_based_blob_carries_salt_and_master_password_can_decrypt_it`、`legacy_payload_without_salt_is_rejected_on_both_paths`、`key_based_encryption_rejects_empty_salt` + `sync.rs` 的 `key_based_pack_unpack_vault_roundtrip`（含「错误主密码仍解不开」的认证性断言）、`unpack_vault_rejects_legacy_assets_only_payload`。
  - **② 重置主密码丢凭据（高）**：`sync_reset_master_password` 用 `sync::unpack()` 取明文再 `sync::pack()` 回写，而 `unpack()` 在明文是 `SyncVaultData` 时**只返回 `assets_store`、丢弃 `credentials`** → 重打包覆盖 Gist 后备份里的密码/私钥整段永久消失，UI 还只提示「✓ 主密码已重置」。修法：改走保管库级 `unpack_vault` / `pack_vault`（原样搬运凭据），新增核心回归用例 `vault_roundtrip_preserves_credentials`（含「资产-only 路径必然丢凭据」的反面断言）。
  - **③ 假「旧主密码错误」（中）**：同一函数对旧格式载荷（salt 缺省）恒报「旧主密码错误」（正确密码也判错，用户会误以为记错密码，进而点「清空同步」）→ 新增 `decrypt_vault_verified`：新格式走密码路径；旧格式改为**比对** `Argon2id(旧密码, 本机 salt)` 与本机会话密钥（既验证密码又能取出完整保管库），无会话密钥时给可操作提示。
  - **④ 表单语义混淆（低）**：视图 B 的表单是「**新建**主密码」语义（主密码 + 确认），却出现在「刚登录成功」之后，看起来像「再输一次登录密码」；真正「输入之前的主密码」只发生在填 Gist ID 的换机场景。现按 `isRestoring`（是否填了 Gist ID）分流标题/说明/字段标签，并把「每次 push/pull 需重新输入」与「自动同步可免输」的矛盾文案一并说清。
  - **配套行为（用户选定）**：启用「自动同步」成功后**立即用会话密钥推送一次**（`stores/sync.ts::enableAutoSync`，直接 invoke 而非 `push()`——loading 已置位会被守卫挡掉），把远端升级为主密码可恢复格式；推送失败**不阻断启用**但如实提示，避免「已启用自动同步」掩盖一份解不开的备份。`sync_enable_auto_sync` 对旧格式载荷不再假装验证过，落 warn 说明「首次推送会升级格式」。
- **【v2.7 续修·二】「登录一次就能用」：账号持久化真相 + 免密默认化**：用户反馈「每次打开都感觉要重新登录、推拉还要输密码」。实测取证（`%APPDATA%\com.redtei.myshelltool`）：`credentials/github-pat.cred` **一直存在**（今天才写过）→ **token 持久化本来就是好的**，问题有两处：① **UI 假象**：`PatConfigCard` 只按本次会话的 `phase` 显示，重启后 phase 回到 `idle` 就摆出主按钮「登录 GitHub」，用户以为掉登录了 —— 现以**本地安全存储里的 token 为权威**（`loggedIn = phase==='success' || (phase==='idle' && githubPatConfigured)`），已登录时只留次级「重新登录」并明说「重启无需再登录」；② **真摩擦**：该机没有 `sync-session-key.cred`（`sync-state.json` 里也没有 `auto_sync_enabled`），所以每次 push/pull 都要主密码 —— 现 `sync_setup` 成功后**自动** `remember_session_key`（按**载荷自己的 salt** 重建 key → `store_session_key` → `auto_sync_enabled = true`），主密码一台机器只输这一次（或换机恢复时），此后推拉与自动推送全免密；存盘失败**不阻断 setup**、不置 flag（前端退回输密码模式并可手动重试，不静默糊弄）。关键不变量：**会话密钥的 salt 必须与载荷的 salt 一致**，否则免密路径与主密码路径得到两把 key、免密拉取会莫名失败 —— 由 `core::sync::session_key_for_payload` + 单测 `session_key_for_payload_rebuilds_the_encrypting_key` 固化。UI 文案统一为「本机免密 & 自动同步」（`SyncAutoSyncControl`）、「立即同步」密码框在免密时提示「留空即可（已免密）」。**已知边界**：主密码仍是唯一的跨机恢复凭据（不落盘的代价），DPAPI 失效/换 Windows 用户时自动退回「输主密码」模式。
- **【v2.7 续修·三】「改了代码但界面没变」的时序坑 + 灰按钮死胡同**：用户报「UI 似乎还没更改，我已登录但还是不能点同步」。**诊断手法（可复用）**：① 查正在跑的进程与产物 —— `Get-Process myshelltool` 拿到 exe 路径与启动时刻，比对 `target\debug\myshelltool.exe` 的构建时刻与源文件 mtime；② **直接在二进制里搜字符串**判版本（`Select-String -Path <exe> -Pattern '新函数名/新文案' -Encoding utf8 -List`）：本次实测 exe 里**有**新 Rust 字符串（`session_key_for_payload`、`已记住本机会话密钥`）却**没有**新前端文案，反而还留着旧文案「启用自动同步」；③ （**该步曾误判，纠正如下**）请求 dev server 的模块 URL 时**必须用 root 相对路径** —— `vite.config.ts` 的 `root=src`，所以正确 URL 是 `http://127.0.0.1:41234/components/shell/Xxx.vue`；写 `/src/components/...` 会命中 **SPA 回退页**（HTTP 200 + index.html），把它当"dev server 在供旧模块"是错的（本次就是这么误判的）。**结论（修正后）**：真正站得住的证据是②——那个 debug 产物里是**新 Rust + 旧前端包**，说明它编译时 `dist` 已过期/编译先于 UI 改动。**项目级教训**：`tauri.conf.json` 的 `frontendDist=../dist` 会在 **Rust 构建阶段（build script）**被打进二进制，所以**改完前端必须先 `npm run build`，再编译 Rust**；调试期优先用 `npm run tauri:dev`（前端走 devUrl，改完刷新窗口即可）。**同时修掉 UX 缺陷**：`SyncPanelContent` 的推送/拉取按钮原先在「密码框为空且未启用免密」时**置灰**——灰按钮不给任何解释，用户只会得到「点不动」的死胡同；现改为**始终可点**（仅 `syncLoading` 时禁用），点击后由 `onPush`/`onPull` 明确提示「请先输入主密码（只需这一次：成功后本机记住密钥，之后免密并自动推送）」。安全档位经用户确认：**保持 DPAPI 持久免密**（已说明"同 Windows 账号下的程序可解密备份"这一攻击面扩大，并保留一键「关闭（取消本机免密）」）。
- **【v2.7 续修·四】同步页重设计（用户："还是很难用"）**：按 `frontend-design` 流程做的（定方向 → token 计划 → 自我批判 → 实现 → 截图复核）。**诊断**（旧版为什么难用）：① 主密码框占据日常视图 C 位，每次同步前都要先判断"要不要输"；② **没有任何"该推还是该拉"的信号**——`has_local_changes_since_last_sync` 只服务于后端 pull 判定，前端看不到，两个按钮等重，用户得回忆上次做了什么；③ 免密/凭据/账号/重置/换机/清空 = 5-6 张并列卡片，把主任务淹在配置里；④ 说明当正文写（推送=…/拉取=…/想免密？…）撑高面板。**改法**：
  - **签名元素 = 方向性管线** `SyncStatusRail`：`本机 N 台 ─▶ 加密(含密码/仅资产) ─▶ Gist …尾号`，三段取代原来无状态的 flow-line；**需要动的那一段亮起**（本机有改动 / 云端有新版本），状态词 + 上次同步为 mono 读数。节点不可点（不做 tab 陷阱）。
  - **读数与操作合成一块面板**：`SyncPanelContent` 提供唯一外框 `.sync-surface`，内部 `SyncStatusRail`（灰底读数）+ `SyncActionBar`（白底操作）用 hairline 分隔；两个子组件不再各自带卡片边框（此前是两张无关的卡）。
  - **`SyncActionBar`**：主密码行**仅在未免密时出现**（且说明"输一次即可"）；按钮主次由状态决定（本机有改动→推送实心；云端有新版本→拉取实心；两边都有→拉取实心；已同步→两个都安静，不制造假紧迫）；**不因"没输密码"置灰**（灰按钮=无解释的死胡同）。
  - **`SyncAdvancedSettings`**：一个折叠入口装下免密 / 密码与密钥 / GitHub 账号与主密码 / 换机恢复 / 清空（按改动风险排序，清空永远最后且二次确认）；`SyncAutoSyncControl` 与 `SyncSecurityOptionsCard` 由"卡片"改写为**设置行**（说人话：这台电脑免密 / 密码与密钥一并备份），`PatConfigCard` 新增 `flat` prop 消除折叠区里的卡片套卡片。
  - **`SyncSetupForm` 一次只问一件事**：默认只问主密码（+确认）→「创建备份」；点「从已有备份恢复」才出现 Gist ID 并改问"原来那台电脑上的主密码"，**恢复模式不显示确认字段**（解密成功本身就是校验）。
  - **后端新增 `sync_status.local_has_changes`**（复用 `has_local_changes_since_last_sync`；未配置时恒 false，避免"待推送"歧义）；`sync.ts` 新增 `activeOp: 'push'|'pull'|null`，让 rail 的方向动效与按钮进行中文案（推送中…/拉取中…）有真实依据，而不是含糊的"同步中"。
  - **设计约束（可复用）**：只用既有 token；状态色**只由左边线 + 圆点承担**（同一个状态不上三遍色，且彩色小字对比度更差）；唯一动效 = rail 箭头行进（`prefers-reduced-motion` 下关闭）；`Gist` **不做 `text-transform: uppercase`**（全大写读成缩写 GIST）；窄列（设置弹窗 max-width 580px）优先，不做宽屏 dashboard。
  - **复核方式**：临时 Playwright harness（mock `window.__TAURI__`）渲染 5 种状态截图逐一批判后迭代（含一个真 bug：rail 在 surface 内外各渲染一次）；用完即删，不留仓库。已知瑕疵：2× 截图里中文段落首字看似被切，DOM 实测内缩正常（13px = padding+border），属渲染瑕疵。
  - **架构备注**：`SyncStatusRail` / `SyncActionBar` 直接 `useSyncStore()`（先例：resourceMonitor panel、useGithubDeviceLogin），避免继续往已超硬上限（517 行）的 workbench 编排壳里加 re-export。新增组件 `SyncStatusRail.vue` / `SyncActionBar.vue` / `SyncAdvancedSettings.vue`。
- **【v2.7 续修·五】零输入建备份：应用生成恢复密码（用户问"新用户能不能只登录一下"）**：用户提出目标「方便用户使用的同时还安全」，并质疑上一轮的"提示存进密码管理器"——**那等于把能否恢复押在用户装没装某个外部软件上**。改法：`core::recovery_code::generate_recovery_password`（24 位、去 `0/O/1/l/I` 歧义字符，供人眼抄写）+ 两个命令 `sync_generate_recovery_password`（生成→DPAPI 存盘→返回明文填入表单并明示一次）/`sync_reveal_recovery_password`（「换机恢复」里查看/复制）；`sync_status.recovery_password_saved` 决定入口显隐。新用户路径因此是：**登录 → （可选）点一下生成强密码 → 创建备份 → 之后全自动免密**。三条工程纪律：① 只保存应用生成的密码，用户手输的不保存（§8 边界）；② `sync_setup`/`sync_reset_master_password` 后发现保存值与本次主密码不同即**删除**（防"查看"给出打不开备份的错值）；③ 明文只回本机 webview、不进 store/localStorage/日志（前端 `reveal` 也是点一次读一次）。**同时按用户要求删掉界面上"旧版备份不再支持"的迁移提示**（唯一用户，全力维护新版；后端那条明确拒绝仍保留 —— 那是 fail-closed 报错不是迁移文案）。设计复核仍走截图：新增的生成结果块与恢复密码明文块各截一张，修掉"按钮被 stretch 居中""密码块缺一句它是什么"。
- **【v2.5】前端事实驱动改造**：连接后文件面板自动加载改 1s/2s/5s 退避重试（重试前校验该资产仍有活跃会话，会话断开可取消退避链，失败静默展示空态+重试）；File 直传以短块（< CHUNK_SIZE / 0 字节）为真 EOF + 上传后字节对账（仅 warn 差异），不信任列表时刻的 file.size；`remotePath` 未加载（空串）时禁止上传/建目录等远程写操作（防折叠到根路径）；AssetWindowShell 删主窗存活探测的 3 次重试门（改挂载 + 焦点/可见事件各探测一次，成败以点击时刻真实探测为准）、关窗等 connecting 会话 settle 上限 65s（后端 60s 超时 + 5s 缓冲）且超时**不销毁窗口**（防 destroy 把孤儿 SSH 会话留在后端）；`assetWindows` label sanitize 发生字符替换时追加 id 的 djb2 短哈希后缀防不同 id 碰撞；终端字号/行高读取路径 clamp（字号 9-28、行高 1-2，手改 localStorage 坏值不再越界渲染）。

---

## 10. Git / 提交

- 默认分支 `master`。提交前确认在正确分支；用户没要求时**不要自动 commit/push**。
- 改动尽量聚焦，提交信息描述清楚"做了什么 + 为什么"。

---

## 11. 给 Agent 的快速决策清单

收到任务时按此顺序判断：
1. **是读/研究类**？→ 用 Explore 子 agent 并行搜索，给出结论而非文件堆。
2. **涉及多文件/架构决策**？→ 先进 Plan 模式，产出计划并经确认。
3. **有现成实现可复用**？→ 复用（查 `ui/index.ts`、`workbench.ts` re-export、`backend.ts` normalize*）。
4. **要改 Rust 命令**？→ 别忘了在 `generate_handler!` 注册。
5. **要加跨 store 逻辑**？→ 加子 store action + `workbench.ts` re-export，用 lazy bridge。
6. **涉及外部环境**（远端路径/命令输出解析/locale/时钟/身份键/就绪信号）？→ 先按指南 §7 逐项想清求证方式：能协议求证（SFTP canonicalize/stat/read_dir、pty 模式）就不猜；确需 exec 解析必须 `LC_ALL=C` + POSIX 选项 + 注释平台假设。新硬编码写进代码前自问「换个合理环境还成立吗」。
7. **改完**？→ 跑 §5 的验证（build 首步即含 lint:facts 门禁），如实报告 exit code。
