---
title: B 线 · 传统 SSH 客户端补齐 · 实施计划
project: myshelltool
status: B1 已落地并验收通过（含关窗时序缺陷修复，2026-10-13）；B2/B3 待实施
created_at: 2026-09-30
purpose: 在 MCP 暂缓适配的前提下，按用户指定顺序补齐传统 SSH 客户端六项能力
---

# B 线 · 传统 SSH 客户端补齐 · 实施计划

> 用户 2026-09-30 决策：MCP 适配暂缓，优先把传统 SSH 客户端能力补齐。
> 六项：多窗口持久化/跨窗同步、alt-screen 回放、SFTP 冲突处理、目录整传、
> ProxyJump 链、本地终端。

## 分批原则

- **按用户列出的顺序**推进，每批独立可交付、可验证，不一次铺开六项。
- 每批结束必须跑：`npm run build`（含 lint:facts / lint:size / check:ipc /
  type-check）、`npm run test:ui`、`cargo check` / `cargo check --tests`；
  Rust 逻辑变更补 `npm run test:core`（安全/协议判据放 core 真跑）。
- 真机验收项在每批文档内单独列出（多窗口与 alt 回放无法由浏览器 UI 测试覆盖）。

## B1 · 多窗口持久化/跨窗同步 + alt-screen 回放（已落地）

### 多窗口
- **主题跨窗实时同步**：`uiStore.setTheme/toggleTheme` 广播 `app-theme-changed`；
  其它窗口 `applyExternalTheme` 应用但不回广播（防回环）。
- **资产跨窗同步**：资产写操作后广播 `assets-changed`；其它窗口
  `reloadAssets({ broadcast: false })` 回读磁盘。资产窗口发现本资产被删时置
  `assetWindowDefunct`，由 `AssetWindowShell` 走既有确认关窗流程（不静默销毁）。
- **窗口持久化恢复**：`windowState.ts` 用主窗口 localStorage 记录打开的资产
  id；`asset-window-opened/closed` 事件维护实时增量；主窗口 boot 完成后
  `restoreAssetWindows` 按 id 恢复（资产已删除则跳过）。窗口几何暂不恢复。
- 实现落点：`src/lib/crossWindowEvents.ts`、`crossWindowSyncRuntime.ts`、
  `windowState.ts`、`assetWindows.ts`、`ui.ts`、`assets.ts`、`workbench.ts`、
  `AssetWindowShell.vue`、`assetWindowClose.ts`。`workbench.ts` 保持 500 行硬限。

### alt-screen 回放
- `handoffData.exportTerminalScrollback`：源终端处于 alternate buffer 时
  `excludeAltBuffer: false`，SerializeAddon 输出 `ESC[?1049h` + alt 内容；
  回放端进入 alt 屏并恢复画面，退出 vim/less 后由远端 `?1049l` 切回 normal。
- 软换行：SerializeAddon 依据 `isWrapped` 保持；纯文本 fallback 路径仍有限制。
- 与旧行为差异：非 alt 场景仍 `excludeAltBuffer: true`，不改变普通迁移。

### B1 真机验收结果（2026-10-13，全部通过）

1. ✅ 双窗口主题跟随（用户实测）；
2. ✅ 资产改名标题更新（用户实测）+ 删资产 defunct 关窗：确认弹窗内容正确、
   约 0.7s 自动销毁、connection-assets.json 磁盘交叉核对同步清理；有会话的
   「断开并关闭」确认分支经 tab 迁移窗口关闭覆盖（同一段 requestCloseAssetWindow）；
3. ✅ 退出后两窗恢复 / ⚠️→✅ 单独关闭的窗口曾复活——**验收抓出缺陷并已修复**
   （destroy 后才发 CLOSED 事件 + 资产窗口侧 localStorage 写破坏退出语义，
   详见 docs/已知边界与修复史.md B1 节），修复后双场景真机回归通过；
4. ✅ vim/less 中拖出 → alt 屏保持、`q` 退出回 normal 历史；普通 shell 迁移
   颜色/软换行不回归（用户实测）。

验收辅助事实：正式版实例并存且有活跃 SSH 时不可杀（共享 app_data_dir，测试期
不要在正式版里增删资产）；删资产测试用「复制出的无凭据临时资产」完成，不动真实资产。

## B2 · SFTP 冲突处理 + 目录整传（下一批）

### 冲突处理
- 现状：上传同名覆盖只支持「覆盖/取消」；下载到已存在本机文件没有冲突门。
- 目标：统一三选 **覆盖 / 跳过 / 重命名**（重命名弹输入框，前后端校验路径）；
  上传、下载、目录整传共用同一冲突策略入口；Strict 档保持逐条询问。
- 关键点：不改变已有「stat 失败按可能存在走确认」的 fail-closed 语义；
  `fileOverwrite.ts` 的队列串行模型扩展为 choice 枚举而非 boolean。

### 目录整传
- 上传目录：前端递归列本机目录（`fs_local_list_dir`）→ 逐文件复用
  `sftp_upload_from_file` + 目录 `sftp_mkdir`；批次聚合进现有 TransferDrawer。
- 下载目录：前端递归列远端目录（`sftp_list_dir`）→ 逐文件
  `sftp_download_to_file` + 本机建目录；符号链接/失真名按现有 fail-closed 守卫。
- 取消：复用 `transfer_cancels` 旗标，整批取消 = 当前文件取消 + 队列不再入队；
  目录层级与半截文件清理口径与单文件一致。
- 风险：深目录数量大（上限 + 进度节流）、远端符号链接环（按深度上限）、
  同名目录合并语义（先做「存在即复用/文件级确认」，不做整目录替换）。

## B3 · ProxyJump 链 + 本地终端（第三批）

### ProxyJump 链
- 现状：`ssh/jump.rs` 单跳，跳板资产自身 `jump_host` 被忽略并告警。
- 目标：目标 → jump1 → jump2 → … 链式 direct-tcpip；循环检测 + 深度上限
  （默认 4 跳）+ 每跳复用该资产的凭据/host key 信任（headless 语义）。
- 关键：每跳 `client::connect_stream` + `authenticate_headless`；中间 Handle
  保活假设需 tauri:dev 真机验收（现有单跳同样假设）；GUI/headless/MCP/SFTP/
  隧道所有走 `open_direct_stream` 的路径自动获得链能力。
- 文档：README/AGENTS/IPC 契约的「仅单跳」声明同步改为「链式（上限 N）」。

### 本地终端
- 目标：新标签/菜单开本地 shell（Windows 默认 PowerShell/ComSpec），
  与 SSH 终端共用 TerminalSurface/xterm 交互；不进入资产树。
- 后端：优先 `portable-pty`（ConPTY）拉本地 pty；新增
  `local_terminal_open/write/resize/close` 命令与 `local-output-<id>` /
  `local-closed-<id>` 事件；若依赖不可用，降级 `std::process` 管道并明确声明
  受限（无 resize/颜色完整度）。
- 前端：sessions store 扩展 session 类型（kind: ssh/local），标签行/右键菜单
  不混淆资产会话；关闭走统一生命周期，防孤儿进程。
- 风险：Windows 子系统/ConPTY 依赖、进程组退出清理、与跨窗口迁移的交互
  （本地终端不可迁移，菜单禁用）。

## 验收总原则

- 每批独立可回滚；不把六项混在一次提交。
- 关闭窗口/断开连接的语义保持现有安全层级：有在途传输、未保存编辑、连接中
  会话时不做静默销毁。
- 所有新增跨窗/协议文件按项目文件大小红线控制（workbench.ts 已贴 500 行，
  继续功能请拆到 lib 模块）。
