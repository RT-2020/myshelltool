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

## Baseline snapshot (2026-06-22, RESET — measured via `wc -l`)

Files currently over hard limit — refactor candidates (real numbers, not estimated):

| File | Lines | Limit | Multiple | Note |
|---|---|---|---|---|
| `src-tauri/src/ssh.rs` | 1874 | 800 | 2.34× | 4 domains fused: session/terminal, SFTP, tunnel (local+remote+SOCKS5), headless. Highest priority to split. |
| `src/stores/sessions.js` | 839 | 500 | 1.68× | terminal lifecycle + session mgmt |
| `src/components/files/FileColumn.vue` | 768 | 500 | 1.54× | single-column component carrying too many duties |
| `src/components/shell/GlobalModals.vue` | 693 | 500 | 1.39× | all modals centralized |
| `src/components/shell/ConnectionSidebar.vue` | 651 | 500 | 1.30× | tree + menu + drag mixed |
| `src/stores/files.js` | 613 | 500 | 1.23× | SFTP + transfer queue |

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
| `src-tauri/src/mcp/pipe.rs` | 592 | .rs (soft 400 / hard 800) | 74% of hard |
| `src-tauri/src/sync.rs` | 527 | .rs (soft 400 / hard 800) | 66% of hard |
| `src-tauri/src/lib.rs` | 491 | .rs (soft 400 / hard 800) | 61% of hard |
| `src-tauri/src/mcp/tools.rs` | 396 | .rs (soft 400 / hard 800) | 50% of hard |
| `src/stores/workbench.js` | 395 | store .js (soft 300 / hard 500) | 79% of hard |
| `src/components/files/TransferDrawer.vue` | 378 | .vue (soft 300 / hard 500) | 76% of hard |
| `src/components/shell/McpPanelContent.vue` | 372 | .vue (soft 300 / hard 500) | 74% of hard |
| `src-tauri/src/dangerous_commands.rs` | 364 | .rs (soft 400 / hard 800) | 46% of hard |
| `src-tauri/src/mcp/server.rs` | 353 | .rs (soft 400 / hard 800) | 44% of hard |
| `src/components/shell/SyncPanelContent.vue` | 331 | .vue (soft 300 / hard 500) | 66% of hard |
| `src/stores/ui.js` | 321 | store .js (soft 300 / hard 500) | 64% of hard |
| `src/components/terminal/TerminalTabs.vue` | 301 | .vue (soft 300 / hard 500) | 60% of hard |

## Next architecture session (proposed)

**Target**: split `src-tauri/src/ssh.rs` (1874 → ~5 modules of 300-500 each). Natural boundaries already marked by `// ---` comment dividers in the file:
- `ssh/session.rs` — connect/auth/host-key/keyboard-interactive
- `ssh/sftp.rs` — all `sftp_*` commands + chunked upload
- `ssh/tunnel.rs` — `tunnel_*` + local/remote/dynamic-forward + SOCKS5
- `ssh/headless.rs` — `HeadlessSshClient` + `connect_headless` + `exec_command_once`
- `ssh/known_hosts.rs` — host-key verification helpers

Mechanical refactor: move code, adjust `pub`, re-export from `ssh/mod.rs`. Zero logic change. Frontend fully insulated (commands keep same names).

**Secondary target (after ssh.rs lands)**: `src/stores/sessions.js` (839, 1.68×) — split terminal lifecycle from session management.
