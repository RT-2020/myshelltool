//! MCP server 接入模块。
//!
//! v1.4：MCP 内嵌 GUI（Streamable HTTP transport）取代双进程架构。
//! - `http_server` — Streamable HTTP server 主循环（GUI 进程内，取代 v1.0 stdio）
//! - `server` — rmcp ServerHandler 实现（协议层，transport 无关）
//! - `tools` — MCP Tools（9 个工具）
//! - `resources` — 3 静态资源 + 1 template
//! - `prompts` — 3 个诊断 prompt
//! - `approval` — 审批判定（v1.4 改为同进程弹窗，删 v1.1 pipe 委托）
//!
//! 已删除（v1.4）：
//! - `pipe` — named pipe 桥接（双进程时 MCP exe 复用 GUI 会话用，内嵌后无需）
//!
//! 重写（v1.4）：
//! - `probe` — 从「一次性 spawn 子进程探测」改为「HTTP 健康检查」（不再 spawn）

pub mod approval;
pub mod http_server;
pub mod probe;
pub mod prompts;
pub mod resources;
pub mod server;
pub mod tools;
