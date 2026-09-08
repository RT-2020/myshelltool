# UI 精简与 MCP 加固 · 实施交接

> 2026-09-08 会话产出。三批改动全部经 execute-code 执行 + review-guard 独立验收通过。
> 设计依据：[UI设计token语料.md](./UI设计token语料.md)。

## 完成内容

### 批1 · UI 六区域精简 + SystemTime bug（~30 文件）
- 顶栏：删「未命名工作区」/同步图标/布局 Menu；「恢复默认布局」收进设置弹窗（payload 回调，照 autoUpdate 先例）
- 侧栏：删两条常驻提示；资产条目单行 32px（删第二行与状态文字标签，tooltip 保留）
- 标签/工具栏：标签=状态点+名称+关闭钮；工具栏 11→6（4 常驻+2 条件态），删连接状态 pill；字号功能保留（Ctrl+滚轮 + Ctrl+=/-）
- 文件区：删「文件传输」label 与拖拽常驻提示；view-pills 改「双栏|仅远程」；表格 6→4 列（删权限/用户组）
- 右栏：删 3 个恒「—」死行、MCP/同步重复行、CPU 假负载注脚、「2秒·60点」×3、重复启停按钮、footer meta
- 状态栏：删「SSH 空闲·后端 tauri-core」/UTF-8/「SSH 已连接」；传输胶囊改 ArrowUpDown+活动数；syncText badge 加 ellipsis
- bug：ssh.rs sftp_list_dir/sftp_stat 修改时间 `Ok(SystemTime{…})` → 复用 fs_local::format_modified（Unix 秒），顺带修复 SFTP 时间排序失效

### MCP · 拦截等级配置 + 执行日志（11 文件）
- 新 `mcp/config.rs`：McpInterceptLevel{minimal(默认)/strict}，存 `<data_dir>/mcp-config.json`
- 新 `mcp/execution_log.rs`：执行日志（tokio Mutex 串行 + .tmp/rename 原子写，30 天惰性清理 + 1000 条上限，best-effort 不阻断工具调用），存 `mcp-execution-log.json`
- approval.rs evaluate 加 level；测试参数化（minimal/strict 全矩阵）
- server.rs：ElicitOutcome 携带审批来源（elicitation/gui/timeout/rejected）；call_tool 记录 5 个真实远程执行工具（ssh_exec/disk_usage/system_status/service_status/sftp_remove）的完整日志（decision 9 值 / outcome 3 值 / 输出头尾各 250 字符）
- lib.rs 4 新命令（mcp_get_config/mcp_set_config/mcp_list_execution_logs/mcp_clear_execution_logs），实时生效无需重连
- 前端：McpInterceptionSettings（两档 AppSelect + Minimal 风险提示文案）、McpExecutionLogList（AppTable + 行展开详情 + 清空内联二次确认）；直接 useMcpStore 不经 workbench（避开 487 行贴上限）
- AGENTS.md §6/§7/§9 已同步

### 批2 · 栏间拖拽上传（3 文件）
- 本地栏行 draggable（自定义 MIME，选中集整组携带）→ 远程栏 drop → 复用 uploadLocalEntry（含同名覆盖确认）
- 栏级 overlay（accent-soft + dashed 内缩 8px +「松开：上传 N 项到 {remotePath}」）；与 OS 整面拖入通道互斥隔离
- 目录不支持如实降级（全目录 warn / 混合则跳过计数）；files.js 零改动（组件层实现）

## 验证记录（各批 review-guard 亲跑）
- npm run build exit 0（三批各跑）
- npm run test:ui 3/3（smoke + host-key + file-loading）
- cargo check / cargo check --tests exit 0
- 真实拖拽交互、MCP 真实会话审批未做运行时验证（自动化不可覆盖），建议 `npm run tauri:dev` 人工过一遍

## 遗留 P2（不阻断，后续小修）
1. MCP 日志 level 快照双读：server.rs 判定与日志各读一次配置，切换等级的毫秒窗口可能不一致（建议 check_approval_needed 返回判定所用 level）
2. 拖拽 symlink 被计入「已跳过 N 个目录」文案，措辞不准
3. 拖拽源行销毁且栏外释放时 columnDragging 理论可残留（概率极低）

## 提交建议（未提交，等用户指令）
按三个逻辑单元拆分：①批1 精简+bug（含 fs_local pub(crate)/lib.rs mod 行）②MCP 配置+日志（含 AGENTS.md）③批2 拖拽（FileColumnList/FileColumn/FileSurface）。
