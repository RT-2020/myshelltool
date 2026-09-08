# UI 设计 token 语料（生图与逐页改造的固定约束）

> **定位**：本文件是 myshelltool UI 产出的「固定语料」。任何生图任务、任何页面改造，
> 都必须携带本文件的 L0–L3 约束，只允许场景描述变化。
> 目的：没有设计师的团队，路径不是每页让 AI 自由发挥，而是——
> ① 提炼事实规范 → ② 固化为 `_tokens.scss`（已完成）→ ③ 语料化后让 AI 按规范逐页产出（本文件）。
>
> **唯一事实源**：颜色/间距/圆角/动效的准确值以 [`src/styles/_tokens.scss`](../src/styles/_tokens.scss) 为准。
> 本文件是它的「AI 可消费翻译版」，两者冲突时改 `_tokens.scss` 并同步本文件。
> 变量名契约（`--app-*` 等 44 处引用）不因设计调整而改名。

---

## 使用方式

| 场景 | 用法 |
|---|---|
| 生图 | 复制 §6 英文固定语料块 + 场景块，拼进 prompt；**不修改语料块本身** |
| 改页面 | 对照 §2 token 取值、§3 组件规范、§4 负向清单自查 |
| 新增组件 | 先查 §3 有无既定形态，再动手；新形态确定后回写 §3 |

---

## L0 设计原则（宏观一致性，决定「像不像一个产品」）

- **P1 信息唯一性**：同一信息全界面只出现一次（标签页已示主机，工具栏/状态栏不得重复展示同一连接）。
- **P2 空值不渲染**：无数据的字段整行消失，禁止「—」占位行。
- **P3 低频不常驻**：教学式提示文案、低频操作不占一级界面；提示只在交互进行时出现（见 §5 拖拽状态机）。
- **P4 层级而非面积**：终端是视觉层级最高的表面（最深的底色、最大的面积）；文件区、监控是次级表面（`--app-panel-2` 打底、字号小一档、密度更高）。
- **P5 状态双编码**：连接/传输/同步状态 = 颜色点 + 文字或图形，不允许只靠颜色区分（色弱可用）。
- **P6 密度优先**：专业用户工具，紧凑但留呼吸：列表行 28–32px，区块间距 12–16px，不堆留白也不拥挤。

---

## L2 Token 数值表（微观一致性）

> 深色主题给生图近似 hex（oklch 生图模型不识别），开发实现一律用 `_tokens.scss` 的 oklch 原值。

### 颜色 · 深色（生图默认主题）

| 角色 | token | 生图近似值 |
|---|---|---|
| 窗口底 | `--app-bg` | `#14171C` |
| 窗口层 | `--app-window` | `#181B21` |
| 外壳 chrome（标题/状态栏） | `--app-chrome` | `#1C2027` |
| 面板 | `--app-panel` | `#20242C` |
| 次级表面（文件区/监控卡） | `--app-panel-2` | `#252A33` |
| 控件底 | `--app-control` | `#292F39` |
| 边框 | `--app-border` | `rgba(255,255,255,0.10)` |
| 主文字 | `--app-text` | `rgba(255,255,255,0.95)` |
| 次文字 | `--app-muted` | `rgba(255,255,255,0.55)` |
| 弱文字 | `--app-subtle` | `rgba(255,255,255,0.38)` |
| 强调（选中/焦点/进度） | `--accent` | `#4D7CFF` |
| 终端底（全界面最深的洞） | `--term-bg` | `#07090F` |
| 成功 | `--success` | `#4CC77D` |
| 告警/生产环境标识 | `--warn` | `#E6A33A` |
| 危险 | `--danger` | `#EF6660` |
| 信息 | `--info` | `#4CB8D6` |

**状态色使用纪律**：只用于状态点、徽标、图表描边、toast 语义条；**禁止大面积填充**、禁止当装饰色。

### 颜色 · 浅色（逐页改造对照用）

底 `#FAFAFA` / 面板 `#FFFFFF` / chrome `#F6F6F6` / 边框 `#E7E7E8` / 主文字 `#111114` / 次文字 `#6B6B72` / accent `#2C5FE5` / 状态色 `#15A34A` `#C8780C` `#D43A3A` `#0A7B9C`。终端区**恒为深色** `#0C0F17`（浅色主题下终端不变浅）。

### 字体与字号

- UI：Inter；终端/路径/IP/数字：JetBrains Mono。
- 字号阶梯只用 12 / 14 / 16：正文 14，辅助信息 12，16 仅标题栏与空态。表格数字用等宽（tabular）。
- 终端字号 13–14px，行高 1.3–1.5。

### 几何

- 间距 4px 基线：4 / 8 / 12 / 16 / 24 / 32（组件内 8–12，区块间 12–16，分区 24+）。
- 圆角：控件 8px（`--radius-sm`），卡片/弹窗 12px（`--radius-md`），16 以上仅空态插画容器。
- 图标：lucide 线条图标，20px（工具栏）/ 16px（行内），1.5px stroke。
- 列表行高 28–32px；表格行 28px。

### 布局框架（token 驱动，改动即改 token）

`--titlebar-h: 52px` · `--sidebar-w: 260px`（收起 44px）· `--right-w: 280px`（收起 0）· `--statusbar-h: 28px`。中央区自上而下：标签栏 → 终端 → 文件区（保留常驻，§3 定义其减负形态）。

### 动效

120ms（hover/点按）/ 200ms（面板、overlay 出现）/ 320ms（抽屉、弹窗），缓动 `cubic-bezier(.4,0,.2,1)`。终端内容永不做过渡动画。

---

## L3 组件形态规范（决定「各页拼起来像一家人」）

| 组件 | 规范 |
|---|---|
| 资产树行 | **单行 32px** = 状态点(8px) + 名称；IP/延迟等进 tooltip；分组头 = 小号次文字 + 计数 |
| 会话标签 | 状态点 + 名称 + 关闭钮，活动标签 accent 下划线 2px；「+」为裸图标 |
| 工具栏 | ≤4 个 20px 图标，无文字标签；快捷键可替代的一律不进工具栏 |
| 文件区 | 本地\|远程双栏；路径面包屑（mono 12px）+ 3 个操作图标；表格 4 列：名称/大小/修改时间/类型图标；行 28px |
| 监控卡片 | 数值（mono 16px）+ 单条 sparkline，无小字注脚；细节 hover tooltip |
| 状态栏 | 单行 28px，左（连接数）中（编码/协议）右（传输图标+百分比、MCP/同步状态点） |
| 弹窗 | 12px 圆角、`--app-panel` 底、遮罩 `--app-scrim`；危险操作主按钮 `--danger` 实色 |
| Toast | 右下角，语义色 3px 左边条 + 深色卡，3s 自动消失；成功不打断 |
| 拖拽 overlay | 见 §5 状态机，唯一允许「大面积 accent-soft」的场合 |

---

## L4 负向清单（每次产出后逐条自查）

- ❌ 常驻教学文案（「拖拽文件即可上传」「输入 ssh user@host…」）
- ❌ 调试字符串入 UI（`Ok(SystemTime {…})`、「启动 tauri-core」）
- ❌ 「—」空值占位行
- ❌ 双行列表项（名称一行 + IP 一行的资产树）
- ❌ 工具栏 >4 图标；表格 >5 列
- ❌ 工作区标题（单工作区工具无此概念）
- ❌ 同一信息第二处出现
- ❌ 渐变、玻璃拟态、装饰插画、玩具感圆角

---

## §5 拖拽传输状态机（提示消失 ≠ 功能消失）

**触发规则**：本地栏 → 远程栏 = 上传；远程栏 → 本地栏 = 下载；OS 文件管理器 → 远程栏 = 上传；拖入终端 = 光标禁止（可选项：插入文件路径）。非目标区域一律禁止态光标，不亮。

| 状态 | 触发 | 视觉 | 退出 |
|---|---|---|---|
| 0 静默 | — | **完全无痕**，两栏正常渲染，无任何常驻提示 | — |
| 1 拖入 | 文件悬停目标栏 | 目标栏覆盖 `accent-soft` 半透明层（≈16% accent）+ 内缩 8px 的 1.5px accent 虚线框 + 中央 ↑/↓ 图标与一行文字「松开：上传 3 个文件到 /var/www」（动态路径）；另一栏无变化 | 松手→2；移出/ESC→0 |
| 2 排队 | 松手 | 状态栏传输图标点亮 + 队列数；传输抽屉自动展开一行任务 | 开始→3 |
| 3 传输中 | 进行 | 抽屉内行级进度条（accent 填充，mono 百分比）；状态栏聚合百分比；目标目录锁定刷新 | 完成→4 / 失败→5 |
| 4 完成 | 全部结束 | success toast「已上传 3 个文件到 /var/www」；目录刷新，新文件行 accent-soft 高亮 1.5s 渐隐 | 自动 |
| 5 失败 | 任一失败 | danger toast + 原因；队列保留失败项可重试；同名覆盖先走确认弹窗（安全红线） | 用户 |

---

## §6 英文固定语料块（生图时原样携带，不改）

```text
[DESIGN SYSTEM BLOCK — paste unchanged]

Professional Windows desktop SSH ops tool for expert users. Density-first,
calm quiet chrome; every piece of information appears exactly once; no helper
sentences, no empty placeholder rows, no debug text anywhere.

Dark theme palette (fixed): layered near-black blue-grey — window background
#14171C, chrome #1C2027, panels #20242C, secondary surfaces #252A33; 1px
hairline borders rgba(255,255,255,0.10); text primary rgba(255,255,255,0.95),
secondary 55% white, tertiary 38% white. ONE accent blue #4D7CFF for selected
states, focus rings and progress fills. Status colors success #4CC77D,
warning #E6A33A, danger #EF6660, info #4CB8D6 — used ONLY as small dots,
badges, chart strokes and toast edge bars, never as large fills. Terminal
surface is the darkest inset #07090F.

Typography: Inter for UI (12/14/16px, body 14px); JetBrains Mono for terminal,
paths, IPs and numbers, tabular numerals in tables.

Geometry: 4px spacing grid, 8px control radius / 12px card-modal radius,
28-32px list rows, lucide-style 20px line icons at 1.5px stroke.

Layout frame: 52px title bar, 260px collapsible left sidebar, center column
(session tabs, terminal hero, bottom dual-pane file browser), 280px right
sidebar, 28px status bar.

Forbidden: workspace title text, persistent hint sentences, two-line list
items, toolbars with more than 4 icons, tables with more than 5 columns,
empty dash placeholder rows, gradients, glassmorphism, decorative
illustrations, playful rounded style.
```

### 场景块示例（与语料块拼接使用）

**场景 A · 主工作台（精简后）**：

```text
[SCENE A — main workbench, slimmed]
Title bar: logo + compact centered search "Ctrl+K" + 3 icons + window controls.
Left sidebar: server tree, single-line rows (status dot + hostname), one bare
quick-connect input at bottom, zero hint text.
Center: tab bar (status dot + name + close x, plus "+"), a 4-icon terminal
toolbar (search, clear, split, pop-out) with NO status text on its right,
then the large terminal hero, then a compact local|remote dual-pane file
browser: breadcrumb + 3 action icons, 4-column table (name, size, modified,
type icon), 28px rows, normal readable dates.
Right sidebar: four resource cards, each ONE value + ONE sparkline; session
info with only non-empty rows; compact service status dots (MCP, sync).
Status bar: "2 connected" | "UTF-8 · SSH" | transfer icon + sync/MCP dots.
Figma mockup style, sharp vector, 1366x800. --ar 16:10 --style raw
```

**场景 B · 拖拽上传进行时**：

```text
[SCENE B — drag-and-drop upload in progress]
Same workbench. User is dragging 3 files from the LOCAL pane onto the REMOTE
pane of the file browser: the remote pane is covered by a translucent blue
overlay (16% of #4D7CFF) with an inset 8px dashed 1.5px #4D7CFF border, and
centered one upload arrow icon plus one short line of text. The local pane
stays normal. Status bar right side shows an active transfer glyph with a
small percentage. Everything else unchanged and quiet. Figma mockup, sharp,
1366x800. --ar 16:10 --style raw
```
