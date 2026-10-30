# G08 GOAL — G15 MkMacro: Bounded Run History and Conditional Safe Recipes

> **Program:** `multi-launcher-reliability-closeout-20261029`  
> **Disposition at plan creation:** `NOT_STARTED / NOT_EXECUTED`  
> **Priority:** P2 / productivity  
> **Lane:** UI-first; optional bounded recipe milestone  
> **Prerequisite:** Recipes require accepted history and explicit policy; do not auto-run after mere display success  
> **Companion:** [KICKSTART.md](KICKSTART.md)  
> **Historical planning-time master:** `56223fdc123396f4c41c48e0542506d912c6c634` (October 29, 2026; verify current upstream at runtime; **never reset to this SHA**).

## 1. Goal and user-visible benefit

Make the already captured metadata-only MkMacro terminal summaries useful through a bounded, privacy-preserving history UI, and decide whether a small safe recipe workflow can be supported without bypassing existing macro execution authorization.

**Scope of user authorization when this package is launched:** execute only the milestones and bounded continuation rules in this Goal; this isn't permission to import unaccepted old queue branches, broaden into refactors, run a full test campaign, or merge into `master` automatically. This is a new action program, not the prior Astra research-only program.

## 2. Source-grounded initial observations

- `SummaryHistory` retains bounded metadata-only records, including run IDs, macro names, origin/mode/subset, elapsed/paused time, visits and sanitized failure categories. Its existing no-Clone retention invariant prevents under-accounted reader copies.
- No user-visible run history or recipe UX was accepted in the prior queue. Adding a new persistence store, raw clipboard text or verbose macro action traces would undermine the accepted privacy/retention design.
- Runs can be entered by hotkeys, Radial, Hot Folder and external sources; read-only history must not misattribute invocation source or relaunch without deliberate user action.

The above is a **planning-time source observation**, not proof of runtime behavior or an assertion that all functions remain identical after this program starts. The worker must re-read the named owners at its actual task base and classify any drift before editing. Important links are pinned to the merged master for provenance:

- [`src/mkmacro/runtime/summary.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/mkmacro/runtime/summary.rs) — `MacroRunSummary`, `SummaryHistory`, safe failure categories and bounded capacity
- [`src/mkmacro/runtime.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/mkmacro/runtime.rs) — runtime entry points and terminal summary publishing
- [`src/gui/mkmacro_dialog/mod.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/mkmacro_dialog/mod.rs) — existing macro UI if this path still owns the dialog
- [PR #1455](https://github.com/multiplex55/Multi_Launcher/pull/1455) — G15 M1-A summary metadata only; history UI/recipes missing

## 3. Exact ownership, permitted changes and exclusions

- `src/mkmacro/runtime/summary.rs` read-only snapshot/borrowed model under established history lock; existing summary accounting stays unchanged.
- Locate current `src/gui/mkmacro*` dialog/editor view before edits; add read-only History tab/section and bounded detail expansion.
- `src/mkmacro/invocation.rs`, `runtime.rs`, `store.rs` only for a separately approved recipe action reusing existing validation, no direct new executor.

**Non-goals / changes to refuse:**

- No automation “AI-generated” macros, cloud history, replay from raw events, or new persistent sensitive logging.
- No MkMacro schema migration or recorder redesign solely to support history.

New files may be created under the goal-specific execution directory `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G08/` and in tightly scoped production owners above only. Do not edit `.codex` role/configuration files, historical research reports, unrelated worktrees, vendored crates, snapshots or independent user content. A previously uncommitted or blocked candidate is **evidence only**, never an implementation source to cherry-pick wholesale.

## 4. Required behavior and preserved invariants

- No raw keystrokes, clipboard contents, OCR results, pixel buffers, file contents, secret variables or arbitrary error strings in run-history UI/diagnostics.
- History reading must not escape existing max count/byte/step-row charges, retain evicted records or hold the lock through egui rendering.
- Run/Stop/Pause, nested calls and original invoker attribution remain exactly as before.
- A displayed record never acts as an authorization to auto-rerun unsafe actions; recipes reuse existing validation and require explicit intent.

If a required invariant has a concrete counterexample that the stated milestone cannot fix without substantial adjacent work, **stop that checkpoint**, preserve its evidence and author an explicit follow-on Goal/Kickstart. Continue the next independent program goal. An accepted subset may be integrated only if default behavior is safe, the subset is independent and the acceptance scope is truthful.

## 5. Milestones and bounded handoffs

| Checkpoint | Owner and outcome | Acceptance / intended commit |
|---|---|---|
| `M0-A` | History owner and privacy contract | Scoped work + tangible artifact; `docs(closeout): [G08/M0-A] ...` or `fix/feat` when source changed |
| `M1-A` | User-visible history | Scoped work + tangible artifact; `docs(closeout): [G08/M1-A] ...` or `fix/feat` when source changed |
| `M1-B` | History ownership/lifecycle tests | Scoped work + tangible artifact; `docs(closeout): [G08/M1-B] ...` or `fix/feat` when source changed |
| `M2-A` | Recipe scope decision then optional implementation | Scoped work + tangible artifact; `docs(closeout): [G08/M2-A] ...` or `fix/feat` when source changed |
| `M3-A` | Focused review and closure | Scoped work + tangible artifact; `docs(closeout): [G08/M3-A] ...` or `fix/feat` when source changed |

### M0-A — History owner and privacy contract

**Objective and detailed handoff.** Inspect current summary history API/retention, actual UI owner, lifecycle and safe metadata fields. Specify an on-demand, bounded read view with a stable run key and no unbounded Clone/long-held lock; clarify whether history is in-memory only.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-A — User-visible history

**Objective and detailed handoff.** Add a History view with most recent completed run, outcome, timing, source, safe failure label and optional per-step aggregate details. Include bounded filter/sort/clear as already supported, no live raw log or secret payload. Lazy refresh only when dialog visible, preserve UI responsiveness and no idle polling.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-B — History ownership/lifecycle tests

**Objective and detailed handoff.** Test completed-run retention/eviction, byte quotas, zero/error/busy/paused outcomes, malformed safe labels, history view reopen/clear, missing macro name and concurrent publication snapshot validity without blocking long-running worker. Preserve metadata-only contract.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M2-A — Recipe scope decision then optional implementation

**Objective and detailed handoff.** Define what a recipe means from actual existing macro facilities: perhaps named saved launch invocation/profile, never replaying the last run from side effects. If an existing validated, permission-aware authoring/launch seam supports it with small migration, expose **one** explicit user-authored recipe with confirmation and no auto-trigger. Otherwise produce `RECIPES_DECISION.md` and a complete future Goal/Kickstart; do not manufacture execution shortcuts.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M3-A — Focused review and closure

**Objective and detailed handoff.** Review privacy, per-run source accuracy, bounded RAM/UI lifetime and user-activation authority; commit/push history independent of uncertain recipe work. Report exactly whether recipe subset shipped, deferred, or rejected.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### Reviewer handoff rule

For each completed production milestone, the parent must provide a reviewer: actual diff from the correct base, original objective, named callers and existing defenses, new tests and selection inventory, first meaningful failure if any, exact run/receipt, stop/accept criteria and non-goals. The reviewer should return **severity-ranked substantive findings only** and identify stale callers, duplicate state, false positive test passes, unsafe activation, migrations or ownership violations. One substantive correction pass is allowed before a recorded stop/spike; do not loop indefinitely.

## 6. Focused acceptance / negative regression tests

- In-memory history bounded eviction, byte charge and ongoing run/terminal publication under concurrent view.
- UI no-secret snapshot, safe labels, empty state, filtering, selection and close/reopen; no continuous repaint when hidden.
- If recipe implemented, explicit permission/revalidate/disabled target and unsafe action refusal; no new macro runner.

Test execution is **not automatic simply because it is listed here**; first identify the actual Cargo package, present test names and test-target support. Select a nonzero meaningful subset. Prefer `cargo nextest list` and `cargo nextest run` scoped to named modules or exact test cases; if a test command selects zero, that is not PASS. Default allowance for one changed source checkpoint is one selected preparation, one selected run and at most two correction reruns **only when required**. The program-wide budgets in [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md) control timings, native proof and documentation-only checks. Never weaken tests, delete failures or claim native acceptance from pure raster/source tests.

## 7. Decision gates: block only unsafe scope, not the whole queue

- If read-only history cannot be exposed without violating summary borrowing/retention invariants, document a small safe snapshot contract and defer that UI; do not add a second global history.
- If recipe semantics or authorization remains ambiguous, close only completed history and create an explicitly gated future package.

Classify each checkpoint as `PENDING`, `IN_PROGRESS`, `ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `DEFERRED_DEPENDENCY`, `DEFERRED_WITH_DESIGN`, `NATIVE_PROOF_DEFERRED`, `FAILED_VERIFICATION`, or `NO_CHANGE_REVALIDATED`. The label must match the documented facts; a `PARTIAL_ACCEPTED` status is not feature-wide completion. On a true stop, create a durable per-goal `stalls/<checkpoint>-UTC.md` with actual source/ref, blocker, attempted one-shot fallback, unrun commands, ownership/cleanup, accepted files and exact next step; then return control to the parent to schedule the next independent goal.

## 8. Branch, commit, push, integration and model rules

1. Parent verifies actual `origin/master` and the program's accepted integration ancestry. Each **new** Goal receives its own branch `codex/closeout-g08-mkmacro-run-history-<RUN_ID>` from the pinned program-start branch (contains only this plan) unless an explicitly accepted predecessor must be present. Dependent goals start from the prior **accepted integration commit** and record this exception. Never base on a rejected/dirty historical implementation tip.
2. Keep one active writer per goal; parent owns Git and the authoritative program ledger. Create the goal's execution directory and a `M0-A` docs baseline commit before editing production source. Accepted milestones each get meaningful Conventional Commit-style subjects including `G08/<milestone>` and one normal non-force push after commit. If push fails, preserve local commits and proceed; remote success must be recorded truthfully.
3. Parent integrates only reviewed, acceptance-gated production/documentation checkpoints into `codex/closeout-integration-<RUN_ID>` in dependency order (normally `git cherry-pick -x`). Do not import failed/unaccepted source. A rejected goal may contribute its **documentation-only** stop report/complete future pair, not an unsafe patch.
4. Role defaults verified from current `.codex`: orchestrator `gpt-6-astra/xhigh`; planner/reviewer `gpt-6-astra/max`; implementer `gpt-6-astra/xhigh`. These are configured role preferences, not proof of actual backend. If supported and selected for a narrow bounded implementation, `gpt-6.1-sol/xhigh` may be used deliberately, with honest attribution. No silent `.codex` configuration edits.
5. Roles contribute `roles/planner/`, `roles/implementer/`, `roles/reviewer/` and `roles/orchestrator/` Markdown under this goal's execution directory. New actionable recommendations get complete role-authored `roles/<role>/follow-on/<id>/GOAL.md` and `KICKSTART.md`; parent deduplicates into a canonical `follow-on/<id>/` library. Do not fabricate findings to fill quotas.
6. Do not ask the user to manually test between milestones; use supported app/native automation if available. If unavailable, log the gap and reserve a **single final manual-review checklist** for the program handoff.

## 9. Completed outcome definition

A usable bounded history UI passes focused tests and separate privacy review; recipe behavior is precisely implemented within existing admission or explicitly deferred with a complete follow-on pair.

**Required closure record:** real branch/base, all milestone dispositions, exact changed files, accepted and rejected interfaces, focused test counts and command receipts, actual native proof or `NOT_RUN`, model/role provenance, reviewer disposition, commits/pushes, output/follow-on paths, source safety constraints, and exact next action. No PR merge/CI success may be claimed without host-confirmed evidence.

## 10. Execution progress — mutable (Codex updates here)

- **Current state:** `DEFERRED_WITH_DESIGN` — R1 correction is unaccepted; preparation timed out, tests NOT_RUN.
- **Run ID and branch:** `20261029-r01`, `codex/closeout-g08-mkmacro-run-history-20261029-r01`.
- **Actual master/source base:** master `4a1283e6`, independent bootstrap `02d0ac64`; contract HEAD `826e418b`; accepted integration `ca2f6acb` separate.
- **Current milestone:** `M1/R1` stopped at verification deadline; all G08 source unaccepted.
- **Active role and requested/configured/resolved model:** NONE; requested Astra roles; configured Sol/high planner/reviewer and Luna/max implementer unchanged; actual backend UNVERIFIED.
- **Last known commit / push:** corrected-source preservation `bb8c572026016d3042efce29e112f8199de41742`, normally pushed; all source UNACCEPTED.
- **Next action:** docs-only integration, then independent G09; future G08 restart requires current-source packet, new explicit finite verification authority, corrected-source tests and R1 confirmation.
- **Blocker and scope:** remaining 32-second preparation timed out; owned-tree termination was denied and process drained naturally. Late exit 0 is not qualifying proof. R1 tests NOT_RUN; recipe gate remains separate.

| Milestone | State | Real artifact, source commit, verification and owner | Next exact action |
|---|---|---|---|
| `M0-A` | COMPLETE | Contract `826e418b` | Implemented in combined checkpoint |
| `M1-A` | DEFERRED_WITH_DESIGN | Prototype and R1 correction unaccepted; old 14/14 only | Fresh authorized proof and R1 confirmation |
| `M1-B` | DEFERRED_WITH_DESIGN | R1 real-toolbar regression written, NOT_RUN | New explicit finite budget/current packet |
| `M2-A` | DEFERRED_WITH_DESIGN | Planner recipe decision and canonical complete pair; accepted history/policy absent | Conditional future continuation only |
| `M3-A` | COMPLETE_DOCS_ONLY | Independent R1 and documentation-only closure; source withheld | Parent docs integration; G09 next |

### Append-only execution events

- Planning package created 2026-10-29; no goal code, test, branch, PR or merge was executed by generating these documents.
- Runtime2026-10-30 M0-A IN_PROGRESS: independentbranch codex/closeout-g08-mkmacro-run-history-20261029-r01 frombootstrap02d0ac64, actualfetchedmaster4aunchanged/integrationca2f6acbauthoritative. Read-onlyboundedhistory/privacyplanner next; no code/tests/native. Recipesgatedseparately.

- 2026-10-30 implementer: combined M1 checkpoint tested 14/14 PASS on exact final blobs; initial E0382 and 13/14 diagnostics preserved with two fixture-only corrections. Aggregate incremental preparation 567.882/600s. No source acceptance/Git mutation; native/manual NOT_RUN; distinct new finding NO_ACTION. Parent review/closure and conditional recipe decision remain.

- 2026-10-30 R1 stop: single five-test preparation timed out at the remaining 32-second cap; taskkill OS Access denied, natural drainage101.880s/late exit0/timed_out=true. No R1 run or renewed budget. Source frozen/unaccepted; complete role-owned remaining-G08 restart pair references existing R1 and recipe packages. Parent checkpoint/docs-only disposition next.

- Parent2026-10-30: bb8c572026016d3042efce29e112f8199de41742 normallypushed; independentclosure supportsdocs-only, R1OPEN_VERIFICATION/allsourceUNACCEPTED. Canonical fullrestart/recipe pairs preserved; nextindependentG09. No native proof or late-test pass claimed.
