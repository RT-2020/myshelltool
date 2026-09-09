## v0.7.3

_区间: 仓库起点 → HEAD_

### ✨ 新功能
- **ui**: 私钥连接体验增强——文件选择器、凭据清除、认证失败引导
- **ui**: 全量 UI/UX 交互体验优化（审计问题批量修复）
- **terminal**: 支持同一资产打开多个会话并区分重名标签页
- **skill**: 发版技能补充两个自动化脚本
- **ci+updater**: 发版自动发布链路 + 应用内自动更新
- **sync**: Gist 资产同步功能完善（冲突解决/自动同步/PAT 引导）
- **mcp**: v1.5 高危工具 GUI 弹窗审批降级（补 v1.4 follow-up）
- **files+terminal**: 文件管理区精简重构 + MCP 内嵌 HTTP 收尾
- **sync**: PR-5/6: Gist 同步前端（store + 管理面板 + 冲突框 + 状态栏真实化）
- **sync**: PR-4: Gist 同步命令层 + 清理 v1.0 死代码
- **sync**: PR-3: Gist 同步引擎（载荷结构 + 加解密封装 + 冲突检测）
- **sync**: PR-2: Gist 同步加密内核（Argon2 + AES-256-GCM）
- **sync**: PR-1: SecretStore 从弱 XOR 升级到 Windows DPAPI
- **mcp**: GUI 回弹窗审批降级（elicitation→pipe→fail-secure 三级）
- **log**: 日志加日期时间前缀（chrono）
- **mcp**: M8 会话复用 named pipe（Part B，MCP 复用 GUI 已建立的 SSH 会话）
- **mcp**: M7 elicitation 审批（Part A，高危命令客户端内确认）
- **mcp**: M6 降级 + 打包文档（Layer 7 + 8，v1.0 收尾）
- **mcp**: M5 Resources + Prompts（Layer 4 + 5，三原语齐全）
- **mcp**: M4 审批与高危工具（Layer 6 v1.0 + Layer 3 高危工具）
- **mcp**: M3 只读 Tools 打通（Layer 3，首个可演示成果）
- **mcp**: M2 MCP 骨架通 stdio（Layer 2）
- **mcp**: M1 地基与共享核心（Layer 0 + Layer 1）

### 🐛 修复
- **settings**: 修复检查更新响应式解包/无反馈/无法重启问题，并优化设置面板各 tab 交互
- **sync**: 补充 maybeAutoPush helper 定义与 workbench bridge 注入，修复资产保存时 'maybeAutoPush is not defined' 报错
- **ui**: 状态栏更新提示接线——available/error 态可点击，设置图标加新版本徽标
- **ssh**: RSA 私钥认证改用 rsa-sha2-256 签名，修复对现代服务器必然失败
- **release**: 修正字段名 createUpdaterArtifacts（之前误写 createUpdateArtifacts）
- **release**: bundle.createUpdateArtifacts=true，否则 Tauri 不生成 .sig/latest.json
- **build**: reqwest 切 native-tls，绕开 mingw gcc 无法编译 ring 的问题
- **sync**: deslop: 修 masking fallback + AGENTS 红线 + 架构简化
- **mcp**: Codex 伪 elicitation 兼容（UserDeclined/Cancelled 降级走 pipe）
- **mcp**: 修复会话复用 pipe client 双连接 bug + spawn context
- **mcp**: 数据目录路径 + 一键开发脚本
- **mcp**: default-run + rmcp 废弃别名清理
- terminal invisible: load xterm.css via JS import (was broken <link>)
- windres icon path for non-ASCII project directories

### 📚 文档
- **v0.4.0**: 文档同步 v1.4 内嵌 HTTP 架构 + 版本号 bump
- **readme**: 补 Gist 同步使用说明 + 修正 DPAPI/同步已实现的过时描述
- 新增 MIT LICENSE + CLAUDE.md 禁止 Claude 进 Contributors 红线
- 修正 mcp-setup.md 数据目录路径笔误
- 新增下一轮迭代交接文档 + 标记 v1.0 实施计划已完成
- 重写 README，覆盖 v0.2.0 MCP server 完整使用说明
- 新增 MCP 服务接入三件套（需求规格/访谈记录/实施计划）

### 🔧 杂项
- **skill**: 沉淀 myshelltool 发版流程为项目内技能 release-myshelltool
- bump v0.5.0
- **gitignore**: 忽略 Windows 保留设备名文件（NUL/CON/PRN 等）
- trigger GitHub contributor cache refresh
- bump 0.2.0 → 0.3.0 + README 更新 v1.1 能力（elicitation 审批 + 会话复用）
- bump version 0.1.0 → 0.2.0

### 📦 其他
- v0.7.3
- v0.7.2
- v0.7.1
- v0.7.0
- v0.6.2
- slow remote file reads in UI tests
- FileColumn below the SFC limit
- shrinking the file column surface
- slow remote file loading feedback
- file column risk before deeper cleanup
- reducing FileColumn display helper weight
- release bumps against version drift
- v0.6.1 after npm build hook alignment
- release builds by using npm for Tauri hooks
- the core lockfile aligned with v0.6.0
- v0.6.0 from the integrated workbench snapshot
- the workbench release-ready for slower remote operations
- v1.2: 无状态就绪探测 + 管理面板
- /group management, resizable layout, resource monitor fixes
- LLM engineering assets + fix stale README architecture
- SSH connect pipeline + ADR v3 architecture cleanup
- project README with features, tech stack, and development guide
- file transfer queue with progress tracking
- Monaco editor, tunnel management, and terminal enhancements
- file manager UI with SFTP integration
- SFTP backend via russh-sftp and fix russh API compatibility
- SSH auth, multi-tab sessions, and host key verification
- SSH terminal backend and xterm.js frontend
- local secure credential storage abstraction
- local connection asset persistence
- Tauri-ready myshelltool baseline
