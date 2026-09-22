# AGENTS.md

> 本文件是 **AI Coding Agent 的项目上下文单一信息源（Single Source of Truth）**，但只收录**每次协作都需要的常驻指令**。
> 所有 AI 编码工具（Claude Code、Cursor、GitHub Copilot、ZCode 等）应首先读本文件。
>
> **分层原则（防膨胀，本文件自限 250 行以内）**：
> - **本文件 = 常驻层**：协作方式、质量红线、约定、命令、安全红线——每次会话全量注入，必须短而硬。
> - **`docs/` = 按需参考层**：任务触及对应区域时**必读**，平时不占上下文：
>
>   | 任务触及 | 必读 |
>   |---|---|
>   | 新增/移动文件、找模块归属 | [`docs/目录结构详解.md`](./docs/目录结构详解.md) |
>   | 增改 IPC 命令/事件、数据模型、持久化格式 | [`docs/IPC契约与数据模型.md`](./docs/IPC契约与数据模型.md) |
>   | 改某模块前查历史坑与回归防线 | [`docs/已知边界与修复史.md`](./docs/已知边界与修复史.md) |
>   | 工程质量反模式完整实证 | [`docs/llm-engineering-guidelines.md`](./docs/llm-engineering-guidelines.md) |
>   | 文件大小基线 / 架构漂移记忆 | [`docs/architecture-log.md`](./docs/architecture-log.md) |
> - 工具派生文件（`CLAUDE.md`、`.github/copilot-instructions.md`）只引用本文件，不复制内容。Cursor / Copilot 等工具已原生读取 AGENTS.md，无需 `.cursor/rules` 式镜像目录（已删除）。
>
> 维护原则：**架构、命令、约定变化时，先改本文件（或对应 docs 文件），再同步指针**。不要把参考/历史内容塞进常驻层——它每次会话都在计费且稀释红线。

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

> ⚠️ 当前已超标的文件（重构候选）由 **`npm run lint:size`（`scripts/size-guard.mjs`）机械守门**：RATCHET 表登记存量超标文件的基线行数，只许缩不许涨；缩回限内必须除名（锁定成果）。软警告区清单随门禁输出实时打印；历史基线演进见 [`docs/architecture-log.md`](./docs/architecture-log.md)。新增功能时优先考虑拆分这些文件，而非继续往里堆。

**禁止事项**：
- ❌ 重复造轮子：写新逻辑/样式/正则前必须先搜（grep/Grep/Explore），已有则复用。
- ❌ 空catch {}：错误至少 `announce` 或显式注释「为何可忽略」。
- ❌ `window.alert` / `window.confirm` / `window.prompt`：用内联校验 + `GlobalModals`。
- ❌ 硬编码颜色/z-index/间距：用 `var(--token)`（见 `src/styles/_tokens.scss`）。
- ❌ 同一概念多份实现：连接状态等用权威定义点（见指南 §5）。
- ❌ 死代码：未被 import 的模块确认后删除。
- ❌ 靠猜测代替事实：对外部环境（家目录/工具链/locale/时钟域/就绪信号/身份键）不做臆断——协议求证（SFTP canonicalize/stat、pty 终端模式）、解析命令输出加 `LC_ALL=C` + POSIX 选项（写法必须 `env LC_ALL=C <cmd>`，`VAR=值 cmd` 在 csh/tcsh 下整条命令失败）、比较同钟域、身份用完整键、探测三态（真/假/未知按保守处理）。详见指南 §7；**机械门禁**：`npm run lint:facts`（`scripts/fact-guards.mjs`，17 条规则，随 build/CI/发版强制执行；含 animation 简写双缓动拦截）。新增远程能力**默认走 SFTP/协议**，确需 exec 解析输出时注释平台假设与降级策略。
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
| 后端 | **Rust** | `russh 0.49`、`russh-sftp 2.x`（vendored fork，见 §9）、`tokio`（rt-multi-thread/net/sync） |
| 前端框架 | **Vue 3**（`<script setup>` + Composition API） | `vue ^3.5.38` |
| 语言 | **TypeScript**（strict 全量，vue-tsc 类型门禁） | `typescript ^5.9.3` + `vue-tsc ^3.3.11` |
| 状态管理 | **Pinia 3**（setup store 风格） | `pinia ^3.0.4` |
| 图标 | **lucide-vue-next** | `^0.460.0` |
| 终端 | **xterm.js 6** + addon-fit/search/serialize/web-links/webgl | `@xterm/xterm ^6` |
| 内置文本/配置编辑器 | **CodeMirror 6**（npm 打包、懒加载 chunk；json/yaml/xml/md 语言包 + legacy toml/properties） | `@codemirror/* ^6` |
| 样式 | **SCSS + 设计 token 系统**（无 Tailwind） | `sass ^1.101`，自定义 `_tokens.scss` |
| 构建 | **Vite 7** | `vite ^7.2.7`，root=`src/` |
| 测试 | Playwright（UI smoke）+ `cargo test`（core 单元测试） | `playwright ^1.60` |

> ⚠️ **历史不一致提醒**：旧文档（README 早期版本、已清理的 `.omc/` 工具状态）曾写"前端 Vanilla JS 无框架"——**这是过时信息**。项目在 Wave 1–5 重构后已全面 Vue 3 + Pinia 化。以本文件与 `package.json` 为准。

---

## 3. 目录结构（速览）

```
myshelltool/
├── src/                     # 前端（Vite root）
│   ├── components/          # shell / workbench / terminal / files / resource-monitor / ui（App* 基础组件）
│   ├── stores/              # Pinia：8 领域 store + workbench 编排壳（resourceMonitor 不经编排）
│   ├── composables/  lib/   # 组合式函数 / 纯函数模块
│   ├── services/backend.ts  # Tauri IPC 桥（invokeBackend / listenBackendEvent / normalizeAsset）
│   └── types/  styles/      # 共享类型 / SCSS token 体系
├── src-tauri/src/           # Rust 后端：lib.rs（AppState + generate_handler 注册）、ssh/（facade + 七个子模块）、
│                            #   mcp/（内嵌 MCP server）、sync*.rs、resource_monitor.rs、fs_local.rs、http.rs
├── crates/myshelltool-core/ # 共享核心库（无 Tauri 依赖，可独立 cargo test）：资产/凭据持久化、加密、
│                            #   危险命令分类、脱敏、proc 解析、shell 分段
├── tests/                   # Playwright UI 测试五套
├── scripts/                 # fact-guards 事实门禁 / bump-version / gen-changelog
└── docs/                    # 按需参考层（见顶部指针表；子目录 访谈/ 规格/ 计划/ 为访谈规划类技能的产物落点，
│                            #   文档类目录与新增文档一律中文命名——工具链固定目录 src/ tests/ 等除外）
```

> 逐文件注释版目录树见 [`docs/目录结构详解.md`](./docs/目录结构详解.md)；新增文件前先查它确认归属。

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
- **hover/focus 反馈铁律（纯绘制属性）**：交互态只允许改 `color`/`background`/`border-color`/`opacity`/`box-shadow`/`outline-color`/`filter`/`transform`；**禁止**改一切盒模型与文档流属性（`padding`/`margin`/`border-width`/`width`/`height`/`min|max-*`/`font-size`/`font-weight`/`line-height`/`letter-spacing`/`gap`/`flex`/`grid-template`/`display`/`position`/`overflow`/`top|left|right|bottom`）——那会触发重排，hover 一行代码周边元素全位移（页面抖动）。悬停显隐元素必须**常驻占位**（默认 `opacity:0`/`visibility:hidden`，hover 改 `opacity:1`），**禁止** `display:none ↔ flex/grid` 切换；需要位移反馈用 `transform`（合成器属性，不重排）。排查记录与验证方法见 [`docs/已知边界与修复史.md`](./docs/已知边界与修复史.md)「hover 抖动排查」节。
- **路径别名 `@`** → `src/`（vite.config.ts 配置）。import 用 `@/stores/...`、`@/components/...`。
- **组件分层**：`ui/` 是无业务的基础组件（`App*` 命名，barrel 导出）；业务组件按域放 `shell/`、`terminal/`、`files/`、`resource-monitor/`。
- **通用操作入口模式**：右键菜单用 `AppContextMenu`（`items: [{label, action, danger, separator, disabled}]`），参照 `FileSurface.vue` 用法。危险操作用 `danger: true`（红色）或 `AppButton variant="danger"`。
- **弹窗**：业务弹窗统一走 `GlobalModals.vue`（`store.modal = { type, ...payload }`），按 `modal.type` 分支。新增 type 需同步改 `modalTitle` / `submitModal` / `watch`。保留 legacy 选择器（`#modalLayer`/`#modalBody`/`.modal-actions .btn.danger`）以兼容测试。

### 4.2 状态管理（跨 store 桥接）
- **`workbench.ts` 是编排壳**：实例化 8 个子 store（sessions/files/tunnels/assets/ui/mcp/sync/editor，v0.18 起），`initialize()` 编排启动加载，用 plain-object 返回 + `computed()` 包裹子 store 的响应式 state（**不要直接暴露子 store 的 ref**，会丢响应性）。**注意 `resourceMonitor.ts` 不经 workbench 编排**——它由 `ResourceMonitorPanel.vue` 直接 `useResourceMonitorStore()` 使用（独立轮询生命周期，与全局初始化解耦）。
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
npm run build        # 前端构建 = 更新日志生成（gen-changelog-history.mjs 遍历 v* tag →
                     #   src/generated/changelog.json，产物提交进 git，设置面板「更新日志」
                     #   区块离线渲染；发版 CI 在 tag checkout 上重新生成最新全历史打进安装包）
                     #   → 事实门禁（lint:facts）→ vue-tsc 类型检查 → Vite 构建
npm run tauri:build  # 完整桌面安装包（Windows NSIS），beforeBuildCommand 走 npm run build 自动含门禁

# —— 测试 ——
npm run test:core    # Rust core 单元测试：cargo test --manifest-path crates/myshelltool-core/Cargo.toml
npm run test:ui      # UI 五套：smoke / host-key / file-loading / **ipc-flows** / **editor**（后两类 mock
                     #   window.__TAURI__ 驱动真实 store 流；editor 覆盖 打开→保存参数→校验拦截→冲突→关 tab；需先 npm run dev）

# —— 静态检查（各自单跑）——
npm run lint:facts   # 「靠猜测代替事实」门禁：scripts/fact-guards.mjs（指南 §7）；build 首步即跑，CI/发版同步生效
npm run lint:size    # 文件大小红线门禁：scripts/size-guard.mjs（RATCHET 制，见「质量红线」节）；随 build 生效
npm run check:ipc    # IPC 契约清单级一致性：scripts/ipc-contract-check.mjs（命令注册表 × 调用面、事件 emit × 监听面）；随 build 生效
npm run type-check   # vue-tsc --build，strict 全量；build 亦含此步

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

## 6. IPC 契约（索引）

前端经 `src/services/backend.ts` 的 `invokeBackend(command, args)` / `listenBackendEvent(event, handler)` 调 Rust；**非 Tauri runtime 会抛错**（浏览器预览模式无 SSH 功能）。

- **完整命令清单**（资产/凭据/Gist 同步/Device Flow/SSH/SFTP/隧道/监控/本地文件/MCP）与**事件列表**见 [`docs/IPC契约与数据模型.md`](./docs/IPC契约与数据模型.md)——**增改命令/事件前必读**，其中固化了易踩坑（`sftp_list_dir` 空 path 语义、流式下载契约、OAuth poll 的 tag enum、后端 `Err(String)` 以字符串 reject 等）。
- 常驻不变量：新命令必须在 `generate_handler!` 注册（§4.3）；参数 camelCase；`sftp_download_to_file` 流式落盘且**不可取消**（勿造假取消按钮）。

---

## 7. 数据模型（速览）

- **持久化分层一句话版**：资产元数据 → `connection-assets.json`；凭据 → `credentials/<id>.cred`（只走 SecretStore，见 §8）；同步状态/会话密钥 → `sync-state.json` + DPAPI 保护的 `.cred`；known_hosts、MCP 配置、MCP 执行日志各一文件；**活跃会话/隧道/SFTP 缓存纯内存**（重启丢失）。
- ConnectionAsset 字段表、分组约定（`asset.group` 字符串、`/` 分隔、「未分组」保留）、Gist 载荷格式（`blob.salt` 必须非空）、McpStatus 结构 → [`docs/IPC契约与数据模型.md`](./docs/IPC契约与数据模型.md#数据模型)。

---

## 8. 安全设计红线

- 凭据（密码/私钥/passphrase）**只走 SecretStore**，不进资产 JSON、日志、错误信息、前端 console。
- 危险文件操作（删除、覆盖）**必须弹窗确认**。
- Host key 变更默认**阻止连接并警告**。
- 远程命令执行/隧道监听 `0.0.0.0` 需安全审视。
- **【v1.6】同步主密码绝不落盘**：资产同步的主密码（master password）在派生 AES 密钥后即丢弃，绝不存盘。自动同步功能用「会话密钥 + DPAPI 保护」绕过每次输密码：首次启用时用主密码派生固定 AES key（Argon2id，确定性），该 key 经 DPAPI（User scope，绑定 Windows 用户登录态）加密后存 SecretStore（credential id = `sync-session-key`）。离机即失效，非 Windows 或 DPAPI 失败时降级为手动主密码模式（不静默失败）。**【v2.7】该 key 的派生 salt 随载荷上传**（`blob.salt`，salt 本身不敏感），于是「离机即失效」只针对会话密钥本身，**备份仍可用主密码在任意机器恢复**——修复前 salt 只存本机，自动同步过的备份换机后连主密码都解不开（来龙去脉见 [`docs/已知边界与修复史.md`](./docs/已知边界与修复史.md) v2.7 续修）。
- **【v2.7】上述红线的边界（一次有理由的例外）**：`credentials/sync-recovery-password.cred` 会保存**应用自己生成**的高熵恢复密码（用户点「生成强密码」时）。判据是**谁选的秘密**：
  - 应用生成的 24 位随机串**只服务这一份备份**，不存在"复用别处口令"的泄漏面；而它防护的风险（"用户想不出强密码 / 忘了 / 把恢复凭据押在外部密码管理器上"）是真实且更常见的失败。落盘经 DPAPI，且注意：**免密功能本来就已经把由它派生的密钥存在本机**了，多存一份明文密码只多出"可跨机使用"这一项能力 —— 而这正是换机恢复的定义。
  - **用户手输的密码一律不保存**（可能是他别处复用的口令）：`sync_reveal_recovery_password` 在那种情况下返回 None，界面如实说"应用没有保存它，请自行记牢"。
  - 一致性：保存值与实际使用的主密码不符时必须**删除**（否则「查看恢复密码」给出打不开备份的错值，用户抄走后换机报"密码错误"且无从归因）。

---

## 9. 当前已知边界（行动清单）

> 本节只保留**现在仍影响决策**的一行式边界。各版本修复史的完整来龙去脉与回归防线（改对应模块前先 grep 相关小节）见 [`docs/已知边界与修复史.md`](./docs/已知边界与修复史.md)。

- **【v0.20/S9】上传与下载都可取消**（`transfer_cancels` 共用旗标通道，块边界 ≤1MiB/64KiB）：`sftp_upload_cancel` / `sftp_download_cancel` 置旗标，循环在块边界中止并清理半截文件；下载取消后端返回 `[download:cancelled]` 前缀，前端收敛为 cancelled 态（非 error）。TransferDrawer 传输行（上传+下载）均有取消按钮。
- **【2026-09-21】源头自适应布局（低分辨率适配）**：窗口 ≥800×600 全宽自适应，**无整体缩放**（字号不变，信息密度按断点降级）。断点（视口 CSS px）：≥1280 现状；<1280 纯 CSS 降档（`workbench-shell.narrow.scss`：标题栏搜索框/状态栏收紧）；<1024 自动折右栏、<860 侧栏收 44px rail（`useAdaptiveLayout.ts` 经 store `setRightCollapsed/setAssetsCollapsed`，persist=false 不污染用户偏好、只恢复自动折的、用户手势优先）；折叠 rail **悬停弹出资产 flyout**（`ConnectionSidebar` 的 `.sb-flyout`，fixed 定位逃 overflow:hidden 祖先、z-index `calc(var(--z-popover) - 1)` 保右键菜单盖住面板）。`--terminal-h` 写入前按当前视口夹紧（`applyCssVars`，文件区保底 120px）。窗口最小 800×600（tauri.conf），首开超屏自动最大化。**改断点值/折叠机制/面板尺寸契约前必读 useAdaptiveLayout.ts 与 usePanelResize.applyCssVars 注释**（纯 CSS 断点会与 store/dataset/内联变量脱节破版，折叠必须走 store action；断点值在 narrow.scss 与 composable 两处保持一致）。回归红线：ui-smoke 窄视口断言（溢出量 `.workbench-shell` 盒宽——body overflow:hidden 使 documentElement.scrollWidth 恒等于 clientWidth，是假阴性；终端夹紧断言量内联 `--terminal-h`）。
- **【v2.9】上传已服务端流式化**：`sftp_upload_from_file`（本机路径直读，字节不经 IPC）；上传入口因此全部是路径型——本地面板条目、原生文件对话框（`plugin:dialog|open`）、OS 拖入（Tauri 窗口级 `onDragDropEvent`，**拖入窗口任意区域都会触发上传**，含终端区；HTML5 drop 在 Windows 上本就不触发）。旧分块三件套（`sftp_upload_start/chunk/finalize`）与 `fs_local_read_chunk/write_chunk` 已删。
- **资产 id 消歧只改 `id`**、不动 `credential_id` 引用，凭据共用面未完全消除（暂不做）。
- **【v2.9】远程转发已实现但走专用连接**（russh 0.49 `tcpip_forward` 要 `&mut Handle`，共享 `Arc<Handle>` 给不出）：每条 remote 隧道一条独立 SSH 连接（OpenSSH 的 MaxStartups/连接数配额视角下与 local/dynamic 不同）；host key 必须已被 GUI 信任过（known_hosts 精确匹配，后台连接不弹窗）；凭据必须已存库（`asset_id` 解析，私钥内容托管 `private_key_credential_id` 形态暂不支持——与 MCP headless 同边界）；`tunnel-traffic-*`/`tunnel-error-*` 事件前端无监听（与 local/dynamic 一致的存量缺口，异步失败靠 `tunnel_list` 的 active/error 字段呈现）。
- **【v0.20/P0-2】ProxyJump 单跳已实现**：资产 `jump_host`（"host"/"host:port"）→ 跳板必须是资产库成员（host+port 匹配，复用其凭据与 known_hosts 信任，未匹配明确报错）；目标握手跑在跳板 direct-tcpip 通道流上（`connect_stream`），GUI/headless/MCP/SFTP/隧道全路径生效；**链式跳板不支持**（跳板资产的 jump_host 被忽略并告警）；跳板 Handle 开完通道即 drop（保活假设：russh 连接由独立任务驱动，通道活着连接就活——**此假设需 tauri:dev 真机验收**）；`user@host`/IPv6 字面量形态不支持。
- `sanitize_credential_id` 是删除式清洗、不同 id 可碰撞（暂不改；改规则需迁移既有凭据文件）。
- **Windows 工具链坑**：`cargo build` 偶被 windres build script 阻断 → `cargo check` 兜底；src-tauri 测试二进制缺 Tauri runtime DLL 跑不起来 → `cargo check --tests` 验编译，**安全判据测试必须放 core 真跑**。
- **改前端后必须先 `npm run build` 再编译 Rust**：`frontendDist` 在 Rust build script 阶段打进二进制；调试期用 `npm run tauri:dev`（前端走 devUrl，刷新即生效）。
- **多窗口已知限制**：窗口不持久化恢复、资产删除不跨窗口同步、主题不跨窗口实时同步、Esc 取消的窗外拖拽可能误开窗；MERGE_ACK 宽限期后到达不自动移交（文案已诚实，维持现状）。
- **MCP 端口**：release 恒 41235 / debug 默认 41500（`MYSHELLTOOL_MCP_PORT` 可覆盖）；dev 与正式版仍共享 `app_data_dir`。**【v0.20/A1】入口已加 token 鉴权**（URL 内嵌 `/mcp/<token>` + Bearer 兼容；判定在 `core::mcp_auth`；重置走 `mcp_reset_token` 或面板按钮）——升级后旧 host 配置里的无 token URL 会 401，需在 MCP 面板重新「复制配置」。
- **【v0.18】内置编辑器**（CodeMirror 6 覆盖面板，双击文本类文件进入）：单文件上限 2 MiB；编码白名单严格转码（GBK 等，绝不 lossy）；写回带 expected 守护（冲突三选）+ temp/rename 原子写（与 MCP 共享 rename 兜底）+ 可选写前备份（app-data 滚动 3 份）；草稿存 app-data（重开较新时引导恢复）。下载完成 toast 的「打开」= ShellExecute 系统默认程序（下载的 .exe 会执行，与浏览器下载条一致）。
- **【v0.20/S8】CSP 已启用**（tauri.conf.json `app.security.csp`）：default-src 'self'；script-src 'self'（**禁 eval**——新增依赖若用 new Function/eval 会被 CSP 静默拦截）；style-src 含 'unsafe-inline'（Vue SFC 运行时注入样式所需）+ Google Fonts；connect-src 含 `ipc:` 与 `http://ipc.localhost`（**Tauri 2 WebView2 IPC 通道形态，缺这条 invoke 全挂——改动 CSP 前必读**）。新增外联资源（字体/图床/API）须同步登记进 CSP 对应指令；无消费面的授权不写（blob: 曾被写后删）。真实 CSP 只在 Tauri shell 生效（tauri:dev/build），浏览器 dev 页面不受影响。
- **russh-sftp 是 vendored fork**（`third-party/russh-sftp`，把非 UTF-8 文件名可逆编码为 PUA-A）：升级 russh-sftp 需重放补丁（buf.rs/ser.rs 两处 + v0.20 set_metadata 透传 + 往返单测）。
- **MCP 会话复用未做**：`tools.rs::exec_on_asset` 直走 headless 建连；可注入 GUI 会话池复用（follow-up）。

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
7. **要找目录归属 / IPC 细节 / 数据模型 / 某模块历史坑**？→ 按顶部指针表读对应 docs 文件；不要把细节复制回本文件。
8. **改完**？→ 跑 §5 的验证（build 首步即含 lint:facts 门禁），如实报告 exit code。
