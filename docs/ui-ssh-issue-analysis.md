# myshelltool 现状问题分析报告

> 调查日期：2026-06-12
> 范围：UI 主题同步、界面设计稿残留、SSH 连接无法建立
> 涉及代码：`src/App.vue`、`src/stores/workbench.js`、`src/services/backend.js`、`src-tauri/src/lib.rs`、`src-tauri/src/ssh.rs`、`src/styles.css`

---

## 摘要

本次调查覆盖三个独立问题域。其中：

| 编号 | 问题 | 严重程度 | 已修复 |
|---|---|---|---|
| 1 | UI 主题不跟随系统 | 体验级 | ✅ 本次已修复 |
| 2 | 工作区总览 / 终端 / 文件栏出现大量"设计稿残留"文案与占位数据 | 体验级 | ⚠️ 文档列出修复方案，未实施 |
| 3 | **新建 SSH 连接点击后无法连接** | **致命级** | ⚠️ 文档列出根因和修复方案，未实施 |

问题 3 是用户反馈中最影响可用性的项，根因是 Tauri 后端的 state 注册方式与命令签名不匹配——所有 SSH 命令在被调用时都会因参数解析失败而立即返回错误。

---

## 问题 1：UI 主题与系统同步

### 现状

主题相关逻辑位于 `src/stores/workbench.js`：

```js
// 原代码 stores/workbench.js:15
const theme = ref(readStored('myshelltool-theme') || 'dark');

// 原代码 stores/workbench.js:152-157
function toggleTheme() {
  theme.value = theme.value === 'light' ? 'dark' : 'light';
  applyTheme(theme.value);
  localStorage.setItem('myshelltool-theme', theme.value);
  announce('已切换到' + (theme.value === 'light' ? '浅色' : '深色') + '主题');
}
```

CSS（`src/styles.css:1-101`）的设计：

- `:root` 默认声明了一套**深色**变量（`color-scheme: dark`，所有 `--app-*` 都基于深色）；
- `:root[data-theme="light"]` 是浅色覆盖；
- 没有任何 `[data-theme="dark"]` 选择器——因为默认就是深色。

整个仓库**完全没有出现** `prefers-color-scheme`、`matchMedia` 或 "system" 关键字（已通过 grep 确认，唯一命中是 CSS 里 `system-ui` 字体名）。

### 根本原因

应用只支持 `light` / `dark` 两个硬编码状态，由用户手动点击按钮在两者之间切换，没有任何机制监听操作系统 / 浏览器的主题偏好变化。"主题与系统同步"这件事在代码层面根本不存在。

### 已实施的修复

修改 `src/stores/workbench.js`：

1. 引入三态主题 `system | light | dark`，默认 `system`；
2. 新增 `systemPrefersDark` ref + `effectiveTheme` computed，`system` 模式下实时计算实际生效主题；
3. `startSystemThemeListener()` 调用 `window.matchMedia('(prefers-color-scheme: dark)')`，监听系统主题变化（同时兼容旧 Safari 的 `addListener`）；
4. `watch(effectiveTheme, next => applyTheme(next))`：系统主题变化时自动重应用；
5. `toggleTheme()` 改为 `system → light → dark → system` 三态循环；
6. 暴露 `themeLabel` computed（"跟随系统" / "浅色" / "深色"），供 UI 显示当前状态。

修改 `src/App.vue:482-484`：

- 按钮文案改为显示**当前主题**（`{{ themeLabel }}`），并把切换提示放到 `title` 属性里；
- 用户点击可在三态之间循环。

验证：

- `npm run build` 通过（vite v7.3.5，25 modules，925ms，exit 0）；
- 验证脚本（`tests/ui-smoke.mjs`）未涉及主题断言，无需调整。

### 后续建议（未实施）

- `styles.css` 中 `:root` 默认是深色，`:root[data-theme="light"]` 是浅色——这套方案可以继续工作。但如果未来想让默认（无 `data-theme` 属性时）也跟随系统，可以再加一层 `@media (prefers-color-scheme: light)` 覆盖。
- `themeToggle` 按钮的 `aria-pressed="theme === 'light'"` 在三态下语义不准确，可改为 `aria-label` 描述当前模式。

---

## 问题 2：UI 设计稿残留内容

这一类问题的共同特征：界面上的文字、数据、占位行**写给设计师或产品看**，而非写给最终用户看。生产环境应该清理或替换为真实状态。

### 2.1 侧边栏的 8 个"假主机"

**位置**：`src/services/backend.js:20-29` 的 `fallbackAssets`。

```js
export const fallbackAssets = [
  makeAsset('prod-bastion', 'prod-bastion', '10.10.4.8', 'root', ...),
  makeAsset('web-01', 'web-01', '10.10.8.21', 'deploy', ...),
  makeAsset('db-readonly', 'db-readonly', '10.10.9.32', 'audit', ...),
  // ... 共 8 个
];
```

**行为分析**：

- **浏览器预览模式**（开发期，没有 Tauri runtime 时）：`readBrowserAssets()` 在 localStorage 没有数据时返回 `fallbackAssets`，侧边栏立即填满 8 个看起来很真实的主机（含内网 IP、用户名、状态点）。
- **Tauri 桌面客户端模式**：`invokeBackend` 走真实 `tauriInvoke`，`list_connection_assets` 从 `connection-assets.json` 文件读取（`src-tauri/src/lib.rs:38-46`）。第一次安装时该文件不存在或为空数组，**侧边栏会是空的**。

**用户感知**：

- 在浏览器预览模式下，用户看到 8 个看起来像真实运维资产的主机，但点击连接全都连不上（因为这些 IP 是设计稿里的样例）。
- 在已安装的桌面客户端下，用户看到空侧边栏，需要点 `+` 自己新增连接。

**修复建议**：

1. 删除 `fallbackAssets`，或者把它的内容改为明显标识为示例的占位（如名字加前缀 `[示例]`、IP 用 `203.0.113.x` 文档范围段）。
2. 浏览器预览模式下显示一条 banner："当前为浏览器预览，SSH 功能需安装桌面客户端"，而不是用假数据掩盖。
3. 桌面客户端首次启动且 `assets` 为空时，侧边栏显示一个引导卡片（"点击 + 新增第一台主机"），而不是一棵空树。

### 2.2 工作区总览面板

**位置**：`src/App.vue:554-607`（`screen-panel[data-panel="overview"]`）。

设计稿残留最典型的几处：

| 行号 | 内容 | 性质 |
|---|---|---|
| 558 | `<p>聚合真实资产、活跃会话、传输队列、后端桥接和风险状态，让第一屏回到设计稿的工作台形态。</p>` | **设计稿解释文案**，直接出现"设计稿"字样 |
| 566-571 | 4 个 `metric-card`：活跃 SSH / 传输队列 / 隧道健康 / 同步状态 | 卡片本身合理，但首次进入时数字全是 0 / `off`，缺乏空状态文案 |
| 575 | `<span class="status-pill running"><span class="dot running"></span>状态已接入</span>` | 静态 pill，永远显示"状态已接入"，不反映任何真实状态 |
| 577-580 | `overviewRows` 默认值 `{title: '选择连接资产开始工作', body: '资产、文件和隧道状态会从后端适配层加载。'}`（`App.vue:166-171`） | 占位行，"从后端适配层加载"是开发术语 |
| 601-604 | `<div class="callout warn"><strong>安全提醒</strong>...</div>` | 静态 callout，写给评审看的安全说明 |

### 2.3 终端栏

**位置**：`src/App.vue:609-656`。

| 行号 | 内容 | 性质 |
|---|---|---|
| 613 | `<p>终端保持最大可读面积，右侧只展示当前路径文件和会话摘要；浏览器预览会明确提示需要桌面客户端。</p>` | **设计稿说明**，写给评审看布局意图 |
| 631 | `<div v-if="!activeSession">SSH 终端需要桌面客户端。当前为浏览器预览模式。</div>` | **严重误导**：只要没建立 SSH 会话就显示，即使已经安装桌面客户端也会显示"浏览器预览模式" |
| 642 | `<p v-if="!remoteEntries.length">打开文件页后加载远程目录。</p>` | 占位文案，但用户在终端 tab 看到这段会困惑（为什么要去文件页？） |

**2.3 的"浏览器预览"误导是最值得优先修的**。代码逻辑：

```html
<div v-if="!activeSession" style="...">
  SSH 终端需要桌面客户端。当前为浏览器预览模式。
</div>
```

判定条件是 `!activeSession`，即"当前没有活跃 SSH 会话"。但文案把它等同为"浏览器预览模式"。实际两种情况都会触发：

- 真的在浏览器预览（没装桌面客户端）；
- 装了桌面客户端，但还没点击连接 / 连接失败。

第二种情况下，用户看到"当前为浏览器预览模式"会非常困惑——明明装了客户端，为什么说我是浏览器预览？

**修复建议**：判定条件应该结合 `isTauriRuntime()`：

```html
<div v-if="!activeSession">
  <span v-if="!isTauriRuntime()">SSH 终端需要桌面客户端。当前为浏览器预览模式。</span>
  <span v-else>请点击左侧主机建立 SSH 连接。</span>
</div>
```

### 2.4 文件栏

**位置**：`src/App.vue:658-727`。

| 行号 | 内容 | 性质 |
|---|---|---|
| 662 | `<p>保留设计稿的本地 / 远程双栏结构，远程列表来自后端目录命令，本地栏展示真实传输队列。</p>` | **直接出现"设计稿"字样**，是最显眼的残留 |
| 676-679 | `localPreviewRows` 默认占位 `[{name: '等待传输任务', ...}, {name: '远程下载', ...}]`（`App.vue:216-222`） | 占位文案可接受，但措辞像 TODO |
| 697 | `<div v-if="!remoteEntries.length"><strong>尚未加载</strong><p class="muted">点击刷新读取远程目录</p></div>` | 占位文案，但用户可能不知道为什么要手动点刷新 |
| 722-725 | `<div class="callout warn"><strong>实时同步状态</strong><p>传输通过 SFTP 进度事件实时推送，完成后保留 60 秒便于复查。</p></div>` | 设计说明，写给评审看实现细节 |

### 2.5 通用修复策略

所有 panel 的 header `<p>` 描述段落（`App.vue:558, 613, 662, 733`）都属于同一类残留：写给设计师 / 评审看的"这个面板做什么"的元描述，对最终用户没有任何价值。建议统一删除或换成一句对用户有意义的引导（如"选择左侧主机开始管理远程文件"）。

`status-pill.running` 里硬写的"状态已接入"（`App.vue:575`）应该绑定到真实状态（如 `backendStatus.ready ? '已连接' : '未就绪'`）。

---

## 问题 3：新建 SSH 连接点击后无法连接（致命）

这是用户反馈的核心可用性问题。

### 现象

- 用户在桌面客户端中点 `+` 新增一个 SSH 连接资产，保存成功（侧边栏出现该主机）；
- 点击该主机 → 点"终端"或"连接"按钮 → 终端区域显示连接提示后立即出现红色错误；
- 无法建立任何 SSH 会话。

### 根本原因：Tauri state 类型不匹配

#### 证据 1：lib.rs 只注册了 `AppState` 类型

`src-tauri/src/lib.rs:156-160`：

```rust
app.manage(AppState {
    asset_store_path: app_data_dir.join("connection-assets.json"),
    secret_store_dir: app_data_dir.join("credentials"),
    ssh_sessions: ssh_mgr,   // Arc<AsyncMutex<SshSessionManager>>
});
```

`AppState` 是一个普通结构体（`lib.rs:11-15`），里面有一个 `ssh_sessions: Arc<AsyncMutex<ssh::SshSessionManager>>` 字段。`app.manage(AppState {...})` 把 `AppState` 类型注册到 Tauri 的 StateManager，key 是 `TypeId::of::<AppState>()`。

#### 证据 2：ssh.rs 所有命令都在期望 `Arc<Mutex<SshSessionManager>>` 类型独立被 manage

`src-tauri/src/ssh.rs` 全部 23 个 `#[tauri::command]` 函数（`ssh_connect`、`ssh_write`、`sftp_*`、`tunnel_*`，已通过 grep 确认）的签名都是：

```rust
pub async fn ssh_connect(
    state: State<'_, Arc<Mutex<SshSessionManager>>>,  // ← 期望这个完整类型
    host: String,
    // ...
) -> Result<SshConnectResult, String>
```

(`src-tauri/src/ssh.rs:352-364`，其余 22 个同构)

#### 证据 3：Tauri 的 State<T> 是按完整类型 T 查找

Tauri 2.x 的 `State<'r, T>` 通过 `app.state::<T>()` 实现，内部从 `StateManager` 的 `HashMap<TypeId, Arc<dyn Any>>` 中按 `TypeId::of::<T>()` 查找。

`AppState` 和 `Arc<Mutex<SshSessionManager>>` 是**两个完全不同的类型**，对应不同的 `TypeId`。注册了 `AppState`，并不会自动让 `Arc<Mutex<SshSessionManager>>` 也能被查找到——StateManager 不会"穿透"结构体字段。

#### 调用链还原

1. 用户点击连接 → 前端 `connectSelected()`（`stores/workbench.js:457`）调用 `invokeBackend('ssh_connect', {...})`；
2. `invokeBackend` 检测到 Tauri runtime，转 `tauriInvoke('ssh_connect', args)`（`services/backend.js:36-39`）；
3. Tauri 在 invoke handler 中找到 `ssh_connect` 已注册（`lib.rs:171`），开始解析参数；
4. 解析 `state: State<'_, Arc<Mutex<SshSessionManager>>>` 时，调用 `app.state::<Arc<Mutex<SshSessionManager>>>()`；
5. StateManager 中没有这个 `TypeId`，**解析失败**，Tauri 返回错误；
6. 前端 `connectSelected` 的 `try/catch`（`stores/workbench.js:551-556`）捕获错误，向终端写入：
   ```js
   term.writeln('\x1b[31mError: ' + error.message + '\x1b[0m\r\n');
   ```
7. 用户看到红色错误提示，连接建立失败。

**所有 23 个 SSH/SFTP/tunnel 命令都会以同样方式失败**——不只 `ssh_connect`。这意味着：

- 新建连接失败；
- 即使连接侥幸建立，`ssh_write`（敲命令）、`sftp_list_dir`（文件列表）、`tunnel_start`（启隧道）全部都调不通。

### 为什么 `saveAsset` 能成功？

`save_connection_asset` 命令的签名（`lib.rs:48-61`）：

```rust
fn save_connection_asset(
    state: State<'_, AppState>,  // ← 这里用的是 AppState，匹配
    asset: myshelltool_core::ConnectionAsset,
) -> Result<ConnectionAssetList, String>
```

`list_connection_assets`、`save_credential`、`get_credential_status`、`delete_credential`、`save_sync_settings`、`backend_status` 这 6 个命令都正确用了 `State<'_, AppState>`，所以它们工作正常——这解释了为什么"新增连接"能保存成功，但"连接"会失败。

### 验证方法

启动桌面客户端后，最直接的验证：

```bash
# 在项目根目录
npm run tauri:dev
```

打开 DevTools Console，执行：

```js
await window.__TAURI__.core.invoke('ssh_connect', {
  host: 'example.com', port: 22, username: 'user',
  password: '', credential_id: null,
  auth_method: 'Password', private_key_path: null,
  passphrase: null, passphrase_credential_id: null
})
```

预期会立即 reject，错误消息包含 "state" / "managed" 字样（具体措辞取决于 Tauri 版本）。

或者打开 `%APPDATA%\com.redtei.myshelltool\logs\myshelltool.log`，应能看到 panic 或 state 解析失败相关日志。

### 修复方案（两种）

#### 方案 A（推荐，改动小）：让 ssh.rs 命令改用 `State<'_, AppState>`

把 `ssh.rs` 23 个命令签名从：

```rust
state: State<'_, Arc<Mutex<SshSessionManager>>>,
```

改为：

```rust
state: State<'_, AppState>,
```

然后在函数体内通过 `state.ssh_sessions.lock().await` 访问 manager。`AppState` 需要从 `lib.rs` 导出（加 `pub struct AppState` 并 `pub` 字段），或把 `AppState` 移到 `ssh.rs`。

辅助函数 `connect_authenticated`、`get_or_create_sftp`（`ssh.rs:192, 634`）的 `state` 参数也需要同步改。

**优点**：最小改动，state 注册不动；6 个已经用 `AppState` 的命令保持不变。

**缺点**：23 处签名 + 2 处辅助函数需要修改，每个调用点 `.lock().await` 之前要加 `.ssh_sessions.`。

#### 方案 B：单独 manage `Arc<Mutex<SshSessionManager>>`

在 `lib.rs:156` 之前加一行：

```rust
app.manage(ssh_mgr.clone());   // 注册 Arc<AsyncMutex<SshSessionManager>>
app.manage(AppState {
    asset_store_path: app_data_dir.join("connection-assets.json"),
    secret_store_dir: app_data_dir.join("credentials"),
    ssh_sessions: ssh_mgr,
});
```

**优点**：`ssh.rs` 0 行改动。

**缺点**：同一个数据被两种类型 manage，容易出现一方被替换、另一方 stale 的设计漂移。但只要 `ssh_mgr` 是同一个 Arc clone，运行期数据一致。

#### 建议

如果短期想最快让 SSH 跑起来，选**方案 B**（1 行改动）；如果想代码结构清晰、避免"同一数据双重 manage"的坑，选**方案 A**。

修复后还需要解决一个潜在的次生问题：`ssh_connect` 命令内部会触发 `check_server_key`（`ssh.rs:54-109`），首次连接会 emit `ssh-host-key-verify` 事件等待前端响应。前端 `setupEventListeners`（`stores/workbench.js:83-106`）正确监听了该事件并弹出确认模态——这部分逻辑是正确的，state bug 修了之后应能正常工作。

---

## 其他附带发现

### A. pnpm 与 npm 混用

- `package.json` scripts: `"dev": "vite --host 127.0.0.1"`（npm 用）
- `tauri.conf.json:7-8`: `"beforeDevCommand": "pnpm run dev -- --port 5173 --strictPort"`、`"beforeBuildCommand": "pnpm run build"`
- 项目根目录有 `package-lock.json`（npm），没有 `pnpm-lock.yaml`

如果开发者机器没装 pnpm，`npm run tauri:dev` / `npm run tauri:build` 在 Tauri 启动前会因 `pnpm not found` 失败。建议把 `tauri.conf.json` 中的 `pnpm run` 统一改为 `npm run`，或在项目根添加 `pnpm-lock.yaml` 并把 `package-lock.json` 删除。

### B. `core:path:default` 已配置但未使用

`src-tauri/capabilities/default.json:17` 包含 `core:path:default`，但 `lib.rs:144` 直接用 `app.path.app_data_dir()?` 而不通过 command 暴露给前端。该权限目前可以保留也可以删除，无功能影响。

### C. 浏览器预览模式的 SFTP 假数据

`services/backend.js:103-115` 的 `sftp_list_dir` 预览实现，返回固定的 4 个假文件（`current / config.toml / logs / backup.tar`）。在浏览器预览模式下打开"文件"标签会看到这些假数据，与 `fallbackAssets` 是同一类问题（参见 2.1）。

### D. 主题 CSS 默认值的隐式约定

`src/styles.css:60` 是 `color-scheme: dark`，所以 `:root` 默认深色。`applyTheme('dark')` 时设置 `data-theme="dark"`，但 CSS 里没有 `[data-theme="dark"]` 选择器——靠的是默认值兜底。这能工作但有点 fragile：如果未来有人不知道这个约定，把 `:root` 的 `color-scheme` 改了，会出现奇怪的表现。建议加一个显式的 `:root[data-theme="dark"]` 选择器，与 light 对称。

---

## 修复优先级建议

| 优先级 | 项 | 工作量 | 影响 |
|---|---|---|---|
| P0 | 修 SSH state 类型不匹配（问题 3，方案 B） | 1 行 | 让核心功能可用 |
| P0 | 修终端"浏览器预览"误导文案（问题 2.3） | 5 行 | 避免已装客户端的用户被误导 |
| P1 | 删除四段 panel header 的设计稿说明（问题 2.2/2.3/2.4） | 删 4 行 | 让产品看起来不像未完成 |
| P1 | 删除 `fallbackAssets` 或改为示例标识（问题 2.1） | 改 1 处 | 避免假数据误导 |
| P2 | 统一 pnpm / npm（附带发现 A） | 改 2 行 | 避免构建环境坑 |
| P2 | 显式 `[data-theme="dark"]` 选择器（附带发现 D） | 加 1 段 | 代码可维护性 |
| 已完成 | UI 主题与系统同步（问题 1） | 本次提交 | — |

---

## 附：本次已修改文件

- `src/stores/workbench.js`：新增三态主题、`prefers-color-scheme` 监听、`effectiveTheme` / `themeLabel` 暴露、`getTerminalTheme` 改用 `effectiveTheme`、清理 listener。
- `src/App.vue`：主题切换按钮文案改为显示当前 `themeLabel`，title 提示循环切换语义。
- 验证：`npm run build` 通过，925ms，25 modules。
