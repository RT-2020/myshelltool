# Ralplan Architect Review -- Fix UI Residue & SSH Connection Failure

**Reviewer:** Architect
**Date:** 2026-06-12
**Mode:** RALPLAN-DR (non-deliberate), non-interactive
**Plan reviewed:** .omc/plans/ralplan-fix-ui-ssh.md
**Source analysis:** docs/ui-ssh-issue-analysis.md
**Verdict:** ITERATE (P0-1 acceptable; P0-2 has a blocking defect; ADR Follow-up #3 should be promoted from optional to required)

---

## Summary

Planner correctly identified the SSH failure as a TypeId mismatch between AppState and Arc<Mutex<SshSessionManager>> in Tauri StateManager, and the Option B choice (app.manage(ssh_mgr.clone())) is technically sound: Tauri 2.x State<T> resolves by TypeId::of::<T>() against a HashMap<TypeId, Arc<dyn Any>>, so registering the same Arc under two type keys gives both State<AppState> and State<Arc<Mutex<SshSessionManager>>> callers a valid hit, with identical backing data because both keys hold clones of the same Arc.

However, two issues require revision before Critic/Executor:
1. P0-2 diff is broken as written - uses isTauriRuntime (bare identifier, no call, no import) in App.vue template, where App.vue does not import it at all (App.vue:1-5).
2. ADR Follow-up #3 (compile-time safety net) is under-weighted - without it, the double-manage pattern can silently regress if someone removes the duplicate app.manage line in the future.

P1/P2 changes are low-risk and acceptable as described, modulo the P1-1 replacement copy which is fine.

---

## 1. Strongest Steelman Antithesis (against Option B)

Standing genuinely in the Option-A advocate shoes, the strongest argument is NOT "double-manage is ugly" - it is: Option B encodes a correctness invariant that cannot be expressed in the type system and therefore cannot be enforced.

### a) The invariant is invisible to future maintainers and tools

With Option A, the invariant "every SSH command can find its session manager" is encoded in the type signature (State<AppState>) and enforced by cargo check. If someone deletes app.manage(AppState {...}) from lib.rs:156, every #[tauri::command] in ssh.rs and the 6 AppState-based commands all break uniformly - the bug becomes a compile-blocker, not a runtime regression.

With Option B, the invariant "ssh_mgr must be managed both as a standalone Arc and inside AppState.ssh_sessions" lives in a comment (lib.rs lines noted in plan diff). The next maintainer who refactors AppState (e.g., splitting it into two structs, or removing the ssh_sessions field because they think it is unused after they migrate a helper) will silently break SSH again - same bug class, just reintroduced. Comments are not load-bearing; type signatures are.

### b) The "1-line diff" claim hides the true cost

Plan advertises "1 line + comment." True diff is:
- 1 new app.manage(ssh_mgr.clone()) line
- 1 explanatory comment block (3-4 lines per plan diff)
- 1 follow-up issue (ADR Follow-up #1: refactor ssh.rs 25 sites to State<AppState>)
- 1 implicit obligation to write a regression test (ADR Follow-up #3) - currently demoted to "consider"

That follow-up work has the exact same blast radius as Option A (25 signature edits + 2 helper edits). Plan is not avoiding Option A cost; it is deferring it. The deferral is acceptable only if the follow-up is scheduled with a deadline. The current ADR lists it as a vague "next iteration."

### c) Concurrent-access semantics are subtly worse under Option B

Both keys share the same Arc, so runtime data is identical - but if any future code calls app.state::<Arc<Mutex<SshSessionManager>>>() and app.state::<AppState>() in quick succession and then replaces AppState.ssh_sessions via interior mutation (e.g., state.ssh_sessions = new_arc), the standalone managed Arc goes stale. Option A makes this class of bug structurally impossible because there is only one path to the manager.

### Conditions under which Option A is actually better

- The team has time for a 30-minute mechanical refactor (25 sites, find/replace, single cargo check pass).
- The codebase is expected to gain more state types in the near future (each new "double-manage" compounds the cognitive cost).
- There is no immediate release pressure (the bug has been latent across multiple commits per git log).
- New contributors may touch this code (the comment is insufficient documentation for them).

**Counter-synthesis (where Option B remains correct):** release pressure is real (SSH is the product core feature and is currently 100%% broken), the diff is genuinely ~5 lines vs ~25, and the regression risk of touching 25 signatures under time pressure is non-trivial. So Option B as a tactical fix is defensible - provided the antithesis above is mitigated by a mandatory, time-boxed follow-up.

---

## 2. Real Tradeoff Tension

### Primary tension: "Unblock the critical bug now" vs. "Encode the invariant in types"

Plan currently leans hard toward the first pole (Option B, tactical). This lean is correct for the SSH bug in isolation - SSH being down is a P0 customer-impacting issue and deserves the shortest path to green.

But the lean becomes questionable when viewed holistically, because:
- The same plan also lists P1-1 (4-line copy rewrite), P1-2 (8-line fallbackAssets rewrite), P1-3 (SFTP fake data delete), P2-1 (pnpm to npm), P2-2 (CSS selector duplication). None of these are P0.
- If there is time for 4 PRs of non-critical cleanup, there is time for the 25-signature refactor that permanently closes the bug class.
- "Merge P0 fast, schedule Option A as follow-up" only works if the team actually executes the follow-up. ADR Follow-up #1 reads as aspirational, not committed.

### Secondary tension: "Preserve browser-preview demo value" vs. "Eliminate misleading fake data"

Plan tries to split the difference: fallbackAssets gets "[示例]" prefix (preserve demo, kill deception), SFTP preview data gets deleted entirely (no demo, no deception). This is inconsistent: same problem class, two different treatments. The justification given ("file column already has empty-state copy") is reasonable, but the inconsistency itself is a smell. A purer choice would be to delete both; a more demo-friendly choice would prefix both. Plan split is defensible but should be acknowledged as a deliberate tradeoff, not presented as obvious.

### Tertiary tension: "Per-PR independent verifiability" vs. "Atomic P0 (backend + frontend together)"

Plan recommends PR #1 = P0-1 (backend) + P0-2 (frontend copy). That coupling is correct - but only if P0-2 actually compiles after the change. See Section 3 below: as written, P0-2 will not compile. So PR #1 atomicity promise is currently broken.

---

## 3. Synthesis

### Synthesis 3.1 - Tactical Option B + Committed Option A follow-up

Accept Option B for P0-1 (unchanged), but promote ADR Follow-up #1 from "next iteration" to a scheduled, deadline-bound follow-up issue created in the same PR. Concretely:
- P0-1 PR must include (in the same commit or a tightly linked follow-up commit) an issue file .omc/plans/followup-ssh-state-unify.md with: scope (25 signatures + 2 helpers), target date (e.g., within 2 sprints), owner (TBD), and acceptance criteria.
- Without this, the team is accumulating a known smell with no enforcement.

### Synthesis 3.2 - Compile-time guard for the double-manage (mandatory, not optional)

ADR Follow-up #3 ("consider adding compile-time assertion") should be mandatory in the P0-1 PR, not optional. Concretely, add a unit test in src-tauri/ that calls app.state::<Arc<AsyncMutex<SshSessionManager>>>() and app.state::<AppState>() and asserts both resolve to the same underlying Arc pointer (compare Arc::as_ptr or Arc::ptr_eq after unwrapping). This makes the invariant executable, not commented. Cost: ~15 lines of test code. Benefit: future regressions of the exact bug class fail CI instead of breaking at runtime.

### Synthesis 3.3 - Replace P0-2 broken isTauriRuntime reference with store.backendStatus

App.vue already has access to store (App.vue:6) and store.backendStatus is exposed from the store (workbench.js:790). Its .mode field is "tauri-core" under Tauri (backend.js:32) and "browser-preview" / "fallback" otherwise (backend.js:45, workbench.js:87). So the cleaner P0-2 diff is:

```diff
-                    <div v-if="!activeSession" style="padding:var(--space-4);color:var(--muted)">SSH 终端需要桌面客户端。当前为浏览器预览模式。</div>
+                    <div v-if="!activeSession" style="padding:var(--space-4);color:var(--muted)">
+                      <span v-if="store.backendStatus.mode !== 'tauri-core'">SSH 终端需要桌面客户端。当前为浏览器预览模式。</span>
+                      <span v-else>请点击左侧主机建立 SSH 连接。</span>
+                    </div>
```

This avoids the import-and-call issues, reuses existing reactive state, and is consistent with how titleChip already discriminates on store.backendStatus.ready (App.vue:114).

Alternatively, if importing isTauriRuntime is preferred, the diff must:
1. Add `import { isTauriRuntime } from "./services/backend.js"` to App.vue:1-5.
2. Define a computed `const isDesktop = computed(() => isTauriRuntime())` and reference `isDesktop` in the template.

Either synthesis fixes the broken diff.

---

## 4. Architectural Soundness Checks

### 4.1 Does app.manage(ssh_mgr.clone()) actually resolve State<Arc<Mutex<SshSessionManager>>>?

Yes. Tauri 2.x Manager::manage<T: Send + Sync + Static>(self, t: T) -> Self inserts t into a HashMap<TypeId, Arc<dyn Any>> keyed by TypeId::of::<T>(). State<T> resolves via Manager::state::<T>() which looks up TypeId::of::<T>() in the same map. So:
- app.manage(AppState { ... }) registers key TypeId::of::<AppState>().
- app.manage(ssh_mgr.clone()) (with ssh_mgr: Arc<AsyncMutex<SshSessionManager>>) registers key TypeId::of::<Arc<AsyncMutex<SshSessionManager>>>().
- State<AppState> hits the first key; State<Arc<AsyncMutex<SshSessionManager>>> hits the second key.
- Both keys hold clones of the same Arc, so both see the same SshSessionManager instance at runtime.

**Important type-precision note:** The analysis doc and Plan loosely write Arc<Mutex<SshSessionManager>>, but ssh.rs:13 imports `use tokio::sync::{oneshot, Mutex};` - so the actual type is Arc<tokio::sync::Mutex<SshSessionManager>> (async mutex), not std::sync::Mutex. lib.rs:9 aliases this as AsyncMutex and lib.rs:14 declares the field as Arc<AsyncMutex<ssh::SshSessionManager>>. Plan diff (line app.manage(ssh_mgr.clone())) is type-correct because ssh_mgr is Arc<AsyncMutex<SshSessionManager>> and the State<Arc<Mutex<SshSessionManager>>> signatures in ssh.rs (where Mutex resolves to tokio::sync::Mutex per the use on line 13) refer to the same monomorphized type. No type error. But Plan should explicitly note this in the comment to prevent future readers from introducing std::sync::Mutex by mistake.

### 4.2 Fix ordering (P0 -> P1 -> P2) - reasonable?

Yes. P0 unblocks the core feature; P1 polishes the visible surface; P2 improves maintainability. Order is correct.

One caveat: P0-2 (terminal copy fix) is in P0 only because it shares the "user cannot tell what is wrong" property with the SSH bug. Once P0-1 lands, the misleading "browser preview mode" text becomes more visible (users will connect successfully and then return to a stale state where the message reappears on disconnect). So P0-1 + P0-2 in the same PR is correct coupling - but only if P0-2 actually works (see Section 3.3).

### 4.3 Missing dependencies?

Yes, one: P0-2 fixes the terminal copy but does not fix the analogous "browser preview" assumption elsewhere. workbench.js:488 has `if (!isTauriRuntime()) { ... }` and workbench.js:429 sets sessionId = "browser-preview". These are correct (they already use isTauriRuntime() correctly), so no fix needed there - but the plan should note that the terminal-panel message is the only place where the misleading copy appears, to make the fix scope explicit. (Confirmed via grep: App.vue:632 is the only occurrence of "浏览器预览模式" in the template.)

### 4.4 Will the 4-PR split produce broken intermediate states?

- PR #1 (P0-1 + P0-2): Atomic. After merge, SSH works AND terminal copy is correct. OK
- PR #2 (P1-1 + P1-4): Pure copy changes. No state coupling. OK
- PR #3 (P1-2 + P1-3): Pure backend.js browser-preview data changes. No effect on Tauri path. OK
- PR #4 (P2-1 + P2-2): Build config + CSS. OK

One risk: PR #2 P1-1 line 613 replacement copy says "通过 xterm.js 与远程主机交互，右侧实时显示当前目录文件". This is correct under Tauri (PR #1 merged first). If PR #2 were merged before PR #1 (unlikely given plan ordering), the copy would over-promise. Plan explicit "P0 must merge and verify before P1/P2" rule (Section 5) handles this. OK

### 4.5 Verification-method soundness

- AC-P0-1a (cargo check): necessary, not sufficient. Type-checks do not verify the TypeId collision actually resolves at runtime. Add: the unit test from Synthesis 3.2.
- AC-P0-1b (DevTools ssh_connect): good - directly exercises the failing code path. OK
- AC-P0-1c (real PTY prompt): excellent end-to-end signal. OK
- AC-P0-1d (ssh_write echo): catches the "even if connect works, write might not" regression. OK
- AC-P0-2b/c: as noted, will fail against the plan literal diff.

---

## 5. Final Verdict

**ITERATE**

### Required revisions before Critic

1. [Blocking] Fix P0-2 diff per Synthesis 3.3. Either:
   - (preferred) rewrite to use `store.backendStatus.mode !== 'tauri-core'` (no new import, reuses existing reactive state), or
   - add `import { isTauriRuntime } from "./services/backend.js"` to App.vue:1-5 AND define a computed `const isDesktop = computed(() => isTauriRuntime())`, then reference `isDesktop` in the template. Bare `!isTauriRuntime` in a template is always-false (function is truthy) and will not compile-resolve.

2. [Required] Promote ADR Follow-up #3 to mandatory. Add a unit test to P0-1 PR that asserts both `app.state::<Arc<AsyncMutex<SshSessionManager>>>()` and `app.state::<AppState>()` resolve and point at the same underlying Arc (via `Arc::ptr_eq` after `Arc::downgrade`/`upgrade` or `as_ptr` comparison). Place at src-tauri/src/tests.rs or inline `#[cfg(test)]` module in lib.rs.

3. [Required] Schedule Option A follow-up with a deadline. Per Synthesis 3.1, the P0-1 PR must create .omc/plans/followup-ssh-state-unify.md with concrete scope (25 signatures + 2 helpers), target date (<=2 sprints), and ACs. Without this, the "double-manage" smell accumulates indefinitely.

4. [Suggested] Tighten the P0-1 comment. Add explicit note that Mutex here is tokio::sync::Mutex (aliased as AsyncMutex), so future maintainers do not reach for std::sync::Mutex.

5. [Suggested] Make AppState fields pub or add accessor methods as part of the Option A follow-up. This is already implied by ADR Follow-up #1 but should be explicit so the refactor is not blocked by visibility decisions later.

6. [Suggested] Acknowledge the fallbackAssets vs SFTP-preview inconsistency (Section 2 secondary tension). Either prefix both with [示例] for symmetry, or delete both, or explicitly document the asymmetry.

### What is correct and should be preserved

- Option B selection for P0 - technically sound, lowest regression risk under release pressure.
- PR splitting - independent and reversible.
- AC set for P0-1 - strong end-to-end coverage.
- P1/P2 scope and copy - appropriate and low-risk.
- Out-of-scope list - correctly excludes the status-pill.running hardcode and the core:path:default cleanup as separate concerns.

### Approval gate

Once revisions 1-3 are addressed, this plan is APPROVE for Critic. Revisions 4-6 are polish.

---

## References

- src-tauri/src/lib.rs:9 - `use tokio::sync::Mutex as AsyncMutex;` (aliasing).
- src-tauri/src/lib.rs:11-15 - AppState struct with `ssh_sessions: Arc<AsyncMutex<ssh::SshSessionManager>>`.
- src-tauri/src/lib.rs:151-160 - ssh_mgr creation and single app.manage(AppState { ... }).
- src-tauri/src/lib.rs:163-192 - generate_handler! registration of 7 AppState commands + 23 SSH commands.
- src-tauri/src/ssh.rs:13 - `use tokio::sync::{oneshot, Mutex};` (confirms async mutex).
- src-tauri/src/ssh.rs:193, 354, 470, 541, 559, 635, 678 - confirmed State<Arc<Mutex<SshSessionManager>>> signatures.
- src/App.vue:1-5 - <script setup> imports; does NOT import isTauriRuntime.
- src/App.vue:6 - `const store = useWorkbenchStore();` (store available in template scope).
- src/App.vue:114 - existing precedent: store.backendStatus.ready used in computed.
- src/App.vue:632 - the broken copy line targeted by P0-2.
- src/services/backend.js:31-33 - isTauriRuntime() is a function (must be called).
- src/services/backend.js:32, 45 - mode: "tauri-core" vs mode: "browser-preview".
- src/stores/workbench.js:14 - backendStatus ref definition.
- src/stores/workbench.js:790 - backendStatus exported from store.
- src/stores/workbench.js:488 - correct isTauriRuntime() usage precedent.
- docs/ui-ssh-issue-analysis.md:196-329 - root-cause evidence and Option A/B comparison.
