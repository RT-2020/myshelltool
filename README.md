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
- [MCP Server 配置（让 AI 调用）](#mcp-server-配置让-ai-调用)
  - [Claude Code](#claude-code)
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
- [版本演进](#版本演进)
- [已知限制与 Follow-ups](#已知限制与-follow-ups)
- [License](#license)

---

## 它是什么

myshelltool 有**两种身份**：

1. **SSH 运维客户端**（给人用）— 图形界面管理多台 SSH 主机：连接、终端、文件传输、隧道、资源监控
2. **MCP Server**（给 AI 用）— 暴露 SSH 能力给 AI 工具，让 Claude/Cursor 等通过自然语言操作你的服务器

第二种身份是**核心差异化能力**——市面上支持 MCP 的 SSH 运维工具极少。

```
┌─────────────────┐   Streamable HTTP       ┌──────────────────────────┐
│ Claude Code     │   (JSON-RPC over HTTP)  │  myshelltool.exe (GUI)   │
│ Cursor / Cline  │ ──────────────────────→ │  内嵌 MCP server         │
└─────────────────┘                         │  127.0.0.1:41235/mcp     │
                                            └────────────┬─────────────┘
                                                         │ russh SSH（同进程）
                                                         ▼
                                            ┌──────────────────────────┐
                                            │  你的服务器们             │
                                            │  (192.168.x.x ...)       │
                                            └──────────────────────────┘
```

> MCP server **内嵌 GUI 进程**（Streamable HTTP transport），没有独立 exe。详见 [架构决策](#架构决策)。

---

## 功能特性

### GUI 客户端

- **连接资产管理** — 多级嵌套分组（`/` 分隔）、标签、筛选、收藏，本地 JSON 持久化；支持 `user@host:port` 连接串快速解析（含 IPv6 括号形式）
- **SSH 终端** — 多标签会话、xterm.js、UTF-8 中文、自动 fit、深色/浅色主题、自动重连（带稳定窗口判定，不无限重试）
- **多窗口工作台** — 资产可拖出/右键「在独立窗口打开」为独立 OS 窗口（上终端/下文件/右栏监控）；已连接的终端 tab 可跨窗口拖出迁移，scrollback 带颜色样式回放，也可一键「移回主窗口」
- **Host Key 验证** — 首次连接指纹确认、变更高危警告、known_hosts 持久化
- **多认证方式** — 密码、私钥（ed25519/RSA，文件选择器选取）、passphrase、keyboard-interactive
- **凭据安全存储** — 密码/passphrase 走本地 SecretStore（Windows DPAPI），不进资产 JSON / 日志
- **SFTP 文件管理** — 远程浏览、上传/下载、新建目录、重命名、删除（带确认）；列视图含权限列与窄栏自适应；支持 UNC 路径面包屑
- **Monaco 远程编辑** — 语法高亮、Ctrl+S 保存、按扩展名识别语言
- **文件传输队列** — 分块传输、实时进度、失败重试、上传后字节对账
- **终端与文件面板联动** — 连接后自动加载远程目录；终端 `cd` 经 OSC 7 跟随切换文件面板目录（注入行全程不可见）
- **SSH 隧道** — Local forwarding、Dynamic SOCKS5 代理
- **资源监控** — 远程 CPU/内存/网络/磁盘实时图表；远端无 `/proc`（非 Linux）时展示明确错误态，不发假数据
- **应用内自动更新** — 内置 updater，新版本自动检测

### Gist 资产同步（跨机器）

- **跨机器同步连接资产** — 把 `connection-assets.json`（连接元数据）加密后推送到 GitHub Gist，换机器/重装时一键拉回
- **GitHub OAuth Device Flow 登录** — 显示一次性代码到浏览器授权即可，无需手动创建/粘贴 PAT（手动粘贴保留为兜底）
- **可选凭据同步** — 默认只同步资产拓扑（host/port/用户名等，不含密码）；可开启「同步登录密码与托管私钥」，端到端加密后一并同步，换机后一键直连
- **端到端加密** — 用户主密码经 Argon2id 派生密钥 + AES-256-GCM 加密，GitHub 服务端只见密文
- **乐观并发检查** — 推送前比对远端版本，两台机器并行改动时中止盲推并提示先拉取，不会静默互相覆盖
- **冲突检测** — 双向同步时检测本地/远端都改动，弹框让用户选本地覆盖 / 远端覆盖

### MCP Server

- **内嵌 GUI 进程** — MCP server 是 GUI 进程内的 axum HTTP service（Streamable HTTP transport），随 GUI 启停，**没有独立 exe**。默认监听 `127.0.0.1:41235/mcp`
- **13 个 Tools** — 7 个系统类（资产/会话/磁盘/系统/服务/资源监控/命令执行）+ 6 个文件类（list/read/write/upload/download/remove），文件传输全能力无桩化
- **4 个 Resources / 3 个 Prompts** — 资产/会话/known-hosts 随机读取；诊断/安全审计/磁盘清理结构化引导
- **拦截等级可配置** — Minimal（默认，仅硬拦毁灭性命令）/ Strict（非白名单一律人工确认），GUI 的 MCP 面板一键切换
- **审批三级降级** — 高危操作先走 MCP elicitation（客户端界面内弹确认框）→ 客户端不支持时降级为 GUI 弹窗审批（60s 超时）→ 均不可用才 fail-secure 拒绝
- **执行审计日志** — 真实触发远程执行的工具调用记一条日志（哪台服务器/什么命令[已脱敏]/什么决策/什么结果），GUI 内可查看/搜索/清空
- **Host key 安全门** — MCP 仅服务已在 GUI 信任过的资产

---

## 快速开始

### 环境要求

- **Windows 10/11**（x64）
- [Node.js](https://nodejs.org/) 20.19+ 或 22.12+（推荐 22/24，使用 npm；Vite 7 的 engines 要求 `^20.19.0 || >=22.12.0`）
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
# 产物：src-tauri/target/release/bundle/nsis/myshelltool_0.13.0_x64-setup.exe
```

---

## GUI 使用

### 配置第一台服务器

1. 启动后，左侧栏点击「+ 新增连接」（快捷键 `+`）
2. 填写：名称、主机、端口（默认 22）、用户名、认证方式
3. 密码认证：输入密码（会加密存到本地 SecretStore）
4. 私钥认证：通过文件选择器选择私钥文件路径，如有 passphrase 一并填写
5. 保存后双击资产连接

### Host Key 首次确认

首次连接某主机时，会弹出指纹确认框。核对 SHA256 指纹后点「信任」——之后该主机记录到 known_hosts，后续连接自动校验。**指纹变更会阻止连接并警告**（可能是中间人攻击）。

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

1. 打开「设置 → 同步」或资产同步面板的 GitHub 授权区块，点「GitHub 账号登录」
2. 界面显示一次性验证码，点「打开授权页」跳转 `github.com/login/device`
3. 在浏览器输入验证码并授权——完成，token 自动存入本地 SecretStore（DPAPI 保护）

> 登录**只需一次**：token 持久保存在本机安全存储里，重启应用/重新打开面板都会显示「已登录」，不会再催你登录（想换账号才点「重新登录」）。

**兜底：手动粘贴 PAT**：到 GitHub → Settings → Developer settings → Personal access tokens 新建一个**只需 Gist 读写权限**的 token，粘贴到 PAT 配置卡片保存。

### 第二步：配置同步

1. 输入主密码（≥6 位，用于加密同步载荷）→ 点「初始化」
   - 会在你的 GitHub 自动创建一个 secret Gist，gist_id 记录到本地 `sync-state.json`
   - **主密码只输这一次**：初始化后本机用 DPAPI 记住派生密钥，之后 push/pull 与自动同步都不再需要输入
2. **换机器拉取已有同步**：登录同一 GitHub 账号 + 输入**原主密码** + 填入已有 `gist_id` → 点「初始化」，本地资产会被远端覆盖（同样只输这一次）

### 日常使用

| 操作 | 说明 |
|------|------|
| **推送（Push）** | 把当前本地资产加密推送到 Gist（已启用免密时无需输密码）。推送前会与远端版本比对，若远端已有新改动则中止并提示先拉取（防两台机器互相覆盖） |
| **拉取（Pull）** | 从 Gist 拉取并解密，**覆盖本地**资产（已启用免密时无需输密码）。若开启了凭据同步，恢复失败的凭据会逐项计数提示 |
| **自动同步** | 开启后（初始化时自动开启），资产增删改会自动推送；本机免密（DPAPI 记住派生密钥，离机即失效）。换机/重装用主密码恢复 |
| **凭据与私钥同步** | 可选开关。开启后登录密码与托管私钥经主密码加密一并同步，换机后一键直连；关闭则仅同步资产拓扑 |
| **冲突解决** | 若本地和远端相对上次同步都有改动，会弹框显示双方摘要，让你选「用本地覆盖远端」或「用远端覆盖本地」 |
| **重置主密码** | 需验证旧密码，新密码立即用于重新加密**整份保管库（含凭据）**并推送 |
| **清空同步** | 删除本地 sync-state（**不会**删 GitHub 上的 Gist，需手动到 GitHub 删除） |

> ⚠️ **主密码丢失无法找回**：它是解密云端备份的唯一凭据，忘了就无法解密已推送的 Gist。
> 不想自己设密码？在同步页点「生成强密码」——应用会生成一份高熵随机密码并**保存在本机**，
> 随时可在「设置与高级 → 换机恢复 → 查看这台电脑的恢复密码」查看或复制带走（你自己手输的密码不会被保存）。

### 安全模型

- **加密**：Argon2id（从主密码 + 随机 salt 派生 256-bit 密钥）+ AES-256-GCM（认证加密，防篡改）。每次推送用新的随机 salt 和 nonce，相同内容每次密文都不同。
- **默认不同步的内容**：密码、passphrase、私钥、GitHub token —— 这些只在本地 SecretStore。开启凭据同步后，密码/托管私钥也经主密码端到端加密后才上传。
- **同步的内容**：`connection-assets.json`（名称、host、port、用户名、分组、标签、认证方式**类型**、私钥路径）+ 可选的加密凭据载荷。私钥文件本身（非托管场景）不同步，只同步路径。
- **GitHub 侧**：Gist 设为 secret（不公开列出，但**有 gist_id 的人仍可访问**），且内容是密文，泄露也无法直接解密。
- **失败安全**：本地资产文件损坏时中止同步并报错，绝不把残缺数据当备份推上去；凭据读取失败时中止推送（宁可少推，不用残缺备份覆盖完整备份）。

---

## MCP Server 配置（让 AI 调用）

> 详细指南见 [`docs/mcp-setup.md`](./docs/mcp-setup.md)

### 前置：启动 GUI + 配置资产

MCP server **内嵌 GUI 进程**（Streamable HTTP transport），**无需单独构建 MCP exe**。

1. **启动 myshelltool GUI**——MCP server 随 GUI 启停，GUI 没开就没有 MCP。
2. **配置资产**：MCP 读取与 GUI 相同的数据目录（`%APPDATA%\com.redtei.myshelltool\`）。**先用 GUI 配置好资产并完成首次连接（信任 host key）**，MCP 才能操作它们。
3. **查看 endpoint URL**：GUI 的「MCP」面板会显示实际监听地址（默认 `http://127.0.0.1:41235/mcp`，端口被占用会 +1）。配置客户端时用面板显示的 URL。面板上还可以切换**拦截等级**、查看/清空**执行日志**。

### Claude Code

Claude Code 原生支持 `streamable-http` transport。编辑项目级或全局 MCP 配置：

```json
{
  "mcpServers": {
    "myshelltool": {
      "url": "http://127.0.0.1:41235/mcp"
    }
  }
}
```

> **url 必须用 GUI MCP 面板显示的实际 endpoint**（端口可能不是 41235）。确保 GUI 正在运行。

重启 Claude Code 后，问它：

> 「列出我的 myshelltool 资产」

Claude 会调用 `list_assets` 返回你配置的服务器列表。

### Cursor

项目级（`.cursor/mcp.json`）或全局（Settings → MCP）：

```json
{
  "mcpServers": {
    "myshelltool": {
      "url": "http://127.0.0.1:41235/mcp"
    }
  }
}
```

### Cline

编辑 `%APPDATA%\Code\User\globalStorage\saoudrizwan.claude-dev\settings\cline_mcp_settings.json`，格式同 Claude Code（`mcpServers` → `url`）。

> ⚠️ Cline 有 auto-approve 开关，但 **myshelltool 自带审批门仍生效**——毁灭性命令两档恒拒，Strict 档高危命令不会因 auto-approve 绕过。

### Claude Desktop（⚠️ 不直接支持）

> ⚠️ Claude Desktop 不直接支持 `streamable-http` transport（仅支持 stdio + sse）。无法直连，社区方案经 [`mcp-remote`](https://www.npmjs.com/package/mcp-remote) 做 stdio→HTTP 代理（依赖 npx/node）。详见 [`docs/mcp-setup.md`](./docs/mcp-setup.md)。

### 可用能力一览

**Tools（13 个 = 7 系统类 + 6 文件类）**

| 工具 | 说明 | 审批 |
|------|------|------|
| `list_assets` | 列出连接资产（脱敏）| 自动 |
| `list_sessions` | 活跃 SSH 会话清单（含 GUI 已建立的会话，可复用目标）| 自动 |
| `disk_usage` | 磁盘使用（df -h），含逐段退出码回声 | 自动 |
| `system_status` | uptime/内存/负载/top，仅 Linux 远端 | 自动 |
| `service_status` | systemctl status，仅 systemd 远端 | 自动 |
| `resource_monitor_snapshot` | CPU/内存/磁盘资源快照 | 自动 |
| `ssh_exec` | **任意 Shell 命令** | 按拦截等级；毁灭性命令两档恒拒 |
| `sftp_list` | 远程目录列表 | 按拦截等级 |
| `sftp_read_file` | 读远程文本文件（二进制/非 UTF-8 明确拒绝，不猜编码）| 按拦截等级；**敏感凭据文件（如私钥）恒需审批** |
| `sftp_write_file` | 写远程文本文件（原子写，失败保留原文件）| 按拦截等级；本机核心系统目录写恒拒 |
| `sftp_upload` | 上传本地文件到远程（流式 + SHA256 校验）| 按拦截等级；本机核心系统目录写恒拒 |
| `sftp_download` | 下载远程文件到本机（流式 + SHA256 校验）| 按拦截等级 |
| `sftp_remove` | 删除远程文件/目录 | 按拦截等级；**根级/核心目录删除两档恒拒** |

**拦截等级**（GUI MCP 面板可切换，改档对已连接的 AI 会话下次调用即生效）：

| 等级 | 行为 |
|------|------|
| **Minimal**（默认） | 仅硬拦毁灭性命令（`rm -rf /`、mkfs、dd 写块设备、fork 炸弹等），其余命令/文件操作直接执行（记执行日志供审计） |
| **Strict** | 非白名单一律人工确认；毁灭性命令同样恒拒 |

**Resources（4 个）**：`myshelltool://assets` / `://sessions` / `://known-hosts` / `://sessions/{id}/log`

**Prompts（3 个）**：`diagnose_server`（健康诊断）/ `audit_security`（安全审计）/ `cleanup_disk`（磁盘清理）

### 安全机制

**审批判定**：

- 只读白名单（df/uptime/systemctl status 等）按 shell 词法**逐段**判定，全部命中才自动放行——`df -h; cat /etc/shadow` 这类拼接/管道夹带不会因前缀像白名单而漏过；含重定向或命令替换一律不进白名单
- 毁灭性命令（两档恒拒）：不弹审批直接拒绝，决策记 `hard_blocked`，命令未执行
- 其余按拦截等级：Minimal 记日志放行 / Strict 人工确认
- **审批三级降级**：elicitation（客户端界面内弹确认框）→ GUI 弹窗（AppHandle emit，60s 超时）→ fail-secure 拒绝（宁可误拦不可漏放）

**三段式确认信息**——高危命令触发确认框时显示：
```
⚠️ 高危操作审批

【AI 声明意图】<AI 声称的意图>
【真实命令】<实际要执行的命令>
【后果预测】<该命令的后果说明>

确认要执行此操作吗？
```
通过三段对照可识破 AI 伪装（如 intent 说「查看日志」但 command 是 `rm -rf /var/log`）。

**执行日志**：真实触发远程执行的工具调用记一条日志（时间/资产/命令[口令已脱敏]/决策/输出摘要），GUI 的 MCP 面板可查看、搜索、清空。

**Host key 安全门**：MCP 仅服务已在 GUI 信任过的资产，未知主机直接拒绝。

---

## 技术栈

| 层 | 技术 | 版本 |
|---|---|---|
| 桌面框架 | **Tauri 2** | `@tauri-apps/cli ^2.9.5` |
| 后端 | **Rust** | `russh 0.49`、`russh-sftp 2.x`、`tokio`、`axum 0.8`（MCP HTTP）、`reqwest 0.12`（Gist 同步/Device Flow） |
| 前端框架 | **Vue 3**（`<script setup>` + Composition API）| `vue ^3.5.38` |
| 语言 | **TypeScript strict 全量**（vue-tsc 类型门禁）| `typescript ^5.9.3` + `vue-tsc ^3.3.11` |
| 状态管理 | **Pinia 3**（setup store）| `pinia ^3.0.4` |
| 图标 | lucide-vue-next | `^0.460.0` |
| 终端 | xterm.js 6 + addon-fit/search/**serialize**/web-links/webgl | `@xterm/xterm ^6` |
| 远程编辑 | Monaco Editor 0.52（CDN）| — |
| MCP SDK | rmcp（官方 Rust SDK，Streamable HTTP transport + elicitation）| `~1.7` |
| HTTP 框架 | axum（MCP Streamable HTTP server）+ tokio-util（CancellationToken）| `axum 0.8` / `tokio-util 0.7` |
| 样式 | SCSS + 设计 token 系统（无 Tailwind）| `sass ^1.101` |
| 构建 | Vite 7（root=`src/`）| `vite ^7.2.7` |
| 测试 | Playwright（UI smoke）+ cargo test（core）| `playwright ^1.60` |

---

## 项目结构

```
myshelltool/
├── src/                          # 前端（Vite root，全量 TypeScript）
│   ├── index.html                # 主页面
│   ├── main.ts                   # 应用入口
│   ├── App.vue                   # 根组件：按 query 分支（主工作台 / 资产独立窗口）+ 启动加载
│   ├── components/
│   │   ├── shell/                # 外壳：侧栏 / 弹窗中枢 / MCP 面板 / 同步面板 / 右栏
│   │   ├── workbench/            # WorkbenchShell（标题栏/状态栏）/ AssetWindowShell（独立窗口壳）
│   │   ├── terminal/             # 终端 surface / tabs / toolbar
│   │   ├── files/                # 文件管理 surface / columns
│   │   ├── resource-monitor/     # CPU/内存/网络/磁盘图表
│   │   └── ui/                   # 基础组件库（App* 命名，barrel 导出）
│   ├── stores/                   # Pinia：7 领域 store + mcp/sync store + workbench 编排壳
│   ├── composables/              # useTheme/useAutoUpdate/useAutoReconnect/useGithubDeviceLogin/...
│   ├── lib/                      # 纯函数模块：dangerousCommands/terminalGuards/sessionHandoff/
│   │                             #   assetWindows+boot/parseSshTarget/transferUtils/...
│   ├── services/backend.ts       # Tauri IPC 桥（invokeBackend/listenBackendEvent/normalize*）
│   ├── types/                    # tauri.d.ts（Tauri 类型 shim）+ domain.ts（共享领域类型）
│   └── styles/                   # SCSS：_tokens/_base/_utilities
├── src-tauri/                    # Tauri/Rust 后端（单 binary）
│   └── src/
│       ├── main.rs               # GUI 入口（tauri::run 壳）
│       ├── lib.rs                # AppState + 资产/凭据命令 + 命令注册 + MCP HTTP server 拉起
│       ├── ssh.rs                # SSH/SFTP/隧道 + headless 会话 + 会话中转（tab 迁移）
│       ├── resource_monitor.rs   # 远程资源轮询（失败态 emit，不发假快照）
│       ├── fs_local.rs           # 本地文件系统命令（系统目录黑名单/UNC 加固）
│       ├── sync.rs               # Gist 同步命令层（push/pull/conflict/Device Flow）
│       ├── sync_oauth.rs         # GitHub OAuth Device Flow 登录
│       ├── dpapi_codec.rs        # DPAPI 凭据编解码（Windows CryptProtectData）
│       └── mcp/                  # MCP server 模块（内嵌 GUI / Streamable HTTP）
│           ├── http_server.rs    # axum + rmcp Streamable HTTP server（绑 127.0.0.1:41235/mcp）
│           ├── probe.rs          # HTTP 健康检查（向自己的 endpoint 发 initialize 握手）
│           ├── server.rs         # rmcp ServerHandler 实现（13 工具/4 资源/3 prompts）
│           ├── tools.rs          # 系统类工具 + exec_on_asset + 执行日志脱敏
│           ├── file_tools.rs     # 文件类工具（list/read/write/upload/download/remove）
│           ├── file_policy.rs    # 文件操作安全策略（路径防穿越、敏感凭据判定）
│           ├── sftp_ops.rs       # Headless SFTP 底层封装（流式传输、原子写、递归删）
│           ├── approval.rs       # 审批：等级判定 / elicitation / GUI 弹窗降级 / fail-secure
│           ├── resources.rs      # 4 个 Resources
│           └── prompts.rs        # 3 个 Prompts
├── crates/myshelltool-core/      # 共享核心库（无 Tauri 依赖，可独立 cargo test）
│   └── src/
│       ├── lib.rs                # ConnectionAsset / SecretStore / 资产持久化（原子写）/ 校验
│       ├── dangerous_commands.rs # 白/黄/黑/Unknown 四层命令分类 + 毁灭层（GUI 与 MCP 共享）
│       ├── shell.rs              # shell 命令分段器（引号感知，安检的词法判据）
│       ├── redact.rs             # 命令文本脱敏（口令落日志前的红线）
│       └── remote_text.rs        # 远端文件内容编码三态判定（二进制/非 UTF-8/UTF-8）
├── tests/                        # UI 测试（Playwright）：冒烟 / host-key / 文件加载
├── scripts/fact-guards.mjs       # 「靠猜测代替事实」机械门禁（build/CI 强制）
├── docs/                         # 文档（mcp-setup / 架构日志 / 工程指南 / specs / plans）
├── AGENTS.md                     # AI Agent 项目上下文（单一信息源）
└── package.json
```

---

## 开发指南

### 常用命令

```bash
npm install              # 安装前端依赖
npm run tauri:dev        # GUI 开发模式（完整功能，热重载）。MCP server 随 GUI 启停

npm run lint:facts       # 「靠猜测代替事实」事实门禁（build 首步强制）
npm run type-check       # vue-tsc strict 全量类型检查
npm run build            # 事实门禁 → 类型检查 → Vite 构建
npm run tauri:build      # 完整桌面安装包（NSIS，自动含上述门禁）
```

### 开发工作流

**日常开发**：
```bash
npm run tauri:dev
# 一条命令：启动 GUI + 内嵌 MCP server（HTTP 随 GUI 拉起）
```

MCP server 内嵌 GUI 进程，没有独立 exe / named pipe / dev watch 脚本。改 MCP 代码就是改 Rust 代码（`src-tauri/src/mcp/*`），与改其他后端代码无差别。

### 编码约定

本项目遵循 [`AGENTS.md`](./AGENTS.md) 的编码约定（AI Coding Agent 的单一信息源），要点：

- **前端**：Vue 3 `<script setup lang="ts">` + Composition API，TypeScript strict，Pinia setup store，SCSS 设计 token（颜色/间距/z-index 一律 `var(--xxx)`，不硬编码）
- **后端**：所有 Tauri 命令用 `State<'_, AppState>` 统一解析，命令参数 camelCase，新增命令需注册到 `generate_handler!`；持久化逻辑放 `crates/myshelltool-core`（可独立单测）
- **质量红线**：不重复造轮子、无空 catch、无 `window.alert`、不硬编码样式、凭据只走 SecretStore、对外部环境不猜测（协议求证 + `LC_ALL=C` + 三态探测）、失败一律 fail-closed
- **图标**：统一用 `lucide-vue-next`；**路径别名**：`@` → `src/`

详见 [`AGENTS.md`](./AGENTS.md) 与 [`docs/llm-engineering-guidelines.md`](./docs/llm-engineering-guidelines.md)。

---

## 测试

```bash
npm run test:core    # Rust core 单元测试（资产/凭据/危险命令分类/命令分段/脱敏/编码判定/原子写，128+ 用例）
npm run test:ui      # UI 测试：ui-smoke + ui-host-key + ui-file-loading（需先 npm run dev 起服务）
npm run lint:facts   # 事实门禁（14 条规则）
npm run type-check   # vue-tsc strict 类型检查
cd src-tauri && cargo check   # Rust 类型检查（Windows 上 build 偶受 build script 阻断时的兜底）
```

> 安全判据（白名单绕过/命令分段/脱敏/UNC 等）必须在 `npm run test:core` 里真跑——src-tauri 的测试二进制受 Tauri runtime DLL 限制跑不起来，用 `cargo check --tests` 验证可编译。

---

## 安全设计

- **凭据隔离**：密码/私钥/passphrase 只走本地 SecretStore（Windows DPAPI，`CryptProtectData`），不进资产 JSON / 日志 / 错误信息。旧 `.cred` 文件首次读取时自动懒迁移到 DPAPI 格式，迁移后只能用当前 Windows 账户解密。旧格式为弱 XOR 混淆，仅兼容未迁移场景。
- **Host key 强制验证**：首次连接必须人工确认指纹，变更阻止连接并警告
- **危险操作确认**：删除/覆盖/批量操作必须弹窗确认
- **MCP 拦截等级**：Minimal（默认，仅拦毁灭性命令）/ Strict（非白名单一律确认）；毁灭性命令两档恒拒。等级存盘（`mcp-config.json`），配置文件损坏时 fail-closed 到 Strict 并隔离损坏文件，绝不静默降级到宽松档
- **白名单逐段判定**：命令按 shell 词法分段（引号感知）后逐段过白名单，防 `df -h; cat /etc/shadow` 式前缀伪装；重定向/命令替换不进白名单
- **命令脱敏**：命令文本落执行日志前经 `redact_command` 遮蔽口令（`mysql -pP@ss`、`curl -u user:pass` 等）；GUI 审批弹窗仍显示原命令（透明性优先）
- **原子写**：资产 JSON、凭据、同步状态、MCP 配置/日志全部走同目录临时文件 + rename 原子替换，失败保留原文件，杜绝半截写入
- **远端内容不猜编码**：MCP `sftp_read_file` 对二进制/非 UTF-8 内容明确拒绝并引导下载，不返回「看起来正常实则乱码」的内容
- **MCP host key 门**：仅服务已信任资产，headless 模式不弹窗不自动信任

### 残余风险（已知，已留档）

- **提示词注入**（行业级未解难题）：AI 可能被远程内容诱导执行恶意命令。靠三段式审批让人工识破 + 拦截等级兜底，无技术级完全拦截。
- **DPAPI 绑定 Windows 账户**：凭据用 DPAPI 加密，同机换账户、或把 `%APPDATA%\com.redtei.myshelltool\credentials\` 拷到别的机器，都无法解密，需重新输入密码。
- **Gist 主密码无法找回**：资产同步的主密码只存在用户记忆中，丢失则无法解密已推送的 Gist。

---

## 架构决策

### 框架选型：Tauri（ADR v3）

详见 `.omc/plans/framework-choice-tauri-vs-qt-vs-electron.md`。加权评分 Tauri 48.5 > Electron 41 > Qt 36。

### MCP 架构：内嵌 GUI + Streamable HTTP

详见 `docs/architecture-log.md` 的 v1.4 重构记录 + `docs/specs/MCP服务接入-需求规格.md`。

**决策**（取代早期「双二进制 + stdio + named pipe」）：
- **内嵌 GUI 进程**：MCP server 是 GUI 进程内的一个 axum HTTP service（`src-tauri/src/mcp/http_server.rs`），随 GUI 启停。
- **Streamable HTTP transport**：MCP 经 `http://127.0.0.1:41235/mcp`（被占用 +1，只监听 localhost）暴露。
- **同进程内存访问**：MCP 与 SSH 会话/资产/凭据同进程，无桥接复杂度；根治了子进程方案的僵尸进程 / os error 32 / 打包缺口。

### 前端：Vue 3 全量迁移 TypeScript（v0.11.0）

`src/` 下全部 `.js` 转 `.ts`、全部 `.vue` 加 `lang="ts"`，一次到位不做渐进式；`vue-tsc` strict 检查进 `npm run build` 门禁。详见 `docs/specs/notes-v0.11.0.md`。

### 多窗口：资产独立工作台 + tab 跨窗口迁移（v0.10.0）

每窗口独立 webview/Pinia 实例、共享 Rust 后端（Rust 零改动）；已连接会话迁移走 scrollback 序列化 + Rust 内存中转 + adopt 协议，不重连。详见 `docs/architecture-log.md`。

---

## 版本演进

| 版本 | 重点 |
|------|------|
| v0.5.0 – v0.7.x | GUI 打磨期：全量 UI/UX 审计修复、私钥连接体验增强（文件选择器/凭据清除/认证失败引导） |
| v0.8.0 | MCP 危险命令拦截等级可配置 + 执行命令日志；六区域信息精简 |
| v0.9.0 | 全新应用图标；MCP 文件传输全能力无桩化；凭据与托管私钥同步 UI；文件面板与终端会话联动 |
| v0.10.0 | 资产独立工作台窗口 + 终端 tab 跨窗口迁移；GitHub Device Flow 登录；MCP 文件工具审批纳入拦截等级 |
| v0.11.0 | 前端全量迁移 TypeScript（strict 门禁）；「靠猜测代替事实」全仓审计修复 18 项（家目录求证/目录列表去 GNU 依赖/资源监控可移植化等）；终端注入无痕化 |
| v0.12.0 | 安全判据迁入 core（命令分段/脱敏/编码三态，`test:core` 真跑）；MCP 配置与执行日志 fail-closed 化；全量写入原子化；推送乐观并发检查 |
| v0.13.0 | 文件列视图新增权限列与窄栏回退；右键菜单遮挡修复 |

---

## 已知限制与 Follow-ups

### MCP

- **会话不复用**：`ssh_exec`/`sftp_*` 工具当前直走 headless 独立建连。`list_sessions` 已能列出 GUI 活跃会话（可作复用目标参考），工具级会话复用（注入 `SshSessionManager`）仍为 follow-up。
- **Claude Desktop 不直连**：不支持 `streamable-http`，需经 `mcp-remote` 桥接（见 MCP 配置节）。
- headless 建连不支持 MFA（仅密码/私钥/keyboard-interactive 密码类 prompt）
- MCP 隧道类工具未实现（隧道的创建/管理目前在 GUI）
- 远端**文件名**非 UTF-8 时（russh-sftp 上游以替换符解码），对相应条目 rename/remove/download 会 `No such file`；内容侧已修（编码三态判定），文件名侧待上游暴露 raw bytes

### GUI

- `sftp_download_with_progress` 仍返回整块 `Vec<u8>`（分块上传已实现，下载侧待改造）
- `start_remote_forward` 是返回 Err 的桩（local/dynamic SOCKS5 已实现）
- ProxyJump/跳板链未实现
- 多窗口已知限制：窗口不持久化恢复、资产删除不跨窗口同步、主题跨窗口不实时同步、Esc 取消的窗外拖拽可能误开窗
- 终端 tab 迁移回放不含 alt 屏与软换行（vim/less 中拖出只还原进 alt 前内容）；迁移瞬间输出可能丢失
- 超标文件拆分计划见 [`docs/architecture-log.md`](./docs/architecture-log.md) 的 Baseline snapshot

---

## License

MIT
