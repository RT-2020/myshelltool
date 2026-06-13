# Ralplan Architect Review — Framework Choice ADR (v1)

> **Reviewer:** Architect (opus)
> **Target:** `framework-choice-tauri-vs-qt-vs-electron.md` v1
> **Date:** 2026-06-13
> **Mode:** Deliberate（强制 steelman + tradeoff tension + principle-violation flags）
> **Verdict:** **ITERATE**（推荐方案 A 的结论成立，但工作量估算与若干风险条目需要修订后再 approve）

> **注：** 本文为 Architect agent 直接输出的评审正文（该 agent 只读，由 ralplan 协调器代为落盘）。原汁原味保留，未做改动。

---

## Summary

Planner 推荐方案 A（继续 Tauri）的方向**正确**，核心三论据（已验证逻辑优先、痛点非框架本质、单人维护者认知负荷）也成立。但 Planner 在**工作量估算的精度**（IPC OOM 改流式 + 测试代码联动）、**方案 B 的备选项漏评估**（Slint/egui 等更适合 Rust 后端的桌面框架未列）、**回滚策略的触发条件过于代码侧**（缺用户主观体验软触发）三处存在可被外部反论攻破的弱点。这些弱点不改变最终结论，但需要修订后才能转为 `Status: approved (v2)`。

---

## 1. Steelman 反论（最重要）

### 反论 1（成立度：**部分成立**）— Planner 低估了方案 A 的真实工作量

**反论核心：** "8-12 人天"的估算在两点上乐观。

**证据 A — IPC OOM 前端签名必须连带改。** Planner 在 ADR 1.3 节只说"把 `content: Vec<u8>` 改为流式"，但实际前端代码不是这么调的。`src/stores/workbench.js:316-317` 的实际调用是：

```js
const buffer = new Uint8Array(await file.arrayBuffer());
await invokeBackend('sftp_upload_with_progress', {
  sessionId: session.sessionId,
  remotePath: remoteTarget,
  content: Array.from(buffer),  // ← 关键：每字节是个 JS Number，过 JSON 序列化
  transferId
});
```

前端**主动把 `Uint8Array` 转成 JS Number 数组**再过 IPC。这意味着 OOM 修复不只是后端签名问题——前端 transferQueue 调用模式必须改成"分块读取 + 多次 IPC 上传 chunk"。这是 `transferQueue`（`workbench.js:301-315`）状态机的**结构性改动**，不是改一行签名。

**证据 B — 测试代码必须联动重写。** Planner 在 ADR open question #2 自己承认没算这部分。实读：

- `tests/ui-smoke.mjs:38` 强断言 `已连接 · browser-preview`
- `tests/ui-smoke.mjs:42-47` 断言终端 fallback 消息
- `tests/ui-extended.mjs:49` 断言 `[data-remote-path]` 为 0（依赖 browser-preview 不返回真实文件）
- `src-tauri/src/lib.rs:232-280` 的源码级回归测试（守护双 manage 不变量）

方案 A 第 1 步（Option A 重构）会让 `lib.rs` 的 `ssh_state_keys_point_to_same_arc` 测试**直接失效**（follow-up plan `followup-ssh-state-unify.md` AC-FU-6 已承认要删/替换）。方案 A 第 3 步（删 invokeBrowserPreview）会让两个 Playwright 测试**整段失败**，必须改为"启动 tauri:dev 跑端到端"或"删除 browser-preview 断言"。这部分工作量 Planner 没估。

**我的评估：** 反论成立。Planner 的 8-12 人天漏了：(a) 前端 transferQueue 分块上传/下载的状态机重写（2-3 天，不是 0）；(b) Playwright 测试改写 + Tauri 模式 E2E 重建（2-3 天，不是 1.5 天的"测试"）。**修订后真实工作量应为 12-18 人天**，仍然远低于 B/C，但不改变结论方向。

---

### 反论 2（成立度：**成立**）— Planner 漏评估了方案 B 的更优子选项

**反论核心：** ADR 3.2 节评估的 Qt 路线只考虑了 PySide6 + paramiko（B1）和 Rust + qmetaobject + russh（B2）。但 2026 年 Rust 生态里**已经有比 qmetaobject 更成熟的 Rust 原生 GUI 框架**，特别适合"已有 Rust 后端 + 想摆脱 WebView"的场景：

- **Slint**（Rust 原生声明式 UI，Tauri 生态友好，2024 年后稳定）：声明式语法、Windows 支持良好、商业许可宽松。如果保留 russh 后端 + 切前端到 Slint，**保留 russh 这一核心资产的同时摆脱了 WebView 依赖**——比 qmetaobject 更工程化。
- **egui**（immediate mode，适合终端/工具型应用）：对 SSH 客户端这种"长时间挂机 + 大量文本输出"的场景，egui 的 immediate mode 性能可能优于 retained-mode QML。

**ADR 当前评估的盲点：** ADR 假设"切 Qt = 切 Python/QML = 重学一个新栈"，但**如果保留 Rust + 切 Slint**，学习曲线陡峭度大幅下降（Rust 已掌握）。B2 被否决的理由"qmetaobject 生态不成熟"在 2026 年的 Rust 桌面生态里**有更好的替代品**。

**我的评估：** 反论成立但不致命。Planner 的"为什么 B 不推荐"在 PySide6 子选项下成立，但漏了 B3（Slint）/B4（egui）评估。即便如此，**Slint 仍然要求重写全部前端 Vue 代码**——这违反 Planner 原则 1（已验证逻辑优先），且 egui 没有成熟的 SSH 终端组件，仍要自己写。**结论不改变**（A 仍胜出），但 ADR 应当补 B3/B4 一节避免被未来 review 时批评"框架评估不完整"。

---

### 反论 3（成立度：**部分成立**）— "Tauri 2.x 长期风险评分 3 分"可能被低估

**反论核心：** ADR 加权表第 8 行给 Tauri 长期维护风险 3 分，理由仅"三者都活着"。但项目已经踩到 `State<T>` 的 TypeId 陷阱（P0-1 PR 的整个起因），这不是孤立事件——它揭示的是 Tauri 2.x 的**核心 state 管理机制本身的类型擦除设计**。State<T> 用 TypeId 索引 Arc，意味着任何在 `app.manage(X)` 调用顺序、clone 时机、类型可见性上的工程化决策都可能让 state 在运行期静默失败（而非编译期报错）。

**反论证据：**

1. `src-tauri/src/lib.rs:157-166` 当前依赖**双重 manage**（同一 Arc 注册两个 TypeId key）才工作。这种 hack 在 Tauri 官方文档里**没有任何推荐范式**——它依赖的是 TypeId 注册语义的实现细节。
2. `lib.rs:232-280` 的源码级回归测试本身就是为了对抗"未来 Tauri 升级 / 重构时不小心删掉这行"的风险。**这种"需要写源码扫描测试来守护框架特性"的本身就是高维护成本的信号**。
3. Tauri 2.x 在 2026 H2 仍有 dynamic plugin / IPC protocol 层的演进计划（基于官方 changelog 节奏），下一次破坏性变更可能在 state 机制之外（如 event channel、permission system）。

**我的评估：** 反论部分成立。R1 风险条目已经识别到这点，但缓解措施"锁定版本 + 回归测试"只是**事后补偿**，没有量化"Tauri 升级一次的隐性成本"。这风险**不改变** A 仍是最低成本方案（即便加 5 人天做版本升级的迁移 buffer，仍是 17-23 天 vs B 的 60-90 天），但 ADR 应该把"长期维护风险"评分从 3 调到 **2.5-3**，并在 Consequences 节明确"Tauri 版本升级每次预估 1-3 人天迁移成本"。

---

### 反论 4（成立度：**不成立但需要 ADR 显式反驳**）— Electron 的 ssh2 在 2026 年是否真的弱于 russh

**反论核心（最强版本）：** ADR 断言"ssh2 (Node.js) 在动态 SOCKS5 转发上功能弱于 russh"，但没引文档。Open question #3 自己也承认"需要外部文档核实"。如果 ssh2 在 2024-2026 年的版本里补齐了 SOCKS5/键盘交互认证，方案 C 的"仍需修 IPC OOM + ssh2 弱"两大否决理由就只剩一个，方案 C 的相对吸引力会上升。

**我的评估：** 反论表面成立但**实质不成立**。实读 `ssh.rs:1183-1347` 的 `handle_socks5_connection`——当前项目的 SOCKS5 是**自实现的**（手写握手 + `channel_open_direct_tcpip`），russh 本身不提供 SOCKS5 协议层，只提供 SSH 通道。这意味着：

- **切到 ssh2 后，SOCKS5 仍然要自己实现**（因为 ssh2 也只提供 channel API，SOCKS5 协议层是项目自写的）
- 唯一差异是 `channel_open_direct_tcpip` 的 API 形状——ssh2 (Node.js) 确实有此 API
- 所以"ssh2 弱于 russh"在**隧道功能上**其实**是错的**——两边都要自实现 SOCKS5，功能对等

但 ADR 的否决理由在**另一个维度**仍成立：russh 的 `check_server_key` 异步回调 + oneshot channel 模式（`ssh.rs:54-110`）做的"前端确认 host key"交互，在 ssh2 里需要不同的事件循环模型重写。**ADR 的最终结论（C 不推荐）成立，但否决理由的表述不精确**，应改为"重写成本"而非"功能缺失"。

---

### 反论 5（成立度：**部分成立，但非框架决策问题**）— 单人维护者 Rust+Tauri 锁定可能影响未来协作

**反论核心：** 项目锁定 Rust + Tauri 后，未来招聘协作者时：

- Rust 后端开发者的招聘池比 Node.js/Electron 小 5-10 倍
- 懂 russh 这种细分库的开发者更稀缺
- Vue+Tauri 的全栈组合在国内招聘市场不常见（国内 Tauri 项目多为 Rust 后端 + React/Vue 前端，但深度做 SSH 守户端的几乎没有）

**我的评估：** 反论部分成立，但**与本次框架决策无关**。理由：

1. 项目当前阶段是**单人维护**（git user: RedTei），招聘是未来 6-12 个月之外的问题
2. 即便切 Electron + ssh2，**SSH 协议层 + 隧道 SOCKS5 + PTY + 文件传输进度**的核心 domain 仍是稀缺技能，Node.js 招聘池里能做的人**比 Rust 还少**（因为 JS 生态做 SSH 客户端的存量项目少）
3. Qt 路线（B1 PySide6）确实招聘池更大，但 ADR 已经正确否决

**这个反论应当被记入 ADR 的 Long-term Consequences 节作为"已识别但 out-of-scope"风险**，不改变结论。

---

## 2. Tradeoff 张力（必须真实）

### 张力 1：原则 1（已验证逻辑优先）vs 原则 4（重写成本诚实量化）—— **真实且无法消除**

**张力的两边：**

- **左边（已验证逻辑优先）：** russh 21 命令、SOCKS5 自实现、host key 持久化、键盘交互认证 loop 都是**已经踩过坑、已经在生产里验证过的代码**。重写它们意味着回归 bug 的隐性成本（ADR 原则 1 + 决策驱动因素 1）。
- **右边（重写成本诚实量化）：** 但"已验证"不等于"已测试"。实读测试：

  - `tests/ui-smoke.mjs` 只测**前端浏览器预览模式**，对 russh 后端**零覆盖**
  - `tests/ui-extended.mjs` 同上
  - `src-tauri/src/lib.rs:232-280` 只守护"双 manage 不变量"（一个 Tauri 内部 hack），不验证 SSH 功能
  - russh 后端**没有单元测试**（无 `#[cfg(test)] mod tests` 在 ssh.rs）

**Planner v1 选了哪边：** 左边（"95% 代码可救"驱动决策）。

**这个选择在什么条件下会变成错误：** 如果未来 6 个月内出现一个 SSH 协议层 bug（比如 SOCKS5 在某种边缘网络环境下挂死），维护者会发现"已验证的代码"其实是"已编译通过但从未被运行测试覆盖的代码"——**回归成本会被严重低估**。这把"已验证"的论据从"风险低"悄悄变成"风险未知"。

**对 ADR 的修订建议：** 在 Follow-ups 节加一条 P2 任务"为 russh 后端补集成测试（mock SSH server，至少覆盖 connect/upload/download/tunnel 4 个命令）"，否则"已验证代码"的论据强度会被未来 reviewer 攻破。

---

### 张力 2：原则 3（单人维护者续航）vs 决策驱动因素 1（保留 95% 代码）—— **真实但本次没触发**

**张力的两边：**

- **左边（续航优先）：** 维护者已掌握 Rust + Vue + Tauri 三栈，认知负荷是项目最大瓶颈（ADR 原则 3）。
- **右边（保留代码）：** "95% 代码可救"是决策驱动因素 1，权重 40%。

**张力场景：** 假设未来维护者**对 Rust 的兴趣显著下降**（6-12 个月后真实可能），而 Vue + Tauri 前端部分仍想保留——此时 Electron 路线（保留 Vue，把后端换成 Node.js + ssh2）会成为合理的"分阶段重写"路径。但 Planner 的 ADR 把方案 C 完全否决了，没留这个未来窗口。

**Planner v1 选了哪边：** 左边（续航优先 → A）。

**这个选择在什么条件下会变成错误：** 维护者弃 Rust 但保留 Vue 的场景出现时。这个条件**今天不满足**（ADR 第 6 节回滚策略有提及），但 ADR 应该在"何时考虑方案 C"里**显式列入**"维护者技能栈发生迁移"作为触发条件，而不是只列"放弃 Rust 后端"这种模糊表述。

**对 ADR 的修订建议：** 第 4 节末尾"何时考虑方案 B 或 C"里，给方案 C 加一条触发条件：**"维护者技能栈从 Rust 主导迁移到 Node.js 主导"**。

---

### 张力 3：原则 2（痛点分类解决）vs 原则 5（局部修复优于框架重写）—— **本次真实触发**

**张力的两边：**

- **左边（痛点分类解决）：** Planner 原则 2 主张 State 陷阱 / IPC OOM / 双实现是**三个独立的应用层问题**，分开修。
- **右边（局部修复优于框架重写）：** 原则 5 是奥卡姆剃刀——三个痛点都能局部修就禁止重写。

**张力场景：** 但**三个痛点的修复路径在时间上互相耦合**：

- Option A 重构（修 State 陷阱）→ 必须先于其他改动，否则 ssh.rs 25 个 State 站点持续是技术债
- IPC OOM 改流式 → 必须改 transferQueue 前端状态机（见反论 1），而 transferQueue 又**强依赖** `invokeBrowserPreview` 的 `Array.from(buffer)` 调用模式（`workbench.js:316-317`）
- 删 invokeBrowserPreview → 必须先有 Tauri 端的等价 E2E 测试，否则前端没回归保护

**Planner v1 实际选了哪边：** 左边（分类解决），ADR 第 1 节列了 5 个独立步骤。

**这个选择在什么条件下会变成错误：** 如果按 ADR 的顺序串行执行（先 Option A → 再 IPC OOM → 再删 browser preview），**Step 1 完成后 Step 2 才能开始**，但 Step 2 改 transferQueue 时会发现 Step 3 还没做（browser preview 还在），改动可能误触发 browser preview 路径，造成调试困难。实际应该是 **Step 3 先做（清掉 browser preview 污染）→ Step 1（State 重构）→ Step 2（IPC OOM）**，或至少 1 和 3 并行。

**对 ADR 的修订建议：** ADR 第 1 节"路径"列表的**顺序应调整为 3 → 1 → 2**，并在第 6 节回滚策略里说明"Step 3 是 Step 2 的前置条件"。

---

## 3. 综合方案（修订建议）

**核心立场：同意 Planner 推荐方案 A，但要求 v2 ADR 修订以下 5 点后才 approve。**

### 修订项 R1（必改）：工作量估算 8-12 天 → **12-18 天**

| Planner v1 估算 | 修订后估算 | 修订理由 |
|---|---|---|
| Option A 重构：2 天 | 2 天 | 不变 |
| IPC OOM 改流式：2-3 天 | **4-5 天** | 含 transferQueue 前端状态机改造（见反论 1 证据 A） |
| 删 invokeBrowserPreview：0.5 天 | **1-2 天** | 含 Playwright 测试改写（见反论 1 证据 B） |
| Windows 构建脚本：1 天 | 1 天 | 不变 |
| 测试：2-3 天 | **3-4 天** | 含 Tauri 模式 E2E 重建 |
| **合计** | **12-18 天** | 仍 << B (60-90) 和 C (25-40) |

**对结论的影响：** 工作量增加 ~50%，但**不改变 A vs B vs C 的相对优劣**（比例从 1:7:3 变成 1:5:2，A 仍是最低）。

### 修订项 R2（必改）：方案 B 补 B3/B4 子选项

在 ADR 第 3.2 节后追加：

> **B3：Rust + Slint + russh**
> - 前端：Slint 声明式 UI（Vue 代码作废，但学习曲线比 QML 平缓）
> - 后端：russh 保留（最大优势——保留已验证 SSH 资产）
> - 终端：Slint 缺乏成熟终端组件，仍需自绘或包装
> - 工作量：40-60 人天（介于 A 和 B1 之间）
> - 否决理由：仍需重写全部前端，违反原则 1；Slint 终端生态不成熟，无法解决当前 xterm.js 优势
>
> **B4：Rust + egui + russh**
> - immediate-mode UI，对工具型 SSH 客户端有性能优势
> - 但 Vue/Pinia 响应式生态在 egui 里**完全没有等价物**，前端是"从响应式倒退到 immediate mode"的重写
> - 否决理由：同 B3

### 修订项 R3（必改）：风险表追加 R4

ADR 第 5 节风险表追加：

> **R4. 已验证代码资产的实际测试覆盖率低于隐含假设**
> - 概率：HIGH（事实，非风险）
> - 影响：MEDIUM（未来回归 bug 成本被低估）
> - 缓解：在 Follow-ups 增加 P2 任务"为 russh 后端补集成测试（mock SSH server，覆盖 connect/upload/download/tunnel 4 个命令）"

### 修订项 R4（必改）：回滚策略追加软触发

ADR 第 6 节"回滚触发条件"追加：

> **软触发（主观体验倒退）：**
> - 终端 PTY 在高频输出（如 `cat 大文件` / `yes` 命令）下出现可感知的卡顿或丢字符
> - SFTP 上传 100MB+ 文件时 UI 完全冻结超过 3 秒
> - 任一上述软触发 + Planner 主推的硬触发条件（Option A 失败 > 3 天 / 传输 < 5MB/s）任一满足 → 触发回滚

### 修订项 R5（必改）：方案 C 否决理由重写

ADR 第 3.3 节"为什么不推荐" 第 2 点改为：

> **2. 重写成本被低估，不是功能缺失：** 项目当前的 SOCKS5 是基于 russh channel **自实现的**（`ssh.rs:1183-1347`），ssh2 (Node.js) 同样需要自实现，两边功能对等。真正的成本在于：
>    - host key 交互确认（`ssh.rs:54-110` 的 oneshot + emit 模式）在 ssh2 的事件循环里要重写
>    - 键盘交互认证 loop（`ssh.rs:297-339`）在 ssh2 里 API 形状不同
>    - 这些重写**新增 5-10 人天工作量**，且伴随回归风险

### 修订项 R6（建议改，非阻塞）：执行顺序调整

ADR 第 1 节方案 A 路径顺序从 **1→2→3→4→5** 改为 **3→1→2→4→5**（先删双实现，再 State 重构，最后 IPC OOM）。

---

## 4. 对 Planner 4 个 Open Questions 的独立回应

### Q1：8-12 人天是否过于乐观？

**Architect 判断：是的，应改为 12-18 天。** 见反论 1 证据 A/B 和修订项 R1。前端 transferQueue 状态机改造和 Playwright 测试改写是 Planner 漏算的两块。

### Q2：95% 代码可救是否忽略测试代码？

**Architect 判断：是，且影响量化。** 实读证据：

- `tests/ui-smoke.mjs:38-47` 强依赖 invokeBrowserPreview 的 browser-preview 模式
- `tests/ui-extended.mjs:49` 同上
- `src-tauri/src/lib.rs:232-280` 守护双 manage hack，Option A 重构后必须删除/替换

**真实代码保留率：** 应从 95% 修订为 **88-92%**（前端的 Playwright 测试套件 + lib.rs 源码级回归测试要重写约 200 行）。

### Q3：ssh2 弱于 russh 是否准确？

**Architect 判断：表述不准确，但结论方向对。** 见反论 4 和修订项 R5。SOCKS5 两边对等（都要自实现），真实差异是 host key 交互模式和键盘认证 API 形状——这是**重写成本**而非**功能缺失**。Planner 的否决结论（C 不推荐）成立，但论据要重写。

### Q4：回滚触发条件阈值是否合理？

**Architect 判断：硬触发合理，但缺软触发。** 见修订项 R4。

- "Option A 重构失败 > 3 天" → 合理（足够给 debug 留窗口，不过短）
- "传输 < 5MB/s" → 偏严苛（局域网 SFTP 单线程通常 10-50MB/s，5MB/s 已是异常；建议改为 "< 10MB/s 或相对当前实现倒退 > 30%"）
- 缺主观体验软触发（终端卡顿 / UI 冻结）→ 必须补

---

## 5. Principle Violations（Deliberate Mode 强制项）

| Principle | 是否违反 | 严重度 | 说明 |
|---|---|---|---|
| 已验证逻辑优先 | 否 | — | Planner 正确应用 |
| 痛点分类解决 | **部分违反** | LOW | Planner 分类正确但**执行顺序耦合未识别**（见张力 3），导致"分类"在执行时变成"串行依赖" |
| 单人维护者续航优先 | 否 | — | Planner 正确应用 |
| 重写成本诚实量化 | **部分违反** | MEDIUM | 工作量 8-12 天漏算前端 transferQueue + 测试改造（反论 1），真实应为 12-18 天 |
| 局部修复优于框架重写 | 否 | — | Planner 正确应用 |

**无 HIGH 严重度违反。** 两处 MEDIUM/LOW 违反均可通过修订项 R1-R6 解决，不改变结论。

---

## 6. 最终裁决

### **ITERATE**

**裁决理由：**

1. **核心结论方向正确**（方案 A 仍是最优），不需要 REJECT
2. **工作量估算与若干风险表述存在 MEDIUM 级缺陷**（反论 1 + 张力 1 + 修订项 R1/R3/R4），需要在 v2 修订后才能 APPROVE
3. **方案 B 评估完整性有 LOW 级缺陷**（反论 2 + 修订项 R2），非阻塞但应补
4. **方案 C 否决理由表述不精确**（反论 4 + 修订项 R5），非阻塞但应重写以避免未来 review 时被攻击

### Planner 下一步行动

1. 按 R1-R5（必改）修订 v1 → 生成 v2 ADR
2. R6（建议改）可酌情采纳
3. v2 转入 Critic 二审
4. Critic 通过后转 `Status: approved (v2)`，本 ADR 关闭

### 不接受 REJECT 的理由

Planner 的 ADR 在**决策原则、备选方案数量、量化对比表、风险识别、回滚策略**五个维度都达到了 ralplan 共识流程的最低标准。问题集中在**精度**而非**方向**。REJECT 会导致项目在没有共识的情况下继续真空运行（双 manage hack 持续累积技术债），代价比 ITERATE 更高。

---

## 7. Consensus Addendum（ralplan 强制项）

- **Antithesis（steelman）：** 最强的反论是反论 1 + 反论 2 的组合——如果"方案 A 真实工作量是 12-18 天（修订后）" + "存在 Slint 这个保留 russh 但摆脱 WebView 的中间路径"，那么 Planner 加权表里 A 的"开发效率 5 分"和 B 的"开发效率 1 分"差距应该收窄。即便如此，B3（Slint）仍需 40-60 天 vs A 的 12-18 天，**A 仍是 3-4 倍便宜**，反论不足以翻转结论。
- **Tradeoff tension：** 最深的张力是张力 1——"已验证代码"的论据建立在"代码被运行测试覆盖过"的隐含假设上，而实际 russh 后端**零单元测试**。这意味着方案 A 的"低回归风险"优势部分建立在虚假前提上。这个张力不能消除，只能在 R3（补集成测试）后缓解。
- **Synthesis：** 保留 Planner 推荐方案 A 的核心结论，同时：(1) 量化真实工作量到 12-18 天，(2) 显式承认已验证代码的测试盲区并补集成测试任务，(3) 把 Slint/egui 作为未来"如果有一天 WebView 成为瓶颈"的明确逃生舱记录在 ADR——既不让它影响今天决策，也不让未来 reviewer 重新发明。
- **Principle violations（deliberate mode）：** 见第 5 节。两处部分违反（原则 2 LOW、原则 4 MEDIUM），均可通过 R1-R6 修订解决。

---

## References

- `src/stores/workbench.js:316-317` — upload 实际通过 `Array.from(buffer)` 把 Uint8Array 转 JS Number 数组过 IPC，是 IPC OOM 的真实形状，Planner ADR 漏算
- `src/stores/workbench.js:301-315` — transferQueue 状态机结构，IPC OOM 改流式必须重写此处
- `src/services/backend.js:36-147` — invokeBrowserPreview 双实现 100+ 行死代码，确认 Planner 方案 A 第 3 步可删
- `src-tauri/src/ssh.rs:1183-1347` — SOCKS5 自实现（非 russh 提供），证明反论 4：ssh2 切换不丢失此功能
- `src-tauri/src/ssh.rs:1135-1143` — `start_remote_forward` 是返回错误的桩函数，ADR 称"remote forward 已实现" 有水分
- `src-tauri/src/ssh.rs:54-110` — host key 交互确认的 oneshot + emit 模式，切 ssh2 时必须重写
- `src-tauri/src/ssh.rs:297-339` — 键盘交互认证 loop，切 ssh2 时 API 形状不同
- `src-tauri/src/lib.rs:157-166` — 双重 manage hack，证明 Tauri State<T> 陷阱非孤立事件
- `src-tauri/src/lib.rs:232-280` — 源码级回归测试守护 hack，证明"需写源码扫描测试"的高维护成本信号
- `tests/ui-smoke.mjs:38-47` — Playwright 测试强依赖 browser-preview 模式，删 invokeBrowserPreview 必须改写
- `tests/ui-extended.mjs:49` — 同上
- `.omc/plans/followup-ssh-state-unify.md:83` (AC-FU-6) — follow-up plan 自己承认 Option A 重构会让 lib.rs 回归测试失效
- `.omc/plans/open-questions.md:11-18` — Planner 自己列的 6 个待审查 open question
