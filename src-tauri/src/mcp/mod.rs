//! MCP server 接入模块。
//!
//! v1.4：MCP 内嵌 GUI（Streamable HTTP transport）取代双进程架构。
//! - `http_server` — Streamable HTTP server 主循环（GUI 进程内，取代 v1.0 stdio）
//! - `server` — rmcp ServerHandler 实现（协议层，transport 无关）
//! - `tools` — MCP Tools（9 个工具）
//! - `resources` — 3 静态资源 + 1 template
//! - `prompts` — 3 个诊断 prompt
//! - `approval` — 审批判定（v1.5 改为同进程弹窗，删 v1.1 pipe 委托）
//! - `config` — v2 拦截等级配置（Minimal/Strict，mcp-config.json 持久化）
//! - `execution_log` — v2 工具执行日志（mcp-execution-log.json，30 天惰性清理）
//!
//! 已删除（v1.4）：
//! - `pipe` — named pipe 桥接（双进程时 MCP exe 复用 GUI 会话用，内嵌后无需）
//!
//! 重写（v1.4）：
//! - `probe` — 从「一次性 spawn 子进程探测」改为「HTTP 健康检查」（不再 spawn）

pub mod approval;
pub mod job_store; // v0.20（C1）：长任务状态机（内存态/TTL 30min/断连取消）
pub mod job_tools; // v0.20（C1）：ssh_exec_async/job_status/job_output/job_cancel
pub mod orchestrator; // v0.20（3-6 月段）：批量编排层（exec_many 内核；GUI 批量面板后消费）
pub mod output_cache; // v0.20（B2）：截断输出 ring buffer + cursor 取回（read_output 工具的载体）
pub mod commands; // v0.20：面板命令层（自 lib.rs 迁出）
pub mod config;
pub mod execution_log;
pub mod file_policy;
pub mod file_tools;
pub mod http_server;
pub mod probe;
pub mod prompts;
pub mod registry; // v0.20（A2）：工具注册表——风险/审批策略/授权需求/审计声明的单一事实源
pub mod readonly_tools; // v0.20（C3）：只读工具四件（journal_query/port_listen/process_list/file_search）
pub mod registry_schemas; // schema 构造（3-6 月段刀自 registry 拆出）
pub mod registry_labels; // audit/approval 文案构造
pub mod registry_handlers; // handler 包装
pub mod resources;
pub mod server;
pub mod service_tools; // v0.20（C4）：service_control + 隧道工具
pub mod session_pool; // v0.20（B3）：headless 连接池（GUI 会话 → 池 → 新建 的中间层）
pub mod sftp_ops;
pub mod tools;
