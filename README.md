# myshelltool

> Windows 桌面 SSH 运维客户端 + **MCP Server**（AI 可调用）
>
> Rust + Tauri 2 构建。既是人用的图形客户端，也能被 Claude Code / Cursor / Cline 等 AI 工具通过 MCP 协议调用，让 AI 帮你执行 SSH 运维操作。

---

## 目录

- [它是什么](#它是什么)
- [功能特性](#功能特性)
- [快速开始](#快速开始)
- [GUI 使用](#gui-使用)
- [Gist 资产同步（跨机器）](#gist-资产同步跨机器)
- [MCP Server（让 AI 调用）](#mcp-server让-ai-调用)
- [技术栈](#技术栈)
- [项目结构](#项目结构)
- [开发指南](#开发指南)
- [安全设计](#安全设计)
- [架构决策](#架构决策)
- [版本演进](#版本演进)
- [已知限制与 Follow-ups](#已知限制与-follow-ups)
- [License](#license)

---

## 它是什么

myshelltool 有**两种身份**：

1. **SSH 运维客户端**（给人用）— 图形界面管理多台 SSH 主机：连接、终端、文件传输、隧道、资源监控
2. **MCP Server**（给 AI 用）— 暴露 SSH 能力给 AI 工具，让 AI 通过自然语言操作你的服务器

第二种身份是**核心差异化能力**——市面上支持 MCP 的 SSH 运维工具极少。

```
┌─────────────────┐   Streamable HTTP       ┌──────────────────────────┐
│ 任意 MCP 客户端  │   (JSON-RPC over HTTP)  │  myshelltool.exe (GUI)   │
│ (AI 工具)       │ ──────────────────────→ │  内嵌 MCP server         │
└─────────────────┘                         │  127.0.0.1:41235/mcp     │
                                            └────────────┬─────────────┘
                                                         │ russh SSH（同进程）
                                                         ▼
                                            ┌──────────────────────────┐
                                            │  你的服务器们             │
                                            └──────────────────────────┘
```

> MCP server **内嵌 GUI 进程**（Streamable HTTP transport），没有独立 exe。详见 [架构决策](#架构决策)。

---

## 功能特性

### GUI 客户端

- **连接资产管理** — 多级嵌套分组（`/` 分隔）、标签、筛选、收藏；`user@host:port` 连接串快速解析（含 IPv6 括号形式）
- **SSH 终端** — 多标签会话、xterm.js、UTF-8 中文、自动 fit、深/浅主题、自动重连（带稳定窗口判定，不无限重试）
- **多窗口工作台** — 资产可拖出为独立 OS 窗口；已连接的终端 tab 可跨窗口迁移（scrollback 带颜色回放），也可一键移回主窗口
- **Host Key 验证** — 首次连接指纹确认、变更高危警告、known_hosts 持久化
- **多认证方式** — 密码、私钥（文件选择器选取）、passphrase、keyboard-interactive
- **凭据安全存储** — 密码/passphrase 走本地 SecretStore（Windows DPAPI），不进资产 JSON / 日志
- **SFTP 文件管理** — 浏览、上传/下载、新建/重命名/删除（带确认）、权限列、UNC 路径面包屑
- **Monaco 远程编辑** — 语法高亮、Ctrl+S 保存、按扩展名识别语言
- **文件传输队列** — 分块传输、实时进度、失败重试、上传后字节对账
- **终端与文件面板联动** — 连接后自动加载远程目录；终端 `cd` 经 OSC 7 跟随切换文件面板目录
- **SSH 隧道** — Local forwarding、Dynamic SOCKS5 代理
- **资源监控** — 远程 CPU/内存/网络/磁盘实时图表；远端无 `/proc`（非 Linux）时明确报错，不发假数据
- **应用内自动更新** — 内置 updater，新版本自动检测

### Gist 资产同步（跨机器）

- **跨机器同步连接资产** — 资产加密推送 GitHub Gist，换机器/重装系统一键拉回
- **GitHub OAuth Device Flow 登录** — 浏览器输一次性代码即可，无需手动创建 token（手动粘贴保留为兜底）
- **可选凭据同步** — 默认只同步资产拓扑；可开启端到端加密的密码/托管私钥同步，换机后一键直连
- **端到端加密** — Argon2id 派生密钥 + AES-256-GCM，GitHub 服务端只见密文
- **乐观并发 + 冲突检测** — 推送前比对远端版本防静默互踩；双向改动时弹框选覆盖方向

### MCP Server

- **内嵌 GUI 进程** — axum HTTP service（Streamable HTTP transport），随 GUI 启停，默认 `127.0.0.1:41235/mcp`
- **13 个 Tools** — 7 个系统类（资产/会话/磁盘/系统/服务/资源监控/命令执行）+ 6 个文件类（list/read/write/upload/download/remove），文件传输全能力无桩化
- **4 Resources / 3 Prompts** — 资产/会话/known-hosts 随机读取；诊断/安全审计/磁盘清理结构化引导
- **拦截等级可配置** — Minimal（默认）/ Strict，GUI 面板一键切换，改档即时生效
- **审批三级降级** — elicitation（客户端内确认框）→ GUI 弹窗（60s 超时）→ fail-secure 拒绝
- **执行审计日志** — 触发远程执行的调用逐条记录（命令已脱敏），GUI 内查看/搜索/清空
- **Host key 安全门** — 仅服务已在 GUI 信任过的资产

---

## 快速开始

### 环境要求

- **Windows 10/11**（x64）
- [Node.js](https://nodejs.org/) 20.19+ 或 22.12+（推荐 22/24，使用 npm）
- [Rust](https://rustup.rs/) stable（含 cargo）
- [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)（Windows 11 自带）

### 安装与运行

```bash
git clone https://github.com/RT-2020/myshelltool.git
cd myshelltool
npm install
npm run tauri:dev        # 开发模式启动（完整 SSH 功能；首启编译 Rust 数分钟）
```

### 从源码构建安装包

```bash
npm run tauri:build
# 产物：src-tauri/target/release/bundle/nsis/myshelltool_<版本>_x64-setup.exe
```

---

## GUI 使用

### 配置第一台服务器

1. 左侧栏点「+ 新增连接」（快捷键 `+`）
2. 填写：名称、主机、端口（默认 22）、用户名、认证方式
3. 密码认证输入密码（加密存入本地 SecretStore）；私钥认证用文件选择器选私钥，有 passphrase 一并填写
4. 保存后双击资产连接

> 首次连接会弹出 host key 指纹确认框，核对 SHA256 后信任即记录到 known_hosts；**指纹变更会阻止连接并警告**（可能是中间人攻击）。

### 日常操作

| 操作 | 方式 |
|------|------|
| 打开终端 | 双击资产 / 右键「连接」 |
| 独立窗口打开 | 右键资产 → 「在独立窗口打开」，或把资产拖出主窗口边界 |
| 迁移终端到其他窗口 | 拖住已连接的终端 tab 拖出主窗口（回迁用独立窗口标题栏「移回主窗口」） |
| 传输文件 | 连接后切到「文件」区域，拖拽上传 / 双击下载 |
| 远程编辑 | 文件区右键 → 「编辑」（Monaco 编辑器打开） |
| 开隧道 | 侧栏「隧道」→ 新增 → 选类型（local/SOCKS5） |
| 资源监控 | 连接后右侧「资源监控」面板自动刷新 |

---

## Gist 资产同步（跨机器）

把连接资产（主机/端口/用户名/分组等元数据）加密同步到 GitHub Gist，换机器或重装系统时一键拉回。默认**不同步密码/私钥**；可选择开启凭据同步（同样端到端加密）。

### 第一步：登录 GitHub

**推荐：OAuth Device Flow**（免手动创建 token）：

1. 打开「设置 → 同步」的 GitHub 授权区块，点「GitHub 账号登录」
2. 界面显示一次性验证码，点「打开授权页」跳转 `github.com/login/device`
3. 浏览器输入验证码并授权——完成，token 自动存入本地 SecretStore（DPAPI 保护）

> 登录**只需一次**：token 持久保存在本机安全存储里，重启应用都会显示「已登录」（想换账号才点「重新登录」）。

> **授权页显示的应用是谁的？** Device Flow 登录走的 OAuth 应用注册在**作者个人 GitHub 账号**下（个人开源工具的常规做法；应用未做 GitHub 认证，授权页可能带有相应提示）。它的 client_id 属公开标识、随源码分发（见 [`src-tauri/src/sync_oauth.rs`](./src-tauri/src/sync_oauth.rs)），只申请 **Gist 读写**一项权限，不涉及你的仓库、代码或其他账号数据；即使该应用日后失效，也只影响新的登录授权，已保存在本机的 token 在有效期内继续可用。介意的话可改用下方「手动粘贴 PAT」登录，功能完全等价。

**兜底：手动粘贴 PAT**：到 GitHub → Settings → Developer settings → Personal access tokens 新建一个**只需 Gist 读写权限**的 token，粘贴到 PAT 配置卡片保存。

### 第二步：配置同步

1. 输入主密码（≥6 位，用于加密同步载荷）→ 点「初始化」
   - 会在你的 GitHub 自动创建一个 secret Gist，gist_id 记录到本地 `sync-state.json`
   - **主密码只输这一次**：初始化后本机用 DPAPI 记住派生密钥，之后 push/pull 与自动同步都不再需要输入
2. **换机器拉取已有同步**：登录同一 GitHub 账号 + 输入**原主密码** + 填入已有 `gist_id` → 点「初始化」，本地资产被远端覆盖（同样只输这一次）

### 日常使用

| 操作 | 说明 |
|------|------|
| **推送（Push）** | 当前本地资产加密推送到 Gist（已免密时无需输密码）。推送前与远端版本比对，远端有新改动则中止并提示先拉取 |
| **拉取（Pull）** | 从 Gist 拉取解密，**覆盖本地**资产。凭据恢复失败的项会逐条计数提示 |
| **自动同步** | 初始化时自动开启；资产增删改自动推送（本机免密，离机即失效） |
| **凭据与私钥同步** | 可选开关。开启后登录密码与托管私钥经主密码加密一并同步，换机一键直连 |
| **冲突解决** | 本地和远端都有改动时弹框显示双方摘要，选「本地覆盖远端」或「远端覆盖本地」 |
| **重置主密码** | 验证旧密码后，新密码立即重新加密**整份保管库（含凭据）**并推送 |
| **清空同步** | 删除本地 sync-state（**不会**删 GitHub 上的 Gist，需手动删除） |

> ⚠️ **主密码丢失无法找回**：它是解密云端备份的唯一凭据。不想自己设密码？点「生成强密码」——应用生成高熵随机密码并**保存在本机**，可随时在「换机恢复」里查看或复制带走（你自己手输的密码不会被保存）。

### 安全模型

- **数据只在你与 GitHub 之间流动**：同步与登录请求直连 GitHub API，没有也不经过任何作者控制的服务器（源码可审计）；备份密文推送到**你自己账号**下的 secret Gist，作者不持有你的任何数据。登录用的 OAuth 应用虽注册在作者个人账号（见上文说明），但不改变这一数据流向。
- **加密**：Argon2id（主密码 + 随机 salt 派生 256-bit 密钥）+ AES-256-GCM（认证加密，防篡改）。每次推送用新的随机 salt 和 nonce，相同内容每次密文都不同。
- **默认不上传的内容**：密码、passphrase、私钥、GitHub token——只在本地 SecretStore。开启凭据同步后也全部经主密码加密才上传。
- **同步的内容**：`connection-assets.json`（名称、host、port、用户名、分组、标签、认证方式**类型**、私钥路径）+ 可选加密凭据载荷。私钥文件本身不同步，只同步路径。
- **GitHub 侧**：Gist 为 secret（不公开列出，但**有 gist_id 的人仍可访问**），且内容是密文，泄露也无法直接解密。
- **失败安全**：本地资产文件损坏时中止同步并报错，绝不把残缺数据当备份推上去；凭据读取失败时中止推送。

---

## MCP Server（让 AI 调用）

myshelltool 内嵌 MCP server（Streamable HTTP，随 GUI 启停，无独立 exe），把 SSH 能力暴露给任何支持 MCP 的 AI 客户端。**各客户端的逐步配置指南（Claude Code / Cursor / Cline / Claude Desktop 桥接）见 [`docs/mcp-setup.md`](./docs/mcp-setup.md)。**

### 三步接入

1. **启动 GUI 并配好资产**——MCP 读取与 GUI 相同的数据目录，且仅服务已完成首次连接（信任 host key）的资产
2. **在 GUI「MCP」面板查看 endpoint**——默认 `http://127.0.0.1:41235/mcp`（端口被占用会 +1，以面板显示为准；只监听 localhost）
3. **客户端 MCP 配置填入 URL**——任何支持 `streamable-http` 的客户端通用：

```json
{
  "mcpServers": {
    "myshelltool": {
      "url": "http://127.0.0.1:41235/mcp"
    }
  }
}
```

> ⚠️ Claude Desktop 目前不支持 `streamable-http`，需经 [`mcp-remote`](https://www.npmjs.com/package/mcp-remote) 桥接；其余主流客户端原生可用。各客户端配置位置与已知问题见 [`docs/mcp-setup.md`](./docs/mcp-setup.md)。

### 能力一览

**13 个 Tools = 7 系统类 + 6 文件类**：资产/会话/磁盘/系统/服务/资源快照/**任意 shell 命令**，加文件 list/read/write/upload/download/remove（流式传输 + SHA256 校验）。另有 4 个 Resources（资产/会话/known-hosts/会话日志）与 3 个诊断 Prompts（健康诊断/安全审计/磁盘清理）。逐工具说明与审批语义见 [`docs/mcp-setup.md`](./docs/mcp-setup.md)。

### 安全机制

**拦截等级**（GUI MCP 面板可切换，对已连接的 AI 会话下次调用即生效）：

| 等级 | 行为 |
|------|------|
| **Minimal**（默认） | 仅硬拦毁灭性命令（`rm -rf /`、mkfs、dd 写块设备、fork 炸弹等），其余直接执行（记执行日志供审计） |
| **Strict** | 非白名单一律人工确认；毁灭性命令同样恒拒 |

- **白名单逐段判定**：只读命令按 shell 词法分段后**逐段**过白名单——`df -h; cat /etc/shadow` 这类拼接不会因前缀相似漏过；含重定向/命令替换一律不进白名单
- **审批三级降级**：elicitation（客户端界面内确认框）→ GUI 弹窗（60s 超时）→ fail-secure 拒绝（宁可误拦不可漏放）。高危确认采用**三段式对照**——AI 声明意图 / 真实命令 / 后果预测，可识破 AI 伪装（intent 说「查看日志」、command 却是 `rm -rf /var/log`）
- **执行审计日志**：真实触发远程执行的调用逐条记录（时间/资产/命令[口令已脱敏]/决策/输出摘要），GUI 内可查看/搜索/清空
- **恒拦项**：毁灭性命令两档恒拒；私钥等敏感凭据文件读取恒需审批；根级/核心目录删除、核心系统目录写恒拒
- **Host key 门**：仅服务已在 GUI 信任过的资产，未知主机直接拒绝

---

## 技术栈

| 层 | 技术 | 版本 |
|---|---|---|
| 桌面框架 | **Tauri 2** | `@tauri-apps/cli ^2.9.5` |
| 后端 | **Rust** | `russh 0.49`、`russh-sftp 2.x`（vendored fork）、`tokio`、`reqwest 0.12` |
| MCP | rmcp（官方 Rust SDK）+ axum + tokio-util（Streamable HTTP + elicitation） | `rmcp ~1.7` / `axum 0.8` |
| 前端框架 | **Vue 3**（`<script setup>` + Composition API） | `vue ^3.5.38` |
| 语言 | **TypeScript strict 全量**（vue-tsc 类型门禁） | `typescript ^5.9.3` |
| 状态管理 | **Pinia 3**（setup store） | `pinia ^3.0.4` |
| 终端 | xterm.js 6 + addon-fit/search/serialize/web-links/webgl | `@xterm/xterm ^6` |
| 图标 / 远程编辑 | lucide-vue-next / Monaco Editor（CDN） | `^0.460.0` / 0.52 |
| 样式 | SCSS + 设计 token 系统（无 Tailwind） | `sass ^1.101` |
| 构建 | Vite 7（root=`src/`） | `vite ^7.2.7` |
| 测试 | Playwright（UI，mock IPC 驱动真实 store 流）+ cargo test（core） | `playwright ^1.60` |

---

## 项目结构

```
myshelltool/
├── src/                        # 前端（Vue 3 + TS strict，Vite root）
│   ├── components/             # shell / workbench / terminal / files / resource-monitor / ui（App* 基础组件）
│   ├── stores/                 # Pinia setup store（领域 store + workbench 编排壳）
│   ├── composables/            # useTheme / useAutoReconnect / useGithubDeviceLogin / ...
│   ├── lib/                    # 纯函数模块（危险命令判定、终端生命周期、会话迁移…）
│   ├── services/backend.ts     # Tauri IPC 桥（invokeBackend / listenBackendEvent）
│   └── styles/                 # SCSS 设计 token
├── src-tauri/                  # Rust 后端：ssh/（会话·SFTP·隧道·headless）、mcp/（内嵌 MCP server）、sync/、http.rs
├── crates/myshelltool-core/    # 共享核心库（无 Tauri 依赖，独立 cargo test：资产/凭据/危险命令/脱敏/proc 解析）
├── tests/                      # Playwright UI 测试
├── scripts/                    # fact-guards 事实门禁 / bump-version / gen-changelog
└── docs/                       # 目录结构详解 / IPC契约与数据模型 / 已知边界与修复史 / 工程指南 / architecture-log / 规格 / 计划 / 访谈
```

> AI 协作约定的单一信息源是 [`AGENTS.md`](./AGENTS.md)：常驻指令（协作方式/约定/命令/质量红线）全量维护在本文件；目录职责、IPC 契约、数据模型、修复史分层在 `docs/` 按需加载，由 `AGENTS.md` 顶部指针表索引。

---

## 开发指南

### 常用命令

```bash
npm install              # 安装前端依赖
npm run tauri:dev        # GUI 开发模式（完整功能）。MCP server 随 GUI 启停

npm run lint:facts       # 「靠猜测代替事实」事实门禁（build 首步强制）
npm run type-check       # vue-tsc strict 全量类型检查
npm run build            # 事实门禁 → 类型检查 → Vite 构建
npm run tauri:build      # 完整桌面安装包（NSIS，自动含上述门禁）

npm run test:core        # Rust core 单元测试（191 项：安全判据/脱敏/原子写/proc 解析…）
npm run test:ui          # Playwright 四套（需先 npm run dev 起服务）
cd src-tauri && cargo check   # Rust 类型检查
```

> 安全判据（白名单绕过/命令分段/脱敏等）必须在 `npm run test:core` 里真跑——src-tauri 的测试二进制受 Tauri runtime DLL 限制跑不起来，用 `cargo check --tests` 验证可编译。

### 编码约定

遵循 [`AGENTS.md`](./AGENTS.md)（单一信息源），要点：

- **前端**：Vue 3 `<script setup lang="ts">` + Composition API，TypeScript strict，Pinia setup store，SCSS 设计 token（颜色/间距/z-index 一律 `var(--xxx)`）
- **后端**：Tauri 命令用 `State<'_, AppState>` 统一解析，参数 camelCase，新命令注册到 `generate_handler!`；持久化逻辑放 `crates/myshelltool-core`
- **质量红线**：不重复造轮子、无空 catch、凭据只走 SecretStore、对外部环境不猜测（协议求证 + `LC_ALL=C` + 三态探测）、失败一律 fail-closed

详见 [`docs/llm-engineering-guidelines.md`](./docs/llm-engineering-guidelines.md)。

---

## 安全设计

- **凭据隔离**：密码/私钥/passphrase 只走本地 SecretStore（Windows DPAPI），不进资产 JSON / 日志 / 错误信息
- **Host key 强制验证**：首次连接必须人工确认指纹，变更阻止连接并警告
- **危险操作确认**：删除/覆盖/批量操作必须弹窗确认
- **命令脱敏**：命令文本落执行日志前遮蔽口令（`mysql -pP@ss`、`curl -u user:pass` 等）；GUI 审批弹窗仍显示原命令（透明性优先）
- **原子写**：资产 JSON、凭据、同步状态、MCP 配置/日志全部走临时文件 + rename 原子替换，失败保留原文件
- **远端内容不猜编码**：二进制/非 UTF-8 内容明确拒绝并引导下载，不返回「看起来正常实则乱码」的内容
- **MCP 审批与拦截等级**：见 [MCP Server 节](#mcp-server让-ai-调用)

### 残余风险（已知，已留档）

- **提示词注入**（行业级未解难题）：AI 可能被远程内容诱导执行恶意命令。靠三段式审批人工识破 + 拦截等级兜底。
- **DPAPI 绑定 Windows 账户**：凭据目录拷到别的机器/账户无法解密，需重新输入密码。
- **Gist 主密码无法找回**：主密码只存在用户记忆中，丢失则无法解密已推送的 Gist。

---

## 架构决策

- **Tauri 2（ADR v3）**：加权评分 48.5 胜 Electron 41 / Qt 36，详见 [`docs/计划/framework-choice-tauri-vs-qt-vs-electron.md`](./docs/计划/framework-choice-tauri-vs-qt-vs-electron.md)
- **MCP 内嵌 GUI + Streamable HTTP**：server 是 GUI 进程内 axum service，与 SSH 会话/资产/凭据同进程访问，无桥接复杂度；取代早期「双二进制 + stdio + named pipe」方案（根治僵尸进程/打包缺口），详见 `docs/architecture-log.md`
- **前端全量 TypeScript（v0.11.0）**：`vue-tsc` strict 检查进 `npm run build` 门禁
- **多窗口（v0.10.0）**：每窗口独立 webview/Pinia、共享 Rust 后端；会话迁移走 scrollback 序列化 + Rust 内存中转，不重连

---

## 版本演进

| 版本 | 重点 |
|------|------|
| v0.5.0 – v0.7.x | GUI 打磨期：全量 UI/UX 审计修复、私钥连接体验增强 |
| v0.8.0 | MCP 拦截等级可配置 + 执行命令日志 |
| v0.9.0 | 全新应用图标；MCP 文件传输全能力无桩化；凭据与托管私钥同步；文件面板与终端联动 |
| v0.10.0 | 资产独立工作台窗口 + 终端 tab 跨窗口迁移；GitHub Device Flow 登录 |
| v0.11.0 | 前端全量迁移 TypeScript（strict 门禁）；「靠猜测代替事实」全仓审计修复 18 项 |
| v0.12.0 | 安全判据迁入 core 真跑；MCP 配置与执行日志 fail-closed；全量写入原子化；推送乐观并发 |
| v0.13.0 | 文件列视图权限列与窄栏回退；右键菜单遮挡修复 |
| v0.14.0 | Gist 同步 v2.7 安全加固（备份主密码可恢复、重置不丢凭据、免密默认化、生成恢复密码）；资源监控与传输抽屉 FinalShell 式重设计；资产树分组快捷新增 |
| v0.15.0 | 远端非 UTF-8 文件名根治（vendored russh-sftp fork 可逆编解码）；监控解析层与安全判据迁入 core 独立测试（191 项）；前后端超标大文件按域拆分；终端快捷键与搜索框/输入框系列修复 |
| v0.15.1 | 设置中心 MCP 页重设计：控制面合成单面板（状态/接入/拦截），执行日志与能力清单限高内部滚动 |
| v0.15.2 | 应用内常驻更新日志（设置 → 关于与更新，构建时打包全历史离线可读）；文件面板新增属主/属组列；MCP 面板下拉菜单截断修复 |

---

## 已知限制与 Follow-ups

### MCP

- **会话不复用**：`ssh_exec`/`sftp_*` 工具当前直走 headless 独立建连，工具级会话复用仍为 follow-up
- **Claude Desktop 不直连**：不支持 `streamable-http`，需经 `mcp-remote` 桥接
- headless 建连不支持 MFA（仅密码/私钥/keyboard-interactive 密码类 prompt）
- MCP 隧道类工具未实现（隧道的创建/管理目前在 GUI）

### GUI

- 下载已流式化但**仍不可取消**（单次 invoke、后端无中断通道）
- 上传仍走前端 8 MiB 分块经 IPC；改为服务端按路径流式上传是后续优化
- `start_remote_forward` 是返回 Err 的桩（local/dynamic SOCKS5 已实现）；ProxyJump/跳板链未实现
- 多窗口：窗口不持久化恢复、资产删除不跨窗口同步、主题跨窗口不实时同步
- 终端 tab 迁移回放不含 alt 屏与软换行（vim/less 中拖出只还原进 alt 前内容）
- 超标文件拆分计划见 [`docs/architecture-log.md`](./docs/architecture-log.md) 的 Baseline snapshot

---

## License

MIT
