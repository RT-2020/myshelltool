//! `--gateway` headless 入口编排（main.rs 在进入 Tauri 之前分流到这里）。
//!
//! 职责：参数解析（朴素 `args` 匹配，不引 clap）、数据目录解析（与 GUI 同目录：
//! `MYSHELLTOOL_DATA_DIR` 优先，否则 `%APPDATA%\<identifier>`——identifier 与
//! tauri.conf.json 同步维护）、FileLogger（gateway.log 独立文件）、SecretStore
//! （DpapiCodec）、两条子命令：
//! - `--gateway --import <file>`：交互读口令（或 `MYSHELLTOOL_GATEWAY_PASSWORD`
//!   环境变量）→ `gateway::run::import_bundle` → 摘要回显，退出码 0/2
//! - `--gateway`：`gateway::run::run_gateway` + CtrlC，常驻

use std::path::PathBuf;

use myshelltool_gateway::run as gw_run;

use crate::dpapi_codec::DpapiCodec;
use crate::gateway_console::{attach_parent_console, console_out, read_password};
use crate::file_logger::FileLogger;

use myshelltool_core::SecretStore;

/// 与 tauri.conf.json 的 `identifier` 同步（改 identifier 必须两处一起改）。
const APP_IDENTIFIER: &str = "com.redtei.myshelltool";

/// 挂载口令的环境变量备选通道（比命令行参数安全：不进进程列表；仍优先交互读）。
const PASSWORD_ENV: &str = "MYSHELLTOOL_GATEWAY_PASSWORD";

/// headless 数据目录：MYSHELLTOOL_DATA_DIR（绝对路径）优先，否则 %APPDATA%\<identifier>。
/// 语义与 lib.rs `mcp_data_dir` 对齐（空/相对值忽略并回退），但来源是手拼
/// （无 Tauri runtime 可解析 app_data_dir）。
fn gateway_data_dir() -> Result<PathBuf, String> {
    if let Some(v) = std::env::var_os("MYSHELLTOOL_DATA_DIR") {
        let s = v.to_string_lossy();
        if !s.trim().is_empty() {
            let dir = PathBuf::from(&v);
            if dir.is_absolute() {
                return Ok(dir);
            }
            log::warn!("MYSHELLTOOL_DATA_DIR 不是绝对路径（{s}），已忽略并回退默认目录");
        }
    }
    let appdata = std::env::var_os("APPDATA")
        .ok_or_else(|| "无法定位 %APPDATA%（数据目录解析失败）".to_string())?;
    Ok(PathBuf::from(appdata).join(APP_IDENTIFIER))
}

fn init_logger(data_dir: &std::path::Path) {
    if let Ok(logger) = FileLogger::new(&data_dir.join("logs").join("gateway.log")) {
        let _ = log::set_boxed_logger(Box::new(logger));
        log::set_max_level(log::LevelFilter::Info);
    }
    // set_boxed_logger 失败 = GUI 进程已装过 logger（不该发生：main 分流在 run() 之前）；
    // 失败则日志仅进控制台，不致命。
}

fn secret_store(data_dir: &std::path::Path) -> SecretStore {
    SecretStore::new(data_dir.join("credentials"), Box::new(DpapiCodec))
}

fn read_bundle_password() -> Result<String, String> {
    // 优先级：环境变量（管道/自动化场景）→ 交互读（CONIN$ 关回显）
    if let Ok(p) = std::env::var(PASSWORD_ENV) {
        if !p.trim().is_empty() {
            return Ok(p);
        }
    }
    read_password("请输入 bundle 加密口令: ")
        .map_err(|e| format!("读取口令失败（{e}）：请在终端/PowerShell 中运行，或设置 {PASSWORD_ENV} 环境变量"))
}

/// CLI 入口。返回进程退出码（main.rs 拿到后 std::process::exit）。
pub fn gateway_main(args: &[String]) -> i32 {
    attach_parent_console();

    let data_dir = match gateway_data_dir() {
        Ok(d) => d,
        Err(e) => {
            console_out(&format!("[gateway] 启动失败：{e}"));
            return 2;
        }
    };
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        console_out(&format!("[gateway] 创建数据目录失败：{e}"));
        return 2;
    }
    init_logger(&data_dir);

    // 子命令：--import <file>
    if let Some(pos) = args.iter().position(|a| a == "--import") {
        let file = args.get(pos + 1).cloned().unwrap_or_default();
        if file.is_empty() {
            console_out("[gateway] 用法：myshelltool --gateway --import <bundle文件路径>");
            return 2;
        }
        let password = match read_bundle_password() {
            Ok(p) => p,
            Err(e) => {
                console_out(&format!("[gateway] {e}"));
                return 2;
            }
        };
        let store = secret_store(&data_dir);
        return match gw_run::import_bundle(std::path::Path::new(&file), &password, &store, &data_dir)
        {
            Ok(rt) => {
                console_out(&format!(
                    "[gateway] 导入成功：VPS {}:{}，挂载口 {}（回环 server :{}）",
                    rt.vps_host, rt.vps_port, rt.mount.remote_port, rt.listen_port
                ));
                console_out("[gateway] 启动网关：myshelltool --gateway（服务化模板见部署指南）");
                0
            }
            Err(e) => {
                console_out(&format!("[gateway] 导入失败：{e}"));
                2
            }
        };
    }

    // 默认：常驻运行网关
    let store = secret_store(&data_dir);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");
    let dir = data_dir.clone();
    let code = runtime.block_on(async {
        tokio::select! {
            r = gw_run::run_gateway(&dir, &store) => {
                // run_gateway 只有配置/绑定级致命错误才返回 Err
                match r {
                    Ok(()) => 0, // 理论不可达（mount loop 不返回），保底
                    Err(e) => {
                        log::error!("gateway: 退出：{e}");
                        console_out(&format!("[gateway] 运行失败：{e}"));
                        2
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                log::info!("gateway: 收到 CtrlC，退出");
                console_out("[gateway] 已停止");
                0
            }
        }
    });
    code
}
