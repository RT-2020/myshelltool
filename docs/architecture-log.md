# Architecture Log

> Cross-session memory for architectural drift. AI has no memory across sessions — this file is the sole persistent carrier.
> Read at session start to detect accumulated drift; append one row per session that touched code.
> Maintained by: `vibe-guard-pure` skill.

## How to use

- **At session start (when working on code)**: skim this log. If any file has grown by ≥0.5× of its limit since the last reset row, that's a downstream trigger — propose a pure architecture session.
- **On every ARCH-CHECK report (medium/high tier)**: append one row to the table below **immediately after the report is produced** — do not wait for session end (session boundaries are undefined in real AI harnesses and cause rows to be silently dropped). Low-tier changes produce no report and therefore no row.
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
| 2026-06-26 | v1.5 方案 A：MCP 高危工具 GUI 弹窗审批降级（补 v1.4 follow-up）。7 文件：`tools.rs` 369→414(+45), `approval.rs` 258→309(+51), `server.rs` 340→403(+63), `lib.rs` 470→501(+31, **踩 .rs soft-warn 400**), `mcp.js` 115→235(+120, store 从纯查询升级为带监听/dispose), `workbench.js` 429→444(+15), `GlobalModals.vue` 660→716(+56, **1.43× hard 500，拆分候选**)。后端复用 ssh.rs pending-oneshot **模式**（非 import，Gate A/B pass）。 | none（审批降级是协议层职责，照 ssh.rs:55-153 状态机模式在 mcp/ 内新建等价物，非跨层耦合） | medium | passed — `cargo check` exit 0 (1m20s, 1 warning `McpToolContext::new` 未用 → `#[allow(dead_code)]` 标注为 headless/测试保留), `npm run build` exit 0 (12.44s, 1676 modules), `test:core` 41/41 pass。**GlobalModals.vue 716 应纳入下次拆分目标（与 FileColumn.vue/ssh.rs 并列）** |
| 2026-06-29 | 分组管理三件套：资产拖拽移动/分组拖拽排序 + 资产编辑器分组字段改 datalist + 删 PASSWORD badge。8 文件：`ConnectionSidebar.vue` 711→842(+131, **1.42×→1.68×，"drag mixed" 进一步坐实拆分候选**), `AssetGroupNode.vue` 332→346(+14), `GlobalModals.vue` 728(实测, moveGroupOptions→groupOptions 共用 + 资产编辑器分组换 input+datalist), `assets.js` ~310→347(+37 reorderGroups), `workbench.js` 445(+1 re-export), `App.vue` +3 handler, `lib.rs` 501→518(+17 reorder_asset_groups 命令+注册), `core/lib.rs` 957(+~45 reorder_asset_groups + 3 测试)。**ConnectionSidebar.vue 已是已知 over-limit 候选**（baseline 注明 "tree + menu + drag mixed"），拖拽逻辑本属此组件职责内聚点，未新引入跨层耦合。 | none（拖拽走 emit→App.vue→store，保持 store-agnostic；moveAsset/reorderGroups 复用既有 id-upsert 模式） | medium | passed — `npm run build` exit 0 (5.72s, 1676 modules), `cargo check` exit 0 (26.39s), `test:core` 44/44 pass（含新增 3 个 reorder 测试）, UI smoke + host-key passed。 |
| 2026-06-29 | v1.6 Gist 同步面板重设计 + PAT 获取引导。6 文件：`SyncPanelContent.vue` 358→475(+117, 0.72×→**0.95× hard 500，soft-warn 监视**), **新增 `SyncPatGuide.vue` 206**（3 步引导+加密说明子组件，0.41×）, `package.json` +`@tauri-apps/plugin-opener`, `Cargo.toml` +`tauri-plugin-opener`, `lib.rs` +1 行 `.plugin(tauri_plugin_opener::init())`, `capabilities/default.json` +`opener:default`。**首次踩 500 硬上限触发拆分**：初版重写达 620 行，立即按 plan 承诺拆 PAT 引导为 SyncPatGuide（参照 McpPanelContent→McpCapabilityList 先例）。 | none（hero/block-head/detail-grid/field 范式照搬 McpPanelContent 同域同层复用，非跨层耦合；openExternal 照 useClipboard「动态 import+runtime 检测」范式；store.saveToken/syncSetup 等 action 全部复用 re-export） | medium | passed — `npm run build` exit 0 (5.42s, 1679 modules), `cargo check` exit 0 (25.73s, tauri-plugin-opener v2.5.4 正确链接), `test:core` 44/44 pass。清 1 个遗留 myshelltool.exe(8776) 解锁 os error 32。 |
| 2026-06-29 | **v1.6 自动同步（会话密钥+DPAPI）+ 启动远端探测**。high-tier：新增加解密抽象层。11 文件：`crypto.rs` +3 函数(derive_session_key/encrypt_with_key/decrypt_with_key)+7 测试, `core/sync.rs` SyncState +auto_sync_enabled 字段, `src-tauri/sync.rs` 699→~830(+~130, **踩 .rs soft-warn**, +3 命令 enable/disable_auto_sync + check_remote_updates + 改造 push/pull/resolve_conflict/reset/clear 走会话密钥路径 + SessionKey helper + b64 工具), `lib.rs` 518→521(+3 命令注册), `sync.js` 223→339(+116, +autoSyncEnabled/remoteHasUpdates 状态 + 4 action + attachWorkbench), `assets.js` 347→~365(+maybeAutoPush helper + 6 挂载点), `workbench.js` 445→~460(+syncStore.attachWorkbench + initialize 探测 + 5 re-export), `SyncPanelContent.vue` 475→461(−14, hero 加徽章+自动同步状态行), **新增 `SyncAutoSyncControl.vue` 156**(自动同步开关), **新增 `SyncConflictResolver.vue` 141**(冲突框迁出, 解 syncPanelContent 超 500 硬上限). | none（crypto 原语复用：encrypt_with_key 复用 AES-256-GCM 核心，仅外提 derive_key；SessionKey 存储照 github-pat 的 SecretStore+DPAPI 范式；maybeAutoPush 走 workbench bridge 调 syncStore 禁循环 import；hero/block 范式同域同层复用） | high | passed — `test:core` 51/51 pass（+7 key-based 测试）, `npm run build` exit 0 (3.31s, 1681 modules), `cargo check` exit 0 (1m18s)。**安全模型变更已写入 AGENTS.md §8**：主密码仍不落盘，自动同步用 DPAPI 保护派生会话密钥。**src-tauri/sync.rs ~830 进 soft-warn，下次大改前关注**。 |

## Baseline snapshot (2026-06-23, RESET — measured via `wc -l` via PowerShell `Get-Content.Count`)

Files currently over hard limit — refactor candidates (real numbers, not estimated):

| File | Lines | Limit | Multiple | Note |
|---|---|---|---|---|
| `src-tauri/src/ssh.rs` | 2097 | 800 | 2.62× | 4 domains fused: session/terminal, SFTP, tunnel (local+remote+SOCKS5), headless. **cycle-tier** (1874→2097 since 06-22 RESET). Highest priority to split. |
| `src/components/files/FileColumn.vue` | 1123 | 500 | 2.25× | **cycle-tier 触发**（768→1123, +355, 1×→2.25× 自 06-22 RESET）。v1.4「文件管理区精简重构」commit 8fa14a0 后反而暴涨。最高优先级拆分。 |
| `src/stores/sessions.js` | 858 | 500 | 1.72× | terminal lifecycle + session mgmt |
| `src/components/shell/ConnectionSidebar.vue` | 842 | 500 | 1.68× | tree + menu + drag mixed（06-29 +131 拖拽逻辑，"drag mixed" 坐实；下次拆分：把 drag state/handlers 抽 useAssetDnd composable） |
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
| `src-tauri/src/sync.rs` | ~830 | .rs (soft 400 / hard 800) | **104% of hard**（06-29 v1.6 自动同步 +SessionKey helper + 3 命令 + b64 工具，585→~830，⚠ 接近 hard limit，下次大改前关注） |
| `crates/myshelltool-core/src/lib.rs` | 891 | .rs (soft 400 / hard 800) | ⚠ **111% — 已超 hard limit**（06-22 记 796 已漂移，实测回升；应移入上方硬上限表，下次 RESET 整理） |
| `src-tauri/src/resource_monitor.rs` | 875 | .rs (soft 400 / hard 800) | ⚠ **109% — 已超 hard limit**（同上，06-22 记 792 已漂移） |
| `src/stores/workbench.js` | ~460 | store .js (soft 300 / hard 500) | 92% of hard（06-29 v1.6 +syncStore bridge + 探测 + 5 re-export，432→~460，贴硬上限） |
| `src/stores/sync.js` | 339 | store .js (soft 300 / hard 500) | 68% of hard（06-29 v1.6 +autoSync/remoteUpdates 状态 + 4 action + attachWorkbench，223→339） |
| `src/components/shell/McpPanelContent.vue` | 374 | .vue (soft 300 / hard 500) | 75% of hard |
| `src-tauri/src/dangerous_commands.rs` | 402 | .rs (soft 400 / hard 800) | 50% of hard（进入 soft-warn） |
| `src/components/shell/SyncPanelContent.vue` | 461 | .vue (soft 300 / hard 500) | **92% of hard**（06-29 v1.6 拆 SyncAutoSyncControl + SyncConflictResolver 后从 586 回降到 461，仍贴硬上限，下次加功能必须先拆） |
| `src-tauri/src/mcp/tools.rs` | 368 | .rs (soft 400 / hard 800) | 46% of hard |
| `src/stores/ui.js` | 344 | store .js (soft 300 / hard 500) | 69% of hard |
| `src/components/terminal/TerminalTabs.vue` | 338 | .vue (soft 300 / hard 500) | 68% of hard |
| `src/components/shell/AppTitleBar.vue` | 334 | .vue (soft 300 / hard 500) | 67% of hard（新进入） |
| `src/components/shell/AssetGroupNode.vue` | 324 | .vue (soft 300 / hard 500) | 65% of hard（新进入） |
| `src/components/terminal/TerminalSurface.vue` | 323 | .vue (soft 300 / hard 500) | 65% of hard |
| `src/stores/assets.js` | ~365 | store .js (soft 300 / hard 500) | 73% of hard（06-29 v1.6 +maybeAutoPush helper + 6 挂载点，323→~365） |
| `src-tauri/src/mcp/server.rs` | 339 | .rs (soft 400 / hard 800) | 42% of hard |
| `src/components/shell/SyncPatGuide.vue` | 206 | .vue (soft 300 / hard 500) | 41% of hard（06-29 v1.6 新建，PAT 引导子组件） |
| `src/components/shell/SyncAutoSyncControl.vue` | 156 | .vue (soft 300 / hard 500) | 31% of hard（06-29 v1.6 新建，自动同步开关子组件） |
| `src/components/shell/SyncConflictResolver.vue` | 141 | .vue (soft 300 / hard 500) | 28% of hard（06-29 v1.6 新建，冲突解决子组件） |

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
