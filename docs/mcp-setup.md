# myshelltool MCP Server 配置指南

myshelltool 内嵌 MCP（Model Context Protocol）server，可被 Claude Code、Cursor、Cline 等支持 MCP 的工具调用，让 AI 执行 SSH 运维操作。

> 版本：v0.4.0（MCP 内嵌 GUI + Streamable HTTP transport）。**MCP server 随 GUI 进程启停**——不再有独立 exe，不再需要单独构建/打包。高危命令（rm -rf 等）经 elicitation 在客户端界面内弹确认（三段式），不支持 elicitation 的客户端 fail-secure 拒绝。

---

## 一、架构（v0.4.0 内嵌 HTTP）

```
┌─────────────────┐   Streamable HTTP      ┌──────────────────────────┐
│ Claude Code     │   (JSON-RPC over HTTP) │  myshelltool.exe (GUI)   │
│ Cursor / Cline  │ ─────────────────────→ │  内嵌 MCP server         │
└─────────────────┘                        │  http://127.0.0.1/mcp    │
                                           └────────────┬─────────────┘
                                                        │ russh SSH（同进程内存访问）
                                                        ▼
                                           ┌──────────────────────────┐
                                           │  你的服务器们             │
                                           │  (192.168.x.x ...)       │
                                           └──────────────────────────┘
```

**核心认知**：
- MCP server **不是独立进程**，是 GUI 进程内的一个 axum HTTP service（`src-tauri/src/mcp/http_server.rs`）。
- GUI 启动时经 `tauri::async_runtime::spawn` 拉起，GUI 退出时 CancellationToken 触发 graceful shutdown。**用 MCP 前必须先开 myshelltool GUI**。
- endpoint 只监听 `127.0.0.1`（localhost，§安全红线），绝不监听 `0.0.0.0`。
- 默认端口 `41235`，被占用则 +1 重试（最多 10 次）。**实际端口看 GUI 的 MCP 面板显示**（下方「数据目录与 endpoint 查看」）。

> v0.4.0 取消了 v0.3 的「独立 `myshelltool-mcp.exe` + stdio + named pipe 桥」架构。原因：根治僵尸进程 / os error 32 / NSIS 双 exe 打包缺口 / pipe 桥复杂度。详见 `docs/architecture-log.md` 的 v1.4 重构记录。

---

## 二、数据目录与资产配置

MCP 与 GUI 共享同一份数据目录（目录名取 `tauri.conf.json` 的 identifier）：

| 文件 | 用途 |
|------|------|
| `connection-assets.json` | 连接资产清单（host/port/username/credential_id 等） |
| `credentials/` | 凭据存储（密码/passphrase，v1.3 起基于 Windows DPAPI 加密） |
| `known_hosts.json` | 已信任主机指纹 |
| `mcp-endpoint.json` | 【v0.4.0】MCP HTTP server 实际监听地址（URL/host/port，启动时写入） |

**配置资产的两种方式**：

1. **推荐：先用 GUI 配置** — 打开 myshelltool GUI，添加连接资产并完成首次连接（host key 信任）。MCP 会读取同一份数据。
2. **手动放置** — 直接编辑 `%APPDATA%\com.redtei.myshelltool\connection-assets.json`（格式见 `crates/myshelltool-core/src/lib.rs` 的 `ConnectionAsset` 结构）。

### 查看 MCP endpoint（实际 URL）

GUI 内打开「MCP」面板（顶部菜单或状态栏入口），会显示：
- **MCP Endpoint**：实际监听 URL（如 `http://127.0.0.1:41235/mcp`，端口可能 +1）
- **HTTP 健康检查结果**：向自己的 endpoint 发 initialize 握手，回答「MCP 能否正常工作」
- **配置 JSON**：一键复制，贴入下方各客户端配置

> endpoint URL 必须用 GUI 面板显示的值，不要硬编码 41235（端口被占用时会变）。

---

## 三、Claude Code 配置

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

> **url 必须用 GUI MCP 面板显示的实际 endpoint**（端口可能不是 41235）。确保 myshelltool GUI 正在运行，否则 Claude Code 连不上。

重启 Claude Code 后，问它：

> 「列出我的 myshelltool 资产」

Claude 会调用 `list_assets` 返回你配置的服务器列表。

---

## 四、Cursor 配置

两种入口：
- **项目级**：仓库根目录 `.cursor/mcp.json`
- **全局**：Settings → MCP 页面 GUI 管理

```json
{
  "mcpServers": {
    "myshelltool": {
      "url": "http://127.0.0.1:41235/mcp"
    }
  }
}
```

> Windows 11 项目级 `.cursor/mcp.json` 有已知路径解析 bug，优先用全局 Settings 配置。url 同样用 GUI 面板显示的实际 endpoint。

---

## 五、Cline（VS Code 扩展）配置

配置文件：

```
%APPDATA%\Code\User\globalStorage\saoudrizwan.claude-dev\settings\cline_mcp_settings.json
```

格式同 Claude Code（`mcpServers` → `url`）：

```json
{
  "mcpServers": {
    "myshelltool": {
      "url": "http://127.0.0.1:41235/mcp"
    }
  }
}
```

> ⚠️ **Cline 安全注意**：Cline 有「auto-approve use MCP servers」开关。开启后执行 MCP 工具不再逐次询问。但 **myshelltool 自带 elicitation 审批门仍生效**——高危命令（rm -rf / mkfs / dd 等）会触发客户端内确认框，不会因 Cline 的 auto-approve 而绕过。

---

## 六、Claude Desktop（⚠️ 不直接支持）

> ⚠️ **Claude Desktop 截至目前不直接支持 `streamable-http` transport**（仅支持 stdio + sse）。v0.4.0 取消 stdio 后，Claude Desktop 无法直连 myshelltool MCP。
>
> **社区桥接方案**：经 [`mcp-remote`](https://www.npmjs.com/package/mcp-remote) 做 stdio→HTTP 代理（依赖 npx/node 环境）。配置示例与已知问题见 [modelcontextprotocol 讨论区](https://github.com/modelcontextprotocol/modelcontextprotocol/discussions/1940)。本指南不展开桥接步骤。
>
> 若你主要用 Claude Desktop，可考虑用 **Claude Code**（原生支持 streamable-http）替代。

---

## 七、可用能力一览（v0.4.0）

### Tools（9 个）

| 工具 | 说明 | 审批 |
|------|------|------|
| `list_assets` | 列出连接资产（脱敏） | 自动 |
| `list_sessions` | 活跃会话列表（当前返回说明，会话复用为 follow-up） | 自动 |
| `disk_usage` | 磁盘使用（df -h） | 自动 |
| `system_status` | uptime/内存/负载/top | 自动 |
| `service_status` | systemctl status | 自动 |
| `sftp_list` | SFTP 目录列表 | 自动 |
| `resource_monitor_snapshot` | 资源监控快照 | 自动 |
| `ssh_exec` | **任意命令** | 白名单自动 / 其余 elicitation 确认 |
| `sftp_remove` | **删除文件** | 始终 elicitation 确认 |

### Resources（3 + 1）

| URI | 说明 |
|-----|------|
| `myshelltool://assets` | 资产清单 |
| `myshelltool://sessions` | 会话列表 |
| `myshelltool://known-hosts` | 已信任主机 |
| `myshelltool://sessions/{id}/log` | 会话日志（template） |

### Prompts（3 个诊断模板）

| Prompt | 用途 |
|--------|------|
| `diagnose_server` | 健康：磁盘/CPU/内存/服务 |
| `audit_security` | 安全：登录/端口/进程 |
| `cleanup_disk` | 磁盘清理引导 |

---

## 八、安全机制（v0.4.0）

### 审批分层（fail-secure 默认拒）

- **白名单**（df/uptime/systemctl status 等 20 条只读前缀）→ 自动执行
- **高危命令**（`ssh_exec` 的非白名单 + `sftp_remove`）→ 经 **MCP elicitation** 在客户端界面内弹确认框，用户 accept 才执行
- **黑名单**（dangerous_commands 的 16 条正则：rm -rf/mkfs/dd/fork bomb/shutdown 等）→ 拒绝
- **不支持 elicitation 的客户端** → fail-secure 拒绝（宁可误拦不可漏放）

### 三段式确认信息

高危命令触发确认框时显示：

```
⚠️ 高危操作审批

【AI 声明意图】<AI 声称的意图>
【真实命令】<实际要执行的命令>
【后果预测】<该命令的后果说明>

确认要执行此操作吗？
```

通过三段对照可识破 AI 伪装（如 intent 声称「查看日志」但 command 是 `rm -rf /var/log`）。降级拒绝时返回同结构文本。

### Host Key 安全门

MCP 仅服务**已在 GUI 信任过**的资产。首次连接/未知 host key 直接拒绝（headless 模式不弹窗）。请在 GUI 先完成首次连接。

---

## 九、已知限制与 Follow-ups（v0.4.0）

- **会话不复用**：`ssh_exec`/`sftp_*` 工具当前直走 headless 独立建连（v0.4.0 删了 v1.1 的 named pipe 复用桥）。Follow-up：注入 GUI 的 `SshSessionManager` 到 MCP context，命中已建立会话时直接复用（同进程内存访问，比 pipe 更简单）。
- **不支持 elicitation 的客户端**：当前直接 fail-secure 拒绝高危命令（v0.4.0 删了 v1.1 的 pipe GUI 弹窗降级）。Follow-up：注入 AppHandle，实现同进程 GUI 弹窗审批（像 ssh.rs host-key 验证那样 emit 前端）。
- **Claude Desktop 不直连**：见上方第六节，需 mcp-remote 桥接。
- **MFA 不支持**：headless 模式无法弹窗收集 MFA，仅支持密码/私钥/keyboard-interactive 密码类 prompt。
- **提示词注入风险**：AI 可能被远程内容诱导执行恶意命令——靠三段式人工识破（行业级未解难题）。

---

## 十、故障排查

### 客户端连不上 MCP

1. **确认 myshelltool GUI 正在运行**——MCP server 随 GUI 启停，GUI 没开就没有 MCP。
2. **在 GUI MCP 面板看 endpoint URL**——确认客户端配置的 `url` 与面板显示一致（端口可能不是 41235）。
3. **看健康检查结果**——GUI 面板的「HTTP 健康检查」会显示失败原因（`endpoint_not_found` / `http_error` / `timeout` / `bad_protocol`）。
4. **端口被占用**——默认 41235 被占用会自动 +1，看面板的实际端口。若 41235-41245 全被占用，MCP server 起不来（日志报 "all ports exhausted"）。

### 工具调用报「资产不存在」

先在 GUI 配置资产，或手动编辑 `connection-assets.json`。用 `list_assets` 确认资产 ID。

### host key 被拒

在 myshelltool GUI 中手动连接该主机一次，确认信任 host key。MCP 只服务已信任的主机。

### elicitation 确认框不出现

客户端可能不支持 elicitation（如旧版 Claude Desktop）。v0.4.0 对此类客户端直接拒绝高危命令（fail-secure）。换用 Claude Code 或 Cursor（支持 elicitation）。
