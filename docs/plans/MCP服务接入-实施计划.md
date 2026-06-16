---
title: myshelltool MCP 服务接入 — 实施计划
spec: docs/specs/MCP服务接入-需求规格.md
interview: docs/interviews/MCP服务接入-访谈记录-20260616.md
decisions: D1-D9 全部按推荐锁定
status: ready for execution
created_at: 2026-06-16
---

# myshelltool MCP 服务接入 — 实施计划

> 凝练了 9 轮 Socratic 访谈 + D1-D9 选型决策（全部按推荐锁定）+ brownfield 基线核查编写而成。
> 任务拆解维度：**按架构层**（访谈末尾由用户选定）。
> 每层独立可验证，从底层往上构建，最后集成。

---

## 0. 已锁定决策（D1-D9，不可漂移）

| 决策 | 锁定值 |
|------|--------|
| D1 二进制形态 | **双二进制**：`myshelltool.exe`(GUI) + `myshelltool-mcp.exe`(console) |
| D2 会话共享 | **两阶段**：v1.0 MCP 进程独立建会话，v1.1 补 named pipe 桥接 |
| D3 MCP SDK | **官方 rmcp ~1.7**（MCP 协议 2025-03-26 基线）|
| D4 host key | **仅服务已在 GUI 信任过的资产** |
| D5 危险命令检测 | **翻译成 Rust `dangerous_commands.rs`，GUI+MCP 共享** |
| D6 Tools | **12 个**（ssh_exec 默认拒，只读快捷工具默认放）|
| D7 Resources | **3 静态 + 会话日志 template** |
| D8 Prompts | **3 个诊断类**（diagnose_server / audit_security / cleanup_disk）|
| D9 默认名单 | **白/黑/黄/默认拒**（fail-secure）|

---

## 1. 架构分层总览（按层拆解顺序）

```
Layer 0: 依赖与脚手架     ← Cargo.toml + bin 骨架（地基）
Layer 1: 共享核心         ← dangerous_commands.rs + 会话桥接（被上下两层复用）
Layer 2: MCP Server 骨架  ← rmcp stdio server + Tools 注册框架
Layer 3: Tools 实现       ← 12 个工具逐个实现（最大工作量层）
Layer 4: Resources 实现   ← 3 静态 + 1 template
Layer 5: Prompts 实现     ← 3 个诊断 prompt
Layer 6: 审批集成         ← 命令层检测 + GUI 三段式弹窗回路
Layer 7: 降级逻辑         ← GUI 未运行的弹性降级
Layer 8: 打包与配置文档   ← Claude Desktop / Cursor / Cline 配置示例
Layer 9: 验证与回归       ← AC1-AC7 验收 + 不破坏现有功能
```

**关键依赖关系**：Layer 0 → Layer 1 → (Layer 2, 6, 7 并行) → Layer 3/4/5 → Layer 8 → Layer 9。

---

## 2. Layer 0：依赖与脚手架（地基）

### 2.1 目标
建立 MCP bin 入口 + 补齐 tokio feature + 加 rmcp 依赖。

### 2.2 文件级改动

**改 `src-tauri/Cargo.toml`**：
```toml
# [dependencies] 新增（在现有 10 个 crate 后追加）：
rmcp = { version = "~1.7", features = ["server", "transport-io"] }
# MCP 协议 2025-03-26 基线；~1.7 允许 1.x 补丁更新，挡 breaking major

# 翻译 dangerousCommands.js 需要：
regex = "1"
# 注：第 11 条 chmod 用到 negative lookahead，regex crate 不支持
# 解决方案见 Layer 1（改写为枚举否定，不引入 fancy-regex 避免新依赖）

# tokio feature 补 "macros" 和 "time"（rmcp + 降级重试需要）：
tokio = { version = "1", features = ["sync", "parking_lot", "io-util", "rt-multi-thread", "net", "macros", "time", "process"] }
# 新增：macros（#[tokio::main]）、time（降级重试 sleep）、process（拉起 GUI）

# [dependencies] 新增 [[bin]] 段（文件末尾）：
[[bin]]
name = "myshelltool-mcp"
path = "src/bin/mcp.rs"
```

**新建 `src-tauri/src/bin/mcp.rs`**（console 子系统入口）：
```rust
// 关键：console 子系统（不加 windows_subsystem = "windows"）
// 让 release 构建的 stdin/stdout 可用，Claude Desktop 才能读 JSON-RPC
#[tokio::main]
async fn main() {
    // 初始化 MCP 专用 logger（输出到 stderr + 文件，绝不污染 stdout）
    myshelltool_lib::init_mcp_logger();
    // 进入 MCP stdio server 阻塞循环
    if let Err(e) = myshelltool_lib::run_mcp_stdio().await {
        eprintln!("MCP server error: {}", e);
        std::process::exit(1);
    }
}
```

**改 `src-tauri/src/lib.rs`**（导出 MCP 入口函数）：
- 新增 `pub mod mcp;`（模块声明，L4 附近）
- 新增 `pub mod dangerous_commands;`（模块声明）
- 新增 `pub async fn run_mcp_stdio() -> Result<(), String>`（在 `run()` 之后）
- 新增 `pub fn init_mcp_logger()`（独立 logger 初始化，不依赖 Tauri AppHandle）

### 2.3 验证（Layer 0 完成标志）
```bash
cd src-tauri && cargo check
# 预期：编译通过（rmcp 依赖拉取 + bin target 注册成功）
# 注意：Windows 上若 build script 阻断，用 cargo check 兜底（AGENTS.md §9）

cargo build --bin myshelltool-mcp
# 预期：生成 src-tauri/target/debug/myshelltool-mcp.exe
```

### 2.4 风险
- rmcp 1.7 可能依赖 tokio 1.x 的特定 feature，若编译报缺 feature → 按错误信息补
- `[[bin]]` 注册后，`cargo build` 默认会构建所有 bin，确认 GUI bin 仍正常

---

## 3. Layer 1：共享核心（被上下两层复用）

### 3.1 目标
建立 GUI 与 MCP 共享的两块基石：危险命令检测 + 会话访问桥接。

### 3.2 子任务 L1-A：`dangerous_commands.rs`（D5）

**新建 `src-tauri/src/dangerous_commands.rs`**：

翻译 `src/lib/dangerousCommands.js` 的 16 条正则为 Rust。**关键改写**：

| JS 原正则 | Rust 翻译策略 |
|----------|--------------|
| 1. `rm\s+(-[a-zA-Z]*r[a-zA-Z]*f\|--recursive\b.*--force\b)/i` | 直接翻译 + `(?i)` 内联 |
| 2-3. mkfs / dd of=/dev/ | 直接翻译 |
| 4. fork bomb 变体 1 `:\s*\(\)\s*\{[^}]*:\|:\s*&\s*\}\s*;` | 直接翻译 |
| 5. `>\s*\/dev\/sd[a-z]/i` | 直接翻译 |
| 6-10. shutdown/reboot/halt/poweroff/init 0 | 直接翻译 |
| **11. `chmod\s+-R\s+[0-7]{3,4}\s+\/(?!tmp\|var\/tmp\|home\|Users)/i`** | **改写**：先匹配 `chmod -R [0-7]{3,4} /\S+`，捕获目标路径，在代码里排除 tmp/var/tmp/home/Users 前缀（regex crate 不支持 lookahead）|
| 12. chown -R | 直接翻译 |
| 13. iptables -F | 直接翻译 |
| 14. fork bomb 变体 2 `:()\{\s*:\|:&\s*\};:` | 直接翻译（注意 JS 的 `:()` 字面量）|
| 15-16. curl/wget pipe bash/sh/zsh | 直接翻译 |

**API 设计**（对齐 JS `detectDangerousCommand`）：
```rust
pub struct DangerousMatch {
    pub pattern: String,   // 命中的正则源码（对应 JS pattern.source）
    pub sample: String,    // 命中文本前 80 字符（对应 JS text.slice(0,80)）
}

pub fn detect_dangerous_command(text: &str) -> Option<DangerousMatch>;

// 三层分类（对应 D9 决策）
pub enum CommandRisk {
    Safe,           // 命中白名单
    Allowed,        // 命中黄名单（用户配置）
    Dangerous(DangerousMatch),  // 命中黑名单
    Unknown,        // 不在任何名单 → 默认拒
}

pub fn classify_command(
    text: &str,
    whitelist: &[String],     // 内置白名单（D9）
    yellow_list: &[String],   // 用户资产级黄名单（asset 字段，v2 数据模型）
) -> CommandRisk;
```

**正则初始化**：用 `once_cell::sync::Lazy` 或 `std::sync::OnceLock`（Rust 1.70+ 标准库）缓存编译后的 `Vec<Regex>`，避免每次调用重编译。

**新增依赖**：`once_cell = "1"`（或确认 Rust 版本 ≥ 1.70 用 `OnceLock`，免依赖）。

### 3.3 子任务 L1-B：会话访问桥接（D2 v1.0 = 独立会话）

**v1.0 策略**（D2-B）：MCP 进程独立建会话，不复用 GUI。

**问题**：`SshSessionManager` 字段全私有，构造需要 `AppHandle`（用于 emit host key/keyboard 事件给 GUI）。MCP 进程无 AppHandle。

**解决方案**：在 `ssh.rs` 新增一个 **headless 构造路径**：
```rust
impl SshSessionManager {
    // 现有：pub fn new(app: AppHandle, ...) → GUI 用
    // 新增：pub fn new_headless(known_hosts_path: PathBuf, secret_store_dir: PathBuf) -> Self
    //   → 不持有 AppHandle，host key/keyboard 交互改为"直接拒绝未信任资产"（D4）
}
```

**D4 落地**：headless manager 在 `ssh_connect` 流程里，遇到 known_hosts 未知主机 → 直接返回 `Err("asset not pre-trusted; please connect via GUI first")`，不触发 emit 事件。

**新增 pub accessor**（MCP 需要读会话列表）：
```rust
impl SshSessionManager {
    pub fn list_session_ids(&self) -> Vec<String>;  // MCP list_sessions 工具用
    pub fn has_session(&self, session_id: &str) -> bool;  // MCP 工具参数校验用
}
```

### 3.4 验证（Layer 1 完成标志）
```bash
cd src-tauri && cargo test dangerous_commands
# 新增单元测试：16 条正则各测 1 个命中样本 + 1 个未命中样本
# 第 11 条 chmod 测试：chmod -R 777 /tmp（安全）vs chmod -R 777 /etc（危险）

cargo test ssh_new_headless
# 测试 headless manager 构造 + 未信任资产拒绝逻辑
```

### 3.5 风险
- `AppHandle` 在 SshSessionManager 里是必需字段（emit 事件）→ headless 版要么用 `Option<AppHandle>`，要么把 emit 抽成 trait 注入。**推荐 `Option<AppHandle>`**（改动最小，emit 前判 None）。

---

## 4. Layer 2：MCP Server 骨架

### 4.1 目标
建立 rmcp stdio server 主循环 + Tools/Resources/Prompts 注册框架（空实现）。

### 4.2 文件级改动

**新建 `src-tauri/src/mcp/mod.rs`**（MCP 模块入口）：
```rust
pub mod server;       // rmcp server 主循环
pub mod tools;        // 12 个 tool 实现
pub mod resources;    // 3 静态 + 1 template
pub mod prompts;      // 3 个诊断 prompt
pub mod approval;     // 审批回路（Layer 6）

pub async fn run_mcp_stdio() -> Result<(), String>;  // lib.rs 调用入口
```

**新建 `src-tauri/src/mcp/server.rs`**：
```rust
use rmcp::{ServiceExt, model::ServerInfo, transport::TokiostdioTransport};

pub async fn run_mcp_stdio() -> Result<(), String> {
    // 1. 加载资产库 + 凭据（复用 myshelltool_core）
    let asset_store = load_assets_for_mcp()?;
    // 2. 构造 headless SshSessionManager（L1-B）
    let ssh_mgr = Arc::new(AsyncMutex::new(SshSessionManager::new_headless(...)));
    // 3. 构造 MCP server handler（实现 rmcp ServerHandler trait）
    let handler = MyshellToolMcpServer::new(ssh_mgr, asset_store);
    // 4. 启动 stdio server（TokiostdioTransport 读 stdin 写 stdout）
    let service = handler.serve(TokiostdioTransport::stdio())
        .await
        .map_err(|e| format!("MCP serve error: {}", e))?;
    service.wait().await.map_err(|e| format!("{}", e))?;
    Ok(())
}
```

**rmcp handler 骨架**（`MyshellToolMcpServer` 实现 `ServerHandler` trait）：
- `get_info()` → 返回 ServerInfo（name="myshelltool", version, capabilities 声明三原语支持）
- `list_tools()` → 返回 Layer 3 的 12 个 tool schema
- `call_tool()` → 分发到 Layer 3 各 tool 实现
- `list_resources()` → 返回 Layer 4 的 3 静态资源
- `read_resource()` → 分发到 Layer 4
- `list_resource_templates()` → 返回会话日志 template
- `list_prompts()` → 返回 Layer 5 的 3 个 prompt
- `get_prompt()` → 分发到 Layer 5

### 4.3 验证（Layer 2 完成标志）
```bash
# 手动测试 stdio 通信（用 echo 模拟 Claude Desktop）
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{...}}' | myshelltool-mcp.exe
# 预期：stdout 返回 initialize 响应，含 serverInfo + capabilities
# stderr 有日志，stdout 无杂质

# 列工具（空实现阶段）
echo '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | myshelltool-mcp.exe
# 预期：返回空 tools 列表（Layer 3 未实现前）
```

### 4.4 风险
- rmcp API 在 1.x 可能有 breaking change → 锁 `~1.7`，遇到 API 不符查 docs.rs/rmcp/1.7 对应版本文档
- `ServerHandler` trait 方法签名需对照 rmcp 1.7 实际定义（核查时点的 API）

---

## 5. Layer 3：Tools 实现（12 个，最大工作量层）

### 5.1 目标
实现 D6 锁定的 12 个 MCP Tools。

### 5.2 Tools 清单与映射

| Tool 名 | inputSchema 关键字段 | 后端调用 | 危险等级 | 审批 |
|---------|---------------------|---------|---------|------|
| `list_assets` | `{}` | `myshelltool_core::load_connection_asset_store` | 只读 | 自动 |
| `list_sessions` | `{}` | `ssh_mgr.list_session_ids()` | 只读 | 自动 |
| `system_status` | `{session_id}` | `ssh_write("uptime; free -h; top -bn1\|head -20")` | 只读（预定义）| 自动 |
| `disk_usage` | `{session_id, path?}` | `ssh_write("df -h {path:-/}")` | 只读 | 自动 |
| `service_status` | `{session_id, service}` | `ssh_write("systemctl status {service}")` | 只读 | 自动 |
| `ssh_exec` | `{session_id, command, intent}` | `ssh_write(command)` + **L6 审批** | **高** | 三段式弹窗 |
| `sftp_list` | `{session_id, path}` | `sftp_list_dir` | 只读 | 自动 |
| `sftp_read` | `{session_id, path}` | `sftp_read_file` | 只读 | 自动 |
| `sftp_upload` | `{session_id, local_path, remote_path, intent}` | `sftp_upload_start/chunk/finalize` + **L6** | **高** | 弹窗 |
| `sftp_remove` | `{session_id, path, intent}` | `sftp_remove` + **L6** | **高** | 弹窗 |
| `resource_monitor_snapshot` | `{session_id}` | `resource_monitor_snapshot` | 只读 | 自动 |
| `tunnel_start` | `{session_id, config, intent}` | `tunnel_create + tunnel_start` + **L6** | **高** | 弹窗 |
| `tunnel_stop` | `{tunnel_id, intent}` | `tunnel_stop` + **L6** | **高** | 弹窗 |

> 注：实际是 13 个（tunnel_start 和 tunnel_stop 分开），spec §10 D6 表里合并写了，实施时拆开。

### 5.3 实现模式

每个 tool 遵循统一模式：
```rust
async fn tool_xxx(params: Value, ctx: &McpCtx) -> Result<CallToolResult, String> {
    // 1. 参数反序列化 + 校验（session_id 存在性等）
    // 2. 危险等级判定（L1-A classify_command）
    // 3. 若 Dangerous/Unknown → 走 L6 审批回路
    // 4. 调用后端（复用 ssh.rs 的 pub async fn，或新增 accessor）
    // 5. 结果包装为 CallToolResult { content: [TextContent], is_error: false }
}
```

**`McpCtx` 共享上下文**：
```rust
struct McpCtx {
    ssh_mgr: Arc<AsyncMutex<SshSessionManager>>,
    asset_store: Arc<RwLock<ConnectionAssetStore>>,
    secret_store: SecretStore,
    approval_channel: ApprovalChannel,  // L6 审批回路
}
```

### 5.4 复用 vs 新增 accessor

**直接复用的 ssh.rs command 函数**（这些是 `pub async fn`，参数含 `State`）：
- 问题：command 函数签名第一个参数是 `State<'_, AppState>`，MCP 没有 Tauri State。

**解决方案**：把 command 函数的**核心逻辑**抽成纯函数（接收 `&SshSessionManager` 而非 `State`），command 函数只做 State 解包 + 调用纯函数。MCP 直接调纯函数。

```rust
// ssh.rs 重构模式（以 ssh_write 为例）：
#[tauri::command]
pub async fn ssh_write(state: State<'_, AppState>, session_id: String, data: Vec<u8>) -> Result<(), String> {
    let mgr = state.ssh_sessions.lock().await;
    ssh_write_impl(&mgr, &session_id, &data).await
}

// 新增纯函数（MCP 复用）：
pub async fn ssh_write_impl(mgr: &SshSessionManager, session_id: &str, data: &[u8]) -> Result<(), String> {
    // 原有逻辑搬到这里
}
```

**这是 v1.0 的工作量主体**——为 12 个 tool 涉及的 command 函数逐个抽取 impl 纯函数。这是 ADR v3 Follow-up 提到的「ssh.rs 解耦」的一部分，MCP 是业务驱动力。

### 5.5 验证（Layer 3 完成标志）
```bash
# 列工具
echo tools/list | myshelltool-mcp.exe → 返回 13 个 tool schema
# 调用只读 tool（先在 GUI 连一个 session，MCP v1.0 独立建连则需先 ssh_connect）
echo tools/call {list_assets} → 返回资产 JSON
echo tools/call {disk_usage, session_id} → 返回 df 输出
# 高危 tool（ssh_exec rm -rf）→ 触发 L6 审批（v1.0 简化：直接拒绝或本地 stdin 询问）
```

---

## 6. Layer 4：Resources 实现

### 6.1 目标
实现 D7 的 3 静态资源 + 1 resource template。

### 6.2 文件级改动

**新建 `src-tauri/src/mcp/resources.rs`**：

| Resource URI | 数据来源 | MIME |
|--------------|---------|------|
| `myshelltool://assets` | `load_connection_asset_store`（脱敏：去除 credential_id）| application/json |
| `myshelltool://sessions` | `ssh_mgr.list_session_ids()` + 元数据 | application/json |
| `myshelltool://known-hosts` | 读 `known_hosts.json`（仅主机名+指纹，无私钥）| application/json |
| `myshelltool://sessions/{id}/log` (template) | 读会话最近 N 行输出（需 ssh.rs 加 accessor）| text/plain |

**会话日志 accessor**（ssh.rs 新增）：
```rust
impl SshSessionManager {
    pub async fn get_session_log(&self, session_id: &str, tail_lines: usize) -> Option<String>;
}
```
> 注：当前 SshSession 是否缓存输出缓冲？核查未确认。若无，v1.0 此 template 返回「日志不可用，请用 ssh_exec 跑 tail」，v1.1 再补缓冲。

### 6.3 验证
```bash
echo resources/list → 返回 3 静态 + 1 template
echo resources/read {myshelltool://assets} → 返回资产 JSON
echo resources/read {myshelltool://sessions/abc/log} → 返回日志或"不可用"提示
```

---

## 7. Layer 5：Prompts 实现

### 7.1 目标
实现 D8 的 3 个诊断 prompt。

### 7.2 文件级改动

**新建 `src-tauri/src/mcp/prompts.rs`**：

```rust
// diagnose_server prompt：动态返回 messages
pub async fn get_diagnose_server(session_id: String, ctx: &McpCtx) -> Vec<PromptMessage> {
    vec![
        PromptMessage::user(format!(
            "请诊断服务器会话 {}。依次执行：\n\
             1. 调用 disk_usage 查看 disk\n\
             2. 调用 system_status 查看 CPU/内存/负载\n\
             3. 调用 service_status 查看 nginx/mysql 状态\n\
             4. 汇总结论，指出异常项",
            session_id
        ))
    ]
}
```

**Prompts 是 server 端 workflow**（非静态模板），Rust 里动态拼 messages[]。每个 prompt 的 arguments 在 `list_prompts` 里声明，`get_prompt` 接收具体值后返回组装好的 messages。

### 7.3 验证
```bash
echo prompts/list → 返回 3 个 prompt（含 arguments 定义）
echo prompts/get {diagnose_server, session_id=abc} → 返回 messages 数组
```

---

## 7. Layer 6：审批集成（命令层检测 + 三段式弹窗回路）

> 注：原文档章节号如此（spec 草稿时 Layer 4/5/6 序号有重叠），实施时按内容理解：本节是审批集成层。

### 6.1 目标
落地 D5+D9 的三层审批 + 三段式弹窗。

### 6.2 架构（v1.0 简化版 vs v1.1 完整版）

**v1.0（D2-B 独立会话下的审批）**：
- MCP 进程内做命令层检测（`classify_command`）
- 命中黑名单/未知 → **MCP 进程内直接拒绝**，返回 `isError: true` + 三段信息（AI 意图 + 命令 + 后果预测）作为 error content
- **不弹 GUI 弹窗**（因 v1.0 MCP 是独立进程，跨进程弹窗依赖 v1.1 的 named pipe）

**v1.1（D2-A named pipe 桥接后的完整审批）**：
- MCP 进程检测到高危 → 经 named pipe 发审批请求到 GUI 进程
- GUI 进程触发 `GlobalModals.vue` 的 `mcpApproval` 分支
- 用户点确认/拒绝 → 决议经 pipe 回传 MCP → 执行或拒绝

### 6.3 文件级改动（v1.0）

**新建 `src-tauri/src/mcp/approval.rs`**：
```rust
pub enum ApprovalDecision {
    AutoExecute,           // 白名单/黄名单
    RejectWithReason(String),  // 黑名单/未知，v1.0 直接拒
    // v1.1 加：RequestHumanApproval(ApprovalRequest)  // 走 pipe 问 GUI
}

pub async fn evaluate(
    command: &str,
    intent: &str,          // AI 声明的意图
    ctx: &McpCtx,
) -> ApprovalDecision {
    match classify_command(command, &WHITELIST, &ctx.yellow_list()) {
        CommandRisk::Safe | CommandRisk::Allowed => ApprovalDecision::AutoExecute,
        CommandRisk::Dangerous(m) => {
            let reason = format_three_section(intent, command, &predict_consequence(&m));
            ApprovalDecision::RejectWithReason(reason)
        }
        CommandRisk::Unknown => {
            let reason = format_three_section(intent, command, "命令不在已知安全名单，默认拒绝");
            ApprovalDecision::RejectWithReason(reason)
        }
    }
}

fn format_three_section(intent: &str, command: &str, consequence: &str) -> String {
    format!(
        "【AI 意图】{}\n【真实命令】{}\n【后果预测】{}",
        intent, command, consequence
    )
}
```

**后果预测**（`predict_consequence`）：基于命中模式给固定提示：
- rm -rf → "将递归删除文件/目录，不可恢复"
- mkfs → "将格式化文件系统，数据全毁"
- dd of=/dev/ → "将直接写入块设备，可能破坏磁盘数据"
- ... 16 条对应 16 条后果文案

### 6.4 文件级改动（v1.1，named pipe 完整版）

**改 `src-tauri/src/lib.rs` setup hook**：
```rust
.setup(|app| {
    // 现有 AppState 初始化...
    // 新增：启动 named pipe server（持有 ssh_mgr 的 Arc clone）
    let ssh_mgr_clone = Arc::clone(&ssh_mgr);
    let app_handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        crate::mcp::pipe::run_pipe_server(ssh_mgr_clone, app_handle).await;
    });
    Ok(())
})
```

**新建 `src-tauri/src/mcp/pipe.rs`**：
- `run_pipe_server`：用 `tokio::net::windows::named_pipe` 监听 `\\.\pipe\myshelltool-mcp`
- 收到审批请求 → 通过 `app_handle.emit("mcp-approval-request", payload)` 发给前端
- 前端 GlobalModals 弹 `mcpApproval` → 用户决议 → 前端调 Tauri command `resolve_mcp_approval(request_id, approved)` → pipe server 收到 → 回传 MCP 进程

**改 `src/components/shell/GlobalModals.vue`**：
- `modalTitle` 新增 case：`case 'mcpApproval': return 'MCP 操作审批';`
- template 新增 `v-else-if="modal.type === 'mcpApproval'"` 分支：渲染三段式（AI 意图 / 真实命令 / 后果预测）
- `submitModal` 新增 case：`case 'mcpApproval': store.resolveMcpApproval(modal.value.requestId, true); return;`
- 新增 deny 按钮（复用 `hostKeyVerify` 的 danger 按钮模式）

**改 `src/stores/ui.js` + `workbench.js`**：
- `ui.js` 新增 `resolveMcpApproval` action（调 Tauri command）
- `workbench.js` return 块 re-export `resolveMcpApproval`（**避免重蹈 `executeTerminalSearch` 漏导出 bug**）

**新增 Tauri command**（lib.rs）：
- `resolve_mcp_approval(request_id: String, approved: bool)` → 注册到 `generate_handler!`

### 6.5 验证
```bash
# v1.0：MCP 进程内拒绝
echo tools/call {ssh_exec, command="rm -rf /tmp/x", intent="清理临时文件"} 
→ 返回 isError:true + 三段式 error content

# v1.1：GUI 弹窗
# 先启动 GUI，再让 MCP 连 pipe
# AI 调高危命令 → GUI 弹 mcpApproval → 三段式显示 → 用户操作
```

---

## 9. Layer 7：降级逻辑（GUI 未运行的弹性降级）

### 7.1 目标
落地 Round 9 的弹性降级契约。

### 7.2 文件级改动

**改 `src-tauri/src/mcp/server.rs`**（run_mcp_stdio 开头）：
```rust
pub async fn run_mcp_stdio() -> Result<(), String> {
    // 检测 GUI 进程是否运行（尝试连 named pipe）
    let gui_online = check_pipe_available().await;
    
    if !gui_online {
        // 尝试拉起 GUI（最多 3 次，递增间隔 1s/2s/5s）
        for (attempt, delay) in [1u64, 2, 5].iter().enumerate() {
            spawn_gui_process()?;  // std::process::Command::new("myshelltool.exe").spawn()
            tokio::time::sleep(Duration::from_secs(*delay)).await;
            if check_pipe_available().await { break; }
            if attempt == 2 {
                // 仍失败 → 进入只读降级模式
                log::warn!("GUI not available, MCP entering read-only degraded mode");
                return run_mcp_stdio_readonly().await;
            }
        }
    }
    // 正常模式（pipe 已通或 v1.0 独立会话）
    run_mcp_stdio_full().await
}
```

**只读降级模式**（`run_mcp_stdio_readonly`）：
- `list_tools()` 只返回只读工具（list_assets / list_sessions / sftp_list / sftp_read / resource_monitor_snapshot）
- 移除所有写工具（ssh_exec / sftp_upload / sftp_remove / tunnel_*）
- `get_info()` 在 serverInfo 注明 `degraded: true`
- 写工具被调用 → 返回 `isError: true, "SSH sessions unavailable, MCP in read-only degraded mode"`

### 7.3 验证
```bash
# 关闭 GUI
# MCP 启动 → 尝试拉起 GUI 3 次 → 仍失败 → 只读模式
echo tools/list → 只返回只读工具
echo tools/call {ssh_exec} → isError + 降级提示
```

---

## 10. Layer 8：打包与配置文档

### 8.1 目标
提供 Claude Desktop / Cursor / Cline 配置示例 + 打包两个 exe。

### 8.2 文件级改动

**改 `src-tauri/tauri.conf.json`**（bundle 配置）：
- 确认 NSIS 打包包含两个 exe（`myshelltool.exe` + `myshelltool-mcp.exe`）
- Tauri 2 默认打包 `[[bin]]` 里所有 binary 到 resources，需确认配置

**新建 `docs/mcp-setup.md`**（用户文档）：
```markdown
# 在 Claude Desktop 配置 myshelltool MCP

编辑 `%APPDATA%\Claude\claude_desktop_config.json`：
{
  "mcpServers": {
    "myshelltool": {
      "command": "C:\\Users\\<你>\\AppData\\Local\\myshelltool\\myshelltool-mcp.exe",
      "args": []
    }
  }
}

重启 Claude Desktop → 工具列表出现 myshelltool 工具。

# 在 Cursor 配置
...（.cursor/mcp.json 或 Settings GUI）

# 在 Cline 配置
...（cline_mcp_settings.json）
注意：Cline 有 auto-approve 开关，开启后 MCP 工具不逐次询问，
myshelltool 自带三层审批门仍生效。
```

### 8.3 验证
```bash
npm run tauri:build
# 产物：release/myshelltool_0.1.0_x64-setup.exe
# 安装后检查安装目录两个 exe 都在
```

---

## 11. Layer 9：验证与回归（AC1-AC7 + 不破坏现有）

### 9.1 AC 验收清单

| AC | 验证方式 | 命令 |
|----|---------|------|
| AC1 Claude 配置发现工具 | 手动配置 + tools/list | `echo tools/list \| myshelltool-mcp.exe` |
| AC2 只读白名单自动执行 | AI 对话 + 不触发 modal | 手动 Claude 实测 |
| AC3 高危三段式弹窗 | 弹窗截图 + 双路径 | v1.1 GUI 弹窗 + 截图 |
| AC4 GUI 未运行降级 | 关 GUI + 只读验证 | 见 Layer 7 验证 |
| AC5 不破坏现有功能 | 四命令回归 | 见下 |
| AC6 stdout 纯净 | 重定向检查 | `myshelltool-mcp.exe > out.txt` 检查无杂质 |
| AC7 host key 安全门 | 未信任资产拒绝 | `tools/call ssh_exec 未信任资产` |

### 9.2 回归命令（AC5，必须全过）
```bash
npm run build                    # 前端编译
npm run test:core                # Rust core 单测（含新增 dangerous_commands 测试）
npm run test:ui                  # UI 冒烟 + host key（需先 npm run dev）
cd src-tauri && cargo check      # Rust 编译
cd src-tauri && cargo test       # 所有 Rust 测试
npm run tauri:build              # 完整打包
```

### 9.3 不破坏现有功能的重点检查
- `tauri::generate_handler!` 列表原有 43 个命令**一个不能少**（新增 MCP 命令追加，不替换）
- `GlobalModals.vue` 的测试选择器（`#modalLayer`/`#modalBody`/`#modalPrimary` 等）**不能改名**
- `dangerousCommands.js` 仍被前端使用（翻译的 Rust 版是 MCP 专用，前端暂不改，避免回归）

---

## 12. 实施顺序与里程碑

### Milestone 1：地基与共享核心（Layer 0 + 1）
- 改 Cargo.toml + 建 mcp.rs bin
- 翻译 dangerous_commands.rs + 单测
- ssh.rs headless 构造 + impl 抽取
- **验证**：cargo check + cargo test 过

### Milestone 2：MCP 骨架通 stdio（Layer 2）
- rmcp server 主循环
- initialize + tools/list（空）能响应
- **验证**：echo initialize → 收到 serverInfo

### Milestone 3：只读 Tools 打通（Layer 3 部分）
- 先做 list_assets / list_sessions / disk_usage / system_status / sftp_list / sftp_read / resource_monitor_snapshot（7 个只读）
- **验证**：Claude Desktop 实测能查服务器状态 ← **第一个可演示成果**

### Milestone 4：审批与高危 Tools（Layer 3 剩余 + Layer 6 v1.0）
- ssh_exec / sftp_upload / sftp_remove / tunnel_start / tunnel_stop（5 个高危 + v1.0 进程内拒绝）
- **验证**：高危命令返回 isError + 三段式

### Milestone 5：Resources + Prompts（Layer 4 + 5）
- 3 静态资源 + 1 template + 3 prompt
- **验证**：resources/list + prompts/get 响应

### Milestone 6：降级 + 打包（Layer 7 + 8）
- GUI 未运行降级
- tauri.conf.json 打包双 exe
- 配置文档
- **验证**：关 GUI 只读模式 + tauri:build 产物

### Milestone 7（v1.1）：named pipe 完整审批（Layer 6 v1.1）
- pipe server + GlobalModals mcpApproval 分支
- **验证**：GUI 弹窗三段式 + 决议回传

### Milestone 8：回归与 AC 验收（Layer 9）
- 全部 AC1-AC7
- 四命令回归
- **验证**：所有 AC PASS

---

## 13. 风险与缓解

| 风险 | 等级 | 缓解 |
|------|------|------|
| rmcp 1.7 API 与文档不符 | 🟡 中 | 锁 ~1.7，遇不符查 docs.rs 对应版本 |
| ssh.rs impl 抽取工作量大（12+ 函数）| 🟡 中 | 分 Milestone 渐进抽取，每个 tool 验证 |
| dangerousCommands 第 11 条 lookahead 改写 | 🟢 低 | 已有方案（枚举否定）|
| 残余提示词注入风险 | 🔴 高 | **无法技术消除**，靠三段式人工识破，文档显式留档 |
| named pipe ACL 越权 | 🟡 中 | v1.1 实现 pipe 时加 Windows SECURITY_ATTRIBUTES 限制当前用户 |
| executeTerminalSearch 漏导出 bug 重蹈 | 🟢 低 | 每个 store action 都在 workbench.js return 块显式 re-export + 检查 |

---

## 14. 残余风险显式继承（来自 spec §13）

以下风险必须在本计划实施中持续留档，不得静默吞掉：

1. **提示词注入**（🔴 致命）：AI 被远程内容诱导执行恶意命令，仅靠人工识破，无技术拦截。行业级未解难题。
2. **AI 语义逃逸**（🟠 高）：拆分命令绕过黑名单，靠默认拒兜底，仍有疲劳风险。
3. **named pipe 本地越权**（🟡 中）：同机其他进程连 pipe，靠 ACL 限制。
4. **MCP 协议 breaking change**（🟡 中）：锁 rmcp ~1.7，大版本升级需迁移。

---

## 15. v1.0 vs v1.1 边界（重要）

| 功能 | v1.0（D2-B 独立会话）| v1.1（D2-A named pipe）|
|------|---------------------|----------------------|
| 会话来源 | MCP 进程独立建连 | 复用 GUI 会话 |
| 审批 | 进程内拒绝（isError）| GUI 三段式弹窗 |
| 降级 | 不适用（本就独立）| GUI 未运行 → 只读 |
| host key | 仅服务已信任资产 | 同 |
| 完整度 | 简化版，功能可用但非访谈完整契约 | 兑现访谈完整契约 |

**文档必须显式标注 v1.0 是简化版**，避免误以为已完整兑现 spec。

---

## 16. 估时（粗略，单人）

| Milestone | 估时 |
|-----------|------|
| M1 地基+共享核心 | 2-3 天 |
| M2 MCP 骨架 | 1-2 天 |
| M3 只读 Tools | 2-3 天 |
| M4 审批+高危 Tools | 2-3 天 |
| M5 Resources+Prompts | 1-2 天 |
| M6 降级+打包 | 1-2 天 |
| **v1.0 小计** | **9-15 天** |
| M7 named pipe 完整审批（v1.1）| 3-5 天 |
| M8 回归验收 | 1-2 天 |
| **总计** | **13-22 天** |

---

## 17. 下一步

本计划书已就绪。实施时建议：
1. 从 Milestone 1 开始，每个 Milestone 完成后跑该层验证命令
2. Milestone 3（只读 Tools）是第一个可演示成果，建议优先达成给用户看进展
3. v1.0 完成后评估是否立即做 v1.1，还是先收集使用反馈
4. 所有改动遵循 AGENTS.md §4 约定（Vue3 setup / Pinia / SCSS token / Rust command 注册等）
