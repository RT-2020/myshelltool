# LLM 工程质量指南（myshelltool）

> 本文件是 **AI Coding Agent 的工程质量红线与最佳实践**，针对本项目反复出现的工程问题归纳而成。
> 与 [`AGENTS.md`](../AGENTS.md) 的关系：AGENTS.md 是「项目是什么 + 怎么跑」，本文件是「代码怎么写才不烂」。
> **硬约束用 ✅ / ❌ 标注，违反必须整改。** 软建议用 💡 标注。

---

## 0. 问题总览（为什么要这份文件）

AI 协作开发若缺少语言级最佳实践约束，会持续产出以下 7 类问题（均在本项目有实证）：

| # | 问题类别 | 一句话 | 本项目实证 |
|---|---|---|---|
| 1 | **重复造轮子 (ai-slop)** | 已有实现不复用，另写一份 | `terminalController.js` 整文件死代码；tag-split 正则 ×3；`.dot` CSS ×4 |
| 2 | **文件过大 / God File** | 单文件塞多个不相关职责 | `ssh.rs` 1548 行；`sessions.js` 810 行；`GlobalModals.vue` 512 行 |
| 3 | **扩展性差 / 高耦合** | 加一个功能要改 N 处且无编译保障 | 新增 modal 要改 4 处；新增命令要改 3 处；workbench re-export ~90 符号手动镜像 |
| 4 | **架构缺失** | 缺少分层抽象，逻辑散落 | 无 service 层（57 处 invokeBackend 散落）；错误处理 3 种风格混用 |
| 5 | **不一致 / 漂移** | 同一概念多份实现渐行渐远 | 两套 status 词汇（normalizeStatus vs ConnectionStatusPill） |
| 6 | **绕过自有抽象** | 有现成系统却用更原始的方式 | 有 GlobalModals 却用 `window.alert/confirm`；有设计 token 却硬编码 |
| 7 | **靠猜测代替事实** | 对外部环境（远端/工具/时钟/就绪）臆断而非求证 | `/home/<username>` 猜家目录；`find -printf` 假设 GNU；本地 mtime 对比 GitHub 时钟 |

下文逐类给出：**反模式（实证）→ 根因 → 最佳实践 → 可执行规则**。

---

## 1. 重复造轮子（ai-slop / Duplicate Code）

### 反模式实证
- **死代码 + 重复实现**：`src/lib/terminalController.js`（整个类）封装了 WebLinksAddon/WebglAddon/ResizeObserver/幂等销毁，但 `src/stores/sessions.js` 从未 import 它，而是在 L682-694、L311-340 内联重写了同样的生命周期逻辑。
- **同一正则 ×3**：tag 分割正则 `/[·,，\s]+/` 出现在 `backend.js:41`（normalizeAsset 内）、`GlobalModals.vue` 的 `splitTags`（约 L167）、`OpsSummaryPanel.vue`（第三份内联）。
- **`.dot` 样式 ×4 且漂移**：`ConnectionSidebar.vue`、`AssetGroupNode.vue`（8px）、`AppStatusBar.vue`（6px，无阴影）、`ConnectionStatusPill.vue`（不同状态名 + pulse 动画）——同一视觉元素四份实现，尺寸/颜色/行为已不一致。
- **`.icon-btn` 样式 ×3**：`ConnectionSidebar.vue`、`GlobalModals.vue`、`FileColumn.vue` 各定义一份。

### 根因
- 写新代码前**没有先搜索现有实现**。
- 重复的实现各自演进，修复只改一份，其余漂移成 bug。
- AI 尤其容易「从零生成」而非「复用」，因为它不主动回忆代码库。

### 最佳实践 + 规则
✅ **先搜后写**：动笔前用 grep/Grep/Explore 搜索相似逻辑、同名工具、同类样式。已有则复用，不另写。
✅ **DRY 的单位是「概念」不是「行」**：一个正则、一个样式、一个状态映射，全仓库只允许一处权威实现。
✅ **共享 CSS 进全局**：被 ≥2 个组件复用的样式（如 `.dot`/`.icon-btn`/`.asset-node`），抽到 `src/styles/_utilities.scss` 的全局 class，组件内不再 `<style>` 重定义。
✅ **共享逻辑进 composable 或 service**：被 ≥2 处复用的纯函数，抽到 `src/lib/` 或 `src/composables/`（composable 命名 `useXxx`）。
✅ **死代码即删除**：发现未被 import 的模块（如 `terminalController.js`），确认无引用后删除，不要留作"以后可能用"。
💡 **新增工具函数时**，先看 `backend.js`（DTO normalize）、`lib/`（terminalThemes/dangerousCommands）、`composables/` 是否已有。

---

## 2. 文件过大 / God File（单一职责 SRP）

### 反模式实证（行数截至当前）
- `src-tauri/src/ssh.rs` **1548 行**：SSH 连接 + 认证 + 键盘交互循环 + PTY 生命周期 + 资源监控 exec + SFTP 全套 + 分块传输 + 隧道 + **完整 SOCKS5 服务器**（L1482-1646，164 行网络逻辑住在 "ssh" 文件里）。`SshSessionManager` 持有 7 个无关 HashMap。
- `src/stores/sessions.js` **810 行**：事件监听注册/销毁 + OSC 标题解析 + xterm addon 构造 + DOM 挂载 + 主题/字号 + 剪贴板 + host-key/keyboard 解析 + 完整连接流程，≥7 个职责。
- `src/components/shell/GlobalModals.vue` **512 行**：9 种 modal 类型塞一个组件。
- `src/components/files/FileColumn.vue` **768 行**、`src/stores/files.js` **612 行**。

### 根因
- 「就近堆叠」思维：新功能往最近的文件里加，不新建文件。
- 无文件大小红线，God File 持续膨胀直到不可维护。
- 违反 **Single Responsibility Principle**：一个模块只应有「一个变更理由」。

### 最佳实践 + 规则
✅ **硬阈值（触发即必须拆分，除非有充分理由并在 PR 说明）**：
| 类型 | 软警告 | 硬上限 |
|---|---|---|
| Vue SFC (`.vue`) | 300 行 | **500 行** |
| Pinia store (`.js`) | 300 行 | **500 行** |
| Rust 模块 (`.rs`) | 400 行 | **800 行** |

> Vue 社区共识：~300 行是常见软上限；超出考虑拆分（[Vue SFC 官方指南](https://vuejs.org/guide/scaling-up/sfc.html)、[SOLID in Vue 3](https://banushushanpuviraj.medium.com)）。本项目多个文件已严重超标。

✅ **按职责拆，不按行数拆**：拆分的判据是「这块逻辑是否有独立的变更理由」，而非「凑够 N 行」。
  - 例：`ssh.rs` 应拆为 `ssh/connect.rs`（连接+认证）、`ssh/pty.rs`（会话生命周期）、`ssh/sftp.rs`（文件操作）、`ssh/tunnel.rs`（含 SOCKS5）、`ssh/known_hosts.rs`。
  - 例：`sessions.js` 应把 OSC 解析、xterm addon 构造、事件监听抽成 composable（`useOscTitleParser`、`useTerminalAddons`、`useSessionEvents`）。
✅ **God File 拆分策略**：先抽出独立纯函数 → 再抽 composable/子模块 → 最后拆组件，每步保证 `npm run build` / `cargo build` 通过。
💡 新增功能时问自己：「这是否属于当前文件的职责？不属于就新建文件。」

---

## 3. 扩展性差 / 高耦合（Open-Closed 原则）

### 反模式实证
- **新增 modal 要改 4 处且无保障**：`GlobalModals.vue` 加一种 type 要同步改：①`modalTitle` switch(L76) ②`submitModal` switch(L179) ③`template` v-if 链(L258) ④`watch` 重置(L100)。漏改任一处 = 运行时 bug，无编译期检查。
- **新增 Tauri 命令要改 3 处**：①定义 `#[command]` ②`generate_handler!` 注册(L230，已 43 个) ③前端 `invokeBackend('字符串')`。漏注册 = "command not found"，且命令名是手写字符串，易拼错。
- **workbench re-export ~90 符号手动镜像**：`workbench.js:175-323` 手动把 5 个子 store 的 state/action 逐个 re-export。加一个 store action 要改子 store + workbench 两处，无类型保障，易漏。

### 根因
- 用 switch/if-else 链做分发，而非「注册表/映射」模式。
- 缺少「单一注册点」，扩展点分散在多处。
- 手动镜像 boilerplate，无编译期同步保证。

### 最佳实践 + 规则
✅ **用注册表代替 switch 链**：
  - Modal：用 `modalType → component` 映射（`const modalRegistry = { assetEditor: AssetEditorModal, ... }`），新增 type 只需注册一项 + 写组件，不再改 3 个 switch。**新增 modal 类型时，优先评估是否引入注册表重构。**
  - 新增分支超过 ~5 个时，强制考虑映射/注册表替代 switch。
✅ **命令名集中管理**：考虑在 `src/services/backend.js` 导出命令常量（`export const CMD = { SSH_CONNECT: 'ssh_connect', ... }`），前端 import 常量而非裸字符串，消除拼写错误。（渐进式，新代码用常量。）
✅ **减少 re-export boilerplate**：workbench 的手动镜像暂保留（架构现状），但**新增 state/action 必须同步两处，PR 自检清单包含「子 store 加了什么，workbench 是否也加了」**。
✅ **扩展点单一化**：任何「加一个功能要改 N 处」的设计，N 应尽量为 1（注册表）或 2（定义+注册）。

---

## 4. 架构缺失（缺少分层抽象）

### 反模式实证
- **无 service 层**：`backend.js` 只有 `invokeBackend`/`listenBackendEvent` 两个泛型包装。57 处 `invokeBackend('命令字符串', {...})` 散落在 6 个 store，领域逻辑（调哪些命令、什么顺序、什么参数）全内联在 store 里。
- **错误处理 3 种风格混用**：
  - 吞掉：`sessions.js` 多处 `try { ... } catch {}`（L297/L323/L375/L405，空 catch）；`resourceMonitor.js:160 catch (e) { /* noop */ }`。
  - 转成 null：`files.js:309/.catch(() => null)`、`sessions.js:585/.catch(() => null)`。
  - 转 announce：`sessions.js:563-564`、`L748`。
  - Rust 统一返回 `Result<T, String>`，但前端对这个 String 的处理各处不同，有时直接丢弃。
- **跨 store 桥接靠手写字符串对象**：每个 store 暴露 `attachWorkbench({...})`，workbench 手动拼装 ~25 个属性绑定，无接口契约，漏提供 → 运行时抛错（`sessions.js:87-90`）。

### 根因
- 直接调用底层 API（invoke），没有领域 service 中间层，逻辑无处安放只能进 store。
- 错误处理无统一策略，每人/每次随手选一种。
- 跨 store 通信用「注入魔法对象」而非显式依赖。

### 最佳实践 + 规则
✅ **引入领域 service（渐进式）**：当某 domain 的 invoke 调用 >5 处时，抽 `src/services/<domain>.js`（如 `sshService.js` 封装 `connect/disconnect/write` 三件套 + 错误归一）。store 调 service，service 调 invokeBackend。新 domain 直接建 service。
✅ **统一错误处理策略**：
  - **业务可恢复错误** → `announce(message)`（状态栏提示），不要空 catch。
  - **预期缺失（如凭据不存在）** → `.catch(() => null)` + 显式 null 检查，**注释说明为何可忽略**。
  - **禁止空 catch {}**：至少 `catch (e) { /* 说明：xxx 预期失败，无需提示 */ }`。
✅ **跨 store 通信显式化**：现有 bridge 保留，但新增跨 store 依赖时优先用：①事件总线 ②composable 共享 ③props/emits，而非往 bridge 对象里加属性。

---

## 5. 不一致 / 漂移（同一概念多份实现）

### 反模式实证
- **两套 status 词汇**：`workbench.js:326 normalizeStatus` 返回 `running/warn/idle`；`ConnectionStatusPill.vue:58-66` 用 `connected/connecting/reconnecting/disconnected/error/idle`。同一「连接状态」概念两套词汇，渲染时各自映射。
- **terminalController（SRP 抽象）被 sessions.js 旁路**，导致两份终端生命周期实现漂移（见 §1）。

### 根因
- 概念的「权威定义点」不清晰，后来者不知道该复用哪份。
- 没有单一数据源（Single Source of Truth）。

### 最佳实践 + 规则
✅ **每个概念一个权威定义**：
  - 连接状态：统一用 sessions store 的 `session.status`（`connecting/connected/disconnected/error`）为运行时权威源；`asset.status`（编辑器初始值）仅作回退。**UI 渲染状态圆点/pill 一律派生自 session.status**，不再各写一套。
  - 状态→视觉映射：统一用 `normalizeStatus()`（workbench.js 导出），禁止组件内另写 status→class 映射。
✅ **新增「同类东西」前先找权威源**：要加状态/类型/枚举映射，先搜是否已有定义点。

---

## 6. 绕过自有抽象（有系统却不用）

### 反模式实证
- **有 GlobalModals 却用 `window.alert/confirm`**：`GlobalModals.vue:182 window.alert('...首次保存请填写密码...')`（表单校验用阻塞式浏览器弹窗）；`files.js:538 window.confirm('确认删除...')`（危险操作确认用原生对话框）。两者都绕过了旁边就有的应用弹窗系统。
- **有设计 token 却硬编码**：（详见 §1，颜色/z-index 散落硬编码，已在 AGENTS.md §4.1 约束）。

### 根因
- 用「最快路径」（原生 API / 硬编码）而非「正确路径」（应用抽象）。
- AI 尤其倾向用熟悉的原始 API。

### 最佳实践 + 规则
✅ **禁止 `window.alert` / `window.confirm` / `window.prompt`**：表单校验用内联错误提示（组件内 reactive error + 红字）；危险操作确认用 `store.modal = { type: 'confirmXxx', ... }` 走 GlobalModals。
✅ **有抽象就用抽象**：颜色用 `var(--token)`、弹窗用 GlobalModals、图标用 lucide-vue-next、右键用 AppContextMenu。发现「有 X 却用了更原始的 Y」时，改用 X。

---

## 7. 靠猜测代替事实（Guessing instead of Facts）

> 2026-09-10 全仓审计沉淀：一次「连接 root 服务器后远程目录加载失败」的排查，牵出 18 处同类缺陷。
> 共同点：**代码对无法控制的外部环境（远端服务器、工具链、locale、时钟、就绪时机、实体身份）做了臆断，而不是去求证**。
> 猜测在开发者自己那台机器上恰好成立，错误被环境掩盖；换个合理的环境（root 用户、BusyBox、中文 locale、慢网络）就炸。
> 这是最低级的做法——不适配多种情况，且失败往往**静默**（数据截断/假数据/被绕过的确认）。

### 四种猜测形态

| 形态 | 定义 | 本项目实证（均已修复） |
|---|---|---|
| **A 环境假设** | 假设外部布局/工具链/平台长某样 | 前端猜家目录 `/home/<username>`——root 的家在 `/root`，连接 117 服务器直接 `No such file`；`find -printf` 假设 GNU coreutils——BusyBox/Alpine/BSD 上不存在；`df -B1` 同为 GNU-only；本地系统目录黑名单硬编码 `c:\windows`——系统装 D 盘即失守；快速连接用 `slugify(host)` 当资产 id——同主机不同用户名互相覆盖且沿用错误凭据 |
| **B 格式猜测** | 按固定格式/字符串嗅探解析外部数据 | `df` 输出按英文表头 `"Filesystem"` 切分且无 `LC_ALL=C`——中文 locale 下磁盘统计静默归零；/proc/diskstats 父设备与分区（sda+sda1）双计数，IO 虚高一倍；OSC 7 路径只编码空格却全量 `decodeURIComponent`——目录名含 `%` 即错乱 |
| **C 静默兜底掩盖错误** | 把「查不到/出错」折叠成默认值继续跑 | 上传循环信任目录列表时刻的 size——增长的文件（日志/导出）静默截断且标记成功；上传前 stat 失败当「目标不存在」——覆盖确认被绕过；资源监控解析失败发全 0 假快照 |
| **D 时序/信号猜测** | 用巧合信号或固定延迟代替就绪证明 | MOTD 横幅（sshd 在 shell 启动**前**打印）被当「shell 已就绪」——慢 `.bashrc` 机器注入竞态双重回显；本地文件 mtime 对比 GitHub `updated_at`——两个时钟域，pull 后永远误判「本地更新」 |

### 根因
- **训练偏置**：模型见过的「典型环境」（桌面 Linux、bash、GNU 工具、英文 locale、/home 普通用户）成了隐含假设，缺乏对「合理但不同的环境」的想象。
- **验证环境单一**：开发/测试都跑在同一台内网 debian 上，猜测恰好全部命中，错误被环境掩盖而非被测试暴露。
- **求证有成本，猜测零成本**：canonicalize/stat/协议握手要多写几行，猜一个 `/home/x` 一行搞定——但前者的正确性是构造出来的，后者的正确性是运气。

### 最佳实践 + 规则
✅ **有协议真值就不猜**：服务器状态用协议问——家目录用 SFTP `canonicalize(".")`，文件存在性用 `stat`，回显控制用 pty 终端模式（`Pty::ECHO`），目录列表用 SFTP `read_dir`（零远端用户态依赖）。必须解析命令输出时：`LC_ALL=C` 前缀 + POSIX 通用选项（`df -P -k` 而非 `-B1`），并明确「该命令在此平台不存在」时的显式失败。
✅ **比较必须同钟域、同来源**：mtime 与 mtime 比、远端时刻与远端时刻比；跨域比较（本地钟 vs 服务器钟、缓存值 vs 实时值）一律禁止——同步基准统一记「本方完成时刻」。
✅ **缓存元数据只做显示，不做语义判据**：进度条可以用列表时刻的 size；循环终止、完整性判断必须用流本身的信号（EOF / 短读 / 0 字节），读完可对账并警告差异。
✅ **身份用完整键**：实体 id 必须包含全部区分维度（username+host+port），部分键当身份 = 碰撞 = 静默串数据。
✅ **「就绪」要事件证明**：等待真实信号（协议握手完成、输出到达），不用「首个 X ≈ 就绪」的巧合假设，更不用固定 sleep 硬扛竞态；能在协议层根治的（如回显）不在应用层打补丁。
✅ **探测三态**：存在性/状态探测返回 真 / 假 / **未知**三种；未知按保守方向处理（当作存在 → 走确认流程），禁止把「查不到」折叠成「不存在」。
💡 **区分「默认值」与「猜测」**：端口缺省 22、normalizeAsset 兜底是**应用自身契约**（我们定义的世界）；家目录/工具链/locale 是**外部世界**（我们无法定义，只能求证）。判断句式：「这个值，我是在定义它，还是在假设外部世界长这样？」——后者必须求证。
💡 **换机自检**：写完任何与外部环境交互的代码，想象远端换成 root 用户 + Alpine + 中文 locale + 慢网络 + GitHub 时钟快 5 分钟，逐项推演还成立吗？

### 机械门禁（文档之外的硬防线）

> 文档是提醒（读到才生效、长会话会衰减），门禁才是约束。以下规则已脚本化并挂进构建链。

`scripts/fact-guards.mjs`（`npm run lint:facts`，随 `npm run build` 执行；CI 在 push / PR 到 `master` 时跑 `npm run build` 会拦下违规，发版链 `tauri beforeBuildCommand` 同样经过它——但直接本地跑或绕过 CI 的路径不受约束）。扫描范围：`src/`、`src-tauri/src`、`crates/myshelltool-core/src`、`src-tauri/build.rs`、`tests/`、`scripts/`（`fact-guards.mjs` 自身除外，其正反样例由 `--self-test` 保障）；命中扩展名 `.ts/.tsx/.vue/.js/.jsx/.mjs/.mts/.cts/.rs`。任一扫描 root 扫到 0 个文件即 exit 1（防空集合上宣称「全部通过」的静默失效）：

| 规则 id | 拦截 | 对应事故 |
|---|---|---|
| `no-guessed-home-path` | 前端拼接 `/home/...` 路径 | root 用户家目录猜测 → 目录加载失败 |
| `no-drive-hardcoded-system-path` | 硬编码 `<盘符>:\windows` 等 | 系统目录保护只覆盖 C 盘 |
| `no-gnu-only-flags` | `find -printf` / `df -B1` | BusyBox/Alpine 上目录列表整条失败 |
| `no-blocking-sleep` | `thread::sleep` / `tokio::time::sleep` 充当就绪/重试等待 | 固定延迟 = 竞态温床 |
| `locale-pinned-df` | 命令串里的 `df -` 必须带 `LC_ALL=C` | 中文 locale 磁盘统计静默归零 |
| `no-host-only-identity` | `slugify(host)` 当实体 id | 同主机不同用户互相覆盖资产 |
| `no-fixed-wait-in-tests` | UI 测试里的 `waitForTimeout(...)` | 快机器断言早于渲染、慢 CI 必 flaky |

- **豁免**：行尾 `// fact-guard:allow <rule-id> <理由≥8字符>`（rule-id 须与命中规则一致，理由必填供 review 审阅；单测反例数据属合法用法）。
- **自检防腐烂**：脚本内置每条规则的正反样例，规则失效（模式写坏/匹配不到）会先让自检失败。
- **增长策略（规则先行）**：每发生一次新的「猜测/硬编码」事故，先在 `fact-guards.mjs` 加规则 + 正反样例，**再**改代码修复——保证同类错误第二次出现被机器拦下。
- **覆盖边界**：门禁只能覆盖「可静态匹配」的形态（字符串/字面量）。时序猜测、缓存元数据误用、错误吞噬等不可 greppable 的形态靠下面的工作流卡点。

### 工作流卡点（不可自动化的部分靠流程兜住）

- **简报必须含「外部环境交互」一节**（派发实现任务的固定项）：逐条列出涉及的远端路径/命令输出解析/locale/时钟域/身份键/就绪信号，每条写明**求证方式**（协议 API/探测/显式失败策略）或标注「应用自身契约，可默认」。无此节的简报不合格，不得派发。
- **review 验收按四种形态扫**：A 环境假设 / B 格式猜测 / C 静默兜底 / D 时序猜测——尤其 C 与 D（门禁覆盖不了）：错误路径是否把失败折叠成了默认值？等待是否用了巧合信号或固定延迟？
- **新能力选型默认协议优先**：能用 SFTP/协议 API 的（canonicalize/stat/read_dir/pty 模式），不允许用「exec 拼命令 + 解析输出」替代；确需 exec 时在代码注释写明平台假设与降级策略。

---

## 8. 自检清单（AI 每次提交前过一遍）

> 这份清单对标上述 7 类问题。提交/PR 前逐项确认。

- [ ] **DRY**：我新写的逻辑/样式/正则，是否已存在？搜过了吗？（§1）
- [ ] **无死代码**：我新增的模块是否被引用？我是否删了确认无用的死代码？（§1）
- [ ] **文件大小**：我改动的文件是否接近/超过硬上限？超了是否该拆？（§2）
- [ ] **扩展点**：我加功能改了几处？能否用注册表降到 1-2 处？（§3）
- [ ] **错误处理**：我写的 catch 是空的吗？是否该 announce 或显式注释？（§4）
- [ ] **一致性**：我用的是概念的权威定义吗？还是另造了一套？（§5）
- [ ] **用对抽象**：我用了 window.alert/硬编码/裸字符串，而项目有更好的系统吗？（§6）
- [ ] **事实还是猜测**：我对远端/外部环境的每个假设（路径/工具链/locale/时钟/就绪/身份），换成 root+Alpine+中文 locale+慢网络的机器还成立吗？能协议求证就不猜；探测失败是否区分了「不存在」与「未知」？（§7）
- [ ] **验证**：`npm run build` + `cargo build` 通过了吗？（AGENTS.md §5）

---

## 9. 参考来源

- [Vue.js 官方 — Scaling Up / SFC](https://vuejs.org/guide/scaling-up/sfc.html)
- [SOLID Principles in Vue 3 (SRP)](https://banushushanpuviraj.medium.com/solid-principles-in-vue-3-writing-cleaner-and-more-maintainable-components-89ad2c6e2f02)
- [Mastering Vue 3 Composables Style Guide](https://alexop.dev/posts/mastering-vue-3-composables-a-comprehensive-style-guide/)
- [Good Practices and Design Patterns for Vue Composables](https://dev.to/jacobandrewsky/good-practices-and-design-patterns-for-vue-composables-24lk)
- [智谱 Coding Agent 最佳实践](https://docs.bigmodel.cn/cn/coding-plan/learning-resources/best-practice)
- Rust：`russh` crate 文档、Rust API Guidelines（`#[serde]` 命名、`Result<T, E>` 错误传播约定）
