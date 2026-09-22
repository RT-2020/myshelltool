// v1.3 Gist 同步加密内核（独立模块，纯函数可单测）
pub mod crypto;
// v2.6 危险命令分类（白/黄/黑/Unknown 四层）+ shell 命令分段：GUI 终端守卫与
// MCP 审批共享的单点真相。原在 src-tauri/src/，迁入 core 以便 `cargo test` 真正
// 执行（src-tauri 的测试二进制因 Tauri runtime DLL 缺失跑不起来，安全判据不可测）。
pub mod dangerous_commands;
// v2.6 命令文本脱敏（执行日志/应用日志落盘前的凭据红线，见模块注释）
pub mod redact;
// v2.7 GitHub Device Flow 轮询判定（传输抖动 vs 授权被拒必须分开：前者退避重试、
// 后者立即停止。见模块注释的事故说明）
pub mod oauth_flow;
// v2.6 远端文件内容编码判定（拒绝 lossy「解码」二进制/非 UTF-8，见模块注释）
pub mod remote_text;
// v2.8 端口覆盖环境变量三态解析（MYSHELLTOOL_MCP_PORT：未设置/无效/生效，
// 防止把「配置了但值不可用」静默折叠成「未配置」，见模块注释）
pub mod port_env;
// v2.7 恢复密码生成（同步备份的"可带走凭据"：应用生成高熵随机密码，用户零输入即可用，
// 换机时可在应用内查看/复制带走。见模块注释的安全定位说明）
pub mod recovery_code;
pub mod shell;
// v2.8 远程 /proc/* 采样输出纯解析层（网络默认路由口径/磁盘栈式设备去重/伪 FS
// 过滤/分段失败语义）。原在 src-tauri/src/resource_monitor.rs，迁入 core 让
// 测试真正运行（同 dangerous_commands 迁入纪律，见模块注释）
pub mod proc_parse;
// v0.18 内置编辑器编码内核：本地编码白名单解码/编码（绝不 lossy）+ EOL 三态
// 检测/应用 + UTF-8 BOM 剥离/还原（见模块注释的「宁严不猜」纪律）
pub mod text_codec;
// v0.20 MCP HTTP 入口 token 鉴权判定（A1）：URL 内嵌/Bearer 两形态 + 段边界 +
// fail-closed 全拒 + base64url 编码。安全判据放 core 真跑（见模块注释）
pub mod mcp_auth;
// v0.20（B1）MCP 授权范围判定内核：McpScope + 组件级分组匹配 + 优先级 +
// fail-closed deny_all。安全判据放 core 真跑；config.rs 只做 IO 与收口
pub mod mcp_scope;
// v0.20（SSH P0-1）OpenSSH client config 解析：~/.ssh/config → 导入候选
// （first-match-wins / 通配参数组 / 逐块容错，见模块注释的诚实边界）
pub mod ssh_config;

// 便捷再导出：调用方写 `myshelltool_core::redact_command(...)`（日志落盘点最常用）
pub use redact::redact_command;
// v2.8 端口覆盖三态解析（http_server.rs 的起始端口解析用它）
pub use port_env::{parse_port, PortOverride};
// v2.7 外部响应正文的脱敏摘录（回显/落日志前遮蔽调用方声明的秘密字面量）
pub use redact::redact_excerpt;
// v2.8 输出文本脱敏（env 赋值 / mysql IDENTIFIED BY 形态，执行日志 outputSummary 用）
pub use redact::redact_output;
// v2.8 lossy 远端路径守卫（按名寻址失真文件名的 fail-closed 判定 + 统一拒绝文案）
pub use remote_text::{is_lossy_remote_path, lossy_remote_path_error};
// v1.3 Gist 同步引擎（载荷结构 + 加解密封装 + 冲突检测，纯逻辑无 HTTP）
pub mod sync;

// v2.8 第四轮按域拆出（lib 根仅保留模块编排与再导出，1084→40 行）：
// 资产域（ConnectionAsset/存储/校验/分组/原子写盘 write_atomic）
pub mod asset_store;
// 凭据域（SecretStore/编解码器/凭据状态与引用结构）
pub mod secret_store;
// glob 再导出保持既有调用路径（myshelltool_core::ConnectionAsset / ::write_atomic 等不变）
pub use asset_store::*;
pub use secret_store::*;

// 原 lib.rs 的测试模块（经上方 glob 再导出解析两域符号）
#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

