---
title: B 线 · 传统 SSH 客户端补齐 · 实施计划
project: myshelltool
status: B1 已验收通过；B2 已落地并真机验收通过（拖拽/OS 拖入两入口待用户手测）；B3 ProxyJump 链已落地并验收通过（2026-10-14）；B3 本地终端待实施
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

## B2 · SFTP 冲突处理 + 目录整传（代码已落地，真机验收待做）

### B2 实施结论（2026-10-13）
- 冲突三选落在 `lib/fileOverwrite.ts`（`OverwriteDecision` 枚举 + 上传/下载
  共用队列 + 重命名按方向探测新名，探测失败 fail-closed 保持弹窗）；
  `fs_local_stat` 契约细化（NotFound → `Ok(None)`）支撑本地「明确不存在/
  无法确认」区分。弹窗体抽 `FileOverwriteContent.vue`（常驻重命名输入框，
  预填 `xxx (1).yyy` 建议名）。
- 交互决策（用户 2026-10-13 确认）：常驻输入框（非两段式展开）；目录/多文件
  批次提供「应用到本批剩余冲突」勾选（覆盖/跳过可批量，重命名始终逐条）。
- 目录整传落在 `lib/dirTransfer.ts`（上传入口 uploadLocalPaths/uploadLocalEntry
  随迁）：前端递归（深度 ≤16 / 文件 ≤2000 超限整批中止）、mkdir 幂等
  （失败 → stat 复核为目录则复用）、整目录共用 batchId、symlink 跳过计数。
  Rust 零新命令。
- 批次取消：批次旗标（收集/mkdir 阶段可取消）+ 现有 `transfer_cancels` 通道；
  UI 双入口 = UploadProgressStrip「取消整批」+ TransferDrawer「取消批次」。
- 机器验证：vue-tsc 0 错、ipc-flows 新增 3b-3g 六场景全过、editor 五套回归过。

### B2 真机验收结果（2026-10-14，117.72.25.115 实测，全部通过）
1. ✅ 三选各分支：重命名撞名 fail-closed 报错不落盘（taken.txt 未被覆盖）；换名
   落盘正确（report-renamed.txt 内容核对）；applyAll skip（x2 不再弹窗）与
   覆盖（x1/x2 内容核对）两方向；未勾选时逐条询问默认行为（3 冲突 3 窗）。
   **抓出并修复「覆盖」按钮不渲染缺陷**（两个同条件 v-else-if 死分支，详见
   已知边界与修复史 B2 节——mock 测试 store 直调不走 DOM 按钮所以漏过）。
2. ✅ 目录上传：入口1（远程菜单「上传目录到当前目录」）+ 入口3（本地面板目录
   右键「上传目录到远程」）走通；嵌套目录创建（data/sub1、data/sub2）；mkdir
   幂等（已存在目录复核复用不报错）；100 文件大批次完成摘要与队列聚合。
3. ✅ 目录下载：递归落盘（rproj/a.txt + rproj/sub/b.txt）；本地子目录自动创建；
   本地冲突门下载方向文案 + 覆盖落盘内容核对。
4. ✅ 整批取消：抽屉「取消批次」——「已取消目录下载（剩余 70/100 如实）」、
   队列不再入队（传输中 0/队列中 0）、零半截文件（全部完整 1MB）。
5. ⏸ 栏间拖拽 / OS 拖入：CUA 无法合成 HTML5 dnd 与真实指针拖放，**待用户
   手测**；代码落点 uploadLocalEntry 目录路由已被入口3 等价覆盖。
6. 入口2（OS 拖入文件夹）同上待用户手测。

### B2 后续优化（2026-10-14，验收反馈驱动）
- 上传完成 toast 聚合（`lib/transferDoneToasts.ts`，镜像下载侧 1.5s 窗口先例）：
  目录上传 N 文件不再 N 条刷屏；目录批次摘要补跳过数。
- 新建目录跳过逐文件冲突探测（mkdir 首次成功 = 空目录 → 免 stat，省 1 RTT/文件）。
- 慢的主因是 debug 构建（russh 慢 14 倍）+ 公网 RTT × 串行往返，代理不在链路。

### B2 并发修复真机验收（2026-10-14 下午，117.72.25.115 + tauri:dev，五项全过）
1. ✅ sftp_stat 新契约：目标不存在直传无弹窗（Ok(null) 路径）+ 同名正确弹冲突窗
   （Ok(Some) 路径）+ 覆盖后服务器侧 cat 核对内容正确——无双写。
2. ✅ 目录整传 100 文件（25MB）并发：服务器侧全数落盘、零字节数偏差、零半截；
   完成 toast 聚合一条。
3. ✅ 在途取消（30×2MB）：仅 1 个落盘（29 截停）、零半截；队列呈现
   「部分取消」混合态（1 done + N cancelled）。
4. ✅ 下载侧冲突门（Ctrl+Alt+D DEV 钩子触发，见下）：本地同名 → 冲突窗（下载
   方向文案 + 预填 rconf (1).txt）→ 覆盖 → 本地内容核对正确（16 B remote-conflict）。
   ——CUA 无法在 WebView2 合成条目右键（contextmenu 被吞，text/行容器/Shift+F10
   /drag 移指针四路均试），下载入口改由 main.ts DEV 测试钩子的 Ctrl+Alt+D 触发
   （与 __myshelltool 同性质，生产构建剔除）；目录对话框/冲突窗/覆盖决策仍是
   真实 UI 全链路。
- 机器验证补跑：npm run build（main.ts 钩子改动）过。

### B2 并发传输（2026-10-14 第二批，审查驱动修复）

- 并发池（`lib/transferConcurrency.ts`，1~5 可配默认 3，设置面板持久化）跑
  目录上传/下载与平铺批；**预留的流水线并发就此落地**。
- 审查抓出并修复：
  - **P0 同目标双写**：并发把「探测→决策→写入」拆开后，同批/跨批/手动
    触发对同一目标路径会在彼此 create（O_TRUNC）前完成探测、双双放行，
    交错写坏文件且无报错。修复 = `serializeOnTarget` 同目标互斥链（会话+
    目标全路径 NUL 分隔为 key），后到者探测必然看到先到者落盘（含半截），
    正确走冲突三选；不同目标照常并发。
  - **P1 摘要口径**：目录下载/上传的完成摘要把失败文件计入完成数——
    downloadEntry 改返回队列项按终态计数，UploadBatchSummary 增 failed；
    摘要区分「完成 N / 失败 M / 跳过 K」，有失败降级 warn。
  - **P2 池异常语义**：注释宣称「不拖垮」实现却整体 reject（且孤儿任务
    无人等）——改为单项失败留 undefined 位继续派发，console.warn 留线索。
  - **P2 sftp_stat 三态契约**：远端探测原靠前端正则匹配 "no such file"
    错误串（形态 E）——后端结构化判 `StatusCode::NoSuchFile` 返回
    `Ok(None)`，与 fs_local_stat 对齐；mkdir 复核补「明确不存在」分支文案。
  - **P2 并发冷缓存**：`get_or_create_sftp` check-insert 之间建链最长 15s，
    N 个并发首传各建一条 channel（后者覆盖前者）——insert 前二次检查。
  - **P2 取消条目摘除不对称**：下载侧无条件 remove 对齐上传侧 Arc::ptr_eq。
  - **applyAll 并发语义回归（实现中发现）**：并发探测让「剩余冲突」提前
    入队等弹窗，勾选「应用到本批」后仍逐个弹——recordBatchDefault 现在
    把队列里同批 pending 一并按默认结算。
- 测试：ipc-flows 新增 3i（同名双写防护：第二次探测必在第一次落盘后 +
  冲突窗拦截 + 跳过后 upload 恰一次）/ 3j（并发上限在途峰值 ≤ 配置且真
  并行 + 单项失败不炸批）；3g 断言从「upload 恰一次」改为终态「全批
  cancelled」（并发下已在途文件被旗标一并中止，串行白盒断言过时）。
- 机器验证：build（含三道门禁 + vue-tsc）过、cargo check 过、UI 五套全过
  （ipc-flows 含新场景）。真机并发吞吐/冲突验收待做。

### 原始设计要点（保留存档）

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

### B3-ProxyJump 实施结论（2026-10-14，代码落地待真机验收）
- 方案经审查修订后实施（审查抓出 P1×2：链解析缺目标参数检不出「指回目标」环、
  后续跳 connect_stream 不包超时的理由不成立；P2×4：验证按改动面走不跑 build、
  config 导入逗号链无感知失败、连接数视角缺声明、单跳保活假设从未验收）。
- 链解析下沉 `core::jump_chain`（`parse_jump_host` 迁入 + `resolve_jump_chain`
  环/深度/缺资产 fail-closed 判据，240 单测真跑含新增 7 个）；链由资产嵌套表达，
  上限 4 跳，「跳板==目标」也算环。
- `ssh/jump.rs` 循环展开逐跳建连（消「绕开 connect_headless_with 防递归」旧结构）；
  每跳 connect/connect_stream+认证纳入该跳资产 `connect_timeout_secs` 超时
  （顺带修掉旧版裸 connect）；错误带链位（「跳板链 t→j1→j2：第 2 跳 …」）；
  `open_direct_stream` 签名未变——GUI/headless/MCP×3/隧道 remote 全路径自动获得。
- config 导入净化：`sanitizeProxyJump` 逗号链/user@ 跳过该字段 + 导入结果警示
  （资产本体照常导入）；jump_host 不支持逗号链（链单一权威 = 资产嵌套）。
- 机器验证：test:core 240 过 / cargo check(--tests) 过 / type-check 过。
- **真机验收通过（2026-10-14 用户确认）**；七项清单：① 单跳回归
  ② 双跳连接+终端+SFTP ③ 环/指回目标报错 ④ 5 跳拒绝 ⑤ 缺资产/未信任
  报错文案 ⑥ 断中间跳板下游表现（含单跳保活假设补验）⑦ 跳板不可达超时。
- 已知边界：调用方目标握手（headless.rs/session.rs `connect_stream`）无超时
  （延续现状）；每会话独立建链 N 跳 = N 条连接（MaxStartups 视角）；
  session_pool AuthSnapshot 不含 jump_host（资产增删改路径兜底池失效）。

### ProxyJump 链（原始设计要点，保留存档）
- 现状：`ssh/jump.rs` 单跳，跳板资产自身 `jump_host` 被忽略并告警。
- 目标：目标 → jump1 → jump2 → … 链式 direct-tcpip；循环检测 + 深度上限
  （默认 4 跳）+ 每跳复用该资产的凭据/host key 信任（headless 语义）。

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
