# 终端不可见回归 — 根因记录

> 日期：2026-06-15
> 关联：UI 全量重构（Wave 1-5）后的终端稳定性修复
> 状态：已修复（最终根因：xterm.css 未加载）

## 症状

`pnpm run tauri:dev` 启动桌面端，连接真实 SSH 主机（192.168.2.2）后：

1. **终端区域完全空白**（纯白，无光标、无文字可见）
2. 连接流程本身正常（标题栏显示 `online · 1 sessions`，标签显示会话）
3. 浏览器预览模式（`npm run dev`）下空状态卡片渲染正常
4. DevTools 元素检查发现：xterm 的 canvas 确实渲染了内容（784×264，有尺寸、有像素），`.composition-view` 里能看到用户输入的"大山东"——**说明 xterm 在工作，只是视觉上看不见**

## 根因（最终确认）

### 主因：xterm.css 根本没加载 → xterm 元素失去所有定位样式 → 层级错乱

**`src/index.html` 里用 `<link>` 引用 xterm.css 的方式在 Vite 下是坏的：**

```html
<!-- src/index.html (src/ 是 Vite root) -->
<link rel="stylesheet" href="../node_modules/@xterm/xterm/css/xterm.css" />
```

- 这个相对路径在浏览器里解析成 `/node_modules/@xterm/xterm/css/xterm.css`
- **Vite dev server 不直接服务 `node_modules` 下的裸文件路径**，对该 URL 做 SPA fallback → 返回 `index.html`
- 浏览器收到一个 **HTML 当作 CSS 加载**，类型不匹配，**CSS 完全没生效**

**验证证据**（决定性）：
```
GET http://127.0.0.1:41234/node_modules/@xterm/xterm/css/xterm.css
→ 200, Content-Length: 432, 但内容是 index.html 的 HTML（标题乱码）
真实 xterm.css 是 7112 字节，含 .xterm-viewport/.xterm-screen 的 position:absolute 等关键样式
```

**没 CSS 导致的视觉错乱**：
- `.xterm-viewport` 失去 `position:absolute`（DOM 里它是空的）
- `.xterm-screen` 失去定位 → canvas 画了内容但位置/层级错乱
- 用户看到"白色空白"，但 canvas 其实在别处画着内容

### 为什么浏览器测试通过、桌面端却空白？

两者用**同一个 dev server**，CSS 都没加载。但：
- Playwright UI 测试只验证"5 分区布局存在 + 空状态卡片可见"，不验证 xterm 实际渲染
- 空状态卡片是纯 Vue 组件，不依赖 xterm.css
- 所以测试通过 ≠ xterm 可见。**测试盲区。**

## 修复（最终）

### 核心修复：xterm.css 改为 JS import

**`src/main.js`** 新增：
```js
import '@xterm/xterm/css/xterm.css';  // 让 Vite 打包，而非 HTML <link>
```

**`src/index.html`** 移除坏掉的 `<link>`。

**验证**：
- dev: `main.js` 现在含 `import "/@fs/.../xterm.css"`，返回 8006 字节的真实 CSS（Vite 包装为 HMR JS 模块）✅
- build: `dist/assets/index-*.css` 275KB，grep `xterm-viewport` 命中 ✅

## 之前以为的根因（实际是次要因素，但仍保留修复，因为它们本身是合理的健壮性改进）

排查过程中一度认为是"xterm 在 display:none 上 open 导致 WebGL 0×0 初始化"。这是基于代码静态分析的合理怀疑，但**最终被 DOM 证据推翻**：DevTools 显示 canvas 尺寸 784×264（正常，不是 0×0），WebGL 其实初始化成功了。

不过这些改动本身是正确的健壮性改进，全部保留：

1. **termDiv 不再 display:none 创建，先可见再 open**（sessions.js）—— 即使 CSS 加载正常，这也更安全
2. **WebGL addon 在 open 之后 loadAddon** —— canvas 按真实尺寸初始化
3. **open 后单 rAF + 同步 fit** —— 拿到准确 cols/rows 再发起连接
4. **ResizeObserver rAF 防抖 + cols/rows 去重** —— 防"输入乱跳"反馈循环（这是原始 bug 的真根因，依然有效）
5. **onResize 加 cols/rows > 0 守卫** —— 防 0×0 PTY
6. **#terminalContainer 去 padding / overflow:hidden** —— padding 偏移测量、overflow:auto 触发抖动
7. **空状态卡片移出 fit 容器** —— 绝对定位覆盖，避免连接时内容尺寸突变

## 防再犯检查清单（更新版）

1. ✅ **第三方库的 CSS 用 JS import，不要用 index.html 的 `<link href="../node_modules/...">`** —— Vite 不直接服务裸 node_modules 路径，会 fallback 到 index.html。这是本 bug 的真正根因，最高优先级。
2. ✅ **`term.open()` 的目标元素必须可见、有真实尺寸** —— 绝不能在 display:none 上 open。WebGL canvas 一旦在 0×0 初始化，后续 fit 不保证恢复。
3. ✅ **WebGL addon 在 `term.open()` 之后加载** —— 先 open（建立 DOM + 测量），再 loadAddon(Webgl)。
4. ✅ **`fit()` 调用前确保元素已完成布局** —— 用 `requestAnimationFrame` 等一帧。
5. ✅ **ResizeObserver 回调里的 `fit()` 必须防抖** —— 否则 xterm 重绘的子像素变化触发 RO→fit→resize→重绘→RO 反馈循环。
6. ✅ **`#terminalContainer` 不加 padding / overflow:auto** —— 滚动交给 xterm 的 `.xterm-viewport`。
7. ✅ **fit 容器内不要放会消失的兄弟元素** —— 空状态用绝对定位覆盖。
8. ✅ **`onResize` / `ssh_resize` 调用前守卫 cols/rows > 0** —— fit 在 0 尺寸元素上会算出 0。

## 教训

- **UI 测试通过 ≠ 功能正常**。smoke 测试验证的是布局结构，没验证 xterm 实际渲染。对"看起来在但实际没渲染"的 bug，测试是盲区。
- **怀疑 CSS 没加载，第一时间用 DevTools 看计算样式**，而不是反复改 JS 时序。当时如果早点查 `.xterm-viewport` 的 `position`，10 秒就能定位。
- **Vite 的 SPA fallback 会把任何不存在的路径返回 index.html**，包括 CSS 路径。`<link>` 拿到 200 不代表拿到 CSS —— 要看 Content-Type 和实际内容。

## 涉及文件

- `src/index.html` — 移除坏掉的 xterm.css `<link>`（**真正根因**）
- `src/main.js` — 新增 `import '@xterm/xterm/css/xterm.css'`（**真正根因修复**）
- `src/stores/sessions.js` — xterm 挂载时序、ResizeObserver 防抖、onResize 守卫（健壮性改进）
- `src/components/terminal/TerminalPane.vue` — 容器 CSS、空状态卡片定位（健壮性改进）
- `src/components/resource-monitor/chart-utils.js` — buildLinePath NaN 守卫（独立 bug）
- `src/stores/workbench.js` — watch source 修复（独立 bug）
- `src-tauri/tauri.conf.json` — beforeDevCommand 改 pnpm（独立改进）

### 次因：终端高度被 tab+toolbar 挤压

Playwright 高度链测量（viewport 1366×800）：

```
.shell-layout         h=800  (grid)
└─ .shell-region--center-top  h=360  (grid 行 minmax(0,1fr))
   └─ .terminal-surface       h=359  (flex column)
      ├─ TerminalTabs         flex:0 0 auto  (~40px)
      ├─ TerminalToolbar      flex:0 0 auto  (~45px)
      └─ .terminal-pane-host  h=274  (flex:1 1 auto)  ← 被 tab+toolbar 挤掉 85px
         └─ #terminalContainer h=274
```

高度链没断，但终端可用高度比预期小（274px 而非 360px），导致 `rows` 偏少（实测 12）。这是布局特性，不是 bug，但放大了渲染问题的影响。

### DevTools 连接日志确认（[MST-CONNECT]，已移除）

```
1. invoking ssh_connect {cols: 112, rows: 12}     ← cols/rows 有值，但 rows 偏小
2. connected, session_id= 0f452b16...              ← 连接成功
3. output listener | term.cols=112 rows=12 | termDiv in container? true  ← xterm 已挂载
4. ssh-output event #1 {payloadLen: 72}            ← SSH 数据到达
5. DONE, status=connected                          ← 全流程完成
```

**连接、挂载、数据流全部正常，唯独 canvas 没渲染** → 指向 WebGL 初始化问题。

## 修复

### 核心修复：xterm 必须在元素已可见、有真实尺寸的状态下 open

`connectSelected` 的 xterm 创建流程改为：

```js
// termDiv 一开始就可见、占满容器
const termDiv = document.createElement('div');
termDiv.style.cssText = 'display:block;width:100%;height:100%;';  // ← 不再 display:none
terminalContainer.appendChild(termDiv);
// ...
term.open(termDiv);          // ← 在可见元素上 open（有真实尺寸）
// WebGL addon 在 open 之后加载（此时尺寸正确）
term.loadAddon(webgl);
await new Promise(r => requestAnimationFrame(r));  // 等一帧布局完成
fit.fit();                   // ← 同步 fit 拿准确 cols/rows
```

关键点：
- `termDiv` 创建时 `display:block`（非 `none`）
- WebGL addon 在 `term.open()` **之后** loadAddon（确保 canvas 按真实尺寸初始化）
- open 后单 rAF 等布局，再 fit

### 配套修复

1. **ResizeObserver 防抖 + cols/rows 去重**（src/stores/sessions.js `attachResizeObserver`）：用 `requestAnimationFrame` 合并同帧多次回调，避免"输入→回显→RO→fit→resize→重绘→RO"反馈循环（这是原"输入字符 UI 乱跳"的根因）。
2. **`onResize` 加 `cols/rows > 0` 守卫**：防止把 0×0 PTY 发给服务器导致整屏重绘。
3. **`#terminalContainer` 去掉 `overflow:auto` 和 `padding`**（src/components/terminal/TerminalPane.vue）：padding 偏移测量原点，overflow:auto 的滚动条 ±8px 触发 fit 抖动。滚动交给 xterm 自己的 `.xterm-viewport`。
4. **空状态卡片移出 fit 容器**：改为绝对定位覆盖，避免连接时卡片消失导致容器内容尺寸突变。
5. **`showOnlyActiveTerminal` 的 fit 用双 rAF**：替代原 `setTimeout(20ms)`，保证刚 `display:''` 的元素布局完成后再测量。

### 顺带修复的独立 bug

- **chart NaN**（src/components/resource-monitor/chart-utils.js `buildLinePath`）：后端 snapshot 字段为 undefined/NaN 时，`Math.min/max` 返回 NaN → SVG path `M236.0,NaN` 报错。加 `Number.isFinite` 守卫归零。
- **workbench.js watch source 警告**：`watch(uiStore.effectiveTheme, ...)` 传的是被 Pinia 解包后的值（字符串 `'light'`），不是 ref。改用 getter 函数 `watch(() => uiStore.effectiveTheme, ...)`。
- **tauri.conf.json `beforeDevCommand`**：从 `npm run` 改成 `pnpm run`，消除 npm 的 `Unknown config` 警告。

## 防再犯检查清单

下次改动终端/xterm 相关代码时，逐项确认：

1. ✅ **`term.open()` 的目标元素必须可见、有真实尺寸** —— 绝不能在 `display:none` 或 0×0 元素上 open。WebGL canvas 一旦在 0×0 初始化，后续 fit 不保证恢复。
2. ✅ **WebGL addon 在 `term.open()` 之后加载** —— 先 open（建立 DOM + 测量），再 loadAddon(Webgl)，让 canvas 按正确尺寸创建。
3. ✅ **`fit()` 调用前确保元素已完成布局** —— 用 `requestAnimationFrame` 等一帧，不要用 `setTimeout(任意毫秒)` 猜测。
4. ✅ **ResizeObserver 回调里的 `fit()` 必须防抖** —— 否则 xterm 重绘的子像素变化会触发 RO → fit → resize → 重绘 → RO 的反馈循环。用 rAF 合并 + cols/rows 去重。
5. ✅ **`#terminalContainer` 不加 padding / overflow:auto** —— padding 偏移 FitAddon 测量原点；overflow:auto 的滚动条宽度变化触发 fit 抖动。滚动交给 xterm 的 `.xterm-viewport`。
6. ✅ **fit 容器内不要放会消失的兄弟元素** —— 连接时 v-if 移除的元素会让容器内容尺寸突变。空状态用绝对定位覆盖，不要放进 fit 容器。
7. ✅ **`onResize` / `ssh_resize` 调用前守卫 cols/rows > 0** —— fit 在 0 尺寸元素上会算出 0，发给服务器会导致整屏重绘。
8. ✅ **桌面端 DevTools 打开方式** —— xterm 吞 F12/Ctrl+Shift+I，调试时点窗口非终端区域再按快捷键，或在 setup 里临时加 `#[cfg(debug_assertions)] window.open_devtools()`。

## 退路（如果本次修复仍无效）

如果"保留 WebGL 修时序"的方案在真实环境仍空白，退路是**临时降级到 DOM 渲染**：注释掉 `connectSelected` 里 `term.loadAddon(webgl)` 那段（约 sessions.js 的 WebGL addon 加载块）。xterm 的默认 DOM renderer 无 canvas 初始化问题，挂载即渲染。性能足够 SSH 终端使用。

## 涉及文件

- `src/stores/sessions.js` — connectSelected 的 xterm 挂载时序、ResizeObserver 防抖、onResize 守卫
- `src/components/terminal/TerminalPane.vue` — 容器 CSS、空状态卡片定位
- `src/components/resource-monitor/chart-utils.js` — buildLinePath NaN 守卫
- `src/stores/workbench.js` — watch source 修复
- `src-tauri/tauri.conf.json` — beforeDevCommand 改 pnpm
- `src-tauri/src/lib.rs` — DevTools 自动打开（调试时临时加，已移除）
