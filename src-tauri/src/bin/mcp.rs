//! myshelltool-mcp — MCP stdio server 二进制入口（console 子系统）。
//!
//! D1 决策（双二进制）：此 bin **不加** `#![cfg_attr(not(debug_assertions),
//! windows_subsystem = "windows")]`，使 release 构建保持 console 子系统，
//! Claude Desktop / Cursor 拉起子进程后 stdin/stdout JSON-RPC 才可用。
//!
//! 复用 `myshelltool_lib` 的 `run_mcp_stdio()` 主循环，与 GUI bin 共享全部业务代码。

#[tokio::main]
async fn main() {
    // 初始化 MCP 专用 logger（stderr + 文件，绝不污染 stdout）
    myshelltool_lib::init_mcp_logger();

    // 进入 MCP stdio server 阻塞循环
    if let Err(e) = myshelltool_lib::run_mcp_stdio().await {
        eprintln!("[myshelltool-mcp] fatal: {}", e);
        std::process::exit(1);
    }
}
