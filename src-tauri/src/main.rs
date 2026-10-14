#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 网关模式分流（v0.22）：必须发生在任何 Tauri 初始化之前——静态窗口配置随
    // generate_context! 生效，走 Builder 必创建主窗口拉起 WebView/MCP。headless
    // 分支详见 gateway_cli.rs（AttachConsole / 数据目录 / gateway.log）。
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--gateway") {
        let code = myshelltool_lib::gateway_cli::gateway_main(&args);
        std::process::exit(code);
    }
    myshelltool_lib::run();
}
