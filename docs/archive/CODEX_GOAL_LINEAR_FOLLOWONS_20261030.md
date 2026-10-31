# CODEX GOAL — Linear Multi Launcher Reliability and Productivity Follow-ons

**Program ID:** `multi-launcher-linear-followons-20261030-r01`  
**Repository:** `multiplex55/Multi_Launcher` (Windows Rust, egui/eframe)  
**Working repository:** discover the actual checkout; historical location `G:\Repos\rust\Multi_Launcher` is a hint, **not** an instruction to use a stale ZIP or another checkout.  
**Recommended repository destination:** `docs/plans/linear-followons-20261030/` (put both Markdown files together there).  
**Companion execution entry point:** `CODEX_KICKSTART_LINEAR_FOLLOWONS_20261030.md`  
**Research/source-of-truth baseline observed while authoring:** `master` at `22c20ff87c65b0d90461da1b0a5a50c0e88176e5` (merged PR #1459). **At execution, fetch and verify current `origin/master`; never reset to this historical SHA.**  
**Authority:** user requests this sequenced implementation/verification program. Each named stage receives one bounded attempt and its stated correction allowance; external permissions, unsupported native capability, user-data protection, and dependency gates are NOT waived.  
**Execution mode:** one orchestrator, one implementation writer at a time, sequential workstreams, scoped reviewers, small accepted commits and normal pushes. One final manual user review, not mid-stage check-ins.

## 1. Mission, measurable deliverables, and non-goals

Complete or truthfully disposition the outstanding work from the merged `multi-launcher-reliability-closeout-20261029-20261029-r01` program, in this order:

1. **G02 Table publication safety** — explain the exact Windows native parent-admission failure and complete its guarded publication/lifecycle proof where supported.
2. **G05 Hot Folder conditions** — prove native source read cancellation/retirement, preserve authoritative source through effects and bind actual queued launcher/MkMacro starts before any activation.
3. **G03 Table full-file operations** — **conditional**, only after ALL G02 required gates are accepted; otherwise record a precise dependency deferral and continue.
4. **G06 Jump Mode native focus/settings proof** — complete the real owned-window interaction protocol for code already accepted in `master`.
5. **G08 MkMacro history** — revalidate the privacy correction, obtain qualifying complete-toolbar and run-history tests, then accept a bounded UI or defer with evidence.
6. **G04 Screenshot native redaction** — verify physical secondary-button crop and actual Save/Copy pixels, only if supported computer-use and clipboard capabilities exist.
7. **G07 ZIP extraction** — resolve actual held-source/ancestor and no-replace native publication incompatibilities; only then implement a bounded Stored writer and gated minimal UI.
8. **G08 optional invocation recipes policy** — decide explicitly after accepted History whether an existing template workflow is sufficient or a separately safe, user-authored invocation workflow can be accepted. Do not create replay features by implication.
9. **Final linear-chain reconciliation, independent cumulative review, and one PR** to the then-current `master`; PR creation is allowed, automatic merging is NOT.

The order favors G02/G05 safety dependencies first, follows with G03 only if unlocked, gives Jump and MkMacro history opportunities to finish without waiting for unsupported screenshot/ZIP native capabilities, and leaves the optional recipe policy last. **This program is not a continuation of the old 17-goal queue**, and it is not authorization to implement unrelated workspaces, search rankers, general input/task frameworks, new caches, generic filesystem infrastructure, or other feature ideas.

**Acceptable overall outcome:** complete, reviewed work where proof succeeds; exact `DEFERRED_*` / `PARTIAL_ACCEPTED` decisions where the named native dependency, tool support, or bounded verification does not succeed. A "program complete" state means every stage has a truthful final disposition and the final PR contains **only accepted source plus documentation**, NOT that every requested feature is enabled. No refactor, test weakening, mocked native claim, or unnecessary Cargo campaign to make a completion percentage look better.

## 2. Authoritative source documents (read before coding)

Read current repository `AGENTS.md`, any nested instructions, the actual source owners and current `master`. The following **already merged** documents are binding technical context; avoid duplicating or contradicting their safety contracts. Preserve their past evidence as history and never treat unaccepted donor-branch code as the working baseline.

**Merged closeout index root:**

`docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/`

- `PROGRAM_STATUS.md`, `ACCEPTED_CHECKPOINTS.md`, `INTEGRATION_MANIFEST.md`, `DEPENDENCY_MAP.md`, `FOLLOW_ON_CATALOG.md`, `RISKS_AND_DECISIONS.md`, `FINAL_HANDOFF.md`.
- `goals/G02/follow-on/native-parent-admission-proof/{GOAL.md,KICKSTART.md}` and G02 `SAFETY_CONTRACT.md`, `SOURCE_REVALIDATION.md`, `VERIFICATION.md`, existing receipts/test inventory.
- `goals/G05/follow-on/g05-native-read-retirement/{GOAL.md,KICKSTART.md}`, G05 `SAFETY_CONTRACT.md`, verification and accepted B1 provenance.
- `goals/G06/follow-on/native-jump-focus-proof/{GOAL.md,KICKSTART.md}` and native-M3 receipts.
- `goals/G04/follow-on/native-gesture-output-proof/{GOAL.md,KICKSTART.md}` and the original screenshot contract.
- `goals/G07/follow-on/native-guard-publication-compatibility/{GOAL.md,KICKSTART.md}`, native diagnostic receipts and planner security/format decision.
- `goals/G08/follow-on/g08-history-verification-restart/{GOAL.md,KICKSTART.md}`, G08 R1 reviewer pair, `HISTORY_CONTRACT.md`, failed/late verification receipts.
- `goals/G08/follow-on/explicit-invocation-recipe-policy/{GOAL.md,KICKSTART.md}`.
- `goals/G01/follow-on/file-search-sender-fixture/{GOAL.md,KICKSTART.md}` is **an independent, unexecuted test-only repair**, not automatically part of this program. Address it only if a newly selected required target cannot compile and a bounded, reviewer-approved fixture-only fix is absolutely necessary; otherwise keep it as an independent follow-on.

**Original complete goal charters:**

`docs/plans/reliability-feature-closeout-20261029/goals/` with `G02-table-publication-safety`, `G03-table-full-file-operations`, `G04-screenshot-native-redaction`, `G05-hotfolder-condition-evaluator`, `G06-jump-settings-and-focus`, `G07-zip-extraction`, and `G08-mkmacro-run-history`. Read their **GOAL and KICKSTART**, not only the shorter follow-on instructions. They carry the omitted downstream acceptance obligations. Their historical branch/reset instructions are **superseded solely by the linear chain defined in this document**; their functionality, proof gates, refusal conditions and non-goals remain binding.

Read-only historical prototype branches when needed: `codex/closeout-g02-table-publication-safety-20261029-r01`, `codex/closeout-g07-zip-extraction-20261029-r01`, `codex/closeout-g08-mkmacro-run-history-20261029-r01`. **NEVER merge or cherry-pick these branches wholesale**; the closeout deliberately excluded their unaccepted Rust. Use `git show` / existing receipts to inspect source and reproduce/reimplement only separately reviewed, accepted deltas on the active linear branch.

## 3. CRITICAL: branch topology is ONE connected chain, not independent forks

Use **one line of history**. Create a descriptive branch for each stage **from the exact final commit of the preceding stage**. Branch names are movable category labels for cumulative commits. They are NOT independent copies from `master` and are NOT merged into each other.

```text
actual verified origin/master @ RUN_START
     |
     +-- codex/linear-01-g02-table-publication-20261030
             |  G02 accepted commits OR documentation-only truthful deferral
             +-- codex/linear-02-g05-hotfolder-20261030
                     |  G05 accepted commits OR documentation-only deferral
                     +-- codex/linear-03-g03-table-full-file-20261030
                             |  G03 if gate passes; otherwise docs-only BLOCKED
                             +-- codex/linear-04-g06-jump-native-20261030
                                     |
                                     +-- codex/linear-05-g08-mkmacro-history-20261030
                                             |
                                             +-- codex/linear-06-g04-screenshot-native-20261030
                                                     |
                                                     +-- codex/linear-07-g07-zip-extraction-20261030
                                                             |
                                                             +-- codex/linear-08-g08-recipes-policy-20261030
                                                                     |
                                                                     +-- codex/linear-09-final-review-20261030
                                                                             |
                                                                             +-- ONE PR: final branch -> master
```

**Exact invariants:**

- B1 is created once from the fetched current `origin/master`; every later B(n) is created from the **exact pushed closing SHA** of B(n-1). Do not fetch/pull `master` and branch from it between stages. No `git rebase`, `reset --hard`, force push, independent `master` forks, merge of sibling goal branches, or intermediate PR merges.
- At handoff the current HEAD on the new branch must **equal** the preceding branch's closing SHA. After its first commit, verify `git merge-base --is-ancestor <previous_sha> HEAD` succeeds and the parent chain contains it; record old/new branch names, base, closing SHA and successful/failed push.
- Each stage must publish at least its source/baseline check and final disposition; push after each meaningful checkpoint. A fully blocked stage still creates a **documentation-only closing commit**, and the next branch starts from **that** exact commit.
- At all times the active chain contains only **accepted** production code. If an experiment/source change fails review, preservation and rollback must be restricted to task-owned paths: save exact patch, donor SHA, logs and hashes into its stage evidence, remove only verified task-authored uncommitted changes (or use an isolated disposable **detached** worktree), then commit the honest docs-only deferral. Never advance the main chain while unaccepted source remains staged/committed; do not discard a user's unrelated changes. A scratch checkout may be detached, but it must not become a second production branch or a forked handoff.
- The final review branch is created from B8's final SHA. **ONE final PR** uses B9 as head and `master` as base; its diff contains all accepted stages. Prior category branches need no individual PR or merge. No branch deletion until their receipts and one final PR are reviewed.
- If the default branch advances during the program, record the divergence. Do **not** silently move the established chain, force-push/rebase, or drop checkpoints. Submit the final PR normally; if GitHub reports conflicts, use a separately documented on-chain integration resolution under normal authorization or leave the PR open with an exact blocker. There is no `master` merge inside the chain by default.
- A stale branch name from a previous run must not be overwritten. On resume, match the existing `PROGRAM_ID`, SHA and branch map; continue the actual first unfinished checkpoint. If a name collides with an unrelated run, use a recorded `-r02` suffix without changing the chain rule.

## 4. Codex roles, autonomy, and model policy

The user has restored **Sol medium for the main Codex agent** and **Luna max for the implementer**. Do not silently upgrade to Astra/xhigh because older follow-on charters requested it. The current repository's `.codex/config.toml` and `.codex/agents/*.toml` must be **read**, not edited. In the recent repository snapshot the parent was `gpt-6.1-sol` / `medium`, implementer was `gpt-6-luna` / `max`, planner/reviewer were separately configured; at run time **actual on-disk settings and accepted dispatch metadata prevail**. Record requested, configured and host-confirmed effective model/effort separately; if the backend does not expose resolution, write `UNVERIFIED`. Do not claim a model was used solely because a config file says so.

- **Orchestrator (Sol medium):** source/permission/ancestry verification, checkpoint packet, delegation, scope, source acceptance, Git/branch/PR, status and stall reports. No speculative deep redesign.
- **Implementer (Luna max):** one narrow source writer per accepted packet; implement/test/reconcile named owner paths and return exact diff, source identities, assertions, commands, times and first diagnostic. No source edits from other agents concurrently.
- **Planner:** only bounded architectural uncertainty (especially real native Windows ownership and exact existing APIs); output an executable packet with 1–2 alternatives and stop criteria. Use configured existing planner; no model setting change.
- **Reviewer:** independent read-only review of final exact candidate source plus tests/evidence, once per substantive checkpoint. Explicitly distinguish a review of a static plan from a runtime/native acceptance; allow one bounded correction round, never an endless review loop.
- **Native GUI/OS:** Codex must operate supported native tools and its own isolated fixtures, app and screenshots where possible. Never ask user to conduct routine tests mid-plan. If support is unavailable, record `NATIVE_PROOF_DEFERRED` and continue independent stages; the user receives **one** final optional manual review checklist.

No background monitoring or promises. Execute steps sequentially in the active session/context; before any interruption persist a real checkpoint, next action, process ownership and normal push result. Only one implementation writer and one Native/Cargo test process family at a time. Avoid expensive duplicate builds: reuse a test receipt only when target, features, dependency graph and all relevant source blobs match exactly; otherwise run the stage's bounded selection.

## 5. Program ledger and artifacts (created by first stage)

Write under `docs/plans/execution/multi-launcher-linear-followons-20261030-r01/` on the active chain:

```text
PROGRAM_STATUS.md                # authoritative current stage, source/base, status, commits/push, exact next action
BRANCH_CHAIN.md                  # B1..B9 branch name, creation parent SHA, closing tip, proof of ancestry
SOURCE_BASELINE.md              # actual fetched master + original accepted source/donor differences
DEPENDENCY_MAP.md               # G02->G03; G05 native->effect->editor->enablement; G08 history->recipes
ACCEPTED_SOURCE_MANIFEST.md      # every accepted source/test path and exact blob, excluded prototype paths
RISKS_AND_DECISIONS.md           # live native/access limits, no-change decisions, non-goals
FOLLOW_ON_CATALOG.md             # minimal new continuation pairs only for truly unresolved items
FINAL_HANDOFF.md                 # which features really work, what was not run, expected next action
PR_SUMMARY.md                    # one final PR scope, evidence limits, explicit exclusions
MANUAL_REVIEW_CHECKLIST.md       # one final user review only
stalls/<stage>-<checkpoint>-<UTC>.md
stages/G02/ ... stages/G05/ ... stages/G03/ ... stages/G06/ ...
stages/G08-history/ ... stages/G04/ ... stages/G07/ ... stages/G08-recipes/ ...
roles/<role>/...                 # attributable source/review packets; stage-scoped if preferred
```

Each stage maintains `SOURCE_REVALIDATION.md`, `ACCEPTANCE_MATRIX.md`, `VERIFICATION.md`, `PROGRESS.md` and (only when truly useful) `UNACCEPTED_SOURCE.patch` / `NOT_RUN.md`. Every status line states what was **observed** versus what was only **designed**. Acceptance states: `ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `DEFERRED_DEPENDENCY`, `NATIVE_PROOF_DEFERRED`, `FAILED_VERIFICATION`, `NO_CHANGE`, or `PAUSED_RESUMABLE`. Labels are not substitutes for exact evidence.

Every artifact must use valid relative links or a pinned GitHub reference. Record evidence first: current tool/version, current branch/tip/parent, actual source blobs and Cargo feature/profile, nonzero test list and counts, exact command/exit/timing, relevant native handle/process/fixture cleanup, independent reviewer disposition, commit SHA and push result. Do not retroactively label a previous historical failed run as passed.

## 6. Stage-by-stage functional contracts and checkpoints

### S1 / B1 — G02: Table publication native ownership (P1, blocks G03)

**Read:** merged G02 follow-on pair, original G02 pair, `goals/G02/SAFETY_CONTRACT.md`, native receipts and original 19-case inventory.

**Historical stop:** two selected 19-case runs with **15 pass / 4 fail**; `CreateFileW` OS error **5** during verified parent-chain admission, before stage creation. Original M1-A publication source was **unaccepted**; M1-B cancellation/recovery was **not started**. Current integrated file-backed pages are not complete full-file publication.

**Owners:** `src/file_operations.rs` (`NativeTablePublication::reserve::hold_parent`, `NativeRenameScanBackend::admit_root`, `open_native_object`, `BackendFailure`), `src/table_utility_session/publication.rs` and directly necessary `multi_launcher_table_core` resource owners only if a concrete proof requires them. No new native abstraction, no second FFI, no Table UI yet.

**M0-A — inspect/reproduce safely.** Compare current `master` owner bytes with preserved failed donor/receipts, reconstruct exact 19 selected tests and 4 failing names. Instrument existing `BackendFailure.path` and actual access/share flags to identify the failing ancestor. Use **only** owned disposable `tempfile::tempdir()` or equivalent under a verified task root; record path/volume/handle ownership. Do not weaken ACLs, elevate, loosen reparse/sharing rules, use arbitrary user files, or substitute path-only publication.

**M1-A — smallest corrected native publisher.** If source/current host actually supports it, prove `CREATE_NEW` staging, source/parent/stage identity, expected-absent vs occupied vs appeared output, same-source refusal, non-clobber handle-bound rename, source retention, quota precharge, sync-before-effect, error-path ownership and cleanup. Native Windows adapter must actually run, not only mocks or generic refusal assertions. Retain the original selected controls and **all four previously failing cases**; a first successful setup does not prove all invariants.

**M1-B — bounded lifecycle and recovery, only after M1-A accepted.** Complete original G02 contract: in-flight cancellation, partial stream writes and charge accounting, no false terminal success, held stage/source/parent until safe retirement, cleanup error retention, replay/restart/recovery and no write to an occupied or substituted destination. Design owner-specific proof, not a broad native helper rewrite. Do not expose public full-file entry points yet.

**M2-A — reviewer + disposition.** Independently review actual exact source and the native observations. `ACCEPTED_COMPLETE` requires *both* M1-A and M1-B plus original safety contract and focused tests passing. If OS5, unsuitable execution context or time cap persists, `NATIVE_PROOF_DEFERRED` or `PARTIAL_ACCEPTED` **without** enabling publication and without unlocking G03. Preserve unaccepted source outside the advancing chain.

**G02 finite budget:** one recorded offline/locked fast-dev selected compile/list preparation **600 seconds**, built selected run **180 seconds**, at most **one localized correction** in an additional **600-second aggregate incremental** allowance, and at most one corrected selected rerun **180 seconds**. The verified 19-case inventory must include 9 new + 10 controls unless a reviewed current-source delta legitimately changes it. M1-B gets a separately recorded fresh bounded packet only after accepted M1-A, at most 1200-second selected list/preparation, 180-second selected run, 600-second correction preparation and at most two affected reruns. No full suite, CI claim, ignored test, mock replacing native proof, or automatic budget renewal.

### S2 / B2 — G05: Hot Folder native read retirement → source/effect binding → safe UI (P1)

**Read:** G05 follow-on pair, original G05 pair and `SAFETY_CONTRACT.md` and accepted M1-A/B1 receipts. **Already merged:** 19/19 metadata/binding, corrected 22/22 pure literal-stream tests. **NOT merged/enabled:** native B2 source ownership, actual queued-start binding, editor/Test Rule and final condition activation.

**N0 — real pending-read proof first.** On a task-owned admitted **regular file**, demonstrate that a native read has genuinely entered the pending/blocking operation; then request cancellation, observe the *actual terminal status*, join/retire its worker and handles/buffers and demonstrate the next operation starts fresh. `AtomicBool` checks between synchronous reads, cached completion, pipes-only, pre-cancellation, parser blocking and timeout-only are **not** equivalent. Consult the existing native `src/file_operations.rs` owner and official Windows CancelIoEx/CancelSynchronousIo semantics without adding generic native infrastructure, services or elevated privileges. If unavailable, stop without changing `FILE_CONDITIONS_NOT_EVALUATED` refusal.

**N1 — retained source and bounded evaluation.** Only after N0 independent acceptance, use existing Hot Folder `src/hotfolder/{model,conditions,execution,mod}.rs`, `HotFolderOperationPort` and existing guarded file-effects owner. Start **one** monotonic candidate budget before acquisition: <=**8 MiB** read in <=**64 KiB** chunks, <=**2 seconds** candidate deadline; combine fingerprint, bounded literal Contains/DoesNotContain and metadata in **one pass**. Validate root/parent/object, length/mtime, stable rule revision, genuine EOF and source binding. Retain/transfer exact admitted source into builtin effects; never fingerprint a guard then reopen a mutable path. Preserve old unconditioned-rule behavior, journal/undo, cooldown/rate limit, cancellation and cleanup.

**N2 — actual queued-start binding.** Inspect `complete_launcher_actions`, `invoke_registered_action_from_catalog`, and `dispatch_mkmacro`; revalidate service/rule revision/epoch/current control and held candidate evidence **at effect-time**. A changed source or rule must cause **zero** launch/macro invocation and one correlated terminal/refusal receipt. No double dispatch, stale reroute or async effect after cancellation.

**N3 — editor/Test Rule and final activation, only after N0–N2 acceptance.** Implement original G05 size/age/literal controls, validation and side-effect-free Test Rule; prove stale/unknown/denied/timeout/refusal and valid matches, preserve source bounds, replay/cancel rules and default settings. **Do not remove fail-closed gates or mark the feature enabled until the original full M1-C/M2-A/M3-A review criteria pass**. A partial foundation still leaves condition evaluation refused.

**G05 finite budget:** per independently authorized source/proof packet: selected Nextest preparation up to **1200 seconds**, run **180 seconds**, aggregate correction preparation **600 seconds**, up to **two** actual-error affected reruns; independent reviewer after each accepted transition. No app-wide/full suite/installation/forced termination or indefinite native probe. If N0 cannot be established with one bounded attempt and one targeted correction, preserve accepted existing foundations and defer N1–N3.

### S3 / B3 — G03: conditional Table full-file operations (P2, HARD GATE)

**Gate:** read S1 G02 acceptance matrix from the **current chain**, not the original donor or a claimed `15/19` improvement. Require `G02=ACCEPTED_COMPLETE`: native publisher, source/stage/parent identity, no-clobber, quota, cancellation, partial write, cleanup, recovery and independently reviewed proof. If **any** load-bearing G02 gate missing, **do not write G03 production source**, create `DEFERRED_DEPENDENCY.md` and a docs-only branch-close commit; proceed to S4.

If unlocked, follow original G03 complete Goal/Kickstart. Current owners: `src/gui/table_utility_dialog.rs`, `src/table_utility_session.rs`, `crates/multi_launcher_table_core/src/lib.rs` and the accepted G02 publication API. Make scope explicitly **page-only versus full-file**; output choices and confirmation must not silently promote a projected selection into a whole-file operation. Use a one-worker bounded stream with source identity/revision, stable schema/column selection, exact output/ownership/quota, progress, cancellation and no stale completion. Preserve compact export and last-good UI behavior. Tests: small, occupied destination, changed source, stale worker, partial cancel, cleanup, memory bound; optional synthetic >500 MiB is **bounded and skipped** if safe resource budget cannot be established—never whole-file load. Gate UI on accepted worker/publication; final reviewer accepts exact supported modes, no invented formats.

**G03 finite budget:** selected prep <=1200s, run <=180s, incremental prep <=600s and at most two localized reruns; optional large-source fixture requires explicit disk/memory/time upper bounds documented before allocating bytes; no broad full suite or unrelated crate migration.

### S4 / B4 — G06: native Jump settings, Save and focus (P2)

**Already accepted:** corrected editor/transaction and effective Paste/Jump conflict handling; focused **29/29** and exact app build passed. **Missing:** actual native window discovery, editor/Save, hint/foreign-focus evidence; prior tool failed with desktop-check **0x80070005** despite owned executable. Do not rewrite accepted settings code from this failure.

Run preflight first: verify supported computer-use tool/skill and one **owned exact-source executable** in a disposable cwd/profile. Paste Sequences next/cancel hotkeys must be disabled **in the fixture before constructing the app**, preserving production defaults. No guessed HWND, stale installed copy, user window, raw SendInput/UIA helper, ACL/security bypass or permanent setting changes. One bounded recovery if owned window is not returned, then a truthful deferral.

If supported, perform editor invalid input, local chord/hint alphabet validation, Save acknowledgement, reopen, changed chord vs fresh default, live hints, complete-hint focus then **fresh Enter** activation, Esc cancel, independent launcher query focus, Settings/Notes protected input, one owned foreground control and held-key-release fence. Capture before/after screenshots and actual process/window/source/profile identities; preserve user-data isolation. If real owner defect is demonstrated, a separately scoped existing-owner correction and focused tests/review only; prior successful deterministic tests do not certify native input.

**G06 finite budget:** at most one necessary actual-source app build <=**600 seconds**, one supported native scenario <=**180 seconds**, one bounded discovery recovery, and cleanup <=**60 seconds**. Reuse exact-source build receipt if source/profile/target still match. If tool support unavailable, `NATIVE_PROOF_DEFERRED` and continue.

### S5 / B5 — G08: MkMacro bounded history + toolbar privacy review (P2)

**Historical stop:** the prototype's original **14/14** passed, but a later toolbar privacy correction had a 32-second preparation deadline and **no qualifying corrected run**; late compiler exit 0 after 101.880 seconds **does not count**. No G08 production code was merged. **Do not import unaccepted history/source wholesale.**

Revalidate current `src/mkmacro/runtime.rs`, `src/mkmacro/runtime/summary.rs`, `src/gui/mkmacro_dialog/{history,toolbar,mod}.rs`, existing UI contracts, and the historical reviewer R1 pair. Preserve **one** existing canonical `SummaryHistory` store (<=100 completed records, <=4 MiB aggregate, <=1024 rows/run), a bounded view (20 headers + at most one selected off-page header, <=64 detail rows and <=256-byte sanitized names). History is metadata-only, no raw args/secret side effects, no Clone duplication, no new disk store/persisted replay, no auto-run. The complete **real toolbar + History** must hide arbitrary command/recording/current-name diagnostics in History while leaving actual Editor status, controls and AccessKit enablement intact; preserve generation/selection/clear authority.

At M1 reconstruct a clean, current-source bounded history implementation, using the corrected donor only as reviewable evidence. At M2 run a nonzero selected library inventory (expected **15**: 8 G08 + 7 existing runtime controls, revalidate exact names); explicitly include the `g08_history_r1_real_toolbar_composition_preserves_editor_diagnostics_and_controls` assertion, not just body-only privacy. Then one stable-source independent **R1-only** review/acceptance. Missing test or timeout means keep source unaccepted and emit exact restart proof. **No native/manual test is needed for this ordinary GUI functionality** if deterministic control/state proof is complete.

**G08 finite fresh authorization:** one prep/list <=**1200 seconds**, one selected run <=**180 seconds**, aggregate incremental prep <=**600 seconds**, <=2 actual-error localized reruns. This is a new explicitly scoped program allowance and **does not relabel or erase** the old exhausted timeout. Use `--cargo-profile fast-dev --offline --locked` when confirmed supported. Recipes are deferred to S8, never automatically replayed from History.

### S6 / B6 — G04: Screenshot physical gesture + Save/Copy native proof (P1 proof, no speculative fix)

**Preflight:** check current supported computer-use interface **before compiling or launching**. It must support a physical held **secondary-button** down/move/up or button-selectable drag, and a supported way to inspect/restorably save the actual **clipboard image**, in an isolated owned profile. The previously reported `DragInput` lacked a mouse-button selector; right-click once, accessibility secondary_action and invented tool fields are **not** equivalent. If missing, docs-only `NATIVE_PROOF_DEFERRED`, no source change, continue S7. No custom OS input helper, global capture of private content or bypass of controls.

If supported, use the complete existing 320x240 fixture and source-pixel calibration in the G04 follow-on Goal: native screenshot/capture; primary Solid Black redaction rectangle `[64,256)x[72,136)`; physical secondary crop `[16,304)x[16,224)`; Undo then Redo; Save and Copy. Output expected **288x208**. Compare every decoded RGBA pixel to an independently computed fixture/crop/redaction oracle **without calling the app's renderer as the oracle**; compare saved file and actual clipboard image. Record baseline capture, zoom/scale, editor focus, input and output, resource restoration and cleanup. The preexisting raster tests are **not** native gesture proof. Only a reproduced source bug authorizes a bounded change in `src/gui/screenshot_editor.rs` or `src/annotation/redaction.rs` followed by focused tests and independent review.

**G04 finite budget:** capability gate without build; if capable, native sequence <=**180 seconds** plus owned-resource cleanup <=**60 seconds**, at most one exact-owner corrected attempt after a reviewer-approved real defect. A source-only raster run is permitted only for an actual correction. No manual user involvement until final program checklist.

### S7 / B7 — G07: ZIP native guard/Stored writer/conditional UI (P2)

**Historical stop:** unaccepted `src/archive/{mod,preparation}.rs` and `src/file_operations.rs` prototype: **10/12** selected tests; held guarded publication returned Windows error **32**, and source/alias deletion test failed with exact endpoint/poststate **UNKNOWN**. There is a weaker directory-sharing control that succeeded; it is **not** an acceptable shortcut. The earlier relative `RootDirectory` rename returned ERROR87; do not retry it speculatively.

**R1 native owner proof before any writer:** inspect existing `archive_io::{Directory,Source,Stage}` and `open_native_object_with_sharing`, `create_native_stage_handle`, `delete_open_file`, `rename_open_file_no_replace`. On owned fixture `target/productivity_expansion/archive_native`, determine which source/alias spelling removal succeeded/failed and exact postconditions (both names, held object identity/bytes), and find **genuinely supported handle-bound no-replace** publication while maintaining *all* ancestor/containment guards. Include hard-link/reparse/parent-swap/effect-time collision and cancellation cleanup negatives; no path-only fallback, released guards, weaker share, new FFI subsystem, generic framework, user paths or unsupported API retries. If R1 fails, defer writer/UI, preserve unaccepted source off-chain.

**R2 writer only after R1 accepted:** stream supported strict **Stored** physical entries from retained validated source into held stages, enforce entry+batch quotas, real EOF/CRC, traversal/reparse/symlink rejection, atomic no-replace per output, cancellation and partial receipt/cleanup semantics. No Deflate/encryption/nested archive/overwrite or whole-file buffers. **R3 minimal UI only after reviewed R2 proof**; preserve existing read-only catalog on any failure. Complete original G07 Goal/security decision without inventing new archive scope.

**G07 finite budget:** one nonzero selected prep <=**1200 seconds**, selected run <=**180 seconds**, aggregate incremental prep <=**600 seconds**, <=2 localized correction reruns; `multi_launcher --lib --features perf-harness` and exact `g07_` + 6 controls only if current receipt verifies the same selection. Additional R2/R3 packets require prior accepted proof and similarly finite fresh documented limits. No dependency/manifest edits unless strictly required and explicitly scoped.

### S8 / B8 — G08 recipes: policy decision, conditional implementation only

**Hard gate:** accepted History source/verification/review from S5. If absent, record `DEFERRED_DEPENDENCY` and do not code. If present, R0 records whether "recipe" is adequately served by existing `src/mkmacro/templates.rs` Preview/Copy (with hotkey clearing) or a **named current-macro invocation**. Existing `src/mkmacro/invocation.rs`, direct run validation, runtime admission/parameters and source attribution remain owners; History summaries are **never** runnable commands.

R0 must state stable macro target identity, subset/mode, parameter and secret policy, preview/confirmation/explicit Start, disabled/deleted/changed-target refusal, storage owner and cancellation/completion attribution. If no safe existing contract or product intent is ambiguous, `NO_CHANGE / POLICY_REQUIRED`; **do not build a store, executor, logging, auto-trigger or replay feature**. Only if R0 is concrete and approved by an independent reviewer may R1 implement **at most one** narrow explicit invocation authoring/launch route, with focused targeted tests and review. Keep full R0 negative proof even on no-change.

**R1 finite budget if unlocked:** prep <=**1200 seconds**, selected run <=**180 seconds**, aggregate incremental prep <=**600 seconds**, <=2 localized reruns; no native/app/full-suite campaign. A docs-only R0 uses no Cargo.

### S9 / B9 — final reconciliation, acceptance, and one PR

Re-read all stage acceptances, current `master` comparison and branch chain. Run one scope-only final check (`git diff --check`, exact committed paths/manifest/links/JSON, source hashes, available selected test receipts). Independent cumulative reviewer checks no unaccepted G02/G07/G08 source, preserved G05 refusal when native unproved, G03 hard gate, stable command/input/Settings/MkMacro semantics, ZIP path safety and no `Cargo.toml`, `Cargo.lock`, `.codex` drift unless narrowly accepted and recorded. A global Nextest/Clippy/native campaign is **not authorized** by this final documentation checkpoint. A newly discovered source issue gets one bounded affected-owner correction, otherwise an explicit follow-on.

Write final `ACCEPTED_SOURCE_MANIFEST`, `DEPENDENCY_MAP`, `FOLLOW_ON_CATALOG`, `FINAL_HANDOFF`, `PR_SUMMARY` and one `MANUAL_REVIEW_CHECKLIST`. One PR from **B9 HEAD** to current GitHub `master`, with accurate source/test/limits, **no automatic merge**. After opening the PR, record actual number, head/base, checks/reviews as observed (empty != pass), and final push state. Leave intermediate category branches as readable historical checkpoints, not separate merge requests.

## 7. Verification, bounded process ownership, and stop policy

Default to selected **Nextest** on Windows, a real nonzero inventory, `--locked --offline` and the **existing** `fast-dev` profile only if available; do not edit TOML to invent one. Capture exact `-E` expressions based on CURRENT test lists/previous accepted inventory rather than hardcoding guessed filters. `cargo nextest list` is compilation/discovery; require output names/count and zero ignored tests before run. Set explicit independent deadlines for preparation/list and selected execution as above. Prefer one initial preparation, one run and at most the documented actual-failure corrective cycle; no 20-minute silence or unbounded recompile loops. For long processes, report elapsed phase/selected counts via a native command wrapper that owns **only its own** process tree, polls without altering target behavior and records output in owned ignored `target/` receipt files. No tool installs, `cargo clean`, vendored lint cleanup, full-suite Nextest, or automatic repeated CI.

Native tests must observe the **actual** named adapter/effect/native window/clipboard/pixel path—not a permissive mock, source inspection, "no crash", generic expected failure or stale installed executable. Use synthetic task-owned files, bounded bytes/time, exact parent/root and resource identity, explicit clipboard/profile restoration; never touch user work. FileSearch sender E0308 and historical OS errors 5/32 are factual blockers with narrow known boundaries, not proof that all current Windows/native operations are broken.

For a true material stop, preserve: stage, first failing operation/OS code, exact command/source/tree SHA, tool/API limitations, attempted bounded correction and remaining allowance, PIDs/handles/fixtures, cleanup, accepted-versus-unaccepted paths, smallest valid next action. Commit a **docs-only disposition** on that stage branch and **continue the next independent stage**. No infinite re-trial, guessing higher privileges, weakening safety assertions, treating late exit 0 after timeout as pass, or silent change of target/perms. If process termination is denied, stop edits, wait only as permitted for the task-owned process to drain within a bounded resource policy, and record final status; never kill an unrelated PID.

## 8. Final acceptance checklist

- [ ] Every stage's final outcome is source/evidence-backed, and both G02→G03 and G08 History→Recipes hard gates were enforced.
- [ ] All new branch pointers form one chain from verified run-start `origin/master`; no stage branched again from `master`, cherry-picked an old prototype tree or needed an intermediate merge.
- [ ] Every accepted code change has a matching reviewer and scoped test/native proof of the actual named behavior; unaccepted work was preserved as evidence but excluded from later commit ancestry.
- [ ] G05 conditions remain refused unless ALL source-retirement/effect/queued-start/editor and acceptance gates passed; G07 extraction UI remains refused unless native stage + Stored writer passed.
- [ ] G06/G04 native claims use actual supported owned-window/physical-secondary-drag/output proofs; a capability gap remains explicitly deferred.
- [ ] G08 history is not accepted on its pre-R1 14/14 or late compile alone; recipe workflows never infer replay authority from summaries.
- [ ] Artifacts record all commits/pushes, tests not run, warning/fixture and native cleanup limits, accepted source hashes, and the final GitHub PR URL.
- [ ] One final user review is possible; NO automatic PR merge or old branch deletion.

**Completion statement template:** `PROGRAM_COMPLETE_WITH_LIMITATIONS` or `PROGRAM_COMPLETE_ALL_ACCEPTED` followed by exact per-stage statuses. Never write "all features completed" when any native, dependency, security or source-acceptance gate is outstanding.
