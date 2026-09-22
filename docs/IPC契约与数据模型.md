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
- **`import_ssh_config_preview({ path? })`**【v0.20/P0-1】→ 解析 OpenSSH config（默认 `~/.ssh/config`；core::ssh_config，first-match-wins + 通配参数组 + 逐块容错）返回 `{ sourcePath, candidates[] }`；每项含 alias/host/port/username/identityFile/proxyJump 与**冲突标记**（同 host:port:username 的现有资产 id——导入执行复用 `save_connection_asset`，前端逐条提交走与手工建资产完全相同的链路）。ProxyJump 映射为资产的 `jumpHost` 字段。

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
- `ssh_connect({...})` → 返回 `session_id`；参数含 host/port/username/authMethod（**v0.20 新增 Agent**：SSH agent 认证——named pipe 优先→Pageant 兜底、逐 key 尝试、私钥留在 agent 进程密钥零复制；headless/MCP 同路径支持）/credentialId/privateKeyPath 等；【v0.20/SSH P1】新增可选 `connectTimeoutSecs`（TCP+握手超时，空=不设）与 `keepaliveIntervalSecs`（keepalive 间隔，空=默认 30；inactivity 固定 = keepalive×10，保持「探测重试窗口充足」的分工约束）。资产 ConnectionAsset 增同名字段（serde default 兼容旧 JSON），编辑器表单可配；GUI/headless/MCP/SFTP/隧道/跳板全路径生效（跳板连接用跳板资产自己的参数）。ssh_list_directory 同形态参数
- **【v0.20/SSH P2】partial-success 认证链**：服务器 `AuthenticationMethods publickey,password` 双因子场景——russh 0.49 把 USERAUTH_FAILURE 的 partial 标志折叠成 false（无法精确区分「被拒」与「需第二因子」），故采用**顺序因子策略**（语义超集）：主方式失败后先尝试另一因子（PrivateKey 失败+有存密码→password；Password 失败+有私钥素材→publickey，素材惰性预读），成功即过；第二因子也失败落回原 keyboard-interactive 兜底。单因子服务器零影响。GUI 与 headless 双路径
- `ssh_list_directory` → 一次性 SFTP 列目录（走独立连接开 sftp 子系统，空 path 时 canonicalize 家目录；不依赖远端用户态工具，曾用 GNU-only `find -printf` 在 BusyBox/BSD 上必挂，已弃用）
- `ssh_write` / `ssh_resize` / `ssh_disconnect` / `ssh_confirm_host_key` / `ssh_keyboard_response`

**SFTP**（`ssh.rs`）
- `sftp_list_dir` / `sftp_read_file` / `sftp_write_file`
- **`sftp_chmod({ sessionId, path, mode })`【v0.20/SSH P2】**：修改远端权限（mode 八进制字符串，前置校验 3-4 位、仅 0-7、4 位首位须 0——setuid/setgid/sticky 暂不支持；russh-sftp setstat 只改 permissions 位；vendored fork 新增 `set_metadata` 透传）。GUI 入口：远程面板右键「修改权限…」（编辑器通用输入弹窗，预填当前权限，前后端双校验）
- **`sftp_readlink({ sessionId, path })`【v0.20/SSH P2】**：读符号链接目标。GUI 入口：symlink 条目右键「查看链接目标」
- `sftp_list_dir` 的 **path 为空串 = 服务器默认目录**：后端 `canonicalize(".")` 解析登录用户真实家目录（root → /root）并在返回的 `path` 字段回传绝对路径；前端不再猜测 home（旧版 `/home/<username>` 对 root 必错、tag 硬编码目录不存在即 No such file，已废弃）
- 流式上传：`sftp_upload_from_file({ sessionId, localPath, remotePath, transferId })`（【v2.9】替代旧 `sftp_upload_start/chunk/finalize` 三件套）——前端只传本机路径，后端读盘直写 SFTP（1 MiB 读块 + russh-sftp 写流水线），**字节不再经 IPC**（旧 8 MiB 分块经 JSON 数字数组序列化约 4 倍膨胀）。进度经 `sftp-transfer-progress` 事件节流上报（与下载同口径：1 MiB / 200ms + 收尾必发）；`total` 取打开时刻的本地文件长度（传输期间增长不截断，读到 EOF）。本机路径经 `fs_local::resolve_input_path` 解析（系统目录黑名单与面板读取同源）。取消/中途失败会**尽力删除远端半截文件**（对称下载侧的半截清理）。
- `sftp_upload_cancel({ transferId })` → 置共享取消旗标，上传循环在下一个读块边界中止（幂等，找不到条目不报错）；前端 `cancelTransfer` 先置本地标记再调本命令，后端 reject 后按取消态收敛（不挂重试）。
- **`sftp_download_cancel({ transferId })`【v0.20/S9】** → 下载取消：与上传共用 `transfer_cancels` 旗标表（`UploadCancelEntry` 已更名 `TransferCancelEntry`），下载循环在 64KiB 读块边界检查；取消后端返回 `[download:cancelled]` 错误前缀，前端收敛为 cancelled 态（非 error、不弹重试），半截本地文件走既有清理分支删除。TransferDrawer 上传/下载行均有取消按钮（「下载不可取消」边界解除）。
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
- `mcp_status` → 【v1.4】HTTP 健康检查 + 聚合能力清单。返回 `McpStatus { serverName, serverVersion, endpoint, dataDir, probe: McpProbeResult, tools[], resources[], prompts[] }`。`probe.ok` 是状态灯唯一信号源（向自己的 HTTP endpoint 发 initialize 握手，不再 spawn 子进程）。`endpoint` 是 MCP HTTP URL，**【v0.20/A1】含入口鉴权 token**（如 `http://127.0.0.1:41235/mcp/<token>`），面板「复制配置」直接可用；探测请求同样带 token（否则 401，状态灯全灭）。
- `mcp_reset_token` → 【v0.20/A1】重置入口鉴权 token：换新（CSPRNG 256-bit → base64url）→ 更新共享 Arc（运行中 server 立即按新值校验，**旧 token 即刻失效**）→ 重写 mcp-endpoint.json → 返回含新 token 的完整 URL。endpoint 文件缺失（server 未起来）或落盘失败返回 Err（不假装成功）。
- `mcp_get_config` → 【v2】读 MCP 危险命令拦截等级（内存共享配置，不动盘）。
- `mcp_set_config({ level })` → 【v2】切换拦截等级（`minimal` 仅硬拦毁灭性命令、其余直接执行 / `strict` 非白名单一律确认；毁灭性命令两档恒拦）→ 更新共享 Arc（已建 MCP 会话下次调用即生效）→ 落盘 mcp-config.json；无效 level 返 Err。
- `mcp_list_execution_logs({ limit? })` → 【v2】读 MCP 工具执行日志最近条目（timestampMs 倒序，limit 缺省 200）。
- `mcp_clear_execution_logs` → 【v2】清空 MCP 执行日志。
- 【v1.4 已删】`mcp_approval_resolve`（原 v1.1 pipe 审批回传，内嵌后无 pipe）
- **MCP 工具面【v0.20/3-6 月段】**：`exec_many`（多机 fan-out：`asset_ids` 数组 + `command` + `intent`；单次上限 64 台、并发 8 台排队（= 会话池容量）；逐目标 scope 判定 / 三层会话复用（GUI→池→新建）/ 每目标 16KiB 输出限幅标 truncated；审批按 command 文本一次判定（ShellExec 策略）放行整批。MCP 工具总数 15）
- **C1 长任务【v0.20/3-6 月段】**：`ssh_exec_async`（后台执行返回 job_id；断连真取消——远端收 SIGHUP）+ `job_status`（状态/退出码/输出字节/错误）+ `job_output`（offset/limit 分页，单页上限 1MiB）+ `job_cancel`（disconnect 收敛 cancelled）。job 内存态不落盘、TTL 30 分钟、条数上限 32、单 job 输出 8MiB ring buffer（截头保尾如实报 dropped）。ssh_exec_async 标注 rmcp TaskSupport::Optional（host 支持原生 tasks 时自动走任务流）。工具总数 19
- **C3 只读工具【v0.20/3-6 月段】**：`journal_query`（journalctl 结构化查询：unit/since/until/grep/limit——服务端固定模板 `env LC_ALL=C journalctl --no-pager -o short-iso -n <limit>`；跨发行版口径：不用 -g（systemd>=237 限制）改管道 grep -F、非 systemd 返回 127 与降级指引；注入防护：unit 白名单 + since/until/grep 禁单引号反斜杠）
- `port_listen`（监听端口结构化：**ss → netstat → lsof 三级降级链固化在服务端**（`if command -v ss … elif netstat … elif lsof … else exit 127`），输出首行标注实际命中工具；三段均 `env LC_ALL=C`；lsof 兜底段后置 grep LISTEN（rc 反映 grep）；tcpOnly 参数收窄协议面；无自由文本参数——模板完全固定不可注入）
- `process_list`（进程列表结构化：ps POSIX 列集 `pid,ppid,user,%cpu,%mem,rss,stat,etime,comm`；`--no-headers` 是 procps 扩展——**子 shell 包裹** `(cmd1 || cmd2)` 保证 BSD 降级路径也进同一 `sort -k<N> -nr | head -N` 管道（shell `|` 优先于 `||`，不包裹首选路径会绕过排序）；sortBy=cpu/mem 枚举白名单、limit 数字——无自由文本）
- `file_search`（按条件查找文件：find POSIX 形态，name 通配/minSizeMb/mtimeDays；**不用 GNU 专有格式化输出选项**；**结果落临时文件再 head 截断**——`rc_find` 如实反映 find（管道后 rc 恒为 head 的，no-pipeline-rc-echo 事故口径），mktemp 失败退化为 /tmp 专名文件；结果上限默认 100 最大 1000 防炸上下文；path 禁空格/分号/单引号）
- `service_control`（systemctl start/stop/restart/reload：action **枚举白名单**（语义化风险——比 ssh_exec 更明确地被拦）；执行后自动补 is-active 确认最终态；**Policy=RemoteWrite**——Minimal 直接执行记日志 / Strict 弹窗确认（审批文案动态生成：服务名+动作+后果））
- `tunnel_list`（列出隧道与状态：kind/active/error 字段；只读）
- `tunnel_create`（**remote 转发**：走资产专用连接，**复用 GUI tunnel_create/tunnel_start 同一路径**（G3：不另写）；MVP 仅 remote（local/dynamic 需活跃会话，提示经 GUI 建——诚实边界）；scope 判定同 exec；**Policy=RemoteWrite**（网络暴露面变更，审批文案注明非回环绑定的外网可达风险））
- **C5 prompts 多资产【v0.20/3-6 月段，MCP 阶段 C 收官】**：三个诊断 prompt 的 `asset_id` → **`asset_ids`**（JSON 数组 / 逗号分隔 / 单值三形态兼容，旧 `asset_id` 参数名后向兼容）；diagnose_server 多机时输出**对比表**口径（行=检查项、列=资产、突出偏离基线者）；audit_security/cleanup_disk 的手编命令段改为引导用 C3 固化工具（port_listen/process_list/file_search）；**「查不到 ≠ 无异常」纪律保留并扩展多机语义**（一台失败 ≠ 该台无异常 ≠ 其他台可跳过——4 个单测锁住，CI Linux 真跑）。**MCP 阶段 C 全部完成（26 工具 + 3 prompts 多资产）**

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
credential_id(Option), passphrase_credential_id(Option),
private_key_credential_id(Option), jump_host(Option, "host"/"host:port"【v0.20/P0-2】)
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
- **MCP 入口鉴权 token** → `mcp-endpoint.json` 的 `authToken` 字段（【v0.20/A1】256-bit CSPRNG → base64url 43 字符；server 每次启动重写本文件；`#[serde(default)]` 兼容旧文件——读出空串时 fail-closed 全拒。token 同时存于 AppState 内存 Arc 供中间件校验，**不进日志**）
- **MCP 执行日志** → `mcp-execution-log.json`（【v2】30 天惰性清理 + 上限 1000 条，append 时触发；tokio Mutex 串行化 + `.tmp`/rename 原子写；不含任何凭据字段）
- **活跃会话/隧道/SFTP 缓存** → 纯内存（重启丢失）

### McpStatus / McpProbeResult（MCP 健康检查，`lib.rs` + `mcp/probe.rs` + `mcp/http_server.rs`）
- **设计取舍（v1.4）**：MCP server 内嵌 GUI 进程，用 Streamable HTTP transport 对外暴露（`http://127.0.0.1:41235/mcp`）。状态灯 = HTTP 健康检查：GUI 每次 `mcp_status` 向自己的 endpoint 发 MCP `initialize` 握手，成功即「可用」。**不再 spawn 子进程**（v1.2 的一次性 spawn 已废弃，根治僵尸进程 + os error 32）。
- **【v0.20/A1】入口鉴权**：`127.0.0.1` 不是安全边界（本机任意进程可连回环端口），故入口加 token——路由形态 `/mcp/<token>`（主）+ `Authorization: Bearer` header（辅），统一 401 不区分「路径错」与「token 错」。判定在 `core::mcp_auth`（段边界/fail-closed 有穷举单测）；中间件 AllowRewrite 剥掉 token 段再进路由。重置走 `mcp_reset_token`（内存 Arc 即时生效 + 落盘）。
- **v1.4 架构变化**：取消双二进制（删 `bin/mcp.rs` + `pipe.rs`），MCP server 直接跑在 GUI 进程内，SSH 会话/资产/审批同进程访问。任何合规 MCP host（Claude Code / Cursor）经 HTTP URL 连入。
- `McpProbeResult { ok: bool, reason?, detail?, exePath, serverInfo?, probedAt }`
  - `reason` 失败分类码：`endpoint_not_found`（server 未启动/未写 endpoint 配置）/ `http_error`（连不上）/ `timeout`（2s）/ `bad_protocol`（握手响应异常）
  - `exePath`：v1.4 语义改为 HTTP endpoint URL（字段名保留兼容前端 mcp.ts）
- 前端 `mcp.ts` 的 `clientConnected` computed 读 `probe.ok`（命名保留是为避免连锁改名，语义已是健康检查结果）。
