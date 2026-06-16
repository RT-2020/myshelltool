# myshelltool

Windows SSH 运维客户端，Rust + Tauri 构建。

## 功能

- **连接资产管理** — 多级嵌套分组（`/` 分隔路径）、标签、筛选、收藏，本地 JSON 持久化；资产支持编辑/复制/删除，分组支持重命名/解散/新建
- **SSH 终端** — 多标签会话、xterm.js 前端、自动 fit、深色/浅色主题
- **Host Key 验证** — 首次连接指纹确认、变更高危警告、known_hosts 持久化
- **多认证方式** — 密码、私钥（ed25519/RSA）、passphrase、keyboard-interactive 降级
- **凭据安全存储** — 密码/私钥 passphrase 走本地 SecretStore，不出现在资产 JSON 或日志中
- **SFTP 文件管理** — 远程目录浏览、上传/下载、新建目录、重命名、删除（带确认弹窗）
- **Monaco Editor 远程编辑** — 语法高亮、Ctrl+S 保存、按扩展名自动识别语言
- **文件传输队列** — 64KB 分块传输、实时进度条、失败重试
- **SSH 隧道/端口转发** — Local forwarding、Dynamic SOCKS5 代理、创建/启动/停止/删除
- **终端搜索** — xterm.js search addon，查找上一个/下一个
- **终端全屏** — 一键全屏切换
- **同步/安全配置** — Git 配置同步、token 管理

## 技术栈

| 层 | 技术 |
|---|---|
| 桌面框架 | Tauri 2 |
| 后端 | Rust (russh 0.49, russh-sftp 2.x, tokio) |
| 前端 | Vue 3 (`<script setup>`) + Pinia (setup store) + SCSS 设计 token |
| 图标 | lucide-vue-next |
| 终端 | xterm.js 6 + addon-fit/search/web-links/webgl |
| 远程编辑 | Monaco Editor 0.52 (CDN) |
| 构建 | Vite 7（root=`src/`，`@`→`src` 别名） |
| 测试 | Playwright (UI smoke), cargo test (core) |

## 项目结构

```
myshelltool/
├── src/                          # 前端（Vite root）
│   ├── index.html                # 主页面（Vite 入口）
│   ├── main.js                   # 应用入口：createApp(App).use(createPinia()).mount()
│   ├── App.vue                   # 根组件：5 区域布局 + onMounted 调 store.initialize()
│   ├── components/
│   │   ├── shell/                # 外壳：AppShellLayout/TitleBar/StatusBar/
│   │   │                         #        ConnectionSidebar/AssetGroupNode/GlobalModals
│   │   ├── terminal/             # 终端：TerminalSurface/Tabs/Toolbar
│   │   ├── files/                # 文件：FileSurface/FileColumn
│   │   ├── resource-monitor/     # 资源监控：Cpu/Memory/Network/Disk 图表
│   │   └── ui/                   # 基础组件库：App*(Input/Button/Select/Modal/...)
│   ├── stores/                   # Pinia：workbench(编排) + sessions/assets/files/
│   │                             #        tunnels/ui/resourceMonitor(6 领域)
│   ├── composables/              # useTheme/useClipboard/useTerminalConfig/...
│   ├── lib/                      # terminalController/terminalThemes/dangerousCommands
│   ├── services/backend.js       # Tauri IPC 桥（invokeBackend/listenBackendEvent）
│   └── styles/                   # SCSS：_tokens(设计token)/_base/_utilities/main
├── src-tauri/                    # Tauri/Rust 后端
│   ├── Cargo.toml
│   ├── build.rs                  # 构建脚本（含 windres 容错）
│   └── src/
│       ├── lib.rs                # Tauri 命令注册 + AppState + 资产/凭据命令
│       ├── ssh.rs                # SSH/SFTP/隧道核心实现
│       ├── resource_monitor.rs   # 远程资源轮询
│       └── fs_local.rs           # 本地文件系统命令
├── crates/
│   └── myshelltool-core/         # 共享核心库（资产存储/凭据/校验，可独立 cargo test）
├── tests/
│   ├── ui-smoke.mjs              # UI 冒烟测试
│   └── ui-host-key.mjs           # Host key 验证流程测试
├── AGENTS.md                     # AI Coding Agent 上下文（单一信息源）
└── package.json
```

## 开发

```bash
# 安装前端依赖
npm install

# 浏览器预览模式（无 SSH 功能）
npm run dev

# Tauri 开发模式（完整功能）
npm run tauri:dev

# 构建
npm run tauri:build

# 测试
npm run test:core    # Rust core 单元测试
npm run test:ui      # UI 冒烟测试
```

## AI 协作上下文

本项目为 AI Coding Agent 提供了完整项目上下文文件 [`AGENTS.md`](./AGENTS.md)（技术栈、目录结构、编码约定、构建/测试命令、IPC 契约、数据模型、安全红线）。使用 Claude Code / Cursor / Copilot 等工具开发时，请先读 `AGENTS.md`。工具专用派生文件（`CLAUDE.md`、`.github/copilot-instructions.md`、`.cursor/rules/*`）均引用该文件。

## 安全设计

- 凭据（密码、私钥、passphrase）只走本地 SecretStore，不在资产 JSON、日志或代码中出现
- 危险文件操作（删除、覆盖）必须弹窗确认
- Host key 变更默认阻止连接并警告
- 远程编辑保存前检测远端文件变化（计划中）
- 传输队列保留操作日志

## License

MIT

---

## 架构决策（ADR v3 APPROVED）

详见 `.omc/plans/framework-choice-tauri-vs-qt-vs-electron.md`（v3 Critic 三审 APPROVE）。

**决策**：继续使用 Tauri 2.x，不切换 Qt 或 Electron。

**核心理由**：
1. 当前痛点（`State<T>` 陷阱、IPC OOM、浏览器预览双实现）均为应用层问题，与框架无关
2. russh 是 Rust SSH 第一梯队，保留资产
3. 切框架 = 切栈 + 修同样的痛点，无净收益

**已修复**：
- ✅ Option A 重构（统一 `State<'_, AppState>`，删除双 manage hack）
- ✅ IPC OOM（chunked upload：sftp_upload_start/chunk/finalize，8MB 分块）
- ✅ 删除 invokeBrowserPreview 假后端（100+ 行）+ fallbackAssets 设计稿残留
- ✅ Tauri command 参数 camelCase 修复（ssh_connect、tunnel_*）
- ✅ Host key 验证流程：IIFE 立即监听 + 65s 自动清理 + 字段名 snake/camel 一致
- ✅ keyboard-interactive 自动响应（debian 等服务器禁用 PasswordAuth 的 fallback）
- ✅ ConnectionAsset 加 credential_id/passphrase_credential_id 字段，向后兼容

## 后续 Follow-ups

- `sftp_download_with_progress` 仍返回 `Vec<u8>` 整块（upload 已分块，download 同样改造作为独立项）
- 隧道转发无认证（默认 127.0.0.1，但允许 0.0.0.0 时缺安全警告）
- known_hosts 不兼容 OpenSSH 格式、无通配符、无原子写入
- AC-6 mock SSH server 测试覆盖 connect/upload/download/tunnel 4 个命令（CI 集成）

## 已知小问题

- `sanitize_credential_id` 过滤 `:` 和 `.`：`192.168.2.2:password` → `192-168-2-2password`，能正常读写但不直观，cleanup 项
- `start_remote_forward` 是返回 Err 的桩函数（local/dynamic SOCKS5 已实现）
- `metadata.modified()` 在 `ssh.rs:713` 仍是 `SystemTime` Debug 打印（UX 优化项）
