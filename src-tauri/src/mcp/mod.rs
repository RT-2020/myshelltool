//! MCP server 接入模块。
//!
//! 按 Layer 拆解（见 docs/plans/MCP服务接入-实施计划.md）：
//! - Layer 2：`server` — rmcp stdio server 主循环 + ServerHandler（当前阶段）
//! - Layer 3：`tools` — 13 个 MCP Tools
//! - Layer 4：`resources` — 3 静态资源 + 1 template
//! - Layer 5：`prompts` — 3 个诊断 prompt
//! - Layer 6：`approval` — 三层审批 + 三段式弹窗回路

pub mod server;
