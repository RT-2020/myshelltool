# 下一阶段实施计划：SFTP 文件管理 + 隧道代理 + 终端增强

## 前置状态

- Topology 1 (连接资产管理)：COMPLETE
- Topology 2 (SSH 终端基础)：phase-1-partial — G001~G004 已完成
- 当前远程文件浏览：通过 SSH exec `find` 命令实现，仅目录列表，无上传下载
- 代码已提交：commit 1e579f5

## 开源库选型

| 用途 | 库 | 说明 |
|------|-----|------|
| SFTP 子系统 | `russh-sftp` (Rust) | 纯 Rust，与 russh 无缝集成，支持 SFTP v3 |
| 远程编辑器 | `monaco-editor` (npm) | VS Code 同款编辑器，CDN 加载，支持语法高亮/差异对比 |
| 文件拖拽 | 原生 HTML5 Drag & Drop API | 无需额外库，vanilla JS 即可 |
| 进度条 | CSS + Tauri events | 自定义，无额外依赖 |
| 路径操作 | `std::path` (Rust) + 前端字符串处理 | 无额外依赖 |

## 实施阶段

### Phase A — SFTP 后端 (Rust)

**目标**：用 `russh-sftp` 替换 SSH exec `find`，实现完整的 SFTP 子系统操作。

**新增依赖** (`src-tauri/Cargo.toml`)：
```
russh-sftp = "2"
```

**改动文件**：

1. `src-tauri/src/ssh.rs` — 新增 SFTP 功能：
   - 复用已有 SSH session handle，通过 `channel_open_session` + `request_subsystem("sftp")` 打开 SFTP 通道
   - 新增 `SftpSession` 结构体，持有 `russh_sftp::client::SftpSession`
   - 将 SFTP session 关联到 `SshSessionManager.sessions`（复用已有 session ID）
   - 新增 Tauri commands：
     - `sftp_list_dir(session_id, path)` → 目录列表（替换现有 `ssh_list_directory`）
     - `sftp_stat(session_id, path)` → 文件元信息
     - `sftp_mkdir(session_id, path)` → 创建目录
     - `sftp_rename(session_id, old, new)` → 重命名
     - `sftp_remove(session_id, path, kind)` → 删除文件/目录
     - `sftp_read_file(session_id, path)` → 读取小文件内容（用于远程编辑）
     - `sftp_write_file(session_id, path, content)` → 写入小文件内容
     - `sftp_download(session_id, remote_path, local_path)` → 下载大文件（带进度事件）
     - `sftp_upload(session_id, local_path, remote_path)` → 上传大文件（带进度事件）

2. `src-tauri/src/lib.rs` — 注册新 commands

3. `crates/myshelltool-core/src/lib.rs` — 新增 `RemoteFileTransfer` 类型（传输队列项）

**关键设计决策**：
- SFTP session 复用已有 SSH 连接，不需要单独认证
- 大文件传输通过 Tauri events 推送进度（`sftp-progress-{transferId}`）
- 小文件读写用内存缓冲，大文件用流式 chunk

**验收标准**：
- `cargo check` 通过
- SFTP list/read/write/mkdir/rename/remove 命令可编译
- 现有 SSH 终端功能不受影响

---

### Phase B — 文件管理器 UI (JS)

**目标**：升级文件面板为功能完整的远程文件管理器。

**改动文件**：

1. `src/main.js` — 文件管理器核心逻辑：
   - 重写 `refreshRemoteFiles()` → 调用 `sftp_list_dir` 替代 `ssh_list_directory`
   - 新增 `renderFileTree(entries)` — 渲染目录/文件树（图标 + 名称 + 大小 + 时间）
   - 新增 `navigateToRemoteDir(path)` — 路径导航（面包屑 + 点击跳转）
   - 新增 `createRemoteDir()` — 创建目录弹窗
   - 新增 `renameRemoteFile(path)` — 重命名弹窗
   - 新增 `deleteRemoteFile(path, kind)` — 删除确认弹窗（危险操作）
   - 新增文件拖拽上传（HTML5 Drag & Drop）
   - 新增本地文件选择上传（`<input type="file">`）
   - 新增右键上下文菜单（文件/目录操作）

2. `src/index.html` — 文件面板 UI 调整：
   - 远程面板增加面包屑路径导航
   - 增加"本地文件"拖放区域
   - 文件行增加操作按钮（编辑/下载/重命名/删除）

3. `src/styles.css` — 文件管理器样式：
   - 面包屑路径条
   - 文件图标（目录/文件/链接/代码文件等）
   - 拖拽高亮
   - 传输队列行样式

**浏览器 fallback**：模拟文件列表，提示"文件传输需要桌面客户端"

**验收标准**：
- 双击目录可进入，面包屑可回溯
- 能创建目录、重命名、删除（带确认）
- 拖拽文件到面板触发上传
- 文件行显示图标、名称、大小、修改时间

---

### Phase C — 传输队列与进度

**目标**：实现完整的文件传输管理，包括队列、进度、冲突处理。

**改动文件**：

1. `src/main.js` — 传输管理：
   - 新增 `TransferQueue` 类 — 管理上传/下载队列
   - 新增 `startTransfer(type, localPath, remotePath)` — 启动传输
   - 监听 `sftp-progress-{transferId}` 事件更新进度条
   - 传输完成/失败回调
   - 冲突检测：远程目标已存在时弹窗（覆盖/跳过/重命名）
   - 失败任务可重试
   - 传输队列 UI 渲染

2. `src-tauri/src/ssh.rs` — 进度事件：
   - 大文件读写使用 chunk 循环，每 chunk 发送进度事件
   - 事件格式：`{ transferId, bytesTransferred, totalBytes, speed }`

3. `src/index.html` — 传输队列区域：
   - 激活底部的传输队列面板
   - 进度条、速度、剩余时间

**验收标准**：
- 上传/下载显示实时进度
- 队列中多个传输按序执行
- 目标冲突弹窗确认
- 失败任务可重试

---

### Phase D — 远程编辑 (Monaco Editor)

**目标**：集成 Monaco Editor 实现远程文件在线编辑。

**改动文件**：

1. CDN 加载 Monaco Editor（`index.html` 或动态加载）：
   - 从 CDN `cdn.jsdelivr.net/npm/monaco-editor` 加载
   - 或本地 bundle（推荐 CDN，减少构建复杂度）

2. `src/main.js` — 编辑器集成：
   - 新增 `openRemoteEditor(filePath, content, language)` — 打开编辑器标签页
   - 编辑器保存：读取编辑器内容 → `sftp_write_file` 写回远程
   - 保存前冲突检测：`sftp_stat` 检查远端 mtime 是否变化
   - 差异查看器：Monaco diff editor 展示本地 vs 远端差异
   - 语言检测：根据文件扩展名自动选择语法高亮

3. `src/index.html` — 编辑器容器：
   - 编辑器标签页（动态 tab）
   - 编辑器工具栏（保存/差异/语言切换）

4. `src/styles.css` — 编辑器全屏样式

**验收标准**：
- 双击远程代码文件打开 Monaco 编辑器
- 语法高亮正确（至少 TOML/JS/Python/YAML/Shell）
- 保存前检测远端变化，冲突时提示
- 差异查看器可用

---

### Phase E — 隧道/端口转发后端 (Rust)

**目标**：实现 Local、Remote、Dynamic SOCKS 端口转发。

**新增依赖** (`src-tauri/Cargo.toml`)：
```
# russh 已包含 port forwarding API，无需额外 crate
# SOCKS5 需要：
tokio-socks = "0.5"  # 可选，用于 SOCKS5 代理实现
```

**改动文件**：

1. `src-tauri/src/ssh.rs` — 隧道功能：
   - 新增 `TunnelConfig` 结构体：{ id, name, kind(local/remote/dynamic), local_addr, local_port, remote_addr, remote_port, connection_asset_id, auto_start }
   - 新增 `TunnelManager` — 管理活跃隧道
   - 复用已有 SSH session 进行 port forwarding
   - Local forward：`channel_open_direct_tcpip`
   - Remote forward：`channel_open_forwarded_tcpip` + `tcpip_forward`
   - Dynamic SOCKS：本地 SOCKS5 服务器 + `channel_open_direct_tcpip`
   - 新增 Tauri commands：
     - `tunnel_create(config)` → 创建隧道
     - `tunnel_start(tunnel_id)` → 启动
     - `tunnel_stop(tunnel_id)` → 停止
     - `tunnel_list()` → 列出所有隧道
     - `tunnel_status(tunnel_id)` → 健康状态

2. `src-tauri/src/lib.rs` — 注册隧道 commands

3. `crates/myshelltool-core/src/lib.rs` — `TunnelConfig` 序列化类型

**验收标准**：
- Local forward 可将远程端口映射到本地
- Remote forward 可将本地端口映射到远程
- Dynamic SOCKS 可作为本地代理使用
- 隧道启动/停止状态正确
- 端口占用检测

---

### Phase F — 隧道管理 UI

**目标**：激活隧道管理面板，实现完整的隧道 CRUD 和监控。

**改动文件**：

1. `src/main.js` — 隧道管理：
   - `renderTunnelList()` — 渲染隧道表格（替换静态 HTML）
   - `createTunnel()` — 新建隧道弹窗（选择类型、配置地址/端口）
   - `toggleTunnel(tunnelId)` — 启动/停止
   - `deleteTunnel(tunnelId)` — 删除确认
   - `checkTunnelHealth(tunnelId)` — 健康检查（TCP 连通性测试）
   - 监听 `tunnel-status-{tunnelId}` 事件更新状态

2. `src/index.html` — 隧道面板：
   - 动态渲染隧道表格（替换当前静态 demo 数据）
   - 新增隧道表单弹窗

3. `src/styles.css` — 隧道管理样式优化

**验收标准**：
- 隧道表格从后端数据渲染
- 新建/启动/停止/删除隧道可用
- 状态实时更新（Running/Stopped/Error/Reconnecting）
- 健康检查结果可查看

---

### Phase G — 终端增强

**目标**：完善 SSH 终端的高级功能。

**改动文件**：

1. `src-tauri/src/ssh.rs` — 认证增强：
   - `keyboard-interactive` auth handler 实现（`russh::client::Handler:: auth_keyboard_interactive`）
   - 前端弹窗收集 keyboard-interactive 响应

2. `src/main.js` — 终端 UI：
   - 终端搜索面板（xterm.js search addon）
   - 全屏/分屏切换
   - 终端设置面板（字体/编码/滚屏缓冲）
   - 连接断线自动重连提示

**验收标准**：
- keyboard-interactive 认证可用
- 终端搜索功能可用
- 全屏模式可用

---

## 执行顺序与依赖

```
Phase A (SFTP 后端)
  ↓
Phase B (文件管理 UI) ← A
  ↓
Phase C (传输队列) ← A + B
  ↓
Phase D (远程编辑) ← A + B
  ↓
Phase E (隧道后端) ← 独立，可与 B/C/D 并行
  ↓
Phase F (隧道 UI) ← E
  ↓
Phase G (终端增强) ← 独立，可与 E/F 并行
```

## 安全约束（继承 deep-interview 规格）

- 凭据（密码、私钥、passphrase）只走 SecretStore
- 不在资产 JSON、日志、代码中写入任何明文凭据
- 危险文件操作（删除、覆盖）必须弹窗确认
- 远程编辑保存前检测远端文件变化
- host key 变更继续阻止并警告
- 传输队列保留操作日志

## 开源库集成注意事项

- **russh-sftp**：需确认与当前 russh 0.49 的版本兼容性；如不兼容则使用 SFTP 协议手工实现（基于 russh channel）
- **monaco-editor**：CDN 加载，不修改构建流程；检测网络不可用时回退为 `<textarea>` 编辑
- 无需 React/Vue 等框架，所有 UI 继续用 vanilla JS + Tauri events
