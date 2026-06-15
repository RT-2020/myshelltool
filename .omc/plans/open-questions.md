# Open Questions

## ralplan-fix-ui-ssh — 2026-06-12

- [ ] `App.vue` 模板作用域中是否能直接访问 `isTauriRuntime`（Task P0-2 假设可访问） — 影响"浏览器预览 vs 已装客户端"的条件渲染实现方式（直接用函数 / 通过 store computed 暴露）。
- [ ] `status-pill.running` 硬编码"状态已接入"（`App.vue:575`）是否绑定到 `backendStatus.ready` — 已列入 out-of-scope，但若实现成本低可纳入 P1-1 同一 PR。
- [ ] Option B 中 `app.manage(ssh_mgr.clone())` 后，`AppState.ssh_sessions` 字段是否还有非 SSH 命令使用 — 若无使用方，可在 follow-up（Option A 重构）中考虑直接删除该字段，进一步简化。
- [ ] RFC 5737 文档段 IP（`203.0.113.x`）是否符合项目目标用户群（中国大陆运维）的认知习惯 — 是否改用明显更夸张的占位（如 `0.0.0.0` 或 `example.invalid`）更不易混淆。
- [ ] `core:path:default` 权限（附带发现 B）保留还是删除 — 无功能影响，需产品决策。

## framework-choice-tauri-vs-qt-vs-electron — 2026-06-13

- [ ] "方案 A 工作量 8-12 人天"是否过于乐观？特别是 IPC OOM 改流式（sftp_upload/download 不再传 `Vec<u8>`）是否低估了前端 `services/backend.js` + `stores/workbench.js` transferQueue 联调成本？ — 影响方案 A 是否仍是最低成本选项的核心论据。
- [ ] "95% 代码可救"是否忽略了测试代码（`tests/ui-smoke.mjs`、`tests/ui-extended.mjs`、`src-tauri/src/lib.rs:232-280` 的源码级回归测试）也要相应更新？ — 影响"代码资产保留率"的精确估算。
- [ ] "切 Electron 后 ssh2 (Node.js) 功能弱于 russh"是否准确？特别是 2026 年 ssh2 库在动态 SOCKS5 转发、键盘交互认证上的支持情况，需要外部文档（npmjs.com/package/ssh2 + GitHub issues）核实 — 影响"方案 C 不推荐"的关键论据强度。
- [ ] 回滚触发条件（Option A 重构失败 > 3 天 / IPC 流式改造后传输性能 < 5MB/s）的阈值是否合理？是否应该再加一条"用户主观体验倒退"作为软触发？ — 影响回滚策略的可执行性。
- [ ] 方案 B 子选项推荐 PySide6（而非 Rust + qmetaobject + russh）是否正确？如果维护者愿意保留 russh，qmetaobject 路线是否值得作为 B3 评估？ — 影响方案 B 否决理由的完整性。
- [ ] Tauri 2.x 在 2026 H2 是否有已知的破坏性变更计划（如 State<T> API 改造）可能让方案 A 的"长期维护风险"评分被低估？ — 影响加权总分表第 8 行的可信度。

## ui-full-refactor-consensus — 2026-06-14

- [ ] `resource_monitor` SSH 命令执行通道：复用现有 session 的 SSH channel exec `cat /proc/*`，还是单独打开新 channel？ — 前者可能与用户交互命令冲突，后者增加 session 复杂度；spec 未明确。
- [ ] 图表手写 vs 引图表库：spec Round 4-5 禁 Tailwind/UI 库但未明确 chart.js/d3 是否算 UI 库 — 影响右侧栏 4 类图表实现方式（本计划假设纯 SVG 手写）。
- [ ] transferDrawer 与现有 `sftp-transfer-progress` 事件关系：复用还是新建？ — 影响 Wave 3.4 传输抽屉实现复杂度。
- [ ] 右侧栏宽度策略：固定宽度（本计划假设 280px）还是可拖拽？ — 影响 AppShellLayout CSS Grid 模板复杂度。
- [ ] 5 区域布局最小屏宽断点：spec AC1 要求同时可见但未定义下限；本计划假设 1280px，低于此横向滚动 — 是否需要更激进的断点（如 1024px 下隐藏右侧栏）。
- [ ] `resource_monitor` 采样间隔默认值：spec 提到 `intervalMs` 参数但未指定默认；本计划假设 2000ms（2s） — 是否太频繁影响 SSH channel 负载。
- [ ] OSC 标题解析在 8 个 terminal 组件 restyle 时的回归保护：spec AC7/Constraints 强制保留 OSC 0/1/2 — Wave 3.3 是否需要补 explicit test step 验证 OSC 标题在 restyle 后仍响应。
