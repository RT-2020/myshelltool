# Follow-up: SSH State 类型归一化（Option A 重构）

**Status:** scheduled（P0-1 PR 内强制创建，绑定到下一个 sprint 承诺）
**Source plan:** `.omc/plans/ralplan-fix-ui-ssh.md`（v2 ADR Follow-up #1）
**Created:** 2026-06-12（与 P0-1 PR 同提交）
**Owner:** TBD（P0-1 合并后 3 天内分配）
**Target date:** **2026-06-26**（P0-1 合并后 ≤2 个 sprint，从 2026-06-12 推算）

---

## 1. 背景

P0-1（方案 B）为快速解封致命 SSH bug，在 `src-tauri/src/lib.rs` 的 setup 中添加了 `app.manage(ssh_mgr.clone())` 行，使 `ssh.rs` 25 个 State 参数站点的 `State<'_, Arc<Mutex<SshSessionManager>>>` 类型能被 StateManager 解析。

这是**战术性折衷**：同一 Arc 被两个 TypeId key（`AppState` 和 `Arc<AsyncMutex<SshSessionManager>>`）双重持有，运行期数据一致但语义上有耦合。本 follow-up 的目标是**消除双 manage 模式**，把所有 SSH 命令统一为 `State<'_, AppState>`，从结构上关闭这一类 bug。

---

## 2. 范围（Scope）

### 2.1 ssh.rs 改动（25 个 State 参数站点）

`src-tauri/src/ssh.rs` 全部需要从 `State<'_, Arc<Mutex<SshSessionManager>>>` 改为 `State<'_, AppState>`：

- **23 个 `#[tauri::command]` 命令**（已通过 grep 确认，覆盖 `ssh_*`、`sftp_*`、`tunnel_*`）：
  1. `ssh_connect`
  2. `ssh_list_directory`
  3. `ssh_write`
  4. `ssh_resize`
  5. `ssh_disconnect`
  6. `ssh_confirm_host_key`
  7. `ssh_keyboard_response`
  8. `sftp_list_dir`
  9. `sftp_read_file`
  10. `sftp_write_file`
  11. `sftp_upload_with_progress`
  12. `sftp_download_with_progress`
  13. `sftp_mkdir`
  14. `sftp_rename`
  15. `sftp_remove`
  16. `sftp_stat`
  17. `tunnel_create`
  18. `tunnel_start`
  19. `tunnel_stop`
  20. `tunnel_list`
  21. `tunnel_delete`
  22-23. 其余 2 个命令（执行者在 PR 时通过 `grep -n "State<'_, Arc<Mutex<SshSessionManager>>>" src-tauri/src/ssh.rs` 复核完整列表）

- **2 个内部辅助函数**（参数带 `&`，签名也要改 `&State<'_, Arc<...>>` → `&State<'_, AppState>`）：
  - `connect_authenticated`（`src-tauri/src/ssh.rs:193`）
  - `get_or_create_sftp`（`src-tauri/src/ssh.rs:635`）

### 2.2 调用点改动

每个命令的函数体内，原来的 `state.lock().await` 必须改为 `state.ssh_sessions.lock().await`（约 25 处，与签名一一对应）。

### 2.3 lib.rs 改动

- **`AppState` 改 `pub`：** `struct AppState` → `pub struct AppState`（`src-tauri/src/lib.rs:11`）
- **字段改 `pub`：** `ssh_sessions` 字段必须 `pub`（其他字段 `asset_store_path`、`secret_store_dir` 若 ssh.rs 不直接用，可不 pub；按编译器提示最小化暴露）
- **删除 P0-1 添加的 `app.manage(ssh_mgr.clone())` 行**（`lib.rs` setup 中）— 此时 StateManager 中只剩 `AppState` 一个 key，所有 25 个站点通过 `state.ssh_sessions` 访问。
- **删除 P0-1 添加的 4 行注释**（"注册独立的 Arc..."）
- **删除 P0-1 添加的 `Arc::ptr_eq` 测试** `ssh_state_keys_point_to_same_arc`（不再适用，因双 manage 模式已消除；可替换为新的简单测试，断言 `app.state::<AppState>()` 能解析且 `ssh_sessions` 字段非 None）

### 2.4 不变的部分

- `lib.rs` 的 `app.manage(AppState { ... })` 注册方式不变。
- 已工作的 6 个命令（`backend_status` / `list_connection_assets` / `save_connection_asset` / `save_sync_settings` / `save_credential` / `get_credential_status` / `delete_credential`）签名不动。
- `generate_handler!` 列表不动。
- 前端代码不动（invoke 调用方无感）。

---

## 3. 明确的验收标准（Acceptance Criteria）

### 改动正确性

- [ ] **AC-FU-1:** `src-tauri/src/ssh.rs` 中 grep `State<'_, Arc<Mutex<SshSessionManager>>>` 返回 **0 个匹配**（25 个站点全部改为 `State<'_, AppState>`）。
- [ ] **AC-FU-2:** `src-tauri/src/ssh.rs` 中 grep `State<'_, AppState>` 返回 **25 个匹配**（23 个命令 + 2 个辅助）。
- [ ] **AC-FU-3:** `src-tauri/src/lib.rs` 中 `AppState` 声明为 `pub struct AppState`，`ssh_sessions` 字段声明为 `pub ssh_sessions`。
- [ ] **AC-FU-4:** `src-tauri/src/lib.rs` setup 中 `app.manage(ssh_mgr.clone())` 行**已删除**（grep 不再命中该独立 manage）。
- [ ] **AC-FU-5:** `src-tauri/src/lib.rs` 中 P0-1 添加的 4 行注释（"注册独立的 Arc..." / "类型精度说明..." / "AppState.ssh_sessions 字段保留..." / "注意：未来如需替换 manager 实例..."）**已删除**。
- [ ] **AC-FU-6:** `src-tauri/src/lib.rs` 中 P0-1 添加的 `ssh_state_keys_point_to_same_arc` 测试**已删除**（或替换为新的 `app_state_resolves_correctly` 测试，断言 `app.state::<AppState>()` 能解析）。
- [ ] **AC-FU-7:** ssh.rs 内每个原 `state.lock().await` 调用点（25 处）已改为 `state.ssh_sessions.lock().await`。

### 构建与测试

- [ ] **AC-FU-8:** `cd src-tauri && cargo check` exit 0。
- [ ] **AC-FU-9:** `cd src-tauri && cargo test` 全部通过（无 P0-1 的 Arc::ptr_eq 测试，新增的 app_state 测试通过）。
- [ ] **AC-FU-10:** `cd src-tauri && cargo clippy`（若已配置）无新增 warning。

### 端到端验证

- [ ] **AC-FU-11:** `npm run tauri:dev`（验证者先装 pnpm 或 P2-1 已合并），DevTools Console 执行 `ssh_connect` invoke 不再因 state 解析失败立即 reject。
- [ ] **AC-FU-12:** 桌面客户端实测：新增一台 SSH 主机 → 点击连接 → 终端出现 PTY 提示符（如 `user@host:~$`）。
- [ ] **AC-FU-13:** 在 SSH 会话中敲键盘，命令字符回显正常（验证 `ssh_write` 也工作）。
- [ ] **AC-FU-14:** 测试 SFTP：在文件 tab 浏览远程目录、上传一个文件、下载一个文件，全部成功。
- [ ] **AC-FU-15:** 测试 tunnel：创建并启动一个 local forward，验证端口转发工作；停止并删除。

### 回归

- [ ] **AC-FU-16:** 6 个原已工作的命令（`backend_status` / `list_connection_assets` / `save_connection_asset` / `save_sync_settings` / `save_credential` / `get_credential_status` / `delete_credential`）行为不变（侧边栏资产 CRUD、凭据保存/读取正常）。
- [ ] **AC-FU-17:** `%APPDATA%\com.redtei.myshelltool\logs\myshelltool.log` 无 panic / state 解析失败相关日志。

---

## 4. 执行顺序建议

1. 在 ssh.rs 顶部添加 `use crate::AppState;`（或 `use crate::lib::AppState`，按可见性结构调整）。
2. lib.rs 先把 `AppState` 和 `ssh_sessions` 改 `pub`，cargo check 通过。
3. ssh.rs 25 个签名 + 25 个调用点一次性 find/replace（机械改动），cargo check。
4. 删除 lib.rs 的 `app.manage(ssh_mgr.clone())` + 注释 + Arc::ptr_eq 测试。
5. cargo check + cargo test。
6. 桌面客户端 E2E 验证（AC-FU-11 到 AC-FU-15）。

预计工作量：30-60 分钟（机械改动为主）。

---

## 5. 风险

- **R1：漏改一个调用点** — `state.lock().await` 漏掉 `.ssh_sessions.` 前缀，编译失败。**缓解**：cargo check 立即捕获，零运行期风险。
- **R2：辅助函数签名未同步** — `connect_authenticated` / `get_or_create_sftp` 的 `&State<'_, Arc<...>>` 漏改。**缓解**：AC-FU-1 grep 断言 0 匹配。
- **R3：测试库迁移** — 删除 `ssh_state_keys_point_to_same_arc` 后无回归测试。**缓解**：AC-FU-9 要求新增 `app_state_resolves_correctly` 简单测试。

---

## 6. 完成后处理

- [ ] 此 follow-up 文件移到 `.omc/plans/completed/`（或归档）。
- [ ] 在主 plan `ralplan-fix-ui-ssh.md` ADR Follow-ups 标记 #1 完成。
- [ ] ADR Follow-up #2（`ssh_sessions` → `sessions` 重命名 + 移到 ssh.rs）可单独开新 follow-up，不阻塞本文件关闭。
