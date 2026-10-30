# G06 GOAL — G11 Jump Mode: Finish Customization UI and Focus Regression Coverage

> **Program:** `multi-launcher-reliability-closeout-20261029`  
> **Disposition at plan creation:** `NOT_STARTED / NOT_EXECUTED`  
> **Priority:** P2 / daily usability  
> **Lane:** bounded UI/runtime completion  
> **Prerequisite:** None  
> **Companion:** [KICKSTART.md](KICKSTART.md)  
> **Historical planning-time master:** `56223fdc123396f4c41c48e0542506d912c6c634` (October 29, 2026; verify current upstream at runtime; **never reset to this SHA**).

## 1. Goal and user-visible benefit

Complete the existing Jump Mode preference editor and reliable focus behavior while preserving the validated configurable chord/hint alphabet and current ownership/Enter activation semantics.

**Scope of user authorization when this package is launched:** execute only the milestones and bounded continuation rules in this Goal; this isn't permission to import unaccepted old queue branches, broaden into refactors, run a full test campaign, or merge into `master` automatically. This is a new action program, not the prior Astra research-only program.

## 2. Source-grounded initial observations

- Current source implements typed/validated local Jump chord and hint alphabet preferences, including Shift+Space defaults and invalid-value fallback.
- The earlier queue did not accept its complete Settings editor integration and native focus/shortcut behavior.
- Separate navigation ownership already protects modal/viewports and Jump selection from immediate execution; replacing it with a new focus framework is not justified.

The above is a **planning-time source observation**, not proof of runtime behavior or an assertion that all functions remain identical after this program starts. The worker must re-read the named owners at its actual task base and classify any drift before editing. Important links are pinned to the merged master for provenance:

- [`src/gui/jump_mode/config.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/jump_mode/config.rs) — `JumpModeChord`, `HintAlphabet`, validation/default fallback
- [`src/gui/jump_mode/ui.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/jump_mode/ui.rs) — semantic target and viewport/navigation ownership
- [`src/gui/render.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/render.rs) — query root and Jump integration
- [`src/settings/model.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/settings/model.rs) — persisted Jump preferences
- [`src/settings_editor.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/settings_editor.rs) — existing Settings editor entry point
- [PR #1455](https://github.com/multiplex55/Multi_Launcher/pull/1455) — G11 M1-A/M1-B prefs/controller accepted; editor/native closure deferred

## 3. Exact ownership, permitted changes and exclusions

- `src/gui/jump_mode/config.rs` and the existing Jump mode controller only if the actual preference application path needs a direct correction.
- Locate the current Settings editor section and data save/restore owner via `src/settings_editor.rs`, `src/settings_editor/`, `src/settings/`, `src/gui/theme_settings_dialog.rs` as appropriate; do not invent an editor path.
- Relevant `src/gui/render.rs`/query focus guards and directly impacted existing keyboard/Jump tests; no broad hotkey global hook rewrite.

**Non-goals / changes to refuse:**

- Do not change Jump leader to a global OS hook or make it activate while the launcher is hidden.
- No comprehensive UX redesign or new target families in this milestone.

New files may be created under the goal-specific execution directory `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G06/` and in tightly scoped production owners above only. Do not edit `.codex` role/configuration files, historical research reports, unrelated worktrees, vendored crates, snapshots or independent user content. A previously uncommitted or blocked candidate is **evidence only**, never an implementation source to cherry-pick wholesale.

## 4. Required behavior and preserved invariants

- Default Shift+Space behavior stays the same for users who have never configured a custom chord.
- Existing launcher hotkey always independently focuses the query when appropriate, even with Jump enabled; avoid suppressing search input when notes/dialogs close.
- Jump hint full sequence focuses/selects; Enter performs action; Esc exits; competing input owners and native focus fencing remain authoritative.
- No settings schema change without backward-compatible fallback/migration proof.

If a required invariant has a concrete counterexample that the stated milestone cannot fix without substantial adjacent work, **stop that checkpoint**, preserve its evidence and author an explicit follow-on Goal/Kickstart. Continue the next independent program goal. An accepted subset may be integrated only if default behavior is safe, the subset is independent and the acceptance scope is truthful.

## 5. Milestones and bounded handoffs

| Checkpoint | Owner and outcome | Acceptance / intended commit |
|---|---|---|
| `M0-A` | Source and UX contract | Scoped work + tangible artifact; `docs(closeout): [G06/M0-A] ...` or `fix/feat` when source changed |
| `M1-A` | Editor and validation | Scoped work + tangible artifact; `docs(closeout): [G06/M1-A] ...` or `fix/feat` when source changed |
| `M1-B` | Commit and live application | Scoped work + tangible artifact; `docs(closeout): [G06/M1-B] ...` or `fix/feat` when source changed |
| `M2-A` | Focused navigation tests | Scoped work + tangible artifact; `docs(closeout): [G06/M2-A] ...` or `fix/feat` when source changed |
| `M3-A` | Close with precise UI evidence | Scoped work + tangible artifact; `docs(closeout): [G06/M3-A] ...` or `fix/feat` when source changed |

### M0-A — Source and UX contract

**Objective and detailed handoff.** Trace initial activation chord, hint alphabet allocation, Settings draft/live committed fields, loaded invalid values, hotkey precedence and already-open dialog focus. Record exactly which UI controls are absent versus already available.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-A — Editor and validation

**Objective and detailed handoff.** Add/choreograph Settings inputs for activation chord and allowed hint alphabet; expose clear inline validation, defaults/restore and stable preview of effective hints before saving. Reject conflicts/reserved keys and duplicate/unusable hint alphabets rather than silently committing invalid data.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-B — Commit and live application

**Objective and detailed handoff.** Use existing Settings transaction and acknowledged live application; preserve original pending draft on validation failure; update Jump controller only on successful commit, invalidate stale hint session/viewport focus once and retain captured key-release fences.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M2-A — Focused navigation tests

**Objective and detailed handoff.** Test initial launcher query focus after hotkey; Enter executes only after full hint selection; Esc cancellation; Shift+Space when typing; edited chord and hint alphabet; already-open Note/Settings/dialog; foreign foreground ownership; fallback from malformed saved setting; disabled Jump path. Use existing egui test seams/owned native proof only if changed.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M3-A — Close with precise UI evidence

**Objective and detailed handoff.** Capture screenshot(s) of actual Settings/editor and Jump hints using supported UI tooling when available, and record changed-file/test proof. If native focus cannot be exercised, label the remaining acceptance item rather than recasting it as unit-PASS.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### Reviewer handoff rule

For each completed production milestone, the parent must provide a reviewer: actual diff from the correct base, original objective, named callers and existing defenses, new tests and selection inventory, first meaningful failure if any, exact run/receipt, stop/accept criteria and non-goals. The reviewer should return **severity-ranked substantive findings only** and identify stale callers, duplicate state, false positive test passes, unsafe activation, migrations or ownership violations. One substantive correction pass is allowed before a recorded stop/spike; do not loop indefinitely.

## 6. Focused acceptance / negative regression tests

- Chord parse/display/roundtrip, conflict and reserved-key errors, invalid saved preference fallback.
- Hint alphabet uniqueness/length, stable deterministic target allocations and Jump session invalidation.
- Focused root query/Note/Settings modal input tests with nonzero selected test cases; native foreground context only if tool can prove it.
- Screenshot UI verification using first-party harness if available; no manual interruption until the program final review.

Test execution is **not automatic simply because it is listed here**; first identify the actual Cargo package, present test names and test-target support. Select a nonzero meaningful subset. Prefer `cargo nextest list` and `cargo nextest run` scoped to named modules or exact test cases; if a test command selects zero, that is not PASS. Default allowance for one changed source checkpoint is one selected preparation, one selected run and at most two correction reruns **only when required**. The program-wide budgets in [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md) control timings, native proof and documentation-only checks. Never weaken tests, delete failures or claim native acceptance from pure raster/source tests.

## 7. Decision gates: block only unsafe scope, not the whole queue

- If editor ownership is shared with another in-flight task, wait for previous writer commit or use existing migrated source; no parallel UI edits.
- If native focus proof is unsupported, preserve a `NATIVE_PENDING` item and complete only the demonstrably verified logic/UI subset.

Classify each checkpoint as `PENDING`, `IN_PROGRESS`, `ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `DEFERRED_DEPENDENCY`, `DEFERRED_WITH_DESIGN`, `NATIVE_PROOF_DEFERRED`, `FAILED_VERIFICATION`, or `NO_CHANGE_REVALIDATED`. The label must match the documented facts; a `PARTIAL_ACCEPTED` status is not feature-wide completion. On a true stop, create a durable per-goal `stalls/<checkpoint>-UTC.md` with actual source/ref, blocker, attempted one-shot fallback, unrun commands, ownership/cleanup, accepted files and exact next step; then return control to the parent to schedule the next independent goal.

## 8. Branch, commit, push, integration and model rules

1. Parent verifies actual `origin/master` and the program's accepted integration ancestry. Each **new** Goal receives its own branch `codex/closeout-g06-jump-settings-and-focus-<RUN_ID>` from the pinned program-start branch (contains only this plan) unless an explicitly accepted predecessor must be present. Dependent goals start from the prior **accepted integration commit** and record this exception. Never base on a rejected/dirty historical implementation tip.
2. Keep one active writer per goal; parent owns Git and the authoritative program ledger. Create the goal's execution directory and a `M0-A` docs baseline commit before editing production source. Accepted milestones each get meaningful Conventional Commit-style subjects including `G06/<milestone>` and one normal non-force push after commit. If push fails, preserve local commits and proceed; remote success must be recorded truthfully.
3. Parent integrates only reviewed, acceptance-gated production/documentation checkpoints into `codex/closeout-integration-<RUN_ID>` in dependency order (normally `git cherry-pick -x`). Do not import failed/unaccepted source. A rejected goal may contribute its **documentation-only** stop report/complete future pair, not an unsafe patch.
4. Role defaults verified from current `.codex`: orchestrator `gpt-6-astra/xhigh`; planner/reviewer `gpt-6-astra/max`; implementer `gpt-6-astra/xhigh`. These are configured role preferences, not proof of actual backend. If supported and selected for a narrow bounded implementation, `gpt-6.1-sol/xhigh` may be used deliberately, with honest attribution. No silent `.codex` configuration edits.
5. Roles contribute `roles/planner/`, `roles/implementer/`, `roles/reviewer/` and `roles/orchestrator/` Markdown under this goal's execution directory. New actionable recommendations get complete role-authored `roles/<role>/follow-on/<id>/GOAL.md` and `KICKSTART.md`; parent deduplicates into a canonical `follow-on/<id>/` library. Do not fabricate findings to fill quotas.
6. Do not ask the user to manually test between milestones; use supported app/native automation if available. If unavailable, log the gap and reserve a **single final manual-review checklist** for the program handoff.

## 9. Completed outcome definition

Settings UI, committed-state behavior and focused keyboard/viewport regressions are supported; any unavailable native behavior is explicitly deferred with reproducible evidence requirements.

**Required closure record:** real branch/base, all milestone dispositions, exact changed files, accepted and rejected interfaces, focused test counts and command receipts, actual native proof or `NOT_RUN`, model/role provenance, reviewer disposition, commits/pushes, output/follow-on paths, source safety constraints, and exact next action. No PR merge/CI success may be claimed without host-confirmed evidence.

## 10. Execution progress — mutable (Codex updates here)

- **Current state:** `NOT_STARTED`.
- **Run ID and branch:** assigned by the orchestrator after safe source verification.
- **Actual master/source base:** `UNVERIFIED_FOR_EXECUTION`.
- **Current milestone:** `M0-A` pending.
- **Active role and requested/configured/resolved model:** `NONE / unverified`.
- **Last known commit / push:** none for this package.
- **Next action:** read [KICKSTART](KICKSTART.md), confirm prerequisite and revalidate source; then begin on a fresh task branch.
- **Blocker and scope:** none reported.

| Milestone | State | Real artifact, source commit, verification and owner | Next exact action |
|---|---|---|---|
| `M0-A` | PENDING | — | Source and UX contract |
| `M1-A` | PENDING | — | Editor and validation |
| `M1-B` | PENDING | — | Commit and live application |
| `M2-A` | PENDING | — | Focused navigation tests |
| `M3-A` | PENDING | — | Close with precise UI evidence |

### Append-only execution events

- Planning package created 2026-10-29; no goal code, test, branch, PR or merge was executed by generating these documents.
- Runtime2026-10-30: M0-A IN_PROGRESS; branch codex/closeout-g06-jump-settings-and-focus-20261029-r01 from bootstrap02d0ac64; fetched actual master4a1283e6 unchanged; integrationcda36e81 authoritative. Read-only planner next; no source/tests/native yet.
