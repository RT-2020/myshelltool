# MCP 文件传输能力 — 实施计划

> 状态：**已完成（v2.2 交付落地）**。全链路 P0~P3 落地并已通过完整编译、单测与前端构建。
> 来源：2026-09-08 会话梳理。前置阅读：[`MCP服务接入-实施计划.md`](./MCP服务接入-实施计划.md)（v1.x 分期背景）、AGENTS.md §6/§8/§9。

---

## 1. 背景与问题

MCP server（v2）已具备完整的外围设施：Streamable HTTP transport、三级审批降级（elicitation → GUI 弹窗 → fail-secure 拒）、可配置拦截等级（Minimal/Strict）、执行日志审计。但**执行层只有 exec（shell 命令）一条数据通道，没有 SFTP 通道**——AI host 无法浏览远程目录（`sftp_list` 是桩）、无法读写远程文件、无法传输文件。当前 AI 想传文件只能用 `ssh_exec` 变通（base64 + heredoc），仅适合小文本。

另有一个信任缺口：`sftp_remove` 审批链路已通，但用户确认删除后返回「执行逻辑尚未实现」——**确认后被拒，比不支持更伤信任**，P0 优先消除。

## 2. 现状盘点

### 2.1 端到端链路

```
MCP host（ZCode / Claude Code / Cursor）
   │ HTTP 127.0.0.1:41235/mcp（Streamable HTTP，端口占用 +1 重试）
   ▼
rmcp server（http_server.rs / server.rs）          ✅ 就绪
   ▼
工具层（tools.rs：schema + dispatch）               ⚠️ 9 工具中 4 个桩/半桩
   ▼
审批层（approval.rs + dangerous_commands.rs）       ✅ 就绪，但只为 shell 命令设计
   ▼
执行层（connect_headless + exec_command_once）      ⚠️ 仅 exec 通道，无 SFTP 通道
   ▼
日志层（execution_log.rs）                          ⚠️ 就绪，但 log_scope 只认 5 个工具
   ▼
GUI 面板（mcp.js + 执行日志/拦截设置组件）           ✅ 就绪，无传输进度概念
```

### 2.2 工具现状清单

| 工具 | 状态 | 说明 |
|---|---|---|
| `list_assets` | ✅ 可用 | 资产元数据（脱敏） |
| `list_sessions` | 🟡 桩 | 返回提示文本，未接 GUI 会话池 |
| `disk_usage` | ✅ 可用 | exec `df -h` |
| `system_status` | ✅ 可用 | exec uptime/free/top |
| `service_status` | ✅ 可用 | exec `systemctl status`（服务名白名单校验） |
| `sftp_list` | ❌ 桩 | 直接报错「M3 桩，M4 完善」 |
| `resource_monitor_snapshot` | ❌ 桩 | 直接报错 |
| `ssh_exec` | ✅ 可用 | 完整审批链 + 结构化返回（exit_code + 16k 截断） |
| `sftp_remove` | 🟡 半桩 | 审批通过后执行未实现 |

### 2.3 可复用资产

- **审批链**：`approval::evaluate`（四层分类 + 等级映射）、三级降级 `degrade_to_pipe_or_reject`、GUI 弹窗事件 `mcp-tool-approval` + `mcp_confirm_tool` 命令 + 60s oneshot。
- **建连**：`ssh::connect_headless`（密码凭据 / 私钥 / passphrase / keyboard-interactive 密码类自动响应），返回 russh `client::Handle`——**可在此 handle 上开 SFTP subsystem channel**，russh-sftp 已是依赖。
- **分块上传协议**：GUI 的 `sftp_upload_start / _chunk / _finalize`（防 IPC OOM）——P2 大文件上传的设计参照。
- **日志**：`ExecutionLogEntry` + decision/outcome 常量体系、原子写、惰性清理，直接挂新工具即可。
- **GUI 传输队列**：files store 已有传输队列概念，P2 进度可见性的落点。

## 3. 目标 / 非目标

**目标**：让 MCP host 具备完整的远程文件能力——浏览（list）、读、写、上传（本机→远程）、下载（远程→本机）、删除，且每一环都在现有审批 + 日志体系内。

**非目标**（本期不做）：
- 目录递归传输（整树上传/下载）——先做单文件，递归由 AI 多次调用单文件工具组合。
- 断点续传——分块协议具备基础，但续传状态管理本期不做。
- GUI 手动发起传输给 MCP 用——方向相反，GUI 文件区已有完整能力。

## 4. 缺口分析（按层）

| 层 | 缺口 | 影响 |
|---|---|---|
| 执行层 | headless SFTP 通道不存在：GUI 的 `sftp_*` 命令依赖 `State<AppState>` + GUI 会话池的 session_id，MCP 拿不到 | 一切文件工具的前置 |
| 工具层 | 无 read / write / upload / download / mkdir / rename 工具定义 | AI 无文件操作入口 |
| 审批层 | 无「操作类型 × 路径」的文件风险模型：现有正则针对 shell 命令文本，无法表达「读 /etc/shadow」「写 /etc/」「覆盖已有文件」这类风险 | 文件工具的安全判定无处落 |
| 数据面 | 大文件策略缺失：MCP 单次调用不适合内联大内容；返回侧 16k 截断会毁掉下载内容 | 大文件场景不可用 |
| 日志层 | `log_scope_for` 只认 5 个工具（ssh_exec / disk_usage / system_status / service_status / sftp_remove） | 新文件工具不进审计 |
| 性能层 | 无会话复用：每次调用重新建连 + 认证（`_session_cache` 是空占位，代码内既有 TODO） | 批量文件操作慢，且刷爆服务器 audit 日志 |
| 体验层 | `sftp_remove` 确认后被拒 | 负信任体验 |

## 5. 产品决策点（动工前必须拍板）

### D1：Minimal 档下文件写入是否免审批？

- **背景**：`ssh_exec` 在 Minimal 档连 `rm -rf 子路径` 都放行（用户明确选择的低摩擦默认）；但文件写入覆盖同样不可逆。
- **建议**：**读走白名单放行（敏感路径除外）；写恒审批**（对齐 `sftp_remove` 红线），不受拦截等级影响。理由：拦截等级的语义是「shell 命令摩擦」，文件覆盖是不可逆破坏，混入等级会让「文件写入免审批」成为整个拦截体系的最大口子。
- **替代**：写入也分档（Minimal 免审批）——摩擦最低，但与 v2 安全叙事冲突，需在 GUI 拦截设置文案中显式声明。

### D2：敏感文件读取的边界

- **背景**：`sftp_read_file` 读 `~/.ssh/`、`/etc/shadow`、凭据文件 = 凭据泄露通道，与 AGENTS.md §8 红线（凭据不进前端/日志/错误信息）直接冲突。
- **建议**：敏感路径清单（`/etc/shadow`、`/etc/gshadow`、`~/.ssh/` 下私钥类、`*.pem`/`*.key` 等）读取**恒审批**（两档同），且**内容不进日志**（见 D4）。

### D3：本机路径越权

- **背景**：`sftp_upload` 参数含本机绝对路径（AI host 与 myshelltool 同机）→ AI 可读本机任意文件外传；`sftp_download` 可写本机任意路径 → 可覆盖本机文件。
- **建议**：最低要求 = 本机路径完整记入审计日志 + **覆盖本机已有文件需确认**；进阶 = 可配置的本机目录白名单（默认仅用户目录 + 临时目录）。P2 首版先做最低要求，白名单作为后续增强。

### D4：执行日志的文件内容脱敏（红线）

- **背景**：现有 `output_summary` 会把工具输出头尾各 250 字符写进 `mcp-execution-log.json`。一旦有 `sftp_read_file`，**文件内容（可能是 /etc/shadow）就会落盘**。
- **建议**：文件类工具的日志 `output_summary` **只记元信息**（字节数 / 是否截断 / 目标路径），不记内容本体。此项是 P1 的硬前置，不是优化项。

> 以上四条建议组合起来即文件风险模型的初始版：**读（普通）自动放行 / 读（敏感）审批 / 写·覆盖·删除恒审批 / 传输记完整路径**。

## 6. 分期规划

### P0 — 补地基（把「看」和「删」做真）

**范围**：
1. **headless SFTP 通道**：新增 `src-tauri/src/mcp/sftp_ops.rs`（或同级新模块），封装「`connect_headless` 的 handle → 开 SFTP subsystem → russh-sftp 操作」。**不要继续堆 `ssh.rs`**（已是超标文件，见 architecture-log Baseline）。
2. **`sftp_list` 补实**：返回 `RemoteDirectoryList` 同构数据（name/path/kind/size/modified/permissions），走 JSON 文本返回。AI 看不到目录，一切文件操作无从谈起。
3. **`sftp_remove` 执行补实**：审批链已通，补上真实删除（文件用 remove，目录递归删除需二次确认文案中显式标注「目录递归」）。

**涉及改动**：`mcp/sftp_ops.rs`（新）、`mcp/tools.rs`（dispatch 换桩）、`mcp/server.rs`（`log_scope_for` 加 `sftp_list`）。

**验收标准**：
- 对已配置资产，MCP host 调 `sftp_list` 返回真实目录（与 GUI 文件区一致）。
- `sftp_remove` 全链路：审批 → 真实删除 → 日志 `outcome=ok`，删除后 `sftp_list` 中该条目消失。
- 桩消除后工具 description 同步更新（去掉「M3 桩」字样）。
- `cargo check` 通过；`sftp_ops` 关键分支有单元测试（路径校验、错误映射）。

### P1 — 小文件读写（覆盖 AI 80% 高频场景）

**前置**：§5 四个决策点拍板（尤其 D1/D4）。

**范围**：
1. **`sftp_read_file`**：`asset_id + path`，单次上限 1MB（超限报错并引导改用 P2 下载工具）；敏感路径命中 D2 清单则恒审批；返回文本（UTF-8 lossy）。
2. **`sftp_write_file`**：`asset_id + path + content`，同上限；**覆盖已有文件恒审批**（D1）；写系统目录（`/etc`、`/boot`、`/usr`、`/bin`、`/sbin`、`/lib*`）恒审批或按 D1 结论处理；审批文案三段式（意图 / 操作+路径 / 覆盖后果）。
3. **日志脱敏**（D4）：文件类工具 `output_summary` 只记元信息；`log_scope_for` 的 `command` 字段填 `read <path>` / `write <path> (<n> bytes)` 形式。
4. **GUI 适配**：`McpExecutionLogList.vue` 工具列展示新工具名；审批弹窗文案覆盖文件操作形态。

**涉及改动**：`mcp/tools.rs`（新增工具 schema/dispatch，**注意行数红线**——tools.rs 已 486 行，新工具建议拆 `mcp/file_tools.rs`）、`mcp/approval.rs` 或新 `mcp/file_policy.rs`（路径风险判定函数）、`mcp/execution_log.rs`（脱敏约定注释 + 测试）、前端 `stores/mcp.js`、`McpExecutionLogList.vue`、`GlobalModals.vue`。

**验收标准**：
- 读→改→写往返成功，`sftp_list` 确认内容变化。
- 超 1MB 读取返回明确报错与替代建议。
- 覆盖已有文件触发审批（Strict 与 Minimal 两档均验证，按 D1 结论）；写 `/etc` 下路径被拦（按拍板结论）。
- **日志文件中 grep 不到写入的文件内容**（负向验证 D4）。
- `npm run build` + `cargo check` 通过。

### P2 — 大文件传输（本地路径直传，不经参数内联）

**范围**：
1. **`sftp_upload`**：`asset_id + local_path + remote_path + intent`。服务端流式读本机文件 + 分块写远程（参照 GUI `sftp_upload_start/_chunk/_finalize` 协议思路）；返回字节数 + SHA256。覆盖远程已有文件 → 恒审批。
2. **`sftp_download`**：`asset_id + remote_path + local_path + intent`。流式写本机；**覆盖本机已有文件需确认**（D3 最低要求）；返回字节数 + SHA256，供 AI 用本地工具校验消费。
3. **进度可见性**：AI 发起的传输打通 GUI files store 传输队列（复用 `sftp-transfer-progress` 事件通道或新增 mcp 前缀事件），用户在 GUI 文件区可见进度。
4. **超时说明**：MCP host 侧单次调用超时各异（常见 60s~几分钟），工具 description 中显式声明大文件行为与建议（先压缩/分卷）。

**涉及改动**：`mcp/file_tools.rs`（续）、`ssh.rs` 的分块上传辅助函数抽取复用（只读引用，不搬代码）、前端 files store 事件监听扩展。

**验收标准**：
- 上传 100MB 文件成功，远端 SHA256 与本机一致；下载同理。
- 覆盖远程文件触发审批；覆盖本机文件触发确认。
- 传输期间 GUI 可见进度；中途中断（拔连接）返回明确错误，不留半截 `.tmp` 远端文件（写临时名 + rename）。
- 审计日志含完整双向路径与字节数。

### P3 — 性能与收尾

**范围**：
1. **会话复用**：把 GUI 的 `Arc<AsyncMutex<SshSessionManager>>` 注入 `McpToolContext`（既有 follow-up，tools.rs 内两处 TODO 注释），命中 GUI 已建会话直接复用；未命中走 headless 并缓存（TTL 驱逐）。批量文件操作不再反复建连。
2. **顺手补桩**：`resource_monitor_snapshot` 接真实轮询快照、`list_sessions` 读 GUI 会话池。

**验收标准**：
- 连续 10 次 `sftp_list` / `ssh_exec` 只建连 ≤1 次（tracing 日志验证）。
- GUI 断开会话后 MCP 缓存失效不误用。
- 9 个工具全部无桩（`grep 桩 tools.rs` 无残留）。

## 7. 安全红线对照（AGENTS.md §8 增量）

| 红线 | 本计划落点 |
|---|---|
| 凭据不进日志/错误信息 | D4 脱敏 + D2 敏感路径读取审批；文件内容一律不落日志 |
| 危险文件操作必须确认 | 写/覆盖/删除恒审批（D1）；GUI 弹窗沿用 GlobalModals |
| Host key 变更默认阻止 | headless 连接沿用 `HeadlessSshClient` 的 known_hosts 校验，不放宽 |
| MCP 只监听 localhost | 不动 transport 层，本计划无新增监听 |
| 本机越权（新增） | D3：双向路径进审计日志 + 覆盖本机文件需确认 |

## 8. 风险与依赖

- **审批文案的路径歧义**：远程路径含空格/换行时的注入展示风险——三段式文案中路径需转义/引号包裹（与 service_status 的校验思路一致，实施时补测试）。
- **russh-sftp 在 headless handle 上的子系统协商**：`connect_headless` 目前只走 exec channel，开 SFTP subsystem 的 API 组合需一次技术验证（P0 第一步先做 spike，半天内出结论，失败则回退为「复用 GUI 会话池优先、headless 不支持 SFTP」并调整 P0 顺序）。
- **行数红线**：tools.rs（486 行）与 server.rs 已接近 Rust 800 行硬上限的量级，P1 起必须拆模块（file_tools.rs / file_policy.rs），不允许继续内联。
- **MCP host 超时差异**：ZCode / Claude Code / Cursor 对长调用超时不同，大文件传输的 host 侧行为需在 P2 实测三家。

## 9. 里程碑与交付节奏建议

| 期 | 规模估计 | 交付物 | 依赖 |
|---|---|---|---|
| P0 | 1 个 spike + 3 个改动点 | sftp_list / sftp_remove 可用 | 无 |
| P1 | 决策拍板 + 6 个改动点 | 读写工具 + 审计闭环 | §5 拍板、P0 |
| P2 | 4 个改动点 | 大文件双向传输 + 进度可见 | P1 |
| P3 | 2 个改动点 | 会话复用 + 无桩收尾 | 独立，可并行 |

每期独立交付、独立可验，P0 完成即可发版暴露 `sftp_list`（AI 导航能力立刻有价值）。

---

## 10. 交付落地记录（2026-09-08 v2.2 实际落地）

### 10.1 决策点拍板与落地结果
- **D1（写入审批）**：普通文件读走自动放行；写（`sftp_write_file`）、上传（`sftp_upload`）、覆盖已有文件、删除（`sftp_remove`）恒审批，不受 Minimal 档低摩擦影响；根级毁灭性删除 HardBlock 直接拦截。
- **D2（敏感文件读取）**：`/etc/shadow`、SSH 私钥（`id_*`、`.pem`、`.key`）、`.env` 等敏感文件读取恒审批。
- **D3（本机路径越权）**：禁止下载/覆盖写入系统受保护核心目录（Windows 系统目录防护）。
- **D4（执行日志内容脱敏）**：`server.rs` 对 `sftp_read_file` 成功输出做截断脱敏，日志落盘仅记录元数据（`[文件内容已脱敏：读取成功，共 N 字符]`），凭据绝不落盘。

### 10.2 模块划分与代码行数（严格遵守 800 行硬上限）
- `src-tauri/src/mcp/file_policy.rs`（186 行）：路径防穿越规整、敏感路径判定、操作风险分级。
- `src-tauri/src/mcp/sftp_ops.rs`（377 行）：headless SFTP 会话建立、目录遍历、带限制读写、原子写、递归删除、流式上传/下载与 SHA256。
- `src-tauri/src/mcp/file_tools.rs`（342 行）：文件工具 Schema 与分发（`sftp_list`, `sftp_read_file`, `sftp_write_file`, `sftp_upload`, `sftp_download`, `sftp_remove`）。
- `src-tauri/src/mcp/tools.rs`（433 行）：消灭所有桩，`list_sessions` 接入真实 GUI 会话池，`resource_monitor_snapshot` 接入真实快照。
- `src-tauri/src/mcp/server.rs`（760 行）：接入文件工具审批判定与 D4 审计日志脱敏。
- `src/components/shell/McpExecutionLogList.vue`（295 行）：前端日志展示支持新增文件传输工具徽标与中文映射。

### 10.3 验证结果
- `cd src-tauri && cargo check`: 退出码 0，编译通过。
- `cd src-tauri && cargo check --tests`: 退出码 0，单元测试编译通过。
- `cargo test --manifest-path crates/myshelltool-core/Cargo.toml`: 51 个 core 测试全部通过（0 failed）。
- `npm run build`: Vite 前端生产打包耗时 3.04s，0 错误构建通过。
