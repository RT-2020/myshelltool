# ADR：桌面框架选型评估 — Tauri vs Qt vs Electron

> **Status:** **APPROVED (v3)** — Critic 三审 APPROVE，ralplan 共识达成（2026-06-13）
> **Author:** Planner (oh-my-claudecode)
> **Date:** 2026-06-13 (v3 — revised per Critic ITERATE feedback)
> **Scope:** 架构决策记录（ADR），不修改任何源码
> **触发问题:** "评估一下是继续使用 Tauri 还是切换到 Qt 或 Electron 比较好？"

---

## 0.5. Revision History

### v2 → v3 改动点（Critic ITERATE 二审反馈）

本子节列出 v3 针对 Critic ITERATE 二审（见 `ralplan-critic-review-framework-choice.md`）的全部修订点，每条引用 Critic 修订项编号（R7-R11）与在本文档的落地位置：

| 编号 | 修订类别 | 修订内容 | 落地位置 |
|---|---|---|---|
| **R7**（必改，CRITICAL） | 深度对比表残留矛盾修复 | 第 2 节深度对比表第 91 行 SSH 生态成熟度行评分理由从 v1 残留的"ssh2 (Node) 功能相对弱（特别是动态 SOCKS5）"改为 **"ssh2 (Node) 在 SOCKS5/host key 交互上需自实现（与 russh 对等），但 host key 异步交互模式 + 键盘交互认证 API 形状差异带来 5-10 人天重写成本（见第 3.3 节）"**；Electron 该行评分从 3 调到 **4**；加权总分从 39 → **41**；RALPLAN-DR Summary Viable Options 方案 C Cons 描述同步更新 | 第 2 节深度对比表第 91 行 + RALPLAN-DR Summary Viable Options 方案 C + 加权总分行 |
| **R8**（建议改，MAJOR） | R6 顺序调整理由重写 | 第 113 行"顺序调整理由"原引用 Architect 张力 3 的错误技术断言（"transferQueue 强依赖 invokeBrowserPreview"）整段重写为 **承认 transferQueue 走 invokeBackend 不走 invokeBrowserPreview，顺序调整是工程美学考虑而非技术依赖** | 第 3.1 节方案 A 路径"顺序调整理由"段 |
| **R9**（建议改，MAJOR） | v1 措辞统一 | 全文 3 处描述 v1 原文的措辞统一为 **"ssh2 功能弱于 russh"**（与 Architect R5 原文一致）：第 78 行"SOCKS5 功能缺失"→ "ssh2 功能弱于 russh"；第 214 行"SOCKS5 功能查漏补缺"→ "ssh2 功能弱于 russh" | 第 1 节切换成本估算表 C 行注 + 第 3.3 节方案 C 路径 Step 4 |
| **R10**（建议改，MAJOR） | 新增验收标准小节 | 新增**"方案 A 成功的可测试标准"小节**，列出 AC-1 到 AC-5 五个可测试验收标准（grep `State<'_, Arc<Mutex<SshSessionManager>>>` = 0 / `app.manage(ssh_mgr.clone())` 已删除 / grep `invokeBrowserPreview` = 0 / 500MB 文件上传堆内存 < 200MB / Tauri 模式 E2E 通过） | 新增第 5.5 节"验收标准" |
| **R11**（建议改，PARTIAL） | mock SSH server 方案 + AC-6 | R4 缓解任务追加"调研可选 mock 方案：[rust-ssh-test-server / docker openssh / russh mock handler]，执行阶段二选一"；验收标准追加 AC-6："mock SSH server 测试覆盖 connect/upload/download/tunnel 4 个命令"；IPC OOM 判定标准已含在 AC-4（500MB 文件 < 200MB 堆） | 第 5 节风险表 R4 缓解措施 + 第 5.5 节验收标准 AC-6 + 第 7 节 Follow-ups |
| **附加 1**（Minor Finding 2） | 现状基线 remote forward 桩函数披露 | 第 1 节现状基线"隧道转发 85%"补注 **"remote forward 是返回 Err 的桩函数（`ssh.rs:1135-1143`），实际仅 local/dynamic 已实现"** | 第 1 节现状基线表 |
| **附加 2**（Ambiguity 2） | 软触发 + 硬触发关系明确 | 第 6 节回滚触发条件明确 **"硬触发 OR 软触发持续 > 2 周"**（OR 关系，非 AND），消除歧义 | 第 6 节回滚触发条件 |
| **附加 3**（Minor Finding 3） | Tauri 升级 buffer 下限校正 | Tauri 版本升级迁移成本从 "1-3 人天" 改为 **"0-3 人天"**（多数小版本 0 成本，最坏 3 人天） | 第 5 节风险表 R1 缓解措施 + 第 7 节 Consequences 负面 |

**v3 自评：** Critic R7-R11 全部落地，3 项附加修订也同步处理。R7 是 CRITICAL 必改，已诚实修复深度对比表与 R5 修订的内部矛盾；R8 已诚实承认 Architect 张力 3 的技术断言错误并改写顺序调整理由；R9 已严格忠实 v1 原文（不再创造 v1 没有的措辞）；R10/R11 补齐结构性遗漏。

---

### v1 → v2 改动点（Architect ITERATE 一审反馈）

本子节列出 v2 针对 Architect ITERATE 评审（见 `ralplan-architect-review-framework-choice.md`）的全部修订点，每条引用修订项编号（R1-R6）与在本文档的落地位置：

| 编号 | 修订类别 | 修订内容 | 落地位置 |
|---|---|---|---|
| **R1**（必改） | 工作量估算上调 | 8-12 人天 → **12-18 人天**。IPC OOM 改流式从 2-3 天调到 **4-5 天**（含 transferQueue 前端状态机改造）；删 invokeBrowserPreview 从 0.5 天调到 **1-2 天**（含 Playwright 测试改写）；测试从 2-3 天调到 **3-4 天**（含 Tauri 模式 E2E 重建） | 第 1 节"切换成本估算"表 + 第 3.1 节方案 A 工作量 |
| **R2**（必改） | 方案 B 补子选项 | 追加 **B3（Rust + Slint + russh）** 和 **B4（Rust + egui + russh）** 子选项评估 | 第 3.2 节"方案 B" 末尾 |
| **R3**（必改） | 风险表追加条目 | 追加 **R4：已验证代码资产的实际测试覆盖率低于隐含假设**（事实级风险，触发 Follow-ups 补集成测试） | 第 5 节风险表 |
| **R4**（必改） | 回滚策略补软触发 | 回滚触发条件追加**软触发**：终端 PTY 高频输出卡顿 / SFTP 上传 100MB+ 文件 UI 冻结 > 3 秒 | 第 6 节回滚策略 |
| **R5**（必改） | 方案 C 否决理由重写 | 第 2 点从"ssh2 功能弱于 russh"改为 **"重写成本被低估，不是功能缺失"**——SOCKS5 两边对等（都要自实现），真实成本是 host key 交互模式 + 键盘认证 API 形状的重写 | 第 3.3 节方案 C 否决理由 + RALPLAN-DR Summary 对应表述 |
| **R6**（建议改） | 执行顺序调整 | 方案 A 路径顺序从 1→2→3→4→5 改为 **3→1→2→4→5**，并注释说明顺序调整理由（v3 已修订为工程美学表述，见 R8） | 第 3.1 节方案 A 路径 |

**核心结论未改变：** 仍推荐方案 A（继续 Tauri）。工作量从 8-12 天调整为 12-18 天后，A vs B vs C 的相对优劣比例从 1:7:3 变为约 1:5:2，A 仍是最低成本方案。v3 阶段 Electron 评分从 3 调到 4 后加权总分 39 → 41，仍远低于 Tauri 48.5，结论方向不变。

---

## 0. RALPLAN-DR Summary（最优先读这一段）

### Principles（决策原则）

1. **已验证的业务逻辑优先于框架美学** — russh 0.49 + russh-sftp 2 + 21 个 SSH/SFTP/Tunnel 命令是已通过编译且实际可用的资产；重写它们引入的回归风险高于框架本身的痛点。
2. **痛点要分类解决，不要混合** — `State<T>` TypeId 陷阱是 Tauri API 学习成本，IPC OOM 是设计错误（用了 `Vec<u8>`），`invokeBrowserPreview` 双实现是技术债残留——这三个问题没有一个是"框架选错了"造成的，切框架都不能自动解决任何一个。
3. **个人项目的核心约束是维护者续航，不是绝对性能** — 单人维护者（RedTei）对栈的熟悉度、生态在 Windows 11 MSVC/GNU toolchain 下的构建顺畅度，比"理论上 Qt 内存占用低 30%"重要 10 倍。
4. **重写成本要诚实量化** — "切框架"在文档里是 5 个字，在工程上是 2-4 个月的全职工作量，且伴随 6-12 个月的隐性回归 bug 期。
5. **当三个选项的痛点都能用低成本局部修复解决时，禁止进行框架级重写** — 这是奥卡姆剃刀，适用于本案例。

### Decision Drivers（真正驱动决策的因素，按权重排序）

1. **现有代码资产的可救度（权重 40%）**：21 个 Tauri 命令、Pinia store、xterm 集成、隧道/SFTP UI 都已存在且部分经过实际使用。重写意味着全部丢弃。**注（v2 修订）：** 实际可救率经 Architect 审查应表述为 **88-92%**——Playwright 测试套件（`tests/ui-smoke.mjs`、`tests/ui-extended.mjs` 强依赖 browser-preview 模式）+ `lib.rs:232-280` 源码级回归测试在方案 A 的 Option A 重构后必须删/替换，约 200 行测试代码不能直接保留。
2. **单人维护者的总拥有成本（权重 30%）**：用户是单人 + Windows 主开发 + 没有 macOS/Linux CI。任何切换都会让维护者同时学新栈 + 修旧 bug，双倍认知负荷。
3. **痛点是否触及框架本质（权重 30%）**：当前所有已知痛点（State 陷阱、IPC OOM、双实现分裂）都是**应用层设计问题**，不是 Tauri 框架无法支持的硬约束。换言之——继续用 Tauri 不会卡死任何路线。

### Viable Options（≥3 备选方案，含 bounded pros/cons）

| 方案 | Pros（已限定边界） | Cons（已限定边界） |
|---|---|---|
| **A. 继续 Tauri**（推荐） | **88-92% 代码可救**（含测试代码联动，见 Decision Driver 1 v2 注）；russh 是 Rust SSH 第一梯队；前端 Vue 生态成熟；包大小 ~10MB；Windows 已可构建 | State API 有学习陷阱（已踩过坑，已知）；IPC 需手改文件传输为流式（已规划）；Windows gnu/msvc toolchain 切换需脚本辅助（已存在 `scripts/tauri-env.ps1`） |
| **B. 切 Qt**（含 B1 PySide6 / B2 qmetaobject / B3 Slint / B4 egui，见第 3.2 节） | Qt 原生 UI 在低端机更流畅；signal/slot 是真双向绑定；单二进制部署（无 WebView）；B3/B4 保留 russh 后端 | 必须**重写全部前端**（Vue/Pinia/xterm 全废，Slint/egui 同样）；终端组件生态不成熟（qtermwidget Windows 构建差，Slint/egui 无成熟终端组件）；单人 3-4 个月全职；Windows 打包复杂度未必低于 Tauri |
| **C. 切 Electron**（保留 Vue） | 前端 Vue/Pinia/xterm **可 100% 复用**；Node.js ssh2 是成熟库；开发者熟悉 JS 生态 | 包大小膨胀到 150-200MB；内存占用从 ~80MB 涨到 ~300MB；**SSH 功能与 russh 完全对等**（SOCKS5/host key 交互两边都要自实现，见深度对比表第 91 行 v3 修订），真实代价是 host key 异步交互模式 + 键盘交互认证 API 形状差异带来 5-10 人天重写成本；切完之后**仍然要修 IPC OOM**（Electron 的 IPC 同样不适合传大 ArrayBuffer，必须改流式） |

**为什么方案 C 不推荐（明确否决理由）：** Electron 唯一的卖点是"前端可复用"，但用户的前端 Vue 代码已经能在 Tauri 里跑，**前端复用不是当前痛点**。而 Electron 引入的新代价（包大小膨胀 15-20 倍、内存膨胀 ~4 倍、host key 交互与键盘认证必须重写 5-10 人天、仍需修 IPC OOM）全是净损失。换言之，切 Electron 既不解决旧问题（IPC OOM、双实现分裂都还在），又引入新问题（体积、内存、SSH 交互层重写）。

**为什么方案 B 不推荐（明确否决理由）：** Qt 的核心代价是**前端全部重写**——这违反原则 1。同时 Qt（及 B3 Slint / B4 egui，见第 3.2 节）在 SSH 终端领域都没有 xterm.js 这种事实标准组件，UI 要从零做（qtermwidget 在 Windows 下的构建口碑并不好；Slint/egui 终端生态更不成熟）。即便用 Rust + qmetaobject / Slint / egui + russh 保留后端，前端从 Vue 切到 QML/Slint/egui 仍是 2-3 个月工作量，且 Pinia 的响应式生态在这些前端框架里没有等价物。

---

## 1. 现状基线

### 当前 Tauri 架构成熟度评估：**~70%**

| 维度 | 成熟度 | 依据 |
|---|---|---|
| SSH 会话管理（连接/写/resize/断开/host key 校验） | 95% | `src-tauri/src/ssh.rs` 的 `SshSessionManager` 已实现完整生命周期 + known_hosts 持久化 + 键盘交互认证 |
| SFTP 文件操作（list/mkdir/rename/remove/stat/read/write） | 90% | 7 个命令齐全，仅 `read_file`/`write_file` 用 String/Vec<u8> 是设计瑕疵 |
| 文件传输进度（upload/download with progress） | 60% | 功能跑通，但用 `Vec<u8>` 传全文是 P1 IPC OOM 风险（见 `ssh.rs:784-823`） |
| 隧道转发（local/remote/dynamic SOCKS5） | 85% | local + dynamic SOCKS5 已实现（含 active/error/status，join handle 持有，可停止）；**remote forward 是返回 Err 的桩函数（`ssh.rs:1135-1143`），未实现**（v3 新增披露，Critic Minor Finding 2）；85% 成熟度反映 local+dynamic 完成 + remote 桩函数占位 |
| Tauri 状态管理 | 50% | 双重 manage 临时方案（`lib.rs:161-166`）+ 源码级回归测试守护，Option A 重构未完成 |
| 前端 Vue/Pinia 工作台 | 85% | 主题/资产/会话/隧道/文件浏览/传输队列都串通了，浏览器预览双实现是唯一污染 |
| 浏览器预览后端（`invokeBrowserPreview`） | 20%（应被删除） | 100+ 行假后端逻辑留在 `backend.js:36-147`，是开发期遗留，应在桌面客户端发布前清理 |

### 切换成本估算

| 方案 | 工作量（人天） | 工作内容 |
|---|---|---|
| **A. 继续 Tauri + 修痛点** | **12-18 人天**（v2 修订，原 v1 估 8-12 天） | 完成 Option A 重构（2 天）+ **IPC OOM 改流式（4-5 天，含 transferQueue 前端状态机分块上传改造）** + **删 invokeBrowserPreview（1-2 天，含 Playwright 测试改写）** + Windows 构建脚本收尾（1 天）+ **测试（3-4 天，含 Tauri 模式 E2E 重建）** |
| B. 切 Qt（B1 PySide6 + paramiko / B2 qmetaobject / B3 Slint / B4 egui） | **60-90 人天**（B1） / 80-110 人天（B2） / **40-60 人天（B3 Slint，见第 3.2 节）** | 重写 21 个命令的 Python 等价（15-20 天，仅 B1；B2/B3/B4 保留 russh 后端则跳过）+ 重写全部前端（QML / Slint / egui）（25-40 天）+ 终端组件选型集成（10-15 天）+ Windows 打包（5-10 天）+ 测试与回归（5-10 天） |
| C. 切 Electron（Node.js + ssh2） | **25-40 人天**（v2 维持，但内部构成已修正） | 后端 Rust 命令翻译成 Node.js + ssh2（15-20 天）+ IPC 重接（3-5 天）+ **host key 交互确认 + 键盘交互认证 API 形状重写（5-10 天，原 v1 表述"ssh2 功能弱于 russh"不准确，R5 修订指出 SOCKS5 两边对等，真实成本是 host key/键盘认证 API 形状重写，见第 3.3 节 v2 修订；R9 进一步统一 v1 措辞）** + 打包（2-3 天）+ 测试（3-5 天） |

**关键结论（v2 修订）：** 方案 A 的成本是方案 B（B1）的约 1/5、方案 C 的约 1/2，且收益（修掉的痛点）完全相同。相比 v1 的 1:7:3 比例，v2 真实比例约为 **1:5:2**——A 仍是最低成本方案，结论方向不变。

---

## 2. 三方案深度对比表

评分 1-5（5 最优），仅评估对**本项目**（SSH/SFTP 桌面客户端 + 单人维护 + Windows 主开发）的实际影响。

| 维度 | Tauri（现状） | Qt (PySide6) | Electron | 评分理由 |
|---|---|---|---|---|
| 开发效率（已有代码基础） | **5** | 1 | 3 | Tauri 已 70% 完成；Qt 全废；Electron 仅前端可复用 |
| SSH 生态成熟度 | **5** | 4 | **4**（v3 修订，原 v1/v2 评 3 分） | **russh 第一梯队；paramiko 成熟但同步阻塞；ssh2 (Node) 在 SOCKS5/host key 交互上需自实现（与 russh 对等），但 host key 异步交互模式 + 键盘交互认证 API 形状差异带来 5-10 人天重写成本（见第 3.3 节）——R7 修订，原 v2 残留"ssh2 (Node) 功能相对弱（特别是动态 SOCKS5）"与 R5 修订矛盾，已纠正** |
| 终端组件（xterm.js 等价物） | **5** | 2 | 5 | xterm.js 在 Tauri/Electron 直接用；Qt 需 qtermwidget，Windows 构建差 |
| 包大小（安装包） | **5**（~10MB） | 4（~30-50MB） | 1（~150-200MB） | Tauri 用系统 WebView；Qt 单二进制但含 Qt 库；Electron 内嵌 Chromium |
| 运行时内存占用 | **4**（~80MB） | **5**（~50MB） | 1（~300MB+） | Qt 原生最优；Electron 最差；Tauri 中等 |
| 启动速度 | **4** | **5** | 2 | Qt 原生最快；Electron 最慢；Tauri 受 WebView 影响 |
| 跨平台一致性（视觉/行为） | 3 | **5** | 4 | Qt 自绘最一致；Tauri 受系统 WebView 差异影响（Windows Edge / macOS WKWebView） |
| 长期维护风险（5 年视角） | **2.5**（v2 修订，原 v1 评 3 分） | 4 | 3 | **三者都活着；但 Tauri 2.x State<T> 用 TypeId 索引 Arc 的类型擦除设计已造成 P0-1 PR 起因，需要源码级回归测试守护双 manage hack，是"需写源码扫描测试来守护框架特性"的高维护成本信号。Architect 反论 3 部分成立，下调 0.5 分。** Qt 2.x API 仍稳定演进；Tauri 2.x 在 2026 H2 仍有 dynamic plugin / IPC protocol 层的演进计划 |
| 社区活跃度 | 4 | **5** | **5** | Qt/Electron 都是老牌；Tauri 增长快但体量小 |
| 单人维护者技能匹配度 | **3-4**（已踩坑但已学到） | 2（Python/QML 新栈） | 4（JS 已会） | 用户已会 Vue/JS，但 Qt 是未知栈 |
| Windows 11 构建顺畅度（含 MSVC/GNU 切换） | 3（已用 `scripts/tauri-env.ps1` 解决） | 3 | **5** | Electron 在 Windows 构建最简单；Tauri 已踩过 dlltool 坑但有脚本；Qt 在 Windows 也不错但部署 PySide6 仍需注意 |
| **加权总分**（前 3 维度 ×2 权重） | **48.5**（v2 修订，原 v1 评 49 分） | **36** | **41**（v3 修订，原 v2 评 39 分） | Tauri 胜出（v3 Electron SSH 评分上调 1 分后总分 +2，仍远低于 Tauri 48.5） |

**结论（v3 修订）：** 在 11 个维度中，Tauri 在 4 个维度拿满分（v1 是 5 个，长期维护下调到 2.5 后满分维度减少 1），Qt 在 4 个维度拿满分（但都是项目次要维度），Electron 在 2 个维度拿满分（也都是次要维度）。**v3 修订后 Electron 加权总分从 39 升到 41（SSH 生态成熟度评分 3→4），仍显著低于 Tauri 48.5，结论方向不变。** Tauri 仍胜出。

---

## 3. 方案详细分析

### 方案 A：继续 Tauri（推荐）

**路径（v2 修订，顺序从 v1 的 1→2→3→4→5 调整为 3→1→2→4→5）：**

> **顺序调整理由（R6 → R8 v3 修订）：** Architect 张力 3 原文用"transferQueue 强依赖 invokeBrowserPreview"作为顺序调整的技术前提，**但此前提错误**——`transferQueue` 走 `services/backend.js` 的统一 `invokeBackend` 入口，在 Tauri runtime 下走 `window.__TAURI__?.core?.invoke`，**根本不走 invokeBrowserPreview**（后者只在浏览器预览模式下被调用）。v2 采纳了 Architect 的错误前提，v3 已纠正。
>
> **真正的顺序调整理由是工程美学，不是技术依赖：** 先做污染清理（Step 1 删 invokeBrowserPreview）再做核心重构（Step 2 Option A）和 IPC 改造（Step 3 IPC OOM），原因是**清理死代码先于核心改动可以减少认知干扰**——避免在重构 transferQueue 时同时面对 browser-preview 残留路径（即便不依赖，仍是认知污染）。这不是技术依赖（transferQueue 走 invokeBackend 不走 invokeBrowserPreview），是工程顺序优化。
>
> **承认：** v2 在 R6 自评里声称"理由引用了 Architect 张力 3 的具体分析"，但没独立核实 Architect 的技术断言就采纳，是真诚度问题。v3 已诚实修复。

1. **（原 v1 第 3 步）删除 `invokeBrowserPreview` 双实现**（架构债清理，**前置条件**）
   - `backend.js:36-147` 的 100+ 行假后端逻辑应整体删除
   - 浏览器预览改为"明确提示需要桌面客户端"，而非伪装后端
   - 副作用：`fallbackAssets`（`backend.js` 早期版本里的 8 个假主机）也要删，避免设计稿残留
   - **同步改写 Playwright 测试**（v2 新增）：`tests/ui-smoke.mjs:38` 的"已连接 · browser-preview" 断言 + `tests/ui-extended.mjs:49` 的 `[data-remote-path]` 为 0 断言必须删除或改为 Tauri 模式 E2E

2. **（原 v1 第 1 步）完成 Option A 重构**（`lib.rs:157-166` 注释里的 deadline 2026-06-26）
   - 把 `ssh.rs` 全部 21 个命令的 `State<'_, Arc<Mutex<SshSessionManager>>>` 参数改为 `State<'_, AppState>`
   - 删除 `app.manage(ssh_mgr.clone())` 双重 manage 止血行
   - **保留现有源码级回归测试（`lib.rs:232-280`）作为不变量守护**——但注意 Option A 完成后该测试守护的"双 manage 不变量"不再存在，需删/替换为新的"单 manage 不变量"测试（见 Follow-ups）

3. **（原 v1 第 2 步）修复 IPC OOM**（P1，最影响生产可用性的项）
   - `sftp_upload_with_progress`：把 `content: Vec<u8>` 参数改为前端走 `tauri::ipc::InvokeBody::Raw` 或改用文件路径 + 后端流式读取
   - `sftp_download_with_progress`：当前 `Result<Vec<u8>, String>` 必须改为后端写临时文件 + 前端通过 Tauri asset 协议或 fs 命令读取
   - 大文件（>50MB）绝对不能整块过 JSON IPC；100MB 文件目前可能膨胀到 ~1GB（base64 + JSON 转义 + V8 字符串）
   - **前端 transferQueue 状态机改造（v2 新增）**：`src/stores/workbench.js:301-315` 的 transferQueue 当前调用 `Array.from(buffer)` 把 Uint8Array 转 JS Number 数组（`workbench.js:316-317`），必须改为"分块读取 + 多次 IPC 上传 chunk"的状态机模式。这是结构性改动，不是改一行签名

4. **Windows 构建脚本收尾**
   - 当前 `scripts/tauri-env.ps1` 已存在，需确认在 GNU/MSVC toolchain 切换时稳定
   - WebView2 运行时依赖已在 `src-tauri/WebView2Loader.dll` 提供

5. **次要技术债**（P2，可延后）
   - SFTP 锁粒度过粗（`state.lock().await` 持有过久）
   - PTY emit 无批处理（高频输出时 IPC 抖动）
   - `metadata.modified()` Debug 打印残留
   - tunnel `select` 死循环（如果有）

**工作量（v2 修订）：12-18 人天**（v1 原 8-12 天，上调原因见第 1 节切换成本估算表 R1 项）
**风险：** LOW
**收益：** 解决全部已识别痛点；**保留 88-92% 代码**（v1 原 95%，下调原因见 Decision Driver 1 v2 注）；架构进入"健康基线"状态

---

### 方案 B：切 Qt

**子选项 B1：PySide6 + paramiko**（Python 路线）

- 前端：QML（Vue 代码全部作废）
- SSH 后端：paramiko（同步 API，需用 QThread 包装避免阻塞 UI）或 asyncssh（异步，但与 Qt 事件循环整合复杂）
- 终端：`qtermwidget`（KDE 项目，Windows 构建口碑不好）或自绘 QML 终端

**子选项 B2：Rust + qmetaobject + russh**（保留 Rust 后端路线）

- 前端：QML（Vue 代码仍然作废）
- SSH 后端：russh 保留（最大优势——但 qmetaobject 的成熟度低于 C++ Qt）
- 终端：同 B1

**子选项 B3：Rust + Slint + russh**（v2 新增，Architect 反论 2 要求评估）

- **前端：Slint 声明式 UI**（Vue 代码作废，但学习曲线比 QML 平缓——声明式语法、Rust 原生、2024 年后稳定、Tauri 生态友好）
- **SSH 后端：russh 保留**（B3 相对 B1/B2 的最大优势——保留已验证 SSH 资产 + 摆脱 WebView 依赖）
- **终端：Slint 缺乏成熟终端组件**，仍需自绘或包装（xterm.js 在 Slint 里没有等价物）
- **工作量：40-60 人天**（介于方案 A 的 12-18 天和 B1 的 60-90 天之间——后端保留省下重写 21 命令的 15-20 天，但前端 Slint 学习曲线 + 终端自绘仍占 40+ 天）
- **否决理由：**
  - 仍需**重写全部前端 Vue 代码**（违反原则 1）
  - Slint 终端生态不成熟，无法解决当前 xterm.js 的优势（违反原则 3——增加单人维护者认知负荷）
  - 工作量仍是方案 A 的 3-5 倍，性价比不成立
  - **但保留为未来逃生舱**：如果某天 WebView 成为性能瓶颈（如 xterm.js 在百万行 scrollback 下卡顿），B3 是比方案 C（Electron）更合理的"摆脱 WebView"路径——保留 russh 后端 + 摆脱系统 WebView 依赖

**子选项 B4：Rust + egui + russh**（v2 新增，Architect 反论 2 要求评估）

- **前端：egui（immediate mode GUI）**——对工具型 SSH 客户端（长时间挂机 + 大量文本输出）有性能优势
- **SSH 后端：russh 保留**
- **终端：egui 无成熟终端组件**，但 immediate mode 对终端这种"每帧重绘"场景天然友好，自绘成本可能低于 Slint
- **工作量：40-60 人天**（与 B3 相当）
- **否决理由：**
  - Vue/Pinia 的响应式生态在 egui 里**完全没有等价物**——前端是"从响应式倒退到 immediate mode"的范式重写
  - 单人维护者需要同时学 immediate mode 编程范式 + 自绘终端，认知爆炸（违反原则 3）
  - 同 B3，工作量仍是方案 A 的 3-5 倍

**推荐子选项：** B1（PySide6 + paramiko）— 因为 qmetaobject 生态不成熟，B2 反而引入新风险；B3/B4 虽保留 russh 后端但前端重写代价仍过高，且终端生态都不成熟。**若未来 WebView 成为瓶颈，B3（Slint）是最合理的逃生舱**（见第 4 节边界条件）。

**迁移路径：**

1. 重写 21 个 SSH/SFTP/Tunnel 命令的 Python 等价（paramiko 的 API 与 russh 差异大，特别是 host key 校验和键盘交互认证）
2. 重写全部前端 UI（Vue 组件 → QML，Pinia store → QObject 派生类）
3. 终端组件选型 + 集成（最不确定的一步，qtermwidget 在 Windows 可能要自己编）
4. Windows 打包（PyInstaller 或 Qt installer-framework）
5. 全量回归测试（21 命令 × 3 平台）

**工作量：** 60-90 人天（B1）；80-110 人天（B2）；**40-60 人天（B3 Slint / B4 egui，v2 新增）**
**风险：** HIGH（终端组件 + Windows 打包 + paramiko 异步包装都是新风险；B3/B4 终端生态不成熟风险更高）
**收益：** 内存占用降低 ~30MB（从 80MB → 50MB）；视觉一致性提升；B3/B4 额外收益是摆脱 WebView 依赖；其他维度无显著收益

**为什么不推荐：**

- 收益（省 30MB 内存、视觉一致性）对 SSH 客户端这种**长时间挂机型**应用几乎无感知
- 代价（前端全部重写、终端组件从零做）违反原则 1
- 单人维护者要同时学 QML + Qt 打包 + paramiko 异步模型，认知爆炸

---

### 方案 C：切 Electron

**路径：**

1. 后端：Rust 命令 → Node.js 主进程函数（ssh2 库）
2. 前端：Vue/Pinia/xterm **100% 复用**（这是唯一卖点）
3. IPC 重接：Electron `ipcMain.handle` 替换 Tauri `#[tauri::command]`
4. **host key 交互确认 + 键盘交互认证重写**（v2 修订，原 v1 表述"ssh2 功能弱于 russh"不准确——SOCKS5 两边对等，真实成本是 host key/键盘认证 API 形状重写；R9 v3 进一步统一 v1 措辞）
5. Windows 打包：electron-builder

**工作量：** 25-40 人天（v2 维持，但内部构成已修正——见切换成本估算表 R1 注）
**风险：** MEDIUM-HIGH（**host key 交互 + 键盘认证 API 重写风险** + 仍需修 IPC OOM）
**收益：** 前端复用（但前端本来就能用，不是痛点）；Node.js 生态熟悉度高

**为什么不推荐（v2 修订）：**

1. **IPC OOM 不会消失**：Electron 的 IPC 同样不适合传大 ArrayBuffer，方案 C 完成后仍要做方案 A 第 3 步的全部工作。换言之，切 Electron 是**净增工作量**（切栈 + 修同样的问题）。

2. **重写成本被低估，不是功能缺失（R5 修订，原 v1 表述"ssh2 功能弱于 russh"不准确）：** 项目当前的 SOCKS5 隧道是基于 russh channel **自实现的**（`ssh.rs:1183-1347` 手写握手 + `channel_open_direct_tcpip`），russh 本身不提供 SOCKS5 协议层，只提供 SSH 通道。**切到 ssh2 (Node.js) 后，SOCKS5 仍然要自己实现**（因为 ssh2 也只提供 channel API，SOCKS5 协议层是项目自写的）——两边在隧道功能上**功能对等**，原 v1 表述"ssh2 动态 SOCKS5 转发支持不完整"是错的。真正的成本在于：
   - **host key 交互确认**（`ssh.rs:54-110` 的 oneshot channel + emit 模式做的"前端确认 host key"交互）在 ssh2 的事件循环里要重写为不同的事件驱动模型
   - **键盘交互认证 loop**（`ssh.rs:297-339`）在 ssh2 里 API 形状不同，必须重写
   - 这些重写**新增 5-10 人天工作量**，且伴随回归风险（host key 确认是安全敏感路径，重写容易引入 MITM 风险）
   - **Architect 反论 4 已确认此修订**：结论方向对（C 不推荐），但否决理由从"功能缺失"改为"重写成本被低估"

3. **包大小膨胀 15-20 倍**（10MB → 200MB），对一个 SSH 客户端是不可接受的体感倒退。
4. **内存膨胀 ~4 倍**（80MB → 300MB+），长时间挂多会话时明显。

---

## 4. 决策推荐

### 单一明确推荐：**方案 A（继续 Tauri）**

**核心理由（≤3 条，v2 修订）：**

1. **当前所有痛点都不是框架本质问题，都能用 12-18 人天的局部修复解决**（v1 原 8-12 天）。切框架无法跳过这些修复（IPC OOM 在任何框架都要改流式），切框架只是把"修痛点"变成"切栈 + 修同样的痛点"。
2. **方案 A 保留 88-92% 已验证代码**（v1 原 95%，下调原因见 Decision Driver 1 v2 注）；方案 B 重写全部前端（违反"已验证逻辑优先"原则，含 B3/B4 子选项）；方案 C 即使复用前端，也在 SSH 交互层（host key + 键盘认证）上新增 5-10 人天重写成本。
3. **单人维护者的最大风险是认知超载**，方案 A 让维护者继续深耕已熟悉的栈，方案 B/C 强迫维护者同时学新栈 + 修旧 bug。

### 何时考虑方案 B 或 C（边界条件，给未来留口）

- **方案 B（含 B1/B2/B3/B4 子选项）的触发条件**：
  - **B1（PySide6）**：目标平台扩展到嵌入式 / ARM Linux + 要求 < 50MB 内存 + 团队扩充到 ≥2 人且新成员是 Qt 背景。当前不满足任何一条。
  - **B3（Slint）/ B4（egui）— v2 新增逃生舱**：**如果未来 WebView 成为性能瓶颈**（如 xterm.js 在百万行 scrollback 下出现明显卡顿、或 Windows WebView2 Runtime 版本碎片化导致行为不一致），B3（Slint）是最合理的"摆脱 WebView 但保留 russh 后端"路径。这是 v2 基于 Architect 反论 2 显式记录的逃生舱，**不影响今天决策**，但避免未来 reviewer 重新发明。
- **方案 C 的触发条件**：
  - 维护者彻底放弃 Rust 后端 + 目标用户群明确不在乎包大小（如内部企业工具）。当前不满足。
  - **v2 新增（Architect 张力 2 建议）：维护者技能栈从 Rust 主导迁移到 Node.js 主导**——如果未来 6-12 个月维护者对 Rust 的兴趣显著下降但仍想保留 Vue 前端，Electron + ssh2 成为合理的"分阶段重写"路径。当前不满足（维护者仍深耕 Rust）。

---

## 5. 风险与缓解（针对推荐方案 A）

| Top 风险 | 概率 | 影响 | 缓解措施 |
|---|---|---|---|
| **R1. Tauri 2.x 未来 API 破坏性变更**（如 State<T> 机制再改） | MEDIUM | MEDIUM | 锁定 Tauri 版本（Cargo.lock）；保留源码级回归测试（`lib.rs:232-280` 已有先例）；关注 Tauri changelog；**v2 新增 + v3 修订：每次 Tauri 版本升级预留 0-3 人天迁移成本（多数小版本 0 成本，最坏 3 人天，基于 State<T> TypeId 陷阱经验外推，见 Consequences 负面条目；v3 按 Critic Minor Finding 3 把下限从 1 调到 0）** |
| **R2. Windows 构建 toolchain 不稳定**（MSVC/GNU 切换、dlltool 路径含空格） | MEDIUM | HIGH | 维护 `scripts/tauri-env.ps1`；CI 加入 Windows 构建 smoke test；文档化 toolchain 选择决策 |
| **R3. xterm.js + Tauri WebView 在某些 Windows 旧版本上输入法/IME 行为异常** | LOW | MEDIUM | 已知问题但概率低；测试矩阵覆盖 Windows 10/11；如触发可考虑嵌入固定版本 WebView2 Runtime |
| **R4. 已验证代码资产的实际测试覆盖率低于隐含假设**（v2 新增，R3 修订项） | **HIGH（事实，非风险）** | MEDIUM | **现状：** russh 后端**零单元测试**（`ssh.rs` 无 `#[cfg(test)] mod tests`）；`tests/ui-smoke.mjs` 和 `tests/ui-extended.mjs` 只覆盖前端 browser-preview 模式，对 russh 后端零覆盖；`lib.rs:232-280` 只守护双 manage hack 不变量，不验证 SSH 功能。这意味着 v1 "95% 代码可救"的论据部分建立在"代码被运行测试覆盖过"的虚假前提上。**缓解措施：** 在 Follow-ups 增加 P2 任务"为 russh 后端补集成测试（mock SSH server，覆盖 connect/upload/download/tunnel 4 个命令；v3 R11 已补 mock 方案调研项）"，否则未来 SSH 协议层 bug（如 SOCKS5 边缘网络环境挂死）的回归成本会被严重低估 |

---

## 5.5 验收标准（v3 新增，R10 + R11）

**方案 A 成功的可测试标准（所有项必须满足，否则不算"方案 A 完成"）：**

- **AC-1（Step 2 Option A 重构完成）：** `src-tauri/src/ssh.rs` 中 grep `State<'_, Arc<Mutex<SshSessionManager>>>` 返回 0（全部迁移到 `State<'_, AppState>`）。对应 followup-ssh-state-unify.md 的 AC-FU-1。
- **AC-2（Step 2 Option A 重构完成，双 manage hack 清理）：** `src-tauri/src/lib.rs` 中 `app.manage(ssh_mgr.clone())` 行已删除（仅保留 `app.manage(AppState { ... })` 单 manage）。对应 followup-ssh-state-unify.md 的 AC-FU-2。
- **AC-3（Step 1 删 invokeBrowserPreview 完成）：** `src/services/backend.js` 中 grep `invokeBrowserPreview` 返回 0（包括 fallbackAssets 设计稿残留也一并删除）。
- **AC-4（Step 3 IPC OOM 改流式完成）：** 上传 500MB 文件，前端堆内存峰值 < 200MB（证明 transferQueue 已改为分块流式上传，不再走 `Array.from(buffer)` 转 JS Number 数组的旧路径）。**此条同时作为 R11 IPC OOM 判定标准**——500MB 文件 < 200MB 堆意味着 JSON IPC 通道未承载数据本体，改流式成功。
- **AC-5（Step 1 测试改写完成）：** `tests/ui-smoke.mjs` 和 `tests/ui-extended.mjs` 改写后能在 Tauri runtime 模式下通过（即从 browser-preview 模式改为 Tauri driver / WebView2 自动化模式）。
- **AC-6（R11 新增，对应 R4 缓解任务）：** mock SSH server 测试覆盖 `connect` / `upload` / `download` / `tunnel` 4 个命令（mock 方案在执行阶段从 rust-ssh-test-server / docker openssh-server / russh mock handler 三者中二选一，见 Follow-ups R4 项）。

**验收标准的执行意义：** 这 6 条标准让 executor 在落地 follow-up 时有客观"完成"判定，避免"方案 A 12-18 天"估算因完成定义模糊而走样。每条都对应 grep 命令或运行时指标，可直接进入 CI 校验。

---

## 6. 回滚策略

**如果方案 A 在执行过程中失败（例如 Option A 重构踩到无法预见的 Tauri 内部限制）：**

1. **短期回滚（< 1 天）**：`git revert` 回到当前双重 manage 的稳定状态（Option B 已验证可用，有源码级回归测试守护）。
2. **中期备选（1-2 周）**：放弃 Option A，接受双重 manage 作为永久方案（性能损失可忽略，仅代码风格不优雅）。重点改为修 IPC OOM + 清理 invokeBrowserPreview。
3. **长期逃生舱（仅在最坏情况）**：如果 Tauri 在某个核心场景（如终端性能）确实无法满足，**才**评估方案 C（Electron）作为降级路径——保留 Vue 前端的复用价值。**永远不要把方案 B（Qt/B1/B2）作为应急逃生舱**，因为重写前端的工作量让它无法应急。**（v2 新增）** 若触发原因是 WebView 性能瓶颈而非 SSH 后端问题，B3（Slint）保留 russh 后端是比方案 C 更合理的逃生舱。

**回滚触发条件（v2 追加软触发；v3 修订消除 Ambiguity 2 AND/OR 歧义）：**

**回滚触发逻辑（v3 明确为 OR 关系，非 AND）：**

- **路径 A — 立即回滚（硬触发 OR 软触发持续超时，两者任一即触发）：**
  - **硬触发（客观指标倒退，任一发生立即回滚）：**
    - Option A 重构后回归测试在 Windows 上失败超过 3 天无法修复
    - IPC OOM 改流式后传输性能反而劣化到无法接受（< 5MB/s）
  - **软触发持续超时（主观体验倒退，单条持续 > 2 周无解则触发回滚评估，可继续到回滚）：**
    - 终端 PTY 在高频输出（如 `cat 大文件` / `yes` 命令）下出现可感知的卡顿或丢字符
    - SFTP 上传 100MB+ 文件时 UI 完全冻结超过 3 秒
- **路径 B — 逃生舱评估（软触发单独持续 > 2 周无解）：**
  - 任一软触发单独持续超过 2 周无法通过局部优化解决 → 触发**逃生舱评估**（不立即回滚，但启动 B3/Electron 方案调研）

**v3 明确化（消除 Critic Ambiguity 2）：** 原 v2 表述"任一软触发 + 上方任一硬触发条件满足 → 触发回滚"字面读是 AND（两者同时发生），与下文"软触发单独 > 2 周启动评估"的 OR 语义冲突。**v3 改为：硬触发单独即回滚，软触发单独持续 > 2 周启动逃生舱评估——两者都是 OR（任一满足即触发），不存在 AND 关系。**

---

## 7. ADR 元数据

```yaml
Decision: 继续使用 Tauri 2.x 作为桌面框架，不切换到 Qt 或 Electron
Drivers:
  - 88-92% 已验证代码可救（21 个 SSH/SFTP/Tunnel 命令 + 完整 Vue 前端，v2 修订，原 v1 表述 95% 高估了测试代码的保留率）
  - 当前痛点（State 陷阱、IPC OOM、双实现分裂）均为应用层问题，与框架无关
  - 单人维护者认知负荷最小化
Alternatives_considered:
  - Qt (B1 PySide6 + paramiko)：前端全废 + 终端组件不确定 + 60-90 人天
  - Qt (B2 Rust + qmetaobject + russh)：qmetaobject 生态不成熟，不推荐
  - Qt (B3 Rust + Slint + russh)：v2 新增，保留 russh 后端 + 摆脱 WebView，但前端仍需全废 + Slint 终端生态不成熟 + 40-60 人天；保留为"WebView 成为瓶颈时"的逃生舱
  - Qt (B4 Rust + egui + russh)：v2 新增，immediate mode 对终端性能友好，但响应式生态缺失 + 前端范式重写 + 40-60 人天
  - Electron (Node.js + ssh2)：前端可复用但 host key 交互 + 键盘认证需重写 5-10 人天（v2 修订，原 v1 表述"ssh2 功能弱于 russh"不准确）+ 包大小膨胀 15 倍 + 仍需修 IPC OOM
Why_chosen:
  - 方案 A 工作量（12-18 天，v2 修订）是 B 的约 1/5、C 的约 1/2，且修掉的痛点完全相同
  - 唯一保留 russh 第一梯队 SSH 能力的方案
  - 唯一不引入新栈学习成本的方案
Consequences:
  正面:
    - 88-92% 代码资产保留（v2 修订，原 95%）
    - 12-18 人天内可达"健康基线"（v2 修订，原 8-12 天）
    - 单人维护者继续深耕熟悉栈
  负面:
    - Tauri 2.x API 仍可能演进，需持续跟进
    - **Tauri 版本升级每次预估 0-3 人天迁移成本（v2 新增 + v3 修订）**——基于已踩到的 State<T> TypeId 陷阱（需源码级回归测试守护双 manage hack），未来 dynamic plugin / IPC protocol 层演进可能引入类似量级的迁移工作；v3 按 Critic Minor Finding 3 把下限从 1 改为 0（多数小版本升级可能 0 成本，统计学上更严谨）
    - Windows 构建 toolchain 维护成本持续存在
    - 视觉跨平台一致性受系统 WebView 影响（次要点）
    - **已验证代码资产的实际测试覆盖率低于隐含假设（v2 新增，见风险表 R4）**——russh 后端零单元测试，"已验证"论据部分建立在虚假前提上
Follow_ups:
  - 完成 Option A 重构（deadline 2026-06-26，见 .omc/plans/followup-ssh-state-unify.md）
  - 修复 IPC OOM（sftp_upload/download 改流式，含 transferQueue 前端状态机改造）
  - 删除 invokeBrowserPreview 双实现 + fallbackAssets 设计稿残留 + Playwright 测试改写（ui-smoke.mjs / ui-extended.mjs）
  - Windows 构建脚本（scripts/tauri-env.ps1）补充 CI smoke test
  - **为 russh 后端补集成测试（mock SSH server，覆盖 connect/upload/download/tunnel 4 个命令）（v2 新增，R3 修订项；v3 R11 补 mock 方案选型）**——缓解风险 R4，让"已验证代码"论据名副其实。**调研可选 mock 方案（v3 R11 新增）：[rust-ssh-test-server / docker openssh-server / russh mock handler]，执行阶段二选一—— russh mock handler 最轻量但需自写 protocol 层桩；docker openssh-server 最贴近真实但需 CI 环境支持 Docker；rust-ssh-test-server 若存在成熟 crate 则是折中**
  - **何时考虑方案 C 触发条件：维护者技能栈从 Rust 主导迁移到 Node.js 主导（v2 新增，Architect 张力 2 建议）**——显式记录此触发条件，避免未来 ADR reviewer 重新发明
  - 重新评估时机：2026-Q4（完成上述 follow-ups 后），若无新框架痛点则本 ADR 关闭
```

---

## 8. 待 Critic 三审的关键判断（v3 修订，Architect + Critic 已回应）

以下判断在 v1 阶段独立于架构师先前结论，需要 Critic 专门挑战。**Architect 已在 `ralplan-architect-review-framework-choice.md` 第 4 节逐条回应，v2 已据此修订；Critic 在 `ralplan-critic-review-framework-choice.md` 二审，v3 已据此修订（R7-R11 + 3 项附加修订）。Critic 三审重点检查 v3 修订是否真诚落地：**

1. ~~**"方案 A 工作量 8-12 人天"** 是否过于乐观？特别是 IPC OOM 改流式是否低估了前端联调成本？~~
   - **Architect 回应（ITERATE）：是，应改为 12-18 天。** IPC OOM 改流式漏算 transferQueue 前端状态机改造（+2 天），测试漏算 Playwright 改写 + Tauri E2E 重建（+1-2 天）。
   - **v2 落地位置：** 第 1 节切换成本估算表 + 第 3.1 节方案 A 工作量 + 第 7 节 Consequences。
   - **Critic 二审结论：** PASS（R1 真诚落地）。

2. ~~**"95% 代码可救"** 是否忽略了测试代码（`tests/ui-smoke.mjs`、`tests/ui-extended.mjs`、`lib.rs` 内的源码级测试）也要相应更新？~~
   - **Architect 回应（ITERATE）：是，且影响量化。** 真实保留率应为 88-92%（Playwright 测试 + lib.rs 源码级回归测试要重写约 200 行）。
   - **v2 落地位置：** Decision Driver 1 v2 注 + Viable Options 表方案 A + 第 3.1 节收益 + 第 7 节 Drivers/Consequences。
   - **Critic 二审结论：** PASS（全文 95% 替换干净，第 64 行 95% 是 SSH 会话管理成熟度评分属不同语义维度）。

3. ~~**"切 Electron 后 ssh2 功能弱于 russh"** 是否准确？ssh2 在 2026 年的隧道/键盘交互认证支持是否已有改善？需要外部文档核实。~~
   - **Architect 回应（ITERATE）：表述不准确，但结论方向对。** SOCKS5 两边对等（都要自实现，见 `ssh.rs:1183-1347` 自实现证据），真实成本是 host key 交互模式（`ssh.rs:54-110`）+ 键盘认证 API 形状（`ssh.rs:297-339`）的重写，约 5-10 人天。
   - **v2 落地位置：** 第 3.3 节方案 C "为什么不推荐" 第 2 点 + Viable Options 表方案 C Cons + RALPLAN-DR Summary 否决理由 + 第 7 节 Alternatives_considered。
   - **Critic 二审结论：FAIL → v3 R7 + R9 修复。** Critical Finding 1 暴露第 91 行深度对比表残留 v1 错误论据与 R5 修订矛盾；Major Finding 2 暴露 v1 措辞被 v2 在 3 处用 3 种方式重写属轻度自评美化。**v3 已修复：** R7 修订第 91 行评分理由 + Electron 评分 3→4 + 加权总分 39→41；R9 统一 v1 措辞为"ssh2 功能弱于 russh"（严格忠实 Architect 原文）。

4. ~~**回滚触发条件**（Option A 重构失败 > 3 天 / 传输 < 5MB/s）的阈值是否合理？~~
   - **Architect 回应（ITERATE）：硬触发合理，但缺软触发。** 必须补"终端 PTY 高频输出卡顿 / SFTP 上传 100MB+ UI 冻结 > 3 秒"等软触发条件。
   - **v2 落地位置：** 第 6 节回滚策略追加软触发段。
   - **Critic 二审结论：** v2 软触发可观测但 AND/OR 表述有歧义（Ambiguity 2）→ v3 附加修订 2 已明确为 OR 关系。

**v3 新增待 Critic 三审的关键判断（来自 Critic 二审新发现）：**

5. **深度对比表内部一致性（R7）：** v3 已修订第 91 行评分理由 + Electron 评分 3→4 + 加权总分 39→41，与 R5 修订完全一致。**Critic 三审核查点：** 全文 grep "ssh2" 不应再有"功能弱"表述（除显式标注"原 v1 表述"位置）。

6. **R6 顺序调整理由真诚度（R8）：** v3 已诚实承认 v2 采纳了 Architect 错误的技术断言，重写为工程美学表述。**Critic 三审核查点：** 第 3.1 节"顺序调整理由"段不应再出现"强依赖 invokeBrowserPreview"措辞。

7. **v1 措辞忠实度（R9）：** v3 已全文统一 v1 措辞为"ssh2 功能弱于 russh"。**Critic 三审核查点：** 全文 grep "SOCKS5 功能缺失" / "SOCKS5 功能查漏补缺"应返回 0（v2 创造的两个 v1 没有的措辞应已删除）。

8. **验收标准可测试性（R10）：** v3 新增第 5.5 节 6 条 AC，每条都对应 grep 命令或运行时指标。**Critic 三审核查点：** AC-1 到 AC-6 都应该是可执行的可观测指标，不应有抽象描述。

这些点已记入 `.omc/plans/open-questions.md`，v3 阶段 Architect + Critic 已全部回应，等待 Critic 三审确认。

---

## 9. Architect Review Resolution（v2 新增；v3 修订 R6 自评）

本节列出 Architect 在 `ralplan-architect-review-framework-choice.md` 提出的 6 项修订（R1-R6）在 v2 中的具体解决位置，供 Critic 二审时逐条核对：

| 修订编号 | 修订类别 | Architect 要求 | v2 解决位置 | 真诚度自评 |
|---|---|---|---|---|
| **R1**（必改） | 工作量上调 8-12 → 12-18 天 | IPC OOM 改流式 2-3 → 4-5 天（含 transferQueue 前端状态机）；删 invokeBrowserPreview 0.5 → 1-2 天（含 Playwright 改写）；测试 2-3 → 3-4 天（含 Tauri E2E 重建） | (1) 第 1 节切换成本估算表（A 行）；(2) 第 3.1 节方案 A 工作量从 8-12 改为 12-18；(3) 第 3.1 节 Step 3 追加"前端 transferQueue 状态机改造"说明；(4) 第 3.1 节 Step 1 追加"同步改写 Playwright 测试"说明；(5) 第 7 节 Consequences 正面 8-12 改为 12-18 | **真诚落地。** 全文 8-12 已全部改为 12-18，IPC OOM 细项分解也已展开说明 |
| **R2**（必改） | 方案 B 补 B3/B4 子选项 | B3 Rust+Slint+russh（保留后端 + 摆脱 WebView）；B4 Rust+egui+russh（immediate mode） | (1) 第 3.2 节"方案 B"追加 B3 和 B4 完整评估；(2) Viable Options 表方案 B pros/cons 更新引用 B1-B4；(3) 第 3.2 节"为什么不推荐"补 B3/B4；(4) 第 4 节边界条件追加 B3 作为"WebView 瓶颈时"逃生舱；(5) 第 7 节 Alternatives_considered 列出 B3/B4 | **基本真诚落地，轻度水分。** B3/B4 评估了并给出否决理由和逃生舱条件，但工作量估算同质化（两者都给 40-60 天，未体现 Slint 声明式 vs egui immediate mode 范式差异）——Critic Minor Finding 4 已指出，v3 未修订（不阻塞批准，但已知水分） |
| **R3**（必改） | 风险表追加 R4 | 已验证代码资产的实际测试覆盖率低于隐含假设（HIGH 概率事实，MEDIUM 影响） | (1) 第 5 节风险表追加 R4 完整条目；(2) 第 7 节 Follow-ups 追加 P2 任务"为 russh 后端补集成测试（mock SSH server）"；(3) 第 7 节 Consequences 负面追加"已验证代码资产的实际测试覆盖率低于隐含假设" | **真诚落地。** R4 标注为"事实级风险"（概率 HIGH），并在 Follow-ups 给出可执行的缓解任务。**v3 R11 补 mock 方案调研项**（rust-ssh-test-server / docker openssh-server / russh mock handler 三选一），消除 v2 mock 方案未指定的轻度水分 |
| **R4**（必改） | 回滚策略补软触发 | 终端 PTY 卡顿 / SFTP UI 冻结等主观体验倒退 | (1) 第 6 节回滚触发条件追加"软触发（主观体验倒退）"段；(2) v2 原表述"软触发 + 硬触发任一满足 → 回滚"字面是 AND 语义与下文 OR 冲突，**v3 R8（Critic Ambiguity 2）已修订为明确 OR 关系** | **v2 基本真诚落地，但 AND/OR 表述有歧义；v3 已修订。** 软触发条件具体到"cat 大文件 / yes 命令"和"100MB+ 文件冻结 > 3 秒"等可观测场景；v3 进一步明确硬触发单独即回滚、软触发单独持续 > 2 周启动逃生舱评估 |
| **R5**（必改） | 方案 C 否决理由重写 | 第 2 点从"ssh2 功能弱于 russh"改为"重写成本被低估，不是功能缺失"——SOCKS5 两边对等，真实成本是 host key 交互 + 键盘认证 API 重写 | (1) 第 3.3 节"为什么不推荐" 第 2 点整段重写；(2) 第 3.3 节路径 Step 4 从"SOCKS5 查漏补缺"改为"host key + 键盘认证重写"；(3) Viable Options 表方案 C Cons 表述更新；(4) RALPLAN-DR Summary 否决理由表述更新；(5) 第 7 节 Alternatives_considered 方案 C 描述更新 | **v2 部分应付——这是 MAJOR 问题，v3 已修复。** v2 自评声称"全文'ssh2 功能弱'表述已全部替换"，但 Critic 核查发现：(a) 第 91 行深度对比表残留 v1 错误表述（Critical Finding 1）；(b) v1 原文措辞被 v2 在 3 处用 3 种不同方式重写（Major Finding 2：第 78 行"SOCKS5 功能缺失"、第 214 行"SOCKS5 功能查漏补缺"、第 303 行"ssh2 功能弱于 russh"），创造 v1 没有的措辞属轻度自评美化。**v3 已修复：** R7 修订第 91 行（评分理由重写 + Electron 评分 3→4 + 加权总分 39→41）；R9 统一 v1 措辞为"ssh2 功能弱于 russh"（严格忠实 Architect 原文）。 |
| **R6**（建议改） | 执行顺序 1→2→3→4→5 → 3→1→2→4→5 | Step 3（删 browser preview）是 Step 2（IPC OOM）的前置条件 | (1) 第 3.1 节方案 A 路径顺序整体调整；(2) 追加"顺序调整理由"段；(3) 每步标注"（原 v1 第 X 步）"保留追溯性 | **v2 顺序调整本身落地，但顺序调整理由引用了 Architect 错误的技术断言——这是 MAJOR 问题，v3 已修复。** Critic Major Finding 1 指出：Architect 张力 3 原文称"transferQueue 强依赖 invokeBrowserPreview"是错的——transferQueue 走 `services/backend.js` 的 `invokeBackend` 入口，在 Tauri runtime 下走 `window.__TAURI__?.core?.invoke`，**不走 invokeBrowserPreview**。v2 没独立核实就采纳了错误前提。**v3 R8 已重写顺序调整理由**，明确承认 transferQueue 与 invokeBrowserPreview 无技术依赖，顺序调整是工程美学考虑（清理污染先于核心重构）。 |

**v3 自评结论（替代 v2 自评）：** v2 自评"6 项修订全部真诚落地"**不准确**。实际是 3 项真诚落地（R1/R3 + R4 主体）、1 项基本真诚但轻度水分（R2 B3/B4 工作量同质化）、2 项有 MAJOR 真诚度问题（R5 自评美化 + 深度对比表残留 / R6 引用错误前提）。v3 已针对 R5/R6 的问题真诚修复（详见下方第 9.5 节 Critic Review Resolution）。Critic 二审升级到 ADVERSARIAL 模式是正确的判断，v2 的应付成分已被诚实承认并修复。

---

## 9.5. Critic Review Resolution（v3 新增）

本节列出 Critic 在 `ralplan-critic-review-framework-choice.md` 提出的 5 项修订（R7-R11）+ 3 项附加修订（Minor Findings + Ambiguity）在 v3 中的具体解决位置，供 Critic 三审时逐条核对：

| 修订编号 | 修订类别 | Critic 要求 | v3 解决位置 | 真诚度自评 |
|---|---|---|---|---|
| **R7**（必改，CRITICAL） | 深度对比表残留矛盾修复 | 第 91 行 SSH 生态成熟度评分理由从 v1 残留"ssh2 功能相对弱"改为与 R5 一致的表述；Electron 该行评分 3 → 4；加权总分 39 → 41；同步更新 RALPLAN-DR Summary 方案 C Cons | (1) 第 2 节深度对比表第 91 行评分理由整段重写为"ssh2 (Node) 在 SOCKS5/host key 交互上需自实现（与 russh 对等），但 host key 异步交互模式 + 键盘交互认证 API 形状差异带来 5-10 人天重写成本"；(2) Electron 该行评分 3 → 4；(3) 加权总分行 39 → 41 + 注释；(4) RALPLAN-DR Summary Viable Options 方案 C Cons 描述更新为"SSH 功能与 russh 完全对等"；(5) 第 0.5 节 Revision History 追加 R7 修订项 | **真诚落地。** 评分理由整段重写（不是表面修订），评分调整（3→4）和加权总分调整（39→41）都明确标注，与 R5 修订完全一致 |
| **R8**（建议改，MAJOR） | R6 顺序调整理由重写 | 第 113 行承认 transferQueue 走 invokeBackend 不走 invokeBrowserPreview，顺序调整是工程美学而非技术依赖 | (1) 第 3.1 节方案 A 路径"顺序调整理由"段整段重写；(2) 明确承认 v2 采纳了 Architect 错误前提（"Architect 张力 3 原文用'transferQueue 强依赖 invokeBrowserPreview'作为顺序调整的技术前提，但此前提错误"）；(3) 重写为工程美学表述（"清理死代码先于核心改动可以减少认知干扰"） | **真诚落地。** 不只是改理由措辞，而是明确承认 v2 的真诚度问题（"v2 在 R6 自评里声称理由引用了 Architect 张力 3 的具体分析，但没独立核实 Architect 的技术断言就采纳，是真诚度问题"） |
| **R9**（建议改，MAJOR） | v1 措辞统一 | 第 78 行和第 214 行的"SOCKS5 功能缺失"和"SOCKS5 功能查漏补缺"统一改为 "ssh2 功能弱于 russh"（与 Architect R5 原文一致） | (1) 第 1 节切换成本估算表 C 行注："原 v1 误记为'SOCKS5 功能缺失'" → "原 v1 表述'ssh2 功能弱于 russh'不准确"；(2) 第 3.3 节方案 C 路径 Step 4："原 v1 误记为'SOCKS5 功能查漏补缺'" → "原 v1 表述'ssh2 功能弱于 russh'不准确"；(3) 第 303 行已是"ssh2 功能弱于 russh"无需修改 | **真诚落地。** 全文 v1 措辞已统一为"ssh2 功能弱于 russh"，不再创造 v1 没有的措辞。**这是对 v2 自评美化问题的真诚修复**——v2 在 3 处用 3 种措辞描述 v1 原文，v3 严格忠实 Architect R5 原文 |
| **R10**（建议改，MAJOR） | 补验收标准小节 | 新增 AC-1 到 AC-5 可测试验收标准（grep `State<'_, Arc<Mutex<SshSessionManager>>>` = 0 / `app.manage(ssh_mgr.clone())` 已删除 / grep `invokeBrowserPreview` = 0 / 500MB 文件上传堆内存 < 200MB / Tauri 模式 E2E 通过） | 新增第 5.5 节"验收标准"，列出 AC-1 到 AC-6（AC-6 由 R11 追加），每条都对应 grep 命令或运行时指标，可直接进入 CI 校验 | **真诚落地。** 验收标准不是抽象描述，而是可执行的可观测指标（grep 命令返回 0、堆内存 < 200MB 等），且每条都对应到具体 Step |
| **R11**（建议改，PARTIAL） | mock SSH server 方案 + IPC OOM 判定标准 | R4 缓解任务追加 mock 方案调研项（rust-ssh-test-server / docker openssh / russh mock handler）；IPC OOM 判定标准定义 | (1) 第 5 节风险表 R4 缓解措施追加"v3 R11 已补 mock 方案调研项"；(2) 第 7 节 Follow-ups R4 项追加"调研可选 mock 方案：[rust-ssh-test-server / docker openssh-server / russh mock handler]，执行阶段二选一"；(3) 新增 AC-6："mock SSH server 测试覆盖 connect/upload/download/tunnel 4 个命令"；(4) AC-4 已含 IPC OOM 判定标准（500MB 文件 < 200MB 堆） | **真诚落地。** mock 方案 3 个候选都给出并比较（russh mock handler 最轻量 / docker 最贴近真实 / rust-ssh-test-server 折中），不是空泛的"调研" |
| **附加 1**（Minor Finding 2） | 现状基线 remote forward 桩函数披露 | 第 1 节"隧道转发 85%"补注 remote forward 是桩函数 | 第 1 节现状基线表隧道转发行补注："local + dynamic SOCKS5 已实现（含 active/error/status，join handle 持有，可停止）；remote forward 是返回 Err 的桩函数（`ssh.rs:1135-1143`），未实现；85% 成熟度反映 local+dynamic 完成 + remote 桩函数占位" | **真诚落地。** 桩函数事实已诚实披露，不再让 85% 评分建立在"三种 kind 都实现"的不实前提上 |
| **附加 2**（Ambiguity 2） | 软触发 + 硬触发关系明确 | 第 6 节回滚触发条件明确 OR 关系（非 AND） | 第 6 节回滚触发条件整段重写为"路径 A 立即回滚（硬触发 OR 软触发持续超时）"+"路径 B 逃生舱评估（软触发持续 > 2 周）"，并追加"v3 明确化"段消除 AND/OR 歧义 | **真诚落地。** 路径 A 和路径 B 触发逻辑明确分离，不再有"任一软触发 + 上方任一硬触发"的字面 AND 歧义 |
| **附加 3**（Minor Finding 3） | Tauri 升级 buffer 下限校正 | "1-3 人天"改为"0-3 人天"（多数小版本 0 成本，最坏 3 人天） | (1) 第 5 节风险表 R1 缓解措施："1-3 人天" → "0-3 人天"；(2) 第 7 节 Consequences 负面 Tauri 升级条目同步修订 | **真诚落地。** 下限从 1 改为 0，统计学更严谨（多数小版本升级 0 成本） |

**v3 自评结论：** Critic R7-R11（R7 必改 + R8-R11 建议改）+ 3 项附加修订**全部真诚落地**，无表面应付。关键修复点：

1. **R7（CRITICAL）：** 深度对比表与 R5 修订的内部矛盾已彻底消除，Electron 评分从 3 调到 4，加权总分从 39 升到 41，结论方向不变但 ADR 内部一致性恢复。
2. **R8（MAJOR）：** 诚实承认 v2 采纳了 Architect 错误的技术断言，重写顺序调整理由为工程美学表述，不再用错误前提支撑结论。
3. **R9（MAJOR）：** v1 措辞统一为"ssh2 功能弱于 russh"，严格忠实 Architect R5 原文，**不再创造 v1 没有的措辞**——这是对 v2 自评美化问题的真诚修复。
4. **R10（MAJOR）：** 新增 6 条可测试验收标准，让 executor 有客观"完成"判定，避免 12-18 天估算因完成定义模糊而走样。
5. **R11（PARTIAL）：** mock SSH server 3 候选方案都给出并比较，IPC OOM 判定标准含在 AC-4。
6. **3 项附加修订：** remote forward 桩函数披露 + AND/OR 歧义消除 + Tauri 升级 buffer 下限校正，全部落地。

**v3 已知残留水分（不阻塞批准）：**
- R2/B3-B4 工作量同质化（Minor Finding 4，v3 未修订）——Slint 和 egui 工作量都给 40-60 天未体现范式差异。此项不阻塞 Critic APPROVE（Critic 已明确归类为 Minor），但承认是已知水分。
- 第 1 节"~70% 成熟度"是 7 个维度算术平均还是加权（Ambiguity 1，v3 未修订）——此项不阻塞批准，但建议未来 reviewer 关注。

**等待 Critic 三审。若 R7-R11 + 3 项附加修订均通过核查，可转为 `Status: approved (v3)`。**

---

**文档结束。等待 Critic 三审通过后转为 `Status: approved (v3)`。**
