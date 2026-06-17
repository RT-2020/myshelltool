# myshelltool

> Windows 桌面 SSH 运维客户端 + **MCP Server**（AI 可调用）
>
> Rust + Tauri 2 构建。既是人用的图形客户端，也能被 Claude Desktop / Cursor / Cline 等 AI 工具通过 MCP 协议调用，让 AI 帮你执行 SSH 运维操作。

---

## 目录

- [它是什么](#它是什么)
- [功能特性](#功能特性)
- [快速开始](#快速开始)
- [GUI 使用](#gui-使用)
- [MCP Server 配置（让 AI 调用）](#mcp-server-配置让-ai-调用)
  - [Claude Desktop](#claude-desktop)
  - [Cursor](#cursor)
  - [Cline](#cline)
  - [可用能力一览](#可用能力一览)
  - [安全机制](#安全机制)
- [技术栈](#技术栈)
- [项目结构](#项目结构)
- [开发指南](#开发指南)
- [测试](#测试)
- [安全设计](#安全设计)
- [架构决策](#架构决策)
- [已知限制与 Follow-ups](#已知限制与-follow-ups)
- [License](#license)

---

## 它是什么

myshelltool 有**两种身份**：

1. **SSH 运维客户端**（给人用）— 图形界面管理多台 SSH 主机：连接、终端、文件传输、隧道、资源监控
2. **MCP Server**（给 AI 用）— 暴露 SSH 能力给 AI 工具，让 Claude/Cursor 等通过自然语言操作你的服务器

第二种身份是 v0.2.0 新增的**核心差异化能力**——市面上支持 MCP 的 SSH 运维工具极少。

```
┌─────────────────┐      stdio (JSON-RPC)      ┌──────────────────────┐
│ Claude Desktop  │ ────────────────────────── │  myshelltool-mcp.exe │
│ Cursor / Cline  │                            │  (MCP Server 进程)   │
└─────────────────┘                            └──────────┬───────────┘
                                                          │ russh SSH
                                                          ▼
                                               ┌──────────────────────┐
                                               │  你的服务器们         │
                                               │  (192.168.x.x ...)   │
                                               └──────────────────────┘
```

---

## 功能特性

### GUI 客户端

- **连接资产管理** — 多级嵌套分组（`/` 分隔）、标签、筛选、收藏，本地 JSON 持久化
- **SSH 终端** — 多标签会话、xterm.js、UTF-8 中文、自动 fit、深色/浅色主题
- **Host Key 验证** — 首次连接指纹确认、变更高危警告、known_hosts 持久化
- **多认证方式** — 密码、私钥（ed25519/RSA）、passphrase、keyboard-interactive
- **凭据安全存储** — 密码/passphrase 走本地 SecretStore，不进资产 JSON / 日志
- **SFTP 文件管理** — 远程浏览、上传/下载、新建目录、重命名、删除（带确认）
- **Monaco 远程编辑** — 语法高亮、Ctrl+S 保存、按扩展名识别语言
- **文件传输队列** — 分块传输、实时进度、失败重试
- **SSH 隧道** — Local forwarding、Dynamic SOCKS5 代理
- **资源监控** — 远程 CPU/内存/网络/磁盘实时图表

### MCP Server（v0.3.0+）

- **9 个 Tools** — AI 能查磁盘/系统/服务状态，高危命令走客户端内审批
- **4 个 Resources** — 资产/会话/known-hosts 可被 AI 随机读取
- **3 个 Prompts** — 诊断/安全审计/磁盘清理的结构化引导
- **Elicitation 审批**（v1.1）— 高危命令在 Claude/Cursor 界面内弹确认框（三段式），不再切到 myshelltool GUI
- **会话复用**（v1.1）— MCP 经 named pipe 复用 GUI 已建立的 SSH 会话，避免重连 + 二次 host key 验证；GUI 离线时自动降级独立建连
- **Host key 安全门** — MCP 仅服务已在 GUI 信任过的资产

---

## 快速开始

### 环境要求

- **Windows 10/11**（x64）
- [Node.js](https://nodejs.org/) 18+（含 npm/pnpm）
- [Rust](https://rustup.rs/) stable（含 cargo）
- [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)（Windows 11 自带）

### 安装与运行

```bash
# 1. 克隆
git clone https://github.com/RT-2020/myshelltool.git
cd myshelltool

# 2. 安装前端依赖
npm install

# 3. 开发模式启动（完整 SSH 功能）
npm run tauri:dev
```

首次启动会编译 Rust 后端（几分钟），之后增量编译很快。

### 从源码构建安装包

```bash
npm run tauri:build
# 产物：src-tauri/target/release/bundle/nsis/myshelltool_0.3.0_x64-setup.exe
```

---

## GUI 使用

### 配置第一台服务器

1. 启动后，左侧栏点击「新增连接资产」
2. 填写：名称、主机、端口（默认 22）、用户名、认证方式
3. 密码认证：输入密码（会加密存到本地 SecretStore）
4. 私钥认证：选择私钥文件路径，如有 passphrase 一并填写
5. 保存后双击资产连接

### Host Key 首次确认

首次连接某主机时，会弹出指纹确认框。核对 SHA256 指纹后点「信任」——之后该主机记录到 known_hosts，后续连接自动校验。**指纹变更会阻止连接并警告**（可能是中间人攻击）。

### 日常操作

| 操作 | 方式 |
|------|------|
| 打开终端 | 双击资产 / 右键「连接」 |
| 传输文件 | 连接后切到「文件」区域，拖拽上传 / 双击下载 |
| 远程编辑 | 文件区右键 → 「编辑」（Monaco 编辑器打开） |
| 开隧道 | 侧栏「隧道」→ 新增 → 选类型（local/SOCKS5） |
| 资源监控 | 连接后右侧「资源监控」面板自动刷新 |

---

## MCP Server 配置（让 AI 调用）

> 详细指南见 [`docs/mcp-setup.md`](./docs/mcp-setup.md)

### 前置：构建 MCP 二进制 + 配置资产

```bash
# 构建 MCP exe（debug，开发测试用）
npm run mcp:build

# 或构建 release 版（更小更快，推荐给 Claude 用）
npm run mcp:build:release
```

MCP exe 位置：
- debug：`src-tauri/target/debug/myshelltool-mcp.exe`
- release：`src-tauri/target/release/myshelltool-mcp.exe`

**资产配置**：MCP 读取与 GUI 相同的数据目录（`%APPDATA%\com.redtei.myshelltool\`）。**先用 GUI 配置好资产并完成首次连接（信任 host key）**，MCP 才能操作它们。

### Claude Desktop

编辑 `%APPDATA%\Claude\claude_desktop_config.json`：

```json
{
  "mcpServers": {
    "myshelltool": {
      "command": "D:\\path\\to\\myshelltool\\src-tauri\\target\\release\\myshelltool-mcp.exe",
      "args": []
    }
  }
}
```

**完全退出并重启 Claude Desktop**（托盘右键 Quit，不是关窗口）。然后问它：

> 「列出我的 myshelltool 资产」

Claude 会调用 `list_assets` 返回你配置的服务器列表。

### Cursor

项目级（`.cursor/mcp.json`）或全局（Settings → MCP）：

```json
{
  "mcpServers": {
    "myshelltool": {
      "command": "D:\\path\\to\\myshelltool-mcp.exe",
      "args": [],
      "type": "stdio"
    }
  }
}
```

### Cline

编辑 `%APPDATA%\Code\User\globalStorage\saoudrizwan.claude-dev\settings\cline_mcp_settings.json`，格式同 Claude Desktop。

> ⚠️ Cline 有 auto-approve 开关，但 **myshelltool 自带三层审批门仍生效**——高危命令不会因 auto-approve 绕过。

### 可用能力一览

**Tools（9 个）**

| 工具 | 说明 | 审批 |
|------|------|------|
| `list_assets` | 列出连接资产（脱敏）| 自动 |
| `list_sessions` | 活跃会话列表（v1.1 经 pipe 查 GUI 真实会话）| 自动 |
| `disk_usage` | 磁盘使用（df -h）| 自动 |
| `system_status` | uptime/内存/负载/top | 自动 |
| `service_status` | systemctl status | 自动 |
| `sftp_list` | SFTP 目录列表 | 自动 |
| `resource_monitor_snapshot` | 资源监控快照 | 自动 |
| `ssh_exec` | **任意命令** | 白名单自动 / 其余 elicitation 确认 |
| `sftp_remove` | **删除文件** | 始终 elicitation 确认 |

> v1.1 起，`ssh_exec`/`sftp_remove` 经 MCP elicitation 在客户端界面内弹确认框（三段式：AI 意图 + 真实命令 + 后果），用户 accept 才执行。客户端不支持 elicitation 时降级为拒绝。

**Resources（4 个）**：`myshelltool://assets` / `://sessions` / `://known-hosts` / `://sessions/{id}/log`

**Prompts（3 个）**：`diagnose_server`（健康诊断）/ `audit_security`（安全审计）/ `cleanup_disk`（磁盘清理）

### 安全机制

**审批分层**：
- **白名单**（df/uptime/systemctl status 等 20 条只读命令）→ 自动执行
- **其余命令**（黑名单 + 未知）→ 经 elicitation 在客户端界面内弹确认框，用户 accept 才执行
- 客户端不支持 elicitation 时降级为拒绝（fail-secure，宁可误拦不可漏放）

**三段式确认信息**——高危命令触发确认框时显示：
```
⚠️ 高危操作审批

【AI 声明意图】<AI 声称的意图>
【真实命令】<实际要执行的命令>
【后果预测】<该命令的后果说明>

确认要执行此操作吗？
```
通过三段对照可识破 AI 伪装（如 intent 说「查看日志」但 command 是 `rm -rf /var/log`）。降级拒绝时返回同结构文本。

**会话复用安全**（v1.1）：MCP 经 named pipe `\\.\pipe\myshelltool-mcp` 复用 GUI 会话，仅限同用户会话内的 GUI 进程；pipe 不可用时自动降级独立建连（仍受 host key 门约束）。

**Host key 安全门（D4）**：MCP 仅服务已在 GUI 信任过的资产，未知主机直接拒绝。

---

## 技术栈

| 层 | 技术 | 版本 |
|---|---|---|
| 桌面框架 | **Tauri 2** | `@tauri-apps/cli ^2.9.5` |
| 后端 | **Rust** | `russh 0.49`、`russh-sftp 2.x`、`tokio`、`rmcp 1.7`（MCP） |
| 前端框架 | **Vue 3**（`<script setup>` + Composition API）| `vue ^3.5.38` |
| 状态管理 | **Pinia 3**（setup store）| `pinia ^3.0.4` |
| 图标 | lucide-vue-next | `^0.460.0` |
| 终端 | xterm.js 6 + addon-fit/search/web-links/webgl | `@xterm/xterm ^6` |
| 远程编辑 | Monaco Editor 0.52（CDN）| — |
| MCP SDK | rmcp（官方 Rust SDK）| `~1.7` |
| 样式 | SCSS + 设计 token 系统（无 Tailwind）| `sass ^1.101` |
| 构建 | Vite 7（root=`src/`）| `vite ^7.2.7` |
| 测试 | Playwright（UI smoke）+ cargo test（core）| `playwright ^1.60` |

---

## 项目结构

```
myshelltool/
├── src/                          # 前端（Vite root）
│   ├── index.html                # 主页面
│   ├── main.js                   # 应用入口
│   ├── App.vue                   # 根组件：5 区域布局
│   ├── components/
│   │   ├── shell/                # 外壳布局 / 侧栏 / 弹窗中枢
│   │   ├── terminal/             # 终端 surface / tabs / toolbar
│   │   ├── files/                # 文件管理 surface / columns
│   │   ├── resource-monitor/     # CPU/内存/网络/磁盘图表
│   │   └── ui/                   # 基础组件库（App* 命名）
│   ├── stores/                   # Pinia：6 领域 store + workbench 编排壳
│   ├── composables/              # useTheme/useClipboard/...
│   ├── lib/                      # terminalController/dangerousCommands
│   ├── services/backend.js       # Tauri IPC 桥
│   └── styles/                   # SCSS：_tokens/_base/_utilities
├── src-tauri/                    # Tauri/Rust 后端
│   ├── Cargo.toml                # 含双 [[bin]]（GUI + MCP）
│   └── src/
│       ├── main.rs               # GUI 入口
│       ├── lib.rs                # AppState + 命令注册 + MCP 入口
│       ├── bin/mcp.rs            # MCP console 子系统入口
│       ├── ssh.rs                # SSH/SFTP/隧道 + headless 会话
│       ├── dangerous_commands.rs # 危险命令检测（GUI+MCP 共享）
│       ├── mcp/                  # MCP server 模块
│       │   ├── server.rs         # rmcp stdio server + ServerHandler
│       │   ├── tools.rs          # 9 个 Tools
│       │   ├── resources.rs      # 4 个 Resources
│       │   ├── prompts.rs        # 3 个 Prompts
│       │   └── approval.rs       # 三层审批 + 三段式拒绝
│       ├── resource_monitor.rs   # 远程资源轮询
│       └── fs_local.rs           # 本地文件系统命令
├── crates/myshelltool-core/      # 共享核心库（资产/凭据/校验）
├── scripts/mcp-dev-watch.mjs     # MCP 开发热重建监听
├── docs/                         # 文档
│   ├── mcp-setup.md              # MCP 配置详细指南
│   ├── specs/                    # 需求规格（deep-interview 产出）
│   ├── plans/                    # 实施计划
│   └── interviews/               # 访谈记录
├── tests/                        # UI 冒烟测试
├── AGENTS.md                     # AI Agent 项目上下文（单一信息源）
└── package.json
```

---

## 开发指南

### 常用命令

```bash
npm install              # 安装前端依赖
npm run tauri:dev        # GUI 开发模式（完整功能，热重载）
npm run dev:all          # GUI + MCP 同时开发（MCP 源码变化自动重建）

npm run mcp:build        # 单独构建 MCP exe（debug）
npm run mcp:build:release # 构建 release MCP exe（给 Claude 用）
npm run mcp:watch        # 仅 MCP 热重建（GUI 单独跑）

npm run build            # 前端 Vite 构建（验证编译）
npm run tauri:build      # 完整桌面安装包（NSIS）
```

### 开发工作流

**日常开发（GUI + MCP）**：
```bash
npm run dev:all
# 一条命令：启动 GUI + 监听 MCP 源码自动重建
# 改 MCP 代码后自动重建，若 GUI 锁住文件会提示重启 GUI
```

**只改前端**：
```bash
npm run tauri:dev    # GUI 热重载
```

**只改 MCP**：
```bash
# 终端 1
npm run tauri:dev
# 终端 2
npm run mcp:watch
```

> ⚠️ Windows 下 GUI 和 MCP 共享 `myshelltool_lib`（cdylib），GUI 运行时会锁住该文件，导致 MCP 无法重建。解决：先停 GUI 再重建 MCP，或用 `dev:all`（自动检测并提示）。

### 编码约定

本项目遵循 [`AGENTS.md`](./AGENTS.md) 的编码约定（AI Coding Agent 的单一信息源）：

- **前端**：Vue 3 `<script setup>` + Composition API，Pinia setup store，SCSS 设计 token（颜色/间距用 `var(--xxx)`，不硬编码）
- **后端**：所有 Tauri 命令用 `State<'_, AppState>` 统一解析，命令参数 camelCase，新增命令需注册到 `generate_handler!`
- **图标**：统一用 `lucide-vue-next`，不引入其他图标库
- **路径别名**：`@` → `src/`

详见 [`AGENTS.md`](./AGENTS.md)。

---

## 测试

```bash
npm run test:core    # Rust core 单元测试（资产/凭据/校验/危险命令检测）
npm run test:ui      # UI 冒烟测试（需先 npm run dev 起服务）
cd src-tauri && cargo check   # Rust 类型检查（Windows build 兜底）
```

---

## 安全设计

- **凭据隔离**：密码/私钥/passphrase 只走本地 SecretStore，不进资产 JSON / 日志 / 错误信息
- **Host key 强制验证**：首次连接必须人工确认指纹，变更阻止连接并警告
- **危险操作确认**：删除/覆盖/批量操作必须弹窗确认
- **MCP 三层审批**：白名单自动 / 黑名单拒绝 / 未知拒绝（fail-secure）
- **MCP host key 门**：仅服务已信任资产，headless 模式不弹窗不自动信任

### 残余风险（已知，已留档）

- **提示词注入**（行业级未解难题）：AI 可能被远程内容诱导执行恶意命令。v0.2.0 靠三段式拒绝让人工识破，无技术拦截。
- **凭据存储强度**：当前 SecretStore 是弱 XOR 混淆（非加密），后续计划升级到 Windows DPAPI。

---

## 架构决策

### ADR v3：框架选型（继续 Tauri）

详见 `.omc/plans/framework-choice-tauri-vs-qt-vs-electron.md`。

**决策**：继续使用 Tauri 2.x，不切换 Qt 或 Electron。加权评分 Tauri 48.5 > Electron 41 > Qt 36。

### MCP 架构：双二进制 + 会话复用（v0.3.0）

详见 `docs/specs/MCP服务接入-需求规格.md`。

**决策**：
- **双二进制**：`myshelltool.exe`（GUI，windows 子系统）+ `myshelltool-mcp.exe`（console 子系统），共享 `myshelltool_lib`。因 `windows_subsystem` 是链接时属性，无法运行时切换。
- **stdio 传输**：MCP 被 Claude Desktop 拉起为子进程，通过 stdin/stdout 收发 JSON-RPC。
- **v0.3.0 会话复用**：MCP 优先经 named pipe `\\.\pipe\myshelltool-mcp` 复用 GUI 已建立的 SSH 会话（免重连 + 二次 host key 验证）；GUI 离线时自动降级独立 headless 建连。
- **v0.3.0 elicitation 审批**：高危命令经 MCP elicitation 在客户端界面内弹确认框，不切到 myshelltool GUI。

---

## 已知限制与 Follow-ups

### MCP（v0.3.0）

- headless 降级模式不支持 MFA（仅密码/私钥/keyboard-interactive 密码类 prompt）
- sftp_upload / 隧道工具未实现（后续版本）
- elicitation 确认框的客户端兼容性：Claude Desktop 支持，部分轻量客户端可能降级为拒绝

### GUI

- `sftp_download_with_progress` 仍返回整块 `Vec<u8>`（upload 已分块）
- `start_remote_forward` 是桩函数（local/dynamic SOCKS5 已实现）
- GitHub/Git 资产同步未实现（规划中的差异化功能）
- ProxyJump/跳板链未实现

---

## License

MIT
