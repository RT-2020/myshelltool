# MCP 服务设计 v3

> **定位**：myshelltool 的 MCP 服务（给 AI 用的那一半身份）的完整设计。
> 上游文档：[MCP服务接入-需求规格.md](./MCP服务接入-需求规格.md)、[MCP服务接入-实施计划.md](../计划/MCP服务接入-实施计划.md)、[MCP文件传输-实施计划.md](../计划/MCP文件传输-实施计划.md)。
> 阅读提示：本文是**设计**（要做什么、为什么、怎么落地）；契约变化的同步义务仍以 [IPC契约与数据模型.md](../IPC契约与数据模型.md) 为准。

## 0. 设计目标与约束

**目标**：让 AI 成为运维的第二双手——**能力面尽量宽，但每一步可解释、可审计、可收回**。

| 约束（来自项目实况） | 对设计的影响 |
|---|---|
| 单人维护、公开仓库 | 一切改动必须"新增工具只改一处"；能被 CI 机械验证的优先级最高 |
| 仅 Windows 打包，多端只做编译验证 | MCP 服务端逻辑不得依赖 Windows 专有 API（core 已平台中立，新代码延续） |
| MCP 是核心差异化能力（README 自述） | 能力面扩展是主线；但扩展必须以"护栏先行"为序 |
| 用户目标：替换 FinalShell（多机运维） | 多主机 fan-out 与长任务是一等公民，不是附加项 |

**必须保持的不变量**（新设计不得破坏）：

1. 凭据不进日志 / 不进工具返回（[AGENTS.md](../../AGENTS.md) §8）。
2. 配置不可信 → 取**更严**的一档（`config.rs::McpConfig::strict()`），绝不静默降级为宽松。
3. 审批三级降级：elicitation → GUI 弹窗 → **fail-secure 拒绝**（`server.rs::degrade_to_pipe_or_reject`）。
4. 危险命令判定单点在 `myshelltool_core::dangerous_commands`（GUI 终端守卫与 MCP 审批同源）。
5. 毁灭性命令**两档恒硬拦**且不进审批链（`approval.rs` 的 catastrophic 前置顺序是安全前提）。

**设计原则**：

- **注册表优先**：新增一个工具 = 在表里加一行，而不是改 4 处 `match`（项目红线"能注册表就别 switch 链"）。
- **每个工具自我声明**：风险等级（给协议与审批用）+ 授权需求（给 scope 用）写在工具定义里，不散落在分发逻辑。
- **所有远程输出有界**：任何工具的返回都必须能放进 AI 的上下文，且**截断必须显式告知并可取回**。

---

## 1. 现状盘点（设计基线）

### 1.1 已有能力

| 面 | 现状 |
|---|---|
| Tools | **13 个**：只读 6（`list_assets` / `list_sessions` / `disk_usage` / `system_status` / `service_status` / `resource_monitor_snapshot`）+ 高危 1（`ssh_exec`）+ 文件 6（`sftp_list` / `sftp_read_file` / `sftp_write_file` / `sftp_upload` / `sftp_download` / `sftp_remove`） |
| Prompts | 3 个：`diagnose_server` / `audit_security` / `cleanup_disk`（均只收单个 `asset_id`） |
| Resources | 3 静态 + 1 模板：`myshelltool://assets` / `://sessions` / `://known-hosts` / `://sessions/{id}/log` |
| 拦截 | 2 档（`Minimal` 默认零审批 / `Strict` 非白名单一律确认）+ catastrophic 恒拦 |
| 审批 | elicitation（客户端原生框）→ GUI 弹窗（同进程，60s 超时）→ fail-secure 拒绝 |
| 审计 | `mcp-execution-log.json`：30 天 + 上限 1000 条，含 `minimal_allowed` 等 decision 区分，无凭据字段 |
| 传输 | 内嵌 axum + rmcp Streamable HTTP，`127.0.0.1` + 端口 41235(release)/41500(debug) 占用则 +1 重试 10 次 |

### 1.2 实测问题（本设计的靶子）

| # | 问题 | 证据 | 后果 |
|---|---|---|---|
| P1 | **工具注册分散在 4 处 `match`** | `tools.rs:256` `call_tool` 的 `match name`；`file_tools.rs::dispatch_file_tool` 二次分发；`server.rs:215` `check_approval_needed` 的 `match tool_name`；`server.rs:97` `log_scope_for` 的 `match tool_name` | 新增工具要改 4 处，漏一处即策略错配（审批降低或日志错归）——正是 AGENTS.md 禁止的"加功能改几处" |
| P2 | **协议层 annotations 全为 `None`** | `lib.rs:95-97` 注释自认；而 `McpToolInfo.tag`（readonly/dangerous）**已算出来只给 GUI 用** | 分类已存在却未发布到 MCP 协议：Claude Code / Cursor 无法据此自动放行只读工具、无法对破坏性工具加警示 |
| P3 | **无授权范围（scope）** | `list_assets` 返回全部资产；`exec_on_asset` 接受任意 `asset_id`；`McpConfig` 只有 `level` 一个字段 | 运维不会把生产库钥匙交给 AI，但当前只能"全给或全不给" |
| P4 | **SSH 会话不复用** | `tools.rs:146` `_session_cache: Arc<Mutex<()>>` —— 占位空类型；AGENTS.md §9 自述"会话复用未做" | `diagnose_server` prompt 要求连调 disk/status/service **4 次完整 SSH 握手**；高频调用易触发服务端 `MaxStartups` |
| P5 | **输出无分页，截断即丢失** | `ssh_exec` 描述"超长自动头尾截断"（原文不可取回）；`sftp_read_file` 超 1MB 直接失败 | 排查大日志（运维最常见场景）拿不到完整证据，AI 只能猜 |
| P6 | **资源面陈旧/死面** | `resources.rs:50` `://sessions` 仍答"v1.0 独立会话模式：无持久会话池"（实际 GUI 有会话池，`ssh.rs:181` `list_sessions_with_meta` 可查）；`://sessions/{id}/log` 模板恒答"暂不可用" | 向 AI 提供**错误的世界模型**；死模板违反死代码红线 |
| P7 | `resources` 的资产清单与 `list_assets` 工具**两处实现** | `resources.rs:64-92` 与 `tools.rs::tool_list_assets` 各序列化一遍 | 契约漂移风险（项目红线"同一概念多份实现"） |
| P8 | 文档漂移 | `tools.rs:151` 注释写"9 个"，实际 13 个 | 低危，但说明工具面变化缺单点事实源 |
| P9 | **headless 私钥认证只认文件路径** | `headless.rs:64-76` `HeadlessConnectParams` 无 `private_key_credential_id` 字段；GUI 侧私钥内容优先走 SecretStore（`session.rs:246-264`），headless 路径不行 | 「私钥内容托管在凭据库」的资产对 MCP/远程隧道**完全不可用**；scope（B1）与会话池（B3）做完后这批资产仍被排除，且 GUI 能连、AI 不能连的割裂会持续产生费解故障 |

---

## 2. 目标架构

```
┌──────────────────────────────────────────────────────────────────────┐
│ ① 接入层  axum + rmcp Streamable HTTP                                 │
│    127.0.0.1:<port>/mcp/<token>   ·  端口占用自动 +1                  │
│    输出：认证通过 / 401（不区分"路径错"与"token 错"，防探测）           │
└───────────────────────────────┬──────────────────────────────────────┘
                                ▼
┌──────────────────────────────────────────────────────────────────────┐
│ ② 授权层  authorize(asset) —— 唯一收口点                              │
│    scope（分组前缀/标签/黑名单/本机FS） × level（Minimal|Strict）      │
│    × policy（AlwaysAllow|ShellExec|RemoteWrite|ReadSensitive|…）      │
│    × catastrophic 硬拦（core，先行）                                  │
└───────────────────────────────┬──────────────────────────────────────┘
                                ▼
┌──────────────────────────────────────────────────────────────────────┐
│ ③ 能力层  TOOL_REGISTRY: &[ToolSpec]  —— 单一事实源                  │
│    name/schema/risk(→annotations)/policy/scope_req/handler            │
│    tools / prompts / resources 三面均从此表派生                        │
└───────────────────────────────┬──────────────────────────────────────┘
                                ▼
┌──────────────────────────────────────────────────────────────────────┐
│ ④ 执行层  SessionPool（GUI 会话 → 池 → 新建）                          │
│    有界输出（head/tail + ring buffer + cursor 分页）                   │
│    长任务 job（或协议 tasks）                                          │
└───────────────────────────────┬──────────────────────────────────────┘
                                ▼
┌──────────────────────────────────────────────────────────────────────┐
│ ⑤ 审计层  execution_log：tool/asset/risk/policy/scope/session/duration │
│    脱敏经 core::redact；无凭据字段（不变量 1）                          │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 3. 逐层设计

### 3.1 接入层：零损伤鉴权

**问题**：当前 `axum::Router::new().nest_service("/mcp", service)`（`http_server.rs:155`）无任何鉴权中间件；本机任意进程可调用全部工具（含 `ssh_exec`）。而"仅监听 127.0.0.1"在多进程桌面上不是安全边界。

**设计**：token **内嵌 URL**，做到对用户零损伤（不破坏"复制一个 URL 就能接"的开箱体验）。

| 项 | 设计 |
|---|---|
| token 生成 | 启动时 32 字节 CSPRNG（`rand` 已是依赖）→ base64url；**不落别的盘**，只写进已有的 `<data_dir>/mcp-endpoint.json`（新增 `authToken` 字段，旧字段不变） |
| 路由 | `/mcp/:token`（路径形式，兼容所有 host 的 URL 输入框）；中间件**同时**接受 `Authorization: Bearer <token>`（给支持自定义 header 的 host） |
| 失败语义 | 401 + `log::warn`（含来源 socket 地址）；**不区分**"路径不存在"与"token 错"，避免成为探测预言机 |
| 前端 | MCP 面板"复制配置"复制**含 token 的完整 URL**；面板另显示"重置 token"按钮（写盘 + 提示需重启 host 侧配置） |
| 自身探针 | `mcp/probe.rs` 的 initialize 握手**必须带 token**，否则 `probe.ok` 恒 false（状态灯全灭）——**这是本次改动最易漏的联动点** |
| 未来 LAN | 若将来允许非本机 host：**绝不默认 0.0.0.0**，必须显式开关 + 界面警告 + 强制 token；`tunnel.rs` 已有"未登记通道一律丢弃"的先例可循 |

**权衡的诚实说明**：URL 中的 token 可能进入 MCP host 自己的日志。对"本机 MCP host"威胁模型足够；跨机场景应改用 header。

### 3.2 授权层：scope（新增维度）

**问题**：`level` 回答的是"这个动作要不要确认"，没有回答"**这台机器允不允许碰**"。

**设计**：`McpConfig` 增加 `scope`，判定收口在**一个函数**。

```rust
// mcp/config.rs（扩展；serde default 保证旧 mcp-config.json 仍可读）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpConfig {
    pub level: McpInterceptLevel,
    #[serde(default)]
    pub scope: McpScope,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpScope {
    /// 允许访问的资产分组前缀；空 = 不按分组限制
    #[serde(default)] pub allowed_groups: Vec<String>,
    /// 允许访问的标签（命中任一即可）；空 = 不按标签限制
    #[serde(default)] pub allowed_tags: Vec<String>,
    /// 显式拒绝的资产 id（优先级最高）
    #[serde(default)] pub denied_asset_ids: Vec<String>,
    /// 是否允许 sftp_upload / sftp_download 触及本机文件系统
    #[serde(default)] pub allow_local_fs: bool,
    /// 配置不可信时置 true：拒绝一切资产访问（fail-closed 收敛态）
    #[serde(default)] pub deny_all: bool,
}
```

**判定语义（关键细节，直接对应项目红线"形态 E：靠形状猜测"）**：

- 分组匹配**必须按路径组件**比较，不能按字符串前缀：`allowed_groups = ["生产"]` **不得**匹配 `生产测试`。实现：按 `'/'` 切分后逐组件比对（`asset.group` 本就是 `/` 分隔多级路径）。
- 优先级：`deny_all` > `denied_asset_ids` > （`allowed_groups` 与 `allowed_tags` 的**并集**，两者都空 = 不限制）> 默认拒绝。
- 凭据边界不变：scope 只管"能不能连"，**不改变**凭据只走 `SecretStore` 的事实。

**收口点（只有 2 处，覆盖全部 13 个工具）**：

1. `tools.rs::exec_on_asset`（所有远程执行类工具的必经之路）
2. `mcp/sftp_ops.rs::connect`（所有文件类工具的必经之路）

**可见性同步**：`list_assets` 与 `myshelltool://assets` **只返回 scope 内的资产**，并在返回体里带 `"scopeApplied": true` 与 `"hiddenCount": N`——让 AI 知道"世界比它看到的大"，而不是以为资产只有这些（避免 AI 给出"你没有其他服务器"的错误结论）。

**配置损坏时的语义（决策点 D2）**：推荐 `scope.deny_all = true` + `level = Strict`，与现有 `unusable_config` 的 fail-closed 哲学一致；代价是 MCP 暂时完全不可用，因此**日志与面板提示必须极其明确**（"配置损坏已隔离到 X，请在 MCP 面板重新配置范围"）。

### 3.3 能力层：工具注册表（消灭 4 处 match）

**目标**：新增工具 = 表里加一行。

```rust
// mcp/registry.rs（新文件）
/// 风险等级：只用于**协议 annotations** 与 GUI 展示（人类与 host 的决策辅助）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskClass { ReadOnly, Write, Destructive }

/// 审批策略：**真正的运行时判定**，与现有 approval.rs 语义一一对应
pub enum Policy {
    AlwaysAllow,           // 纯本地查询（list_assets / list_sessions）
    ShellExec,             // catastrophic 硬拦 + 按 level（ssh_exec）
    RemoteWrite,           // 按 level（sftp_write_file / sftp_upload）
    RemoteReadSensitive,   // 敏感路径恒审批（sftp_read_file → file_policy::is_sensitive_remote_path）
    RemoteDelete,          // 根级/核心目录恒拒 + 按 level（sftp_remove）
    LocalPathWrite,        // is_protected_local_write_path 恒拒 + 按 level（sftp_download）
}

/// 该工具需要哪种授权
pub enum ScopeReq { None, Asset, AssetAndLocalFs }

pub struct ToolSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub schema: fn() -> Map<String, serde_json::Value>,
    pub risk: RiskClass,
    pub policy: Policy,
    pub scope_req: ScopeReq,
    pub handler: fn(&McpToolContext, &Map<String, serde_json::Value>)
        -> Pin<Box<dyn Future<Output = Result<CallToolResult, String>> + Send + '_>>,
}

pub fn registry() -> &'static [ToolSpec] { /* OnceLock 初始化，13 项逐一登记 */ }
```

**四个消费点全部改为读表**：

| 原位置 | 改后 |
|---|---|
| `tools.rs::list_all_tools` + `file_tools.rs::list_file_tools` | 遍历 `registry()` → `Tool::new(..).with_annotations(..).with_raw_output_schema(..)` |
| `tools.rs::call_tool` 的 `match name` + `dispatch_file_tool` | `registry().iter().find(|s| s.name == name)` → `(s.handler)(ctx, args)` |
| `server.rs::check_approval_needed` 的 `match tool_name` | `match spec.policy { .. }`（5 个分支，不再按工具名列举） |
| `server.rs::log_scope_for` 的 `match tool_name` | `spec.risk` / `spec.scope_req` |
| `lib.rs` 的 `McpToolInfo.tag`（GUI 用） | `spec.risk`（消除第三份分类） |

**⚠️ 迁移纪律**：这一重构必须**零行为变更**。特别是 `sftp_download` 的本机系统目录恒拒、`sftp_remove` 的根级恒拒、`sftp_read_file` 的敏感路径恒审批——这些是路径相关的**硬拒**（不是"按等级"），必须原样落到对应 `Policy` 分支，并用 core 级单测锁住（`cargo check --tests` 只能验编译，硬拒语义**必须放 core 真跑**——见 AGENTS.md §9 工具链坑）。

**annotations 落地**（已核实 rmcp 1.7 API 存在：`Tool::with_annotations`、`ToolAnnotations::read_only/destructive/idempotent/open_world`）：

| RiskClass | readOnlyHint | destructiveHint | idempotentHint | openWorldHint |
|---|---|---|---|---|
| ReadOnly | `true` | —（无意义） | `true` | `true` |
| Write | `false` | `false` | `false` | `true` |
| Destructive | `false` | `true` | `false` | `true` |

> 协议注释明确写着 annotations 是 **hints**、"clients should never make tool use decisions based on annotations from untrusted servers"——所以它**不能替代**服务端审批，只是让 host 的 UI 与自动放行策略有依据。这一点必须在代码注释里写明，防止后人误以为加了 annotations 就可以放松 `approval.rs`。

**工具面扩展清单**（按运维价值排序，**每项都必须声明 Policy**）：

| 优先级 | 工具 | 目的 | Policy | 为什么不能只用 `ssh_exec` |
|---|---|---|---|---|
| 1 | `exec_many` | 多主机 fan-out 执行（与 GUI 批量执行**共用同一编排层**） | ShellExec（逐目标判定，任一被拒则该目标跳过） | 并发、部分失败语义、按目标聚合输出 |
| 2 | `journal_query` | `journalctl`/`dmesg` 结构化查询（since/until/unit/grep/limit） | AlwaysAllow | 跨发行版命令差异固化在服务端，且结果结构化省 token |
| 3 | `port_listen` | 监听端口结构化（`ss` → `netstat` → `lsof` 降级口径已写在 `audit_security` prompt 里，应固化） | AlwaysAllow | 同一件事现在每次靠 AI 现场编命令，是在"猜环境"（红线 §7） |
| 4 | `process_list` | 进程列表结构化（cpu/mem 排序、过滤） | AlwaysAllow | 同上 |
| 5 | `file_search` | 按 size/mtime/name 查找（结果有上限） | AlwaysAllow | 结果集必须有界，否则炸上下文 |
| 6 | `service_control` | start/stop/restart | **RemoteWrite** | 必须比 `ssh_exec` 更明确地被拦（语义化风险） |
| 7 | `job_*` | 长任务（见 3.4） | 继承触发工具 | 超时命令必须可轮询 |
| 8 | `tunnel_list`/`tunnel_create` | 隧道管理（与 GUI 同源） | RemoteWrite | 复用 `ssh/tunnel.rs`，不另写 |

> **诚实提醒**：上表 2–5 项在能力上都是 `ssh_exec` 的子集，加它们的**唯一正当理由**是①结构化返回省 token、②把跨平台降级口径固化（不在运行时猜环境）、③声明式风险等级。**不要**为了"工具数量好看"而加——每多一个工具就多一份要与 `registry`/`approval`/`execution_log` 同步的契约。

### 3.4 执行层

#### 3.4.1 会话池（`_session_cache` 从占位变实体）

**复用优先级**：GUI 已连接会话 → 池中 headless 连接 → 新建。

- **GUI 会话**：`ssh.rs:164` `find_session_by_host(host, port, username)` **已存在**，直接可用——用户在界面上正开着的终端，AI 复用同一条连接，用户能实时看到 AI 在做什么（这对运维信任极重要）。
- **池**：`PoolKey { asset_id, auth_fingerprint }`；`auth_fingerprint` 取凭据版本/路径摘要，**凭据变更即失效旧条目**（防止改密码后复用旧连接产生难解释的认证失败）。默认 `max = 8`、`idle_ttl = 5 min`、LRU 淘汰 + 后台 reaper。
- **并发**：每次工具调用取 `in_flight` 守卫；同一连接上的并发 exec 要么串行化、要么各开一个 channel（推荐各开 channel + 上限）。
- **fan-out 上限**：`exec_many` 等多目标工具的并发建连数**与池容量对齐**（默认 8），超出部分排队而不是突破池上限——否则一次 20 台机器的 fan-out 会在池门口互相挤死，也会瞬间打满服务端 `MaxStartups`。这条写进 C2 的验收。

**必须处理的失效路径（这是本设计最容易出错的地方）**：
1. 用户手动断开该会话 → 池条目失效；
2. 服务器侧 `ClientAliveInterval` 掐断 → 复用前**探活**（轻量 exec 或 channel open），失败则重建并记 `log::warn`，**不静默重试**；
3. 资产被删除 → 主动失效并释放；
4. 凭据更新 → 按 `auth_fingerprint` 失效。

> 这四条的判定必须是**协议求证**（连接是否真活着），不能靠"上次用过所以应该还行"——同族问题见 AGENTS.md §7"就绪信号"。

#### 3.4.2 输出契约（上下文经济）

**问题**：AI 的上下文是最稀缺资源。`ssh_exec` 目前头尾截断后**原文不可取回**，`sftp_read_file` 超 1MB 直接失败。

**设计**：统一结构化返回（已核实 rmcp 1.7 支持 `Tool::with_raw_output_schema` + `CallToolResult::structured`）：

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutput {
    pub ok: bool,
    pub summary: String,                 // 一行结论，AI 先读这个
    pub exit_code: Option<i32>,
    pub data: Option<Value>,             // 结构化主体（各工具自定义）
    pub truncated: Option<Truncation>,   // 截断必须显式
    pub cursor: Option<String>,          // 可取回被截断的部分
    pub hint: Option<String>,            // 可操作的下一步（如"用 cursor 继续读"）
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Truncation {
    pub stream: &'static str,   // "stdout" | "stderr" | "content"
    pub kept_head_lines: usize,
    pub kept_tail_lines: usize,
    pub omitted_bytes: u64,
    pub note: String,           // 人话："已省略 152 KB 中间内容"
}
```

**有界策略**（默认值，写进各工具 schema 描述里让 AI 知道）：

| 工具 | 默认上限 | 超限行为 |
|---|---|---|
| `ssh_exec` | stdout 64 KiB（头 200 + 尾 100 行） | 截断 + `cursor`（ring buffer 保留最近 8 MiB/次调用） |
| `sftp_read_file` | 256 KiB（可传 `max_bytes`，上限 4 MiB） | 返回前 N 字节 + `cursor`（支持 `offset` 续读），**不再直接失败** |
| `exec_many` | 每目标 16 KiB，汇总表单独 | 超限目标标注 `truncated` |

**新增分页工具**：`read_output(cursor, offset?, limit?)` → 从 ring buffer 取回；cursor 是**服务端签发的不可猜字符串**（含 `job/调用 id + 偏移`，不暴露本机路径），过期或不存在返回明确错误而非空结果。

#### 3.4.3 长任务

**问题**：`apt upgrade` / `mysqldump` / `rsync` 会超过工具调用超时，AI 只能看到超时。

**两个可选机制**（rmcp 1.7 都已具备，已核实）：

| 方案 | 机制 | 优点 | 风险 |
|---|---|---|---|
| **A. 自建 job 工具**（推荐先做） | `ssh_exec_async` → 返回 `job_id`；`job_status` / `job_output(job_id, cursor)` / `job_cancel` | 任何 host 都能用，不依赖协议能力协商 | 自建状态机与生命周期（需 TTL 回收） |
| **B. 协议 tasks**（并行标注） | `Tool::with_execution(ToolExecution::new().with_task_support(TaskSupport::Optional))`，rmcp 的 `ServerHandler::enqueue_task` 处理 | 协议原生、host UI 可展现进度 | **host 侧支持面未验证**（Claude Code / Cursor 对 2025-11-25 tasks 的支持需实测） |

**建议**：A 先落地（稳定可用），同时在长任务工具上标 `TaskSupport::Optional`，让未来支持 tasks 的 host 自动走原生路径。job 状态机：`running → done|failed|cancelled`，内存态 + 有界 ring buffer，TTL 30 分钟，**不落盘**（与"活跃会话纯内存"的既有分层一致）。

### 3.5 审计层

`execution_log` 条目扩展（**不加任何凭据字段**，不变量 1）：

| 新字段 | 用途 |
|---|---|
| `risk` | 该工具声明的风险等级 |
| `policyDecision` | 命中哪个 Policy 分支、是否硬拒 |
| `scopeResult` | `allowed` / `denied_group` / `denied_tag` / `denied_id` / `deny_all` |
| `sessionSource` | `gui` / `pool` / `new`（回答"AI 是否复用了我的终端"） |
| `durationMs` | 执行耗时（性能回归与"AI 在干什么"的可解释性） |
| `truncated` | 是否发生截断（判断 AI 是否基于不完整证据下结论） |

**GUI 侧**：MCP 面板的执行日志列表增加 `scopeResult` 与 `sessionSource` 列；被 scope 拒绝的调用**也要记**（`denied` 是运维最想看的事件）。

### 3.6 资源与提示词

| 项 | 动作 |
|---|---|
| `myshelltool://sessions` | **改为真实会话清单**（复用 `ssh.rs:181` `list_sessions_with_meta`），删掉"v1.0 独立会话模式"陈旧文案（P6） |
| `myshelltool://sessions/{id}/log` | **删除该模板**（恒答"暂不可用"= 死面，违反红线）；需要会话日志时用 `ssh_exec`/`journal_query` |
| `resources` vs `list_assets` 双实现（P7） | 抽一个 `fn assets_view(scope, store) -> Value`，两处共用 |
| 新增 `myshelltool://mcp/scope` | 当前授权范围（AI 应知道自己的边界，可主动告知用户"我看不到 X 分组"） |
| 新增 `myshelltool://mcp/audit` | 最近 N 条执行摘要（让 AI 自查"我刚做了什么"） |
| Prompts | `asset_id` → `asset_ids`（数组）；`diagnose_server` 输出多机对比表；保留"命令失败必须如实报告，不得用无异常代替没查到"的既有纪律（`prompts.rs:77-78`，这条写得很好，别丢） |

---

## 4. 分期实施

> 每期的验收都要求：`cargo check` / `cargo check --tests` exit 0 零警告；受影响的安全判据测试放 **core 真跑**（AGENTS.md §9）；契约变化同步 `docs/IPC契约与数据模型.md`。

### 阶段 A：护栏（1 周）——不改变任何对外能力

| 任务 | 文件 | 验收标准 |
|---|---|---|
| A1 token 鉴权 | `mcp/http_server.rs`、`mcp/probe.rs`、前端 `stores/mcp.ts`、`stores/settings` 复制按钮 | 无 token → 401 且落 warn；带 token 的 host 握手成功且 `probe.ok = true`；面板复制出的 URL 可直接用 |
| A2 工具注册表迁移（**零行为变更**） | 新 `mcp/registry.rs`；改 `tools.rs`/`file_tools.rs`/`server.rs`/`lib.rs` | 13 个工具名称/schema 与迁移前逐字一致（快照比对）；`check_approval_needed` 对每个工具返回与旧 `match` 相同的结果（补充针对性测试）；4 处 `match` 收敛为 1 张表 |
| A3 annotations | 同上 | `tools/list` 响应中每个工具带 `annotations`；只读工具的 `readOnlyHint = true` |
| A4 审计扩展 | `mcp/execution_log.rs`、GUI 日志列表 | 新字段落盘且可展示；**断言日志中不含凭据字段**（含新增字段的回归测试） |

### 阶段 B：可用性（2–3 周）

| 任务 | 文件 | 验收标准 |
|---|---|---|
| B0（前置）headless 认证与 GUI 对齐 | `ssh/headless.rs`（`HeadlessConnectParams` 增加 `private_key_credential_id` / `passphrase_credential_id`，读取顺序与 `session.rs:246-264` 一致：SecretStore 优先、文件路径兜底） | 私钥内容仅托管在凭据库（无 `private_key_path`）的资产，经 `ssh_exec` 与 `sftp_*` 工具可连；凭据缺失时报错文案明确区分「凭据不存在 / 解密失败 / 私钥解析失败」三态（P9） |
| B1 scope 授权 | `mcp/config.rs`（新增 `McpScope`）、`mcp/scope.rs`（新，判定+组件级匹配）、`tools.rs::exec_on_asset`、`sftp_ops.rs::connect` | ① `allowed_groups=["生产"]` **不匹配** `生产测试`（组件级匹配单测）；② scope 外资产：`list_assets` 不返回、`ssh_exec` 明确拒绝并记 `denied`；③ 配置损坏 → `deny_all` + 明确提示 |
| B2 输出契约 | `mcp/output.rs`（新）、各工具 handler | 所有工具返回带 `outputSchema` 的结构化结果；`ssh_exec` 大输出截断时**必带** `cursor` 且 `read_output` 可取回原文；`sftp_read_file` 超限不再失败而是分页 |
| B3 会话池 | `mcp/session_pool.rs`（新，实体化 `_session_cache`）、`tools.rs::exec_on_asset`、`sftp_ops.rs::connect` | ① 连续调 4 个工具只建 1 次连接（日志可证）；② GUI 已有同主机会话时优先复用（`sessionSource=gui`）；③ 手动断开/凭据更新后下次调用重建且不静默失败 |
| B4 资源面修正 | `mcp/resources.rs` | `://sessions` 返回真实会话；死模板已删；资产视图与 `list_assets` 共用一个实现 |

### 阶段 C：能力面（1–2 个月）

| 任务 | 验收标准 |
|---|---|
| C1 长任务 job（方案 A）+ `TaskSupport::Optional` 标注 | 超时命令可轮询到最终结果与退出码；`job_cancel` 真中断（旗标 + 通道关闭）；job TTL 到期回收，无泄漏 |
| C2 `exec_many` | 批量编排层先在 **Rust 侧独立落地**（目标列表/逐目标 scope 与 Policy 判定/并发上限/结果聚合/取消语义），`exec_many` 做第一个消费者，GUI 批量面板后消费——**同一份实现，不写两遍**；fan-out 并发上限 = 池容量（默认 8），超出排队（见 3.4.1）；单目标失败不影响其余 |
| C3 只读工具扩展（`journal_query` / `port_listen` / `process_list` / `file_search`） | 每个都有结构化返回与有界输出；跨发行版降级口径有注释与测试；`readOnlyHint = true` |
| C4 `service_control` + 隧道工具 | Policy = RemoteWrite，Strict 档必经审批；Minimal 档记 `minimal_allowed` |
| C5 Prompts 多资产 | `asset_ids` 数组；`diagnose_server` 输出多机对比；保留"查不到 ≠ 无异常"纪律 |

---

## 5. 需要拍板的决策点

| # | 决策 | 我的推荐 | 理由 |
|---|---|---|---|
| D1 | token 形态 | **URL 路径内嵌 + 同时接受 Bearer header** | 保住"复制即用"的开箱体验；header 作为进阶通道 |
| D2 | 配置损坏时 scope 语义 | **`deny_all = true` + Strict**（与现有 fail-closed 一致） | 现有代码已确立"读不出来 ≠ 没配"的原则；代价是可用性，故提示必须极明确 |
| D3 | 长任务机制 | **先自建 job 工具，同时标 `TaskSupport::Optional`** | 不赌 host 对 tasks 的支持面；两条路可并存 |
| D4 | Strict 档审批记忆 | **不做** | "记住本次允许"一旦落盘就是 fail-open 面；运维高频确认的痛应靠 scope + 只读工具扩展缓解，而不是放松审批 |
| D5 | 首批扩展工具 | **`exec_many` + `journal_query` + `port_listen`** | 与"替换 FinalShell"的多机运维动机最贴；且后两个把跨平台口径从"运行时猜"变成"服务端固化" |

---

## 6. 明确不做（NOT-doing）

单人维护项目里，这份清单和上面的设计同等重要：

- **不做 MCP 工具的白名单模式**（用户级 per-tool 开关）：`level` + `policy` + `scope` 三个维度已足够表达，再加一层会让"当前到底允许什么"无法一句话说清。
- **不做远端 MCP host 的默认支持**（默认仍 `127.0.0.1`）：LAN 暴露是一个独立的安全议题，需要单独设计与评审。
- **不做 AI 自主的"多步计划执行"**（server 侧编排）：MCP 的职责是暴露能力与边界，编排属于 host（AI）侧；server 侧编排会把审批链变得不可解释。
- **不把 `ssh_exec` 拆成无数细粒度工具**：细粒度只在能带来"结构化返回 / 跨平台正确性 / 风险等级可声明"三者之一时才做。
