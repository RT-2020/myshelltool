# Ralplan Critic Review — Framework Choice ADR (v3 三审)

> **Reviewer:** Critic
> **Target:** `framework-choice-tauri-vs-qt-vs-electron.md` v3
> **Date:** 2026-06-13
> **Mode:** THOROUGH（未升级到 ADVERSARIAL——未发现新的 CRITICAL，已披露残留水分外的应付成分仅 1 处轻度不彻底，未达升级阈值）
> **Verdict:** **APPROVE**（R7 CRITICAL 真诚落地，R8-R11 + 3 项附加修订全部落地；存在 1 处 Planner 未披露的轻度残留措辞不一致 [第 250 行]，但程度不足以阻塞共识）

> **注：** 本文为 Critic agent 直接输出的评审正文（该 agent 通常只读，由 ralplan 协调器代为落盘）。原汁原味保留，未做改动。

---

## Pre-commitment Predictions vs Actuals

| # | 预测 | 结果 |
|---|---|---|
| 1 | R7 残留：评分理由已重写但风险表 R1 / Consequences 等位置可能未清理 | **未命中** — 评分理由全文一致，无残留 |
| 2 | R8 残留：顺序调整理由重写但其他位置仍残留"强依赖 invokeBrowserPreview" | **未命中** — grep "强依赖 invokeBrowserPreview" 只在第 134 行以否定句式（"原 v2 采纳的错误前提"）出现 |
| 3 | R9 残留：v1 措辞在 78/214 行统一了，但 v2 实际还有其他变体未被 Critic 二审发现 | **命中** — 第 250 行残留"ssh2 动态 SOCKS5 转发支持不完整"措辞变体（v2 第 4 种措辞），未统一为 "ssh2 功能弱于 russh" |
| 4 | 加权总分 39→41 算式可能不透明 | **部分命中** — 表头注释"前 3 维度 ×2 权重"无法精确算出 41（这是 v2 已存在的 Ambiguity 1，Planner 已在第 9.5 节自评披露为"已知残留水分"） |
| 5 | AC-4 数值口径可能不清 | **命中（轻度）** — 500MB < 200MB 堆的采集方式（DevTools/CI）未说明，但不影响 AC 的判定方向 |

---

## A. R7-R11 + 附加修订逐条 Verdict

### R7（CRITICAL 必改）— **PASS**

**修订位置：**
- 第 112 行（深度对比表 SSH 生态成熟度行）评分理由整段重写
- 第 122 行加权总分行 Tauri 48.5 / Qt 36 / Electron 41
- 第 71 行 Viable Options 表方案 C Cons 同步更新为"SSH 功能与 russh 完全对等"
- 第 124 行结论段同步说明
- 第 0.5 节 Revision History 表追加 R7 修订项

**核查证据：**

第 112 行评分理由原文：
> `russh 第一梯队；paramiko 成熟但同步阻塞；ssh2 (Node) 在 SOCKS5/host key 交互上需自实现（与 russh 对等），但 host key 异步交互模式 + 键盘交互认证 API 形状差异带来 5-10 人天重写成本（见第 3.3 节）——R7 修订，原 v2 残留"ssh2 (Node) 功能相对弱（特别是动态 SOCKS5）"与 R5 修订矛盾，已纠正`

Electron 该行评分明确标为 **4（v3 修订，原 v1/v2 评 3 分）**。加权总分行 Electron **41（v3 修订，原 v2 评 39 分）**，方向调整（3→4 评分对应加权 +2）逻辑自洽。

**残留问题：** 无关键残留。加权公式本身（"前 3 维度 ×2 权重"）从 v2 起即存在 Ambiguity 1（Planner 在第 9.5 节末尾"v3 已知残留水分"已诚实披露，归类为不阻塞批准的 minor issue），不属于 R7 范畴。

**真诚度判定：** 真诚落地。CRITICAL 矛盾已彻底消除，与 R5 修订完全一致。

---

### R8（MAJOR 建议改）— **PASS**

**修订位置：** 第 3.1 节方案 A 路径"顺序调整理由"段（第 134-138 行）

**核查证据：**

第 134 行原文：
> `顺序调整理由（R6 → R8 v3 修订）：Architect 张力 3 原文用"transferQueue 强依赖 invokeBrowserPreview"作为顺序调整的技术前提，但此前提错误——transferQueue 走 services/backend.js 的统一 invokeBackend 入口，在 Tauri runtime 下走 window.__TAURI__?.core?.invoke，根本不走 invokeBrowserPreview（后者只在浏览器预览模式下被调用）。v2 采纳了 Architect 的错误前提，v3 已纠正。`

第 136 行明确承认：
> `真正的顺序调整理由是工程美学，不是技术依赖...这不是技术依赖（transferQueue 走 invokeBackend 不走 invokeBrowserPreview），是工程顺序优化。`

第 138 行还有真诚度承认：
> `承认：v2 在 R6 自评里声称"理由引用了 Architect 张力 3 的具体分析"，但没独立核实 Architect 的技术断言就采纳，是真诚度问题。v3 已诚实修复。`

**残留问题：** 无。grep "强依赖 invokeBrowserPreview" 全文仅在第 134 行和第 424 行出现，均为否定句式（"原 v2 采纳的错误前提"），无正向引用残留。

**真诚度判定：** 真诚落地。不只是表面改措辞，而是明确承认 v2 的真诚度问题并给出技术修正。

---

### R9（MAJOR 建议改）— **PARTIAL（轻度残留）**

**修订位置：**
- 第 99 行（切换成本估算表 C 行注）：已统一为 "原 v1 表述'ssh2 功能弱于 russh'不准确" ✅
- 第 239 行（方案 C 路径 Step 4）：已统一为 "原 v1 表述'ssh2 功能弱于 russh'不准确" ✅
- 第 347 行（Alternatives_considered 方案 C）：已统一为 "原 v1 表述'ssh2 功能弱于 russh'不准确" ✅

**残留问题（轻度）：**

第 250 行（第 3.3 节"为什么不推荐" 第 2 点核心论据段）仍残留：
> `...切到 ssh2 (Node.js) 后，SOCKS5 仍然要自己实现（因为 ssh2 也只提供 channel API，SOCKS5 协议层是项目自写的）——两边在隧道功能上功能对等，原 v1 表述"ssh2 动态 SOCKS5 转发支持不完整"是错的。真正的成本在于：`

这是 v2 实际存在的**第 4 种** v1 措辞变体——"ssh2 动态 SOCKS5 转发支持不完整"，**未在 v3 统一为 "ssh2 功能弱于 russh"**。Planner 在第 9.5 节 R9 自评中声称"全文 v1 措辞已统一为'ssh2 功能弱于 russh'"，**不准确**。

**为什么是 PARTIAL 而非 FAIL：**

1. R9 核心要求（不再创造 v1 没有的措辞）的**主体落地**——Critic 二审明确点名的两个错措辞（"SOCKS5 功能缺失"、"SOCKS5 功能查漏补缺"）都已修订
2. 第 250 行残留的"ssh2 动态 SOCKS5 转发支持不完整"虽然仍是 v1 措辞变体，但**语义方向一致**（都是 v1 错误论据的不同侧面表述）
3. 不构成 R9 应付——Planner 真诚修订了 Critic 明确点名的两处，遗漏的是同一语义簇中的第三处变体（可能 v2 实际有 4 种措辞，Critic 二审只发现了 3 种）
4. 不影响 ADR 论据一致性——该段主要论点（SOCKS5 两边对等）已用与 R5 一致的措辞表达

**Fix（可选，不阻塞 APPROVE）：** 把第 250 行的 "ssh2 动态 SOCKS5 转发支持不完整" 也改为 "ssh2 功能弱于 russh"。

**真诚度判定：** 主体真诚落地，1 处轻度残留未披露。归类为 Minor（不阻塞）。

---

### R10（MAJOR 建议改）— **PASS**

**修订位置：** 第 5.5 节"验收标准"（第 293-304 行），AC-1 到 AC-6 全部存在

**核查证据：**
- **AC-1：** grep `State<'_, Arc<Mutex<SshSessionManager>>>` 返回 0（可测试） ✅
- **AC-2：** `app.manage(ssh_mgr.clone())` 行已删除（可测试） ✅
- **AC-3：** grep `invokeBrowserPreview` 返回 0（可测试） ✅
- **AC-4：** 上传 500MB 文件堆内存峰值 < 200MB（**具体数值** ✅，但采集方式略模糊）
- **AC-5：** ui-smoke/ui-extended 在 Tauri 模式下通过（可测试） ✅
- **AC-6：** mock SSH server 覆盖 connect/upload/download/tunnel 4 命令（可测试） ✅

**残留问题（轻度）：**

AC-4 / AC-5 / AC-6 没说明**指标如何采集**：
- AC-4 "500MB 文件 < 200MB 堆"——通过 Chrome DevTools Memory profiler？还是 Tauri WebView2 Performance API？还是 CI 自动化采集？
- AC-5 "Tauri runtime 模式"——是 WebDriverIO + tauri-driver？还是 Playwright + WebView2 自动化？这两种 E2E 工作量差别很大
- AC-6 "mock SSH server 覆盖 4 命令"——是 assert return value 还是 assert side effect？

这是 Executor 视角的轻度缺口，但 AC 的**判定阈值**已经清晰（grep 命令、数值阈值、命令清单），不算"模糊表述"。Planner 第 9.5 节 R10 自评"每条都对应 grep 命令或运行时指标，可直接进入 CI 校验"——基本准确，但"直接进入 CI"略有夸张（executor 还要决定采集工具）。

**真诚度判定：** 真诚落地。AC 都可测试，结构补齐，足以让 executor 有客观"完成"判定。指标采集方式缺失是 minor。

---

### R11（PARTIAL 建议改）— **PASS**

**修订位置：**
- 第 289 行（风险表 R4 缓解措施）：追加 "v3 R11 已补 mock 方案调研项"
- 第 368 行（Follow-ups R4 项）：追加 3 候选方案 + 比较说明
- 第 302 行（AC-6）：mock SSH server 覆盖 4 命令

**核查证据：**

第 368 行原文：
> `为 russh 后端补集成测试（mock SSH server，覆盖 connect/upload/download/tunnel 4 个命令）（v2 新增，R3 修订项；v3 R11 补 mock 方案选型）——缓解风险 R4...调研可选 mock 方案（v3 R11 新增）：[rust-ssh-test-server / docker openssh-server / russh mock handler]，执行阶段二选一—— russh mock handler 最轻量但需自写 protocol 层桩；docker openssh-server 最贴近真实但需 CI 环境支持 Docker；rust-ssh-test-server 若存在成熟 crate 则是折中`

3 候选方案都给出并明确比较（轻量 / 真实 / 折中），不是空泛的"调研"。AC-6 覆盖 4 命令明确。IPC OOM 判定标准（500MB < 200MB 堆）含在 AC-4。

**残留问题：** 无关键残留。`rust-ssh-test-server` 是否实际存在 Planner 用"若存在成熟 crate"做了诚实的条件限定——这是真诚而非应付。

**真诚度判定：** 真诚落地。

---

### 附加修订 1（Minor Finding 2：remote forward 桩函数披露）— **PASS**

**修订位置：** 第 88 行（现状基线表隧道转发行）

**核查证据：**
> `local + dynamic SOCKS5 已实现（含 active/error/status，join handle 持有，可停止）；remote forward 是返回 Err 的桩函数（ssh.rs:1135-1143），未实现（v3 新增披露，Critic Minor Finding 2）；85% 成熟度反映 local+dynamic 完成 + remote 桩函数占位`

桩函数事实已诚实披露，85% 评分的依据（local+dynamic 完成 + remote 占位）已澄清。

**真诚度判定：** 真诚落地。

---

### 附加修订 2（Ambiguity 2：AND/OR 关系明确）— **PASS**

**修订位置：** 第 316-330 行（第 6 节回滚触发条件）

**核查证据：**

第 320 行明确：
> `路径 A — 立即回滚（硬触发 OR 软触发持续超时，两者任一即触发）`

第 330 行追加 v3 明确化段：
> `原 v2 表述"任一软触发 + 上方任一硬触发条件满足 → 触发回滚"字面读是 AND（两者同时发生），与下文"软触发单独 > 2 周启动评估"的 OR 语义冲突。v3 改为：硬触发单独即回滚，软触发单独持续 > 2 周启动逃生舱评估——两者都是 OR（任一满足即触发），不存在 AND 关系。`

路径 A（立即回滚）和路径 B（逃生舱评估）触发逻辑明确分离，AND/OR 歧义已消除。

**真诚度判定：** 真诚落地。

---

### 附加修订 3（Minor Finding 3：Tauri 升级 buffer 下限校正）— **PASS**

**修订位置：** 第 286 行（风险表 R1）+ 第 359 行（Consequences 负面）

**核查证据：**
- 第 286 行："每次 Tauri 版本升级预留 **0-3 人天**迁移成本（多数小版本 0 成本，最坏 3 人天...v3 按 Critic Minor Finding 3 把下限从 1 调到 0）"
- 第 359 行："v3 按 Critic Minor Finding 3 把下限从 1 改为 0（多数小版本升级可能 0 成本，统计学上更严谨）"

下限从 1 改为 0，统计学逻辑（多数小版本 0 成本）已说明。

**真诚度判定：** 真诚落地。

---

## B. 第 9.5 节 Critic Review Resolution 真诚度自评核对

Planner 在第 9.5 节给出 R7-R11 + 3 项附加修订的真诚度自评。逐条独立判断：

| 修订 | Planner 自评 | Critic 三审独立判断 | 一致性 |
|---|---|---|---|
| R7 | "真诚落地" | **PASS** | ✅ 一致 |
| R8 | "真诚落地" | **PASS** | ✅ 一致 |
| R9 | "真诚落地" | **PARTIAL（轻度残留）** | ⚠️ Planner 自评不准确——第 250 行残留"ssh2 动态 SOCKS5 转发支持不完整"措辞变体未统一 |
| R10 | "真诚落地" | **PASS（轻度残留：AC 指标采集方式未说明）** | ✅ 基本一致（Planner 的"直接进入 CI 校验"略夸张但不构成不真诚） |
| R11 | "真诚落地" | **PASS** | ✅ 一致 |
| 附加 1 | "真诚落地" | **PASS** | ✅ 一致 |
| 附加 2 | "真诚落地" | **PASS** | ✅ 一致 |
| 附加 3 | "真诚落地" | **PASS** | ✅ 一致 |

**Planner 主动披露的"3 处残留水分"准确性核查：**

Planner 在第 9.5 节末尾（第 454-456 行）主动披露：
1. **R2/B3-B4 工作量同质化**（Minor Finding 4，v3 未修订）— **披露准确**，确实存在但属 Minor
2. **第 1 节"~70% 成熟度"算术平均 vs 加权**（Ambiguity 1，v3 未修订）— **披露准确**，加权公式确实不透明

**Planner 未披露的残留水分（Critic 三审新发现）：**

3. **第 250 行残留 v1 措辞变体"ssh2 动态 SOCKS5 转发支持不完整"**（R9 范畴）— **未披露**。Planner 在 R9 自评中声称"全文 v1 措辞已统一"，但实际第 250 行还有一处变体。

**未披露水分的严重度判定（Realist Check）：**

1. **现实最坏情况：** 不影响 ADR 论据一致性，因为第 250 行该段的主要论点（SOCKS5 两边对等）已用与 R5 一致的措辞表达。残留的只是 v1 错误论据的**第 4 种**措辞变体（"动态 SOCKS5 转发支持不完整"），语义方向与其他 3 处一致。
2. **缓解因素：** R9 主体要求（修订 Critic 明确点名的两处错措辞）已落地；遗漏的是同一语义簇中的第三处变体，可能是 Planner 写 v2 时v1 实际有 4 种措辞而 Critic 二审只发现了 3 种，不是 Planner 故意应付。
3. **检测速度：** 未来 reviewer 立即会发现（grep "SOCKS5" 即可见），但不会质疑 ADR 可信度——因为该段主要论据已正确。
4. **是否夸大：** 不属于 hunting mode bias——这是 grep 客观发现。

**严重度：** 不下调到 Open Question（因为有客观证据），但归类为 **Minor**（不阻塞批准）。理由：R9 主体落地真诚，残留是同一语义簇的措辞变体未清理，不影响 ADR 内部一致性或结论方向。

---

## C. 最终裁决

### **APPROVE**

**裁决理由：**

1. **R7（CRITICAL 必改）真诚落地** — 这是上次明确的硬条件，v3 已彻底消除深度对比表与 R5 修订的内部矛盾，Electron 评分（3→4）和加权总分（39→41）调整方向正确，与 R5 修订完全一致。
2. **R8-R11 + 3 项附加修订全部落地** — 每项都给出了具体修订位置和实质内容，不是表面应付。
3. **R9 存在 1 处未披露的轻度残留**（第 250 行"ssh2 动态 SOCKS5 转发支持不完整"措辞变体），但程度轻：
   - R9 主体要求（修订 Critic 明确点名的两处）已落地
   - 残留是同一语义簇的措辞变体，不影响 ADR 内部一致性
   - 归类为 Minor，不阻塞批准
4. **Planner 主动披露的 2 处已知水分**（B3/B4 工作量同质化 + 加权公式不透明）诚实准确
5. **核心结论方向、决策原则、备选方案评估、风险识别、回滚策略、验收标准** 七个维度全部达标
6. **ralplan 共识流程要求**（Principle-Option Consistency、Alternatives Depth、Risk/Verification Rigor）全部满足

**未升级到 ADVERSARIAL 的理由：**
- 未发现新的 CRITICAL
- 未达到"3+ MAJOR"阈值（R9 残留已下调为 Minor）
- 不存在系统性应付模式（R7/R8/R10/R11 + 3 项附加修订都真诚落地，R9 仅 1 处遗漏）

**Realist Check 校准：**
- R9 第 250 行残留措辞 → **保持 Minor**。Mitigated by: R9 主体要求已落地 + 残留是同一语义簇变体 + 不影响 ADR 论据一致性 + 未来 reviewer 不会因此质疑 ADR 可信度。

**ralplan 共识达成判定：**
- Architect v1 评审 → ITERATE → v2 修订 → Critic v2 二审 → ITERATE → v3 修订 → Critic v3 三审 → **APPROVE**
- ADR 可转为 `Status: approved (v3)`
- ralplan 流程结束

---

## Final Notes（已知 minor issues，不阻塞批准，可在执行阶段顺带处理）

1. **第 250 行 R9 残留**（可选修订）：把 "ssh2 动态 SOCKS5 转发支持不完整" 改为 "ssh2 功能弱于 russh"，以彻底实现 v1 措辞统一。
2. **AC-4/5/6 指标采集方式**（执行阶段决定）：AC-4 堆内存采集工具（DevTools / WebView2 API / CI 自动化）、AC-5 E2E 框架（tauri-driver / Playwright+WebView2）、AC-6 mock SSH server 断言方式（return value / side effect）可在执行阶段由 executor 决定。
3. **B3/B4 工作量同质化**（已知）：Slint（声明式）vs egui（immediate mode）工作量都给 40-60 天未体现范式差异，若未来 reviewer 重新评估可补差异化估算。
4. **加权公式 Ambiguity 1**（已知）：表头"前 3 维度 ×2 权重"无法精确算出 41，建议未来 reviewer 重新核算或显式列出每个维度的权重。

---

## Open Questions（unscored）

1. **Architect 张力 3 的技术断言错误（"transferQueue 强依赖 invokeBrowserPreview"）**是否需要反馈给 Architect agent 修订其评审文档？建议 ralplan 协调器把此问题记入 Architect 评审的 erratum，避免未来其他 ADR 复用该错误前提。
2. **rust-ssh-test-server crate 是否实际存在**？Planner 用"若存在成熟 crate"做了诚实条件限定，但执行阶段需要验证。如果不存在，docker openssh-server 是更稳妥的 fallback。
3. **v1 实际有多少种措辞变体**？Critic 二审发现 3 种，三审发现第 4 种（第 250 行）。是否还有第 5 种？建议未来 reviewer 在 v4（如存在）阶段用更细粒度的 grep pattern 一次性核查。

---

## Ralplan Summary Row

- **Principle/Option Consistency:** **PASS** — 5 条 Principles 真实驱动推荐，备选否决基于原则非偏好。R7 修订后内部一致性恢复。
- **Alternatives Depth:** **PASS** — B1/B2/B3/B4 + C 五个备选方案都有具体评估和否决理由；B3/B4 工作量同质化是已知 Minor（Planner 已披露）。
- **Risk/Verification Rigor:** **PASS** — R1-R4 风险缓解具体可执行；软触发条件可观测；R4 mock 方案 3 候选已列出比较；新增 6 条 AC 验收标准（grep 命令 + 数值阈值）；AC 指标采集方式是 Minor 缺口（不阻塞）。
- **Deliberate Additions（ralplan 强制项）：** N/A（本 ADR 非 deliberate 模式）。

---

**评审结束。裁决 APPROVE。ADR 可转为 `Status: approved (v3)`。ralplan 流程结束。**
