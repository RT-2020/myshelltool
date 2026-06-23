# Architecture Log

> Cross-session memory for architectural drift. AI has no memory across sessions — this file is the sole persistent carrier.
> Read at session start to detect accumulated drift; append one row per session that touched code.
> Maintained by: `vibe-guard-pure` skill.

## How to use

- **At session start (when working on code)**: skim this log. If any file has grown by ≥0.5× of its limit since the last reset row, that's a downstream trigger — propose a pure architecture session.
- **At session end (if code changed)**: append one row to the table below.
- **On a pure architecture session**: after restructuring, add a `RESET` row recording the new baseline.

## Size limits (from AGENTS.md §质量红线)

| Type | Soft warn | Hard limit |
|---|---|---|
| Vue SFC `.vue` | 300 | **500** |
| Pinia store `.js` | 300 | **500** |
| Rust module `.rs` | 400 | **800** |

## Drift log

| Date | Files touched (current size, delta) | Suspected duplicates | Tier | Outcome |
|---|---|---|---|---|
| 2026-06-20 | baseline snapshot (see below) | — | — | log initialized post v0.3.0 |
| 2026-06-22 | re-baseline via `wc -l` (see below) | — | — | **RESET** — skill re-init, 6 files still over hard limit (down from 8) |
| 2026-06-22 | `FileColumn.vue` 780 (−40 from 820实测), `files.js` 664 (+51), `workbench.js` 431 (+36), `ui.js` 344 (+1); 删 `AppBreadcrumb.vue` + barrel 行 | none (manualLocal* 对称复制 manualRemote*, Gate A/B pass 不抽公共) | medium | passed — `npm run build` exit 0 (12.66s). FileColumn 远离硬上限；files/workbench 仍 1.33×/0.86×，无新热点 |
| 2026-06-22 | `TerminalSurface.vue` 323 (+13, 进入 soft-warn), `TerminalPane.vue` 217 (+9), `useClipboard.js` 68 (+16), `workbench.js` 432 (+1 vs 上一行); 加 `tauri-plugin-clipboard-manager` (Cargo+capabilities+lib.rs); `useTerminalConfig.js` 删 rightClickSelectsWord | none (右键菜单纯接线复用 copyTerminalSelection/pasteToTerminal/handlePasteWithGuard, Gate 不触发) | medium | passed — `npm run build` exit 0 (3.80s) + `cargo check` exit 0 (1m42s, 清 42 个僵尸 MCP 进程后) + `test:core` 41/41 pass |
| 2026-06-23 | 文档同步重写（v1.4 落地）：README/mcp-setup/AGENTS/architecture-log。**只改文档 + 版本号 bump 0.3.0→0.4.0，零代码逻辑改动**。实测发现 `FileColumn.vue` 768→1123（+355, **cycle-tier 触发**, 1×→2.25×），`ssh.rs` 1874→2097。清 2 个 v1.2 遗留 `myshelltool-mcp.exe` 僵尸进程（锁 WebView2Loader.dll 致 os error 32）。`cargo check` exit 0 (17.50s) | none | low | passed — 见下方 RESET 基线重置。**下次纯架构会话：拆 FileColumn.vue + ssh.rs（cycle-tier 双触发）** |

## Baseline snapshot (2026-06-23, RESET — measured via `wc -l` via PowerShell `Get-Content.Count`)

Files currently over hard limit — refactor candidates (real numbers, not estimated):

| File | Lines | Limit | Multiple | Note |
|---|---|---|---|---|
| `src-tauri/src/ssh.rs` | 2097 | 800 | 2.62× | 4 domains fused: session/terminal, SFTP, tunnel (local+remote+SOCKS5), headless. **cycle-tier** (1874→2097 since 06-22 RESET). Highest priority to split. |
| `src/components/files/FileColumn.vue` | 1123 | 500 | 2.25× | **cycle-tier 触发**（768→1123, +355, 1×→2.25× 自 06-22 RESET）。v1.4「文件管理区精简重构」commit 8fa14a0 后反而暴涨。最高优先级拆分。 |
| `src/stores/sessions.js` | 858 | 500 | 1.72× | terminal lifecycle + session mgmt |
| `src/components/shell/ConnectionSidebar.vue` | 711 | 500 | 1.42× | tree + menu + drag mixed |
| `src/components/shell/GlobalModals.vue` | 694 | 500 | 1.39× | all modals centralized |
| `src/stores/files.js` | 688 | 500 | 1.38× | SFTP + transfer queue |

### Δ vs previous baseline (2026-06-20)

All previously-over-limit hotspots shrank; 2 dropped below hard limit:

| File | 2026-06-20 | 2026-06-22 | Δ | Status |
|---|---|---|---|---|
| `src-tauri/src/ssh.rs` | 2055 | 1874 | −181 | still over (2.57× → 2.34×) |
| `src/stores/sessions.js` | 888 | 839 | −49 | still over (1.78× → 1.68×) |
| `crates/myshelltool-core/src/lib.rs` | 891 | 796 | −95 | **below limit now** (was 1.11×, now 0.99×) |
| `src-tauri/src/resource_monitor.rs` | 875 | 792 | −83 | **below limit now** (was 1.09×, now 0.99×) |
| `src/components/files/FileColumn.vue` | 820 | 768 | −52 | still over (1.64× → 1.54×) |
| `src/components/shell/GlobalModals.vue` | 729 | 693 | −36 | still over (1.46× → 1.39×) |
| `src/components/shell/ConnectionSidebar.vue` | 711 | 651 | −60 | still over (1.42× → 1.30×) |
| `src/stores/files.js` | 661 | 613 | −48 | still over (1.32× → 1.23×) |

### Δ vs previous baseline (2026-06-22 → 2026-06-23)

⚠️ **两处 cycle-tier 触发**（单文件自上次 RESET 跨越增长阈值）：

| File | 2026-06-22 | 2026-06-23 | Δ | Status |
|---|---|---|---|---|
| `src-tauri/src/ssh.rs` | 1874 | 2097 | **+223** | still over (2.34× → 2.62×), **cycle-tier** |
| `src/components/files/FileColumn.vue` | 768 | 1123 | **+355** | still over (1.54× → **2.25×**), **cycle-tier 严重** |
| `src/stores/sessions.js` | 839 | 858 | +19 | still over (1.68× → 1.72×) |
| `src/stores/files.js` | 613 | 688 | +75 | still over (1.23× → 1.38×) |
| `src/components/shell/ConnectionSidebar.vue` | 651 | 711 | +60 | still over (1.30× → 1.42×) |
| `src/components/shell/GlobalModals.vue` | 693 | 694 | +1 | still over (1.39× → 1.39×, 持平) |

> **FileColumn.vue 警报**：v1.4 commit `8fa14a0`（feat: 文件管理区精简重构 + MCP 内嵌 HTTP 收尾）名义是「精简重构」，但 FileColumn.vue 从 768 暴涨到 1123（+355，几乎翻倍）。这是典型的"重构名义下堆叠"——下次会话优先做纯架构拆分，而非继续往里加功能。建议拆分边界：`FileColumnHeader` / `FileColumnBody`（列表）/ `FileColumnContextMenu` / `FileColumnDropZone`（拖拽上传）。

### v1.4 MCP 内嵌 GUI 重构（2026-06-23，架构级变更）

重大架构重构：MCP 从「双二进制 + stdio + named pipe 桥」改为「内嵌 GUI + Streamable HTTP transport」。净文件变化：

| 操作 | 文件 | 说明 |
|---|---|---|
| **删除** | `src-tauri/src/mcp/pipe.rs` (592 行) | named pipe 桥，内嵌后无需（从 watch list 消失） |
| **删除** | `src-tauri/src/bin/mcp.rs` (19 行) | 独立 console bin 入口，取消双二进制 |
| **删除** | `scripts/mcp-dev-watch.mjs` | 独立 bin 的 dev watch 脚本，已无目标 |
| **新增** | `src-tauri/src/mcp/http_server.rs` (~110 行) | axum + rmcp Streamable HTTP server |
| **重写** | `src-tauri/src/mcp/probe.rs` (318→~190 行) | spawn 探测 → HTTP 健康检查 |
| **瘦身** | `src-tauri/src/mcp/server.rs` (379→~340 行) | 删 serve_stdio + pipe 审批降级 |
| **瘦身** | `src-tauri/src/mcp/tools.rs` (427→~360 行) | 删 pipe 复用分支，exec_on_asset 直走 headless |
| **瘦身** | `src-tauri/src/lib.rs` (529→~470 行) | 删 run_mcp_stdio/init_mcp_logger/pipe 启动 |
| **清理** | `src/stores/sessions.js` / `workbench.js` / `GlobalModals.vue` | 删 mcpApproval modal 死代码（v1.1 pipe 审批） |

**架构收益**：单 exe 单安装包（根治 NSIS 打包缺口）、根治僵尸进程/os error 32、消除 pipe 桥复杂度。**Follow-up**：会话复用（注入 SshSessionManager）、GUI 弹窗审批（注入 AppHandle）。

### Known drift from AGENTS.md §0.5 (manual list, still stale)

AGENTS.md §0.5 records `ssh.rs(1548), sessions.js(810), FileColumn.vue(768), files.js(612), GlobalModals.vue(512)` — actual ssh.rs is 1874 (off by +326), GlobalModals.vue is 693 (off by +181). The hand-maintained list rotted again. This log remains the source of truth; AGENTS.md §0.5 list should be regenerated from here or removed.

### Soft-warn zone (300–500 for .vue/.js, 400–800 for .rs) — watch list

Not over the hard limit, but approaching — track so they don't silently cross:

| File | Lines | Limit type | Soft% |
|---|---|---|---|
| `src-tauri/src/sync.rs` | 585 | .rs (soft 400 / hard 800) | 73% of hard |
| `crates/myshelltool-core/src/lib.rs` | 891 | .rs (soft 400 / hard 800) | ⚠ **111% — 已超 hard limit**（06-22 记 796 已漂移，实测回升；应移入上方硬上限表，下次 RESET 整理） |
| `src-tauri/src/resource_monitor.rs` | 875 | .rs (soft 400 / hard 800) | ⚠ **109% — 已超 hard limit**（同上，06-22 记 792 已漂移） |
| `src/stores/workbench.js` | 432 | store .js (soft 300 / hard 500) | 86% of hard |
| `src/components/shell/McpPanelContent.vue` | 374 | .vue (soft 300 / hard 500) | 75% of hard |
| `src-tauri/src/dangerous_commands.rs` | 402 | .rs (soft 400 / hard 800) | 50% of hard（进入 soft-warn） |
| `src/components/shell/SyncPanelContent.vue` | 358 | .vue (soft 300 / hard 500) | 72% of hard |
| `src-tauri/src/mcp/tools.rs` | 368 | .rs (soft 400 / hard 800) | 46% of hard |
| `src/stores/ui.js` | 344 | store .js (soft 300 / hard 500) | 69% of hard |
| `src/components/terminal/TerminalTabs.vue` | 338 | .vue (soft 300 / hard 500) | 68% of hard |
| `src/components/shell/AppTitleBar.vue` | 334 | .vue (soft 300 / hard 500) | 67% of hard（新进入） |
| `src/components/shell/AssetGroupNode.vue` | 324 | .vue (soft 300 / hard 500) | 65% of hard（新进入） |
| `src/components/terminal/TerminalSurface.vue` | 323 | .vue (soft 300 / hard 500) | 65% of hard |
| `src/stores/assets.js` | 323 | store .js (soft 300 / hard 500) | 65% of hard（新进入） |
| `src-tauri/src/mcp/server.rs` | 339 | .rs (soft 400 / hard 800) | 42% of hard |

> 已移除：`mcp/pipe.rs`（v1.4 已删）、`lib.rs`/`resource_monitor.rs` 实测已超 hard limit（标 ⚠ 待下次 RESET 移入主表）。

## Next architecture session (proposed)

**Target 1**: split `src-tauri/src/ssh.rs` (2097, 2.62× → ~5 modules of 300-500 each). Natural boundaries already marked by `// ---` comment dividers in the file:
- `ssh/session.rs` — connect/auth/host-key/keyboard-interactive
- `ssh/sftp.rs` — all `sftp_*` commands + chunked upload
- `ssh/tunnel.rs` — `tunnel_*` + local/remote/dynamic-forward + SOCKS5
- `ssh/headless.rs` — `HeadlessSshClient` + `connect_headless` + `exec_command_once`
- `ssh/known_hosts.rs` — host-key verification helpers

Mechanical refactor: move code, adjust `pub`, re-export from `ssh/mod.rs`. Zero logic change. Frontend fully insulated (commands keep same names).

**Target 2（cycle-tier 严重，优先级 = Target 1）**: split `src/components/files/FileColumn.vue` (1123, 2.25×). 候选拆分边界（需先读文件确认职责分布）：
- `FileColumnHeader.vue` — 路径栏 / 排序 / 视图切换
- `FileColumnBody.vue` — 列表渲染（表格/图标双视图）
- `FileColumnContextMenu.vue` — 右键菜单项 + actions
- `FileColumnDropZone.vue` — 拖拽上传处理

> ⚠️ FileColumn.vue 在 v1.4「精简重构」commit 后从 768 暴涨到 1123，是当前最危险的堆叠点。拆分前需 vibe-guard Gate A/B 判定（确认是单组件职责过载，而非该有独立层）。

**Secondary target (after ssh.rs lands)**: `src/stores/sessions.js` (839, 1.68×) — split terminal lifecycle from session management.
