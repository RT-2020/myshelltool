//! myshelltool-gateway —— 网关模式内核。
//!
//! 跳板机上的 myshelltool 以 `--gateway` headless 常驻：内嵌 SSH server（仅回环、
//! 仅 publickey、仅 direct-tcpip）+ 挂载器（出站 SSH 连公网 VPS，remote forward
//! 把内嵌 server 暴露到 VPS 挂载口）。本 crate 无 Tauri 依赖，可独立 cargo test
//! （src-tauri 测试二进制受 Tauri runtime DLL 限制在本机跑不起来——安全判据必须
//! 可真跑，与 myshelltool-core 同一工程理由）。
//!
//! 设计决策见 docs/计划/网关模式-实施计划.md（v4）。

pub mod config;
pub mod keys;
pub mod mount;
pub mod run;
pub mod server;
