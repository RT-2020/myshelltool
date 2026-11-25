# myshelltool 界面设计方案总览（00–10）

> 本目录是 myshelltool 全部约 47 个用户可见界面的设计规范与实施总览。方案阶段产出了 `00` + `01–10` + 本 `README` 共 12 份文档（含 P0×8 + P1×44 + P2×84 共 136 项整改），并已于 2026-11 分批落地实施完成。桌面 `tauri:dev` 真机与读屏人工走查口径见 §6 诚实边界。

当前状态：**8 项 P0 + 44 项 P1 + 84 项 P2（12 份文档全部调整项）均已在代码库落地实施**；原 §5 待拍板事项中的实施取舍已按项目决策收敛（详见 §4.7）。

独立实测记录：[本机稳定性与体验走查](./本机稳定性与体验走查.md)。包含已安装版本的 UIAutoma 结果、低分辨率启动建议及未验收的链路，不计入下方设计方案验收。

## 1. 本方案是什么

- **性质**：designOnly——只产出设计方案与决策依据，不修改任何代码；实施需另行确认与授权。
- **范围**：约 47 个用户可见界面，按域拆分为 00（全局方向与设计系统）+ 10 份域文档 + 本 README，共 12 份。
- **方法**：每域先做基线诊断（P0/P1/P2 分级，逐条给处理方式），再逐界面定格——workspace 类型判定、ASCII 线框与阅读顺序、区块任务表、8 状态矩阵、空态四分、断点四档验收基准；全局层（00）先经评审修订固化方向（见 §3）。
- **证据口径**：代码引用全部到 path:line；对比度数字凡 WCAG 公式 node 实算者已标注，alpha 合成估算者已标注「待实测」。

产出清单（行数为 `wc -l` 实测）：

| 文档 | 覆盖范围 | 行数 |
|---|---|---|
| [00-全局方向与设计系统.md](./00-全局方向与设计系统.md) | 设计方向、token 体系、动效/状态/对比度基线、语料口径（经评审 7 条修订，见 §3） | 266 |
| [01-全局壳与布局.md](./01-全局壳与布局.md) | 应用根、主窗口壳、资产窗口壳、全局搜索 | 361 |
| [02-资产侧栏.md](./02-资产侧栏.md) | 资产侧栏（展开态 / 折叠态 rail+flyout / 空态与失败态） | 370 |
| [03-终端域.md](./03-终端域.md) | 终端域 8 个表面 | 484 |
| [04-文件域与传输.md](./04-文件域与传输.md) | 文件域与传输 6 个界面 | 410 |
| [05-内置编辑器.md](./05-内置编辑器.md) | 内置编辑器（弹窗形态 workspace） | 255 |
| [06-资源监控与会话摘要.md](./06-资源监控与会话摘要.md) | 资源监控与会话摘要 3 个界面 | 295 |
| [07-设置中心.md](./07-设置中心.md) | 设置中心（主从骨架 6 tab） | 466 |
| [08-同步面板.md](./08-同步面板.md) | 资产同步面板 | 439 |
| [09-MCP面板.md](./09-MCP面板.md) | MCP 面板（执行日志审计） | 432 |
| [10-弹窗体系与全局反馈.md](./10-弹窗体系与全局反馈.md) | GlobalModals / AppModal / toast / 右键菜单等 9 个浮层界面 | 559 |
| README（本文件） | 总览索引、高优问题汇总、拍板事项汇总 | — |

## 2. 阅读顺序建议与各文档一句话简介

建议顺序：**00 → 10 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 本 README §4/§5**。

理由：00 是全局约束单一来源，所有域文档引用它；10（弹窗体系）是多数域共享的浮层宿主（GlobalModals / AppModal），先读可理解各域弹窗引用的公共契约；01 是所有界面的骨架宿主；02–05 为高频主工作流域，06–09 为功能面板域。

一句话简介（编号即建议阅读顺序）：

1. **00 全局方向与设计系统**：token、动效规则（持续动画允许类别仅两类 pulse/spinner + D4 存量归类）、状态与对比度基线、语料口径；已按评审完成 7 条就地修订（见 §3）。
2. **10 弹窗体系与全局反馈**：统一键盘契约（初始焦点=安全侧动作）、AppModal footer 出口解锁、token 映射消三份复制；17 条诊断（1 P0 经三重确证）。
3. **01 全局壳与布局**：四界面骨架定格；16 条诊断，P0 对比度修复全部为消费侧换 token 档位方案且替换组合经 node 脚本实跑验证；复核推翻诊断一条（侧栏搜索框焦点指示不成立，改判死规则清理项）。
4. **02 资产侧栏**：三态定格、状态点六态消费 .dot 权威集、网关徽标改消费既有 token；核验新增「后端初始化失败被首空态掩盖」实证发现（workbench.ts:120-125）并给出错误视图方案。
5. **03 终端域**：8 个表面定格，保留恒深终端区特色与既有交互契约；14 条诊断集中在样式链断裂（全局 .btn 类不存在被两文件引用、Alt+Enter 死键、裸 z-index 7 处）而非结构。
6. **04 文件域与传输**：双栏对开表格主骨架 6 界面定格；复核修正诊断「队列只增不减」实为 60 秒窗口自动裁剪（transferQueueOps.ts:90-96），真实问题反转为失败记录消失过快。
7. **05 内置编辑器**：弹窗形态四段式骨架定格；P1×2（预览通道补 tabId 归属、关闭钮改原生 button 打通键盘关闭与 dirty 三选），既有交互契约全部保留为资产。
8. **06 资源监控与会话摘要**：三界面定格；两大 P0——信息性小字升 --app-muted（node 实算 subtle 2.79/2.61:1 不达标、muted 5.29/4.94:1 两主题过线）、焦点环全局修复按继承机制提回 00 层（§5 A5）。
9. **07 设置中心**：主从骨架；补齐「设置选择卡」统一契约（token 修正 + aria-pressed + 焦点环显式声明）、.block 五份复制样式上收 _utilities.scss（连带修复 fallback 链双落空）、UpdateSection :value→:percent 恢复下载进度。
10. **08 同步面板**：focused 型三层 IA 沿用；冲突按钮对称化+diff 摘要、面板内探测不广播、travel 双向化（待 A4 豁免登记）、死代码清理与两处 00 §4.3 格式覆盖登记。
11. **09 MCP 面板**：console 时间线型（执行日志审计区）；「信息小字 12px+中性色、状态色移至色点」对比度规则、store 六处 announce 补 level、两处读盘失败 fail-closed 显错；P0 对比度数字经 node 复算与诊断完全吻合。

## 3. 全局方向（00 文档）评审修订记录

00 文档已按评审逐条**就地修订**（未新建文件）。流程：先复现验证评审断言，确认 7 条全部属实后才动笔；修订后 grep 复查零残留。

**第一步：复现验证评审断言**（7 条全部属实）：

- find 检索「构图决策」「视觉决策」两份外部文件：无输出——仓内确无这两份文件，它们是用户级技能参考，非仓库资产；
- grep「六问」命中 00 文档 3 处 + site/README.md；
- grep `infinite` 对 src/ 全量实测 12 处（本会话抽查：00 文档 D4 行登记与之一致——spinner 类 8、pulse 类 2、travel/blink 超类 2）；
- grep `text-lg` 命中含 `src/styles/_base.scss:44`（本会话抽查实测：`_base.scss:44` 即 `h2 { font-size: var(--text-lg); font-weight: 650; }`）；
- 语料检索「克制」「高级」零命中；
- 语料 L4:111 实为「表格 >5 列」。

**第二步：逐条修订**（7 条全部完成）：

1. **不可解析来源**：两份外部文件的引用全部改为内联判据或 WCAG/path:line。
2. **编号漂移修正**：§2.4→§2.3 三处 + 评审外自查第 4 处（D1 行）；§2.7→§2.6；grep 复查零残留。
3. **持续动画表述改为规则**：「持续动画仅两处」改为「允许类别仅两类（pulse/spinner）——规则表述，非现状计数」，并实测登记 12 处存量；travel/blink 超类入 §1.3 新增 D4。
4. **动效表按真实分布拆 4 行**：强调浮层=emphasized（含 GlobalModals --dur-slow）、瞬态轻浮层/Tab 指示条=standard、布局位移=emphasized；抽屉缓动未复核不预填，留域文档实测登记。
5. **text-lg 消费补全**：补上 `_base.scss:44` h2=20px，并写明标题档豁免白名单。
6. **「克制高级感」归因修正**：改指 `open-design-prompt.md:23-27`/`:525`；P4/P6 单独指语料 :29-31。
7. **表格列上限修正**：工具栏 ≤4（L3:95/L4:111）、通用表格 ≤5（L4:111）、文件区 4 列为 L3:96 既成不外推。

结论：全部 7 条均可安全修订，无遗留障碍项。

**冷复审（2026-11-11，独立复审员对修订后最终稿）**：**pass**，无 mustFix 残留。7 条修订逐条兑现确认（六问/ctx 陈旧引用零残留、空话词仅 1 处且带出处、动效表与代码一致、抽屉缓动留空未预填、标题档豁免白名单在位、覆盖措辞与语料逐字吻合）；抽查代码锚点 20+ 处全部命中（含 workbench-shell 断点两处一致性、tauri.conf.json 窗口尺寸、_tokens 全部被引行）。3 条不阻塞建议已当场采纳：AGENTS.md §4.1 z-index 枚举补 `--z-popover`（与 token 八档对齐）、§1.1 背景阶梯措辞改精确值「每级恒 +2%」、§2.3 展示角色依据改引 open-design-prompt.md:525 并括注 h1 豁免。

## 4. 高优先级问题汇总

各域基线诊断计数（P0 最高；全部已在该域文档给出处理方式）：

| 域文档 | P0 | P1 | P2 | 合计 |
|---|---|---|---|---|
| 01 全局壳与布局 | 3 | 7 | 6 | 16 |
| 02 资产侧栏 | 0 | 2 | 10 | 12 |
| 03 终端域 | 0 | 4 | 10 | 14 |
| 04 文件域与传输 | 0 | 6 | 6 | 12 |
| 05 内置编辑器 | 0 | 2 | 9 | 11 |
| 06 资源监控与会话摘要 | 2 | 5 | 8 | 15 |
| 07 设置中心 | 0 | 6 | 7 | 13 |
| 08 同步面板 | 1 | 3 | 8 | 12 |
| 09 MCP面板 | 1 | 4 | 9 | 14 |
| 10 弹窗体系与全局反馈 | 1 | 5 | 11 | 17 |
| **合计** | **8** | **44** | **84** | **136** |

P0 分布（8 条）：01×3（对比度，消费侧换 token 档位，替换组合经实跑验证）、06×2（信息性小字对比度 + 焦点环全局 token）、08×1、09×1、10×1。除 06 的焦点环属全局 token 需 00 层决策（§5 A5）外，其余均有域内处理方案。

## 4.5 首批实施记录（2026-11-11）

P0 批次已实施并通过验证（对照各域文档处方逐条落地）：

| 来源 | 实施 | 文件 |
|---|---|---|
| 10 域 A1（P0） | AppModal 补 `v-if="$slots.footer"` 具名插槽出口——SshImportDialog「取消/导入」按钮从不渲染 → 实渲染 | `src/components/ui/AppModal.vue` |
| 00 §5 A5（06 域 P0-2 提案） | `--focus-ring` 双层化：`0 0 0 1px accent + 0 0 0 4px accent-soft-strong`（浅色内环 5.42:1；暗色经 var 惰性求值自动适配） | `src/styles/_tokens.scss` |
| 01 域 A1-A3（3×P0） | 状态栏 `.sb-item.muted` subtle→muted（4.89:1）；`.badge.success/.warn` 文字→muted（4.60-4.69:1，色点+soft 底双编码保留）；警示横幅文字→`--app-text`（16.38:1）+ 补 AlertTriangle 图标 | `workbench-shell.scss`、`_utilities.scss`、`App.vue` |
| 06 域 P0-1 | OpsSummaryPanel 信息小字 subtle→muted ×4 处；摘要值行两级层级（数据=text、占位=muted）；DiskChart/CpuChart 同款 | `OpsSummaryPanel.vue`、`DiskChart.vue`、`CpuChart.vue` |
| 08 域 调整1（P0） | `.adv-note` subtle→muted（4.94:1） | `SyncAdvancedSettings.vue` |
| 09 域 R1（P0） | MCP 四件：10/11px→`--text-xs` 全量（15 处）、信息文字 subtle→muted、warn/success 状态改「6px 色点 + muted 文字」（danger 例外保留）、时间列补 tabular-nums | `McpExecutionLogList/McpCapabilityList/McpLogFilterBar/McpPanelContent.vue` |

**验证**：`npm run build` 全绿（含 lint:facts/size、check:ipc、type-check）；`test:ui` 五套全过（smoke 新增导入弹窗 footer 按钮回归断言——打开设置→资产→导入向导→断言按钮可见可点→取消关闭）；浏览器实测：footer 按钮 DOM 实在（`["取消","导入[disabled]"]`）、双层焦点环计算样式逐字节吻合、横幅/状态栏换档计算色值吻合、console 仅 1 条预览模式存量警告（非本次引入）。**未验**：tauri:dev 真桌面全量走查（A5 落地条件）、MCP 面板真实数据态（预览模式无后端）。

## 4.6 评分记录（2026-11-16，design-craft 量规 · 预览模式子集）

> 应用户要求补跑 design-craft 式 8 维评分。**口径**：浏览器预览模式（`npm run dev`）可探子集实测取证（Playwright：DOM/计算样式/键盘/缩放/对比度页面内 WCAG 实算）；Tauri 依赖面如实标 na。这是「预览模式子集」分数，不是产品验收分——`scoring.deliverable` 判定不适用（桌面应用真实验收在 tauri:dev）。

| 维 | 权重 | 分 | 关键证据（实measured） | failReasons |
|---|---|---|---|---|
| D1 首屏与信息层级 | .90 | 2 | 6 区域全渲染；搜索占位「搜索连接 / 命令 / 文件」；侧栏首空带 CTA；终端空态带指引；状态栏三段状态 | — |
| D2 版式与节奏 | .75 | 2 | grid 三行三列契约（52/720/28 + 260/826/280）；shell=视口零溢出；200%/400% 等效视口（683/341px）溢出均 false | — |
| D3 字体与排版 | .65 | 1 | 角色清晰（Inter/Mono）；time 列 tabular | 10–11.5px 同角色散值带无 <12px token 档（=§5 A2 待拍板项，存量仍在） |
| D4 色彩与表面 | .55 | 2 | 浅色 8 点实测全 ≥4.5（横幅 16.38/状态栏 4.89/监控占位 4.94/标签 5.29…）；暗色 muted 系 6.27；badge 点+文字双编码 | 同步/MCP 面板内点位预览模式无状态实例，未取证（登记 P1） |
| D5 状态与交互反馈 | .75 | 1 | Tab 6 站焦点全可见（双层环计算样式逐字节吻合）；空态带下一步 | 主任务演练（连接→失败→重试）预览模式不可执行；hover/active 时长未逐一浏览器实测 |
| D6 文案与真实性 | .60 | 2 | 空态/横幅/状态栏文案如实带下一步；零占位假文 | — |
| D7 细节一致性 | .45 | 1 | console 0 error（1 条存量预览警告豁免）；缩放无溢出 | 按钮圆角杂音：7px×2、999px 与 9999px 两套药丸值并存、0px×8 待归类；icon-btn 28×28（桌面指针场景按 D13 从宽，na 有理由） |
| D8 分享元数据 | .35 | na→按 1 计 | 桌面应用无 og:/canonical 消费面 | na+豁免理由 |

**总分**：raw **7.80**（满分 16 加权；P0=0 不封顶）。**五条门槛**：c1 无 P0 ✓、c2 无 0 分 ✓、c3 ≤1 个 1 分维 ✗（3 个：D3/D5/D7）、c4 P1≤2 ✓、c5 主任务演练 ✗（预览模式不可执行）→ **deliverable = false（如实）**。

**本轮评分发现并当场修复**：`ResourceMonitorPanel.vue` 两处 06 域同款漏网点（`.rs-empty-banner` 占位横幅 2.61:1、同名 `.rs-section-title` 2.79:1——06 域诊断当时只点名 OpsSummaryPanel，同名复制漏扫）→ 换 `--app-muted` 复测 4.94/5.29 达标，build+smoke 回归绿。

### 4.6.1 精修轮 + 真机取证轮（2026-11-16 续，工作流 ⑦⑧ 段）

**精修（⑦）**：圆角散值最小修复——`workbench-shell.scss` 两处 `999px` → `var(--radius-pill)`（同为全圆零视觉差异）；7px×5 与 0px 群属 §5 A8 拍板项不动。DESIGN.md 沉淀由 docs/设计/ 体系承担（沿用既有结构）。

**真机取证（tauri:dev + computer-use a11y 操控，真实 SSH 链路）**：
- **主任务成功路径 ✓**：点击终端空态「连接 192.168.2.2-fusionpbx」→ 真实 SSH 认证成功（auth succeeded，dev 日志）→ 全状态链实测：状态栏「正在连接：…」（进行态）→ 会话摘要「连接中 → 已连接」→ 终端 tab 出现 + Terminal input 聚焦 → 监控「等待首次采样 → 指标收集中」（CPU 30.4% / 内存 36.3% / 磁盘 / 网络双向速率实采）→ 文件区转 busy 禁用 → 「远程文件已刷新：192.168.2.2-fusionpbx」→ 远程目录真实条目渲染（权限/属主/类型/大小/时间全列）。
- **失败路径真机取证 ✗（如实登记）**：快速连接输入不可达主机的操作被工具边界阻断——a11y 树被远程目录 398 元素撑到优先级裁剪，侧栏 textfield 进不了树；坐标点击按 GUI 代操纪律（误点风险）不采用。失败路径反馈链以 00 §3.6 代码级定格 + term-error-banner 实现锚点为准，真机失败态留待后续人工/工具轮。
- **D5 hover 实测 ✓**（静态 CSS 预览模式等价取证）：`.icon-btn`/`button`/`.badge` transition 120ms（≤160ms 合规）、纯绘制属性（background/color/border-color），无带时长的 `transition:all`；`.sb-item`/`.transfer-pill` 0s 瞬切（无动效不违规）。

**最终判定（⑧，分数不变 7.80 / deliverable = false，如实）**：D5 维持 1 分（成功路径已真机实证，失败步未真机取证——量规犹豫取低档）；D7 维持 1 分（杂音减少但 7px/0px 群仍在）；c5 主任务演练=失败步受阻未闭合；c3 三个 1 分维（D3/D5/D7，全部对应 §5 拍板项 A2/A8 或工具边界）。**工作流八段至此全部走完**（需求→约束→构图→定格→实现→评审修复→精修→最终复验），未闭合项均为已登记的待拍板存量，非未执行步骤。残留 WebView2 实例占位问题（安装版残留进程先占 browser process 会吞掉 WebView2 调试参数）已在本轮定位并清场。

## 4.7 P1/P2 全量实施收口（2026-11-25）

在前期 8 项 P0 落地基础上，分 8 批完成 `00` + `01–10` 全部 **44 项 P1 + 84 项 P2** 整改，并取消 500 行机械行数门禁（改以职责内聚度评估拆分）：

- **全局 token 与基础控件（Batches 1–2）**：`_tokens.scss` 新增 `--term-cursor`；`_utilities.scss` 补充 `.sr-only`、disabled `.icon-btn`、非交互 `.badge` 及设置面板共享排版类；新增 `_form-feedback.scss` 与 `useModalFocus.ts`（统一焦点围栏与关闭归位）；`toastQueue.ts` + `stores/ui.ts` + `AppToastHost.vue` 支持悬停/聚焦暂停自动消隐、同级折叠与等级图标；`AppButton`/`AppSelect`/`AppCombobox`/`AppTab`/`AppTabGroup`/`AppProgress`/`AppModal`/`AppContextMenu` 补全 `:active`、`aria-live` 与键盘导航；弹窗域（`10`）拆出 `SecurityPromptContent.vue`、`KeyboardInteractiveContent.vue`，收窄 `ModalType`（移除死分支 `terminalSearch`/`syncPanel`），统一 `GlobalModals` 与 `GatewayWizardContent` 底部对齐。
- **壳层、设置、同步与 MCP（Batches 3–5）**：壳层（`01`）拆出 `WindowControls.vue`、`ThemeToggleButton.vue`，对齐窄视口标题栏与状态栏交互反馈；设置中心（`07`）关于区 `AppBrandLogo` 调整为 `:size="40"`，整理外观/传输/资产/更新日志分区；同步面板（`08`）将 `SyncStatusRail.vue` 动效重命名为双向 `travel-out`/`travel-in` 并完善备份查找、冲突裁决、PAT 配置与自动同步交互反馈；MCP 面板（`09`）落实 `mcp.ts` fail-closed `strict` 策略、错误态 ref/announce 广播及日志筛选与作用域设置清晰度。
- **侧栏、监控、终端、文件与编辑器（Batches 6–8）**：
  - 资产侧栏（`02`）：新增 `src/lib/sessionStatus.ts` 归一化与主导状态选择、`AssetSidebarEmpty.vue` 空态引导、`localStorage` 分组折叠态持久化与 `Esc` 取消拖拽。
  - 资源监控（`06`）：`resourceMonitor.ts` 新增 `paused`/`pause()`/`resume()`（保留最近快照）、`OpsSummaryPanel.vue` 四态矩阵与复制会话 ID 按钮、CPU/Memory/Network/Disk 图表补齐 `vector-effect: non-scaling-stroke` 与可读 tooltip。
  - 终端域（`03`）：新增批量关标签确认窗 `confirmBatchCloseTabs`；`sessions.ts` 记录 `hasConnectedOnce` 区分首开空态与断连后状态；`CommandPalette.vue` 补齐 combobox/listbox 语义；`TerminalTabs.vue` 溢出/右键菜单键盘导航与焦点恢复；`TerminalSearchBar.vue` 26px 动作按钮 + `--z-popover`（同步修复 `tests/ui-ipc-flows.mjs` 在 `800×600` 等待 grid transition 稳定后再测几何）。
  - 文件域（`04`）与编辑器（`05`）：`formatBytes`（1024 进制）扩至 `GB` 档并移除重复 `formatFileEntrySize`；`transferQueueOps.ts` 豁免 `error`/`cancelled` 自动裁剪并在 `TransferDrawer.vue` 增加「清空已完成」与焦点围栏；`dirTransfer.ts` 超限报错补全四要素；`CodeMirrorHost.vue` 改为 `(tabId, text)` 派发防切 tab 串写、`EditorSurface.vue` 绑定 `tabId` 预览状态并优化工具栏加载态与状态栏。
- **已拍板收敛与制度更新**：保留全局 `13.5px` 正文与 `10–11px` chrome 微字号、`SyncStatusRail` 双向 `travel` 与 `term-blink` 动效、`右键查看更多操作` 提示；从 `package.json`、`AGENTS.md`、`.github/copilot-instructions.md` 与 `docs/architecture-log.md` 取消 `lint:size` / 500 行机械行数门禁。
- **验证口径**：已跑通 `npm run test:ui`（Playwright 5 套全部通过）、`npm run test:core`（243 passed, 0 failed）、`cd src-tauri && cargo check`（0 errors）以及终版 `npm run build`；`npm run tauri:dev` 真机桌面 SSH/SFTP 实操与读屏人工走查**未执行**。

## 5. 待用户拍板事项（33 项）

> 由 10 份域文档 openQuestions（58 条）与 6 条全局待确认项（合计 64 条）去重、同类合并而成；每项标注来源域文档与关键证据。编号供后续拍板引用。

### A. 全局设计系统（提回 00 层裁决）

- **A1 排版基线收敛**（00+05）：语料 L2「正文 14px」与代码全局 13.5px（_base.scss:14）不一致——改语料或改代码（文档暂按代码口径记录）；CodeMirror 编辑器默认字号（05 域 A4 建议 var(--text-sm)）与 00 D1 联动裁决；h1–h4 的 650/620 字重（_base.scss:43-46，本会话实测确认）是否按 open-design-prompt.md:523 规整为 600/500，需走查视觉影响后定。
- **A2 小档位 token 缺口**（05+08+07）：微间距 gap:2px 类全域约 18 处而 token 无 <4px 档；是否新增 --text-2xs（11px）token 收敛 10–11px 硬编码字号（不补则维持登记不整改）；.block-head 11px 标签低于 00 §2.3 标签角色 12px 白名单一档（五份复制既成值，上收时不为风格统一重写；08/09 域随迁删副本时是否全局统一到 12px 需与 McpPanelContent 等域外现值一并评估）——增设档位或登记豁免，均属全局层决策。
- **A3 终端 token 化与画布内 z-index 档位**（03）：--term-cursor 走新增 token（倾向）还是 color-mix 视觉等价，是否连带规整 --terminal-text-dim 旧名档；错误横幅/搜索条升档用 --z-popover（250）属非 Teleport 浮层的升档使用，是否为画布内局部浮层增设 token 档。
- **A4 持续动画归类与豁免**（00 D4 + 08，同源）：term-blink（TerminalSurface.vue:443）建议作终端光标固有行为登记终端区豁免；travel 双向动效（SyncStatusRail.vue:202）整改为 pulse 还是登记 00 §2.5 第三类豁免——取舍属设计+代码决策（拍板前 08 域按双向 travel 执行）。
- **A5 焦点环全局修复**（06，P0-2）：--focus-ring 现值约 1.39:1 低于 00 §3.3 焦点环 ≥3:1，与 00 §1.1 token 冻结冲突——双层环提案（0 0 0 1px accent + 0 0 0 4px accent-soft-strong，纯 token 组合，内环实算 5.42:1）或改 accent-soft-strong 取值；落地需 tauri:dev 全量走查。
- **A6 暗色 --app-subtle 消费纪律**（01+08）：暗色 --app-subtle 合成对比实测约 3.41:1（低于 4.5:1，与诊断 3.57:1 存在换算口径差已标注），是否在 00 定「subtle 仅限 placeholder/纯装饰」的全局消费纪律；设置弹窗窗框标题 12px subtle（GlobalModals.modal.scss:106-113，复算约 2.6:1）同款问题归设置域还是全局弹窗层。
- **A7 品牌徽标档位口径**（01+07）：00 §2.6 登记「标题栏 20px 档」实测为 26px（WorkbenchShell.vue:271 / AssetWindowShell.vue:244，仍属 <40 简化渲染档），01 域建议提回 00 修订措辞；关于页登记 ≥40px 完整档（与 AppBrandLogo.vue:9-11 组件注释一致）但实装 :size="36"（SettingsPanelContent.vue:163）走简化渲染——改登记还是改实装（07 域倾向提 40px，判为实现漂移）。
- **A8 圆角与间距归档**（01+10）：A9 圆角/间距归档精确档位（5/7/10px 归 sm 或 md）有 ≤4px 视觉变化，需 tauri:dev 走查逐处定案（999px→pill 两处可先行，零视觉差）；GlobalModals（md）/ AppModal（lg）双轨圆角是否统一，需一次视觉走查后再定（本轮不改）。
- **A9 弹窗进场动效强度**（05）：编辑器等高频弹窗建议低强度进场（scale 0.98→1 + fade）避免打断感；00 §2.5 只定缓动家族未定具体 keyframes，走查后定格。
- **A10 无消费面项定案**（00）：舒适密度档维持仅紧凑档（出现真实诉求再按 data-density 机制实现）；「权限空」空态类型在当前本地单用户架构下无消费面，仅保留定义（出现远端权限错误语义时再落地）——两项请确认或撤销。
- **A11 ui/ 状态缺口整改排期**（00）：按压 active 态、Select/Combobox/Tab 的 disabled 与空集守卫属代码任务，是否进入近期迭代由任务编排方决定。

### B. 共享宿主与架构边界

- **B1 GlobalModals 宿主双决策**（10+09）：495 行贴 500 硬上限，是否显式冻结「只维护不新增 type」、新增一律走 AppModal 轨（属架构决策）；宿主无焦点 trap/焦点还原（grep 实证，Esc 由 useModalDismiss 注册 window keydown），与 00 §3.4「AppModal 是浮层键盘可达标准」存在宿主级差距，是否补属域外共享宿主的全局改造，不由单域定。
- **B2 EditorSurface --accent-contrast 收敛范围**（09）：editor/EditorSurface.vue:363 存在同款 --accent-contrast 不存在 token 引用，属 editor 域文件——是否随 09 域 R5 一并收敛需任务编排方划定授权范围，避免两处漂移残留。
- **B3 共享子组件粒度**（01）：WindowControls / ThemeToggleButton 落 workbench/ 的粒度需与多窗口域对齐（两壳关闭按钮行为不同，子组件须 emit 而非内联关闭）。
- **B4 日志组件预防性拆分时机**（09）：McpExecutionLogList.vue 现有 470 行（wc -l 实测），叠加方案增量约 485 行仍低于 500 硬上限但已无二次增量空间——是否现在预防性拆分（先例：过滤条已拆出）属任务编排决策。
- **B5 编辑器 too-large 下载入口**（05）：本方案用文案指向文件面板（零新增）；若在错误面板直供下载按钮（复用 files 域能力）属跨域新增，需单独立项。

### C. 产品与交互语义

- **C1 危险与确认语义**（03+10）：终端批量关闭（关闭其他/关闭右侧）是否加确认——建议 GlobalModals 简单确认+会话数文案，但高频工具不设闸同样成立；安全审批初始焦点落「拒绝」的高频摩擦——hostKeyVerify 每次首连必弹，fail-closed 与键盘效率的取舍（备选：初始焦点=信任钮但 Enter 二次确认）；error toast 限时+hover 暂停+状态栏兜底 vs 常驻——本方案选前者（行动入口全在常驻 UI），与 states「需行动错误常驻」有域内例外，待确认。
- **C2 传输队列语义**（04）：失败/取消记录保留策略——现状 60 秒自动裁剪使「已失败保留可重试」名存实亡，提案 error/cancelled 豁免自动裁剪 + 手动清空已完成，替代方案延长窗口 + 清理入口（涉队列内存语义）；字节格式化 GB 档与进制口径——两份实现均无 GB 档（>1GiB 显示 1536 MB），合并为单一实现时是否补 GB、二进制还是十进制标签，一次决策。
- **C3 状态点与空态语义**（02+03）：手动断开与异常断开是否细分状态点——SessionEntry.manualDisconnect 字段已存在但 .dot 权威集无对应档，细分须动 _utilities.scss 全局评估，待产品决策；终端空态首空/清空分叉需 sessions store 增应用生命周期布尔（hasConnectedOnce），不加则接受双 CTA 合一形态。
- **C4 常驻教学文案**（04）：「右键查看更多操作」常驻弱提示（FileSurface.vue:420，S2 有意决策）与语料 L4/P3 常驻教学文案禁令的张力——保留 / 首次出现 / 移除，待项目决策。
- **C5 偏好持久化范围**（02+06）：侧栏分组折叠态是否跨会话持久化（现状组件内存、重启全展开；目录树型布局建议持久化，存 localStorage 还是随资产元数据待定）；监控暂停态按 AGENTS.md §7「活跃会话/监控纯内存」契约定格为内存态，若需持久化偏好则与该契约冲突，需另行决策。
- **C6 侧栏键盘与浮层排期**（02）：ARIA tree 完整重构（调整 #11c）改变现有键盘行为，需 test:ui 回归+读屏走查证据后排期（只做 11a/11b 还是含 11c 待编排决策）；拖出开窗 affordance 是否需要、Esc 取消守卫落地批次待真桌面拖拽误开窗频率实测数据；flyout aria-expanded 用 JS 态并存还是纯 JS 驱动重构 :has()，待读屏实测暴露实际问题时定。
- **C7 后端不可用的恢复路径**（02）：本轮未找到资产重载/后端 re-init 用户入口（initialize 仅 App 挂载执行一次），错误视图的下一步只能写重启应用——是否新增重载能力，若新增再补重试主操作。
- **C8 语料与代码口径同步**（04+05）：文件区表格语料 L3「4 列」vs 代码「详细 6 列 + 窄栏回退 4 列」（FileColumnColumns.vue:17-24, :70-80）漂移，建议 00 §4.3 与语料按代码口径同步；编辑器工具栏「图标+文字」按钮组（最多同屏 9 钮、flex-wrap）与语料「工具栏 ≤4 图标」（纯图标条口径）的关系需全局审计裁决，若不认可形态差异解释须收敛进「更多」菜单。
- **C9 同步面板三项**（08）：遗留 syncPanel modal 分支（GlobalModals.vue:335-336 + modalTitles.ts:24，grep src/ 与 tests/ 零命中无打开方）是否删除；危险操作分组 danger-soft 卡底与 00 §2.2 状态色白名单的出入按覆盖登记处理，项目若否决改 danger 描边方案；按钮 loading 文案切换宽度跳动（推送中…/拉取中…）是否按 states 收敛——ipc-flows 测试对按钮文案有断言面，改动须同步核对。
- **C10 主密码恢复长度下限**（08）：放开恢复模式长度下限（调整 12）前，是否存在 <6 位主密码的历史备份未证实——建议用一个真实历史备份验证解密路径后再合入。
- **C11 设置中心三项**（07）：节卡底色显式化为 var(--app-panel)（现状 transparent，视觉零变化）后是否给 --app-panel-2 微浮起，建议落地走查两方向比对后定；UpdateSection 真实错误文案需 tauri:dev 断网场景采样校准（err 来自异常消息 useAutoUpdate.ts:84/:150，A11 兜底方向已定）；modal 760×560 上限（GlobalModals.modal.scss:101-104）对当前 6 tab 够用，后续新增设置分组是否升级独立窗口/整页属产品决策（本轮不动仅挂账）。

### D. 实测与复核任务（落地前置，非方向决策）

- **D1 对比度实测回填**（03+05+06+10）：03 域 --term-muted 与 --danger 在实际背景上的实值、焦点环深色画布 3:1；05 域浅色焦点环 --accent-soft-strong #cfdaff（_tokens.scss:88）在浅色面板是否 ≥3:1 及 CM outline:none 后 caret+activeLine 焦点可感知性；06 域深色四值（subtle 3.53 / muted 5.87 / 焦点环 1.47 / accent 4.41，均 alpha 合成估算）；10 域 muted 标签 / danger-note / 4 级图标 on color-mix 背景——按 00 §3.3 最弱位置口径，以 tauri:dev 真桌面走查为验收前置。其中 05 域浅色焦点环问句已可代码级回答：#cfdaff on #ffffff=1.39:1、不达 3:1（2026-11-11 独立复算，坐实 §5 A5）；06 域深色前三项独立复算吻合（subtle 3.53–3.56 / muted 5.86 / 焦点环 1.46，≤0.01 舍入差），渲染实测仍为落地前置。
- **D2 键盘与读屏走查**（02+03+05+07+10）：03 域 A7 焦点圈闭后真实 Tab 路径、A10 listbox 读屏朗读、A11 溢出菜单键盘链（均系代码级推导）；05 域 A2 落地后 Tab 全链；10 域 Tab 全链路；07 域 I7 焦点压制为代码级级联推断（.theme-card.active 特异性 0,2,0 > 全局 :focus-visible 0,1,0）需 Tab 实测取证（A6 显式声明无论实测结果均正确）；02 域读屏走查。
- **D3 回归与测试面**（10+04+05）：10 域 A7 ModalType 字面量收窄后 PatConfigCard v-else fallback 角色移除，需一次五套 test:ui 全量回归确认无隐性依赖（tests 对 SshImportDialog 零覆盖已实证测试面薄弱）；04 域 A4 roving tabindex 与 tests/file-loading 依赖的行 data-path/data-kind 测试钩子兼容提前登记（不阻塞方向）；05 域 ui-editor.mjs 回归。
- **D4 badge 消费面清点**（01+06）：改全局 .badge 文字色影响域外消费面（OpsSummaryPanel 等，_utilities.scss:176-177），落地时需列全消费方并同步走查；AssetWindowShell 自有 connecting/error 徽标类落地时同口径复测；06 域 badge cursor 归位消费方（SyncAutoSyncControl / SyncSecurityOptionsCard / AssetWindowShell 中 badge 是否可点未逐一实读）需 grep 清点后逐处确认。
- **D5 诊断源回写**（01+04）：两处复核修正是否回写诊断文档由任务编排方决定——01 域（P1 侧栏搜索焦点不成立 + 暗色对比数字口径差）、04 域（传输队列裁剪事实反转 + 文件区表格语料 4 列 vs 代码 6 列漂移登记）。
- **D6 MCP 后端行为补证**（09）：mcp_get_config IPC 层失败时后端 mcp_scope 判定对 MCP 工具调用的实际行为未验证（denyAll 只对应读盘损坏层，UI 已按「不显示宽松结论」设计但「以后端判定为准」文案的确切行为需读 crates/myshelltool-core 判定路径后补证）；后端是否可能返回 minimal/strict 以外的拦截等级值未验证——R13 已按 fail-closed 定格（未知值显示折叠为 strict，仅显示层不写后端），若后端保证枚举两值则降级为防御性注释。

## 6. 诚实边界

1. **证据口径**：设计现状基线源自代码级 + 文档级核对，后续在预览模式下完成了 5 套 Playwright UI 测试、`test:core`（243 条）及 `cargo check`/`npm run build` 验证；**完整桌面端（`npm run tauri:dev`）真机 SSH/SFTP 实操与读屏人工走查均未执行**。对比度数字分两类：WCAG 公式 node 实算（各域已标注实算值）与 alpha 合成估算（已标注「待实测」），后续桌面真机走查仍需按 00 §3.3 最弱位置口径复核。
2. **诊断条目实施前复核记录**：实施过程中已对照源码逐条复核并纠偏，例如推翻「侧栏搜索框零焦点指示」（`ConnectionSidebar.sidebar.scss` scoped `:focus` 规则已存在，改作死规则清理）、确认「传输队列按 60 秒自动裁剪」（`transferQueueOps.ts` 后为 `error`/`cancelled` 加豁免及「清空已完成」）。
3. **实施收敛与待决边界**：12 份文档列出的 P0×8 + P1×44 + P2×84 调整项已分批在代码库落地（见 §4.5 与 §4.7）；§5 所列历史待拍板清单保留作背景与设计依据留档，若未来需二次微调（如字号升档、深浅对比细调）可再按条重开。
4. **已知覆盖缺口（已闭环）**：04 域对 `src/lib/dirTransfer.ts`（目录整传）原无实质覆盖——已补 `04` 文档 §4.8「目录整传边界反馈」（超限文案四要素定格）与 §5 调整清单 A13，并已在源码落地。
5. **验收状态**：`npm run tauri:dev` 下的桌面真机浏览器/系统交互与读屏走查按**未验收**处理；各域文档中的验收基准（断点四档、8 状态矩阵、空态四分）保留为桌面真机回归口径。
