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

## Part A：elicitation 审批（M7）

- A1: Cargo.toml 加 `elicitation` feature
- A2: approval.rs 加 `RequestElicitation(ElicitationInfo)` 变体
- A3: server.rs call_tool 接入 `context.peer.elicit()`，不支持时降级 Reject
- A4: ApprovalResponse 类型（elicit 泛型参数）
- A5: 验证 Claude 实测高危命令弹确认

## Part B：会话复用 named pipe（M8）

- B1: ssh.rs 新增 `pub async fn exec_on_session`
- B2: lib.rs setup hook spawn pipe server
- B3: 新建 mcp/pipe.rs（pipe 协议 JSON over named pipe）
- B4: tools.rs exec_on_asset 优先走 pipe，失败降级 headless
- B5: 会话映射（asset_id → host:port 匹配 GUI sessions）
- B6: 验证 GUI 在线复用 + 离线降级

## Part C：回归发布（M9）

- C1: 全量回归（build / test:core / cargo check）
- C2: AC8-AC11 验收
- C3: 文档更新 + 合并 + tag v0.3.0

## 估时：5-7 天（M7: 2-3天 / M8: 2-3天 / M9: 1天）
