# myshelltool MCP Server 配置指南

myshelltool 内嵌 MCP（Model Context Protocol）server，可被 Claude Code、Cursor、Cline 等支持 MCP 的工具调用，让 AI 执行 SSH 运维操作。

> **MCP server 随 GUI 进程启停**——内嵌 GUI + Streamable HTTP transport，无独立 exe、无需单独构建。入口已加 **token 鉴权**：URL 内嵌 256-bit 随机 token，无 token 一律 401。高危命令按**拦截等级**（Minimal/Strict，GUI 面板可切换）审批：确认框优先经 elicitation 在客户端界面内弹出（三段式），客户端不支持时降级为 myshelltool GUI 弹窗，均不可用才 fail-secure 拒绝。

---

## 一、架构与接入模型

```
┌─────────────────┐   Streamable HTTP + token   ┌──────────────────────────┐
│ 任意 MCP 客户端  │   (JSON-RPC over HTTP)      │  myshelltool.exe (GUI)   │
│ (AI 工具)       │ ──────────────────────────→ │  内嵌 MCP server         │
└─────────────────┘                             │  127.0.0.1:<port>/mcp    │
                                                │  （/mcp/<token> 鉴权）    │
                                                └────────────┬─────────────┘
                                                             │ russh SSH（同进程）
                                                             ▼
                                                ┌──────────────────────────┐
                                                │  你的服务器们             │
                                                └──────────────────────────┘
```

**核心认知**：

- MCP server **不是独立进程**，是 GUI 进程内的 axum HTTP service（`src-tauri/src/mcp/http_server.rs`）。
- GUI 启动时经 `tauri::async_runtime::spawn` 拉起，GUI 退出时 CancellationToken 触发 graceful shutdown。**用 MCP 前必须先开 myshelltool GUI**。
- endpoint 只监听 `127.0.0.1`（localhost，安全红线），绝不监听 `0.0.0.0`。
- release 默认起始端口 `41235`，debug（`tauri:dev`）默认 `41500`；被占用则 +1（最多 10 次）。可用 `MYSHELLTOOL_MCP_PORT` 覆盖起始端口，**实际端口以 GUI 的 MCP 面板显示为准**。
- **接入 URL 形如 `http://127.0.0.1:<端口>/mcp/<token>`**；除路径内嵌 token 外也接受 `Authorization: Bearer <token>`。服务端日志不记录 token。
- token 等同凭据：泄露时在 GUI MCP 面板点「重置 token」，旧 URL 立即 401；已配置的客户端需要重新复制新 URL。

## 二、数据目录与 endpoint

MCP 与 GUI 共享同一份数据目录（目录名取 `tauri.conf.json` 的 identifier）：

| 文件 | 用途 |
|---|---|
| `connection-assets.json` | 连接资产清单（host/port/username/credential_id 等） |
| `credentials/` | 凭据存储（密码/passphrase，DPAPI 加密） |
| `known_hosts.json` | 已信任主机指纹 |
| `mcp-config.json` | 拦截等级 + 授权范围（scope） |
| `mcp-execution-log.json` | 执行审计日志（命令已脱敏） |
| `mcp-endpoint.json` | 实际监听地址与 authToken（启动时重写） |

默认路径：`%APPDATA%\com.redtei.myshelltool\`；可用环境变量 `MYSHELLTOOL_DATA_DIR` 覆盖。

### 查看并复制 endpoint（重要）

GUI 内打开「MCP」面板，会显示：

- **MCP Endpoint**：含 token 的完整 URL（复制按钮复制整条 URL）
- **HTTP 健康检查结果**：向自己的 endpoint 发 initialize 握手，回答「MCP 能否正常工作」
- **配置 JSON**：一键复制，贴入各客户端配置

> ⚠️ 不要硬编码端口，也不要把 token 从 URL 里删掉——两者都会导致连不上（后者必定 401）。请始终使用 GUI 面板复制出的完整 URL。
> ⚠️ 不要把带 token 的 URL 提交到仓库、截图发群或写入公开文档；泄露后在 MCP 面板重置即可。

---

## 三、客户端配置

myshelltool 的 MCP 是 **Streamable HTTP + URL 内嵌 token**，所有支持该 transport 的客户端配置形态一致：

```json
{
  "mcpServers": {
    "myshelltool": {
      "url": "<从 GUI MCP 面板复制的完整 URL，形如 http://127.0.0.1:<端口>/mcp/<token>>"
    }
  }
}
```

### Claude Code

Claude Code 原生支持 `streamable-http`。编辑项目级或全局 MCP 配置，`url` 粘贴完整 endpoint。重启 Claude Code 后问它：

> 「列出我的 myshelltool 资产」

Claude 会调用 `list_assets` 返回你配置的服务器列表。

### Cursor

两种入口：

- **项目级**：仓库根目录 `.cursor/mcp.json`
- **全局**：Settings → MCP 页面 GUI 管理

```json
{
  "mcpServers": {
    "myshelltool": {
      "url": "<从 GUI MCP 面板复制的完整 URL>"
    }
  }
}
```

> Windows 11 项目级 `.cursor/mcp.json` 有已知路径解析 bug，优先用全局 Settings 配置。

### Cline（VS Code 扩展）

配置文件：

```
%APPDATA%\Code\User\globalStorage\saoudrizwan.claude-dev\settings\cline_mcp_settings.json
```

格式同上（`mcpServers` → `url`）：

```json
{
  "mcpServers": {
    "myshelltool": {
      "url": "<从 GUI MCP 面板复制的完整 URL>"
    }
  }
}
```

> ⚠️ **Cline 安全注意**：Cline 有「auto-approve use MCP servers」开关。开启后执行 MCP 工具不再逐次询问。但 **myshelltool 自带审批门仍生效**（elicitation / GUI 弹窗降级 + 拦截等级）——毁灭性命令两档恒拒，Strict 档高危命令不会因 Cline 的 auto-approve 绕过。

### Claude Desktop（需 mcp-remote 桥接）

Claude Desktop 目前不直接支持 `streamable-http`（仅 stdio + 旧 SSE）。用社区桥 [`mcp-remote`](https://www.npmjs.com/package/mcp-remote) 做 stdio→HTTP 代理（依赖 Node/npx）：

```json
{
  "mcpServers": {
    "myshelltool": {
      "command": "npx",
      "args": ["-y", "mcp-remote", "<从 GUI MCP 面板复制的完整 URL>"]
    }
  }
}
```

> 如果主要在 Claude Desktop 使用，也可先用原生支持 `streamable-http` 的 Claude Code / Cursor / Cline。把 myshelltool 原生桥做成安装即用已列入下一轮计划（见「已知限制与 Follow-ups」）。

---

## 四、可用能力一览

myshelltool 当前暴露 **26 个 Tools**、**4 个 Resources**、**3 个 Prompts**；工具清单与 GUI「MCP」面板的能力清单同源（`src-tauri/src/mcp/registry.rs` 是单一事实源）。

### Tools（26 个 = 16 只读 + 6 写/控制 + 4 执行/删除）

**只读 / 查询（16 个）**

| 工具 | 说明 | 审批 |
|---|---|---|
| `list_assets` | 列出连接资产（脱敏；受授权范围过滤） | 自动 |
| `list_sessions` | 列出当前 GUI / 后端活跃 SSH 会话 | 自动 |
| `disk_usage` | 指定资产磁盘使用（POSIX `df`，逐段退出码回声） | 自动 |
| `system_status` | uptime / 内存 / 负载 / top 进程（Linux） | 自动 |
| `service_status` | systemd 服务状态（systemctl status，Linux） | 自动 |
| `resource_monitor_snapshot` | CPU / 内存 / 磁盘资源快照 | 自动 |
| `journal_query` | journalctl 结构化查询（unit/since/until/grep/limit，跨发行版口径固化） | 自动 |
| `port_listen` | 监听端口（ss → netstat → lsof 三级降级） | 自动 |
| `process_list` | 进程列表（POSIX ps，按 CPU / 内存排序，服务端限量） | 自动 |
| `file_search` | 远端文件查找（POSIX find，name/minSizeMb/mtimeDays 过滤，结果有界） | 自动 |
| `tunnel_list` | 隧道清单与状态（kind / active / error） | 自动 |
| `sftp_list` | 远程目录列表（名称 / 类型 / 大小 / 时间 / 权限） | 自动 |
| `sftp_read_file` | 读取远程文本（默认 256KB 分页，单次最高 4MB；二进制 / 非法编码明确拒绝） | 敏感凭据路径恒审批 |
| `read_output` | 分页取回 `ssh_exec` 被截断的完整输出（cursor 10 分钟内有效） | 自动 |
| `job_status` | 长任务状态（running / done / failed / cancelled + 退出码 + 输出字节） | 自动 |
| `job_output` | 长任务输出分页（offset / limit，单页上限 1MiB） | 自动 |

**写与控制（6 个）**

| 工具 | 说明 | 审批 |
|---|---|---|
| `job_cancel` | 取消后台长任务（断开其连接，状态收敛为 cancelled） | 自动 + 审计 |
| `service_control` | systemd 服务 start/stop/restart/reload（action 白名单，执行后补 is-active） | Minimal 放行记日志 / Strict 审批 |
| `tunnel_create` | 建立 remote 端口转发（MVP 仅 remote；local/dynamic 用 GUI） | Minimal 放行记日志 / Strict 审批 |
| `sftp_write_file` | 写远程文本（原子替换，单次 1MB 上限） | 按等级（Minimal 放行 / Strict 审批） |
| `sftp_upload` | 本机 → 远程流式上传（SHA256 校验；本机 FS 受 scope 控制） | 按等级 |
| `sftp_download` | 远程 → 本机流式下载（SHA256 校验；本机 FS 受 scope 控制） | 按等级；本机受保护系统目录恒拒 |

**执行与删除（4 个）**

| 工具 | 说明 | 审批 |
|---|---|---|
| `ssh_exec` | 单机任意 Shell（结构化 exit_code/stdout/stderr；调用必须声明 intent） | 白名单自动；未知 / 非毁灭黑名单按等级；毁灭性恒拒 |
| `exec_many` | 多机同一命令 fan-out（单次 ≤64 台，并发 8 台排队；逐目标 scope 与结果） | 同 `ssh_exec`（按命令文本一次判定放行整批） |
| `ssh_exec_async` | 后台长任务，返回 job_id（用 job_* 轮询 / 取输出 / 取消；TTL 30 分钟） | 同 `ssh_exec` |
| `sftp_remove` | 删除远程文件 / 目录（根级与核心目录恒拒；非空目录需 recursive=true） | 毁灭性路径恒拒；其余按等级 |

### Resources（4 个，无 template）

| URI | 说明 |
|---|---|
| `myshelltool://assets` | 资产清单（与 `list_assets` 同源、同 scope 过滤） |
| `myshelltool://sessions` | 当前活跃 SSH 会话（exec 类工具优先复用，sessionSource=gui） |
| `myshelltool://known-hosts` | 已信任主机指纹 |
| `myshelltool://mcp/scope` | 当前授权范围（AI 应知道自己的边界） |

### Prompts（3 个，均支持多资产）

| Prompt | 用途 |
|---|---|
| `diagnose_server` | 健康诊断：磁盘 / CPU / 内存 / 关键服务，多机输出对比表 |
| `audit_security` | 安全审计：最近登录 / 监听端口 / 可疑进程 |
| `cleanup_disk` | 磁盘清理：大文件 / 旧日志，给出清理建议 |

> `asset_ids` 支持 JSON 数组、逗号分隔、单值三种形态（旧 `asset_id` 参数名兼容）。纪律：任何一台任何一步失败都必须标注「该步未取得数据」，不得据此判定无异常。

---

## 五、安全机制

### 入口鉴权（token）

- 启动时用 CSPRNG 生成 256-bit token，路由形态为 `/mcp/<token>`；同时兼容 `Authorization: Bearer <token>`。
- 路径错与 token 错统一返回 401，不做探测预言机；token 不写日志。
- token 是凭据级别：泄露后在 GUI MCP 面板点「重置 token」，旧 URL 立即失效；已配置的 host 需要更新为新 URL。

### 授权范围（scope）

GUI MCP 面板可配置 AI 能碰哪些资产：

| 字段 | 语义 |
|---|---|
| 允许的分组 | 按 `/` 组件匹配；授权父组同时授权其子组，但「生产」不匹配「生产测试」 |
| 允许的标签 | 与分组是**并集**（命中任一即允许） |
| 排除的资产 | 显式拒绝某几台，优先级最高 |
| 允许本机文件 | 控制 `sftp_upload` / `sftp_download` 能否触及本机 FS；一旦配置了任一资产限制，本机 FS 默认关闭，需显式打开 |
| 配置损坏 | `mcp-config.json` 读不出/解析失败时 fail-closed：Strict + 拒绝全部资产（面板会提示重新配置） |

分组/标签都为空且无排除项 = 不限制资产（默认行为）。范围外资产的工具调用会明确拒绝并记入执行日志；`list_assets` 与 `://assets` 资源同样按 scope 过滤。

### 拦截等级（GUI MCP 面板可切换，对已连接 AI 会话下次调用即生效）

| 等级 | 行为 |
|---|---|
| **Minimal**（默认） | 只硬拦「机器报废级」毁灭性命令（mkfs / dd 写块设备 / rm 根级删除等），其余直接执行并记 `minimal_allowed` 审计日志 |
| **Strict** | 非白名单一律人工确认；毁灭性命令同样硬拦 |

补充语义：

- **白名单逐段判定**：只读命令按 shell 词法分段后逐段过白名单，`df -h; cat /etc/shadow` 这类拼接不会因前缀相似漏过；含重定向 / 命令替换一律不进白名单。
- **敏感文件读取恒审批**：`sftp_read_file` 命中 `/etc/shadow`、SSH 私钥、`.env` 等敏感路径时，不受 Minimal 档影响，必须人工确认。
- **毁灭性命令两档恒拒**：不弹确认框、直接拒绝，决策记 `hard_blocked`；根级 / 核心目录删除、本机受保护系统目录写入同属硬拒。
- **写类工具按等级**：`sftp_write_file` / `sftp_upload` / `sftp_download` / `service_control` / `tunnel_create` 在 Minimal 档放行记日志，在 Strict 档需人工确认。

### 审批三级降级

高危命令触发确认时，按顺序尝试：

1. **elicitation**：在支持该能力的 MCP 客户端界面内弹三段式确认框；
2. **GUI 弹窗**：客户端不支持时，降级到 myshelltool GUI 的全局弹窗（60 秒超时）；
3. **fail-secure 拒绝**：GUI 也不可用（headless / 测试）时直接拒绝，宁可不执行。

### 执行审计日志

真实触发远程执行的工具调用会记录：时间、资产、命令（口令已脱敏）、工具名、决策、会话来源（gui / pool / new）、耗时与输出摘要。`sftp_read_file` 的文件正文绝不写入日志，只记元信息。GUI 的 MCP 面板可查看 / 搜索 / 清空。

### Host Key 安全门

MCP 只服务**已在 GUI 完成首次连接（信任 host key）**的资产；headless 路径遇到未知 host key 直接拒绝、不弹窗。请先在 GUI 连接一次目标服务器。

### 三段式确认信息

高危命令触发确认框时会显示：

```
⚠️ 高危操作审批

【AI 声明意图】<AI 声明的意图>
【真实命令】<实际要执行的命令>
【后果预测】<该命令的后果说明>

确认要执行此操作吗？
```

三段对照用于识破「intent 声称查看日志、command 却是删除」这类提示词注入场景。

---

## 六、已知限制与 Follow-ups

- **Claude Desktop 不直连**：不支持 `streamable-http`，需按上文用 `mcp-remote` 桥接；把 myshelltool 原生桥做成安装即用是下一轮优先项。
- **headless MFA 不支持**：MCP 独立建连只支持密码 / 私钥 / keyboard-interactive 密码类 prompt；非密码 prompt（MFA 等）需先在 GUI 建立该资产会话，MCP 会优先复用 GUI 会话（`sessionSource=gui`）。
- **隧道工具是 MVP**：`tunnel_create` 仅支持 remote；local/dynamic 需在 GUI 隧道面板建立；工具暂无隧道停止 / 删除操作（GUI 面板可管理）。
- **本机 FS 默认收紧**：配置了任一资产授权范围后，`sftp_upload` / `sftp_download` 需显式打开「允许本机文件」；未配置 scope 的老用户行为不变。
- **只支持本机接入**：MCP 只监听 `127.0.0.1`，不提供 LAN / 远程暴露；远程 MCP 是独立安全议题，需要单独设计与评审。
- **提示词注入风险**：AI 仍可能被远端内容诱导执行恶意命令；请保持 Strict 档或最小权限 scope，并定期查看执行审计日志。
- **能力随版本演进**：工具 / Resources / Prompts 清单以 GUI MCP 面板和 `list_*` 返回为准；升级应用后如客户端仍显示旧清单，重启客户端。

---

## 七、故障排查

### 客户端连不上 MCP

1. **确认 myshelltool GUI 正在运行**——MCP server 随 GUI 启停，GUI 没开就没有 MCP。
2. **确认 URL 是从 GUI MCP 面板复制的完整 endpoint**——包含 `/mcp/<token>`，不要省略 token、也不要改端口。
3. **看健康检查结果**——GUI 面板会显示失败原因（endpoint_not_found / http_error / timeout / bad_protocol）。
4. **端口被占用**——release 默认从 41235 开始 +1；若 41235–41245 都被占用，MCP server 起不来（日志报 "all ports exhausted"）。可用 `MYSHELLTOOL_MCP_PORT` 指定起始端口。

### 返回 401 / 配置后突然失效

- 只在 URL 里写了端口、漏了 token；或面板点过「重置 token」。
- 处理：回 GUI MCP 面板重新复制完整 URL，更新客户端配置并重启客户端。

### 工具调用报「资产不存在」/ 被拒绝

- 先确认目标资产已在 GUI 配置并成功连接过（host key 已信任）。
- 再检查 MCP 面板的授权范围：资产是否在允许的分组 / 标签内，是否被显式排除；本机上传 / 下载还需打开「允许本机文件」。
- `mcp-config.json` 损坏会触发 fail-closed（拒绝全部资产 + Strict），面板会提示重新配置。

### host key 被拒

在 myshelltool GUI 中手动连接该主机一次，确认信任 host key；MCP 只服务已信任主机。

### elicitation 确认框不出现

客户端可能不支持 elicitation（旧版 Claude Desktop 等），此时会降级为 myshelltool GUI 弹窗；若 GUI 不在或超时，则 fail-secure 拒绝。想获得客户端界面内确认体验，用支持 elicitation 的 Claude Code / Cursor / Cline。

### MFA / 交互式提示导致连接失败

headless 建连无法收集 MFA 等非密码 prompt。处理：先在 GUI 完成该资产连接（会话会被 MCP 优先复用），或给资产配置密码 / 私钥 / keyboard-interactive 密码类凭据。

### 长命令 / 大输出

- 超过工具超时的命令用 `ssh_exec_async` 后台执行，再用 `job_status` / `job_output` 轮询和分页取输出。
- `ssh_exec` 输出被截断时，用返回的 cursor 调 `read_output` 在 10 分钟内取回完整输出。
- 需要停止长任务时用 `job_cancel`。

### 升级后客户端看不到新工具

重启 MCP 客户端；并在 GUI MCP 面板点击刷新 / 查看能力清单，确认 serverVersion 与刚安装的应用版本一致。
