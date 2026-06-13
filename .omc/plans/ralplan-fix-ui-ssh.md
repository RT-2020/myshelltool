# Ralplan: Fix UI Residue & SSH Connection Failure

**Status: pending approval (v2, post-Architect+Critic iteration 1)**
**Mode:** SHORT (RALPLAN-DR, non-deliberate)
**Source analysis:** `docs/ui-ssh-issue-analysis.md`
**Scope:** Tauri backend + Vue frontend cleanup. Excludes already-fixed theme sync (问题 1).
**Lock context:** Tauri = `2.11.2` (per `src-tauri/Cargo.lock:3976-3978`).

---

## 1. RALPLAN-DR Summary

### Principles

1. **修复优先于清理** — 致命的 SSH state bug 必须先修，UI 文案清理次之，可维护性改进最后。
2. **不能破坏已工作的 6 个命令** — `backend_status / list_connection_assets / save_connection_asset / save_sync_settings / save_credential / get_credential_status / delete_credential` 现在用 `State<'_, AppState>` 跑得很好，方案选择必须保证它们继续工作。
3. **最小可验证 diff** — 每一步独立可验证（build pass / invoke 可调 / 肉眼看不到设计稿字样），避免一次性大改。
4. **保留功能性引导** — 删除设计稿文案时不要留"真空"，对用户有意义的引导（"点击 + 新增第一台主机"）可保留。
5. **不让假数据掩盖真相** — `fallbackAssets` / SFTP 假数据这类"看起来像真的但点不动"的内容必须明显标识或删除，不能用真实 IP 假装存在连接。
6. **不变性必须可执行而非靠注释**（v2 新增）— Option B 留下的"同一 Arc 双重 manage"约定若只能用注释维护，下一个重构者移除即回归。P0-1 PR 必须带自动化单元测试断言。

### Decision Drivers (Top 3)

1. **致命 bug 优先级**：所有 23 个 SSH/SFTP/tunnel 命令目前都调不通，整个 SSH 功能不可用 — 任何方案都必须立即解决。
2. **改动量与回归风险**：方案 A（23 处 `#[tauri::command]` 签名 + 2 处辅助函数 = 25 个 State 参数站点）改动面大、回归风险高；方案 B（1 行 + 单元测试）零改动 ssh.rs、无回归。
3. **代码可维护性**：方案 B 留下"同一 Arc 双重 manage"的隐式约定，未来若有人替换 ssh_mgr 实例容易出错；方案 A 把 state 类型归一化更清晰。

### Viable Options — SSH State 修复

#### Option A — 改 `ssh.rs` 23 个 `#[tauri::command]` 签名 + 2 个辅助函数为 `State<'_, AppState>`

- **Pros (bounded):**
  - 类型单一，所有命令统一从 `AppState` 取数据，无歧义。
  - 消除"同一数据双重 manage"的隐患。
  - 与 `lib.rs` 已有的 `app.manage(AppState {...})` 注册方式对称一致。
- **Cons (bounded):**
  - 改动 23 个 `#[tauri::command]` 签名 + 2 个 helper（`connect_authenticated` `ssh.rs:193`、`get_or_create_sftp` `ssh.rs:635`），共 25 个 State 参数站点。
  - 每处调用点 `.lock().await` 之前要加 `.ssh_sessions.`，diff 噪声大。
  - 回归风险较高：任何一个命令漏改或拼错都会编译失败或运行时 panic。
  - `AppState` 需要从 `lib.rs` 导出（`pub struct AppState` + `pub ssh_sessions` 字段）或搬到 `ssh.rs`。

> **脚注（Critic Finding #5 已修正）**：ssh.rs 中 `State<'_, Arc<Mutex<SshSessionManager>>>` 共出现 25 处：23 个 `#[tauri::command]` 命令风格 + 2 个内部辅助函数（`connect_authenticated` @ 行 193、`get_or_create_sftp` @ 行 635，参数带 `&`）。Plan v1 之前混用 "23 / 25 / 25 站点" 系对此数字的口径不一致；v2 统一为 "**23 个命令 + 2 个辅助 = 25 个 State 参数站点**"。

#### Option B — 在 `lib.rs:156` 之前额外 `app.manage(ssh_mgr.clone())`

- **Pros (bounded):**
  - **零** 改动 `ssh.rs`，所有 SSH 命令签名不动。
  - 1 行改动（+注释 + 单元测试，详见 P0-1），最短路径让致命 bug 跑通。
  - 已工作的 6 个 `State<'_, AppState>` 命令完全不受影响。
- **Cons (bounded):**
  - `ssh_mgr: Arc<AsyncMutex<SshSessionManager>>` 同时被 `AppState.ssh_sessions` 和 StateManager 两个键持有 — 同一 Arc clone，运行期数据一致，但语义上"双 manage"对新人不直观。
  - 未来若有人替换 `AppState.ssh_sessions`（重新赋值字段），managed 的独立 Arc 不会跟着换 — 隐式耦合。**v2 缓解**：P0-1 PR 必须包含自动化单元测试（详见 P0-1 / AC-P0-1e），未来若有人删除 `app.manage(ssh_mgr.clone())` 行，CI 中该测试将立即失败。
  - 隐式耦合在 P0-1 PR 内**创建** follow-up 文件 `.omc/plans/followup-ssh-state-unify.md`（有截止日期 + 负责人 + 明确 AC），将其绑定到下一个 sprint 的承诺。

### Option Selection

**选 Option B**（理由见 ADR）。短期价值（让 SSH 立即能用）压倒长期清晰度；隐式耦合通过 (a) 单元测试断言 `Arc::ptr_eq`、(b) 带截止日期的 follow-up 文件 双重缓解。

### Other 改动的选项

#### `fallbackAssets`（`src/services/backend.js:20-29`）

| Option | Pros | Cons | 选择 |
|---|---|---|---|
| 全部删除 | 干净，浏览器预览下侧边栏空白 | 丧失"预览模式有数据可看 UI"的开发体验 | ✗ |
| 改成 `[示例]` 前缀 + `203.0.113.x` 文档段 IP | 保留 UI 演示价值，且明显不可点击 | 8 行编辑 | **✓** |
| 仅在桌面客户端首次启动时显示空状态卡片 | 改动面广（涉及 Vue 组件） | 超出 P1 范围 | ✗ |

选 "改成 [示例] 前缀"，单 viable option，理由：保留开发预览价值同时消除"看起来像真实运维资产"的误导。

#### SFTP 浏览器假数据（`src/services/backend.js:103-115`）

| Option | Pros | Cons | 选择 |
|---|---|---|---|
| 删除假数据，返回空数组 | 与桌面客户端行为一致 | 文件栏永远空白（预览模式） | **✓** |
| 改为 `[示例]` 前缀文件名 | 与 fallbackAssets 一致风格 | 仍然误导用户以为有文件 | ✗ |

选 "删除（返回空）"，单 viable option，理由：文件栏已有"尚未加载 / 点击刷新"空状态文案，不需要假数据填空。

> **Critic Finding #6 不对称说明（v2 增补）**：fallbackAssets 用"[示例]前缀保留演示"，SFTP 预览假数据"完全删除"——同一问题类别但策略相反，原因是**两侧已有兜底不同**：文件栏已有"尚未加载"空状态文案，删除假数据后用户能看到清晰反馈；侧边栏没有空状态卡片兜底，纯删除会让浏览器预览空白一片丧失开发体验。故采用不对称处理。

#### Panel header 描述文案（`App.vue:558, 613, 662, 733`，行号说明见 Minor 1）

| Option | Pros | Cons | 选择 |
|---|---|---|---|
| 全部删除 | 干净 | header 区域只剩标题 | ✗ |
| **替换为对用户有意义的引导语** | 保留视觉结构，文案有用户价值 | 需要 4 处文案撰写 | **✓** |
| 保留原样 | 0 改动 | 仍是设计稿残留 | ✗ |

选 "替换为引导语"，单 viable option，理由：直接删除会让 header 视觉留白；替换为"选择左侧主机开始 / 拖拽文件上传 / Local 与 Dynamic SOCKS 隧道"等功能引导更具产品价值。

---

## 2. Detailed Fix Plan

### P0 — 让核心 SSH 功能可用

#### Task P0-1: 修 SSH state 类型不匹配（方案 B + 强制单元测试）

**文件：**
- `src-tauri/Cargo.toml`（启用 `test` feature）
- `src-tauri/src/lib.rs`（新增 `app.manage(ssh_mgr.clone())` + 注释 + `#[cfg(test)] mod tests`）

**改动 A — `src-tauri/Cargo.toml`：**
```diff
 [dependencies]
-tauri = { version = "2", features = [] }
+tauri = { version = "2", features = ["test"] }
```

> **执行者前置任务**：在 `src-tauri/Cargo.lock` 确认 Tauri 精确版本（当前 `2.11.2`，见 `Cargo.lock:3976-3978`），并查阅 Tauri 2.11.2 文档确认 `tauri::test::mock_builder()` API 名称与签名。已通过 docs.rs 确认 Tauri 2.x 提供 `tauri::test::mock_builder()`（需 `test` feature）。
>
> **若 Tauri 2.11.2 在该 feature flag 下的 API 与预期不同**（例如 `mock_builder` 不可用），执行者必须改用 `tauri::test::mock_app` + `mock_context` + `noop_assets` 组合，或退化为最小化手写 `Builder::default()` setup；**绝不**取消单元测试本身。

**改动 B — `src-tauri/src/lib.rs:155-160`：**
```diff
             let ssh_mgr = Arc::new(AsyncMutex::new(ssh::SshSessionManager::new(
                 app.handle().clone(),
                 app_data_dir.join("credentials"),
                 app_data_dir.join("known_hosts.json"),
             )));
+            // 注册独立的 Arc<AsyncMutex<SshSessionManager>>，供 ssh.rs 中
+            // 23 个 #[tauri::command] + 2 个辅助函数（共 25 个 State 参数站点）
+            // 解析 State<'_, Arc<Mutex<SshSessionManager>>> 时使用。
+            //
+            // 类型精度说明：此处 Mutex 是 tokio::sync::Mutex（lib.rs:9 别名 AsyncMutex），
+            //              ssh.rs:13 同样 use tokio::sync::Mutex — 与 std::sync::Mutex
+            //              是不同的 monomorphized 类型，未来维护时勿混用。
+            //
+            // AppState.ssh_sessions 字段保留同一 Arc clone，用于其他逻辑。
+            // 注意：未来如需替换 manager 实例，必须同时替换 managed Arc；
+            //       此不变性由 #[cfg(test)] mod tests 中的 ssh_state_keys_point_to_same_arc
+            //       测试自动断言（详见文件尾部）。
+            app.manage(ssh_mgr.clone());
             app.manage(AppState {
                 asset_store_path: app_data_dir.join("connection-assets.json"),
                 secret_store_dir: app_data_dir.join("credentials"),
                 ssh_sessions: ssh_mgr,
             });
```

**改动 C — `src-tauri/src/lib.rs` 文件末尾新增 `#[cfg(test)] mod tests`：**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tauri::Manager;

    /// 防回归测试：确保 State<'_, Arc<AsyncMutex<SshSessionManager>>> 与
    /// State<'_, AppState> 两个 TypeId key 指向同一份底层 SshSessionManager。
    ///
    /// 若未来有人移除 `app.manage(ssh_mgr.clone())` 行（lib.rs setup 中），
    /// 此测试将立即失败，避免 ssh.rs 23 个命令静默回归。
    #[test]
    fn ssh_state_keys_point_to_same_arc() {
        // 用 mock_builder 构造一个 MockRuntime App（无 webview，CI 友好）。
        // Tauri 2.11.2 在 `test` feature 下提供 tauri::test::mock_builder()。
        // 若该 API 在当前版本签名不同，执行者按下方"前置任务"调整为
        // mock_app / mock_context + noop_assets 组合。
        let app = tauri::test::mock_builder()
            .build(tauri::generate_context!())
            .expect("mock app build failed");

        let handle = app.handle().clone();

        // 复用 run() 的 setup 形态：构造 ssh_mgr，分别 manage 两次。
        let ssh_mgr = Arc::new(AsyncMutex::new(ssh::SshSessionManager::new(
            handle.clone(),
            std::env::temp_dir().join("myshelltool-test-credentials"),
            std::env::temp_dir().join("myshelltool-test-known_hosts.json"),
        )));
        app.manage(ssh_mgr.clone());
        app.manage(AppState {
            asset_store_path: std::env::temp_dir().join("myshelltool-test-assets.json"),
            secret_store_dir: std::env::temp_dir().join("myshelltool-test-credentials"),
            ssh_sessions: ssh_mgr,
        });

        // 两个 TypeId key 必须都能解析，且指向同一份 Arc 内的 SshSessionManager。
        let standalone: Arc<AsyncMutex<ssh::SshSessionManager>> =
            app.state::<Arc<AsyncMutex<ssh::SshSessionManager>>>().inner().clone();
        let from_app_state: Arc<AsyncMutex<ssh::SshSessionManager>> = {
            let app_state = app.state::<AppState>();
            Arc::clone(&app_state.ssh_sessions)
        };

        assert!(
            Arc::ptr_eq(&standalone, &from_app_state),
            "StateManager 中 Arc<AsyncMutex<SshSessionManager>> 与 AppState.ssh_sessions \
             必须指向同一 Arc；若此断言失败，请检查 lib.rs setup 中是否漏掉 \
             `app.manage(ssh_mgr.clone())` 行"
        );
    }
}
```

> **执行者注意事项**：
> 1. `app.state::<T>()` 在 Tauri 2.x 返回 `State<'_, T>`；访问内部 Arc 需 `.inner()` 或解引用。具体形式以 Tauri 2.11.2 API 为准，执行者编译时按编译器提示调整。
> 2. `ssh::SshSessionManager::new(...)` 的参数列表来自 `lib.rs:151-155`；若签名不同（例如不接受 `app.handle()`）按现状对齐。
> 3. 此测试在 CI 中无需图形界面（MockRuntime），可在 cargo test 下直接运行。

**验证方法：**
1. `cd src-tauri && cargo check` exit 0。
2. **`cd src-tauri && cargo test ssh_state_keys_point_to_same_arc`** exit 0（**v2 新增的 AC-P0-1e**）。
3. `npm run tauri:dev` 启动桌面客户端（**注意：当前 `tauri.conf.json:7-8` 用 `pnpm run`，验证者必须先装 pnpm，或先合并 P2-1**），DevTools Console 执行：
   ```js
   await window.__TAURI__.core.invoke('ssh_connect', {
     host: '<test-host>', port: 22, username: '<test-user>',
     password: '<test-password>', credential_id: null,
     auth_method: 'Password', private_key_path: null,
     passphrase: null, passphrase_credential_id: null
   })
   ```
   预期：不再因 state 解析失败立即 reject；触发 `ssh-host-key-verify` 事件或正常建立连接（错误应来自 SSH 协议本身，而非 Tauri State 解析）。
4. 检查 `%APPDATA%\com.redtei.myshelltool\logs\myshelltool.log` 无 "state" / "managed" 相关错误。

#### Task P0-2: 修终端"浏览器预览模式"误导文案（使用 store.backendStatus.mode）

**文件：** `src/App.vue`
**行号：** 632（当前）— 此行内容是 `<div v-if="!activeSession" style="...">SSH 终端需要桌面客户端。当前为浏览器预览模式。</div>`
**改动类型：** 替换 1 行为条件渲染

**Diff 摘要（v2 修订 — 解决 v1 的 isTauriRuntime 双重错误）：**
```diff
-                    <div v-if="!activeSession" style="padding:var(--space-4);color:var(--muted)">SSH 终端需要桌面客户端。当前为浏览器预览模式。</div>
+                    <div v-if="!activeSession" style="padding:var(--space-4);color:var(--muted)">
+                      <span v-if="store.backendStatus.mode !== 'tauri-core'">SSH 终端需要桌面客户端。当前为浏览器预览模式。</span>
+                      <span v-else>请点击左侧主机建立 SSH 连接。</span>
+                    </div>
```

**前置确认（v2 已验证）：**
- `store` 在 setup scope 中（`App.vue:6` `const store = useWorkbenchStore()`），模板可直接通过 `store.` 前缀访问。
- `backendStatus` 已从 store return（`stores/workbench.js:790`），无需改 `storeToRefs` 解构列表。
- `backendStatus.mode` 在 Tauri 桌面为 `"tauri-core"`（`src-tauri/src/lib.rs:32-35`），在浏览器预览为 `"browser-preview"`（`src/services/backend.js:45`），fallback 为 `"fallback"`（`stores/workbench.js:87`）。
- 已有先例：`App.vue:114` 的 `titleChip` 用了 `store.backendStatus.ready`。
- **不需要新增 import**；v1 中错误的 `!isTauriRuntime` 写法已废弃（`isTauriRuntime` 是函数引用，truthy，`!isTauriRuntime` 永远 false，浏览器预览分支永远不渲染）。

**验证方法：**
1. `npm run build` exit 0。
2. 浏览器预览模式（`npm run dev`，无 Tauri runtime，`mode === "browser-preview"`）下，未连接时显示"SSH 终端需要桌面客户端。当前为浏览器预览模式。"
3. fallback 模式（`mode === "fallback"`，例如 backend 适配层初始化失败）下，同样显示"浏览器预览模式"分支（因 `!== "tauri-core"` 涵盖此情况）。
4. 桌面客户端（`mode === "tauri-core"`）未连接时显示"请点击左侧主机建立 SSH 连接。"

---

### P1 — 产品完整度

#### Task P1-1: 清理 4 处 panel header 设计稿说明文案

**文件：** `src/App.vue`
**行号（v2 修正，Critic Finding #4）：**

> **Minor 1 行号澄清**：v1 原写 `558 / 613 / 662 / 733` 指代"`<p>` 段落"，但 Critic 实测发现 558 实际是 `<h1>当前运维上下文</h1>`、559 才是 `<p>`；733 是隧道 `<h1>` 标题、734 才是 `<p>`。v2 改用**内容锚点**描述避免再次漂移：

| 内容锚点 | 原文（设计稿残留 `<p>`） | 替换为 |
|---|---|---|
| 概览面板 `<h1>当前运维上下文</h1>` 紧下方的 `<p>` | 聚合真实资产、活跃会话、传输队列、后端桥接和风险状态，让第一屏回到设计稿的工作台形态。 | 选择左侧主机开始工作，或新建连接资产。 |
| 终端面板 header `<h1>终端</h1>` 紧下方的 `<p>` | 终端保持最大可读面积，右侧只展示当前路径文件和会话摘要；浏览器预览会明确提示需要桌面客户端。 | 通过 xterm.js 与远程主机交互，右侧实时显示当前目录文件。 |
| 文件面板 header `<h1>文件传输</h1>` 紧下方的 `<p>` | 保留设计稿的本地 / 远程双栏结构，远程列表来自后端目录命令，本地栏展示真实传输队列。 | 左侧拖拽文件上传，右侧浏览远程目录；下方查看传输队列进度。 |
| 隧道面板 header `<h1>隧道</h1>` 紧下方的 `<p>` | Local、Dynamic SOCKS 通过 backend adapter 读写；Remote 转发暂不支持，已标注禁用。 | 配置本地端口转发或 SOCKS 代理；Remote 转发暂不支持。 |

**验证方法：**
1. `npm run build` exit 0。
2. `npm run tauri:dev`，依次切换 4 个 tab（overview / terminal / files / tunnels），header 副标题 `<p>` 不再出现"设计稿"、"backend adapter"、"工作台形态"等元描述字样。

#### Task P1-2: `fallbackAssets` 改为示例标识

**文件：** `src/services/backend.js`
**行号：** 20-29
**改动类型：** 替换（重写 8 个 makeAsset 的 id/name/IP）

**Diff 摘要：**
```diff
 export const fallbackAssets = [
-  makeAsset('prod-bastion', 'prod-bastion', '10.10.4.8', 'root', ...),
-  makeAsset('web-01', 'web-01', '10.10.8.21', 'deploy', ...),
-  // ...
+  makeAsset('example-bastion', '[示例] 堡垒机', '203.0.113.10', 'demo', ...),
+  makeAsset('example-web-01', '[示例] Web 服务器', '203.0.113.11', 'demo', ...),
+  // ... 全部加 [示例] 前缀，IP 改为 RFC 5737 文档段 203.0.113.x
 ];
```

**验证方法：**
1. `npm run build` exit 0。
2. `npm run dev`（浏览器预览），侧边栏 8 个主机的名字全部带 `[示例]` 前缀，IP 全部在 `203.0.113.x` 段。
3. 桌面客户端首次启动（清空 `connection-assets.json`）侧边栏为空（不显示 fallback，因 `invokeBackend` 走真实路径）。

#### Task P1-3: 删除 SFTP 浏览器预览假数据

**文件：** `src/services/backend.js`
**行号：** 103-115
**改动类型：** 替换（`readBrowserSftpListDir` 返回空数组）

**Diff 摘要：**
```diff
 async function readBrowserSftpListDir(path) {
-  // 返回固定的 4 个假文件（current / config.toml / logs / backup.tar）
-  return [
-    { name: 'current', kind: 'dir', size: 0, modified: '2026-06-01' },
-    ...
-  ];
+  // 浏览器预览模式无法访问真实远程文件系统，返回空数组。
+  // 文件栏空状态文案由 App.vue 中的 "尚未加载 / 点击刷新" 处理。
+  // 不对称说明：与 fallbackAssets 的 "[示例]" 策略不同——文件栏已有
+  //            空状态文案兜底，无需假数据填空；侧边栏无兜底故保留示例。
+  return [];
 }
```

**验证方法：**
1. `npm run build` exit 0。
2. 浏览器预览模式下打开"文件"tab，远程栏显示"尚未加载"空状态，不再有假文件。

#### Task P1-4: 文件栏占位行文案优化（可选，与 P1-1 合并执行）

**文件：** `src/App.vue`
**行号：** 643, 698, 725
**改动类型：** 替换文案

| 行号 | 原文 | 替换为 |
|---|---|---|
| 643 | 打开文件页后加载远程目录。 | 连接主机后此处显示远程文件。 |
| 698 | 点击刷新读取远程目录 | 点击"刷新"读取远程目录列表 |
| 725 | 传输通过 SFTP 进度事件实时推送，完成后保留 60 秒便于复查。 | 传输进度实时更新；任务完成后保留 60 秒以便复查。 |

**验证方法：** 肉眼检查 + `npm run build` 通过。

---

### P2 — 可维护性

#### Task P2-1: pnpm/npm 统一

**文件：** `src-tauri/tauri.conf.json`
**行号：** 7-8
**改动类型：** 替换（`pnpm run` → `npm run`）

**Diff 摘要：**
```diff
   "build": {
-    "beforeDevCommand": "pnpm run dev -- --port 5173 --strictPort",
-    "beforeBuildCommand": "pnpm run build",
+    "beforeDevCommand": "npm run dev -- --port 5173 --strictPort",
+    "beforeBuildCommand": "npm run build",
```

**理由：** 项目根有 `package-lock.json`（npm）无 `pnpm-lock.yaml`，pnpm 不是开发者默认环境；选 npm 改动最小。

**验证方法：**
1. 卸载 pnpm 或在 PATH 中隐藏 pnpm。
2. `npm run tauri:dev` 启动成功（不再因 `pnpm not found` 失败）。
3. `npm run tauri:build` 产出 `release/` 目录。

#### Task P2-2: 显式 `[data-theme="dark"]` 选择器

**文件：** `src/styles.css`
**行号：** 在 `:root` 块（约 1-50）之后、`:root[data-theme="light"]` 之前插入
**改动类型：** 新增（复制 `:root` 默认深色变量到 `:root[data-theme="dark"]`）

**Diff 摘要：**
```diff
 :root {
   /* 现有深色变量定义保持不变 */
   color-scheme: dark;
   --app-bg: ...;
   ...
 }
+
+/* 显式 [data-theme="dark"] 选择器：与 light 对称，
+   消除"默认即深色"的隐式约定，避免未来改 :root 时出错。 */
+:root[data-theme="dark"] {
+  color-scheme: dark;
+  --app-bg: ...;       /* 与 :root 相同的值 */
+  --app-fg: ...;
+  /* ... 所有 --app-* 变量 ... */
}

 :root[data-theme="light"] {
   /* 现有浅色覆盖 */
 }
```

**验证方法：**
1. `npm run build` exit 0。
2. 桌面客户端三态切换：system / light / dark，每种主题下变量正确生效（深色背景 / 浅色背景 / 跟随系统）。

---

## 3. ADR — SSH State Fix

- **Decision：** 采用 Option B — 在 `lib.rs:156` 之前额外 `app.manage(ssh_mgr.clone())`，让 `ssh.rs` 23 个 `#[tauri::command]` 命令的 `State<'_, Arc<Mutex<SshSessionManager>>>` 类型能被 StateManager 查找到。
- **Drivers：**
  1. 致命 bug — 所有 SSH/SFTP/tunnel 命令当前完全不可用，需要最快路径修复。
  2. 零回归 — 6 个已工作的 `State<'_, AppState>` 命令完全不受影响。
  3. 最小 diff — 1 行改动 + 4 行注释 + 1 个单元测试（~25 行），可立即 review、立即验证、CI 自动断言。
- **Alternatives considered：**
  - **Option A**（改 `ssh.rs` 25 个 State 参数站点签名为 `State<'_, AppState>`）：长期更清晰，但 P0 阶段改动面过大、回归风险高、需要导出 `AppState`。已绑定 follow-up 文件 `.omc/plans/followup-ssh-state-unify.md` 在 P0-1 PR 内创建，承诺 ≤2 个 sprint 完成。
- **Why chosen：**
  - 短期价值（让 SSH 立即可用）压倒长期清晰度；
  - 运行期数据一致性保证（同一 Arc clone，所有持有者看到同一份 SshSessionManager）；
  - 隐式耦合通过 (a) `ssh_state_keys_point_to_same_arc` 单元测试自动断言、(b) 带截止日期的 follow-up 文件双重缓解。
- **Consequences：**
  - 正面：SSH 功能立即可用；改动 1 行可立即 review；未来若有人删除 `app.manage(ssh_mgr.clone())` 行，`cargo test ssh_state_keys_point_to_same_arc` 立即失败。
  - 负面：`Arc<Mutex<SshSessionManager>>` 与 `AppState.ssh_sessions` 双重持有同一 Arc；未来若有人重构 `AppState` 字段，可能忘记同步 managed 独立 Arc；新人理解需要看注释和测试。
- **Follow-ups（v2 修订 — Critic Finding 强制绑定）：**
  1. **P0-1 PR 内创建** `.omc/plans/followup-ssh-state-unify.md`（**不再是"稍后开 issue"**）：包含范围（ssh.rs 23 个 `#[tauri::command]` + 2 个辅助函数 = 25 个 State 参数站点）、目标日期（**2026-06-26，即 P0-1 合并后 ≤2 个 sprint**）、负责人（TBD，P0-1 合并后 3 天内分配）、明确 AC（见文件）。
  2. 重构时把 `AppState` 的 `ssh_sessions` 字段改名为 `sessions`（更短），并移到 `ssh.rs` 中定义，减少跨文件依赖。
  3. ~~考虑添加编译期断言或测试~~（**v2：已升级为 P0-1 必交项 `ssh_state_keys_point_to_same_arc`，不再是 follow-up**）。

---

## 4. 验收标准（Acceptance Criteria）

### P0

- [ ] **AC-P0-1a:** `cd src-tauri && cargo check` exit code 0。
- [ ] **AC-P0-1b:** `npm run tauri:dev` 启动后（验证者必须先装 pnpm，或先合并 P2-1），DevTools Console 执行 `ssh_connect` invoke，错误消息**不再**包含 "state" / "managed" / "TypeId" 等字样（错误应来自 SSH 协议本身或主机密钥确认流程）。
- [ ] **AC-P0-1c:** 实际场景：在桌面客户端点 `+` 新增一台测试 SSH 主机，点击"连接并打开终端"，终端区域出现 PTY 提示符（如 `user@host:~$`）而非红色 `\x1b[31mError: ...\x1b[0m`。
- [ ] **AC-P0-1d:** 在已建立的 SSH 会话中敲键盘，命令字符回显正常（验证 `ssh_write` 也工作）。
- [ ] **AC-P0-1e（v2 新增）:** `cd src-tauri && cargo test ssh_state_keys_point_to_same_arc` exit 0；若未来有人移除 `app.manage(ssh_mgr.clone())` 行，此测试必须失败。
- [ ] **AC-P0-1f（v2 新增）:** `.omc/plans/followup-ssh-state-unify.md` 文件在 P0-1 PR 中创建，包含范围、目标日期（2026-06-26）、负责人占位（TBD）、明确 AC。
- [ ] **AC-P0-2a:** `npm run build` exit 0。
- [ ] **AC-P0-2b:** 浏览器预览模式（`mode === "browser-preview"`）下，未连接时终端占位文案为"SSH 终端需要桌面客户端。当前为浏览器预览模式。"
- [ ] **AC-P0-2c:** fallback 模式（`mode === "fallback"`）下，未连接时同样显示"浏览器预览模式"分支文案。
- [ ] **AC-P0-2d:** 桌面客户端（`mode === "tauri-core"`）未连接时，终端占位文案为"请点击左侧主机建立 SSH 连接。"

### P1

- [ ] **AC-P1-1a:** `npm run build` exit 0。
- [ ] **AC-P1-1b:** 依次切换 overview / terminal / files / tunnels 4 个 tab，header 副标题（`<p>` 段落）中**不再出现**字符串 "设计稿"、"backend adapter"、"工作台形态"。
- [ ] **AC-P1-2a:** `npm run build` exit 0。
- [ ] **AC-P1-2b:** 浏览器预览模式下，侧边栏所有主机名称均以 `[示例]` 前缀开头。
- [ ] **AC-P1-2c:** 浏览器预览模式下，侧边栏所有主机 IP 均在 `203.0.113.0/24` 网段（RFC 5737 文档段）。
- [ ] **AC-P1-3a:** `npm run build` exit 0。
- [ ] **AC-P1-3b:** 浏览器预览模式下打开"文件"tab，远程栏显示空状态文案（"尚未加载"），不再出现 `current / config.toml / logs / backup.tar` 等假文件。
- [ ] **AC-P1-4a:** `npm run build` exit 0；P1-4 列表中 3 处文案肉眼检查更新。

### P2

- [ ] **AC-P2-1a:** `npm run tauri:dev` 在未安装 pnpm 的环境下成功启动（不再因 `pnpm not found` 失败）。
- [ ] **AC-P2-1b:** `npm run tauri:build` 成功产出 `release/` 目录中的安装包。
- [ ] **AC-P2-2a:** `npm run build` exit 0。
- [ ] **AC-P2-2b:** 桌面客户端依次切换 system → light → dark 三态主题，深色模式下所有 `--app-*` 变量与改动前视觉一致（无颜色漂移）。

---

## 5. 执行顺序建议

1. **PR #1 = P0-1 + P0-2 + followup-ssh-state-unify.md 文件**（强制绑定）→ 立即用桌面客户端实测 SSH 连接。
   - **验证者注意**：当前 `tauri.conf.json:7-8` 用 `pnpm run`，PR #1 验证必须先装 pnpm；或把 P2-1 移入 PR #1（**不推荐**，PR #1 已含后端 + 前端 + 测试 + follow-up，不再加 build config 改动）。
2. PR #2 = P1-1 + P1-4（panel 文案）→ 纯文案。
3. PR #3 = P1-2 + P1-3（假数据）→ 仅改 `backend.js`。
4. PR #4 = P2-1 + P2-2（可维护性）→ 独立改动。

每个 PR 独立可回退。P0 必须先 merge 并验证后再处理 P1/P2。

---

## 6. Out of Scope（明确不在此 plan 处理）

- 主题系统同步（问题 1，已修复）。
- `core:path:default` 权限清理（附带发现 B，无功能影响）。
- Option A 的 `ssh.rs` 25 个 State 参数站点签名重构 → 已绑定到 follow-up 文件 `.omc/plans/followup-ssh-state-unify.md`（目标 2026-06-26）。
- 工作区总览面板的 `status-pill.running` 硬编码"状态已接入"绑定真实状态（分析报告 2.5 提到，但属于状态展示优化，非设计稿残留清理，独立 follow-up）。

---

## 7. Open Questions

见 `.omc/plans/open-questions.md`（如不存在则创建）。

**v2 新追加的 open questions（来源：本轮 iteration 中 Architect/Critic findings）：**

- 执行者在实现 P0-1 单元测试时，需在 `Cargo.lock` / docs.rs 确认 Tauri 2.11.2 `tauri::test::mock_builder()` 的精确签名；若 API 与 diff 中示例不同，按编译器提示调整（如 `mock_app + mock_context + noop_assets` 组合）。此为执行细节，不阻塞 plan 批准。

---

## 8. CI 检查说明（v2 新增 — Critic 缺失项）

**当前项目无 CI：** `.github/workflows/` 目录不存在（已通过 Glob 确认）。

**未来若添加 CI，必须包含：**
- `cd src-tauri && cargo test`（特别是 `ssh_state_keys_point_to_same_arc` 测试，防止 Option B 不变性被未来重构破坏）
- `cd src-tauri && cargo check`
- `npm run build`

在 CI 落地前，AC-P0-1e（cargo test）必须在 PR review 时由验证者本地执行。

---

## 9. v2 修订记录（本轮 Architect+Critic iteration 1）

本轮修订基于 `.omc/plans/ralplan-architect-review.md`（Verdict: ITERATE）和 Critic 内联 findings，解决了 **3 个阻塞性修订** + **5 个 Minor findings**：

### 阻塞性修订（P0）

1. **修订 1（P0-2 diff 双重错误）— 已修复**
   - v1 问题：用 `<span v-if="!isTauriRuntime">` — (a) `App.vue` 没 import `isTauriRuntime`，(b) 即使 import 了，`isTauriRuntime` 是函数引用（truthy），`!isTauriRuntime` 永远 false，浏览器预览分支永远不渲染。
   - v2 修复：改用 `store.backendStatus.mode !== 'tauri-core'`。已验证 `store` 在 setup scope（`App.vue:6`）、`backendStatus` 已 return（`workbench.js:790`）、`mode` 在 Tauri 为 `"tauri-core"`、浏览器为 `"browser-preview"`、fallback 为 `"fallback"`；有先例 `App.vue:114`。
   - 同步澄清 AC-P0-2b（覆盖 `browser-preview`）和 AC-P0-2c（覆盖 `fallback`）。

2. **修订 2（P0-1 缺少回归测试）— 已修复**
   - v1 问题：P0-1 只有 `cargo check`（AC-P0-1a）和手动 E2E（AC-P0-1b/c/d），Option B 的不变性只靠注释维护，下一个重构者移除 `app.manage(ssh_mgr.clone())` 不会触发编译错误。
   - v2 修复：(a) `Cargo.toml` 启用 `tauri` 的 `test` feature；(b) `lib.rs` 文件末尾新增 `#[cfg(test)] mod tests` 含 `ssh_state_keys_point_to_same_arc` 测试，用 `tauri::test::mock_builder()` + `Arc::ptr_eq` 断言两个 TypeId key 指向同一 Arc；(c) 新增 AC-P0-1e 要求 cargo test exit 0。
   - 执行者前置任务明确：在 `Cargo.lock` 确认 Tauri 精确版本（已确认 `2.11.2`），查阅 docs.rs 确认 API 名称（已通过 WebSearch 确认 `tauri::test::mock_builder()` 存在）。

3. **修订 3（Option A follow-up 未承诺）— 已修复**
   - v1 问题：ADR 后续 #1 写"单独开 issue：在下一个迭代周期内重构"——无截止日期、负责人、文件路径、issue 链接，"稍后"=永远不会做。
   - v2 修复：(a) P0-1 PR 内**创建** `.omc/plans/followup-ssh-state-unify.md` 文件，强制绑定到 P0-1 提交；(b) 文件包含范围（ssh.rs 23 命令 + 2 辅助 = 25 站点 + lib.rs AppState 改 `pub`）、目标日期（2026-06-26）、负责人（TBD，P0-1 合并后 3 天内分配）、明确 AC（删除 v2 新增的 manage 行 + 删除注释 + 删除 Arc::ptr_eq 测试 + 改 ssh.rs 25 处签名 + 全部 cargo test 通过 + 桌面 E2E）；(c) 新增 AC-P0-1f 要求文件存在。

### Minor findings

4. **Minor 1（行号偏移，Critic Finding #4）— 已修复**
   - v1 问题：P1-1 引用 `App.vue:558` 是 `<p>`，但实测 558 是 `<h1>`，559 才是 `<p>`；733 同理。
   - v2 修复：P1-1 表格改用内容锚点描述（"概览面板 `<h1>当前运维上下文</h1>` 紧下方的 `<p>`"），不再依赖脆弱的行号。

5. **Minor 2（ssh.rs 计数，Critic Finding #5）— 已修复**
   - v1 问题：交替写"23 个命令"、"25 个签名"、"25 个站点"，口径混乱。
   - v2 修复：在 Option A 描述和 ADR Follow-up #1 加脚注，统一为"23 个 `#[tauri::command]` + 2 个内部辅助函数（行 193, 635）= 总共 25 个 State 参数站点"。

6. **Minor 3（fallbackAssets 与 SFTP 不对称，Critic Finding #6）— 已修复**
   - v1 问题：fallbackAssets 用"[示例]前缀"，SFTP 假数据"完全删除"，同问题类别相反策略未说明。
   - v2 修复：在 SFTP diff 注释和 RALPLAN-DR Summary 增补"Critic Finding #6 不对称说明"，明确文件栏已有空状态文案兜底、侧边栏没有，故采用不对称处理。

7. **Minor 4（P0-1 验证顺序依赖 pnpm，Critic 缺失项）— 已修复**
   - v1 问题：AC-P0-1b/c/d 需要 `npm run tauri:dev`，但 `tauri.conf.json:7-8` 用 `pnpm run`，验证者若未装 pnpm 会被未合并的 P2-1 阻塞。
   - v2 修复：在 P0-1 验证方法和执行顺序建议（PR #1）明确"验证者必须先装 pnpm，或先合并 P2-1"；并说明为什么不把 P2-1 移入 PR #1（PR #1 已含后端 + 前端 + 测试 + follow-up，不再加 build config 改动）。

8. **Minor 5（CI 检查，Critic 缺失项）— 已修复**
   - v1 问题：新加的 cargo test 应该在 CI 上跑，但项目无 `.github/workflows/`。
   - v2 修复：新增第 8 节"CI 检查说明"，明确当前无 CI、未来若添加必须包含 `cargo test`（特别是 `ssh_state_keys_point_to_same_arc`）+ `cargo check` + `npm run build`；在 CI 落地前，AC-P0-1e 必须在 PR review 时由验证者本地执行。

### 未修订的项（已正确）

- Option B 选择（短期价值压倒长期清晰度）— 保留。
- PR splitting（4 个独立可回退 PR）— 保留。
- P0-1 AC 集合（cargo check + invoke + PTY + ssh_write）— 保留并增补。
- P1/P2 scope 和文案 — 保留。
- Out-of-scope 列表（status-pill.running、core:path:default）— 保留。
