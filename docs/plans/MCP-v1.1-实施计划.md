---
title: MCP Server v1.1 实施计划 — elicitation 审批 + 会话复用
spec: docs/specs/MCP服务接入-需求规格.md
previous: docs/plans/MCP服务接入-实施计划.md（v1.0 已完成）
status: in_progress
created_at: 2026-06-17
---

# MCP Server v1.1 实施计划

> 解决 v1.0 的两个妥协：
> ① 审批在客户端界面内完成（MCP elicitation，不切 myshelltool GUI）
> ② 复用 GUI 已建立的 SSH 会话（named pipe，不重复建连）

## 架构总览

```
Claude / Cursor（客户端）
   └── myshelltool-mcp.exe（stdio 子进程）
          │
          ├─ 高危命令 → context.peer.elicit() → 客户端弹确认（三段式）
          │                                       accept → 执行 / decline → 拒绝
          │
          └─ 命令执行 → named pipe \\.\pipe\myshelltool-mcp
                          ↓
myshelltool.exe（GUI）→ AppState.ssh_sessions → 复用已建立会话
```

## Part A：elicitation 审批（M7）✅ 已完成（提交 d9b94d5）

- ✅ A1: Cargo.toml 加 `elicitation` + `schemars` feature
- ✅ A2: approval.rs `evaluate()` 对高危/未知命令返回 `RequestElicitation(ElicitationInfo)`
- ✅ A3: server.rs call_tool 接入 `context.peer.elicit()`，不支持时降级 NotSupported
- ✅ A4: `ApprovalForm { confirmed: bool }` + `rmcp::elicit_safe!` 宏
- ⏳ A5: Claude 实测高危命令弹确认（待 M9 集成验证）

清理：移除了 `ApprovalDecision::Reject` 死变体（v1.1 不再进程内拒绝），
3 个测试断言更新为 `RequestElicitation`。`to_rejection()` 保留作为
elicitation NotSupported 的降级文案。

## Part B：会话复用 named pipe（M8）✅ 已完成（代码 + 单测验证）

- ✅ B1: `ssh.rs` 新增 `exec_on_session`（开新 channel 一次性 exec，不干扰 PTY）
        + `SessionMeta { host, port, username }` 映射表（ssh_connect 写入/ssh_disconnect 清理）
        + `find_session_by_host`（host:port 全匹配优先，退化为仅 host:port）
        + `list_sessions_with_meta`
- ✅ B2: `lib.rs` setup hook `tokio::spawn(run_pipe_server(ssh_mgr))`
        （照 resource_monitor 的 Arc 抽取范式）
- ✅ B3: 新建 `mcp/pipe.rs` —— JSON 行协议（扁平字段，零歧义）
        - GUI 端 `run_pipe_server`：循环 accept + per-client spawn
        - MCP 端 `PipeClient`：开两个独立连接凑读写分离（单工轮转协议）
        - 三个方法：`exec` / `resolve_session` / `list_sessions`
        - `resolve_and_exec` 便捷函数（resolve + exec 两步合一）
- ✅ B4: `tools.rs exec_on_asset` 双路径：先 pipe 复用 → miss 降级 headless
        + `list_sessions` 从桩升级为走 pipe 返回真实会话
- ✅ B5: 会话映射内嵌 B4（asset 库解析 host:port:username → pipe.find_session_by_host）
- ✅ B6: cargo build 成功 + core 21 测试全绿（0 error, 0 warning）

### 验证结果（2026-06-17）

```
cargo check   → 0 error, 0 warning（彻底清零，含 M7 遗留 Reject 死代码）
cargo build   → Finished dev profile in 1m18s（双二进制产出正常）
test:core     → 21 passed, 0 failed
```

⏳ **待 M9 实测**：GUI 在线时 pipe 复用命中 + GUI 离线时降级 headless 的端到端验证
（需启动 GUI + Claude Desktop 连 MCP 跑 disk_usage 观察日志路径）。

## Part C：回归发布（M9）⏳ 待做

- C1: 全量回归（build / test:core / cargo check）✅ 已跑通
- C2: AC8-AC11 验收（elicitation 实测 + pipe 实测）
- C3: 文档更新（README 补 v1.1 会话复用说明）+ 合并 master + tag v0.3.0

## 估时：5-7 天（M7: 2-3天 ✅ / M8: 2-3天 ✅ / M9: 1天 ⏳）
