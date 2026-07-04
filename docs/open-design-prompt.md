# myshelltool UI 重构设计提示词（open-design）

> 本文件是提交给 open-design（或同类 AI 设计工具）的完整提示词。
> 维护原则：功能/布局/交互以 AGENTS.md 与实际代码为准；视觉规范为「克制高级感」方向。
> 源头单一信息源：`AGENTS.md`、`src/styles/_tokens.scss`、`src/components/`。

---

## 一、项目定位与设计目标

### 1.1 产品是什么

**myshelltool** 是一款 **Windows 桌面 SSH 运维客户端**，面向运维工程师与后端开发者。它把「连接管理 / 多终端 / 文件传输 / 资源监控 / 隧道 / 资产同步 / MCP 服务」集成在同一个桌面窗口内，是专业的、高频使用的**生产力工具**，不是消费类应用。

技术栈：Tauri 2 桌面框架（Rust 后端 + Vue 3 前端 + xterm.js 终端）。运行于原生 Windows 窗口，非浏览器网页。

### 1.2 目标用户画像

- 后端开发者 / 运维工程师 / SRE，每天长时间使用，多窗口多会话并行操作。
- 专业用户：能理解 SSH、SFTP、端口转发、资源指标等专业概念，**不需要过度拟物化或新手引导装饰**。
- 偏好：信息密度高、操作快、键盘友好、长时间盯着不累眼。

### 1.3 视觉风格定位：克制高级感（Refined Elegance）

**对标产品**：Linear、Raycast、Vercel、Arc Browser 的设置面板、1Password 8 桌面版、Cron/Notion Calendar。

**核心气质关键词**：精致、专业、克制、冷静、有呼吸感、高级、不喧宾夺主。

**设计哲学**：

1. **留白即设计**——靠间距、层级、留白体现质感，而非装饰元素。不要为了"好看"堆叠渐变、阴影、装饰图形。
2. **层级靠对比，不靠装饰**——通过字号、字重、颜色明度、间距密度建立信息层级，几乎不用粗边框、重阴影、强渐变。
3. **低饱和、冷调**——配色克制，主色只用一处点睛，大面积是中性的灰阶与近无彩色。
4. **精致到像素**——对齐严格、1px 分隔线、等比间距、统一的圆角语言、考究的字体细节。
5. **深浅双主题，同等精致**——不是"深色好看、浅色凑合"，两套主题都要达到「高端大气上档次」的标准，用户切换无落差。
6. **工具感优先**——这是工具，不是艺术品。每个视觉元素都要服务于"让用户更快完成任务"，拒绝为美而美的冗余装饰。

---

## 二、整体布局结构（必须严格遵循）

### 2.1 五区域 CSS Grid 布局

整个应用窗口固定划分为 **5 个区域**（外加上下两条通栏），用 CSS Grid 实现，最小窗口宽度 1280px。

```
┌──────────────────────────────────────────────────────────────────┐
│  顶栏 TitleBar（52px 高，全宽，固定不可滚动）                          │
│  [品牌logo+名称]    [全局搜索框 Ctrl+K]    [主题|同步|警告|面板|设置] │
├────────────────┬─────────────────────────────┬───────────────────┤
│                │                             │                   │
│  左侧栏         │    中上：终端 Terminal       │   右侧栏           │
│  连接资产树     │    （可拖拽调高）            │   资源监控         │
│  （可折叠至     ├─────────────────────────────┤   +               │
│   44px 竖排）   │    中下：文件 FileSurface    │   运维摘要         │
│  可拖拽调宽     │    （可拖拽调高）            │   （可整列折叠）    │
│                │                             │   可拖拽调宽       │
├────────────────┴─────────────────────────────┴───────────────────┤
│  底栏 StatusBar（28px 高，全宽，固定不可滚动）                       │
│  [SSH状态|后端模式|消息]  [传输胶囊]  [同步状态|MCP灯|隧道|警告]      │
└──────────────────────────────────────────────────────────────────┘
```

**关键约束**：

- 顶栏、底栏**固定 52px / 28px**，全宽，不随内容滚动。
- 左侧栏默认宽度约 260px，可**拖拽调宽**，可**折叠为 44px 竖排图标栏**（折叠机制通过 CSS 变量 `--sidebar-w: 44px` 实现，**不能用 `display:none`**，否则会触发白屏 bug）。
- 右侧栏默认宽度约 280px，可**拖拽调宽**，可**整列折叠为 0px**（同样用 CSS 变量 `--right-w: 0px`）。
- 中上（终端）与中下（文件）之间有**可拖拽的水平分界线**调整两者高度比。
- 三条可拖拽分界线（左栏右边缘、右栏左边缘、中上下分界）都是 **1px 细条**，hover 时扩大热区并轻微高亮，拖动时光标变 `col-resize` / `row-resize`。
- 每个区域用 `data-region="<name>"` 标记，便于测试与无障碍。
- 区域之间用 **1px 分隔线** 区隔（`var(--app-border)`），不用阴影、不用强对比色块堆叠。

### 2.2 折叠/展开与布局重置

- 左侧栏折叠按钮、右侧栏折叠按钮、恢复默认布局按钮，都放在顶栏右侧操作区。
- 折叠状态与各栏宽度**持久化到 localStorage**，重启恢复。
- "恢复默认布局"按钮会清空持久化的宽度/高度变量，回到初始比例。

---

## 三、深浅双主题设计规范

### 3.1 三态主题机制

支持三种模式：**跟随系统（system）/ 浅色（light）/ 深色（dark）**。

- 通过 `<html data-theme="light|dark|auto">` 属性切换。
- "跟随系统"模式监听操作系统的明暗偏好，自动在 light/dark 间切换。
- 主题选择入口：顶栏快速切换按钮（循环切换）+ 设置中心「外观」Tab（三卡片选择，即时生效）。

### 3.2 配色原则（两套主题同等重要）

**深色主题（dark）—— 默认主题，主力调性**：

- **不是纯黑**，而是"tinted dark"：在深灰底色里掺入极少量品牌蓝（5-8%），得到偏冷调的深灰蓝层次。背景有微妙的蓝调呼吸感，比纯黑高级、不压抑。
- 多层背景用明度差异建立层次：`app-bg`（最底层，最深）→ `app-window` → `app-chrome`（顶栏底栏工具栏）→ `app-panel`（面板）→ `app-panel-2`（次级面板/表头）→ `app-control`（输入框背景）。每一层比上一层亮约 2-4%。
- 文字层级靠**白色透明度**区分：`app-text`（接近全白）→ `app-strong`（纯白，强调）→ `app-muted`（白 53% 不透明）→ `app-subtle`（白 38% 不透明）。不要用独立色值，统一用透明度梯度。
- 边框极细、半透明：`app-border`（浅色 22% 不透明）、`app-border-strong`（浅色 34% 不透明）。1px hairline 风格。
- 状态色在深色底上适当提亮（success/warn/danger 各自的深色版本）。

**浅色主题（light）—— 必须同等精致**：

- 底色不是纯白，而是 `#fafafa`（极浅暖灰）+ `#ffffff` 纯白面板的层次。
- 文字 `#111111` 接近纯黑但不刺眼，muted 用 `#6b6b6b`。
- 边框 `#e5e5e5` 实色 hairline。
- 浅色主题的关键挑战：**避免显得"廉价 web 风"**。要靠精致的留白、考究的字重、克制的强调色、统一的圆角，达到与深色同等的高级感。参考 Linear 浅色模式、Vercel 文档站。

**强调色（accent）使用纪律**：

- 主强调色是**中蓝**，**只用于**：主操作按钮、激活态指示（选中项左边框、激活 Tab 下划线、focus ring）、链接、进度条、关键数据高亮。
- **绝不大面积铺强调色**作背景。强调色是"点睛"，不是"主色块"。
- 状态色 success（绿）/ warn（黄）/ danger（红）只用于状态指示（状态点、进度条状态、危险操作按钮、警告徽章），不当装饰色。

### 3.3 设计 Token 契约（必须保留的语义层）

重构后的设计**必须沿用以下 CSS 变量名作为语义契约**（前端代码已大量引用，改名会破坏一致性）。你可以在提示词产出中给出新的**色值建议**，但变量名保持不变：

```css
/* 主题语义层 —— 随主题切换，组件消费这一层 */
--app-bg            /* 最底层背景 */
--app-window        /* 窗口背景 */
--app-chrome        /* 顶栏/底栏/工具栏 chrome 背景 */
--app-panel         /* 面板背景 */
--app-panel-2       /* 次级面板/表头/卡片背景 */
--app-control       /* 输入框/下拉背景 */
--app-hover         /* hover 态叠加色 */
--app-border        /* 1px hairline 边框 */
--app-border-strong /* 强调边框（激活/聚焦容器） */
--app-text          /* 主文字 */
--app-strong        /* 强调文字（标题/数值） */
--app-muted         /* 次要文字（副标题/说明） */
--app-subtle        /* 最弱文字（占位/辅助） */
--app-scrim         /* 遮罩层（modal/drawer 背后） */
--app-shadow        /* 浮层阴影（modal/drawer） */

/* 终端专用 */
--terminal-bg
--terminal-text
--terminal-prompt

/* 基础色板（不随主题变的原始值） */
--accent / --accent-on / --accent-hover / --accent-active
--success / --warn / --danger
--bg / --surface / --fg / --muted / --border
```

**字体 token**（必须保留变量名）：

```css
--font-display: 'Inter', -apple-system, system-ui, sans-serif;  /* 标题 */
--font-body:    'Inter', -apple-system, system-ui, sans-serif;  /* 正文 */
--font-mono:    ui-monospace, 'JetBrains Mono', monospace;       /* 等宽 */
```

> ⚠️ **字体加载修复（必须处理）**：当前 `Inter` 和 `JetBrains Mono` 在 token 里声明了，但**实际从未加载**（无 `@font-face`、无字体文件、index.html 无 Google Fonts 引入），导致回退到系统字体。设计稿必须明确：**正式引入 Inter（UI 字体）+ JetBrains Mono（等宽字体）**，建议用 `@font-face` 自托管 woff2（桌面应用离线优先，不依赖 CDN）。终端等宽字体回退链：`'JetBrains Mono', Consolas, 'Courier New', ui-monospace, monospace`。

**间距 / 圆角 / 动效 / z-index token**（4px 基线，建议保留）：

```css
/* 间距（4px 基线）*/
--space-1: 4px;  --space-2: 8px;  --space-3: 12px; --space-4: 16px;
--space-5: 20px; --space-6: 24px; --space-8: 32px; --space-12: 48px;

/* 圆角 */
--radius-sm: 8px;    /* 按钮/输入框/菜单项 */
--radius-md: 12px;   /* 下拉/小卡片 */
--radius-lg: 16px;   /* Modal/Drawer/大面板 */
--radius-pill: 9999px; /* 状态药丸/徽章 */

/* 动效（克制）*/
--motion-fast: 150ms;
--motion-base: 200ms;
--ease-standard: cubic-bezier(0.2, 0, 0, 1);

/* z-index 层级 */
--z-base:1; --z-dropdown:100; --z-sticky:200;
--z-drawer:300; --z-modal:400; --z-toast:500; --z-tooltip:600;

/* 阴影（极克制，仅浮层用）*/
--elev-flat: none;
--elev-ring: 0 0 0 1px var(--border);          /* 静态容器描边 */
--elev-raised: 0 2px 8px rgba(...,8%);         /* 轻微浮起 */
--focus-ring: 0 0 0 3px rgba(accent,30%);      /* 聚焦环 */
```

> 可清理的死代码：`--section-y-*`、`--container-*`、`--container-gutter-*` 是早期 web 落地页遗留，桌面应用用不到，设计稿不必考虑。

---

## 四、各区域详细设计规范

### 4.1 顶栏 TitleBar（52px）

三栏 grid：`auto minmax(220px,1fr) auto`。

**左侧 · 品牌区**：
- 方块 logo（建议用首字母 `m` 或抽象 SSH/终端符号的极简图标，单色，accent 色或 currentColor）+ "myshelltool" 产品名 + 极弱的副标题"Windows SSH 客户端"（subtle 色，小字）。
- logo 与文字之间间距考究，整体克制不张扬。

**中间 · 全局搜索框**（核心交互）：
- 占据中间弹性宽度，最小 220px。
- 占位符："搜索主机、标签、命令；输入 ssh user@host 快速连接"。
- 左侧搜索图标（subtle 色，聚焦时变 accent）。
- 右侧 `Ctrl K` 键盘徽章（kbd 样式：1px border、圆角 sm、subtle 文字、subtle 背景）。
- 聚焦时展开下拉建议面板：资产模糊搜索结果（图标+名称+user@host 副标题）+ 快速连接建议。
- **Ctrl+K 全局快捷键**聚焦此框。

**右侧 · 操作按钮组**（全部 ghost 按钮，icon + 可选文字）：
1. 主题切换（图标随当前主题：浅色=Sun / 深色=Moon / 跟随=对比半圆图标，附文字"浅色/深色/跟随系统"）。
2. 同步 · 安全（图标 + 文字，打开同步面板）。
3. 警告计数（`N warning`，有警告时文字变 warn 色 + warn 圆点）。
4. 分隔细条（1px 竖线，border 色）。
5. 收起/展开右侧面板（PanelRight 图标）。
6. 恢复默认布局（RotateCcw 图标）。
7. 设置齿轮（Settings 图标）。

按钮之间间距 4-8px，hover 态用 `app-hover` 叠加 + 圆角 sm。**不要给按钮加粗边框或强背景**，ghost 风格即可。

### 4.2 底栏 StatusBar（28px）

三栏 grid：`1fr auto 1fr`。背景 `app-chrome`，字号 xs（12px），文字 muted 色。

**左侧 · 状态信息**：
- SSH 状态点（绿=connected / 灰=idle）+ "SSH connected/idle" 文字。
- 后端运行模式（backend tauri/web）。
- 可点击的状态消息（点击若含"更新"触发更新流程，否则回显）。普通态 subtle 色，有内容时变 text 色。

**中间 · 传输胶囊**（pill 样式，可点击打开传输抽屉）：
- Upload 图标 + "传输" + 计数 `N 进行中 / ✓N 完成`。
- 传输中：图标呼吸动效（pulse）+ accent 高亮。
- 空闲：muted 色弱化。

**右侧 · 多项状态指示**（各项之间用细竖线或间距分隔）：
- 同步状态文本（同步中…/最近操作结果/已配置/未配置）。
- MCP 指示灯（绿点 + "MCP 可用" / 灰点 + "不可用"），可点击跳 MCP 面板。
- 隧道计数 `tunnels N/M`（运行中/总数）。
- 警告计数 `N warning`（warn 圆点）。

### 4.3 左侧栏 · 连接资产（ConnectionSidebar）

**三段式结构**：sticky header / 可滚动树区 / sticky footer。

**Header（sticky）**：
- 标题"连接资产"（strong 色，base 字号）+ 三个图标按钮（ghost，sm）：收起/展开（PanelLeftClose/Open）、新建分组（FolderPlus）、新增连接（Plus）。
- 筛选搜索框（AppInput search 样式，占位符"筛选分组、标签、主机、用户"），有值时显示清空 X。

**树区（可滚动）**：
- 资产按 `group` 字段（`/` 分隔多级路径，如"生产/数据库/主"）聚合为**递归分组树**，子组件递归渲染。
- 分组头：折叠箭头（ChevronDown 展开态 / ChevronRight 折叠态，subtle 色）+ 分组名（text 色）+ 计数徽章（subtle 色小字 `N`）。
- 资产项：状态圆点（connected/connecting=success 绿 / idle=灰；connecting 时圆点 pulse 动效）+ 资产名（text 色，激活项 strong 色）+ 副标题 `user@host`（mono，muted 色，xs 字号）。
- 激活项：左侧 2px accent 竖条 + `app-hover` 浅背景。
- 选中/hover：`app-hover` 叠加。
- 缩进按 depth，每级 12-16px。
- 悬停资产项右侧出现快捷操作按钮（编辑/删除，ghost sm）。
- **"未分组"是保留顶级**，始终置于末尾，不可重命名/解散/拖拽排序。

**空状态**（无资产）：居中 Server 图标（subtle 大号）+ "尚未添加连接资产" + "新增连接"主按钮。

**Footer（sticky）**：
- 快速连接输入框（mono 字体，占位符 `ssh user@host[:port]`）+ 回车连接提示（subtle xs）。
- 输入合法的 `ssh user@host` 回车即建连。

**折叠态**（44px 宽）：
- 缩成竖排图标栏：展开按钮（顶部）+ 新建分组 + 新增连接，三个图标垂直居中排列。
- 隐藏 header 文字、搜索框、树、footer。

**交互**：
- 右键菜单（AppContextMenu）：资产菜单（编辑/复制/分隔/移动到分组…/分隔/删除 danger）；分组菜单（重命名…/分隔/解散分组 danger）。
- 拖拽：资产拖入分组→直接移动（自定义 MIME，落盘不弹窗）；分组间拖拽→同级排序。
- 键盘：上下箭头移动焦点，Enter 连接。

### 4.4 中上 · 终端（TerminalSurface）

**Tab 栏（顶部）**：
- 每个 SSH 会话一个 Tab：状态圆点 + 名称 + host + 动态 OSC 远端标题（可变）。
- 激活 Tab：底部 2px accent 下划线（Tabby 风格，**不用 pill 背景**）+ 文字 strong 色。
- 非激活 Tab：muted 色，hover 变 text 色 + `app-hover`。
- 支持新建、选中、关闭（× 按钮，hover 显示）、中键关闭、`Ctrl+Tab`/`Ctrl+Shift+Tab` 切换。
- Tab 过多溢出收进「⋯」溢出菜单。
- Tab 右键：复制主机地址 / 关闭其他 / 关闭右侧 / 关闭此标签（danger）。

**快捷工具栏（Toolbar，Tab 栏下方一行）**：
- 左侧：连接状态药丸（ConnectionStatusPill）—— 圆点（connecting/reconnecting 时 pulse）+ 状态文案（空闲/连接中/已连接/已断开/重连 n/total/错误）+ 主机名（strong）+ 副标题 `user@host`（mono muted）+ OSC 标题（省略号 subtle）。
- 右侧：字号徽章「Npx」（pill subtle）+ 图标工具组（ghost sm，每个带 title 提示快捷键）：搜索 / 复制 / 粘贴 / 字号+ / 字号- / 清屏 / 重连（仅非 connected 显示，warn 色）/ 会话详情抽屉（激活态）/ 全屏。

**终端主体**：
- xterm.js 渲染区，背景 `terminal-bg`，文字 `terminal-text`，等宽字体（JetBrains Mono 优先）。
- 光标闪烁，光标样式 bar。
- 支持滚轮滚动（Alt 快速滚动）、URL 可点击（WebLinks）。
- `Alt+Enter` 切换窗口全屏。

**内联搜索栏**（Ctrl+Shift+F 唤出，浮在终端顶部）：
- 搜索框 + 三个开关（区分大小写 / 全字匹配 / 正则）+ 上一个/下一个按钮 + 匹配计数 `n/total` 或"未匹配"。

**命令面板**（Ctrl+Shift+P）：
- 模态浮层，模糊搜索会话列表 + 9 个动作（搜索/复制/粘贴/字体/清屏/全屏/连接/速查表），上下键导航 + 回车执行。

**快捷键速查表**（按 `?` 唤出模态）：4 组分组（会话/编辑/字体/视图），kbd 样式展示快捷键。

**危险粘贴守卫**（粘贴危险命令时弹红色确认窗）：显示命令预览 + 命中规则 + "仍然粘贴"(danger) / "取消"。

**终端空状态**（无会话）：居中 TerminalSquare 图标（subtle 大号）+ 标题 + 引导文案 + "连接 {资产名}"/"新增连接"按钮 + 快捷键速查表。

### 4.5 中下 · 文件（FileSurface）

**双栏布局**：
- 默认仅远程栏占满全宽；点"本地"按钮（pill 样式）展开为 `1fr 1fr` 双栏，中间 1px 分隔。
- 整面支持拖拽上传：从 Windows 资源管理器拖文件进来即上传，dragover 时整面 accent 虚线边框 + 居中浮层"松开以上传到当前远程目录" + 显示目标路径。

**单栏结构（FileColumn，local/remote 两用）**：
- **Header**：标题"本地/远程"（strong）+ 项数/已选数（muted）+ 路径面包屑（每段可点跳转，分隔符 ChevronRight subtle）+ 末端"✎"按钮切原始路径输入框（回车跳转/Esc 取消）+ 过滤按钮（Filter 图标，有值时 accent）+ 上级目录（ChevronUp/Level）+ 刷新（RotateCw）。
- **列头（detailed 模式）**：6 列可点排序（名称/大小/类型/修改时间/权限/用户组），升降序切换，ChevronUp/Down 指示。
- **文件列表**：两种模式 —— detailed（6 列完整信息）/ compact（仅图标+名称）。
  - 行：图标（目录 accent 色 / 链接 / 文件 muted）+ 名称（text）+ [detailed] 大小（mono）/ 类型（扩展名大写如 JS/PY/DIR/LNK）/ 修改时间（mono muted）/ 权限（mono muted，八进制串）/ 用户:组（mono muted）。
  - hairline 行分隔（border-soft），hover `app-hover`，选中 accent 浅背景。
- **过滤浮层**：右上角小窗（Filter 触发），输入即时过滤，空状态"无匹配"。

**选择**：单击选中、Ctrl/Cmd 多选、Shift 范围选、Ctrl+A 全选、空白处/Esc 清空。

**快捷键**：F2 重命名 / Del 删除 / F5 刷新 / Ctrl+A 全选 / Esc 清选。

**右键菜单**（区分 local/remote，区分单选/多选）：
- remote 单项：进入目录/下载 / 重命名 / 删除（danger）/ 复制路径 / 上传文件到当前目录 / 新建远程目录 / 刷新 / 切换紧凑/详细列表 / 显示本地面板。
- remote 多选：批量下载（N）/ 批量删除（N, danger）+ 目录级操作。
- local 单项：进入目录/上传到远程（待接线）/ 重命名 / 删除（danger）/ 复制路径 / 新建本地目录 / 刷新 / 隐藏本地面板。

### 4.6 右侧栏 · 资源监控 + 运维摘要

右侧栏上下叠放两组件（静态布局，非 Tab 切换）。

**上半 · 资源监控（ResourceMonitorPanel）**：

跟随当前活跃 SSH 会话启停，**2 秒轮询**，历史保留 60 点。所有图表用原生 SVG 自绘（无第三方库）。

四张图表，每张 panel header 是**小写 + letter-spacing 的 chrome 标题**风格：

1. **CPU**：label `cpu`（小写带字距）+ 当前值 `XX.X%`（strong 粗体）+ `N cores`（muted）。**面积+折线图**（accent 色，渐变填充 35%→0% 透明度），高 60px，纵轴 0-100%。
2. **内存**：label `内存` + `已用 / 总量`（formatBytes）。**折线图**（accent，纵轴 0-100%）高 36px + 下方 3px 细进度条（usedPct%）。
3. **网络**：label `网络` + `↓RX速率` `↑TX速率`（rx info 色 / tx success 色）。**双折线图**（RX accent + TX success），纵轴自适应 max，高 60px。
4. **磁盘 I/O**：label `磁盘 I/O` + `R读速率` `W写速率`（rd info / wr warn）。**双折线图**（读 accent + 写 warn）高 60px + 容量区块：根分区 `已用/总量` + 百分比（≥70% warn / ≥85% danger）+ 4px 进度条（同色阶）。

四种空状态：需桌面端（MonitorOff）/ 未连接 SSH（Cpu）/ 等待采样（Activity）/ 正常展示。

**下半 · 运维摘要（OpsSummaryPanel）**：

显示选中资产的关键信息，用 `<dl>` 网格（`grid-template-columns: auto 1fr`，dt muted + dd text）展示：
- 主机 host:port（mono）
- 凭据绑定状态
- 最近连接时间
- 活跃会话 N/M
- Git 同步来源
- 隧道 N 活跃 / N 总

空状态："选择左侧主机查看详情"（subtle 居中）。

### 4.7 设置中心（SettingsPanel，模态）

通过齿轮按钮打开（默认 Tab），顶栏同步按钮跳 sync Tab，状态栏 MCP 灯跳 mcp Tab。用 `AppTabGroup` 分页容器。

**4 个 Tab**：

1. **关于与更新**（默认）：
   - Hero 区：myshelltool logo + 版本号（`vX.X.X`，strong 大字）。
   - 应用更新区：单一主按钮"检查更新"，状态机（idle/checking/downloading/available/error），状态文案随状态变化。
   - 关于：项目主页（链接 accent）/ 许可（MIT）。

2. **外观**：
   - 主题三卡片网格（跟随系统 / 浅色 / 深色），每张卡片：预览缩略图（迷你窗口示意）+ 名称 + 选中态（accent 边框 + 角标 Check）。
   - 点击即时生效并持久化。

3. **同步**：复用同步面板（见 §5.7）。

4. **MCP**：复用 MCP 面板（见 §5.6）。

### 4.8 全局传输抽屉（TransferDrawer）

从底栏"传输"胶囊触发，**Teleport 到 body**，从窗口底部上滑（bottom side drawer）。

- 头部：标题"传输队列"（strong）+ 统计 `N 进行中 · N 完成 · N 总计`（muted）+ 收起按钮。
- 每行：方向图标（Upload accent / Download success）+ 文件名（text，超长省略）+ 已传/总大小（mono muted）+ 线性进度条（accent/success/danger 着色）+ 状态药丸：
  - 传输中：Loader2 旋转图标 + 百分比
  - 完成：CheckCircle2（success）
  - 失败：AlertCircle（danger）+ 错误文案
  - 排队：muted
- 完成行 60 秒后自动清理。

---

## 五、各业务弹窗与面板规范

### 5.1 全局弹窗中枢（GlobalModals）

所有业务弹窗走统一的 `AppModal` 组件（Teleport to body，focus trap，Esc 关闭，点击遮罩关闭，圆角 lg，`app-shadow`，`app-scrim` 遮罩）。

按 `modal.type` 分支渲染，共 **18 种**，标题与主按钮文案随类型变化。关键类型：

| 类型 | 用途 | 标题 |
|---|---|---|
| assetEditor | 资产新增/编辑表单 | 新增/编辑连接资产 |
| tunnelCreate | 新建隧道表单 | 新增隧道 |
| tokenConfig | GitHub PAT 配置 | 配置/更新 GitHub token |
| hostKeyVerify | 主机密钥验证 | 主机密钥验证 |
| keyboardInteractive | 键盘交互认证 | 键盘交互认证 |
| mcpApproval | MCP 高危操作审批 | MCP 高危操作审批 |
| mcpPanel | MCP 服务管理 | MCP 服务管理 |
| syncPanel | 资产同步 Gist | 资产同步（Gist） |
| settings | 设置中心 | 设置 |
| mkdir / localMkdir | 新建目录 | 新建远程/本地目录 |
| rename / localRename | 重命名 | 重命名远程/本地条目 |
| terminalSearch | 终端搜索 | 终端搜索 |
| confirmDelete | 删除资产确认 | 删除连接资产 |
| renameGroup | 重命名分组 | 重命名分组 |
| createGroup | 新建分组 | 新建分组 |
| moveAsset | 移动资产到分组 | 移动到分组 |

**Modal footer 行为**：危险类（hostKeyVerify/mcpApproval）有 danger"拒绝"按钮；面板类（mcpPanel/syncPanel/settings）主按钮为"关闭"；confirmDelete 有独立 danger 删除按钮；其余有"取消"。

### 5.2 资产编辑表单（assetEditor）

双列 grid（`grid-2`）表单：
- 名称 / 主机 / 端口 / 用户名 / 分组（原生 input + datalist 自动补全，可输新路径）/ 标签（逗号分隔）/ 认证方式（Password/PrivateKey，AppSelect）/ 私钥路径（PrivateKey 时启用）/ 状态（Connected/Warning/Idle）。
- 凭据 callout（提示框，info 色弱背景）：密码已存储提示。
- 密码 / Passphrase 输入（Password 首次必填，内联校验错误用 danger 色文字 + 红边框）。

### 5.3 隧道创建表单（tunnelCreate）

- 名称 / 类型（AppSelect：local 本地转发 / dynamic SOCKS5 / remote 远端转发[标注"暂未实现"]）/ 本地地址（默认 127.0.0.1）/ 本地端口 / 远程地址+端口（kind≠dynamic 时显示）/ 自动启动 checkbox。

> ⚠️ **设计缺口提示**：当前**隧道只有创建表单 + 计数展示，没有独立的隧道管理列表 UI**（无启停/删除/编辑界面）。建议在本次重构中**补全隧道管理面板**：可作为右侧栏的第三个折叠区块，或作为设置中心新 Tab，或作为独立 Drawer。展示隧道列表（名称/类型/状态/端口映射/自动启动开关/启停按钮/删除按钮）。**请在设计稿中给出隧道管理 UI 的方案**。

### 5.4 主机密钥验证（hostKeyVerify）

危险提示框：警示图标（warn）+ 标题 + 主机密钥指纹（mono，break-all）+ 引导文案 + "拒绝"(danger) / "确认"。

### 5.5 MCP 高危操作审批（mcpApproval）

外部 LLM 调用 myshelltool 工具执行高危命令时的审批弹窗。三段信息（`<dl>` 网格）：
- **AI 声明意图**（text）
- **真实命令**（mono，danger 色，break-all，num）
- **后果预测**（warn 色）
+ 引导核对文案 + "拒绝执行"(danger) / "确认"。65 秒自动超时关闭。

### 5.6 MCP 面板（McpPanelContent）

myshelltool 作为 MCP server，供外部 LLM 宿主（Claude Code / Cursor）调用。三层信息架构：

1. **Hero 状态条**（全宽横幅）：
   - 可用：success 着色（浅 success 背景 + success 边框）+ Plug 图标 + "MCP 可用" + 副标题"协议握手通过" + 右侧 meta（版本 `myshelltool vX.X` + 探测时间 + "刷新"按钮）。
   - 不可用：虚线 muted 边框 + "MCP 不可用" + probe.detail + 引导文案。

2. **连接详情**（`<dl>` 网格 auto 1fr）：
   - 探测 Endpoint（mono，可复制）
   - 数据目录（mono，可复制）
   - MCP Endpoint URL（mono accent）

3. **接入配置**：
   - 说明 v1.4 HTTP transport。
   - 三家宿主接入方式（Claude Code → 配置文件 / Cursor → `.cursor/mcp.json`）。
   - `<pre>` 配置 JSON 代码块（**深色终端背景**，即使浅色主题也用深色代码块以突出代码）+ "复制配置 JSON"按钮（带 ✓ 复制成功提示）。
   - ⚠ 内联警告：确保 GUI 运行。

4. **能力清单**（McpCapabilityList）：
   - 工具按分组：**高危（需审批）**（ShieldAlert 图标，tone-danger，红左边框，优先显示）/ **只读（自动放行）**（BookOpen，tone-muted）。每工具：`code` 名 + 描述。
   - 底部双栏：Resources·N（URI 列表，模板项有 tag 徽章）+ Prompts·N（name + 参数列表）。

### 5.7 资产同步面板（SyncPanelContent）

通过 GitHub Gist 加密备份连接资产，主密码加解密。四视图分支（互斥，按优先级）：

1. **冲突框**（最高优先级）：SyncConflictResolver —— AlertTriangle 标题"检测到同步冲突" + 双卡选项（本地 / 远端 Gist）各显示资产摘要"N 个连接 · N 个分组" + 主密码 + 三按钮："用本地覆盖远端"(danger) / "用远端覆盖本地"(primary) / 取消。

2. **PAT 未配置**：SyncPatGuide —— 3 步引导（含外链 github.com/settings/tokens）+ 加密说明。

3. **首次设置**（PAT OK 但同步未配）：主密码（≥6 位）+ 确认主密码 + 已有 Gist ID（可选，换机器时填）+ 主按钮"创建同步/拉取远端数据"。

4. **日常管理**（同步已配）：
   - 同步状态网格（`<dl>`）：Gist ID（脱敏掩码）/ 上次同步时间 / 自动同步状态。
   - 自动同步开关（SyncAutoSyncControl）。
   - 推送/拉取区：主密码输入 + 推送/拉取按钮（自动同步启用时可留空）。
   - 重置密码（折叠区）。
   - 清空同步（内联二次确认）。

**Hero 状态条**（三态着色，与 MCP 面板同范式）：
- is-connected（success）/ is-warn（accent，PAT 配置但同步未配）/ is-offline（虚线 warn，未配 PAT）。
- 显示状态文案 + Gist ID + 上次同步时间 + 远端更新徽章（`remoteHasUpdates` 时显示"远端有更新"可点拉取）+ 刷新按钮。

### 5.8 GitHub PAT 配置卡（PatConfigCard）

自包含卡片，复用于 tokenConfig 弹窗与设置同步 Tab：
- 状态展示"已配置/未配置"（只展示状态，不回显 token）。
- 输入框 + 保存/清除按钮。
- token 仅写本地安全存储，绝不回显。

---

## 六、基础组件库规范（必须沿用）

以下 12 个基础组件已实现，重构**必须沿用其 API 与 variants**（前端代码已大量引用）。设计稿中所有 UI 元素都应基于这些组件表达：

| 组件 | variants / 关键约束 |
|---|---|
| **AppButton** | `variant`: primary（accent 实心）/ ghost（默认，透明 hover 态）/ subtle（弱背景）/ danger（danger 色）；`size`: sm / md；支持 `loading`（spinner）。圆角 sm。 |
| **AppInput** | `type`: text / password（带眼睛切换）/ search（自带 Search 图标 + X 清空）/ number；支持 `error`（danger 文案 + 红边框）/ `disabled` / `mono`（切等宽字体）。 |
| **AppSelect** | 自定义下拉，`options:[{label,value}]`，菜单 z-dropdown，圆角 md，选中项 accent + Check 图标，caret 旋转。 |
| **AppModal** | Teleport to body，z-modal，圆角 lg，app-shadow，app-scrim 遮罩。**内置 focus trap + Esc 关闭 + 点击遮罩关闭 + 打开自动聚焦**。 |
| **AppDrawer** | `side`: right（默认）/ bottom；各自带 slide 过渡；Esc + 遮罩关闭。 |
| **AppContextMenu** | `items:[{label,action,danger,separator,disabled}]`；fixed 定位；danger 项红字 + 红底 hover；支持 separator。 |
| **AppTooltip** | `placement`: top/right/bottom/left；`delay` 300ms；反相配色（深底浅字）。 |
| **AppTable** | sticky 表头，sortable 列，空态 slot，具名插槽 `cell-{key}`，圆角 md。 |
| **AppTab / AppTabGroup** | active 用 `inset 0 -2px 0 accent` 下划线指示（**不用 pill 背景**），底部 1px border 分隔。 |
| **AppProgress** | `variant`: linear（轨高 4px pill）/ circular（SVG 环）；`status`: running(accent) / done(success) / error(danger)。 |
| **AppStatusBar** | 3 列 grid（1fr auto 1fr），高 28px，背景 app-chrome，字号 xs。 |

> **建议补充的组件**（当前缺失，重构可补）：Checkbox/Radio/Switch 开关、Badge/Tag 徽章、Toast 提示（已有 z-toast 预留）、Skeleton 骨架屏、空态组件（EmptyState）。其中 **Switch 开关**和 **Badge 徽章**使用频率高，建议优先补。

---

## 七、视觉细节与质感规范（克制高级感的关键）

### 7.1 字体与排版

- **UI 字体**：Inter（必须正式加载，自托管 woff2）。字重梯度：Regular 400（正文）/ Medium 500（次级强调）/ Semibold 600（标题/数值）。**避免使用 650/620 这类非标准字重**（当前 base.scss 遗留，应规整到标准 400/500/600/700）。
- **等宽字体**：JetBrains Mono（必须正式加载）。用于：所有 `user@host`、路径、命令、权限串、大小数值、端口、时间戳、代码块。等宽字体是运维工具的"专业感"来源。
- **字号体系**：xs 12px（状态栏/徽章/辅助）/ sm 14px（正文，body 默认）/ base 16px（重要正文）/ lg 20px（区域标题）/ xl 24px（页面标题）。**不要用过大的标题**（运维工具不需要 32px+ 的大字），克制即高级。
- **行高**：正文 1.5，紧凑标题 1.2。
- **字间距**：chrome section header（如资源监控的 `cpu`/`网络`/`磁盘 I/O` 标签）用 **小写 + letter-spacing 0.08-0.12em + muted 色**，这是 Linear/Vercel 风格的标志性细节。
- **数字**：所有数值用 `font-variant-numeric: tabular-nums`（等宽数字），避免跳动。

### 7.2 颜色与对比

- **强调色使用纪律**（重申）：accent 蓝只用于点睛，绝不大面积铺。每个屏幕上 accent 色的总面积应 < 5%。
- **状态色克制**：success/warn/danger 只用于状态指示，不当装饰。
- **对比度**：文字对比度满足 WCAG AA（正文 ≥ 4.5:1）。`minimumContrastRatio: 4.5`（终端配置已有）。
- **不用强渐变**：除进度条的微妙渐变填充、面积图的 35%→0% 透明渐变外，**不用大面积渐变背景**。Linear/Vercel 风格是纯色 + 层次，不是渐变。

### 7.3 圆角、间距、密度

- **圆角语言统一**：按钮/输入/菜单项 sm(8px)；下拉/小卡片 md(12px)；Modal/Drawer lg(16px)；状态药丸/徽章 pill。**不要混用**。
- **4px 基线**：所有间距是 4 的倍数（4/8/12/16/20/24/32）。
- **高信息密度但有呼吸感**：运维工具要密度，但不是挤。靠**留白节奏**而非堆砌体现高级。输入框 padding 6-8px 10-14px，菜单项 padding 6-8px 10-12px，状态栏高 28px。
- **对齐严格**：所有文字基线对齐、图标与文字垂直居中、数值右对齐（表格里）、标签左对齐。

### 7.4 分隔线与边框

- **1px hairline 主导**：区域分隔、表格行、列表项、卡片都用 1px `app-border` 细线，**不用粗边框**。
- **激活/聚焦容器**用 `app-border-strong` 或 accent 边框。
- **不用重阴影堆层级**：除 Modal/Drawer/Popover 浮层用 `app-shadow` 外，静态容器用 1px 边框或纯背景层次区分，不用阴影。

### 7.5 动效（克制）

- 时长：fast 150ms（hover/focus）/ base 200ms（展开/切换）。
- 缓动：`cubic-bezier(0.2, 0, 0, 1)`（标准，有减速感）。
- **不用弹跳、不用回弹、不用长动画**。所有动效快速、干脆、不拖沓。
- 状态点 pulse（连接中/传输中呼吸）：subtle 的透明度脉动，不是强烈闪烁。
- 进度条过渡平滑，数值跳变用 tabular-nums 避免抖动。

### 7.6 滚动条

- **极细半透明**：8px 宽，thumb 用 `app-border-strong`，圆角 pill，hover 变 `app-muted`。轨道透明。这是 Linear/Raycast 风格。

### 7.7 聚焦环（无障碍）

- 所有可交互元素 `:focus-visible` 用 `focus-ring`（3px accent 30% 透明环），不用默认 outline。键盘导航友好。

---

## 八、深浅主题对照要点（设计稿必须同时给出）

**设计稿必须同时呈现深色与浅色两套**，关键区域都要有双主题对照：

1. 整体五区域布局（深 + 浅）
2. 连接资产侧栏（含分组树、资产项、激活态、空状态）（深 + 浅）
3. 终端区（Tab 栏 + 工具栏 + 终端主体 + 内联搜索）（深 + 浅）—— 终端在两套主题下都要有对应的 ANSI 配色
4. 文件区（双栏 + 表格 detailed/compact + 拖拽上传态）（深 + 浅）
5. 资源监控四图表（深 + 浅）
6. 运维摘要 dl 网格（深 + 浅）
7. 设置中心 4 Tab（深 + 浅）
8. 关键弹窗：资产编辑 / 主机密钥验证 / MCP 审批 / 危险粘贴守卫（深 + 浅）
9. MCP 面板 / 同步面板（含 Hero 状态条三态）（深 + 浅）
10. 传输抽屉（深 + 浅）
11. 隧道管理面板（**新增，补全缺口**）（深 + 浅）

**浅色主题的高标准**：浅色不能是"深色反转凑合"。要独立设计，确保：
- 不显廉价 web 风（靠精致留白 + 考究字重 + 克制强调色）。
- 边框、阴影、对比都重新校准（浅色下阴影更浅、边框更实）。
- 状态色在浅底上用稍深的版本，避免过亮刺眼。

---

## 九、输出要求

请基于以上规范，产出完整的 UI 设计方案，至少包含：

1. **设计 Token 定义**：完整的深/浅两套 `--app-*` 语义层色值建议（可直接替换 `_tokens.scss` 的值），含基础色板（accent/success/warn/danger 两套）、字体加载方案（Inter + JetBrains Mono 的 @font-face）、间距/圆角/动效/z-index。
2. **整体布局设计稿**：五区域布局的深浅双主题对照（标注尺寸：52px/28px 通栏、各栏默认宽度、最小 1280px）。
3. **各区域详细设计稿**：按 §4 逐区域给出深浅双主题，标注交互态（default/hover/active/disabled/focus）、空状态。
4. **基础组件设计稿**：12 个现有组件 + 建议补充组件（Switch/Badge 等）的深浅双主题 + 各 variant + 各状态。
5. **业务弹窗与面板设计稿**：按 §5 逐个给出。
6. **隧道管理面板方案**（补全缺口）：给出独立的隧道列表管理 UI 设计。
7. **交互动效说明**：关键过渡（折叠展开/抽屉滑入/Tab 切换/状态变化/进度更新）的动效规格。
8. **设计决策说明**：对每个关键设计选择，简述"为什么这样设计"（服务于克制高级感 + 运维工具的专业性）。

**风格硬约束（再次强调）**：
- ✅ 克制、留白、层次靠对比、低饱和冷调、1px hairline、精致到像素、深浅同等精致。
- ❌ 不要大面积渐变、不要重阴影堆叠、不要粗边框、不要装饰性图形、不要消费类应用的拟物化、不要为美而美的冗余元素。
- ✅ 这是专业运维工具，目标是"长时间使用不累眼、操作快、看着高级"。
