# myshelltool

Windows SSH 运维客户端，Rust + Tauri 构建。

## 功能

- **连接资产管理** — 分组、标签、筛选、收藏，本地 JSON 持久化
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
| 前端 | Vanilla JS + CSS，无框架 |
| 终端 | xterm.js 6 + addon-fit + addon-search |
| 远程编辑 | Monaco Editor 0.52 (CDN) |
| 构建 | Vite 7 |
| 测试 | Playwright (UI smoke), cargo test (core) |

## 项目结构

```
myshelltool/
├── src/                          # 前端
│   ├── index.html                # 主页面
│   ├── main.js                   # 应用逻辑
│   └── styles.css                # 样式
├── src-tauri/                    # Tauri/Rust 后端
│   ├── Cargo.toml
│   ├── build.rs                  # 构建脚本（含 windres 容错）
│   └── src/
│       ├── lib.rs                # Tauri 命令注册
│       └── ssh.rs                # SSH/SFTP/隧道核心实现
├── crates/
│   └── myshelltool-core/         # 共享核心库（资产存储、凭据、同步）
├── tests/
│   └── ui-smoke.mjs              # UI 冒烟测试
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

## 安全设计

- 凭据（密码、私钥、passphrase）只走本地 SecretStore，不在资产 JSON、日志或代码中出现
- 危险文件操作（删除、覆盖）必须弹窗确认
- Host key 变更默认阻止连接并警告
- 远程编辑保存前检测远端文件变化（计划中）
- 传输队列保留操作日志

## License

MIT
