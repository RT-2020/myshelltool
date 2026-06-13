# Ralplan Critic Review — Framework Choice ADR (v2)

> **Reviewer:** Critic
> **Target:** `framework-choice-tauri-vs-qt-vs-electron.md` v2
> **Date:** 2026-06-13
> **Mode:** THOROUGH → ADVERSARIAL（触发：发现 1 个 CRITICAL 残留矛盾 + 2 个 MAJOR 真诚度问题，符合升级阈值）
> **Verdict:** **ITERATE**（v2 主体落地真诚，但发现 1 项必须修复的残留矛盾和 2 项需要校正的真诚度声明；v3 修订后即可 APPROVE）

> **注：** 本文为 Critic agent 直接输出的评审正文（该 agent 通常只读，由 ralplan 协调器代为落盘）。原汁原味保留，未做改动。

---

## Pre-commitment Predictions

**评审前预测的 3-5 个最可能的弱点区域：**

1. **预测：** Planner 在自评里声称"全文 X 已全部替换为 Y"通常会漏 1-2 处
   - **结果：命中。** 第 2 节深度对比表第 91 行残留 "ssh2 (Node) 功能相对弱（特别是动态 SOCKS5）"——这是 CRITICAL 级残留矛盾，详见 Critical Finding 1。

2. **预测：** "代码可救率"和"工作量"两个数字会在多处口径不一
   - **结果：未命中。** 经 grep 全文核查，95% → 88-92% 替换干净；8-12 → 12-18 替换干净。第 64 行残留的 "95%" 是 SSH 会话管理**成熟度**评分（不同语义维度），合理保留。

3. **预测：** ADR 引用的代码行号会有偏差
   - **结果：部分命中。** `lib.rs` ADR 引用 161-166，Architect 引用 157-166，实际声明注释在 157-160、调用在 161-166——两者都对但不完全一致。`ui-smoke.mjs` ADR 引用第 38 行的断言，实际在第 34 行，±4 行偏差，断言本身存在。

4. **预测：** R5 的"重写成本"修订可能没有同步到全文所有相关位置
   - **结果：命中且严重。** 详见 Critical Finding 1。

5. **预测：** Planner 把 Architect 的张力 3（"transferQueue 强依赖 invokeBrowserPreview"）当作既定事实采纳，但这个技术断言可能本身就有问题
   - **结果：命中。** 详见 Major Finding 1。

---

## A. 5 项强制检查结果

### 检查 1：Principle-Option Consistency — **PASS**

**依据：**
- 5 条 Principles（已验证逻辑优先 / 痛点分类解决 / 单人维护者续航 / 重写成本诚实量化 / 局部修复优于框架重写）确实驱动了最终推荐
- 推荐方案 A 不违反任何 Principle
- 备选 B1/B2/B3/B4 的否决都明确引用了原则（B1/B2 违反原则 1；B3/B4 违反原则 1 和原则 3；详见 ADR 第 3.2 节"为什么不推荐"）
- 备选 C 的否决理由（IPC OOM 不消失、包大小膨胀、host key 重写成本）基于 Decision Driver 1 和原则 4，不是主观偏好

**唯一保留：** Critical Finding 1 暴露的"SSH 生态成熟度评分"内部矛盾会影响 Electron 评分的精度——但不改变 A 仍是最高分的结论。

### 检查 2：Fair Alternatives — **PARTIAL**

**B3/B4 评估的真诚度判定：基本真诚，但有一处水分。**

通过项：
- B3（Slint）和 B4（egui）都给出了具体工作量估算（40-60 人天）
- 都给出了具体否决理由（前端重写违反原则 1；终端生态不成熟违反原则 3）
- 都给出了"何时 reconsider"的明确触发条件（B3 在第 4 节"边界条件"显式记为"WebView 成为瓶颈时"逃生舱）
- B3/B4 还指出了相对 B1/B2 的关键差异（保留 russh 后端）

扣分项（轻度假应付）：
- B3 和 B4 的工作量估算**完全相同**（都是 40-60 人天），但 Slint（声明式）和 egui（immediate mode）的范式差异很大，工作量相同的理由没有展开
- B4 提到"immediate mode 对终端天然友好，自绘成本可能低于 Slint"，但工作量估算仍是 40-60，没体现这个差异

**C 方案否决理由在 R5 修订后是否仍然成立：**
- 第 3.3 节、RALPLAN-DR Summary、第 7 节 Alternatives 的 R5 修订成立且真诚
- **但第 2 节深度对比表残留矛盾**（详见 Critical Finding 1）—— R5 修订未同步到深度对比表的 SSH 生态成熟度行评分依据

**漏评估的框架选项：**
- Flutter Desktop：考虑到 Dart 新栈 + Flutter 终端组件生态空白，不评估合理
- Wails（Go + WebView）：考虑到 Go + russh 无对应物（russh 是 Rust 资产），不评估合理
- Avalonia（.NET）：考虑到 Windows-only 视野过窄 + C# 新栈，不评估合理
- **未发现关键漏项。**

### 检查 3：Risk Mitigation Clarity — **PARTIAL**

R1-R4 四个风险条目整体可执行，但 R4 的"补集成测试"范围不够具体。

**通过项：**
- R1 缓解措施可执行（锁 Cargo.lock + 保留源码级回归测试 + 关注 changelog + 1-3 人天迁移 buffer）
- R2 缓解措施可执行（维护 scripts/tauri-env.ps1 + CI smoke test）
- R3 缓解措施可观测（测试矩阵覆盖 Win10/11，触发可考虑嵌入 WebView2）
- 软触发条件**具体且可观测**——`cat 大文件` / `yes` 命令卡顿、SFTP 100MB+ 冻结 > 3 秒、单软触发持续 > 2 周启动逃生舱评估——全部是可主观但可重现的场景

**扣分项：**
- R4 缓解措施"补集成测试（mock SSH server，覆盖 connect/upload/download/tunnel 4 个命令）"虽然范围比 v1 具体了，但**没指定 mock SSH server 用什么库**（russh 本身有 mockable 设计吗？还是用 docker 起一个 openssh-server？还是用 `thrussh_server` 之类测试 crate？）。这会让 executor 在落地时还要再调研。
- **修订建议：** 在 Follow-ups R4 项追加"调研可选 mock 方案：[rust-ssh-test-server / docker openssh / rush mock handler]，执行阶段二选一"。

### 检查 4：Testable Acceptance Criteria — **FAIL**

**ADR 没有定义"方案 A 成功"的可测试验收标准。**

这是 v2 最大的结构性遗漏。ADR 列了 5 步路径（顺序已调整为 3→1→2→4→5），但**每步的"完成"判定标准未定义**。例如：

- Step 2（Option A 重构）怎样算"完成"？是说 "ssh.rs 中 grep `State<'_, Arc<Mutex<SshSessionManager>>>` 返回 0"？（followup-ssh-state-unify.md 的 AC-FU-1 已经定义了这个标准，但 ADR 没引用）
- Step 3（IPC OOM 修复）怎样算"完成"？是说"上传 500MB 文件无 OOM"？还是"上传时间 < X 秒"？
- Step 1（删 invokeBrowserPreview）怎样算"完成"？是说"backend.js 中 grep `invokeBrowserPreview` 返回 0"？

**建议补一段"验收标准"小节：**
```
方案 A 成功的可测试标准（所有项必须满足）：
- AC-1: ssh.rs 中 grep `State<'_, Arc<Mutex<SshSessionManager>>>` 返回 0（迁移到 AppState）
- AC-2: lib.rs 中 `app.manage(ssh_mgr.clone())` 行已删除（仅保留 manage(AppState)）
- AC-3: backend.js 中 grep `invokeBrowserPreview` 返回 0
- AC-4: 上传 500MB 文件，前端堆内存峰值 < 200MB（证明改流式成功）
- AC-5: tests/ui-smoke.mjs 和 ui-extended.mjs 改写后能在 Tauri runtime 模式下通过
```

**严重度：** MAJOR——不阻塞批准，但 executor 落地时需要自己定义"完成"标准，会让 follow-up 工作量评估走样。

### 检查 5：Concrete Verification Steps — **PARTIAL**

**"Tauri 版本升级每次预估 1-3 人天迁移成本"的依据：**
- 这是对已踩到的 State<T> TypeId 陷阱（需要源码级回归测试守护）的外推
- 依据可信但不充分——只基于 1 次踩坑的外推到"每次升级"统计学上不严谨
- **建议改为：** "Tauri 版本升级每次预留 0-3 人天迁移成本（基于 State<T> 陷阱经验，最坏情况 3 人天）"，把下限改为 0（多数小版本升级可能 0 成本）

**工作量估算 12-18 天引用具体代码改动范围：**
- IPC OOM 4-5 天：引用 `ssh.rs:788` 的 `content: Vec<u8>` + `workbench.js:316-322` 的 `Array.from(buffer)` + transferQueue 状态机改造——**范围引用具体**
- 删 invokeBrowserPreview 1-2 天：引用 `backend.js:36-147`（100+ 行）+ Playwright 改写——**范围引用具体**
- Option A 重构 2 天：引用 `lib.rs:161-166` + 21 命令 + AC-FU-1/2——**范围引用具体**
- 测试 3-4 天：引用 `ui-smoke.mjs:34` + `ui-extended.mjs:49` + `lib.rs:232-280`——**范围引用具体**

**总体：** 工作量估算引用了具体代码改动，依据充分。**通过。**

**Follow-ups 验证步骤的具体性：**
- 列了 6 项 follow-up（Option A / IPC OOM / 删 invokeBrowserPreview / Windows CI / mock SSH 测试 / 方案 C 触发条件）
- 但**未指定每项 follow-up 的验证方法**（如 IPC OOM 改完后怎么验证？跑什么测试？传多大文件？）—— 这与检查 4 的验收标准遗漏同源

---

## B. Planner v2 修订真诚度评估

### R1（工作量 8-12 → 12-18）：**真诚落地**

经 grep 全文核查（pattern: `8-12|95%|88-92%|12-18`）：
- 所有原本写 "8-12" 的位置都已替换为 "12-18" 或显式标注"v1 原 8-12 天"
- 第 64 行的 "95%" 是 SSH 会话管理**成熟度评分**（不同语义维度），不应替换，正确保留
- IPC OOM 细项分解（4-5 天）、测试改写（3-4 天）、Playwright 改写（1-2 天）全部展开说明
- **结论：自评准确。**

### R2（B3/B4 子选项）：**基本真诚落地，轻度水分**

通过项：
- B3/B4 完整评估追加在第 3.2 节
- Viable Options 表更新引用
- 边界条件追加 B3 作为逃生舱
- 第 7 节 Alternatives_considered 列出 B3/B4

轻度水分（不足以构成 MAJOR）：
- B3 和 B4 工作量估算**完全相同**（40-60 人天），未体现两种范式差异
- B4 自评"immediate mode 对终端自绘成本可能更低"，但工作量没体现这个差异
- **结论：自评大体准确，但工作量同质化是轻度应付。**

### R3（风险表追加 R4）：**真诚落地**

- R4 完整条目已追加（HIGH 概率事实 + MEDIUM 影响 + 具体现状描述 + 缓解任务）
- Follow-ups 追加 P2 任务"为 russh 后端补集成测试"
- Consequences 负面追加"已验证代码资产的实际测试覆盖率低于隐含假设"
- **唯一保留：** 缓解任务的 mock SSH server 方案未指定（见检查 3 扣分项）
- **结论：自评准确。**

### R4（回滚策略补软触发）：**真诚落地**

- 软触发段已追加（终端卡顿 / SFTP 冻结 > 3 秒 / 单软触发持续 > 2 周启动逃生舱评估）
- 触发场景具体可观测
- **结论：自评准确。**

### R5（方案 C 否决理由重写）：**部分应付——这是 MAJOR 问题**

详见 **Major Finding 2**。简要概括：Planner 在 v2 自评（第 9 节）和文档其他位置描述 v1 原文时，用了**三种不同的措辞**来描述 v1 的"错误"：
- 第 78 行："原 v1 误记为 **'SOCKS5 功能缺失'**"
- 第 214 行："原 v1 误记为 **'SOCKS5 功能查漏补缺'**"
- 第 303 行："原 v1 表述 **'ssh2 功能弱于 russh'**"

而 Architect R5 原文（`ralplan-architect-review-framework-choice.md:222`）明确指出 v1 原文是 **"ssh2 功能弱于 russh"**。

Planner 在自评里创造两个 v1 没有的措辞（"SOCKS5 功能缺失"、"SOCKS5 功能查漏补缺"），然后"修正"它们——这是**轻度自评美化**。再加上 Critical Finding 1 暴露的深度对比表残留矛盾，R5 的落地真诚度从"真诚"降级为"部分应付"。

### R6（执行顺序调整）：**真诚落地但前提有误**

详见 **Major Finding 1**。简要概括：
- 顺序从 1→2→3→4→5 调整为 3→1→2→4→5 已落地
- 但"顺序调整理由"引用的 Architect 张力 3 的核心技术断言（transferQueue 强依赖 invokeBrowserPreview）**本身是错的**——transferQueue 走 `invokeBackend`，在 Tauri runtime 下不走 invokeBrowserPreview
- 顺序调整本身的结论仍然合理（先清理污染再做核心重构），但 Planner 没核实就采纳了 Architect 的错误前提

### v2 自评整体结论

Planner 自评"6 项修订全部真诚落地，无表面应付"——**不准确**。实际是 4 项真诚落地（R1/R3/R4 + R6 的执行部分）、1 项基本真诚（R2 轻度水分）、1 项部分应付（R5 见 Major Finding 2 + Critical Finding 1）。

---

## C. 事实核查（5 项）

### 核查 1：SOCKS5 自实现于 ssh.rs:1183-1347 — **真实**

实读 `ssh.rs:1183-1347`：函数 `handle_socks5_connection` 完整实现——手写 SOCKS5 握手（version/method negotiation）、CONNECT 命令解析（IPv4/Domain/IPv6 三种地址类型）、`channel_open_direct_tcpip` 调用、双向 select loop 数据转发。**完全确认是自实现，非 russh 提供。** ADR 论据成立。

### 核查 2：lib.rs:232-280 源码级回归测试守护双 manage — **真实**

实读 `lib.rs:232-280`：测试函数 `ssh_state_keys_point_to_same_arc` 读取 lib.rs 自身源码，截取 `pub fn run()` 函数体，断言 `app.manage(ssh_mgr.clone())` 调用确实存在且在 `app.manage(AppState {` 之前。**完全确认是源码扫描测试，守护双 manage 不变量。** ADR 论据成立。

### 核查 3：transferQueue 强依赖 Array.from(buffer) 于 workbench.js:316-317 — **真实但表述需精化**

实读 `workbench.js:295-330`：`uploadFiles` 函数确实在 316-322 行调用 `invokeBackend('sftp_upload_with_progress', { content: Array.from(buffer), ... })`——`Array.from(buffer)` 把 Uint8Array 转 JS Number 数组，是 IPC OOM 的真实源头。**ADR 论据成立。** 精度校正：ADR 引用 "316-317"，实际 Array.from 出现在第 320 行（更精确范围应是 316-322）。

### 核查 4：双 manage hack 于 lib.rs:157-166 — **真实**

实读 `lib.rs:157-166`：第 157-160 行是 4 行注释解释，第 161 行 `app.manage(ssh_mgr.clone())`，第 162-166 行 `app.manage(AppState { ... })`。**双 manage 模式完全确认。** ADR 论据成立。

### 核查 5：start_remote_forward 是桩函数 — **真实，且 ADR 未充分披露**

实读 `ssh.rs:1135-1143`：`start_remote_forward` 函数体直接 `Err("Remote forwarding is not yet supported...")`——**桩函数，未实现。**

但 ADR 第 1 节"现状基线"称"隧道转发（local/remote/dynamic SOCKS5）...三种 kind 都实现"，给 85% 成熟度。**这是水分**——remote forward 是桩函数，没实现。Architect References 已经提及（第 320 行），但 v2 ADR 没修订这个表述。

**这构成 Open Question：** ADR 第 1 节"隧道转发 85% 成熟度，三种 kind 都实现"应改为"local/dynamic 已实现，remote 是桩函数返回 Err"。

---

## Critical Findings（阻塞批准）

### Critical Finding 1：深度对比表残留 v1 错误论据，与 R5 修订直接矛盾

**Confidence: HIGH**

**Evidence:**
- ADR 第 91 行（深度对比表 SSH 生态成熟度行）评分理由原文：
  > `russh 第一梯队；paramiko 成熟但同步阻塞；ssh2 (Node) 功能相对弱（特别是动态 SOCKS5）`
- ADR 第 225 行（R5 修订后的方案 C "为什么不推荐"）明确写道：
  > `切到 ssh2 (Node.js) 后，SOCKS5 仍然要自己实现（因为 ssh2 也只提供 channel API，SOCKS5 协议层是项目自写的）——两边在隧道功能上功能对等，原 v1 表述"ssh2 动态 SOCKS5 转发支持不完整"是错的`

**矛盾：** 同一份 ADR 在第 2 节说"ssh2 SOCKS5 弱"，在第 3.3 节说"两边 SOCKS5 功能对等"。

**Why this matters:**
- 深度对比表是 v2 加权总分（Tauri 48.5 / Qt 36 / Electron 39）的核心评分依据
- Tauri 在 SSH 生态成熟度拿 5 分、Electron 拿 3 分，**这 2 分差距部分建立在 v2 自己已经承认错误的论据上**
- 如果按 R5 修订逻辑，Electron 的 SSH 生态成熟度应至少 4 分（SOCKS5 对等，只是 host key/键盘认证 API 形状不同）
- 调整后 Electron 总分约 39 → 41，仍低于 Tauri 48.5，**结论方向不变**
- 但**这是未来 reviewer 攻击的明确着力点**——"你的加权表评分依据和正文论据自相矛盾"是 ADR 的硬伤
- 真诚度检查：Planner 在第 9 节 R5 自评写"全文'ssh2 功能弱'表述已全部替换为'重写成本被低估'"，但第 91 行深度对比表残留——**自评不准确**

**Fix（R7 必改）：**
- 把第 91 行 SSH 生态成熟度行的评分理由改为：
  > `russh 第一梯队；paramiko 成熟但同步阻塞；ssh2 (Node) 在 SOCKS5/host key 交互上需自实现（与 russh 对等），但 host key 异步交互模式 + 键盘交互认证 API 形状差异带来 5-10 人天重写成本（见第 3.3 节）`
- 同时考虑把 Electron 该行评分从 3 调到 4（与修订后论据一致），加权总分相应调整（39 → 约 41），结论方向不变，但 ADR 内部一致性恢复

---

## Major Findings（导致返工）

### Major Finding 1：R6 顺序调整理由引用了 Architect 错误的技术断言

**Confidence: HIGH**

**Evidence:**
- ADR 第 113 行（R6 顺序调整理由）原文：
  > `Step 2（IPC OOM 改 transferQueue 状态机）会触碰 workbench.js:316-317 的 Array.from(buffer) 调用模式，而该模式强依赖 invokeBrowserPreview 的存在。如果 Step 3 还没做（browser preview 还在），改 transferQueue 时可能误触发 browser preview 路径，造成调试困难。`

- 实读 `workbench.js:317`：`await invokeBackend('sftp_upload_with_progress', {...})`——调用的是统一的 `invokeBackend` 入口
- 实读 `services/backend.js`：`invokeBackend` 在 Tauri runtime 下走 `window.__TAURI__?.core?.invoke`，**不走 invokeBrowserPreview**
- `invokeBrowserPreview` 只在非 Tauri runtime（即浏览器预览模式）下被调用

**矛盾：** `Array.from(buffer)` 是 IPC 编码层的事，与 `invokeBrowserPreview` 是否保留**完全无关**。Planner 在 R6 顺序调整理由里采纳了 Architect 张力 3 的错误断言。

**Why this matters:**
- R6 顺序调整本身的结论（先清理 browser preview 再做其他重构）**仍然合理**——理由是"清理污染优先于核心重构"的工程美学，不需要"强依赖"这个错误前提
- 但保留错误前提会让未来 reviewer 困惑：为什么"清理 browser preview"是"IPC OOM 改流式"的前置条件？实际上两者并无依赖
- 真诚度检查：Planner 没有独立核实 Architect 的技术断言就采纳了

**Fix（R8 建议改）：**
- 把第 113 行"顺序调整理由"重写为：
  > `顺序调整理由：先做污染清理（Step 1 删 invokeBrowserPreview）再做核心重构（Step 2 Option A）和 IPC 改造（Step 3 IPC OOM）的原因是工程美学——清理死代码先于核心改动可以减少认知干扰，避免在重构 transferQueue 时同时面对 browser-preview 残留路径（即便不依赖，仍是认知污染）。这不是技术依赖（transferQueue 走 invokeBackend 不走 invokeBrowserPreview），是工程顺序优化。`

### Major Finding 2：R5 自评三种 v1 错误描述措辞不一致，轻度美化

**Confidence: MEDIUM**

**Evidence:**
- ADR 第 78 行：`原 v1 误记为"SOCKS5 功能缺失"`
- ADR 第 214 行：`原 v1 误记为"SOCKS5 功能查漏补缺"`
- ADR 第 303 行：`原 v1 表述"ssh2 功能弱于 russh"不准确`
- Architect R5 原文（`ralplan-architect-review-framework-choice.md:222`）：`原 v1 表述"ssh2 功能弱于 russh"`

**矛盾：** Planner 在 v2 中创造了两处 v1 实际没有的措辞（"SOCKS5 功能缺失"、"SOCKS5 功能查漏补缺"），让自己显得更诚实地"修正"了 v1。

**Why this matters:**
- v1 实际的措辞是 "ssh2 功能弱于 russh"（第 91 行深度对比表残留可证）
- Planner 在 v2 文档多处"重写"v1 的原话，这是 ADR 自评美化的信号——影响 ADR 的可追溯性
- 严重度不至于 CRITICAL（结论方向仍正确），但**这是真诚度问题**

**Fix（R9 建议改）：**
- 把第 78 行和第 214 行的"SOCKS5 功能缺失"和"SOCKS5 功能查漏补缺"统一改为 "ssh2 功能弱于 russh（v1 原文，见深度对比表第 91 行残留）"
- 这样既准确引用 v1 原文，又顺便提醒自己第 91 行需要修订（联动 R7）

---

## Minor Findings（不阻塞）

1. **行号引用精度问题：**
   - `lib.rs:161-166`（ADR）vs `lib.rs:157-166`（Architect）——两者范围不一致，建议统一为 157-166（含注释）
   - `workbench.js:316-317` 应更精确为 316-322（Array.from 在第 320 行）
   - `ui-smoke.mjs:38` 实际断言在 34 行（38 行是 assetSource 断言）——ADR 引用偏差 ±4 行

2. **第 1 节现状基线的 remote forward 桩函数未披露：**
   - `ssh.rs:1135-1143` 的 `start_remote_forward` 是返回 Err 的桩函数
   - ADR 第 1 节"隧道转发 85% 成熟度，三种 kind 都实现"应改为"local/dynamic 已实现，remote 是桩函数"

3. **长期维护风险评分 2.5 的来源不够硬：**
   - 依据只有 1 次踩坑（State<T> TypeId 陷阱）外推到"每次升级"，统计学不严谨
   - 建议改为"0-3 人天迁移 buffer（多数小版本 0 成本，最坏 3 人天）"

4. **B3/B4 工作量同质化：**
   - 两者都给 40-60 天，未体现 Slint（声明式）vs egui（immediate mode）的范式差异

5. **Follow-ups 缺乏每项的验证方法：**
   - 与检查 4 的验收标准遗漏同源

---

## What's Missing（缺失项）

1. **方案 A 的可测试验收标准（AC-1 到 AC-N）：** 见检查 4。这是 v2 最大的结构性遗漏。
2. **mock SSH server 的具体方案选型：** R4 缓解任务未指定（rust-ssh-test-server / docker openssh / russh mock handler）。
3. **IPC OOM 修复后的"无 OOM"判定标准：** 例如"上传 500MB 文件前端堆内存峰值 < 200MB"。
4. **remote forward 桩函数的事实披露：** 现状基线应反映这个事实。
5. **B3/B4 工作量差异化：** 见 Minor Finding 4。
6. **Tauri 版本升级迁移 buffer 的下限校正：** 见 Minor Finding 3。

---

## Ambiguity Risks（歧义风险）

1. **第 1 节"~70% 成熟度"整体评分：** 是 7 个维度算术平均，还是加权？加权的话权重是什么？→ 影响整体基线评估的可解释性。

2. **第 6 节"任一软触发 + 上方任一硬触发条件满足 → 触发回滚"：** 这里"任一软触发 + 任一硬触发"是 AND 关系（两个都发生）还是 OR 关系？字面读是 AND（同时满足），但下一段又写"任一软触发单独持续超过 2 周无法通过局部优化解决 → 触发回滚评估"——这是 OR。两种解释下回滚行为不同，需要明确。

3. **第 5 节 R4 风险"概率 HIGH（事实，非风险）"：** 风险表里把"事实"标为"概率 HIGH"在语义上不准确——事实不是概率事件。建议单独列"已识别的非风险事实"小节，避免和真实风险（R1/R2/R3）混淆。

---

## Multi-Perspective Notes

### Executor 视角（执行者）
- "Step 2 IPC OOM 改流式的 4-5 天估算够吗？" transferQueue 改分块上传需要重新设计状态机（当前是 push-and-await，要改成分块迭代），4-5 天偏紧。建议加 buffer 到 5-6 天。
- "Step 1 删 invokeBrowserPreview 后 Playwright 测试怎么改？" ADR 没说改写成什么样——是改断言还是改测试运行模式（从 playwright-vite 改成 tauri-driver）？两种改法工作量差很多。

### Stakeholder 视角（项目维护者）
- "ADR 解决了我提的问题吗？" 是的——明确推荐继续 Tauri，给出 12-18 天路径，列出逃生舱条件。
- "工作量上调到 12-18 天是否影响决策？" 不影响——仍远低于 B/C。
- "验收标准缺失会影响我推进 follow-up 吗？" 会——需要自己定义"完成"。

### Skeptic 视角（怀疑论者）
- "Planner 是不是过度乐观地采纳了 Architect 的所有意见？" 不完全是——R5 修订就有应付成分（Major Finding 2），R6 顺序调整理由也基于错误前提（Major Finding 1）。Planner 在两处表现出"形式上响应但实质上轻度假应付"。
- "方案 A 12-18 天估算是否仍然乐观？" IPC OOM 4-5 天偏紧（见 Executor 视角），建议加到 13-19 天。但这不改变结论。

---

## Verdict Justification

**裁决：ITERATE**

**裁决理由：**
1. **核心结论方向正确**（推荐方案 A 继续成立），不需要 REJECT
2. **发现 1 个 CRITICAL 残留矛盾**（Critical Finding 1：深度对比表第 91 行与 R5 修订直接矛盾），必须修复
3. **发现 2 个 MAJOR 真诚度问题**（Major Finding 1: R6 前提错误；Major Finding 2: R5 自评美化），应修复
4. **检查 4（可测试验收标准）FAIL**——这是结构性遗漏，应补充
5. 升级到 ADVERSARIAL 模式（触发条件：1 个 CRITICAL + 2 个 MAJOR），在 ADVERSARIAL 模式下进一步核查了 start_remote_forward 桩函数、sftp_download 实际行为（确认 IPC OOM 论据成立）、B3/B4 工作量同质化等

**Realist Check 校准：**
- Critical Finding 1（深度对比表残留矛盾）——Realist Check 后**保持 CRITICAL**：因为这是 ADR 内部硬性矛盾（同一份文档说 X 和非 X），任何未来 reviewer 都会立即发现并质疑 ADR 的可信度。无缓解因素可下调。
- Major Finding 1（R6 错误前提）——Realist Check 后**保持 MAJOR**：因为顺序调整的结论本身正确（理由是工程美学），但保留错误前提会误导未来 executor 和 reviewer。Mitigated by: 顺序结论本身正确，错误前提不影响执行结果。
- Major Finding 2（R5 自评美化）——Realist Check 后**保持 MAJOR**：因为这是 ADR 自评准确性问题，影响 ralplan 流程对 Planner 修订真诚度的判定。Mitigated by: 不影响最终结论方向。

**升级到 APPROVE 需要的修订项：**

- **R7（必改，对应 Critical Finding 1）：** 修订第 91 行深度对比表 SSH 生态成熟度行的评分理由，使其与 R5 修订一致；同时调整 Electron 该行评分（3 → 4）和加权总分（39 → ~41），结论方向不变。
- **R8（建议改，对应 Major Finding 1）：** 重写第 113 行 R6 顺序调整理由，承认 transferQueue 与 invokeBrowserPreview 无技术依赖，顺序调整是工程美学考虑。
- **R9（建议改，对应 Major Finding 2）：** 统一 v1 原文引用措辞，全部用 "ssh2 功能弱于 russh"（与 Architect 原文一致）。
- **R10（建议改，对应检查 4）：** 补充"方案 A 验收标准"小节（AC-1 到 AC-5 见上）。
- **R11（建议改，对应 What's Missing 2/3）：** R4 mock SSH server 方案选型 + IPC OOM 无 OOM 判定标准。

**不裁决 REJECT 的理由：**
- 核心结论、决策原则、备选方案数量、量化对比、风险识别、回滚策略——五个维度都达标
- 所有修订项都是可局部修复的精度问题，不需要推倒重写
- REJECT 会让项目在没有共识的情况下继续真空运行（双 manage hack + IPC OOM + browser preview 死代码全部累积），代价比 ITERATE 更高

**v3 修订后预期：** Planner 修复 R7-R11（其中 R7 必改、R8-R11 建议改）后，可由 Critic 直接 APPROVE 转为 `Status: approved (v3)`。本 ADR 关闭。

---

## Open Questions（unscored）

1. **Architect 张力 3 的技术断言错误是否影响 Architect 评审本身的可信度？** Architect 评审作为整体仍然可信（其他 5 项修订要求都准确），但张力 3 这一节的技术断言需要 Architect 在 v3 阶段承认并修订。建议 ralplan 协调器把此问题反馈给 Architect agent。

2. **Electron 评分从 3 调到 4 后，加权总分 39 → 41 是否会影响 Qt vs Electron 相对优劣？** 不会——Electron 仍显著低于 Tauri 48.5，且仍高于 Qt 36（Electron 仍胜 Qt，因前端复用）。

3. **Planner 在 R5 自评里"创造 v1 错误措辞"是无意还是有意？** 无从判定。从最善意角度解读，可能是 Planner 在撰写 v2 时为了内部表述一致而重新措辞；从最严苛角度解读，可能是为了让 R5 修订显得"修了更多错误"。建议按"无意"处理，但要求 R9 统一措辞以消除歧义。

4. **`start_remote_forward` 桩函数是否应当作为 follow-up 任务列入？** ADR 第 1 节"现状基线"的水分披露属于事实校正，但实际是否要实现 remote forward 是产品决策，不属于本 ADR 范围。

---

## Ralplan Summary Row

- **Principle/Option Consistency:** **PASS** — 5 条 Principles 真实驱动推荐，备选否决基于原则非偏好。但 Critical Finding 1 的内部矛盾影响 Electron 评分精度（不改变结论方向）。
- **Alternatives Depth:** **PARTIAL** — B3/B4 评估基本真诚但工作量同质化；R5 修订有轻度应付；漏评估的关键框架选项未发现（Flutter/Wails/Avalonia 不评估合理）。
- **Risk/Verification Rigor:** **PARTIAL** — R1-R4 风险缓解具体可执行；软触发条件可观测；但 R4 mock 方案未指定；验收标准缺失（检查 4 FAIL）；工作量引用具体但 IPC OOM 4-5 天偏紧。
- **Deliberate Additions（ralplan 强制项）：** N/A（本 ADR 非 deliberate 模式，无强制 pre-mortem 或 expanded test plan 要求）。

---

**评审结束。裁决 ITERATE，等待 Planner 修订 v2 → v3（至少修复 R7）后再审。**

---

**相关文件路径（绝对路径，供父 agent 落盘参考）：**

- 被审查 ADR：`D:\Project\Person Github Project\myshelltool\.omc\plans\framework-choice-tauri-vs-qt-vs-electron.md`
- Architect 评审：`D:\Project\Person Github Project\myshelltool\.omc\plans\ralplan-architect-review-framework-choice.md`
- 建议落盘位置：`D:\Project\Person Github Project\myshelltool\.omc\plans\ralplan-critic-review-framework-choice.md`
- 事实核查依据：
  - `D:\Project\Person Github Project\myshelltool\src-tauri\src\ssh.rs`（1183-1347 SOCKS5 / 54-110 host key / 297-339 键盘认证 / 784-824 upload / 826-871 download / 1135-1143 remote forward 桩）
  - `D:\Project\Person Github Project\myshelltool\src-tauri\src\lib.rs`（157-166 双 manage / 232-280 源码级回归测试）
  - `D:\Project\Person Github Project\myshelltool\src\stores\workbench.js`（295-330 uploadFiles / 316-322 Array.from）
  - `D:\Project\Person Github Project\myshelltool\src\services\backend.js`（36-147 invokeBrowserPreview）
  - `D:\Project\Person Github Project\myshelltool\tests\ui-smoke.mjs`（34 已连接断言 / 38 assetSource 断言）
  - `D:\Project\Person Github Project\myshelltool\tests\ui-extended.mjs`（49-50 remote entries 断言）
  - `D:\Project\Person Github Project\myshelltool\.omc\plans\followup-ssh-state-unify.md`（83 AC-FU-6）
