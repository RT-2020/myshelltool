# IPC 契约与数据模型

> **何时必读**：新增/修改 Tauri 命令或事件、改同步（sync）语义、改 MCP 工具、动持久化文件格式前。
> 硬约束（命令注册、camelCase、凭据红线）以 [`AGENTS.md`](../AGENTS.md) §4.3 / §8 为准；本文是完整清单与契约细节。
>
> 维护：命令/事件/持久化格式增删或语义变化时改本文；`AGENTS.md` §6/§7 只保留索引与不变量，不回贴细节。

## IPC 契约（前端 ↔ Rust，关键命令清单）

前端通过 `src/services/backend.ts` 的 `invokeBackend(command, args)` / `listenBackendEvent(event, handler)` 调用 Rust。**非 Tauri runtime 会抛错**（浏览器预览模式无 SSH 功能）。

### 命令分组（在 `src-tauri/src/lib.rs` 与 `ssh.rs` 注册）

**资产持久化**（`lib.rs`，存 `<app_data_dir>/connection-assets.json`）
- `list_connection_assets` → `{ source, count, assets, groups }`
- `save_connection_asset({ asset })` → upsert，返回更新后的列表
- `delete_connection_asset({ id })` → 删除 + 容错清理关联凭据
- `rename_asset_group({ oldPath, newPath })` / `dissolve_asset_group({ path })` / `create_asset_group({ path })` → 分组批量操作

**凭据**（`lib.rs`，存 `<app_data_dir>/credentials/<id>.cred`）
- `save_credential({ id, secret })` / `get_credential_status({ id })` / `delete_credential({ id })`
- 凭据 id 约定：`<assetId>:<kind>`（kind = `password` | `passphrase`）

**Gist 资产同步**（`sync.rs`，v1.3）
- `sync_status` → 同步配置状态（是否已配置 / 上次同步时间 / gist_id 掩码 / `auto_sync_enabled`＝本机已记住免密密钥 / **`local_has_changes`【v2.7】＝本机有未推送改动**，面板据此决定「推送到云端 / 从云端拉取」谁是主按钮）
- `sync_setup({ masterPassword, gistId? })` → 首次设置（主密码派生密钥 + 可选拉取已有 Gist）。**【v2.7】成功后自动调用 `remember_session_key`**：按该载荷的 salt 重建 key、DPAPI 存盘并置 `auto_sync_enabled = true` —— 主密码只输这一次（或换机时），此后 push/pull 与自动同步全免密
- **`sync_discover_gists()`** → 换机恢复免填 Gist ID：列出当前 GitHub 账号下的备份候选（`GET /gists` 分页拉满，per_page=100 上限 10 页），**按文件名 `myshelltool-sync.json` 过滤**（description 用户可改，不是判定标记），只读元数据不碰加密载荷。返回 `[{ gist_id, updated_at }]`，顺序不做承诺（前端 `Date.parse` 解析后排序，未知/非法时间排尾）。PAT 未配置 → Err 引导先登录；HTTP 401（token 被 revoke）→ Err 指向「账号」区重新登录。选中候选后仍走 `sync_setup` 提交（签名未变）
- `sync_push({ masterPassword })` / `sync_pull({ masterPassword })` → 加密推送 / 拉取解密（pull 返回 Conflict 时前端弹框）。`masterPassword` **留空 = 走本机会话密钥**（免密路径，需已启用）；**带密码且本机还没有密钥时顺手记住**（`remember_session_key`，按该载荷的 salt）——「输一次密码」就够开启免密
- `sync_resolve_conflict({ masterPassword, choice })` → 冲突解决（local_overwrite / remote_overwrite）
- `sync_reset_master_password({ old, new })` / `sync_clear`
- **`sync_generate_recovery_password()`**【v2.7】→ 生成 24 位高熵恢复密码（去歧义字符集，`core::recovery_code`）→ 存 SecretStore（id `sync-recovery-password`，DPAPI）→ **返回明文**供界面填入并明示一次。让用户"零输入"也能建备份，且换机时有据可依（不依赖外部密码管理器）
- **`sync_reveal_recovery_password()`**【v2.7】→ 读回已保存的恢复密码（供「换机恢复 → 查看这台电脑的恢复密码」）；用户手输的密码从未保存，此处返回 None 时前端如实说明"应用没有保存它"

**GitHub Device Flow 登录**（`sync_oauth.rs`，v1.3；【v2.7】抗抖动改造）
- `sync_oauth_start({ provider })` → `{ userCode, verificationUri, expiresIn, interval }`（`Err` = 请求设备码失败/未注册 client_id）
- `sync_oauth_poll()` → tag enum `status`：`pending` / `slow_down`（带 `user_code`、【`interval`】）| `denied` | `expired` | `success` | **`unstable{user_code, reason}`**（本次没问到：网络/代理抖动、429/5xx、非 JSON 响应 → 前端退避重试，**设备码仍有效**）| `superseded`（后端已无本流程 session → 前端静默回 idle）| **`failed{reason}`**（GitHub 明确报错 / token 落库失败 → 终态）。`Err` 只留给后端内部错误（锁中毒）
- `sync_oauth_cancel()` → 清内存槽（单槽：新 start 覆盖旧 start）

**SSH 会话/终端**（`ssh.rs`）
- `ssh_connect({...})` → 返回 `session_id`；参数含 host/port/username/authMethod/credentialId/privateKeyPath 等
- `ssh_list_directory` → 一次性 SFTP 列目录（走独立连接开 sftp 子系统，空 path 时 canonicalize 家目录；不依赖远端用户态工具，曾用 GNU-only `find -printf` 在 BusyBox/BSD 上必挂，已弃用）
- `ssh_write` / `ssh_resize` / `ssh_disconnect` / `ssh_confirm_host_key` / `ssh_keyboard_response`

**SFTP**（`ssh.rs`）
- `sftp_list_dir` / `sftp_read_file` / `sftp_write_file`
- `sftp_list_dir` 的 **path 为空串 = 服务器默认目录**：后端 `canonicalize(".")` 解析登录用户真实家目录（root → /root）并在返回的 `path` 字段回传绝对路径；前端不再猜测 home（旧版 `/home/<username>` 对 root 必错、tag 硬编码目录不存在即 No such file，已废弃）
- 流式上传：`sftp_upload_from_file({ sessionId, localPath, remotePath, transferId })`（【v2.9】替代旧 `sftp_upload_start/chunk/finalize` 三件套）——前端只传本机路径，后端读盘直写 SFTP（1 MiB 读块 + russh-sftp 写流水线），**字节不再经 IPC**（旧 8 MiB 分块经 JSON 数字数组序列化约 4 倍膨胀）。进度经 `sftp-transfer-progress` 事件节流上报（与下载同口径：1 MiB / 200ms + 收尾必发）；`total` 取打开时刻的本地文件长度（传输期间增长不截断，读到 EOF）。本机路径经 `fs_local::resolve_input_path` 解析（系统目录黑名单与面板读取同源）。取消/中途失败会**尽力删除远端半截文件**（对称下载侧的半截清理）。
- `sftp_upload_cancel({ transferId })` → 置共享取消旗标，上传循环在下一个读块边界中止（幂等，找不到条目不报错）；前端 `cancelTransfer` 先置本地标记再调本命令，后端 reject 后按取消态收敛（不挂重试）。
- `sftp_download_to_file` / `sftp_mkdir` / `sftp_rename` / `sftp_remove` / `sftp_stat`
  - 下载已改为**流式落盘**（后端分块读 → `tokio::fs` 直接写本地文件，进度事件节流 1 MiB / 200ms）；字节不再经过 IPC，也不返回 `Vec<u8>`。前端用 `plugin:dialog|open`（`directory: true`）让用户选**保存目录**、文件名沿用远端名，再调用它；批量下载整批只弹一次目录框（不是每文件一次）。

**编辑器文本读写**（`ssh/text_file.rs` + `fs_local.rs` + `editor_store.rs`，v0.18 内置编辑器专用链路）
- `sftp_read_text({ sessionId, path, encoding? })` / `fs_local_read_text({ path, encoding? })` → `ReadTextResult { content, encoding, hasBom, eol("lf"|"crlf"|"mixed"), size, modified, readOnly? }`。防线：lossy 路径守卫 → 句柄/路径 stat 判 **2 MiB 上限**（超过 `[editor:too-large]`，确定性错误不给重试）→ 二进制嗅探（`core::remote_text` 三态，选了 GBK 也救不了 PNG）→ 未指定编码仅接受 UTF-8（非 UTF-8 报 `[editor:not-utf8]` 含首个非法偏移，前端提供「按 GBK 重试」）→ 指定编码走 `core::text_codec` 白名单严格转码（encoding_rs `had_errors` 当硬错误，绝不 lossy）。UTF-8 BOM 读入剥离、写回按 keepBom 还原。
- `sftp_write_text` / `fs_local_write_text`（参数多一组：`{ ..., content, encoding?, eol?("lf"|"crlf"|null=原样), keepBom?, expectedSize?, expectedModified?, backup? }`）→ `WriteTextResult { status: "written"|"conflict", size, modified, conflictNote? }`。语义：
  - **expected 守护写**：`expectedSize`/`expectedModified` 任一提供即守护（stat 不一致返回 `status:"conflict"`，**不写任何字节**，前端三选：覆盖=expected 置空重写 / 另存为 / 重新加载）；两者都为 null = 强制写（新建/另存为，前端已做自己的覆盖确认）。
  - **原子写**：`<path>.myshelltool.<uuid>.tmp` + rename 覆盖兜底（`ssh::sftp_rename_with_overwrite_fallback`，从 MCP `sftp_ops.rs` 提取共享；**失败不删 temp**——temp 是新数据唯一副本，错误信息带 temp 路径）。本地侧 Windows rename 不覆盖 → 先 remove 再 rename（注释说明窗口期）。
  - **写前备份**：`backup=true` 且目标存在 → 旧字节先快照进 app-data `editor-backups/`（每文件滚动 3 份）；备份写失败**中止保存**（fail-closed：没有备份的覆盖等于裸覆盖）。
- 错误前缀协议（前端 `lib/editor/editorIo.ts` 分类依据）：Err(String) 正文以 `[editor:kind]` 开头，kind ∈ too-large / binary / not-utf8 / encoding / lossy / not-found / dir / permission；无前缀按网络类（可重试）。
- `editor_backup_list({ target, path })` / `editor_backup_read({ target, path, id, encoding? })`：备份元数据与只读读取（id 为纯数字毫秒时间戳，进文件名前校验防穿越）。
- `editor_draft_save({ target, path, content, encoding, eol })` / `editor_draft_get` / `editor_draft_delete`：草稿存 app-data `editor-drafts/`（UTF-8 正文 + meta；key=FNV-1a64(target+path)，meta 回存原文并校验防串）。

**隧道**（`ssh.rs`，仅内存）
- `tunnel_create` / `tunnel_start` / `tunnel_stop` / `tunnel_list` / `tunnel_delete`
- **【v2.9】remote（远程端口转发）已实现**：`TunnelConfig.asset_id`（serde default None）是 remote 的必填认证上下文——`tunnel_start` 按 asset_id 从本地资产库解析认证（与 MCP `exec_on_asset` 同路径），经 `connect_headless_with` 建**专用连接**后发 `tcpip-forward`（russh 0.49 的该方法要 `&mut Handle`，共享 `Arc<Handle>` 给不出）；host key 用 headless 同款「仅 known_hosts 精确匹配」策略（后台连接不弹窗）。到达的 `forwarded-tcpip` 通道只在「端口与已登记转发匹配且地址相容」时接管桥接到 `local_addr:local_port`，未登记通道一律丢弃（防恶意服务器借客户端探测本机服务）。停止/删除/会话清理 abort 驻守任务 → drop Handle → 服务器监听随连接消失；驻守任务发现连接死亡会回写 `active=false` + error（tunnel_list 不谎报）。local/dynamic 不变（复用会话句柄）。

**资源监控**（`resource_monitor.rs`）
- `resource_monitor_start` / `_stop` / `_snapshot` / `_list_active`

**本地文件**（`fs_local.rs`）
- `fs_local_home_dir` / `fs_local_list_dir` / `fs_local_mkdir` / `fs_local_delete` / `fs_local_rename` / `fs_local_stat`
  - `fs_local_stat(path)` → 单路径 stat（`RemoteFileEntry`：name/path/kind/size/modified），供上传入口（原生文件对话框 / OS 拖入）过滤目录并取进度分母初始值。
  - 【v2.9 已删】`fs_local_read_chunk` / `fs_local_write_chunk`：分块上传链路消亡后双双无调用方（后者本就无），按死代码红线移除。

**MCP 服务**（`lib.rs` 调 `mcp/http_server.rs` + `mcp/probe.rs`）
- `mcp_status` → 【v1.4】HTTP 健康检查 + 聚合能力清单。返回 `McpStatus { serverName, serverVersion, endpoint, dataDir, probe: McpProbeResult, tools[], resources[], prompts[] }`。`probe.ok` 是状态灯唯一信号源（向自己的 HTTP endpoint 发 initialize 握手，不再 spawn 子进程）。`endpoint` 是 MCP HTTP URL（如 `http://127.0.0.1:41235/mcp`），供用户配置 MCP host。
- `mcp_get_config` → 【v2】读 MCP 危险命令拦截等级（内存共享配置，不动盘）。
- `mcp_set_config({ level })` → 【v2】切换拦截等级（`minimal` 仅硬拦毁灭性命令、其余直接执行 / `strict` 非白名单一律确认；毁灭性命令两档恒拦）→ 更新共享 Arc（已建 MCP 会话下次调用即生效）→ 落盘 mcp-config.json；无效 level 返 Err。
- `mcp_list_execution_logs({ limit? })` → 【v2】读 MCP 工具执行日志最近条目（timestampMs 倒序，limit 缺省 200）。
- `mcp_clear_execution_logs` → 【v2】清空 MCP 执行日志。
- 【v1.4 已删】`mcp_approval_resolve`（原 v1.1 pipe 审批回传，内嵌后无 pipe）

### 事件（Rust → 前端，`listenBackendEvent`）
- `sftp-transfer-progress`、`ssh-host-key-verify`、`ssh-keyboard-interactive`、`ssh-session-status`
- 【v2.5】`resource-monitor-error`（payload camelCase `sessionId`/`reason`）：远端无 /proc（非 Linux）等不可恢复采样失败，emit 后该会话轮询即停止，前端展示错误态
- 【v1.4 已删】`mcp-approval-verify`（原 v1.1 pipe 审批委托事件，内嵌后无 pipe）

---

## 数据模型

### ConnectionAsset（连接资产，`crates/myshelltool-core/src/asset_store.rs`）
```rust
id, name, host, port(u16), username, auth_method(Password|PrivateKey|Token),
private_key_path(Option), group(String, '/' 分隔多级路径如 "生产/数据库"),
tags(Vec<String>), status(Connected|Warning|Idle), last_connected,
credential_id(Option), passphrase_credential_id(Option)
```
- 前端经 `normalizeAsset()`（`backend.ts`）规整：默认 port=22、group=「未分组」、status=Idle。
- **分组不是独立实体**，是 `asset.group` 字符串字段（`/` 分隔层级）。空分组用 `ConnectionAssetStore.groups: Vec<String>` 单独持久化。
- 「未分组」是保留顶级，不可重命名/解散。

### 持久化分层
- **资产元数据** → `connection-assets.json`（JSON-on-disk，无密钥）
- **凭据** → `credentials/<id>.cred`（弱 XOR 混淆，非加密）
- **同步状态 / 会话密钥** → `sync-state.json`（gist_id / local_rev / auto_sync_enabled / last_synced_at，无秘密）+ `credentials/sync-session-key.cred`（payload = `base64(key):base64(salt)`，DPAPI 保护）。**【v2.7】`sync_setup` 成功即写入**（免密默认化），salt 必须与「该次载荷的 salt」一致（`core::sync::session_key_for_payload`），否则免密路径与主密码路径拿到两把不同的 key
- **恢复密码** → `credentials/sync-recovery-password.cred`（DPAPI）。**【v2.7】只保存应用生成的那份**（用户点「生成强密码」时写入）；用户手输的密码一律不落盘（[`AGENTS.md`](../AGENTS.md) §8 凭据红线的范围）。`sync_setup` / `sync_reset_master_password` 成功后若发现保存值与本次使用的主密码不同 → **删除**（那份已打不开当前备份，留着会让「查看」给出错值）
- **Gist 同步载荷** → `SyncPayload { version, blob{salt,nonce,ciphertext}, remote_rev }`。**【v2.7】`blob.salt` 必须非空**：会话密钥 = `Argon2id(主密码, salt)`，salt 不随载荷上传时换机后主密码无法重建同一把 key（备份永久解不开）。会话密钥路径与主密码路径共用同一 salt，故**同一份密文两条路都能解**；`crypto::encrypt_with_key` 对空 salt 直接返回 Err（fail-closed，杜绝再产出「只有本机能解」的载荷）。**【v2.7】只支持这一种格式**：`parse_vault_plaintext` 只认 `SyncVaultData`（不再兼容旧版「纯 assets JSON」），`crypto::decrypt` / `decrypt_with_key` 对空 salt 统一返回 `LEGACY_PAYLOAD_REJECTED`（明确说「旧格式不再支持，请重推一次」）——不允许出现「密码路径报错、会话密钥路径却能悄悄解开」的半支持状态
- **known_hosts** → `known_hosts.json`
- **MCP 拦截等级** → `mcp-config.json`（【v2】minimal/strict，缺省 Minimal；与 endpoint 同目录，经 mcp_data_dir() 解析）
- **MCP 执行日志** → `mcp-execution-log.json`（【v2】30 天惰性清理 + 上限 1000 条，append 时触发；tokio Mutex 串行化 + `.tmp`/rename 原子写；不含任何凭据字段）
- **活跃会话/隧道/SFTP 缓存** → 纯内存（重启丢失）

### McpStatus / McpProbeResult（MCP 健康检查，`lib.rs` + `mcp/probe.rs` + `mcp/http_server.rs`）
- **设计取舍（v1.4）**：MCP server 内嵌 GUI 进程，用 Streamable HTTP transport 对外暴露（`http://127.0.0.1:41235/mcp`）。状态灯 = HTTP 健康检查：GUI 每次 `mcp_status` 向自己的 endpoint 发 MCP `initialize` 握手，成功即「可用」。**不再 spawn 子进程**（v1.2 的一次性 spawn 已废弃，根治僵尸进程 + os error 32）。
- **v1.4 架构变化**：取消双二进制（删 `bin/mcp.rs` + `pipe.rs`），MCP server 直接跑在 GUI 进程内，SSH 会话/资产/审批同进程访问。任何合规 MCP host（Claude Code / Cursor）经 HTTP URL 连入。
- `McpProbeResult { ok: bool, reason?, detail?, exePath, serverInfo?, probedAt }`
  - `reason` 失败分类码：`endpoint_not_found`（server 未启动/未写 endpoint 配置）/ `http_error`（连不上）/ `timeout`（2s）/ `bad_protocol`（握手响应异常）
  - `exePath`：v1.4 语义改为 HTTP endpoint URL（字段名保留兼容前端 mcp.ts）
- 前端 `mcp.ts` 的 `clientConnected` computed 读 `probe.ok`（命名保留是为避免连锁改名，语义已是健康检查结果）。
