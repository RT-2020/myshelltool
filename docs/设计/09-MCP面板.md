# 09 · MCP 面板域设计文档

继承：docs/设计/00-全局方向与设计系统.md（无覆盖）。

> 本域覆盖：MCP 内嵌服务的控制面、执行日志审计区、暴露能力折叠区、日志过滤条、授权范围编辑器，以及 mcpApproval 高危审批弹窗。宿主两处：`mcpPanel` 弹窗与设置弹窗 MCP tab。本文档只做设计决策，不改任何代码；所有现状描述带 `path:line` 依据，对比度数字为本会话用 node 脚本按 WCAG 公式对 `_tokens.scss` 实值的确定性计算（非浏览器实测，纯色组合的数学结论不受渲染层影响）。
>
> 评估基线：master 工作区（2026-11-11）。行号以本会话实读为准，与诊断 brief 有 1–2 行偏差处一律按实读修正。

---

## 1. 域概述与现状小结

v2.9 信息架构已定格且方向正确：控制面收敛为单 surface（状态行/接入行/拦截行/授权范围行，hairline 分隔，`McpPanelContent.vue:5-10` 头注释），执行日志为审计区（双行紧凑条目 + 原位展开 + 过滤 + 分页，`McpExecutionLogList.vue:8-15`），能力清单默认折叠（`McpCapabilityList.vue:5-9`）。**本轮不推翻骨架，沿用既有布局与交互契约**。

诊断结论（引用）：token 纪律总体到位——无 `transition:all`、hover 无盒模型改动、关键交互点 `focus-visible` 已覆盖、空态区分首空/无结果、审批弹窗 Esc/关闭=拒绝（fail-closed，`useModalDismiss.ts:29-30`）。短板三条线：

1. **低对比 token 承载小字号信息文本**：`--app-subtle` 浅色 on panel 2.79 / on panel-2 2.61，连 3:1 大文本档都不达；浅色 `--warn` 3.42、`--success` 3.30（本会话 node 复算与诊断一致，算法为 oklch→sRGB→WCAG 相对亮度比）。
2. **store 反馈通道与弹窗宿主错配**：`mcp.ts` 6 处 announce 不带 level（只写状态栏不进 toast，`toastQueue.ts:36`），而面板两个宿主都是 fixed 全屏弹窗（`GlobalModals.modal.scss:5-12`，`--z-modal=400`，`_tokens.scss:174`），状态栏被遮罩覆盖——反馈不可见，最重是重置 token 失败静默回位。
3. **读盘失败被折叠成空态或宽松断言**：执行日志加载失败呈现为「暂无执行记录」（审计留痕面把「读不出」当「没有数据」）；授权范围 IPC 失败时摘要恒显示「未限制（MCP 可访问全部资产）」——安全语义上的宽松方向折叠。

另有体量约束：`McpPanelContent.vue` 461 行、`McpExecutionLogList.vue` 470 行（本会话 `wc -l` 实测；后者含尾空行，诊断 brief 记 469 行），逼近 500 行硬上限（AGENTS.md 质量红线；300 行软警告区）。**本方案的每项新增都附行数预算**，任一文件触及 500 上限时优先拆分（过滤条/详情展开已有独立组件先例）。

用户与场景（继承全局 Q1/Q2）：MCP 面板服务两类时刻——接入配置时刻（把 endpoint+token 复制给外部 MCP host，低频但一次要对）与审计/信任时刻（查看真实远程执行留痕、调整拦截等级与授权范围、处理高危审批弹窗，排查期高频）。用户是运维/后端/SRE，能理解「拦截等级」「授权范围」「入口 token」语义，不需要解释性装饰。

### 1.1 本域对象与布局选型（console 第 0 步，先写对象）

```
用户在这一屏反复处理的对象是：内嵌 MCP 服务的「接入配置 + 执行留痕」这一个服务对象
他每个小时要处理：接入期 1-2 次（复制配置/调范围）；排查/审计期 5-20 次（查日志/处理审批）
处理一次需要的动作是：看状态 → 复制/调整控制项 → 筛选查日志 → 展开一条核对 →（弹窗）确认或拒绝
这个对象现在缺什么信息：加载失败时无错误态、读取失败被显示成「不限制」、反馈 toast 被弹窗遮罩吞掉
```

页面类型判定：**workspace**。依据 `design-craft-layout-rhythm/references/布局适用范围.md` workspace 定义（后台/工作台，围绕工作对象组织一种主骨架，重复行、固定密度与滚动区是允许且常常必要的；检查对象处理路径、滚动区、空/满载状态）——执行日志正是重复行 + 独立滚动区（`.log-scroll` max-height 320px 内部滚动，`McpExecutionLogList.vue:264-269`）。不是 focused（内容远超单表单），也不是 long-form。

console 十布局选型（`design-craft-console`）：主骨架 = **时间线型（#3）**——执行日志按时间倒序（`mcp.ts:83` timestampMs 倒序）、可展开看上下文（意图/完整时间/输出摘要），符合时间线型「事件按时间发生、需要看之前发生了什么」的适用判据；其「时间戳对齐、默认收起可展开、最近事件在默认视口内」三条必须项现状均已满足（`fmtShortTime` 当天短格式 :102-111、`onRowClick` 原位展开 :75-77、倒序排列）。控制面三行与能力折叠区是该主骨架的**附属面板**（console 硬规则：一屏一种主骨架，其余是附属面板），信息顺序为「服务上下文（控制面）→ 主对象（日志）→ 参考（能力）」，自上而下与既有 v2.9 结构一致，不重排。

mcpApproval 审批弹窗单独判定为 **focused 单点核对流**（states §7.1 dialog 判据：必须先处理才能继续——审批不确认后端 oneshot 挂 60s 超时，`GlobalModals.vue:155-156` 注释），无列表无滚动区，不适用 workspace 检查项。

---

## 2. 基线诊断表

全部 14 条（1 P0 + 4 P1 + 9 P2）逐条登记。处理方式与 §3/§4 的方案条目互相引用。

| # | severity | 位置 | 问题 | 依据 | 本方案处理方式 |
|---|---|---|---|---|---|
| 1 | **P0** | `McpExecutionLogList.vue:313`（.line1 11px）、`:331`（tone-ok success 文字）、`:370-372`（decision-minimal_allowed warn pill）、`:388`（.time 11px + subtle）；`McpCapabilityList.vue:133`（cap-note subtle）；`McpLogFilterBar.vue:134`（搜索图标 subtle）；`McpPanelContent.vue:459`（level-note.is-warn warn 文字） | 小字号信息文本对比度低于 4.5:1。node 复算（本会话执行）：浅色 subtle on #fff **2.79** / on #f7f7f8 **2.61**（连 3:1 都不达）；浅色 warn **3.42**、success **3.30**；深色 subtle **3.53/3.47**（同不达 4.5） | 00 §3.3「普通正文与有信息含义的辅助小字 ≥4.5:1」；WCAG 2.1 AA | §4.1 修复规则 R1：信息小字统一 12px（`--text-xs`）下限；中性信息文字色 `--app-subtle`→`--app-muted`（复算 4.94–5.86 达标）；状态语义改「色点/图标承担颜色 + 文字用中性色」（danger 文字例外保留：浅色 4.70、深色 5.27 两主题达标） |
| 2 | P1 | `mcp.ts:164,261,263,278,305,307`（6 处 announce 无 level）；通道语义 `toastQueue.ts:36`；遮挡 `GlobalModals.modal.scss:5-12` + `_tokens.scss:174` | 重置 token 失败、拦截等级切换成功/失败、授权范围保存失败、清空日志成功/失败均只写状态栏；弹窗宿主遮罩下不可见；重置 token 失败静默回位可被误判为已重置 | AGENTS.md「失败朝宽松方向折叠」红线；组件内既有判据 `McpPanelContent.vue:82-85`（「弹窗场景必须带 level」，2026-09-22 修复注释） | §4.2 R2：六处补 level（失败类 error、成功类 success）；同 store 同形态的 `mcp.ts:205`（审批响应失败）、`:223`（审批超时）顺带补齐（error/warn） |
| 3 | P1 | `mcp.ts:292-294`（loadExecLogs catch 仅 console.warn）→ `McpExecutionLogList.vue:213` | 日志加载失败折叠为「暂无执行记录」空态，审计面无法区分真实空与读不出 | AGENTS.md「『读不出来』永远不等于『空数据』——曾致整份审计日志被覆盖」；本域正是审计日志显示面 | §3.3 R3：store 增 `execLogsError` 状态；组件增非阻塞错误条（不遮已加载内容）+ 重试；三态分支定型 |
| 4 | P1 | `mcp.ts:245-248`（loadMcpConfig catch 仅 console.warn，config 保持 null）→ `McpScopeSettings.vue:39-50` | 授权范围读取失败时摘要恒显示「未限制（MCP 可访问全部资产）」——IPC 失败（无法确认）被展示为安全宽松断言；对照读盘损坏态 denyAll 已有 fail-closed 文案（`McpScopeSettings.vue:45`），IPC 失败路径无等价处理 | AGENTS.md fail-closed 红线（「涉及审批/拦截等级的读盘失败只允许回落到更严一档」）；本组件承载审批范围可见性 | §3.2 R4：store 增 `configLoadError`；摘要增「读取失败」分支——不再显示「未限制」结论，显示「无法确认」+ 重试入口；UI 侧不得代答后端实际拦截行为（后端行为验证进 openQuestions Q1） |
| 5 | P1 | `McpScopeSettings.vue:317`（`var(--accent-contrast, #fff)`）；同款域外先例 `editor/EditorSurface.vue:363` | 引用不存在的 token：`--accent-contrast` 全仓零定义（本会话 grep 仅 2 处使用、0 处定义），fallback `#fff` 恒生效；正确 token 是 `--accent-on`（`AppButton.vue:82`、`_tokens.scss:84`） | AGENTS.md「硬编码颜色必须用 var(--token)」 | §4.2 R5：改 `--accent-on`；EditorSurface 同款是否随本域任务一并收敛列 openQuestions Q3 |
| 6 | P2 | `McpLogFilterBar.vue:84-92`（filter-clear 内容为文本「×」，:91）；`McpPanelContent.vue:189-195,221-227` 与 `McpScopeSettings.vue:127-129`（icon-act 纯图标复制钮无 aria-label，仅 title 兜底） | 图标/符号按钮可访问名弱：「×」子树文本优先于 title 参与可访问名计算 | p0-list K 组「图标按钮没有可访问名称」 | §4.2 R6：filter-clear 换 lucide `X` 图标 + `aria-label="清除全部筛选"`；三处 icon-act 补 `aria-label` |
| 7 | P2 | `McpPanelContent.vue:199-207`（armed 态后果仅存在于 :203 title；按钮保持 ghost，:206 可见文本仅「确认重置？」） | 重置 token 的破坏性后果（旧 token 立即失效、全部 host 需重配）只活在 tooltip，键盘/触屏/读屏不可见；对照同面板「复制配置」primary 强调，权重错位 | anti-AI H 组危险操作表达 | §3.1 R7：armed 时按钮 variant→danger，并在接入行下插入条件警示行（12px，warn 点 + muted 文字），3s 未确认自动撤回 |
| 8 | P2 | `McpScopeSettings.vue:179`（text-btn 单击即 `save(true)`）对照 `McpPanelContent.vue:107-122`（重置 token 两步）与 `McpExecutionLogList.vue:136-144`（清空日志两步） | 「清空全部限制」立即放宽为不限制且 announce success，零确认；同一面板三种破坏动作三种防护，最弱者恰是后果最放宽的一个 | anti-AI H 组危险操作一致性 | §3.2 R8：改内联两步确认（照清空日志范式：按钮变 danger「确认清空？」+ 取消） |
| 9 | P2 | `McpScopeSettings.vue:126`（summary ellipsis，样式 :211-218 无 title 无展开途径） | 受限组合较长时截断部分不可恢复 | 基础状态覆盖：信息被容器静默裁剪且无替代读取路径 | §3.2 R9：`:title="summary"` 兜底（与日志行 .host/.time 的 title 先例一致）；编辑器展开即完整可见 |
| 10 | P2 | `McpLogFilterBar.vue:79-83`（搜索框仅 placeholder）；`McpScopeSettings.vue:150-158`（field-label 是 span，未 label[for] 关联裸 input） | 输入无可编程关联标签，读屏只读到 placeholder | p0-list K7「placeholder 不替代标签」 | §4.2 R10：tag-input 加 id + label[for]（组件内自洽）；搜索框经 AppInput 既有 `id` prop（`AppInput.vue:64`）+ 过滤条内置视觉隐藏 label 关联（AppInput 不透传 aria-label 至 input 元素，attrs 落根 div） |
| 11 | P2 | `McpCapabilityList.vue:75-94`（Resources/Prompts 0 条只渲染「Resources · 0」+ 空 ul，:77-83/:86-92 无 else 文案） | 空态覆盖缺口；对照日志区已区分首空/无结果（`McpExecutionLogList.vue:211-213`） | 基础状态覆盖 empty 态 | §3.5：0 条渲染「暂无暴露的 X」一行，不渲染空 ul |
| 12 | P2 | `McpPanelContent.vue:151`（恒「正在探测 MCP…」）+ `mcp.ts:129-134`（refresh catch 仅 console.warn，status 恒 null） | 初始探测 IPC 失败时无限加载：无错误文案，刷新按钮只在 hasStatus 分支内渲染（`McpPanelContent.vue:153` template v-else），无恢复入口 | 基础状态覆盖 error 态缺失（低概率 IPC 异常路径，待实测确认触发面） | §3.1 R12：store 增 `probeError`；`!hasStatus && probeError` → 错误态（AlertTriangle + 四要素文案 + 刷新按钮）；未失败保持现状 loading 文案 |
| 13 | P2 | `mcp.ts:244`（`cfg?.level ?? 'minimal'` 直通未知值）→ `McpPanelContent.vue:129-130`（isMinimal 宽松渲染）+ `AppSelect.vue:33-36`（currentLabel 无匹配返回 ''）+ `:183-185`（`currentLabel || placeholder`，本域未传 placeholder → 触发器空白） | 未知/损坏拦截等级值时 select 显示空白、说明行按宽松档渲染——解析失败被宽松折叠 | AGENTS.md「解析失败按最严方向处理」 | §4.2 R13：store 对未知值 fail-closed 折叠显示为 `'strict'`（仅影响显示与说明行，不写后端；误差朝严方向可接受）；后端是否可能返回其他值未验证，列 openQuestions Q2 |
| 14 | P2 | `McpPanelContent.vue:367`（padding 4px 6px）、`:385`（11px）；`McpCapabilityList.vue:195`（11px）、`:212`（10px）；`McpExecutionLogList.vue:241,313,388,399,405,433`（11px）；`McpScopeSettings.vue:228`（padding 2px 8px）、`:259`（11px） | 间距/字号大量自定 px；`--space-*` 仅 4px 基线整档（`_tokens.scss:138-140`），5–7px 与 10/11px 无对应档；属系统性尺度缺口而非本域孤立（`AppButton.vue:71` 同款） | AGENTS.md「间距/字号用 token」；00 §1.3 D3 同类纪律 | §4.3 R14：P0 修复已覆盖全部 10/11px 字号（统一 `--text-xs`）；间距类在域任务自然触及对应文件时以 `--space-1/--space-2` 对齐（允许 ±2px 视觉微调），新增样式一律 token，不做独立重构轮 |

---

## 3. 逐界面方案定格

### 3.0 本域统一决策（各界面共用，避免重复）

- **继承声明**：token 取值与变量名、语义色彩职责表、字体角色表、hover 铁律、z-index 八档、对比度底线、动效分配、空态四分、错误四要素、键盘可达契约全部继承 00 §2/§3，无覆盖项。
- **动效盘点**：本域自绘过渡仅两处且合规——`.log-row` 与 `.load-more` 的 `transition: … var(--dur-fast) var(--ease-standard)`（`McpExecutionLogList.vue:302,421-423`，单时间位）；无持续动画（modal.scss 的 `modal-btn-spin` spinner 属 00 §1.3 D4 已归类白名单，域外宿主不改）。本方案不新增任何持续动画；补状态点/pill 不带 pulse（状态变化是离散的、非「忙碌中」）。
- **对比度核算底册**（本会话 node 复算，WCAG 相对亮度比，浅色为纯色组合的数学确定值）：
  | 组合 | 浅色 | 深色 | 判定 |
  |---|---|---|---|
  | `--app-subtle` on panel | 2.79 | 3.53 | 双主题均 <4.5，P0 根因 |
  | `--app-subtle` on panel-2 | 2.61 | 3.47 | 同上 |
  | `--app-muted` on panel | 5.29 | 5.86 | 达标，修复用色 |
  | `--app-muted` on panel-2 | 4.94 | —（同机制 ≥4.5） | 达标 |
  | `--warn` 文字 on panel | 3.42 | 7.56 | 浅色不达标 |
  | `--success` 文字 on panel | 3.30 | 7.65 | 浅色不达标 |
  | `--danger` 文字 on panel | 4.70 | 5.27 | 双主题达标（例外保留） |
  | `--app-strong`/`--app-text` on panel | ≥16 | 16.43/14.92 | 达标 |

### 3.1 界面 A：MCP 控制面（McpPanelContent.vue，双宿主）

**页面类型与依据**：workspace 主骨架的附属控制面板（console 硬规则「一屏一种主骨架，其余是附属面板」）。宿主一：`mcpPanel` 弹窗（`GlobalModals.vue:333`，max-width 580px `GlobalModals.modal.scss:31`，主按钮「关闭」:486、无「取消」:430）；宿主二：settings 弹窗 MCP tab（`SettingsPanelContent.vue:237`，modal--settings 760px + 172px 左导航，`GlobalModals.modal.scss:101-105`）。两宿主零 props 复用（`SettingsPanelContent.vue:14` 头注释），方案对两宿主同时生效。

桌面 ASCII 线框（阅读顺序按数字；示例值为示意）：

```
┌ mcpPanel 弹窗（max 580px）/ settings·MCP tab（内容列）──────────────┐
│ MCP 服务管理                                                    [×] │
├────────────────────────────────────────────────────────────────────┤
│ ① 状态行（.s-status，--space-3 padding）                            │
│   ● MCP 服务正常                      v0.21.3   [刷新]              │
│     HTTP 健康检查通过 · 探测于 14:32:05    （muted 12px）           │
│   ── 不可用分支：●(danger) + 「MCP 服务不可用」+ 排查提示两行 ──    │
│   ── 加载分支：正在探测 MCP…；错误分支(R12)：警示点 + 文案 + [刷新] ──│
│ ─────────────────────────────── hairline（--app-border-soft）────── │
│ ② 接入行                                                            │
│   ▸ 接入配置 http://127.0.0.1:41235/mcp（mono,省略）                │
│        [复制endpoint] [复制配置(primary)] [重置 token(ghost)]       │
│   ── armed(R7)：按钮变 danger「确认重置？」，行下插警示：           │
│      ●(warn) 旧 token 将立即失效，所有已配置的 MCP host 需重新复制   │
│      配置；3 秒内再次点击生效。                                      │
│   ── 展开（config-toggle）：JSON 代码块（--terminal-bg 表面）+      │
│      贴置说明 + 数据目录行 + [复制] ──                              │
│ ─────────────────────────────── hairline ────────────────────────── │
│ ③ 拦截行                                                            │
│   (盾形图标) 危险命令拦截    [仅拦截毁灭性命令（默认）▾]            │
│   ●(warn) 其余命令与文件操作直接执行并记入日志；敏感凭据读取仍需     │
│     确认。（muted 12px；strict 档为 muted 点 + muted 文字）         │
│ ─────────────────────────────── hairline ────────────────────────── │
│ ④ 授权范围行（界面 B，McpScopeSettings 自包含）                     │
│   (盾勾图标) 授权范围  2 个分组 · 1 个标签 · 排除 1 台 · 本机文件未开放│
├────────────────────────────────────────────────────────────────────┤
│ ⑤ 执行日志区（界面 C）    ⑥ 暴露能力折叠区（界面 E）                │
│                          〔关闭〕（仅 mcpPanel 弹窗宿主有此主按钮）  │
└────────────────────────────────────────────────────────────────────┘
```

区块任务表：

| 区块 | 承担任务 | 形式 |
|---|---|---|
| 状态行 | 第一眼回答「MCP 现在能不能用」；版本与探测时间是信任依据 | 8px 状态点（success/danger，`McpPanelContent.vue:339-346`）+ 状态词 + 副标题；刷新 = AppButton ghost + loading |
| 接入行 | 把带 token 的 endpoint 交给外部 host；泄露时重置 token | 折叠热区（chevron+label+endpoint）+ 两级复制 + 两步确认的危险按钮 |
| 拦截行 | 让用户知情当前摩擦档位与其后果 | AppSelect 两档 + 一行动态说明（R1 修复后：warn 点承担色，文字 muted） |
| 授权范围行 | 知情当前范围 + 进入编辑 | 摘要（R9 加 title 兜底）+ 编辑入口 |

语义 token 引用：surface = `--app-panel` + `--radius-md` + `--app-border`（`:283-291`）；行分隔 `--app-border-soft`（`:292-294`）；行内详情灰底 inset = `--app-panel-2`（`:404-412`，与外框 hairline 区分层级）；配置 JSON 与输出代码块 = `--terminal-bg/--terminal-text`（`:416`；`McpExecutionLogList.vue:453` 同）——属 00 §2.2「必须像终端的表面」白名单先例（open-design-prompt.md:460），保留。间距 `--space-1..4` 为主。**密度：紧凑档**（s-row padding 8/12 → 行高约 32px，符合 00 §2.4 紧凑档 28–32）。

字体角色：状态词 = 正文 600（`--text-sm`，`:312-316`）；副标题/说明 = 标签档 12px muted（`:317-321,:453-458`）；endpoint/数据目录/JSON = 数据档 `--font-mono`（`:378-387,:413-429`），endpoint 从 11px 升 `--text-xs` 并补 `tabular-nums`。

状态矩阵（8 基础状态）：

| 状态 | 处理 |
|---|---|
| 默认 | 三行 surface + 授权范围行；状态点承担状态色，外框中性（组件既有设计约束，`:16-17` 头注释） |
| hover | `config-toggle`→`--app-hover`（:376）、`icon-act`→strong+hover（:401）、AppButton/AppSelect 既有——全部纯绘制属性，合规 |
| focus-visible | `config-toggle` 显式 focus-ring（:377）；其余吃全局兜底（`_base.scss:76-79`） |
| active（按压） | 自绘 `config-toggle`/`icon-act` 补 `:active { background: var(--app-active) }`（00 §1.2 结论 1 的域内落地）；AppButton/AppSelect 随全局补齐，不在本域单独造 |
| disabled | `icon-act` 复制钮在 endpoint 为空时不渲染（:190/:222 v-if），无需 disabled 形态；「重置 token」busy 期用 loading 非 disabled |
| loading | 刷新 = AppButton `:loading`（spinner 占位、宽度不跳，`AppButton.vue:121-128`）；初始 `!hasStatus` → 「正在探测 MCP…」保留现状（本地 IPC 秒内返回，150ms 阈值下闪现代价低，states §1.2），**新增**：探测失败错误态（R12） |
| empty | N/A——控制面无「列表为空」概念；endpoint 为空显示「—」占位符属字段级兜底（`:187`） |
| error | **新增（R12）**：`!hasStatus && probeError` → 状态行区域显示 AlertTriangle + 四要素文案「MCP 状态读取失败（无法确认服务是否可用）：<原因>。点「刷新」重试；若刚启动应用请稍候。」+ 刷新按钮（恢复入口移出 hasStatus 分支） |

空态四分：本界面无列表空态；字段级「—」占位与 P2 列表空态归界面 C/E。错误文案四要素落位见上表 error 行。

特色与克制：无自设特色。状态色只由 8px 点承担（外框中性是组件既有设计约束并保留）；`--terminal-*` 仅代码块表面（白名单内）；accent 仅「复制配置」primary 一处 + focus-ring，面积 <5%。

非目标：不改探测机制（probe.rs HTTP 健康检查语义）；不改双宿主复用结构；不把控制面拆出弹窗。

验收方式：① 键盘：Tab 顺序 = 状态行刷新 → 接入行折叠热区 → 复制钮 ×2 → 复制配置 → 重置 token → 拦截 select → 授权范围编辑钮；全部焦点环可见（focus-ring 或全局兜底）；Esc 关面板。② 对比度：修复后副标题/说明行/endpoint 逐项复测 ≥4.5:1（底册见 §3.0）。③ 断点：两宿主均为弹窗，不触达主窗断点折叠机制（那是 sidebar/right 折叠，`useAdaptiveLayout.ts` 管）；弹窗宽 100% 随窗口缩窄，`<1280/<1024/<860` 下弹窗仍 ≥ 800-2×margin 宽，三行 flex 布局靠 `min-width:0` + endpoint ellipsis（`:379-386`）收缩，AppSelect `min-width:200px`（`:449-452`）在 <560px 内容宽时允许换行（s-row 需补 flex-wrap——现状无 wrap，**列入 R7 同文件落地时顺带补**，依据：最小 800px 窗宽下 mcpPanel 弹窗恒 580px、内容列 548px（580 − modal-body 左右 padding 32px，`GlobalModals.modal.scss:31,:84-87`），拦截行 label+select 仍可容纳，wrap 仅作兜底）。④ tauri:dev 真桌面走查双宿主各一遍。

### 3.2 界面 B：授权范围编辑器（McpScopeSettings.vue）

**页面类型与依据**：workspace 附属面板内的行内展开编辑器（inline-first——00 §3.4「弹窗选型先 inline 后打断」，范围编辑不是「不打断就没法继续」的场景，现状行内展开正确，保留）。自包含组件（`McpScopeSettings.vue:3` 头注释），嵌于控制面 surface 第四行。

ASCII 线框（摘要行见界面 A ④；下方为编辑态）：

```
│ (盾勾图标) 授权范围  未限制（MCP 可访问全部资产）     [设置…]  ←1 摘要行
│ ┌ 编辑态（--app-panel-2 inset，--radius-sm）──────────────────┐
│ │ 分组与标签为并集（命中任一即允许）；两者都留空 = 不按组/标签  │ ←2 说明
│ │ 限制。完全清空所有项 = 不限制任何资产。          (muted 12px)│
│ │ 允许的分组（2/5）                                            │ ←3 check-list
│ │ [□ 生产] [□ 运维] [□ 数据库]…（120px 限高滚动）             │
│ │ 允许的标签（逗号分隔）                          ←label[for]  │ ←4 R10
│ │ [如 prod, 运维____________________]                          │
│ │ 现有标签：prod、运维、staging                  (muted)       │
│ │ 显式排除的资产（优先级最高，1 台）                           │ ←5 check-list
│ │ [□ web-01（192.168.1.10）]…（120px 限高滚动；无资产时空态文案）│
│ │ [□] 允许 MCP 触及本机文件系统（仅在配置任一限制时生效…）     │ ←6
│ │ [清空全部限制]                    [取消] [保存(primary)]     │ ←7 R8 两步
│ └─────────────────────────────────────────────────────────────┘
```

区块任务表：

| 区块 | 承担任务 | 形式 |
|---|---|---|
| 摘要行 | 不展开即知当前范围；denyAll 有专属恢复指引 | ellipsis + title 兜底（R9）；denyAll 显示 danger 色（`:219` 保留） |
| 分组/标签/排除三字段 | 表达「并集 + 排除优先 + 本机 FS 附加」的范围模型 | 勾选列表 + 逗号输入 + 提示行（现有标签回显防拼错） |
| 动作区 | 保存 / 放弃 / 危险放宽 | text-btn ×2 + primary-btn；「清空全部限制」改两步确认（R8） |

语义 token 引用：编辑器 inset = `--app-panel-2`（`:240`）；check-list 边框 `--app-border-soft` + `--radius-sm`（`:267-269`）；tag-input = `--app-panel` 底 + `--app-border`（`:286-294`）；primary-btn = `--accent`/`--accent-hover` + **`--accent-on`（R5 修正，废除 `--accent-contrast`）**；text-btn hover = `--app-hover` + strong（`:312`）。间距 `--space-1..3`。

字体角色：field-label = 标签档 12px 600 strong（`:252-256`）；说明/提示 = 正文 muted 12px（`:242-246`）；资产 label 含 host 属数据（现状裸文本，保留——括号内 host 已足够辨识，不强改 mono 以控制行数预算）。

状态矩阵：

| 状态 | 处理 |
|---|---|
| 默认 | 摘要行（editing=false）；摘要文案四分支：denyAll / 受限 / 未限制+FS 开 / 未限制（`:44-57`） |
| hover | icon-act/text-btn/primary-btn 既有 hover，纯绘制合规（`:231,:312,:324`） |
| focus-visible | tag-input 显式（`:295`）；三个自绘按钮吃全局兜底 |
| active | 三个自绘按钮补 `:active`（primary 用 `--accent-active`、text-btn 用 `--app-active`，00 §1.2 结论 1） |
| disabled | saving 时三按钮 disabled + opacity（`:313,:325`，既有） |
| loading | 保存中 = 按钮文字「保存中…」+ disabled（`:183`，既有；本地 IPC 快，不升 spinner） |
| empty | 分组空态已有（`:146`「尚无分组…」）；**新增**：排除资产列表空时补「还没有连接资产——先在侧栏添加主机」（首空四分：一句为什么空 + 指向主操作位置，不给假按钮） |
| error | 保存失败 → store announce error（R2，toast 在弹窗宿主可见）；**新增（R4）**：`configLoadError` 时摘要行显示「授权范围读取失败（无法确认是否受限）——后端实际拦截以后端判定为准；[重试]」。**规则：读取失败态下绝不显示「未限制」类宽松结论**；且该态下仍可进入编辑（以表单初值为空的安全集），保存成功即恢复显示。denyAll 分支文案保留（`:45`，读盘损坏 fail-closed 先例） |

空态四分落位：首空=无分组（既有）/无资产（新增）；清空/搜索无结果=不适用（勾选列表无过滤）；权限=本地单用户无权限模型（00 §3.5 定格无消费面）。denyAll 不是空态，是配置态，专属文案已存在。

特色与克制：无。danger 仅 denyAll 摘要文字（4.70/5.27 达标，保留）与两步确认按钮。

非目标：不改后端 scope 模型（并集/排除优先/fail-closed 判定）；不改「deny_all 不收 UI 通道」约定（`mcp.ts:79-80` 注释）。

验收：① 键盘：编辑态 Tab 顺序 = 三字段 → 动作区；勾选项为原生 checkbox 可空格切换；清空两步确认中 Esc 收回 armed。② 对比度：摘要 muted 4.94+ 达标。③ 断点：编辑态在两宿主内容列（548px / 约 588px，即 760 − 172 导航列，`GlobalModals.modal.scss:101-105`；子组件头注释保守口径 ~370-500px，`McpExecutionLogList.vue:8`）下 check-list 自带 120px 限高滚动（`:265-266`），无横向溢出；tag-input 满宽。④ tauri:dev 走查「受限组合最长态」（多分组+标签+排除+FS 开）摘要 title 完整可读。

### 3.3 界面 C：执行日志审计区（McpExecutionLogList.vue）

**页面类型与依据**：本域主骨架——console 时间线型（§1.1 选型）；审计区。区块头 + 过滤条 + 限高滚动列表 + 分页脚注，四段式结构保留。

ASCII 线框（阅读顺序）：

```
│ ▤ 执行日志  真实远程执行留痕 · 保留 30 天      [刷新] [清空]  ←1 区头
│（清空两步：[确认清空?(danger)] [取消]）                        │
│ ┌ 过滤条（界面 D）──────────────────────────────────────┐    ←2
│ └────────────────────────────────────────────────────────┘    │
│ ┌ .log-scroll（max-height 320 内部滚动）─────────────────┐    │
│ │ ┌ 条目 ────────────────────────────────────────────┐  │    ←3
│ │ │ ●成功  执行命令  web-01          [放行]  14:32:05 │  │    │
│ │ │   systemctl restart nginx        （mono 省略）    │  │    │
│ │ └──────────────────────────────────────────────────┘  │    │
│ │ ┌ 展开态（点击条目原位展开）────────────────────────┐  │    ←4
│ │ │ 2025-11-11 14:32:05 · 意图：重启服务 · user@host  │  │    │
│ │ │ ▍输出摘要（--terminal-bg 代码块，max 200px）       │  │    │
│ │ └──────────────────────────────────────────────────┘  │    │
│ │ …（最多渲染 PAGE_SIZE=50 条）                         │    │
│ └────────────────────────────────────────────────────────┘    │
│            [加载更多（已显示 50 / 128 条）]                    │ ←5
│  无匹配：「没有匹配的记录——调整关键词或筛选，或点 × 清除全部」 │ ←6
│  首空：「还没有 MCP 工具执行记录…」                            │
│  错误条(R3)：警示点 执行日志加载失败…[刷新]                    │
└───────────────────────────────────────────────────────────────┘
```

区块任务表：

| 区块 | 承担任务 | 形式 |
|---|---|---|
| 区头 | 标明审计属性 + 保留期 + 刷新/清空 | 小写字距标签（chrome 区块头，`:225-236` uppercase+letter-spacing） |
| 过滤条 | 条件收集（执行在父组件） | 见界面 D |
| 条目列表 | 每条 = 结果/工具/主机/决策/时间 + 命令；展开核对输出 | 双行紧凑条目（`:8-10` 设计注释：容器仅 ~370-500px 宽，6 列表格会折行故用双行——该约束正确，保留） |
| 分页脚注 | 1000 条上限下 DOM 恒 ≤50 | 「加载更多」pill + 条数收尾（`:203-208`） |

语义 token 引用：条目 = `--app-panel` + `--app-border` + `--radius-sm`（`:280-289`），展开态边框 strong（`:286-288`）；详情块 = `--app-panel-2`（`:437-446`）；输出代码块 = `--terminal-bg/--terminal-text`（`:453,:460-467`，白名单表面）；决策/结果 pill = `color-mix(currentColor 10%)` 底（`:358-365`，机制保留）；滚动容器 320px（`:264-269`）。

字体角色：主机 = 正文 12px（`.host` `--text-xs`，`:347-355`）；命令/时间 = 数据档 mono（`.cmd`/`.mono`），**时间列补 `font-variant-numeric: tabular-nums`**（00 §2.1 强化第 3 条「时间戳必须 mono+tabular」的域内落地）；全部从 11px 升 `--text-xs`（R1）。

**R1 的条目内落位**（逐元素）：

| 元素 | 现状 | 修复后 |
|---|---|---|
| `.line1` 行 | 11px（:313） | `--text-xs`（12px） |
| 结果「成功」tone-ok | success 色文字 11px（浅 3.30 不达） | 文字 muted；**色移到既有 6px `outcome-dot`**（`:325-330` 已存在，currentColor 机制天然跟随）；「失败」tone-err 保留 danger 色文字（4.70/5.27 达标）+ danger 点 |
| 决策 pill「放行」 | warn 色文字 11px（浅 3.42 不达） | pill 文字 muted；pill 内前置 6px warn 点（复用 outcome-dot 形制）；danger 组 pill 同理（danger 点 + muted 文字，统一形制；danger 色文字在 pill 10% 透明底上未单独测算，统一收敛到「点承担色」避免逐底复测） |
| `.time` | 11px + subtle（浅 2.79） | `--text-xs` + muted + tabular（完整时间已有 title 兜底 `:183`） |

状态矩阵：

| 状态 | 处理 |
|---|---|
| 默认 | 列表 + 过滤条 + 脚注 |
| hover | 条目 `--app-hover`（`:304-306`）、加载更多 accent 描边（`:425-428`）——纯绘制合规 |
| focus-visible | 条目/按钮均为真实 button，吃全局兜底 |
| active | 条目补 `:active { background: var(--app-active) }`（00 §1.2 结论 1 域内落地） |
| disabled | N/A（无禁用交互） |
| loading | 首载 `execLogsLoading && !rows.length` →「正在加载执行记录…」（`:148`）；**刷新已有内容保留旧列表**（`:148` 条件保证），按钮 spinner 表达进行中——符合 states §1.1「清空重来比不动更糟」判据，保留 |
| empty | 首空（`:213`「暂无执行记录」）文案方向升级：「还没有 MCP 工具执行记录。从 MCP host 调用一次工具后，这里会留下完整留痕」——复用用户视角的词、说明来源，审计区无创建动作故**不放主操作按钮**（空态铁则「1 个主操作」的合法零按钮变体：此处任何按钮都是假指引）；清空后回到此首空文案（announce success 已由 R2 补）；无结果（`:211`）文案升级：「没有匹配的记录——调整关键词或下方筛选，或点 × 清除全部条件」（回显条件 + 指向清除入口，00 §3.5 清空/搜索两类在此共用一形但文案覆盖两义，条件本身在过滤条可见） |
| error | **新增（R3）**：`execLogsError` 非空时在区头下渲染非阻塞错误条（不遮已加载内容，00 §3.6 呈现层级）：「执行日志加载失败（已有记录不受影响）：<原因>。[刷新]」；rows 为空且失败时该错误条**替代**「暂无执行记录」空态（错误态优先于空态——禁止把读不出呈现为没有数据） |

行数预算：R3 + 模板 4 行/脚本 2 行/样式 2 行 ≈ 478 行 < 500；若连带 R1/active 态逼近 500，先拆详情展开块（先例：过滤条已拆出，`:4-5` 头注释「主文件贴近 500 行 SFC 红线」）。

非目标：不改分页/过滤执行归属（条件收集在子组件、过滤在父组件的既有分工，`:4-7` 头注释）；不改保留 30 天/上限 1000 的后端事实。

验收：① 键盘：Tab = 刷新 → 清空（两步）→ 搜索 → ×清除 → 三下拉 → 条目（Enter 展开/收起）→ 加载更多；展开态焦点不丢。② 对比度：修复后 line1/time/pill 文字全部 ≥4.5:1（muted 底册）。③ 滚动：320px 内部滚动不带动弹窗整体（AppModal body 是唯一外层滚动容器，`McpPanelContent.vue:273-274` 注释）。④ 断点：条目 line1 在窄内容列下靠 `min-width:0` + spacer 逐项省略（`.host` flex 0 1 auto、`.time`/pill `flex 0 0 auto` 恒显）；最小 800px 窗宽下 mcpPanel 内容列 548px、settings 内容列约 588px（组件头注释保守口径 ~370-500px，`McpExecutionLogList.vue:8`）——`<1280/<1024/<860` 弹窗宽度随窗，与现状约束一致，无新增降级需求。⑤ tauri:dev 走查三态（首空/无匹配/错误条——错误条需临时断 IPC 才能真触发，不可造时按「浏览器验证未执行」口径如实记录）。

### 3.4 界面 D：日志过滤条（McpLogFilterBar.vue）

**页面类型与依据**：workspace 主骨架的固定过滤头（console「固定头与滚动区」：过滤条固定在滚动容器外，滚动时不丢过滤上下文——现状注释 `McpExecutionLogList.vue:158-159` 已实现该判据，保留）。纯条件收集组件（`:2-7` 头注释），无自身异步。

ASCII 线框：

```
│ [搜索图标(muted)] [搜索命令/主机/资产/意图____________ [×清除]] │ ←1
│ [全部工具 ▾] [全部资产 ▾] [全部结果 ▾]   （flex-wrap 兜底）    │ ←2
```

区块任务表：

| 区块 | 承担任务 | 形式 |
|---|---|---|
| 搜索行 | 关键字命中命令/主机/资产/意图（`:54-59`） | AppInput + 前置图标 + 条件显示的清除钮 |
| 三下拉 | 工具/资产/结果维度过滤；选项由数据实时推导（`:42-61`） | AppSelect ×3，恒有「全部 X」头选项（`:40,:60,:63-68`） |

语义 token 引用：图标色 muted（R1 修复，≥3:1 UI 组件档）；清除钮 hover danger 色 + `--app-hover`（`:160-163`，保留）；下拉选项 hover `--app-hover`（AppSelect.vue:296-297）。间距 `--space-2`。

字体角色：placeholder/选项 = 正文/标签档（AppInput/AppSelect 既有）。

状态矩阵：默认/hover/focus-visible/active/disabled 全部由 AppInput/AppSelect 基础组件承担（00 §1.2 矩阵），本组件不另造；loading = N/A（选项同步推导，无异步不造 loading）；empty = N/A（三下拉恒有「全部」头，空集守卫无消费面——00 §1.2 结论 2「勿提前抽象」）；error = N/A（过滤是纯前端计算）。

修复落位：R6（× → lucide X + aria-label）；R10（AppInput 传 id + 组件内置视觉隐藏 label「搜索执行日志」关联——AppInput 的 attrs 落根 div 不落 input，故走 id+label 通道，`AppInput.vue:64` 的 `:id` 透传是既有能力）。

空态四分：本界面无空态（下拉恒非空）；「过滤无结果」的呈现归界面 C 空态分支。

特色与克制：无。图标仅 lucide（Search/X，AGENTS.md §4.1 图标纪律）。

非目标：不把过滤执行移入本组件；不改为 AppSelect 增 disabled（无消费面）。

验收：① 键盘：搜索框 focus 环可见；× 清除钮 Tab 可达且 Enter 生效；三下拉走 AppSelect 既有 ARIA（listbox/activedescendant，`AppSelect.vue:173-208`）。② 有任一条件时 × 才出现（`:84-92` v-if 保留——常驻占位铁律不适用于**条件渲染的功能钮**，与 hover 显隐是两回事）。③ 断点：filter-row flex-wrap（`:166-170`）在窄列折为两行，验收 <860 极限档无横向溢出。④ 读屏抽查：× 读「清除全部筛选」、搜索框读 label 文本。

### 3.5 界面 E：暴露能力折叠区（McpCapabilityList.vue）

**页面类型与依据**：workspace 附属面板的参考折叠区（disclosure）。定位是「纯参考信息，不该与状态/日志争抢首屏」（`:5-8` 头注释）——默认折叠正确，保留。

ASCII 线框：

```
│ ▸ (块形图标) 暴露能力    11 工具 · 2 资源 · 3 诊断  ←1 折叠头（aria-expanded）
│ ── 展开 ▼ ──                                                  │
│ ┌ .cap-scroll（max 260px 内部滚动）──────────────────────┐   │
│ │ 高危（需审批）                      （danger 标签）      │   │ ←2
│ │ ▍exec_command  在主机上执行任意命令…                    │   │
│ │ 只读（自动放行）                    （muted 标签）      │   │
│ │ ▍list_assets   列出全部连接资产…                        │   │
│ │ （两组皆空：「探测尚未返回任何工具声明」）               │   │
│ └─────────────────────────────────────────────────────────┘   │
│ Resources · 2                    Prompts · 3                  │ ←3 两列
│ [uri/名称列表]                  [名称(参数) 列表]              │
│ （各自 0 条：「暂无暴露的资源」/「暂无诊断模板」）             │
```

区块任务表：

| 区块 | 承担任务 | 形式 |
|---|---|---|
| 折叠头 | 不展开即知暴露面规模 | chevron + 计数摘要（cap-note） |
| 工具清单 | 分高危/只读两组，左边线承担语义色 | 限高滚动列表（`border-left 2px`，`:184-186`——「语义色只由左边线承担」的 v2.7 约束保留） |
| Resources/Prompts 两列 | 参考清单 | 1fr 1fr grid（`:199-205`） |

语义 token 引用：折叠容器 panel-2、展开后 panel（`:103-109`）；工具条目 panel-2 + 左边线（`:180-186`）；分组标签 danger/muted 文字（`:170-171`——**标签文字 12px danger 浅色 4.70 达标可保留；muted 达标**）。cap-note subtle→muted（R1）。

字体角色：工具名 = 数据档 mono 12px 600（`:187-192`）；描述 = muted 12px（11px 升 `--text-xs`，R1/:195）；sub-label = 10px 升 `--text-xs` 12px（`:212`，小写字距标签档）；uri = mono（`:230-235`）。

状态矩阵：默认=折叠；hover=cap-head（`:127`）；focus-visible=显式（`:128`）；active=cap-head 补 `--app-active`；disabled/loading=N/A（props 同步传入，无异步）；empty=**新增三处**：两组工具皆空 → 「探测尚未返回任何工具声明」（此时通常伴随服务不可用，上层状态行已报错，此处一句即够）；Resources/Prompts 各自 0 条 → 「暂无暴露的资源」「暂无诊断模板」，不渲染空 ul（R11/P2-6）；error=N/A（跟随上层状态表达，不重复报错——错误只报一次原则）。

空态四分：首空=工具 0 条（探测未返回）/资源/诊断 0 条；清空/搜索无结果=不适用；权限=不适用。文案遵守「复用用户刚看的词」（cap-note 已用「工具/资源/诊断」三词，空态沿用同词）。

特色与克制：无。danger 仅高危组左边线与标签（描边形态，00 §4.3 状态色白名单内）。

非目标：不改高危/只读分组判据（`tag === 'dangerous'`，`:33-34`）；不改两列 grid。

验收：① 键盘：折叠头 Enter/Space 切换 + aria-expanded 正确翻转；展开后 Tab 进入工具列表阅读。② 对比度：cap-note muted 达标。③ 断点：两列 grid 在最窄内容列（548px）各约 262px、在子组件头注释保守口径下限（370px，`McpExecutionLogList.vue:8`）各约 173px，uri `word-break: break-all`（`:233`）防溢出；<860 无新增处理。④ 行数预算：新增空态文案 +6 行 → 252 行，远离红线。

### 3.6 界面 F：mcpApproval 高危审批弹窗（GlobalModals.vue 域外共享宿主）

**页面类型与依据**：focused 单点核对流（§1.1）。dialog 判据成立：审批未决时后端 oneshot 挂起，60s 超时自动拒绝（`mcp.ts:74-75` 注释；前端 65s 对齐清理 `:213-227`）——「不打断就没法继续」的真实场景。

ASCII 线框：

```
┌ MCP 高危操作审批 ──────────────────────────────────── [×] ┐
│ 外部 AI 客户端（如 ZCode）请求执行高危命令，请确认三段     ←1 引导
│ 信息是否一致：
│ AI 声明意图   重启 nginx 服务                             ←2 三段核对
│ 真实命令      sudo systemctl restart nginx   （mono 折行）│   （context-grid
│ 后果预测      远程服务将短暂中断约 2 秒                   │    dt/dd 两列）
│ 核对意图与命令是否相符后再确认。拒绝或关闭都会阻止执行。   ←3 fail-closed 提示
│                            [拒绝执行(danger)] [确认执行]   ←4 Esc/×/关闭=拒绝
└───────────────────────────────────────────────────────────┘
```

区块任务表：

| 区块 | 承担任务 | 形式 |
|---|---|---|
| 引导句 | 说明事件来源与核对要求 | muted 正文 |
| 三段核对 | 意图/真实命令/后果逐项对照（域混淆攻击的防线） | dt/dd 网格（`:312-316`），命令 mono 折行 |
| fail-closed 提示 | 明示「拒绝或关闭都会阻止执行」 | muted 正文（`:317`） |
| 动作区 | danger「拒绝执行」（`:428`）+ 确认 | 拒绝为 danger 主按钮位——危险路径显性化 |

语义 token 引用：`.context-grid` dt muted 12px（`modal.scss:200`）；`.num` mono（`:221`）；`.btn.danger` = `--danger` 实色 + accent-on 文字（`modal.scss:173-177`）。

状态矩阵：默认=三段静置待核对；hover=btn 既有（`modal.scss:178-181`）；focus-visible=全局兜底；active=随全局补齐；disabled=submitting（`:428`）；loading=N/A（确认/拒绝是 fire-and-forget invoke，弹窗即时关，无长等待不造 spinner）；empty=N/A（approvalPrompt 为 null 时弹窗不存在）；error=审批响应失败 → announce error（R2 顺带 `mcp.ts:205`）；65s 超时 → announce warn + 自动关闭（`:219-226`，超时=不执行，fail-closed 语义保留）。

**定格保留的既有契约**（本域不改动，只登记为验收基线）：Esc=拒绝（`useModalDismiss.ts:29-30`）；×/关闭=拒绝（`GlobalModals.vue:155-158`）；拒绝后 `resolveMcpApproval(id,false)` 回传后端（`:238-240`）。

诚实登记：GlobalModals 宿主无焦点 trap/焦点还原实现（本会话 grep `focus|Escape` 于 GlobalModals.vue 无命中；Esc 由 useModalDismiss 注册 window keydown）。这与 00 §3.4「AppModal 是浮层键盘可达标准」存在宿主级差距，属域外共享宿主的存量状态——是否补 trap 属全局宿主改造，列 openQuestions Q4；本域验收仅要求 Esc/Tab 可达、焦点顺序与视觉一致。

空态四分/特色与克制：均 N/A（审批是中断流，无空态；无自设特色）。

非目标：不改审批三段字段与 fail-closed 语义；不把审批迁出 GlobalModals（简单确认类进 GlobalModals 是 AGENTS.md §4.1 双轨判据的正例）。

验收：① 键盘：Esc=拒绝且后端收到 false；Tab 两钮可达。② 读屏：三段 dt/dd 关联可读；danger 按钮文本「拒绝执行」自含语义。③ 断点：弹窗 580px 上限 + `max-height calc(100vh-80px)`（`modal.scss:32`），命令折行 `word-break:break-all`（`:314`）在 800px 最小窗宽下不溢出。④ tauri:dev 真审批流走查一次（需 MCP host 环境，不可造时如实记录未验）。

---

## 4. 主要调整清单

每条：改什么 / 为什么 / 依据 / 影响文件。落地时统一跑 `npm run build`（含 lint:facts、lint:size）+ `npm run test:ui` + tauri:dev 走查（00 §5.2）。

| # | 改什么 | 为什么 | 依据 | 影响文件 |
|---|---|---|---|---|
| R1 | 信息小字统一 12px（`--text-xs`）：.line1/.time/.cmd/.mono/.head-note/.list-end/.tool-desc/.sub-label/.endpoint 等全部 10/11px→`--text-xs`；`--app-subtle` 承载的信息文字换 `--app-muted`；warn/success 状态语义改「6px 点承担色 + 文字 muted」（danger 文字例外保留，双主题 4.70/5.27 达标）；时间列补 `tabular-nums` | 消除 P0：subtle 浅色 2.61–2.79 连 3:1 都不达；浅色 warn/success 文字 3.30–3.42 <4.5 | 00 §3.3、§2.2（状态色只作点/徽标/描边）、§2.1 强化第 3 条；本会话 node 复算底册 | McpExecutionLogList.vue、McpCapabilityList.vue、McpLogFilterBar.vue、McpPanelContent.vue |
| R2 | `mcp.ts` 六处 announce 补 level：164/263/278/307→error，261/305→success；顺带 205→error、223→warn | 弹窗宿主遮罩下无 level 反馈不可见；重置 token 失败静默回位可误判为成功 | AGENTS.md fail-closed 红线；组件内既有判据 `McpPanelContent.vue:82-85`；`toastQueue.ts:36` 通道语义 | src/stores/mcp.ts |
| R3 | store 增 `execLogsError`（loadExecLogs catch 置错、成功清空）；组件增非阻塞错误条 + 重试；rows 空且失败时错误条替代空态 | 审计面不得把「读不出」呈现为「没有数据」 | AGENTS.md「读不出来≠空数据」红线；00 §3.6 呈现层级 | mcp.ts、McpExecutionLogList.vue（行数预算见 §3.3） |
| R4 | store 增 `configLoadError`；摘要增「读取失败（无法确认是否受限）」分支 + 重试；该态禁止显示「未限制」类宽松结论 | IPC 失败被展示为安全宽松断言；denyAll（读盘损坏）已有 fail-closed 先例而 IPC 失败路径缺失 | AGENTS.md fail-closed 红线；对照 `McpScopeSettings.vue:45` | mcp.ts、McpScopeSettings.vue |
| R5 | `--accent-contrast, #fff` → `--accent-on` | 引用了不存在的 token，fallback 硬编码恒生效 | AGENTS.md 禁硬编码颜色；`AppButton.vue:82`、`_tokens.scss:84` | McpScopeSettings.vue（EditorSurface.vue:363 同款，见 openQuestions Q3） |
| R6 | filter-clear 文本「×」→ lucide `X` 图标 + `aria-label="清除全部筛选"`；三处 icon-act 复制钮补 aria-label（复制 Endpoint/复制数据目录路径/编辑授权范围） | 「×」子树文本优先于 title 参与可访问名；图标钮有名但不该依赖 hover 发现 | AGENTS.md 图标统一 lucide；p0-list K 组 | McpLogFilterBar.vue、McpPanelContent.vue、McpScopeSettings.vue |
| R7 | 重置 token armed 态：按钮 variant→danger + 行下插条件警示行（12px，warn 点 + muted 文字，四要素文案），3s 超时撤回机制不变；s-row 补 flex-wrap 兜底 | 破坏性后果只活在 title，键盘/读屏不可见；与同面板「复制配置」primary 强调权重错位 | anti-AI H 组；00 §3.6 四要素；状态切换非 hover 显隐，不违反常驻占位铁律 | McpPanelContent.vue |
| R8 | 「清空全部限制」改内联两步确认（照清空日志范式：变 danger「确认清空？」+ 取消） | 同面板三种破坏动作三种防护，最弱者恰是后果最放宽的 | anti-AI H 组一致性；`McpExecutionLogList.vue:136-144` 先例 | McpScopeSettings.vue |
| R9 | 摘要 `:title="summary"` | 长组合截断不可恢复，title 是最小兜底（与 .host/.time 先例同形） | 信息静默裁剪须有替代读取路径 | McpScopeSettings.vue |
| R10 | tag-input 加 id + `label[for]`；搜索框经 AppInput `id` prop + 过滤条内置视觉隐藏 label「搜索执行日志」 | placeholder 不替代可编程标签；AppInput attrs 落根 div，须走 id+label 通道 | p0-list K7；`AppInput.vue:64` 既有 `:id` 透传 | McpScopeSettings.vue、McpLogFilterBar.vue |
| R11 | Resources/Prompts 与工具组三处空态文案；排除资产空列表文案 | 0 条渲染空 ul 无「暂无」文案；排除资产列表无空分支 | 基础状态覆盖 empty 态；00 §3.5 空态四分 | McpCapabilityList.vue、McpScopeSettings.vue |
| R12 | store 增 `probeError`；`!hasStatus && probeError` → 错误态文案 + 刷新按钮（恢复入口移出 hasStatus 分支） | 初始探测 IPC 失败无限「正在探测」，无恢复入口 | 基础状态覆盖 error 态；00 §3.6 四要素 | mcp.ts、McpPanelContent.vue |
| R13 | `loadMcpConfig` 对非 `'strict'` 的已知外值 fail-closed 显示折叠为 `'strict'`（仅显示层；不写后端；代码注释登记平台假设） | 未知值导致 select 空白 + 宽松说明——解析失败须按最严方向折叠 | AGENTS.md「解析失败按最严方向处理」；`AppSelect.vue:33-36,:183-185` 机制 | mcp.ts |
| R14 | 间距自定 px 在域任务自然触及对应文件时以 `--space-1/--space-2` 对齐（4px 6px→4px 8px 等，允许 ±2px 微调）；新增样式一律 token | `--space-*` 无 5–7px 档，组件自定值与 token 尺度混合 | AGENTS.md 间距 token 约定；00 §1.3 D3「自然触及顺带收敛」纪律（不做独立重构轮） | 上述五组件随改随收 |

**统一验收口径**：以上落地属后续代码任务；验收 = `npm run build`（首步 lint:facts）+ `npm run lint:size` + `npm run test:ui` + tauri:dev 真桌面走查（SSH/MCP 功能浏览器预览不可验证）；错误条/读取失败态等低概率 IPC 异常路径若无法真实触发，按「该项验证未执行」如实记录，不得以静态检查替代（全局协作约定「不粉饰」）。

---

## openQuestions（本域移交时的未决项）

1. **`mcp_get_config` IPC 失败时后端 `mcp_scope` 判定的实际行为未验证**：`denyAll` 收敛态对应的是读盘损坏（`McpScopeSettings.vue:6` 头注释、`mcp.ts:79-80`），IPC 层失败（命令异常/未注册）时后端对 MCP 工具调用按什么等级判定，本会话未读后端判定代码、无法断言。UI 已按「不显示宽松结论」设计（R4），但 R4 文案中「以后端判定为准」的确切行为需读 `crates/myshelltool-core` 的 mcp_scope 判定路径后补证。
2. **后端是否可能返回 `'minimal'/'strict'` 以外的 level 值未验证**（诊断 P2 同项）：R13 已按 fail-closed 方向定格（未知值显示折叠为 strict），但触发面与后端写入侧的取值约束需实测/读 `mcp-config.json` 读写代码确认；若后端保证枚举两值，R13 降级为防御性代码注释。
3. **`EditorSurface.vue:363` 同款 `--accent-contrast` 是否随本域落地一并收敛**：属 editor 域文件，超出本域范围；建议任务编排方将其挂入 editor 域文档或本域任务的授权范围扩展项，避免两处定义漂移残留。
4. **GlobalModals 宿主是否补焦点 trap/焦点还原**：审批弹窗与 mcpPanel 弹窗宿主现状无焦点管理（grep 实证），与 00 §3.4「AppModal 是浮层键盘可达标准」有宿主级差距；属域外共享宿主的全局改造决策，不由本域单方面定。
5. **`McpExecutionLogList.vue`（470 行）与本方案增量（R1/R3/active 态合计约 +15 行）的余量判断**：预计 ~485 行仍 <500，但已无二次增量空间；若后续还要给日志区加能力（如导出），须先拆分（过滤条先例 `:4-5`）。是否现在就预防性拆分属任务编排决策。
