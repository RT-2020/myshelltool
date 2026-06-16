---
title: myshelltool MCP 服务接入 — 需求规格
interview_id: mcp-server-20260616
project_type: brownfield
profile: standard
threshold: 0.2
threshold_percent: 20%
final_ambiguity: 0.18
status: approved — D1-D9 选型已全部按推荐锁定，可进入实施
created_at: 2026-06-16
transcript: docs/interviews/MCP服务接入-访谈记录-20260616.md
plan: docs/plans/MCP服务接入-实施计划.md
---

# myshelltool MCP 服务接入 — 需求规格

> 本规格由 `deep-interview-pure`（Standard 档位）9 轮 Socratic 访谈凝练而成。
> 访谈全程基于 brownfield 事实核查（Explore agent 核实 MCP Rust SDK 现状、Tauri 子命令可行性、named pipe 选型、项目现有架构）。
> **本文件是后续实施的需求单一信息源**。任何实施偏离需回访本规格并修订。

---

## 1. 元数据

| 字段 | 值 |
|------|---|
| profile | standard（阈值 ≤ 0.20，max 12 轮）|
| interview_id | mcp-server-20260616 |
| context_type | brownfield（已有 Tauri+Rust+Vue3 SSH 客户端）|
| rounds | 9（实际，未触达 max 12）|
| final_ambiguity | 0.18（≤ 0.20 PASSED）|
| readiness_gate_non_goals | ✅ 显式（v1 排除 SSE/模式 B）|
| readiness_gate_decision_boundaries | ✅ 显式（重大决策需候选+推荐+依据）|
| pressure_pass | ✅ Round 9 回访 Round 2 |
| information_freshness | MCP 生态核查时点 2026-06-16（rmcp 1.7.0）|

---

## 2. Clarity Breakdown（加权歧义计算）

| 维度 | 权重 | 得分 | 依据 |
|------|------|------|------|
| Intent Clarity | 0.25 | 0.92 | AI 运维助手（白天人在场）+ 自动化执行节点（v2 场景），myshelltool 作被动服务方 |
| Outcome Clarity | 0.20 | 0.90 | MCP stdio server，被 Claude/Cursor 拉起，复用 GUI SSH 会话 |
| Scope Clarity | 0.20 | 0.85 | v1：Tools+Resources+Prompts + 模式 A；v2：模式 B + SSE |
| Constraint Clarity | 0.15 | 0.88 | Windows 优先；复用 russh/AppState/SecretStore/dangerousCommands/GlobalModals |
| Success Criteria | 0.10 | 0.82 | Claude Desktop 能配置并执行 SSH 命令，审批与降级行为符合契约 |
| Context Clarity（brownfield）| 0.10 | 0.90 | 已摸清 45 个 Tauri 命令、AppState 结构、审批复用点、named pipe 可行性 |

**加权歧义** = 1 - (0.92×0.25 + 0.90×0.20 + 0.85×0.20 + 0.88×0.15 + 0.82×0.10 + 0.90×0.10)
            = 1 - (0.230 + 0.180 + 0.170 + 0.132 + 0.082 + 0.090)
            = 1 - 0.884
            = **0.116**

> 注：实时评估时取保守值 0.18（含未在加权表体现的 D1-D9 决策未拍板的残余不确定性）。即便取保守值仍 ≤ 0.20 阈值 PASSED。

---

## 3. Intent（为什么做）

让 AI（Claude Desktop / Cursor / Cline）和大模型通过 MCP（Model Context Protocol）调用 myshelltool，把它作为**运维执行层**。两个主要场景：

1. **AI 运维助手**（白天，人在场）：你在 Claude 里说「看看 web-01 的 CPU 和磁盘」，AI 调用 myshelltool 去查。你本人仍用 GUI，AI 是跑腿助手，myshelltool 是它的手脚。
2. **自动化执行节点**（v2 留档）：myshelltool 作为可被 CI/脚本/其他 agent 调用的运维执行节点，融入更大自动化链路（定时巡检、故障自愈）。

**架构师价值判断**：此功能填补原规划 `deep-interview-rust-finalshell.md` 未兑现的差异化护城河（原规划第一动机 GitHub 同步未实现），且 MCP 路线比 Git 同步**更高杠杆**——把 myshelltool 从「另一个 SSH 客户端」升级为「AI 时代运维执行层」，2026 年几乎无竞品。

---

## 4. Desired Outcome

`myshelltool.exe` 发布形态包含配套的 `myshelltool-mcp.exe`（console 子系统二进制），被 Claude Desktop 以 stdio 方式拉起为 MCP server，通过 Windows named pipe 与 GUI 主进程通信以复用其已建立的 SSH 会话，向 AI 暴露 Tools / Resources / Prompts 三类原语，并在高危操作时经跨进程审批回路触发 GUI 三段式弹窗。

---

## 5. In-Scope（v1）

### 5.1 传输与会话
- **MCP stdio server**（双二进制：`myshelltool-mcp.exe` console 子系统 + `myshelltool.exe` GUI 子系统，共享 `myshelltool_lib`）
- **模式 A**：复用 GUI 会话（named pipe 桥接，MCP 子进程 ↔ GUI 进程）
- **弹性降级**：GUI 未运行 → 重试拉起 GUI（递增间隔，上限 N 次）→ 仍失败 → 降级只读（仅本地数据类工具，不调 SSH），对 AI 返回明确错误

### 5.2 MCP 三原语
- **Tools**：12 个运维工具（详见 §10 决策 D6）
- **Resources**：3 个静态资源 + 1 个 resource template（详见 §10 决策 D7）
- **Prompts**：3 个诊断类 prompt（详见 §10 决策 D8）

### 5.3 安全审批
- **三层审批**：白名单（自动）/ 黄名单（自动+日志）/ 默认拒（弹窗）
- **三段式弹窗**：AI 自述意图 + 真实命令 + 后果预测
- **危险命令检测**：从 `dangerousCommands.js` 翻译为 Rust `dangerous_commands.rs`，GUI 与 MCP 共享单点真相
- **host key 处理**：MCP 仅服务已在 GUI 信任过的资产（known_hosts 已有记录 + 凭据已存），首次连接/未知 host key 直接返回错误提示

---

## 6. Out-of-Scope / Non-goals

### 6.1 v1 明确排除
- ❌ **模式 B**（AI 独立建连）：需解决凭据注入 + host key 自动信任 + 多进程并发仲裁
- ❌ **SSE / HTTP 传输**：v2 远程访问场景
- ❌ **CI 自动化**：依赖模式 B
- ❌ **Resources 订阅能力**（`resources/subscribe`）：MCP 规范 optional，v2 再做
- ❌ **无人值守场景**：与「高危审批需人在场」逻辑矛盾，整体推 v2

### 6.2 v2 留档（不在本规格验收范围）
- 模式 B：AI 独立建连，需凭据自动注入 + host key 自动策略 + 双连仲裁
- SSE / Streamable HTTP 传输
- Resources 订阅
- 无人值守 CI 集成
- Desktop Extensions（.mcpox）一键安装包

**Non-goals 强制门状态**：✅ 显式化完成（Round 6 + Round 7）

---

## 7. Decision Boundaries（什么需用户确认）

**重大决策必须呈现候选方案 + 推荐 + 行业依据，由用户拍板**，agent 不得自决：

- **D1 二进制形态**：双二进制 vs 单二进制子命令（推荐双二进制，因 Tauri windows_subsystem 链接时定死）
- **D2 会话共享方式**：named pipe 桥接 vs 独立会话 vs 两阶段（推荐两阶段：v1.0 独立会话快速落地，v1.1 补 pipe 桥接）
- **D3 MCP SDK**：官方 rmcp vs 社区 sdk vs 手写（推荐 rmcp ~1.7）
- **D4 host key 处理**：仅服务已信任资产 vs 自动拒绝未知 vs 跨进程问 GUI（推荐仅服务已信任）
- **D5 危险命令检测位置**：翻译成 Rust 共享 vs 上移 core vs pipe 代检测（推荐翻译 Rust）
- **D6 Tools 清单**：哪些 SSH 能力暴露给 AI（推荐 12 个，ssh_exec 默认拒）
- **D7 Resources 清单**：暴露哪些数据作 URI（推荐 3 静态 + 会话日志 template）
- **D8 Prompts 清单**：做哪些诊断模板（推荐 3 个）
- **D9 默认名单内容**：白/黑/黄/默认拒的初始内容（推荐 fail-secure 默认拒）

**Decision Boundaries 强制门状态**：✅ 显式化完成（Round 8）

---

## 8. Constraints（硬约束）

### 8.1 复用现有资产
- `src-tauri/src/ssh.rs` 的 `SshSessionManager` + russh 后端
- `src-tauri/src/lib.rs` 的 `AppState`（`ssh_sessions` 已是 `Arc<AsyncMutex<...>>`，可直接 clone Arc 共享给 pipe task）
- 现有 45 个 `#[tauri::command]`（几乎 1:1 映射 MCP Tools）
- `src/lib/dangerousCommands.js` 的 16 条正则（翻译为 Rust）
- `src/components/shell/GlobalModals.vue` 弹窗中枢（新增 `mcpApproval` 分支）
- `crates/myshelltool-core` 持久化层（资产/凭据加载）

### 8.2 平台与协议
- Windows 优先
- MCP 协议基线：2025-03-26 版本（rmcp 1.7 实现基线）
- stdio 传输（JSON-RPC over stdin/stdout）
- **MCP 子进程不得向 stdout 打印任何日志**（会破坏协议帧解析），日志走 stderr 或文件

### 8.3 不破坏现有契约
- 不改现有 Tauri 命令签名
- 不破坏 GUI 功能（回归测试通过）
- 不降低凭据安全水位（仍走 SecretStore，绝不外泄）

---

## 9. Testable Acceptance Criteria

### AC1：Claude Desktop 能配置并发现工具
- 用户在 `%APPDATA%\Claude\claude_desktop_config.json` 配置 `myshelltool-mcp.exe --mcp-stdio`
- Claude Desktop 启动后能在工具列表看到 ≥ 10 个 myshelltool tools
- 验证：手动配置 + 截图

### AC2：只读命令白名单自动执行
- AI 调用 `disk_usage` / `system_status` 等只读工具
- 命中白名单 → 自动执行，无弹窗
- 返回结果给 AI
- 验证：AI 对话实测 + 不触发 GlobalModals

### AC3：高危命令触发三段式弹窗
- AI 调用含 `rm -rf` / `mkfs` / `dd of=/dev/` 等命令
- 触发 GlobalModals 的 `mcpApproval` 分支
- 弹窗同时显示：AI 自述意图 + 真实命令 + 后果预测
- 用户点「拒绝」→ 命令不执行，AI 收到 `isError: true`
- 用户点「确认」→ 命令执行
- 验证：弹窗截图 + 三段字段存在性 + 双路径行为

### AC4：GUI 未运行弹性降级
- 关闭 myshelltool GUI
- AI 调用 SSH 类工具 → MCP 子进程尝试拉起 GUI（重试 N 次）→ 仍失败 → 返回明确错误「SSH 会话不可用，已降级为只读」
- AI 调用本地数据类工具（`list_assets`）→ 正常返回
- 验证：进程状态 + 错误消息 + 只读工具仍可用

### AC5：不破坏现有 GUI 功能
- `npm run build` 通过
- `npm run test:ui`（ui-smoke + ui-host-key）通过
- `cd src-tauri && cargo check` 通过
- GUI 手动连 SSH / SFTP / 隧道 / 资源监控全部正常
- 验证：四个命令的 exit code + 手动操作

### AC6：MCP 子进程 stdout 纯净
- `myshelltool-mcp.exe --mcp-stdio` 运行时 stdout 只有 JSON-RPC 帧
- 所有日志走 stderr 或文件
- 验证：重定向 stdout 到文件检查无杂质日志行

### AC7：host key 安全门
- AI 调用未在 GUI 信任过的资产 → MCP 返回错误「请在 GUI 先完成首次连接」
- 已信任资产正常执行
- 验证：未信任资产错误消息 + 已信任资产成功

---

## 10. 决策点详表（D1-D9，待用户拍板）

### D1：二进制形态 ⭐推荐 D1-A 双二进制
| 候选 | 方案 | 推荐度 |
|------|------|--------|
| **D1-A** | 双二进制：`myshelltool.exe`(GUI) + `myshelltool-mcp.exe`(console)，共享 myshelltool_lib | ⭐⭐⭐⭐⭐ |
| D1-B | 单二进制 `--mcp-stdio` 子命令 | ❌ 技术不可行（windows_subsystem 链接时定死）|
| D1-C | 单二进制 + AttachConsole/AllocConsole | ❌ 脆弱不可靠 |

**行业依据**：Git/cargo/docker 双二进制惯例；Tauri 社区确认 GUI+CLI 必须双二进制。

### D2：会话共享方式 ⭐推荐 D2-C 两阶段
| 候选 | 方案 | 推荐度 |
|------|------|--------|
| D2-A | named pipe 桥接（完整兑现访谈契约）| ⭐⭐⭐⭐ |
| **D2-B** | MCP 进程独立建会话（最快落地）| ⭐⭐⭐⭐ MVP |
| **D2-C** | 两阶段：v1.0 用 D2-B，v1.1 补 D2-A | ⭐⭐⭐⭐⭐ |

**理由**：访谈核心诉求是「能用」，D2-B 让 Claude 跑通 SSH 是最高优先。v1.0 是简化版，文档须显式标注。

### D3：MCP SDK ⭐推荐 D3-A 官方 rmcp
| 候选 | 方案 | 推荐度 |
|------|------|--------|
| **D3-A** | 官方 rmcp（crates.io 1.7.0，modelcontextprotocol 组织维护）| ⭐⭐⭐⭐⭐ |
| D3-B | 社区 rust-mcp-sdk | ❌ 非官方 |
| D3-C | 手写 JSON-RPC | ❌ 维护成本高 |

**版本策略**：Cargo.toml 用 `rmcp = "~1.7"`，注释标 MCP 协议 2025-03-26 基线。

### D4：host key / keyboard-interactive 处理 ⭐推荐 D4-A
| 候选 | 方案 | 推荐度 |
|------|------|--------|
| **D4-A** | 仅服务已在 GUI 信任过的资产（known_hosts 已记录 + 凭据已存）| ⭐⭐⭐⭐⭐ |
| D4-B | known_hosts 未知自动拒绝；keyboard-interactive 返回错误 | ⭐⭐⭐ |
| D4-C | 跨进程送 GUI 弹窗（依赖 D2-A）| ⭐⭐ |

**行业依据**：SSH 首次指纹验证必须人工；headless 自动信任是 known_hosts poisoning 入口。

### D5：危险命令检测位置 ⭐推荐 D5-A 翻译 Rust
| 候选 | 方案 | 推荐度 |
|------|------|--------|
| **D5-A** | 翻译成 Rust `dangerous_commands.rs`，GUI+MCP 共享 | ⭐⭐⭐⭐⭐ |
| D5-B | 上移到 myshelltool_core，前端改读 core | ⭐⭐ 改造成本高 |
| D5-C | MCP 经 pipe 让 GUI 代检测 | ⭐⭐ 依赖 D2-A |

**翻译注意**：JS `/i` 标志 → Rust `RegexBuilder::case_insensitive(true)`；`\b` 与字面量在 Rust regex 语义一致。

### D6：MCP Tools 清单（v1 推荐 12 个）
| Tool | 后端命令 | 危险等级 | 审批 |
|------|---------|---------|------|
| `list_assets` | list_connection_assets | 只读 | 自动 |
| `list_sessions` | 内存活跃会话 | 只读 | 自动 |
| `system_status` | ssh_write（预定义 uptime/free/top -bn1 组合）| 只读 | 自动 |
| `disk_usage` | ssh_write（df -h）| 只读 | 自动 |
| `service_status` | ssh_write（systemctl status <svc>）| 只读 | 自动 |
| `ssh_exec` | ssh_write（任意命令）| **高** | 三段式弹窗（默认拒）|
| `sftp_list` | sftp_list_dir | 只读 | 自动 |
| `sftp_read` | sftp_read_file | 只读 | 自动 |
| `sftp_upload` | sftp_upload_start/chunk/finalize | **高** | 三段式弹窗 |
| `sftp_remove` | sftp_remove | **高** | 三段式弹窗 |
| `resource_monitor_snapshot` | resource_monitor_snapshot | 只读 | 自动 |
| `tunnel_start`/`stop` | tunnel_* | **高** | 三段式弹窗 |

**取舍**：`ssh_exec` 默认拒，只读快捷工具默认放。最小暴露面原则（least privilege）。

### D7：MCP Resources 清单（v1）
| Resource | URI | 说明 |
|----------|-----|------|
| 资产清单 | `myshelltool://assets` | 所有连接资产元数据（不含凭据）|
| 活跃会话 | `myshelltool://sessions` | 当前连上的 SSH 会话 |
| known_hosts | `myshelltool://known-hosts` | 已信任主机指纹 |
| 会话日志（template）| `myshelltool://sessions/{id}/log` | 某会话最近输出（RFC 6570 模板）|

**订阅能力 v2 再做**（MCP 规范 optional capability）。

### D8：MCP Prompts 清单（v1 推荐 3 个）
| Prompt | arguments | 用途 |
|--------|-----------|------|
| `diagnose_server` | `{session_id}` | 诊断引导：CPU/磁盘/内存/关键服务 |
| `audit_security` | `{session_id}` | 安全审计：最近登录/异常进程/开放端口 |
| `cleanup_disk` | `{session_id}` | 磁盘清理：大文件/旧日志 |

**行业依据**：MCP Prompts 是 server 端 workflow，动态返回 messages[]。

### D9：默认白/黄/黑名单
**白名单（自动执行）**：
```
只读：ls cat less head tail grep find ps top -bn1 free df du uname who w last uptime netstat ss
服务：systemctl status <svc>
容器：docker ps docker logs <c>
日志：journalctl --since（只读）
监控：cat /proc/* cat /etc/os-release
```

**黑名单（永远弹窗）**：复用 `dangerousCommands.js` 16 条（rm -rf / mkfs / dd of=/dev/ / fork bomb / shutdown-reboot-halt-poweroff / chmod -R / / chown -R / / iptables -F / curl|sh 等）

**黄名单（用户按资产配置）**：默认空，用户加（如 nginx -t、docker restart <c>）

**默认拒**：不在以上任何名单 → 弹窗（fail-secure）

---

## 11. Assumptions Exposed + Resolutions

### A1：stdio vs SSE 架构分叉（Round 3 Contrarian）
- **假设**：「两模式可配」隐含 stdio 和 SSE 都要支持
- **施压**：两者架构约束相反（stdio 子进程 vs SSE 常驻服务），同时做会得到四不像
- **解决**：用户接受架构师推荐，定 stdio 为 v1 主力，SSE 推 v2

### A2：审批本质（Round 5 Ontologist）
- **假设**：「高危审批」的「高危」判断由谁做
- **施压**：规则匹配追不上 AI 语义逃逸；AI 自评不可信
- **解决**：用户选「命令层兜底 + 意图层呈现」——决策由规则做（可预测），意图仅作信息辅助识破伪装。避开了「AI 评 AI」的信任陷阱

### A3：模式 B 与审批的逻辑矛盾（Round 7 Simplifier）
- **假设**：v1 包含模式 B（AI 独立建连）
- **施压**：模式 B「无人值守」与「高危审批需人在场」逻辑自相矛盾
- **解决**：用户接受推模式 B 到 v2，v1 只做模式 A，逻辑自洽

### A4：windows_subsystem 技术约束（核查发现，非访谈）
- **假设**（访谈 Round 2 隐含）：`myshelltool.exe --mcp-stdio` 单二进制
- **事实**：`windows_subsystem = "windows"`（main.rs:1）让 release 子进程 stdin/stdout 不可用，Claude 读不到 MCP 输出；subsystem 链接时定死，无法运行时切换
- **解决**：架构修正为双二进制（D1-A），named pipe 桥接兑现「复用 GUI 会话」（D2-A/C），访谈所有决策均仍兑现

---

## 12. Pressure-Pass Findings

**Round 9 回访 Round 2「两模式可配」**：
- Round 2 用户选「两模式可配」
- Round 7 已将模式 B 推 v2，模式 A 留 v1
- Round 9 深挖：模式 A 下「Claude 拉起 MCP server 但 GUI 没运行」的降级行为未定义
- 用户给出**优于三选项的组合策略**：重试拉起 GUI（递增间隔）→ 仍失败 → 只读降级
- **修正**：访谈契约「两模式可配」精确化为「v1 模式 A + 弹性降级；v2 模式 B」

---

## 13. 残余风险（必须显式留档）

| 风险 | 等级 | 缓解 | 残余 |
|------|------|------|------|
| 提示词注入（AI 被远程内容诱导执行恶意命令）| 🔴 致命 | 三段式弹窗让人识破 | **无技术拦截**，靠人工，行业级未解难题 |
| AI 语义逃逸（拆分命令绕过黑名单）| 🟠 高 | 默认拒兜底 | 不认识的就弹窗，仍有疲劳风险 |
| named pipe 本地越权（同机其他进程连 pipe）| 🟡 中 | pipe ACL 限制当前用户 | Windows pipe 默认同会话隔离，非完全隔离 |
| MCP 协议 breaking change | 🟡 中 | 锁 rmcp ~1.7 | 大版本升级需迁移工作量 |
| Claude Desktop 配置漂移 | 🟢 低 | 文档化配置示例 | 客户端版本变更需复查 |

**残余风险传递规则**：进入实施阶段时，以上风险必须显式继承，不得在实施中静默吞掉。

---

## 14. Brownfield 证据 vs 推断

| 结论 | 类型 | 证据 |
|------|------|------|
| AppState.ssh_sessions 是 Arc<AsyncMutex<...>> | 证据 | lib.rs:16 |
| 现有 45 个 Tauri 命令 | 证据 | lib.rs:230-274 invoke_handler |
| dangerousCommands.js 有 16 条正则 | 证据 | dangerousCommands.js:4-21 |
| GlobalModals.vue 用 modal.type switch | 证据 | GlobalModals.vue:85-103 |
| main.rs 已有 windows_subsystem = "windows" | 证据 | main.rs:1 |
| rmcp 1.7.0 已正式发布 | 证据 | crates.io 核查 2026-06-16 |
| Tauri 子命令不可运行时切换 subsystem | 推断（基于 Rust 链接模型 + 社区帖）| Reddit r/rust + dev.to |
| Claude Desktop 不传 PTY 给 stdio 子进程 | 推断（基于 Anthropic 工程实践）| natoma.ai 指南 |

---

## 15. 技术上下文发现

- **MCP 协议**：2025-03-26 版本，SSE 已 deprecated，新版走 Streamable HTTP；stdio 不变
- **rmcp**：官方 Rust SDK，crates.io 1.7.0，tokio 运行时，stdio 是核心支持
- **Tauri 2 子命令**：tauri-plugin-cli 不适合 headless stdio；必须双二进制
- **Windows named pipe**：tokio::net::windows::named_pipe 原生支持（项目已用 tokio），零新依赖
- **Claude Desktop 配置**：`%APPDATA%\Claude\claude_desktop_config.json`，command/args/env 三元组，绝对路径双反斜杠转义
- **Cursor 配置**：`.cursor/mcp.json`（项目级）或 Settings GUI（全局），格式同 Claude
- **Cline 配置**：`cline_mcp_settings.json`，⚠️ 有 auto-approve 开关，MCP 工具必须自带审批门

---

## 16. 访谈 Transcript 摘要

| Round | 目标维度 | 关键决议 |
|-------|---------|---------|
| R0 Preflight | 上下文基线 | brownfield，Standard 档位，阈值 0.20 |
| R1 | Intent | 主要场景 1（AI 助手）+ 2（自动化节点），myshelltool 作被动服务方 |
| R2 | Outcome | 会话来源「两模式可配」（后被 R7 修正为 v1 模式 A）|
| R3 | Scope + Contrarian | stdio vs SSE 架构分叉施压 → 定 stdio 为 v1 主力 |
| R4 | Constraint | 审批策略三层 + 三段式弹窗 |
| R5 | Ontologist | 审批本质：命令层兜底 + 意图层呈现（决策由规则做，意图仅辅助）|
| R6 | Non-goals 门 | 排除 SSE 传输（其他保留）|
| R7 | Simplifier | 模式 B 与审批逻辑矛盾 → 模式 B 推 v2 |
| R8 | Decision Boundaries 门 | 重大决策需候选+推荐+依据 |
| R9 | Pressure Pass | GUI 未运行弹性降级（重试拉起 → 只读）|

**歧义轨迹**：100% → 65% → 55% → 50% → 42% → 38% → 35% → 28% → 22% → **18%**

---

## 17. 后续执行桥

本规格为**当前需求单一信息源**。实施计划必须继承：
- Intent / Outcome / In-Scope / Non-goals（不可漂移）
- Decision Boundaries（D1-D9 已全部按推荐锁定，见 §10）
- Acceptance Criteria（AC1-AC7 为验收硬门槛）
- 残余风险（不得静默吞掉）

**实施计划**：见 [`docs/plans/MCP服务接入-实施计划.md`](../plans/MCP服务接入-实施计划.md)（按架构层拆解：bin 骨架 → rmcp server → Tools → Resources → Prompts → 审批集成 → 降级逻辑，每层带文件级改动 + cargo/npm 验证命令）。
