//! MCP server 接入模块。
//!
//! 按 Layer 拆解（见 docs/plans/MCP服务接入-实施计划.md）：
//! - Layer 2：`server` — rmcp stdio server 主循环 + ServerHandler（M2 完成）
//! - Layer 3：`tools` — MCP Tools（M3+M4 完成 9 个工具）
//! - Layer 4：`resources` — 3 静态资源 + 1 template（M5 完成）
//! - Layer 5：`prompts` — 3 个诊断 prompt（M5 完成）
//! - Layer 6：`approval` — 三层审批 + 三段式拒绝（M4 v1.0 完成）

pub mod approval;
pub mod prompts;
pub mod resources;
pub mod server;
pub mod tools;
