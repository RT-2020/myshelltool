---
title: myshelltool UI 全栈重构规格 — Tabby 视觉 × FinalShell 布局
interview_id: ui-refactor-abandon-design-2026-06-14
project_type: brownfield
threshold: 0.2
threshold_percent: 20%
threshold_source: default
final_ambiguity: 0.128
status: approved — pending execution approval
created_at: 2026-06-14
rounds: 9
---

# myshelltool UI 全栈重构规格

## Metadata

- **访谈 ID**: ui-refactor-abandon-design-2026-06-14
- **轮次**: 9
- **最终歧义**: 12.8%
- **项目类型**: brownfield（在现有 myshelltool 代码库上重构）
- **生成时间**: 2026-06-14
- **阈值**: 20% (source: default)
- **Initial Context Summarized**: yes（代码事实摘要已浓缩）
- **Status**: PASSED

## Clarity Breakdown

| Dimension | Score | Weight | Weighted |
|-----------|-------|--------|----------|
| Goal Clarity | 0.92 | 0.35 | 0.322 |
| Constraint Clarity | 0.80 | 0.25 | 0.200 |
| Success Criteria | 0.92 | 0.25 | 0.230 |
| Context Clarity | 0.80 | 0.15 | 0.120 |
| **Total Clarity** | | | **0.872** |
| **Ambiguity** | | | **0.128 (12.8%)** |

## Topology

7 个 active 顶层组件，无 deferral。

| Component | Status | Description | Coverage |
|-----------|--------|-------------|----------|
| `app-shell` | active | 应用外壳：5 区域布局骨架、titlebar、statusbar、tabbar、导航范式 | 仿 FinalShell 完整 5 区域布局（左/中上/中下/右/底），所有区域必须同时可见 |
| `connection-sidebar` | active | 连接资产侧栏：主机树、分组、过滤、快速连接 | 左侧栏 10-15% 宽，承接现有 ConnectionAsset 数据 |
| `terminal-surface` | active | 终端面板：工具栏、xterm、内嵌搜索、tab、状态点 | 中上区域，承接现有 sessions/activeSession/connectSelected |
| `file-surface` | active | 文件面板：双栏、面包屑、过滤、传输抽屉、右键菜单 | 中下区域，承接现有 remoteEntries/localEntries/transferQueue |
| `auxiliary-surfaces` | active | overview/tunnels/context panel/资源监控面板的整合 | 右侧栏：服务器资源监控（实时图表）+ 运维摘要卡片 |
| `design-system` | active | 视觉系统：颜色/字体/间距 token、组件视觉语言 | Tabby/Termius 调性（干净、低视觉重量、现代），手写组件 |
| `code-architecture` | active | Pinia store 拆分、组件目录、CSS 架构 | 按 domain 拆 store（sessions/files/tunnels/ui）、组件按 surface 分目录、SCSS + token 系统 |

## Goal

将 myshelltool 现有 UI **整体推倒重写**——抛弃 `.omc/` 下既有 spec 文档规定的"FinalShell 卡片堆叠"视觉决策，但保留 **FinalShell 软件本身的"多面板信息密集"布局范式**，搭配 **Tabby/Termius 的现代化干净视觉语言**，最终交付一个"高信息密度但视觉不臃肿"的 SSH 客户端界面，**视觉调性、信息架构、交互范式、代码架构四个层面同时重写**。

具体落地为 **5 区域同时可见布局**：

```
┌────────────────────────────────────────────────────────────────────┐
│ TitleBar（全局搜索 Ctrl+K / 主题切换 / 同步 / 警告）                  │
├──────────┬──────────────────────────────────┬─────────────────────┤
│          │  中上：终端面板                    │                     │
│          │  （多 tab + 工具栏 + 内嵌搜索）     │  右：服务器资源监控   │
│ 左：连接  │                                  │  （CPU/内存/网络/磁盘  │
│ 资产侧栏  ├──────────────────────────────────┤   实时图表）          │
│          │  中下：文件管理                    │                     │
│          │  （双栏：本地 / 远程）              │  + 运维摘要卡片       │
│          │                                  │  （主机/凭据/隧道/同步）│
├──────────┴──────────────────────────────────┴─────────────────────┤
│ StatusBar（SSH 状态 / backend / 传输 / 警告） + 传输队列抽屉           │
└────────────────────────────────────────────────────────────────────┘
```

## Constraints

- **必须保留现有 Tauri 后端命令签名**：`ssh_connect`/`ssh_write`/`ssh_resize`/`ssh_disconnect`/`ssh_confirm_host_key`/`ssh_keyboard_response`/`ssh_list_directory`/`sftp_*`/`fs_local_*`/`tunnel_*`/`save_connection_asset`/`list_connection_assets`/`save_credential`/`delete_credential`/`get_credential_status`/`backend_status` 全部不变
- **新增 Tauri 命令**：在 `src-tauri/src/` 下新增 `resource_monitor.rs`（或同名模块），通过 SSH 通道执行 `top`/`vmstat`/`free`/`iostat` 或读 `/proc/stat`/`/proc/meminfo`/`/proc/net/dev`/`/proc/diskstats`，返回结构化的 CPU/内存/网络/磁盘实时数据；在 `lib.rs` 注册到 `invoke_handler`
- **前端依赖白名单**：
  - 允许：Vue 3 / Pinia / lucide-vue-next / xterm 6 / SCSS / 现有 composables/lib
  - **禁止**：Tailwind CSS / UnoCSS / 任何 Vue UI 组件库（Naive UI / Element Plus / Ant Design Vue / shadcn-vue 全禁）
  - 所有组件**完全手写**（含 Modal/Drawer/Tooltip/Select/Table 等基础件），用 SCSS + token 系统控制视觉
- **保留浏览器预览模式兼容**：`!isTauriRuntime()` 分支不能破坏，`npm run dev` 必须能渲染所有 5 区域（资源监控显示"需要桌面端"提示）
- **后端秘密边界不可破**：密码、私钥、passphrase、token、PAT 仍只走 SecretStore，不出现在前端代码 / Git / 日志
- **OSC 0/1/2 标题解析保留**：终端会话 tab 标签必须继续响应远程 shell 的标题序列
- **现有快捷键体系保留**：Ctrl+K 全局搜索、Ctrl+Shift+F 终端搜索、Ctrl+Shift+C/V 复制粘贴、Ctrl+Tab 会话切换、Ctrl+=/-/0 字体缩放、F2 重命名、F5 刷新、Delete 删除等

## Non-Goals

- **不重写 Rust 后端的 SSH/SFTP/tunnel 核心**：仅新增 resource_monitor 模块；其他后端代码（`ssh.rs`/`lib.rs` 主流程）不改
- **不做跨平台**：仍 Windows-only（用户当前唯一使用平台）
- **不引入任何外部 UI 组件库或原子化 CSS 框架**（已确认）
- **不做拖拽上传/下载、目录递归传输、书签快速跳转、同步浏览**（延续 autopilot-file-manager.md 的 P1+ 推迟项）
- **不做 SSH agent/Pageant、OpenSSH config、SSH 证书、FIDO2 支持**（延续 rust-finalshell spec 的延期项）
- **不做主题编辑器/自定义皮肤**：仅保留 system/light/dark 三态
- **本次不做新功能开发**：资源监控之外，不引入 FinalShell 没有的新功能（如 AI 补全、命令历史云同步等）

## Acceptance Criteria

### A. 视觉与布局（design-system + app-shell）

- [ ] **AC1** 5 区域布局**同时可见**：左侧栏（资产）/ 中上（终端）/ 中下（文件）/ 右侧栏（资源监控+摘要）/ 底部状态栏
- [ ] **AC2** 整体视觉调性 ≈ Tabby/Termius：低视觉重量、克制配色、干净边框、合理留白；不出现 FinalShell 式的"卡片堆叠+多色块+边框嵌套"
- [ ] **AC3** 所有基础组件（Button/Input/Modal/Drawer/Tooltip/Select/Table/ContextMenu/Tab/StatusBar/Breadcrumb/Progress）全部手写，存放在 `src/components/ui/`（或同等命名的目录）
- [ ] **AC4** SCSS token 系统（`--accent`/`--space-*`/`--radius-*`/`--text-*`/`--app-bg`/`--app-border`/`--app-muted` 等）整理到独立的 `_tokens.scss`，所有组件用变量，禁硬编码
- [ ] **AC5** 浅色/深色/跟随系统三态主题切换无视觉断裂

### B. 信息架构与功能（5 区域）

- [ ] **AC6** 左侧栏：连接资产树（分组+过滤+快速连接 ssh user@host），承接现有 `assets`/`groupedAssets`/`connectSelected`
- [ ] **AC7** 中上：终端 tab 多会话，每个 tab 显示 `host`+状态点+OSC 标题；工具栏图标按钮（搜索/复制/粘贴/清屏/字体+/-/重连/全屏/侧栏开关）；内嵌搜索条（Ctrl+Shift+F）
- [ ] **AC8** 中下：文件管理双栏（本地+远程），含面包屑、过滤、列头排序、Ctrl 多选/Shift 范围选、右键菜单（含批量操作）、F2 重命名、Delete 删除、F5 刷新、Ctrl+A 全选、Esc 清选
- [ ] **AC9** 右侧栏上半：**服务器资源监控实时图表**——CPU（占用率+核心数）、内存（已用/总量）、网络（上传/下载速率）、磁盘（读写速率/已用空间），数据来自新后端 `resource_monitor_*` 命令；未连接时显示空状态
- [ ] **AC10** 右侧栏下半：运维摘要卡片（主机摘要、标签、最近连接、凭据状态、隧道健康、Git 同步状态），承接现有 context panel 内容
- [ ] **AC11** 底部状态栏：SSH 连接状态、backend 模式、传输进度摘要、警告数；点击展开传输队列抽屉（含进度条/速度/历史）

### C. 代码架构（code-architecture）

- [ ] **AC12** Pinia store 按 domain 拆分：至少 `useSessionsStore`（终端会话）、`useFilesStore`（文件管理）、`useTunnelsStore`（隧道）、`useAssetsStore`（连接资产）、`useUiStore`（主题/tab/抽屉开关）、`useResourceMonitorStore`（资源监控数据流）；不再用单文件 1572 行的 `workbench.js`
- [ ] **AC13** 组件目录按 surface 分组：`src/components/shell/`（titlebar/statusbar/sidebar）、`src/components/terminal/`（已有 8 个+新增）、`src/components/files/`（双栏/面包屑/传输抽屉）、`src/components/resource-monitor/`（图表/指标卡）、`src/components/ui/`（基础件）；App.vue 退化成"布局容器 + 路由 tab"，不超过 200 行
- [ ] **AC14** SCSS 架构：`src/styles/_tokens.scss`（变量）、`src/styles/_base.scss`（reset+全局）、`src/styles/_utilities.scss`（工具类）、`src/styles/main.scss`（入口）；每个组件 `<style scoped lang="scss">`

### D. 后端（resource_monitor）

- [ ] **AC15** `src-tauri/src/resource_monitor.rs` 新增至少 4 个 Tauri 命令：`resource_monitor_start(sessionId, intervalMs)`、`resource_monitor_stop(sessionId)`、`resource_monitor_snapshot(sessionId)`、`resource_monitor_list_active()`
- [ ] **AC16** 数据通过 Tauri event 持续推送到前端（类似现有 `sftp-transfer-progress` 模式），前端用 `listenBackendEvent` 订阅
- [ ] **AC17** CPU 数据通过解析 `/proc/stat` 或 `top -b -n 2` 得到；内存通过 `/proc/meminfo` 或 `free -b`；网络通过 `/proc/net/dev`；磁盘通过 `/proc/diskstats` 或 `iostat`。**仅在 Linux 远程主机上工作**（用户实际运维目标平台）
- [ ] **AC18** 资源监控随会话生命周期启停：会话断开时自动停止监控事件流，不泄漏后端 task

### E. 工程验收

- [ ] **AC19** `npm run build` 通过
- [ ] **AC20** `npm run test:ui` 通过（如有 ui-smoke.mjs 测试需要适配新 DOM 结构，测试逻辑可改但核心断言保留）
- [ ] **AC21** `cargo check --manifest-path src-tauri/Cargo.toml` 通过
- [ ] **AC22** 浏览器预览模式（`!isTauriRuntime()`）下 5 区域全部渲染（资源监控显示"需要桌面端"占位），不崩
- [ ] **AC23** `npm run tauri:dev` 启动后 5 区域全部可用，能连接真实 SSH 主机并看到资源监控实时数据

## Assumptions Exposed & Resolved

| Assumption | Challenge | Resolution |
|------------|-----------|------------|
| "不按照设计" = 抛弃所有 FinalShell 痕迹 | Round 7 揭示：用户实际想保留 FinalShell 的**布局**，抛弃的是 `.omc/` 里 spec 文档规定的视觉决策 | 视觉 = Tabby/Termius；布局 = FinalShell 5 区域；两者不冲突 |
| Round 2 全选 4 类信息 vs Round 6 减法 3 件套 | Round 7+8 让用户在 Simplifier/Ontologist 双重压力下表态 | 4 类信息全要 + 5 区域同时可见；Round 6 的减法被推翻 |
| "全部 UI 重构"包括后端 | Round 3 显式询问 | 后端**可以**新增 Tauri 命令（资源监控需要）；现有命令签名不变 |
| 是否需要 Tailwind/UI 库加速开发 | Round 4 Contrarian 挑战 + Round 5 直接选库 | 用户主动**禁止** Tailwind/UnoCSS，且选择**完全手写组件**——牺牲开发速度换 100% 视觉可控 |
| 资源监控可以"先占位后续接" | Round 9 显式询问 | 用户选择"全部要可用，含资源监控"——本次必须做完整闭环，资源监控不进 v2 |

## Technical Context

### 现有可复用资产

- **后端命令签名**：全部保留（`ssh_*` / `sftp_*` / `fs_local_*` / `tunnel_*` / credential / asset / backend_status）
- **前端 composables**：`useTerminalConfig` / `useClipboard` / `useAutoReconnect` / `useTerminalShortcuts` 全部保留
- **前端 lib**：`terminalThemes` / `dangerousCommands` / `terminalController` 全部保留
- **8 个 terminal 组件**：TerminalTabs/TerminalToolbar/TerminalSearchBar/TerminalPane/ConnectionStatusPill/ShortcutCheatsheet/CommandPalette/DangerousPasteConfirm——可保留并按新视觉调整
- **CSS token 系统**：`--accent` / `--space-*` / `--radius-*` / `--text-*` / `--app-bg` / `--app-border` / `--app-muted` 整理到 `_tokens.scss`

### 必须重写的部分

- `src/App.vue`（1576 行 → <200 行布局容器）
- `src/stores/workbench.js`（1572 行 → 拆成 6 个 domain store）
- `src/styles.css`（单文件 → SCSS 架构 4 文件）
- 现有 4 tab 布局（overview/terminal/files/tunnels 独立 tab → 5 区域同时可见）
- 右侧 context panel（独立 panel → 整合到右侧栏下半 + statusbar）

### 必须新增的部分

- `src-tauri/src/resource_monitor.rs`（新后端模块）
- `src/components/ui/`（基础组件库）
- `src/components/shell/`、`src/components/files/`、`src/components/resource-monitor/`（按 surface 分组）
- `src/stores/{sessions,files,tunnels,assets,ui,resourceMonitor}.js`（6 个 domain store）
- `src/styles/{_tokens,_base,_utilities,main}.scss`

## Ontology (Key Entities)

| Entity | Type | Fields | Relationships |
|--------|------|--------|---------------|
| 5-区域布局 | core domain | 左侧栏/中上/中下/右侧栏/底部 | 包含所有 surface |
| 终端面板 | core domain | tab/工具栏/xterm/内嵌搜索 | 承载 Session |
| 文件面板 | core domain | 双栏/面包屑/过滤/右键菜单/传输抽屉 | 操作 RemoteEntry/LocalEntry |
| 服务器资源监控 | core domain | CPU/内存/网络/磁盘 + 实时数据流 | 依赖 Session |
| 连接资产 | core domain | name/host/port/username/auth/credential | 启动 Session |
| Session | core domain | sessionId/term/asset/status/oscTitle | 资源监控的载体 |
| RemoteEntry / LocalEntry | supporting | name/path/kind/size/modified | 文件面板操作目标 |
| TransferQueue | supporting | id/direction/percent/status | 文件面板底部抽屉 |
| 隧道 | supporting | id/kind/ports/active | 仍存在，但 tab 改为右侧栏摘要 |
| 设计 token | supporting | --accent/--space-*/--radius-*/--text-* | 所有组件消费 |

## Ontology Convergence

| Round | Entity Count | New | Changed | Stable | Stability Ratio |
|-------|-------------|-----|---------|--------|----------------|
| 1 | 3 | 3 | - | - | N/A (初始) |
| 2 | 6 | 4 | 0 | 2 | 33% |
| 3 | 8 | 2 | 0 | 6 | 75% |
| 4 | 9 | 1 | 0 | 8 | 89% |
| 5 | 10 | 1 | 0 | 9 | 90% |
| 6 | 11 | 1 | 0 | 10 | 91% |
| 7 | 12 | 1 | 0 | 11 | 92% |
| 8 | 13 | 1 | 0 | 12 | 92% |
| 9 | 13 | 0 | 0 | 13 | 100% |

**收敛信号**: Round 9 实现 100% 稳定，无新增/重命名——域模型完全锁定。

## Interview Transcript

<details>
<summary>Full Q&A (9 rounds + Round 0 topology)</summary>

### Round 0 (Topology)
**Q**: "你说的「全部 UI 重新重构、不按照设计」是哪一层？"（4 选项：只换皮 / 视觉+IA / 范式级推倒 / 全部连代码架构也重写）
**A**: 全部，连代码架构也重写
**Topology locked**: 7 active components, 0 deferrals

### Round 1 (Goal)
**Q**: 新 UI 的「正向锚点」是什么？(Tabby·Termius / Warp·Wave / VS Code·JetBrains / macOS·Linear·Raycast)
**A**: Tabby / Termius，布局的功能参考 finalshell，需要显示一些参考的信息等
**Ambiguity**: 100% → 73.5%

### Round 2 (Goal)
**Q**: FinalShell 式「参考信息」里你实际想保留 / 新增哪些？(服务器资源监控/会话状态/文件传输/运维摘要)
**A**: 全选 4 项
**Ambiguity**: 73.5% → 68.3%

### Round 3 (Constraints)
**Q**: 服务器资源监控的数据从哪里来？(改后端新增 Tauri 命令/只做本地资源/纯前端待后续/不要了)
**A**: 可以改后端，新增 Tauri 命令
**Ambiguity**: 68.3% → 61.3%

### Round 4 (Constraints, Contrarian Mode)
**Q**: 重构后的前端用什么写样式和组件库？(零新依赖手写/Tailwind/UI 库/Tailwind+Headless)
**A**: 可以使用 SCSS / UI 组件库 / **禁止使用 Tailwind / UnoCSS**
**Ambiguity**: 61.3% → 56.3%

### Round 5 (Constraints)
**Q**: 选哪个 UI 组件库？(Naive UI / Element Plus / Ant Design Vue / 完全手写)
**A**: 完全手写组件
**Ambiguity**: 56.3% → 50.0%

### Round 6 (Goal+Success, Simplifier Mode)
**Q**: 如果只能保留 3 项，你实际天天会用的「核心功能集」是什么？(只保留终端+文件+侧栏 / 4 项都要同时可见 / 终端为主其他抽屉)
**A**: 只保留 终端+文件+侧栏
**Ambiguity**: 50.0% → 35.3%（但与 Round 2 形成矛盾，Round 7 解决）

### Round 7 (Goal)
**Q**: Round 2 的 4 项 vs Round 6 的 3 件套——哪个是真需求？
**A**: 我想就仿照 finalshell 的布局这样
**Ambiguity**: 35.3% → 31.0%

### Round 8 (Goal, Ontologist Mode)
**Q**: FinalShell 的哪几块区域是你锚点里「必须同时可见」的？(完整 5 区域 / 终端+右边栏+文件 tab / 终端为主其他抽屉)
**A**: 完整 5 区域（左右中上下）
**Ambiguity**: 31.0% → 22.2%

### Round 9 (Success)
**Q**: 「做好了」的具体可测试锚点是什么？(全部要可用含资源监控 / 5 区域都渲染资源待接 / 仅布局原型)
**A**: 全部要可用，含资源监控
**Ambiguity**: 22.2% → 12.8% ✅

</details>
