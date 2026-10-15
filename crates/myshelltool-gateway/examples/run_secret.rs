//! 【验证辅助临时工具，用后删除】口令模式网关 runner（模拟跳板机 GUI setup 后的运行态）。
use myshelltool_gateway::{config as gw_config, run as gw_run};
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // args: <relay主机> <口令> [listen_port]
    let mut rt = gw_config::GatewayRuntimeConfig::from_secret(&args[0], 22222);
    if let Some(p) = args.get(2) { rt.listen_port = p.parse().unwrap(); }
    let loaded = gw_run::loaded_from_secret(&rt, &args[1]).expect("load from secret");
    gw_run::run_loaded(loaded).await.expect("run");
}
