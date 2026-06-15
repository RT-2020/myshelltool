---
plan_id: ui-full-refactor-consensus
spec_ref: .omc/specs/deep-interview-ui-full-refactor.md
mode: ralplan --consensus --direct --short
created_at: 2026-06-14
status: consensus-approved-pending-execution
iteration: 1
planner_pass: yes
architect_pass: yes
critic_verdict: APPROVE_WITH_IMPROVEMENTS
critic_pass: yes (10 improvements applied)
---

# myshelltool UI 全栈重构实施计划

## 1. Requirements Summary

将 myshelltool 现有 UI 整体重写为 **FinalShell 5 区域同时可见布局**（左资产 / 中上终端 / 中下文件 / 右资源监控+摘要 / 底部状态栏），搭配 **Tabby/Termius 视觉调性**（低视觉重量、克制配色、合理留白）。所有基础组件（Button/Input/Modal/Drawer/Tooltip/Select/Table/ContextMenu/Tab/StatusBar/Breadcrumb/Progress）100% 手写，禁 Tailwind/UnoCSS/UI 库；SCSS + token 系统统一视觉。Pinia store 按 domain 拆 6 个（sessions/files/tunnels/assets/ui/resourceMonitor），App.vue 退化为 <200 行布局容器；`src-tauri/src/resource_monitor.rs` 新增 4 个 Tauri 命令通过 SSH 通道读 `/proc/*` 推送 CPU/内存/网络/磁盘实时数据。23 条 AC 必须全部满足；浏览器预览模式回归不可破坏。

---

## 2. RALPLAN-DR Summary (short mode)

### Principles (5)

1. **视觉调性 = Tabby/Termius**：低视觉重量、克制配色、合理留白；不出现 FinalShell 式卡片堆叠/多色块/边框嵌套
2. **信息密度 = FinalShell 5 区域同时可见**：左侧栏 / 中上终端 / 中下文件 / 右侧资源监控+摘要 / 底部状态栏 一屏全在
3. **完全手写组件（禁 Tailwind/UnoCSS/UI 库）**：100% 视觉可控，牺牲开发速度换可控性
4. **不破坏现有 Tauri 命令签名**：`ssh_*`/`sftp_*`/`fs_local_*`/`tunnel_*`/credential/asset/backend_status 全部保留，仅新增 `resource_monitor_*`
5. **浏览器预览模式必须仍能渲染**：`!isTauriRuntime()` 分支不被破坏，`npm run dev` 必须渲染所有 5 区域（资源监控显示占位）

### Decision Drivers (top 3)

1. **用户的视觉偏好**（spec Round 4-5：主动禁止 Tailwind/UI 库 + 强制手写组件）→ 决定 SCSS token 架构 + 基础组件目录
2. **后端资源监控数据可用性**（spec Round 3 + AC9/AC15-18）→ 决定必须改 Rust 新增 `resource_monitor.rs`
3. **4-tab 独立切换 → 5 区域同时可见的范式转变**（spec Round 7-8 + AC1）→ 决定 App.vue 必须从 1576 行退化为 <200 行布局容器

### Viable Options

#### Option A: Wave-based incremental delivery（5 waves，每波独立可 commit）

- **Wave 1**：SCSS 架构 + design tokens + 基础 UI 组件（Button/Input/Modal/Drawer/Tooltip）
- **Wave 2**：Pinia store 拆分（6 domain stores）+ App.vue 骨架化
- **Wave 3**：5 区域布局 shell + 组件目录重组
- **Wave 4**：Resource monitor 后端（resource_monitor.rs）+ 前端 store + 图表组件
- **Wave 5**：视觉抛光 + AC 验收 + 浏览器预览模式回归测试

**Pros**（bounded）：
- 每波独立可 `npm run build` + `npm run test:ui` 验证
- 中途暂停时前 N 波可用（Wave 3 落地后 5 区域布局即可见）
- 回归定位粒度小（出问题二分到具体波次）
- 与现有 8 个 terminal 组件 + 现有 store 状态可平滑迁移

**Cons**（bounded）：
- 总工作量略多于 Big-bang（多写中间过渡状态）
- Wave 2-3 之间存在临时状态（store 已拆但布局未完）
- 跨波次需要维护向后兼容（旧变量名 alias）

#### Option B: Big-bang rewrite on a feature branch（单一大 commit/PR）

一次性重写所有 7 个 topology 组件，最后切回主分支。

**Pros**（bounded）：
- 视觉一致性最容易保证（一次性设计无中间妥协）
- 无中间过渡状态成本
- 一次 PR review 看全貌

**Cons**（bounded）：
- 回归风险高：单次提交太大，bug 难定位
- 无法增量验证：`npm run build` 要等所有部分都完成
- 与现有 8 个 terminal 组件 + workbench.js 状态需一次性迁移
- 长期 feature branch 与 main 分叉严重，merge 冲突大

#### Option C（已被 spec 否决）: 仅视觉换皮

spec Round 0 已锁定 topology = "全部，连代码架构也重写"，此选项直接排除。

### Adopted Option

**Option A — Wave-based incremental delivery**

**理由**：spec 强制 23 条 AC（每条都要可验收）+ 浏览器预览模式回归风险 + 用户禁 Tailwind 强制手写组件 → 增量验证最稳。Big-bang 在 23 条 AC + Rust 改动 + SCSS 重构 + Pinia 拆分同时进行时回归风险不可控。

---

## 3. Implementation Steps

### Wave 1: SCSS 架构 + 基础 UI 组件（预计 4 步）

#### Step 1.1 — SCSS token 系统 [AC4][AC14]

- 创建 `src/styles/_tokens.scss` — 移植 `src/styles.css:1-100` 的 `:root` token 块（含 `--accent`/`--space-*`/`--radius-*`/`--text-*`/`--app-bg`/`--app-border`/`--app-muted`），结构化为 `$colors`/`$spacing`/`$radius`/`$typography` SCSS maps；同时输出 `:root { ... }` CSS 变量供运行时主题切换使用
- 创建 `src/styles/_base.scss` — reset（box-sizing/margin/padding）+ 全局排版（font-family/line-height/code.mono）+ 滚动条样式
- 创建 `src/styles/_utilities.scss` — `.mono` / `.num` / `.muted` / `.stack` / `.row-between` / `.truncate` 工具类
- 创建 `src/styles/main.scss` — `@forward '_tokens.scss'` + `@forward '_base.scss'` + `@forward '_utilities.scss'`，作为 vite 入口
- 修改 `vite.config.js` — 添加 `css.preprocessorOptions.scss.additionalData: '@use "@/styles/_tokens.scss" as *;'`，让所有组件 `<style scoped lang="scss">` 自动注入 token
- 修改 `src/main.js` — 入口改为 `import './styles/main.scss'`（替代旧 `import './styles.css'`）
- 安装 sass dev dep：`npm install -D sass`
- **Acceptance**：`npm run dev` 启动无 SCSS 编译错误；旧组件视觉无变化（变量名向后兼容）

#### Step 1.2 — 基础 UI 组件库骨架 [AC3][AC4][AC14]

- 创建 `src/components/ui/` 目录
- 创建 `src/components/ui/AppButton.vue` — props: variant(primary/ghost/subtle/danger)/size(sm/md)/disabled/loading；用 SCSS token 不硬编码
- 创建 `src/components/ui/AppInput.vue` — props: modelValue/placeholder/type(password)/error；emits update:modelValue
- 创建 `src/components/ui/AppModal.vue` — teleport-to body；props: open/title；emits close；ESC 关闭 + 焦点陷阱
- 创建 `src/components/ui/AppDrawer.vue` — 从右侧/底部滑出；props: open/side(right/bottom)/width；emits close
- 创建 `src/components/ui/AppTooltip.vue` — hover 延迟显示；props: content/placement(top/right/bottom/left)
- 创建 `src/components/ui/AppSelect.vue` — 自定义下拉（不用 native `<select>`）；props: options/modelValue
- 创建 `src/components/ui/AppTable.vue` — slots: columns/data；支持列头排序、行点击
- 创建 `src/components/ui/AppContextMenu.vue` — 右键定位；props: items/open/x/y
- 创建 `src/components/ui/AppTab.vue` + `src/components/ui/AppTabGroup.vue` — props: tabs/active；emits update:active
- 创建 `src/components/ui/AppStatusBar.vue` — slot-based 段落布局
- 创建 `src/components/ui/AppBreadcrumb.vue` — props: items；emits navigate
- 创建 `src/components/ui/AppProgress.vue` — props: percent/variant(linear/circular)
- 创建 `src/components/ui/index.js` — 统一 export
- **Acceptance**：每个组件单独通过手写 demo 验证（不依赖 Storybook）；`npm run build` 通过

#### Step 1.3 — 浅色/深色/跟随系统三态主题 [AC5]

- 在 `src/styles/_tokens.scss` 增加 `[data-theme="dark"]` / `[data-theme="light"]` / `[data-theme="auto"]` 选择器；auto 模式通过 `@media (prefers-color-scheme)` 切换
- 在 `src/composables/useTheme.js`（如不存在则新建）封装 `applyTheme(theme)` 写入 `document.documentElement.dataset.theme`
- 修改 `src/stores/workbench.js`（暂留，Wave 2 才拆）的 theme 状态调用 useTheme
- **Acceptance**：手动切换三态无视觉断裂；浏览器预览模式可切

#### Step 1.4 — Wave 1 验收

- `npm run build` 通过
- `npm run test:ui` 通过（测试 DOM 结构未变）
- **浏览器预览模式回归**（Critic 改进 10）：`npm run dev` 渲染正常（Wave 1：现有 4-tab 结构 + 新 SCSS token 共存）
- 提交 commit：`refactor: introduce SCSS token system + base UI components (Wave 1)`

---

### Wave 2: Pinia store 拆分 + App.vue 骨架化（预计 4 步）

#### Step 2.1 — 拆出 `useSessionsStore` [AC12]

- 创建 `src/stores/sessions.js` — 从 `src/stores/workbench.js`（1572 行）抽取 session 相关 state：`sessions` / `activeSessionId` / `connectSelected` / `connectLoading` / `sshWrite` / `sshResize` / `sshDisconnect` / `sshConfirmHostKey` / `sshKeyboardResponse`；保留所有现有 Tauri command 调用签名不变
- **必迁移事件监听器**（Critic 发现）：`progressUnlisten` / `hostKeyUnlisten` / `keyboardUnlisten`（`workbench.js:67-69, 206-218`）+ `setupEventListeners` / `disposeEventListeners` 必须随 session state 一并迁入，否则 Wave 3 删除 workbench.js 时会丢失 SSH 输出/断连/host key 事件流
- **必迁移 OSC parser**（Critic 发现 + 改进 1）：`createOscParser` + parser factory + `oscParser.feed(text)` 调用（`workbench.js:1049, 1090-1117`）随 session state 迁入；listener 内部调用从 `session.decoder.decode` + `oscParser.feed(text)` + `term.write(text)` 保留不变
- **必迁移 host key 超时 watcher**（Critic 发现）：`watch(hostKeyPrompt, ...)`（`workbench.js:99-113`）+ `hostKeyTimeout` 闭包变量必须随 `sshConfirmHostKey` 一起迁入，保持与后端 60s 超时 + 5s 缓冲（共 65s）对齐；漏迁会导致 modal 卡死
- 在 `src/stores/workbench.js` 中改为 `import { useSessionsStore }` 并 re-export 兼容旧调用（避免一次性破坏所有 import）
- **Acceptance**：`npm run dev` 连接真实 SSH 主机（`npm run tauri:dev`）仍能打开终端；OSC 标题响应；host key modal 65s 后能自动清理

> **localStorage key 保留约束**（Critic 改进 3，Wave 2 全程适用）：所有新 domain store 必须复用现有 localStorage key，**禁止重命名**——`myshelltool-theme`（`workbench.js:8`）→ `useUiStore`；`myshelltool-terminal-font`（`workbench.js:59`）→ `useUiStore`；`myshelltool-terminal-aside`（`workbench.js:60`）→ `useUiStore`；`myshelltool-assets`（`workbench.js:30, 1560`）→ `useUiStore`。**无迁移逻辑**，仅迁移读写调用。验证：`grep -rn "myshelltool-" src/stores/` 返回所有 4 个 key。

#### Step 2.2 — 拆出 `useFilesStore` / `useTunnelsStore` / `useAssetsStore` [AC12]

- 创建 `src/stores/files.js` — 抽取 `remoteEntries` / `localEntries` / `remoteCwd` / `localCwd` / `transferQueue` / `sftpListDirectory` / `sftpUpload` / `sftpDownload` / `sftpDelete` / `sftpRename` / `sftpMkdir` / `fsLocalListDir` / `fsLocalMkdir` / `fsLocalDelete` / `fsLocalRename`
- 创建 `src/stores/tunnels.js` — 抽取 `tunnels` / `tunnelStart` / `tunnelStop` / `tunnelList`
- 创建 `src/stores/assets.js` — 抽取 `assets` / `groupedAssets` / `assetFilter` / `saveConnectionAsset` / `listConnectionAssets` / `saveCredential` / `deleteCredential` / `getCredentialStatus`
- workbench.js 中保留 re-export 兼容层
- **Acceptance**：`npm run tauri:dev` 文件浏览/上传/下载/隧道/凭据保存全部仍工作；`npm run test:ui` 通过

#### Step 2.3 — 拆出 `useUiStore` + App.vue 骨架化准备 [AC12][AC13]

- 创建 `src/stores/ui.js` — 抽取 `theme` / `activeTab` / `sidebarOpen` / `sidebarWidth` / `terminalSearchOpen` / `transferDrawerOpen` / `commandPaletteOpen` / `shortcutCheatsheetOpen`
- 不再修改 workbench.js（Wave 2 结束后 workbench.js 可视为 legacy re-export 壳，等 Wave 3 完成后删除）
- **Acceptance**：所有现有 UI 状态在 store 中可访问；`npm run test:ui` 通过

#### Step 2.4 — Wave 2 验收

- `npm run build` 通过
- `npm run tauri:dev` 启动后所有现有功能（终端/文件/隧道/资产）仍工作
- 浏览器预览模式回归：`npm run dev` 渲染正常
- 提交 commit：`refactor: split workbench store into 5 domain stores (Wave 2)`

---

### Wave 3: 5 区域布局 shell + 组件目录重组（预计 5 步）

#### Step 3.1 — shell 组件目录骨架 [AC13]

- 创建 `src/components/shell/` 目录
- 创建 `src/components/shell/AppTitleBar.vue` — 全局搜索 Ctrl+K / 主题切换 / 同步 / 警告按钮（用 lucide-vue-next 图标）
- 创建 `src/components/shell/AppStatusBar.vue` — SSH 状态 / backend 模式 / 传输进度摘要 / 警告数；点击展开传输抽屉（用 AppDrawer）
- 创建 `src/components/shell/AppShellLayout.vue` — CSS Grid 5 区域布局：`grid-template-areas: "titlebar titlebar titlebar" "sidebar center-top right" "sidebar center-bottom right" "statusbar statusbar statusbar"`
- **Acceptance**：`npm run dev` 看到空白 5 区域布局，不崩

#### Step 3.2 — 左侧栏 ConnectionSidebar [AC6]

- 创建 `src/components/shell/ConnectionSidebar.vue` — 用 assets store 的 `groupedAssets` 渲染树；分组折叠 / 过滤输入 / 快速连接输入框（`ssh user@host` 解析）；点击主机调 `useSessionsStore.connectSelected`
- 用 `src/components/ui/AppInput.vue` 做过滤 + 快速连接
- 用 lucide-vue-next 的 Server/Folder/ChevronRight 等图标
- **Acceptance**：左侧栏显示资产树，点击连接主机，终端出现在中上区域

#### Step 3.3 — 中上 TerminalSurface 整合 [AC7]

- 移动并 restyle 现有 `src/components/terminal/` 下 8 个组件（TerminalTabs/TerminalToolbar/TerminalSearchBar/TerminalPane/ConnectionStatusPill/ShortcutCheatsheet/CommandPalette/DangerousPasteConfirm）到新视觉调性（更薄边框、更克制配色）
- 创建 `src/components/terminal/TerminalSurface.vue` 作为容器，组合上述 8 个组件 + 工具栏图标按钮（搜索/复制/粘贴/清屏/字体+/-/重连/全屏/侧栏开关）
- OSC 标题解析：当前在 `src/stores/workbench.js:1049` (`createOscParser`) 与 `:1090-1117`（parser factory + feed 实现）；Wave 2.1 拆 `useSessionsStore` 时必须把 `createOscParser` + `oscParser.feed(text)` 调用一并迁入新 store，Wave 3.3 仅迁移 UI 引用（TerminalTabs 等消费者）
- 快捷键保留：Ctrl+Shift+F 终端搜索 / Ctrl+Shift+C/V 复制粘贴 / Ctrl+Tab 会话切换 / Ctrl+=/-/0 字体缩放
- **WebGL addon 保留**（Critic 改进 6）：`@xterm/addon-webgl`（`package.json:25`）当前活跃；TerminalPane restyle 后必须仍 attach WebGLAddon，如 scoped CSS 导致 canvas 渲染异常则 fallback 到 CanvasAddon；Wave 3.3 commit message 必须包含 `webgl-verified` 标记
- **xterm 容器样式穿透**：scoped 样式下用 `:deep(.xterm)` 穿透；xterm 容器禁用 `overflow: hidden` 之外的 transform/filter（破坏 WebGL canvas）
- **Acceptance**：中上区域显示 tab 多会话，工具栏可工作，内嵌搜索可弹出；手动验证 `term.write('\\x1b]0;osc-test\\x07')` → tab title 更新为 `osc-test`；连接真实 SSH 主机后 GPU info 确认 WebGL addon 活跃（无 canvas fallback）

#### Step 3.4 — 中下 FileSurface [AC8]

- 创建 `src/components/files/` 目录
- 创建 `src/components/files/FileSurface.vue` — 双栏布局（左本地 + 右远程）
- 创建 `src/components/files/FileColumn.vue` — 单栏（path/breadcrumb/filter/list）；用 useFilesStore 的 `localEntries` 或 `remoteEntries`
- 创建 `src/components/files/TransferDrawer.vue` — 用 AppDrawer + AppProgress 显示传输队列
- 用 AppBreadcrumb 做路径；用 AppContextMenu 做右键菜单（含批量操作）
- 快捷键：F2 重命名 / Delete 删除 / F5 刷新 / Ctrl+A 全选 / Esc 清选 / Ctrl 多选 / Shift 范围选
- **Acceptance**：中下双栏显示本地/远程文件，所有快捷键工作，传输队列抽屉可弹出

#### Step 3.5 — App.vue 退化为布局容器 [AC13]

- 重写 `src/App.vue`（1576 行 → <200 行）：只负责挂载 AppShellLayout + 渲染 5 区域 + 顶级路由（如 command palette 全局快捷键）
- 删除 `src/stores/workbench.js` 中所有 re-export 兼容层（所有 import 应已迁移到新 domain stores）
- 删除旧 `src/styles.css`（已被 `src/styles/main.scss` 替代）
- **Acceptance**：App.vue 文件行数 <200；`npm run build` 通过；`npm run tauri:dev` 启动后 5 区域布局可见（中下文件 / 中上终端 / 左侧栏资产 / 右侧栏占位 / 底部状态栏）

#### Step 3.6 — Wave 3 验收

- `npm run build` 通过
- `npm run tauri:dev` 5 区域布局全部可见（右侧栏可临时显示占位）
- `npm run test:ui` 通过（测试 DOM 结构已适配）
- 浏览器预览模式回归：`npm run dev` 渲染 5 区域
- 提交 commit：`refactor: 5-region shell layout + App.vue skeletonization (Wave 3)`

---

### Wave 4: Resource monitor 后端 + 前端 + 右侧栏（预计 5 步）

#### Step 4.1 — Rust resource_monitor 模块骨架 [AC15][AC17]

- 创建 `src-tauri/src/resource_monitor.rs` — 用 `src-tauri/src/fs_local.rs` 作为模板（同样的模块结构 + `#[command]` 注解 + serde 序列化）
- 定义 `ResourceSnapshot` 结构体：`{ cpu_usage: f32, cpu_cores: u32, mem_total: u64, mem_used: u64, net_rx_bytes: u64, net_tx_bytes: u64, disk_read_bytes: u64, disk_write_bytes: u64, disk_total: u64, disk_used: u64, timestamp: u64 }`
- 实现 `parse_proc_stat(content: &str) -> Result<(f32, u32)>` — 解析 `/proc/stat` 第一行 + 计数行数得 CPU 占用率与核心数
- 实现 `parse_proc_meminfo(content: &str) -> Result<(u64, u64)>` — 解析 `/proc/meminfo` 的 `MemTotal`/`MemAvailable`
- 实现 `parse_proc_net_dev(content: &str) -> Result<(u64, u64)>` — 累加 `/proc/net/dev` 所有接口（除 lo）的 rx/tx bytes
- 实现 `parse_proc_diskstats(content: &str) -> Result<(u64, u64)>` — 解析 `/proc/diskstats` 块设备读写扇区
- **优先读 `/proc/*`**，不依赖 `top`/`free`/`vmstat`/`iostat` 二进制（避免不同发行版命令差异）
- **Acceptance**：`cargo check --manifest-path src-tauri/Cargo.toml` 通过；单元测试覆盖 4 个 parser

#### Step 4.2 — Resource monitor SSH 执行器 + Tauri 命令 [AC15][AC16][AC18]

- 在 resource_monitor.rs 实现 `ResourceMonitorHandle` — 持有 sessionId + tokio task handle + interval
- **SSH 通道策略**（Critic 改进 5 + Architect C1）：现有 `SshCommand` enum（`src-tauri/src/ssh.rs:156-160`）只有 `Write/Resize/Disconnect` 三个 variant，**不支持 exec 通道复用**。在 enum 添加 `MonitorExec(String)` variant；`resource_monitor_start` 通过现有 `cmd_tx: mpsc::UnboundedSender<SshCommand>`（`ssh.rs:163`）发送 `MonitorExec('cat /proc/stat; cat /proc/meminfo; cat /proc/net/dev; cat /proc/diskstats')`；session task loop 收到 `MonitorExec` 时打开新 `channel_session` 执行命令并通过 `app.emit("resource-monitor-snapshot", payload)` 推送结果，避免打开独立 SSH session；命令完成后 channel 自动关闭，不与用户交互 PTY 冲突
- 实现 `resource_monitor_start(session_id: String, interval_ms: u64) -> Result<(), String>` — 启动 tokio interval task，按 interval 周期发送 `MonitorExec`；interval 默认 **2000ms**（Architect C6：CPU/内存 2s 合理；diskstats 不应快于 5s 但首版统一 2s，v2 改自适应）
- 实现 `resource_monitor_stop(session_id: String) -> Result<(), String>` — abort task + 清理 handle
- 实现 `resource_monitor_snapshot(session_id: String) -> Result<Option<ResourceSnapshot>, String>` — 一次性快照（同步路径）
- 实现 `resource_monitor_list_active() -> Result<Vec<String>, String>` — 返回当前活跃 sessionId 列表
- 在 session disconnect 钩子（`ssh_disconnect` 在 `src-tauri/src/ssh.rs:654`）中调用 `resource_monitor_stop`，防泄漏
- 修改 `src-tauri/src/lib.rs` — 在 `invoke_handler!` 注册 4 个新命令；在 `mod` 声明添加 `mod resource_monitor;`
- **Acceptance**：`cargo check --manifest-path src-tauri/Cargo.toml` 通过；连接真实 Linux 主机后 `resource_monitor_snapshot` 返回合理数据

#### Step 4.3 — `useResourceMonitorStore` + 事件订阅 [AC12][AC16]

- 创建 `src/stores/resourceMonitor.js` — state: `activeSessionId` / `snapshot` / `history`（保留最近 60 个采样点）/ `enabled`
- action `start(sessionId, intervalMs)` — 调 `invoke('resource_monitor_start', ...)`；用 `listenBackendEvent('resource-monitor-snapshot', handler)` 订阅事件，更新 snapshot + push history
- action `stop(sessionId)` — 调 `invoke('resource_monitor_stop', ...)`；取消事件订阅
- 与 `useSessionsStore.activeSessionId` 联动：active session 变化时 stop 旧 + start 新
- **Acceptance**：浏览器预览模式（非 Tauri runtime）下 store 检测到 `!isTauriRuntime()` 直接显示占位 state，不调用 invoke

#### Step 4.4 — 右侧栏 ResourceMonitor + OpsSummary [AC9][AC10]

- 创建 `src/components/resource-monitor/` 目录
- 创建 `src/components/resource-monitor/ResourceMonitorPanel.vue` — 容器组件，订阅 resourceMonitorStore；未连接时显示空状态
- 创建 `src/components/resource-monitor/CpuChart.vue` — 折线图显示最近 60 个 CPU 采样点（用 SVG 手写，不引图表库）
- 创建 `src/components/resource-monitor/MemoryChart.vue` — 内存已用/总量 + 折线
- 创建 `src/components/resource-monitor/NetworkChart.vue` — 上传/下载速率折线
- 创建 `src/components/resource-monitor/DiskChart.vue` — 读写速率折线 + 已用空间进度条（用 AppProgress）
- 创建 `src/components/shell/OpsSummaryPanel.vue` — 运维摘要：主机摘要 / 标签 / 最近连接 / 凭据状态（用 useAssetsStore）/ 隧道健康（用 useTunnelsStore）/ Git 同步状态
- 整合到 AppShellLayout 右侧栏：上半 ResourceMonitorPanel + 下半 OpsSummaryPanel，用 CSS Grid 上下分割
- **Acceptance**：连接真实 Linux SSH 主机后，右侧栏显示 4 类实时图表 + 运维摘要；断开后图表空状态

#### Step 4.5 — Wave 4 验收

- `cargo check --manifest-path src-tauri/Cargo.toml` 通过
- `npm run build` 通过
- `npm run tauri:dev` 连接真实 Linux SSH 主机，右侧栏 4 类图表实时刷新
- 浏览器预览模式：右侧栏显示"需要桌面端"占位
- 提交 commit：`feat: resource monitor backend + frontend charts (Wave 4)`

---

### Wave 5: 视觉抛光 + AC 验收 + 回归测试（预计 4 步）

#### Step 5.1 — 视觉调性统一 [AC1][AC2][AC5]

- 全局审视：边框颜色统一用 `$app-border` token；间距统一用 `$space-*`；圆角统一用 `$radius-*`
- 去掉所有硬编码颜色（grep `#[0-9a-f]{3,6}` 在 src/ 下，全部替换为 token 引用）
- 调整 z-index 层级表（写在 `_tokens.scss` 中 `$z-indexes` map：base/dropdown/modal/toast/tooltip）
- 5 区域边框处理：用 `border-inline-end: 1px solid var(--app-border)` 而非阴影/双重边框（FinalShell 卡片堆叠感来源）
- **Acceptance**：视觉接近 Tabby/Termius 截图；浅色/深色/跟随系统三态切换无断裂

#### Step 5.2 — 5 区域布局边界兜底 [AC1][AC22]

- 在 AppShellLayout 定义 `min-width` 阈值（如 1280px），低于此宽度允许横向滚动而非堆叠
- 5 区域每个 surface 内部独立滚动（overflow: auto），不溢出父 grid
- 浏览器预览模式（`npm run dev`）显式回归：5 区域全部渲染（资源监控显示"需要桌面端"占位）
- **Acceptance**：1280px+ 宽度下 5 区域同时可见；<1280px 横向滚动兜底；浏览器预览模式不崩

#### Step 5.3 — 测试文件适配（双文件）[AC20]

- **必适配两个测试文件**（Critic 改进 4）：
  - `tests/ui-smoke.mjs` — 现有断言依赖 `.desktop-only-banner`（line 26）与 `.window`（line 33）selector；新 5 区域布局会破坏这些。改为 `[data-region="titlebar"]` / `[data-region="statusbar"]` 等新 selector；核心断言保留：连接表单可填、终端可挂载、文件可列、资产可存
  - `tests/ui-host-key.mjs` — mock `__TAURI__.core.invoke`（line 30-40）+ 断言 host key modal flow；Wave 3.5 删除 workbench.js re-export 后 + Wave 3.4 引入 FileSurface 后这些 modal selector 也会失效；必须同步适配
- 增加 5 区域可见性断言：titlebar/sidebar/center-top/center-bottom/right/statusbar 6 个 grid area 均存在
- 增加资源监控占位断言：浏览器预览模式下显示"需要桌面端"
- **不允许 silently 跳过任一文件**；如 `package.json` 的 `test:ui` 脚本当前只跑 `ui-smoke.mjs`，需扩展为同时跑两个（如 `node tests/ui-smoke.mjs && node tests/ui-host-key.mjs`）
- **Acceptance**：`npm run test:ui` 跑两个文件均 exit 0

#### Step 5.4 — Wave 5 全 AC 验收 + 最终 commit

- 跑完 AC1-AC23 全部验收清单（见第 5 节）
- 提交 commit：`polish: final visual pass + AC1-AC23 acceptance (Wave 5)`

---

## 4. Risks and Mitigations

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| Pinia store 拆分时遗漏 reactive 引用导致 terminal 失效 | 高 | 高 | 每个 store 拆完先跑 `npm run dev` + `npm run tauri:dev` 实连测试再 commit；保留 re-export 兼容层过渡（Wave 2-3） |
| 浏览器预览模式回归（AC22 破坏） | 中 | 高 | 每波结束显式检查 `!isTauriRuntime()` 分支；resourceMonitorStore 在非 Tauri runtime 直接返回占位 state |
| resource_monitor 在不同 Linux 发行版 `/proc/*` 格式差异 | 中 | 中 | 优先读 `/proc/*`（POSIX 标准）不依赖 top/free/vmstat 二进制；parser 单元测试覆盖典型 + 边界样本；解析失败时该字段返回 0 而非整体报错 |
| SCSS token 重命名导致大量样式失效 | 中 | 中 | `_tokens.scss` 中保留旧 CSS 变量名作 alias（`--app-bg: var(--surface)`），分波重命名；grep 检查所有硬编码颜色 |
| 5 区域布局在小屏下溢出 / 视觉崩 | 中 | 中 | 定义 `min-width: 1280px` + 横向滚动兜底；不为小屏做完整 responsive（spec Non-Goals） |
| Rust resource_monitor task 泄漏（session 断开未停） | 中 | 中 | 在 `ssh_disconnect` 命令内显式调 `resource_monitor_stop`；`resource_monitor_list_active` 暴露用于调试 |
| xterm 6 + addon 在 SCSS scoped 样式下渲染异常 | 低 | 中 | Wave 3.3 commit 前必须跑：`npm run tauri:dev` → 连接真实 SSH → `term.write('\\x1b]0;osc-test\\x07')` → 断言 tab title == `osc-test` → 写入 1MB scrollback → 确认无 WebGL canvas 渲染异常（GPU info 显示 WebGL 活跃）；xterm 容器用 `:deep(.xterm)` 穿透 scoped |
| OSC 标题解析在 terminal 组件 restyle 时被破坏 | 低 | 高 | Wave 2.1 已把 `createOscParser`（`workbench.js:1049, 1090-1117`）迁入 `useSessionsStore`；Wave 3.3 commit 前显式手动验证：连接 SSH → `echo -ne '\\033]0;custom\\007'` → 断言 TerminalTabs 标签更新为 `custom`；解析逻辑零改动 |
| 8 个 terminal 组件迁移时丢失快捷键绑定 | 低 | 高 | Wave 3.3 单独跑 `useTerminalShortcuts` 全部快捷键清单（Ctrl+Shift+F/C/V/P、Ctrl+Tab、Ctrl+W、Alt+Enter、Ctrl+=/-/0、`?`）；每条都手动触发并断言 |
| 删除 workbench.js 后遗漏某个 import 路径 | 低 | 中 | Wave 3.5 删除前 grep 全代码库 `workbench` 引用，确认全部迁移 |

---

## 5. Verification Steps (mapped to AC1-AC23)

| AC | Verification | Wave |
|----|-------------|------|
| AC1 | 手动：`npm run tauri:dev` 启动，截图验证 5 区域同时可见（sidebar/center-top/center-bottom/right/statusbar） | W3 |
| AC2 | 视觉对比：截图与 Tabby/Termius 截图比对，确认无卡片堆叠/多色块/边框嵌套 | W5 |
| AC2a | 文件 + 视觉：`docs/design-reference/tabby-screenshot.png` 存在作为锚点；截图与该参考 diff 主观相似度 ≥70%（无嵌套背景填充、单像素边框、低视觉重量） | W5 |
| AC3 | 文件检查：`ls src/components/ui/` 列出 12 个基础组件；grep 排除 `tailwind`/`naive`/`element-plus`/`ant-design` import | W1 |
| AC4 | 文件检查：`src/styles/_tokens.scss` 存在；grep 硬编码颜色 `#[0-9a-f]{6}` 在 src/ 下应为 0（除 _tokens.scss） | W5 |
| AC5 | 手动：theme toggle 切换浅色/深色/跟随系统，截图无断裂 | W5 |
| AC6 | 手动：左侧栏点击资产连接主机，终端在中上区域出现 | W3 |
| AC7 | 手动：终端 tab 显示 host+OSC 标题+状态点；Ctrl+Shift+F 搜索弹出；Ctrl+=/-/0 字体缩放工作 | W3 |
| AC8 | 手动：双栏文件管理；F2 重命名 / Delete 删除 / F5 刷新 / Ctrl+A 全选 / Esc 清选 / 右键菜单批量操作 | W3 |
| AC9 | 手动：连接真实 Linux SSH 主机后右侧栏上半 4 类图表实时刷新；断开后空状态 | W4 |
| AC10 | 手动：右侧栏下半运维摘要显示主机/标签/最近连接/凭据/隧道/Git 同步 | W4 |
| AC11 | 手动：底部状态栏显示 SSH 状态/backend/传输/警告；点击展开传输抽屉 | W3 |
| AC12 | 文件检查：`ls src/stores/*.js` 列出 sessions/files/tunnels/assets/ui/resourceMonitor 6 个；`wc -l src/stores/workbench.js` 应不存在或仅 re-export 壳 | W2 |
| AC13 | 文件检查：`wc -l src/App.vue` <200；`ls src/components/{shell,terminal,files,resource-monitor,ui}/` 5 个目录存在 | W3 |
| AC14 | 文件检查：`ls src/styles/{_tokens,_base,_utilities,main}.scss` 4 文件存在；grep `<style scoped lang="scss">` 在组件中 | W1 |
| AC15 | 文件检查：grep `pub fn resource_monitor_start/stop/snapshot/list_active` 在 `src-tauri/src/resource_monitor.rs`；4 个 `#[command]` 注解 | W4 |
| AC16 | 手动：监听 `resource-monitor-snapshot` 事件流；前端 store 收到数据 | W4 |
| AC17 | 代码审查：parser 函数解析 `/proc/stat`/`/proc/meminfo`/`/proc/net/dev`/`/proc/diskstats`；单元测试覆盖 | W4 |
| AC18 | 手动：session disconnect 后调 `resource_monitor_list_active` 确认 task 清理 | W4 |
| AC19 | 命令：`npm run build` exit 0 | 每波 |
| AC20 | 命令：`npm run test:ui` exit 0 | W5 |
| AC21 | 命令：`cargo check --manifest-path src-tauri/Cargo.toml` exit 0 | W4 |
| AC22 | 手动：`npm run dev` 浏览器打开，5 区域全部渲染，资源监控显示"需要桌面端"占位 | W5 |
| AC23 | 手动：`npm run tauri:dev` 启动，连接真实 SSH Linux 主机，全部 5 区域可用 + 资源监控实时数据 | W5 |
| AC24 | 命令：`cargo test --manifest-path src-tauri/Cargo.toml resource_monitor` exit 0（4 个 /proc parser 单元测试全过） | W4 |

---

## 6. ADR (Architecture Decision Record)

- **Decision**：选择 Wave-based incremental delivery（Option A），分 5 波交付
- **Drivers**：
  - spec 强制 23 条 AC（每条都要可独立验收）
  - 浏览器预览模式回归风险（spec AC22 强制要求）
  - 用户禁 Tailwind/UnoCSS/UI 库强制手写组件（spec Round 4-5）
  - 后端 resource_monitor 模块需新增 + 现有 SSH/SFTP/tunnel 核心不可改（spec Constraints）
- **Alternatives considered**：
  - Option B（Big-bang rewrite on feature branch）：拒绝 — 23 条 AC + Rust 改动 + SCSS 重构 + Pinia 拆分 + App.vue 骨架化同时进行时回归风险不可控；无法增量验证；与现有 8 个 terminal 组件 + 1572 行 workbench.js 状态需一次性迁移
  - Option C（仅视觉换皮）：已被 spec Round 0 排除（topology 锁定为"全部，连代码架构也重写"）
- **Why chosen**：
  - 每 Wave 独立可 commit / 可 `npm run build` 验证 / 可手动 smoke test
  - Wave 3 落地后 5 区域布局即可见（中段里程碑）
  - Wave 4 单独完成后端 + 资源监控真实数据
  - Wave 5 抛光 + 全 AC 验收作为收尾
  - 与 spec "保留可复用资产" 原则一致（8 terminal 组件 / composables / lib 全部不破坏）
- **Consequences**：
  - 总工作量略多于 Big-bang（多写 Wave 2-3 之间的过渡状态 + re-export 兼容层）
  - 风险降低一个数量级：中途暂停时前 N 个 Wave 是可用的
  - 跨波次需维护向后兼容（旧 CSS 变量名 alias / workbench.js re-export 壳）直到 Wave 3 完成
- **Follow-ups**：
  - Wave 5 完成后考虑 v2 拓展项（spec Non-Goals 中推迟项）：
    - 拖拽上传/下载、目录递归传输
    - 书签快速跳转、同步浏览
    - SSH agent/Pageant、OpenSSH config、SSH 证书、FIDO2
    - 主题编辑器/自定义皮肤
    - 资源监控 Windows/macOS 远程主机支持（当前仅 Linux）

### 6.1 Capability 文件说明（Critic 改进 9）

`resource_monitor_*` Tauri 命令**不需要**在 `src-tauri/capabilities/default.json` 添加权限条目。Tauri 2 模型中只有 plugin-scoped 命令需要 capability 条目；app-defined 命令（在 `invoke_handler!` 直接注册的）自动允许。当前 `src-tauri/capabilities/default.json` 只含 `core:*` 权限——这是正确的，Wave 4 不要"修复"非 bug。验证：Wave 4 完成后 `git diff src-tauri/capabilities/default.json` 对 capability 数组返回空（identifier/description 元数据可改）。

---

## 7. Open Questions Resolution (Critic + Architect 闭环)

> Round 1 共识循环中识别的 7 个 spec 歧义。Architect 已给出方向，Critic 已验证 file:line 准确性。下表为最终决议，**已嵌入相关 Step**，不需要再询问用户。

| # | 问题 | 决议 | 落地位置 |
|---|------|------|---------|
| 1 | resource_monitor SSH 通道 | **新增 `SshCommand::MonitorExec(String)` variant**，通过 `cmd_tx` 复用 session；session task 收到时开新 `channel_session` 执行 + emit 结果，不与 PTY 冲突 | Step 4.2 |
| 2 | 图表库 | **纯手写 SVG**（不引 d3/chart.js，与"完全手写组件"原则一致） | Step 4.4 |
| 3 | transferDrawer 事件 | **复用现有 `sftp-transfer-progress`** event（`workbench.js:12`）；不新建事件，避免破坏 spec 约束 Principle 4 | Step 3.4 |
| 4 | 右侧栏宽度 | **固定 280px**（Wave 4）；可拖拽分屏推迟 v2 | Step 4.4 |
| 5 | 5 区域最小屏宽 | **min-width 1280px + 横向滚动兜底**（不响应式折叠，违反 spec AC1 "同时可见"） | Step 5.2 |
| 6 | resource_monitor interval | **默认 2000ms**；v2 改自适应（CPU/内存 2s、disk 5s） | Step 4.2 |
| 7 | OSC 标题响应保护 | **Wave 2.1 迁移 createOscParser 入 useSessionsStore**；Wave 3.3 加 explicit manual test（已 promote 到 AC24 相关验证） | Step 2.1 + Step 3.3 |

---

## 8. Changelog (consensus loop iteration 1)

**Round 1 — Critic APPROVE_WITH_IMPROVEMENTS（10 项已应用）**

| # | 来源 | 改进 | 应用位置 |
|---|------|------|---------|
| 1 | Architect + Critic | 修正 OSC parser 位置错误（`terminalController.js` → `workbench.js:1049`） | Step 3.3 |
| 2 | Critic 隐藏 gap | Step 2.1 加 hostKeyPrompt watcher (65s) + hostKeyTimeout 闭包 + 3 个 listener handle 迁移 | Step 2.1 |
| 3 | Critic 隐藏 gap | localStorage key 保留约束（`myshelltool-theme/font/aside/assets` 禁重命名） | Step 2.1 |
| 4 | Critic 隐藏 gap | Step 5.3 加 `tests/ui-host-key.mjs` 双文件适配 | Step 5.3 |
| 5 | Architect + Critic | Step 4.2 加 `SshCommand::MonitorExec` variant + 修正"复用 session handle"误述 | Step 4.2 |
| 6 | Critic 隐藏 gap | Step 3.3 加 WebGL addon 保留 + `:deep(.xterm)` + canvas fallback | Step 3.3 |
| 7 | Critic 评分 0.65/0.70 | Risks table row 7-9 替换为具体验证命令（含 OSC + WebGL 测试） | Risks |
| 8 | Architect + Critic | AC table 加 AC2a（Tabby 参考截图）+ AC24（cargo test parser） | Verification |
| 9 | Critic 隐藏 gap | Section 6.1 加 capability 文件说明（app-defined 命令不需要 capability 条目） | ADR 6.1 |
| 10 | Critic 评分 0.65 | 确认每波 acceptance 都含浏览器预览回归检查（Wave 1-4 已满足） | 全波验收 |

**未应用（Critic 标记为 optional）**

- Wave 1.5 提取 `useResourceMonitor` mock composable — Critic 仅要求 Wave 2.4 加显式 assertion，未强制加 Wave 1.5；Step 4.3 acceptance 已包含浏览器预览占位 state 检查，等价覆盖。

**Round 1 共识结论**：Plan 状态从 `pending-architect-review` → `consensus-approved-pending-execution`。Critic 0 CRITICAL / 3 MAJOR / 0 systemic。架构方向（Wave-based Option A）+ ADR 决策保持不变，仅落地细节加固。
