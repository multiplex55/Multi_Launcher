# G04 GOAL — G12 Screenshot Redaction: Native Crop, Output and Interaction Acceptance

> **Program:** `multi-launcher-reliability-closeout-20261029`  
> **Disposition at plan creation:** `NOT_STARTED / NOT_EXECUTED`  
> **Priority:** P1 / output safety  
> **Lane:** native acceptance and only justified source correction  
> **Prerequisite:** No Table dependency; native proof required for end-to-end completion  
> **Companion:** [KICKSTART.md](KICKSTART.md)  
> **Historical planning-time master:** `56223fdc123396f4c41c48e0542506d912c6c634` (October 29, 2026; verify current upstream at runtime; **never reset to this SHA**).

## 1. Goal and user-visible benefit

Close the outstanding screenshot-redaction M2-A native workflow gap without weakening existing crop, gesture ownership, save/copy semantics or sensitive-content protection. Prioritize actual supported interaction proof over adding unreviewed features.

**Scope of user authorization when this package is launched:** execute only the milestones and bounded continuation rules in this Goal; this isn't permission to import unaccepted old queue branches, broaden into refactors, run a full test campaign, or merge into `master` automatically. This is a new action program, not the prior Astra research-only program.

## 2. Source-grounded initial observations

- Current master includes redaction layers/modes and cached previews. `apply_edits()` renders markup and crops the final image; `Save` and `Copy` use that image path in `ScreenshotEditor`.
- The previous G12 M2-A candidate had ordinary raster/logic tests but lacked required native crop/save/copy proof because the historical interaction driver did not expose secondary-button drag. Its unaccepted patch was excluded from `master`.
- A successful raster-only unit test or synthetic screenshot does not prove the complete native gesture-to-output workflow, nor privacy of blur/pixelate; Solid must be preferred for covering secrets.

The above is a **planning-time source observation**, not proof of runtime behavior or an assertion that all functions remain identical after this program starts. The worker must re-read the named owners at its actual task base and classify any drift before editing. Important links are pinned to the merged master for provenance:

- [`src/gui/screenshot_editor.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/screenshot_editor.rs) — `ScreenshotEditor::ui`, `apply_edits`, `save_image`, `copy_to_clipboard`, redaction/crop gesture owners
- [`src/gui/screenshot_capture.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/screenshot_capture.rs) — capture admission, cancellation, worker retirement
- [`src/annotation/redaction.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/annotation/redaction.rs) — bounded solid/blur/pixelate compositor
- [PR #1455](https://github.com/multiplex55/Multi_Launcher/pull/1455) — G12 M1-A/M1-B accepted, unproved M2-A excluded

## 3. Exact ownership, permitted changes and exclusions

- `src/gui/screenshot_editor.rs` actual gesture handler, transformations, redaction controls, preview, save and copy; `src/annotation/redaction.rs` only for a proved compositing defect.
- `src/gui/screenshot_capture.rs` only for a directly reproduced capture-owner/lifetime defect; do not refactor all screenshot ownership.
- Existing screenshot tests and supported first-party/native GUI harnesses, if they can drive *the actual* secondary crop + primary redaction interactions.

**Non-goals / changes to refuse:**

- No new screenshot backend, full OS capture suite, external image upload, invisible screen scraping or broad renderer rewrite.
- Do not broaden to unrelated pinboard/Screen Draw features without a verified shared-owner defect.

New files may be created under the goal-specific execution directory `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G04/` and in tightly scoped production owners above only. Do not edit `.codex` role/configuration files, historical research reports, unrelated worktrees, vendored crates, snapshots or independent user content. A previously uncommitted or blocked candidate is **evidence only**, never an implementation source to cherry-pick wholesale.

## 4. Required behavior and preserved invariants

- The original screenshot editor, capture cancellation, crop, marker/text/draw and undo/redo tools retain previous behavior.
- Save and Copy must use the same finalized redaction/crop composition; neither may leak previously visible source pixels in a claimed solid-redacted region.
- Blur/pixelate are visual effects, not guaranteed secret erasure; maintain truthful labels.
- Native interaction authority/focus and previous-foreground restoration remain owned by existing capture/editor paths, not a new global mouse hook.

If a required invariant has a concrete counterexample that the stated milestone cannot fix without substantial adjacent work, **stop that checkpoint**, preserve its evidence and author an explicit follow-on Goal/Kickstart. Continue the next independent program goal. An accepted subset may be integrated only if default behavior is safe, the subset is independent and the acceptance scope is truthful.

## 5. Milestones and bounded handoffs

| Checkpoint | Owner and outcome | Acceptance / intended commit |
|---|---|---|
| `M0-A` | Native feasibility and accepted-source inventory | Scoped work + tangible artifact; `docs(closeout): [G04/M0-A] ...` or `fix/feat` when source changed |
| `M1-A` | Build a truthful synthetic-or-native proof protocol | Scoped work + tangible artifact; `docs(closeout): [G04/M1-A] ...` or `fix/feat` when source changed |
| `M1-B` | Correct only demonstrated defects | Scoped work + tangible artifact; `docs(closeout): [G04/M1-B] ...` or `fix/feat` when source changed |
| `M2-A` | Run focused tests and native path if support exists | Scoped work + tangible artifact; `docs(closeout): [G04/M2-A] ...` or `fix/feat` when source changed |
| `M3-A` | Acceptance classification | Scoped work + tangible artifact; `docs(closeout): [G04/M3-A] ...` or `fix/feat` when source changed |

### M0-A — Native feasibility and accepted-source inventory

**Objective and detailed handoff.** Document exact existing controls and distinguish primary redaction drag from secondary crop drag. Inspect input harness/API and supported screenshot pixel-observation mechanisms, identify which native actions can truly be driven, what user-state isolation is needed, and which M2-A aspects are absent from master.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-A — Build a truthful synthetic-or-native proof protocol

**Objective and detailed handoff.** Describe one owned scratch screenshot with colored known pixels and secret-like synthetic text, one primary solid-redaction rectangle, one secondary crop, undo/redo, Save to owned temp PNG and Copy to controlled clipboard backend. Define expected pixel/text dimensions and exact order. Prefer automation support; do not simulate a secondary drag by calling internal crop setters while claiming native proof.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-B — Correct only demonstrated defects

**Objective and detailed handoff.** If supported automation exposes native path failures, change the smallest actual controller/editor owner and tests. Preserve existing accepted redaction/raster paths, markup history and crop semantics. An unprovable native interaction is recorded as `NATIVE_BLOCKED` with a complete future acceptance Kickstart, not an excuse to cherry-pick the excluded patch.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M2-A — Run focused tests and native path if support exists

**Objective and detailed handoff.** Selected unit/editor tests for raster/coords/hit testing, then one bounded native end-to-end capture-to-save/copy workflow *only* when physical button event capability is confirmed. Inspect the actual saved/copied pixel output. Ensure temporary files and clipboard state are isolated/restored when supported; do not require the user to perform manual checks mid-goal.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M3-A — Acceptance classification

**Objective and detailed handoff.** Declare `ACCEPTED_COMPLETE` only with real tested native gesture/output paths; otherwise mark `UNIT_VERIFIED_NATIVE_PENDING` and publish exact missing driver/tool constraints, safe controls and one final user manual-review checklist for eventual proof.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### Reviewer handoff rule

For each completed production milestone, the parent must provide a reviewer: actual diff from the correct base, original objective, named callers and existing defenses, new tests and selection inventory, first meaningful failure if any, exact run/receipt, stop/accept criteria and non-goals. The reviewer should return **severity-ranked substantive findings only** and identify stale callers, duplicate state, false positive test passes, unsafe activation, migrations or ownership violations. One substantive correction pass is allowed before a recorded stop/spike; do not loop indefinitely.

## 6. Focused acceptance / negative regression tests

- Pure raster tests: Solid obscures exact known pixel region, Blur/Pixelate bounded, crop bounds/transform and layer order, undo/redo final output.
- UI interaction tests: primary redaction and secondary crop are distinct; no underlying click-through/capture leakage if existing interfaces permit proof.
- Native output: owned synthetic image; explicitly invoke real primary/secondary drag; save/copy; compare PNG dimensions/hash/masked pixel coordinates.
- If driver lacks secondary drag, do **not** report end-to-end PASS; log exact missing API and produce a future native proof plan without repeatedly failing the same test.

Test execution is **not automatic simply because it is listed here**; first identify the actual Cargo package, present test names and test-target support. Select a nonzero meaningful subset. Prefer `cargo nextest list` and `cargo nextest run` scoped to named modules or exact test cases; if a test command selects zero, that is not PASS. Default allowance for one changed source checkpoint is one selected preparation, one selected run and at most two correction reruns **only when required**. The program-wide budgets in [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md) control timings, native proof and documentation-only checks. Never weaken tests, delete failures or claim native acceptance from pure raster/source tests.

## 7. Decision gates: block only unsafe scope, not the whole queue

- Missing supported native secondary drag or pixel-output inspection → `NATIVE_PROOF_DEFERRED` for that milestone; continue subsequent independent work.
- No automatic reuse of any old uncommitted M2-A candidate; source check plus current-scope review required.

Classify each checkpoint as `PENDING`, `IN_PROGRESS`, `ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `DEFERRED_DEPENDENCY`, `DEFERRED_WITH_DESIGN`, `NATIVE_PROOF_DEFERRED`, `FAILED_VERIFICATION`, or `NO_CHANGE_REVALIDATED`. The label must match the documented facts; a `PARTIAL_ACCEPTED` status is not feature-wide completion. On a true stop, create a durable per-goal `stalls/<checkpoint>-UTC.md` with actual source/ref, blocker, attempted one-shot fallback, unrun commands, ownership/cleanup, accepted files and exact next step; then return control to the parent to schedule the next independent goal.

## 8. Branch, commit, push, integration and model rules

1. Parent verifies actual `origin/master` and the program's accepted integration ancestry. Each **new** Goal receives its own branch `codex/closeout-g04-screenshot-native-redaction-<RUN_ID>` from the pinned program-start branch (contains only this plan) unless an explicitly accepted predecessor must be present. Dependent goals start from the prior **accepted integration commit** and record this exception. Never base on a rejected/dirty historical implementation tip.
2. Keep one active writer per goal; parent owns Git and the authoritative program ledger. Create the goal's execution directory and a `M0-A` docs baseline commit before editing production source. Accepted milestones each get meaningful Conventional Commit-style subjects including `G04/<milestone>` and one normal non-force push after commit. If push fails, preserve local commits and proceed; remote success must be recorded truthfully.
3. Parent integrates only reviewed, acceptance-gated production/documentation checkpoints into `codex/closeout-integration-<RUN_ID>` in dependency order (normally `git cherry-pick -x`). Do not import failed/unaccepted source. A rejected goal may contribute its **documentation-only** stop report/complete future pair, not an unsafe patch.
4. Role defaults verified from current `.codex`: orchestrator `gpt-6-astra/xhigh`; planner/reviewer `gpt-6-astra/max`; implementer `gpt-6-astra/xhigh`. These are configured role preferences, not proof of actual backend. If supported and selected for a narrow bounded implementation, `gpt-6.1-sol/xhigh` may be used deliberately, with honest attribution. No silent `.codex` configuration edits.
5. Roles contribute `roles/planner/`, `roles/implementer/`, `roles/reviewer/` and `roles/orchestrator/` Markdown under this goal's execution directory. New actionable recommendations get complete role-authored `roles/<role>/follow-on/<id>/GOAL.md` and `KICKSTART.md`; parent deduplicates into a canonical `follow-on/<id>/` library. Do not fabricate findings to fill quotas.
6. Do not ask the user to manually test between milestones; use supported app/native automation if available. If unavailable, log the gap and reserve a **single final manual-review checklist** for the program handoff.

## 9. Completed outcome definition

Either reproducible native crop/redaction/save/copy acceptance is achieved with relevant focused tests, or an honest, independently reviewable native-gate report and future Kickstart is published. No unsafe partial acceptance.

**Required closure record:** real branch/base, all milestone dispositions, exact changed files, accepted and rejected interfaces, focused test counts and command receipts, actual native proof or `NOT_RUN`, model/role provenance, reviewer disposition, commits/pushes, output/follow-on paths, source safety constraints, and exact next action. No PR merge/CI success may be claimed without host-confirmed evidence.

## 10. Execution progress — mutable (Codex updates here)

- **Current state:** `NATIVE_PROOF_DEFERRED`; reviewed documentation-only inventory/protocol/decision complete.
- **Run ID and branch:** 20261029-r01 / codex/closeout-g04-screenshot-native-redaction-20261029-r01.
- **Actual master/source base:** fetched4a1283e6; bootstrap02d0ac64; authoritative integrationdf94f192.
- **Current milestone:** `M2-A` native gate deferred; M0/M1 protocol and M3 decision are docs only.
- **Active role and requested/configured/resolved model:** NONE; planner/reviewer requested Astra max, implementer Astra xhigh; pins unchanged; backend UNVERIFIED.
- **Last known commit / push:** donor f9e0fc3e1c9823e9b50f2e8184aecefd7590721e pushed; closure docs next.
- **Next action:** parent imports reviewed canonical native-gesture-output-proof documentation, then G05. Future execution needs documented secondary drag, identified current-source binary, isolated profile/fixture and actual clipboard-image observation/restoration.
- **Blocker and scope:** documented native drag API lacks secondary-button selection; current-source app/output proof unexecuted.

| Milestone | State | Real artifact, source commit, verification and owner | Next exact action |
|---|---|---|---|
| `M0-A` | NATIVE_PROOF_DEFERRED | Source/capability inventory documented only | Reviewed docs only |
| `M1-A` | NATIVE_PROOF_DEFERRED | Planner complete native pixel protocol; NOT_EXECUTED | Preserve future pair |
| `M1-B` | NATIVE_PROOF_DEFERRED | No reproduced defect; no correction made | Correct only a future demonstrated defect |
| `M2-A` | NATIVE_PROOF_DEFERRED | Cargo/unit/native/output all NOT_RUN | Await exact capability/provenance/output gates |
| `M3-A` | NATIVE_PROOF_DEFERRED | Documentation-only decision; no UNIT_VERIFIED claim | Reviewed canonical docs import then G05 |

### Append-only execution events

- Planning package created 2026-10-29; no goal code, test, branch, PR or merge was executed by generating these documents.

- Runtime2026-10-30: G03 dependency decision closed before G04. Native skill/runtime inspected; import/list_apps succeed, installed Multi Launcher not running/no windows. Documented DragInput lacks button selection; right-click does not prove held secondary crop. No launch/input/output/clipboard/native gesture attempted, no source candidate imported. Source/protocol planner next.

- 2026-10-30 01:30:34 UTC: planner source/protocol packet persisted and docs-only disposition recorded in [M2-A stall](../../../execution/multi-launcher-reliability-closeout-20261029-20261029-r01/goals/G04/stalls/M2-A-20261030T013034Z.md). Primary Redact/secondary crop, markup-only history, shared layer-then-crop Save/Copy and Solid semantics are SOURCE_OBSERVED only. Native secondary drag, current-source binary identity and actual clipboard-image proof remain unestablished. No source defect inferred; all current tests/native/output NOT_RUN. Planner pair covers known gap; NO_ACTION distinct findings, no duplicate follow-on.

- Closure2026-10-30: independent reviewer accepts documentation-only atf9e0fc3e; no substantive finding. Native proof remains deferred, Cargo/unit/gesture/output NOT_RUN. Canonical complete planner future pair preserved. Parent imports safe docs and proceeds G05; no historical source candidate imported.
