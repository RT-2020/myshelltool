# myshelltool MCP Server 配置指南

myshelltool 提供独立的 MCP（Model Context Protocol）server 二进制 `myshelltool-mcp.exe`，可被 Claude Desktop、Cursor、Cline 等支持 MCP 的工具通过 stdio 调用，让 AI 执行 SSH 运维操作。

> 版本：v1.0（独立会话模式）。AI 通过 myshelltool 的 headless SSH 路径独立建连查询服务器。高危命令（rm -rf 等）经三层审批拦截。

---

## 一、构建 myshelltool-mcp.exe

`myshelltool-mcp.exe` 是 console 子系统的独立二进制，与 GUI 的 `myshelltool.exe` 共享同一份 Rust 业务代码（`myshelltool_lib`）。

```bash
# 从项目根目录构建（会同时编译 GUI 和 MCP 两个 binary）
npm run tauri:build
# 或仅编译 MCP binary：
cd src-tauri && cargo build --release --bin myshelltool-mcp
```

构建产物：
- GUI：`src-tauri/target/release/myshelltool.exe`
- **MCP**：`src-tauri/target/release/myshelltool-mcp.exe` ← 本文档关注的二进制

> **双 exe 打包说明**：当前 `tauri.conf.json` 的 NSIS 配置默认打包主 binary。MCP exe 需在 `tauri build` 后手动拷贝到安装目录，或将其加入 `bundle.resources`。详见下方各客户端配置中的路径示例。

---

## 二、数据目录与资产配置

MCP 进程默认从 `%APPDATA%\myshelltool\` 读取数据（与 GUI 共享）：

| 文件 | 用途 |
|------|------|
| `connection-assets.json` | 连接资产清单（host/port/username/credential_id 等）|
| `credentials/` | 凭据存储（密码/passphrase，弱 XOR 混淆）|
| `known_hosts.json` | 已信任主机指纹 |

### 配置资产的两种方式

1. **推荐：先用 GUI 配置** — 打开 myshelltool GUI，添加连接资产并完成首次连接（host key 信任）。MCP 会读取同一份数据。
2. **手动放置** — 直接编辑 `%APPDATA%\myshelltool\connection-assets.json`（格式见 `crates/myshelltool-core/src/lib.rs` 的 `ConnectionAsset` 结构）。

### 自定义数据目录

在客户端配置的 `env` 里设置 `MYSHELLTOOL_DATA_DIR` 可指定其他路径：

```json
"env": { "MYSHELLTOOL_DATA_DIR": "D:\\my-mcp-data" }
```

---

## 三、Claude Desktop 配置

编辑配置文件（Windows）：
```
%APPDATA%\Claude\claude_desktop_config.json
```

```json
{
  "mcpServers": {
    "myshelltool": {
      "command": "C:\\Users\\<你的用户名>\\AppData\\Local\\myshelltool\\myshelltool-mcp.exe",
      "args": []
    }
  }
}
```

重启 Claude Desktop 后，工具列表会出现 myshelltool 的 9 个工具 + 3 个资源 + 3 个诊断 prompt。

> **路径注意**：`command` 必须用**绝对路径**，JSON 里反斜杠需双写 `\\`（或用正斜杠 `/`）。不要指向快捷方式，直接指 exe。

---

## 四、Cursor 配置

两种入口：
- **项目级**：仓库根目录 `.cursor/mcp.json`
- **全局**：Settings → MCP 页面 GUI 管理

```json
{
  "mcpServers": {
    "myshelltool": {
      "command": "C:\\Users\\<你>\\AppData\\Local\\myshelltool\\myshelltool-mcp.exe",
      "args": [],
      "type": "stdio"
    }
  }
}
```

> Windows 11 项目级 `.cursor/mcp.json` 有已知路径解析 bug，优先用全局 Settings 配置。

---

## 五、Cline（VS Code 扩展）配置

配置文件：
```
%APPDATA%\Code\User\globalStorage\saoudrizwan.claude-dev\settings\cline_mcp_settings.json
```

格式同 Claude Desktop（`mcpServers` → command/args/env）。

> ⚠️ **Cline 安全注意**：Cline 有「auto-approve use MCP servers」开关。开启后执行 MCP 工具不再逐次询问。但 **myshelltool 自带三层审批门仍生效**——高危命令（rm -rf / mkfs / dd 等）会被拦截并返回三段式拒绝信息，不会因 Cline 的 auto-approve 而绕过。

---

## 六、可用能力一览（v1.0）

### Tools（9 个）

| 工具 | 说明 | 审批 |
|------|------|------|
| `list_assets` | 列出连接资产（脱敏）| 自动 |
| `list_sessions` | 活跃会话（v1.0 返回说明）| 自动 |
| `disk_usage` | 磁盘使用（df -h）| 自动 |
| `system_status` | uptime/内存/负载/top | 自动 |
| `service_status` | systemctl status | 自动 |
| `sftp_list` | SFTP 目录（M3 桩，M4 完善）| — |
| `resource_monitor_snapshot` | 资源快照（M3 桩）| — |
| `ssh_exec` | **任意命令**（经三层审批）| 黑名单/未知拒绝 |
| `sftp_remove` | **删除文件**（v1.0 默认拒）| 拒绝 |

### Resources（3 + 1）

| URI | 说明 |
|-----|------|
| `myshelltool://assets` | 资产清单 |
| `myshelltool://sessions` | 会话列表 |
| `myshelltool://known-hosts` | 已信任主机 |
| `myshelltool://sessions/{id}/log` | 会话日志（template，v1.0 返回不可用）|

### Prompts（3 个诊断模板）

| Prompt | 用途 |
|--------|------|
| `diagnose_server` | 健康：磁盘/CPU/内存/服务 |
| `audit_security` | 安全：登录/端口/进程 |
| `cleanup_disk` | 磁盘清理引导 |

---

## 七、安全机制（v1.0）

### 三层审批（fail-secure 默认拒）

- **白名单**（df/uptime/systemctl status 等 20 条只读前缀）→ 自动执行
- **黄名单**（用户按资产配置，v1.0 暂未启用）→ 自动执行
- **黑名单**（dangerous_commands 的 16 条正则：rm -rf/mkfs/dd/fork bomb/shutdown 等）→ 拒绝
- **未知命令** → 拒绝（fail-secure）

### 三段式拒绝信息

被拒绝的命令返回 `isError: true` + 三段对照：
```
【AI 声明意图】<AI 声称的意图>
【真实命令】<实际要执行的命令>
【后果预测】<该命令的后果说明>
```
通过三段对照可识破 AI 伪装（如 intent 声称「查看日志」但 command 是 `rm -rf /var/log`）。

### Host Key 安全门（D4）

MCP 仅服务**已在 GUI 信任过**的资产。首次连接/未知 host key 直接拒绝，不弹窗（headless 模式）。请在 GUI 先完成首次连接。

---

## 八、已知限制（v1.0）

- **会话不共享**：MCP 进程独立建连，不复用 GUI 已建立的 SSH 会话（v1.1 计划 named pipe 桥接）
- **删除操作默认拒**：sftp_remove 在 v1.0 默认拒绝（删除不可逆 + 跨进程确认需 v1.1）
- **MFA 不支持**：headless 模式无法弹窗收集 MFA，仅支持密码/私钥/keyboard-interactive 密码类 prompt
- **提示词注入风险**：AI 可能被远程内容诱导执行恶意命令——v1.0 靠三段式人工识破（行业级未解难题）

---

## 九、故障排查

### Claude Desktop 连不上

1. 检查 `command` 路径是否正确（绝对路径、exe 真实存在）
2. 手动运行 `myshelltool-mcp.exe`，看 stderr 是否有报错
3. 检查 `%APPDATA%\myshelltool\logs\myshelltool-mcp.log` 日志

### 工具调用报「资产不存在」

先在 GUI 配置资产，或手动编辑 `connection-assets.json`。用 `list_assets` 确认资产 ID。

### host key 被拒

在 myshelltool GUI 中手动连接该主机一次，确认信任 host key。MCP 只服务已信任的主机。
