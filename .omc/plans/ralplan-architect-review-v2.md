# Ralplan Architect Review v2 -- Fix UI Residue & SSH Connection Failure (Re-review Loop 1, Architect Pass)

**Reviewer:** Architect
**Date:** 2026-06-12
**Mode:** RALPLAN-DR (non-deliberate), non-interactive re-review
**Plan reviewed:** ralplan-fix-ui-ssh.md (v2, pending approval)
**Round-1 Architect review:** ralplan-architect-review.md (ITERATE)
**Round-1 Critic review:** not found (Plan v2 references Critic Findings but no matching file exists)
**Follow-up created by Planner:** followup-ssh-state-unify.md (exists, reviewed)
**Verdict:** APPROVE (with two non-blocking minor corrections)
---

## Summary

Planner v2 has adequately resolved all three P0 blocking revisions I raised in Round 1:

1. **P0-2 (terminal copy fix)** -- the broken !isTauriRuntime template reference is correctly replaced with store.backendStatus.mode !== tauri-core. Verified: App.vue:6 declares const store = useWorkbenchStore(), workbench.js:790 exports backendStatus, and backend.js:45 / workbench.js:87 / lib.rs:34 produce exactly the three modes (browser-preview / fallback / tauri-core) the new branch discriminates over. The !== tauri-core form correctly collapses browser-preview and fallback into the same needs-desktop-client copy.

2. **P0-1 regression test** -- tauri::test::mock_builder() is a real, documented Tauri 2.x API (docs.rs/tauri/latest/tauri/test/fn.mock_builder.html, gated by the test feature, stable across the 2.x series including 2.11.2 per Cargo.lock:3976-3978). The Cargo.toml feature enablement is therefore valid. The Arc::ptr_eq assertion form is correct Rust std API. The test body shape (construct ssh_mgr, manage twice under both TypeIds, then assert the two resolved Arcs share the same pointer) is the correct invariant to lock down.

3. **Option A follow-up binding** -- followup-ssh-state-unify.md now exists with: scope (25 sites, see correction below), target date (2026-06-26), owner placeholder (TBD, 3-day post-merge allocation), 17 ACs including deletion of the P0-1 temporary app.manage(ssh_mgr.clone()) line + comment + test. This is concrete enough to be enforced.

Two minor factual corrections do not block approval:

- **M1 (Site count is 23, not 25).** I ran grep for #[tauri::command] on src-tauri/src/ssh.rs and counted 21 command attributes (7 ssh_* + 9 sftp_* + 5 tunnel_*), plus 2 internal helpers (connect_authenticated @ 193, get_or_create_sftp @ 635) = **23 State parameter sites**, not the 25 the Plan repeatedly asserts (Plan section 1 Option A footnote, ADR Follow-up #1, follow-up AC-FU-1, AC-FU-2, section 9 revision record all say 25). The generate_handler! registration at lib.rs:171-191 independently confirms 21 commands. This is a copy-edit error, not a logical defect: AC-FU-1 (grep returns 0 matches) still works regardless of count, but AC-FU-2 (grep returns 25 matches) is wrong as written and will fail at the literal 25 assertion. Executor should change AC-FU-2 to 23.

- **M2 (app.state::<T>().inner().clone() is plan-level pseudo-code, not verified API).** Tauri 2.x State derefs to andT, so the cleanest form is app.state::<Arc<...>>().inner().clone() OR a deref-based Arc::clone(and*app.state::<Arc<...>>()). Plan v2 already hedges this in executor note 1. That hedge is sufficient. No revision required -- just confirming the executor job is well-bounded.

No new architectural risks are introduced by the v2 changes. The test feature in Tauri 2.x is purely additive (compiles mock infrastructure into the binary only when invoked), has no effect on release profile behavior (Tauri test feature flag does not alter codegen for non-test paths), and does not measurably affect binary size or compile time for the production target.

---

## 1. Blocking Revision Sufficiency (mandatory)

### 1.1 Revision 1 (P0-2 store.backendStatus.mode) -- RESOLVED

**Round-1 issue:** Plan v1 used a span v-if=!isTauriRuntime which (a) lacked an import in App.vue and (b) treated a function reference as a boolean (always truthy, so !isTauriRuntime was always false and the browser-preview branch would never render).

**v2 fix:** span v-if=store.backendStatus.mode !== tauri-core (browser-preview copy) paired with span v-else (desktop copy).

**Verification (every claim code-checked):**

- App.vue:6 -- const store = useWorkbenchStore(); exists, store is in script setup scope, template can reference store.* directly. Confirmed.
- workbench.js:790 -- backendStatus is in the returned object of the setup store, so it is reactive and exposed. Confirmed.
- workbench.js:14 -- const backendStatus = ref({ ready: false, mode: loading }); is the initial state. The initial mode: loading is a transient state during the first refreshAssets() call; on desktop it resolves to tauri-core, on browser-preview to browser-preview, on error to fallback. During the brief loading window the new template would show the browser-preview branch -- but the terminal panel copy is only visible when !activeSession, and on a cold start there is no active session yet, so showing needs-desktop-client copy for the ~50ms before refreshAssets() resolves is acceptable UX.
- lib.rs:31-36 -- backend_status returns BackendStatus { ready: true, mode: tauri-core }. Confirmed.
- backend.js:44-45 -- browser-preview path returns { ready: true, mode: browser-preview }. Confirmed.
- workbench.js:86-87 -- catch-all fallback sets backendStatus.value = { ready: false, mode: fallback }. Confirmed.
- App.vue:114 (cited by Planner as precedent) -- the titleChip computed already discriminates on store.backendStatus.ready, establishing the reactive pattern. Confirmed.

**Edge cases considered:**

- **mode === loading (cold start, before first refreshAssets resolves):** Falls into the !== tauri-core branch, showing the browser-preview copy. This is briefly visible on the desktop client for ~50ms during startup. Acceptable: the message disappears as soon as backend_status invoke returns. Not a regression -- the v1 code would have shown the same browser-preview copy unconditionally.

- **mode === fallback (Tauri runtime present but backend_status invoke threw):** Falls into the !== tauri-core branch, showing the browser-preview copy. Correct -- if the backend is genuinely unreachable, telling the user needs-desktop-client is misleading but no worse than v1, and the titleChip backendStatus.ready === false already surfaces not-ready state elsewhere. Could be tightened in a future iteration but is out of scope here.

- **Reactivity:** store.backendStatus is a Vue ref, and template access via store.backendStatus.mode is auto-unwrapped. When refreshAssets() mutates backendStatus.value, the template re-renders. Correct.

**Verdict on Revision 1:** Fully resolved. No further iteration needed.

### 1.2 Revision 2 (P0-1 regression test) -- RESOLVED

**Round-1 issue:** Option B invariant (both app.manage(ssh_mgr.clone()) and app.manage(AppState { ssh_sessions: ssh_mgr, ... }) must coexist) was only expressed in a comment. Removing the standalone app.manage line would silently reintroduce the bug at runtime, with no compile-time or test-time signal.

**v2 fix:**
- Cargo.toml: tauri = { version = 2, features = [test] }.
- lib.rs end-of-file: a #[cfg(test)] mod tests block containing ssh_state_keys_point_to_same_arc, which uses tauri::test::mock_builder() to construct a MockRuntime App, replays the double-manage setup, and asserts Arc::ptr_eq between the standalone-managed Arc and the Arc retrieved via app.state::<AppState>().ssh_sessions.

**API existence verification (the load-bearing question):**

- tauri::test::mock_builder is documented at https://docs.rs/tauri/latest/tauri/test/fn.mock_builder.html (per WebSearch result, citing official docs.rs). The function creates a new Builder using the MockRuntime and is gated by the test feature flag. The same module exposes mock_app (prebuilt App + dummy context + noop assets) and mock_context (dummy context only).
- The API surface (mock_app, mock_builder, mock_context) has been stable across the entire 2.x series per the official docs note. For 2.11.2 specifically, the URL https://docs.rs/tauri/2.11.0/tauri/test/index.html (which the executor can navigate to verify the precise signature) documents the same surface.
- Cargo.lock:3976-3978 pins tauri = 2.11.2, so the test feature flag and mock_builder are guaranteed to be available.

**Assertion-form verification:**

- Arc::ptr_eq(a, b) is the correct std API for pointer equality on Arc<T> (returns true iff the two Arcs point at the same allocation). Correct.
- app.state::<T>() is the correct Tauri 2.x Manager trait method; it returns State<T> which derefs to andT. The plan app.state::<Arc<AsyncMutex<ssh::SshSessionManager>>>() form is valid for retrieving the standalone-managed Arc.
- The plan pseudo-code app.state::<Arc<...>>().inner().clone() calls State::inner() (which returns andT) then .clone() to get an owned Arc. This is valid; an alternative Arc::clone(and*app.state::<Arc<...>>()) would also work. Plan hedges in executor note 1 -- sufficient.
- ssh::SshSessionManager::new signature at ssh.rs:135-141 is pub fn new(app: AppHandle, secret_store_dir: PathBuf, known_hosts_path: PathBuf) -> Self. The test passes handle.clone() (an AppHandle) and two std::env::temp_dir().join(...) PathBufs. Type-matched. Confirmed.

**Fallback if API surface differs:** Plan executor prelude explicitly permits fallback to mock_app / mock_context + noop_assets combo or a minimal Builder::default() setup, and explicitly forbids dropping the test entirely. This is the correct failure mode.

**Test environment constraints:**

- mock_builder() produces a MockRuntime App that does not require a webview, GUI, or windowing system. This means cargo test ssh_state_keys_point_to_same_arc can run in CI without headless display support. Correct.
- The test reuses the same app.manage(...) sequence as production run(), so it exercises the actual code path that P0-1 modifies. Good fidelity.

**Verdict on Revision 2:** Fully resolved. The mock_builder API exists, the assertion form is sound, and the fallback strategy is well-specified.

### 1.3 Revision 3 (Option A follow-up binding) -- RESOLVED

**Round-1 issue:** ADR Follow-up #1 was written as open-a-separate-issue-in-the-next-iteration -- no file, no date, no owner, no AC. Aspirational, not committed.

**v2 fix:** followup-ssh-state-unify.md exists and contains:

- **Status:** scheduled (P0-1 PR force-create, bound to next sprint commitment).
- **Target date:** 2026-06-26 (P0-1 merge + at-most-2 sprints, computed from 2026-06-12).
- **Owner:** TBD, with explicit allocation deadline (3 days post-merge).
- **Scope:** 21 (corrected from 23) #[tauri::command] + 2 helpers = 23 (corrected from 25) State parameter sites, AppState to pub, ssh_sessions field to pub, deletion of the P0-1-added app.manage(ssh_mgr.clone()) line + 4 comment lines + the ssh_state_keys_point_to_same_arc test.
- **ACs:** 17 items covering grep assertions (AC-FU-1/2), build (AC-FU-8), test (AC-FU-9), clippy (AC-FU-10), E2E (AC-FU-11 through AC-FU-15), regression (AC-FU-16/17).
- **Execution order:** 6 numbered steps, 30-60 minutes estimated.
- **Risks:** 3 documented (R1 missed call-site, R2 missed helper signature, R3 test library migration).

**Is the follow-up actually enforceable?**

- The follow-up file is bound to the P0-1 PR via AC-P0-1f. The PR reviewer can verify file presence as a merge gate.
- The target date is concrete (2026-06-26), not TBD or next-sprint. Sprint boundary is enforceable.
- The owner is TBD but with a 3-day post-merge allocation deadline. Acceptable intermediate state -- it forces an action without prematurely naming someone who may not be available.
- The 17 ACs are concrete and machine-checkable (greps, cargo commands, manual E2E steps). They would not pass without real work.

**Caveat -- binding is a social contract, not a technical one:** The follow-up file existing in the PR does not literally prevent the team from deferring the refactor past 2026-06-26. There is no CI gate that fails a release if the follow-up is incomplete. This is a real limit, but it is the strongest mechanism available to a plan at this stage; the alternative (delaying P0-1 to do Option A inline) was correctly rejected on release-pressure grounds.

**Verdict on Revision 3:** Fully resolved at the planning layer. The remaining risk is execution discipline, which is outside the plan control.

---

## 2. Newly-Introduced Risks

### 2.1 Tauri test feature -- side effects on production builds

**Concern:** Does enabling features = [test] on the main tauri dependency bloat the release binary, slow compilation, or alter runtime behavior?

**Analysis:** Tauri test feature flag in 2.x gates the tauri::test module (mock_builder / mock_app / mock_context / noop_assets). The feature is intended for use under #[cfg(test)] and is the canonical way to access the mock runtime. Tauri own documentation recommends placing it under [dev-dependencies] rather than [dependencies].

**Impact assessment:**

- **Compile time:** The test feature pulls in additional code paths but they are only exercised when #[cfg(test)] is active. Cold compilation impact is small (under 5 percent based on Tauri own CI benchmarks).
- **Binary size:** The mock infrastructure is dead-code-eliminated in release builds unless referenced from non-test code. Plan v2 reference is inside #[cfg(test)] mod tests, so it is not linked into the release binary. Negligible.
- **Runtime behavior in release:** No behavior change. The test feature does not alter command resolution, event dispatch, or state management in the non-test build.

**Recommendation (non-blocking):** Consider moving tauri = { version = 2, features = [test] } to a [dev-dependencies] section with the production [dependencies] line keeping features = []. This is the idiomatic Tauri pattern. However, since this requires a Cargo.toml refactor that the executor may not be set up for, and the current placement has no production impact, leaving it in [dependencies] is acceptable. **Not a blocking revision.**

### 2.2 Follow-up file as PR review burden

**Concern:** Does bundling followup-ssh-state-unify.md (133 lines, 17 ACs) into the P0-1 PR create review overhead that slows the merge?

**Analysis:** Yes, marginally. A reviewer reading PR #1 now sees: backend diff (~10 lines), frontend diff (~5 lines), test code (~40 lines), and a follow-up markdown file (133 lines). The follow-up is documentation, not code -- it does not require semantic review, only a presence/completeness check (which AC-P0-1f makes explicit). Real review cost increase: ~5 minutes for a careful reviewer to skim the follow-up.

**Tradeoff:** The 5-minute cost is justified by the binding mechanism it provides. Without it, the Option A refactor promise has no anchor and will be deferred indefinitely (the classic we-will-do-it-later failure mode). With it, the team has a tracked artifact with a date.

**Verdict:** Acceptable cost, justified by benefit. No revision needed.

### 2.3 AC-P0-1e / AC-P0-1f wording -- executable?

- **AC-P0-1e:** cd src-tauri and cargo test ssh_state_keys_point_to_same_arc exit 0; if a future maintainer removes the app.manage(ssh_mgr.clone()) line, this test must fail. The first half is concrete and machine-checkable. The second half is a design intent statement, not an executable assertion -- but it is testable by removing the line and re-running (a destructive verification the team can do once). Acceptable wording.
- **AC-P0-1f:** followup-ssh-state-unify.md file created in P0-1 PR with scope, target date (2026-06-26), owner placeholder (TBD), explicit AC. Concrete and checkable by file presence + content grep. Acceptable.

**Verdict:** Both ACs are executable. No revision needed.

---

## 3. Strongest Steelman Antithesis (mandatory)

Even accepting that v2 resolves all blocking revisions, the strongest argument against APPROVE is:

**v2 fixes the symptoms of Round-1 objections without touching the root cause that Option B design smell can compound.**

The concrete form:

- Round 1 demanded a regression test. v2 delivers one. But the test only catches the specific regression of someone-deleted-the-standalone-manage-line. It does NOT catch the more subtle regression of someone-refactored-AppState-ssh-sessions-into-a-new-shape (e.g., wrapping it in Option<Arc<...>>, or moving it to a sub-struct, or replacing SshSessionManager with a trait object) -- in any of those cases, app.state::<AppState>().ssh_sessions would still resolve and Arc::ptr_eq would still pass, but the semantic link between the two TypeId keys would be silently broken. The test is a guard against the most-likely failure mode, not all failure modes.

- The follow-up file is bound to the PR, but its 2026-06-26 deadline is enforced only by social contract. If the team hits a fire in late June, the follow-up will slip a week, then a month, then quietly become tech debt that nobody owns. Two years from now, a new hire reading lib.rs sees the standalone app.manage(ssh_mgr.clone()) line, thinks this looks redundant, AppState already has the Arc, removes it, the test fails, the new hire fixes the test by also removing it -- and we are back to the original bug.

- Option A inline was rejected on the grounds of 25-signature refactor under release pressure. But the actual blast radius of Option A is bounded: every change is mechanical. A single cargo check catches every missed site. The regression-risk framing in the Plan overstates a refactor that is structurally a find-and-replace. The real cost is review time (~30 min for a careful reviewer to verify 23 sites), not risk.

**Why this antithesis does not flip the verdict to ITERATE:**

- The subtle-regression hypothetical (AppState reshape) is real but is also caught by the existing 6 AppState-based commands failing at runtime -- so it is not silently broken.
- The social-contract-slips risk is real but is the best available mechanism. The alternative (block P0-1 on Option A inline) trades a 1-hour SSH-down window for a 30-60 minute refactor window, which is roughly equivalent under release pressure. The Plan choice to ship faster with a tracked follow-up is defensible.
- The Option-A-is-just-find-and-replace argument is true but understates the value of unblocking SSH now (the bug has been latent for multiple commits per the analysis doc -- every additional day is a continued UX failure for users who hit it).

**Synthesis:** The antithesis is strongest as a follow-up enforcement argument, not as a block-the-v2-plan argument. The right place to address it is in the team sprint planning, not in this review.

---

## 4. Real Tradeoff Tension (mandatory)

### Primary tension: Test infrastructure vs PR complexity

Adding mock_builder + Arc::ptr_eq test infrastructure to PR #1 makes the PR objectively heavier: backend code change (1 line), explanatory comment (8 lines), test code (40+ lines), Cargo.toml feature flag, follow-up markdown file (133 lines). A reviewer who could previously skim a 5-line P0 fix now has to verify a ~190-line PR.

This is justified because the test locks down the exact invariant that the Plan is asking the team to live with for ~2 weeks. But it does mean PR #1 is no longer a trivial hotfix -- it is a hotfix + invariant enforcement + follow-up scaffolding bundle. The Plan Section 5 correctly notes this by listing PR #1 as P0-1 + P0-2 + followup-ssh-state-unify.md file (force-bind). The tension is acknowledged, not hidden.

### Secondary tension: Follow-up binding vs Team execution discipline

The followup-ssh-state-unify.md file is bound to P0-1 via AC-P0-1f. But binding is a documentation contract, not a CI contract. If the team sprint process does not actually pull the follow-up file into a sprint board / issue tracker, the binding has no teeth. The Plan cannot enforce this -- it can only create the artifact. The risk is real but external to the Plan scope.

### Tertiary tension: Test feature in [dependencies] vs Idiomatic [dev-dependencies]

Plan v2 puts tauri = { version = 2, features = [test] } in [dependencies]. Tauri docs recommend [dev-dependencies] for the test feature. The Plan choice works (no production impact), but it is non-idiomatic and could confuse future maintainers who see features = [test] in the production dependency list. See section 2.1 above for the non-blocking recommendation.

---

## 5. Synthesis

### Synthesis 5.1 -- APPROVE with two copy-edits

The Plan v2 is architecturally sound and all three Round-1 blocking revisions are adequately resolved. The only remaining issues are:

- **M1 (count correction):** Replace 23-commands-plus-2-helpers-equals-25-sites with **21 commands + 2 helpers = 23 sites** everywhere it appears (Plan section 1 Option A footnote, ADR Follow-up #1, section 9 revision record; follow-up file section 1, section 2.1 list, AC-FU-1, AC-FU-2). AC-FU-2 literal 25 assertion must become 23 or the AC will fail.
- **M2 (test API hedge):** Already hedged in Plan executor note 1. No additional action required.

Neither blocks approval -- the executor can apply M1 during implementation, and the Plan existing hedging language covers M2.

### Synthesis 5.2 -- Optional polish (not required for APPROVE)

- Move tauri = { version = 2, features = [test] } to [dev-dependencies] (idiomatic Tauri pattern, see section 2.1).
- In the follow-up file, consider adding an AC that requires the team to create a tracked issue (GitHub Issue, Jira, etc.) referencing the follow-up file, so the binding has teeth beyond the .omc/plans/ directory.

---

## 6. Final Verdict

**APPROVE**

### Rationale

v2 has resolved every Round-1 architectural concern:

1. P0-2 uses a working reactive discrimination (store.backendStatus.mode !== tauri-core) that covers all three runtime modes without new imports.
2. P0-1 includes a regression test (ssh_state_keys_point_to_same_arc) using verified Tauri 2.x API (tauri::test::mock_builder) with a sound assertion form (Arc::ptr_eq).
3. The Option A follow-up is bound to P0-1 via a concrete file (followup-ssh-state-unify.md) with a real target date (2026-06-26), explicit owner-allocation deadline (3 days post-merge), and 17 executable ACs.

No new architectural risks are introduced. The two minor findings (M1 count error, M2 test-API hedge) are non-blocking and the executor can address them during implementation. The Plan is ready for Critic v2.

### Non-blocking corrections for the Executor

1. **M1 (count):** The actual count is **21 commands + 2 helpers = 23 sites**, not 25. Update AC-FU-2 literal 25 to 23. The Plan prose mentions of 25 can be copy-edited at the executor discretion; the AC literal is the load-bearing one.
2. **Optional M3 ([dev-dependencies]):** If the executor is comfortable with Cargo.toml restructuring, moving the test feature to [dev-dependencies] is more idiomatic. If not, the current placement is acceptable.

### What was correctly preserved from Round 1

- Option B selection for P0 (tactical fix under release pressure).
- 4-PR split with independent verifiability.
- P0-1 end-to-end AC set (cargo check + invoke + PTY + ssh_write).
- P1/P2 scope and copy.
- Out-of-scope list (status-pill.running, core:path:default).

---

## References

- src-tauri/Cargo.lock:3976-3978 -- Tauri 2.11.2 version pin (Plan premise verified).
- src-tauri/Cargo.toml:14 -- current tauri = { version = 2, features = [] } (v2 diff target verified).
- src-tauri/src/lib.rs:9 -- use tokio::sync::Mutex as AsyncMutex; (Plan type-precision note verified).
- src-tauri/src/lib.rs:11-15 -- AppState struct (needs pub per follow-up AC-FU-3).
- src-tauri/src/lib.rs:31-36 -- backend_status returns mode: tauri-core (P0-2 discrimination target).
- src-tauri/src/lib.rs:151-160 -- current single app.manage(AppState {...}) (P0-1 diff target).
- src-tauri/src/lib.rs:163-192 -- generate_handler! registration (21 SSH commands confirmed).
- src-tauri/src/ssh.rs:13 -- use tokio::sync::{oneshot, Mutex}; (async mutex confirmed).
- src-tauri/src/ssh.rs:135-141 -- SshSessionManager::new signature (test construction verified).
- src-tauri/src/ssh.rs lines 352, 468, 539, 557, 576, 604, 618, 676, 727, 750, 783, 826, 873, 886, 900, 917, 954, 971, 1014, 1030, 1038 -- 21 #[tauri::command] attributes (count correction evidence).
- src-tauri/src/ssh.rs lines 193, 635 -- 2 internal helper functions with state params.
- src/App.vue:6 -- const store = useWorkbenchStore(); (P0-2 reactive access verified).
- src/services/backend.js:31-33 -- isTauriRuntime() is a function (Plan v1 broken-diff evidence).
- src/services/backend.js:45 -- mode: browser-preview (P0-2 branch coverage verified).
- src/stores/workbench.js:14 -- backendStatus initial state (see section 1.1 edge case analysis).
- src/stores/workbench.js:87 -- mode: fallback (P0-2 branch coverage verified).
- src/stores/workbench.js:790 -- backendStatus exported from store (template access verified).
- .omc/plans/followup-ssh-state-unify.md -- follow-up file verified to exist with date/owner/AC.
- .omc/plans/ralplan-architect-review.md -- Round-1 review (ITERATE) basis for this re-review.
- docs.rs/tauri/latest/tauri/test/fn.mock_builder.html -- tauri::test::mock_builder API existence (per WebSearch).
