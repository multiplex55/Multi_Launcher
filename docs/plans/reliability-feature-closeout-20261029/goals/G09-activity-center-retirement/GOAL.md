# G09 GOAL — G06 Activity Center: Source-Grounded Native Retirement Acceptance

> **Program:** `multi-launcher-reliability-closeout-20261029`  
> **Disposition at plan creation:** `NOT_STARTED / NOT_EXECUTED`  
> **Priority:** P2 / reliability  
> **Lane:** targeted proof and narrow correction only  
> **Prerequisite:** Native proof when owner retirement is claimed; do not infer from terminal UI  
> **Companion:** [KICKSTART.md](KICKSTART.md)  
> **Historical planning-time master:** `56223fdc123396f4c41c48e0542506d912c6c634` (October 29, 2026; verify current upstream at runtime; **never reset to this SHA**).

## 1. Goal and user-visible benefit

Resolve the remaining G06 M2-B native owner-retirement acceptance gap for the activity view and its selected background owners, while keeping owner-specific cancellation/retirement contracts and existing low-overhead behavior.

**Scope of user authorization when this package is launched:** execute only the milestones and bounded continuation rules in this Goal; this isn't permission to import unaccepted old queue branches, broaden into refactors, run a full test campaign, or merge into `master` automatically. This is a new action program, not the prior Astra research-only program.

## 2. Source-grounded initial observations

- `ActivityCenter` explicitly differentiates operation lifecycle from worker retirement; an Activity row saying Cancelled or Failed is not proof the worker thread has joined.
- The Activity dialog is already visible-only, throttled, and supports filter, navigation, cancellation and sanitized diagnostics; a new generic Activity framework would duplicate working ownership.
- The former queue accepted the selected worker/apartment ownership improvement and Activity view but did not establish all planned M2-B native retirement semantics.

The above is a **planning-time source observation**, not proof of runtime behavior or an assertion that all functions remain identical after this program starts. The worker must re-read the named owners at its actual task base and classify any drift before editing. Important links are pinned to the merged master for provenance:

- [`src/activity.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/activity.rs) — `ActivityLifecycle`, `ActivityRetirement`, `ActivityCenter`, resource facts
- [`src/gui/activity_dialog.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/activity_dialog.rs) — visible-only Activity UI, filter, cancel, copy sanitized diagnostics
- [`src/thread_reaper.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/thread_reaper.rs) — actual join/reap permits
- [`src/gui/screenshot_capture.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/screenshot_capture.rs) — screenshot worker cancellation/reaper
- [`src/duplicate_finder/session.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/duplicate_finder/session.rs) — owner-specific join/retirement if current path exists
- [PR #1455](https://github.com/multiplex55/Multi_Launcher/pull/1455) — G06 M1/M2-A accepted, G06 M2-B not accepted

## 3. Exact ownership, permitted changes and exclusions

- `src/activity.rs`, `src/gui/activity_dialog.rs`, `src/thread_reaper.rs` only where actual incorrect receipt/state observed.
- Two representative owner classes (one retained worker, one detached/cancellable owner) selected from current code; inspect exact actual handlers/tests before changing.
- Existing Activity, Duplicate Finder, File Search, pinboard/screenshot and root lifecycle fixtures as relevant to the chosen owner, not entire app.

**Non-goals / changes to refuse:**

- No universal worker manager, high-frequency periodic sampling or full Windows native acceptance campaign.
- No relabeling Activity cancellation as resource retirement.

New files may be created under the goal-specific execution directory `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G09/` and in tightly scoped production owners above only. Do not edit `.codex` role/configuration files, historical research reports, unrelated worktrees, vendored crates, snapshots or independent user content. A previously uncommitted or blocked candidate is **evidence only**, never an implementation source to cherry-pick wholesale.

## 4. Required behavior and preserved invariants

- Activity terminal status is not proof of joined/reaped thread or released native handle.
- Existing owner-specific retirement fences, reaper permits, cancel tokens, and stale-result rejection remain intact.
- Visible-only Activity sampling and sanitized metadata do not become hidden-window polling or private log export.
- Do not regress File Search ripgrep child kill/wait or Duplicate Finder reaper-held admission by changing generic Activity APIs casually.

If a required invariant has a concrete counterexample that the stated milestone cannot fix without substantial adjacent work, **stop that checkpoint**, preserve its evidence and author an explicit follow-on Goal/Kickstart. Continue the next independent program goal. An accepted subset may be integrated only if default behavior is safe, the subset is independent and the acceptance scope is truthful.

## 5. Milestones and bounded handoffs

| Checkpoint | Owner and outcome | Acceptance / intended commit |
|---|---|---|
| `M0-A` | Choose and document exact pending acceptance scenario | Scoped work + tangible artifact; `docs(closeout): [G09/M0-A] ...` or `fix/feat` when source changed |
| `M1-A` | Deterministic owner proof first | Scoped work + tangible artifact; `docs(closeout): [G09/M1-A] ...` or `fix/feat` when source changed |
| `M1-B` | Minimal correction when demonstrated | Scoped work + tangible artifact; `docs(closeout): [G09/M1-B] ...` or `fix/feat` when source changed |
| `M2-A` | Native specificity if needed | Scoped work + tangible artifact; `docs(closeout): [G09/M2-A] ...` or `fix/feat` when source changed |
| `M3-A` | Close with owner matrix | Scoped work + tangible artifact; `docs(closeout): [G09/M3-A] ...` or `fix/feat` when source changed |

### M0-A — Choose and document exact pending acceptance scenario

**Objective and detailed handoff.** Read PR #1455 and current owner lifecycle code; select one actual missing M2-B behavior that can be demonstrated (e.g. rapid Cancel/Reopen, stale completion post-close, join receipt). Distinguish retired, unavailable, and invalidated rows and root/window visibility.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-A — Deterministic owner proof first

**Objective and detailed handoff.** Use blocked-worker and reaper probes with owned synchronization; show exact admission/terminal/retirement sequence and one cancellation handoff, without sleeps or polling-only assumptions. If existing contract passes, record NO_CHANGE and omit code.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-B — Minimal correction when demonstrated

**Objective and detailed handoff.** If source and fixture identify a real violation (stale reopen, unmatched join receipt, wrong visible status), fix the first current owner boundary only. Do not install another scheduler or change global input/focus ownership.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M2-A — Native specificity if needed

**Objective and detailed handoff.** For a claim involving Windows apartment/handle/capture cleanup, require a bounded supported native scenario that actually exercises that OS API. If no supported instrument/input is available, record `NATIVE_NOT_PROVEN` and do not claim feature-wide native completion.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M3-A — Close with owner matrix

**Objective and detailed handoff.** Publish actual measured/proven status by owner, which UI rows are informational only, exact selected tests and any deferred targeted native proof Goal/Kickstart. No broad rebuild.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### Reviewer handoff rule

For each completed production milestone, the parent must provide a reviewer: actual diff from the correct base, original objective, named callers and existing defenses, new tests and selection inventory, first meaningful failure if any, exact run/receipt, stop/accept criteria and non-goals. The reviewer should return **severity-ranked substantive findings only** and identify stale callers, duplicate state, false positive test passes, unsafe activation, migrations or ownership violations. One substantive correction pass is allowed before a recorded stop/spike; do not loop indefinitely.

## 6. Focused acceptance / negative regression tests

- Deterministic owner cancellation→terminal→join/reap callback with one blocked worker and rapid reopen attempts.
- Retired/unavailable row labels, omission limits, sanitized copy and no hidden dialog refresh.
- One exact native apartment/handle case if a native claim is at issue; otherwise explicit NOT_RUN with portable protocol.

Test execution is **not automatic simply because it is listed here**; first identify the actual Cargo package, present test names and test-target support. Select a nonzero meaningful subset. Prefer `cargo nextest list` and `cargo nextest run` scoped to named modules or exact test cases; if a test command selects zero, that is not PASS. Default allowance for one changed source checkpoint is one selected preparation, one selected run and at most two correction reruns **only when required**. The program-wide budgets in [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md) control timings, native proof and documentation-only checks. Never weaken tests, delete failures or claim native acceptance from pure raster/source tests.

## 7. Decision gates: block only unsafe scope, not the whole queue

- If a required native proof cannot be run, preserve tested source status and classify M2-B `NATIVE_DEFERRED`, leaving no new native guarantee.
- Stop feature broadening after at most one selected owner correction; route distinct owner bugs into new follow-on pairs.

Classify each checkpoint as `PENDING`, `IN_PROGRESS`, `ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `DEFERRED_DEPENDENCY`, `DEFERRED_WITH_DESIGN`, `NATIVE_PROOF_DEFERRED`, `FAILED_VERIFICATION`, or `NO_CHANGE_REVALIDATED`. The label must match the documented facts; a `PARTIAL_ACCEPTED` status is not feature-wide completion. On a true stop, create a durable per-goal `stalls/<checkpoint>-UTC.md` with actual source/ref, blocker, attempted one-shot fallback, unrun commands, ownership/cleanup, accepted files and exact next step; then return control to the parent to schedule the next independent goal.

## 8. Branch, commit, push, integration and model rules

1. Parent verifies actual `origin/master` and the program's accepted integration ancestry. Each **new** Goal receives its own branch `codex/closeout-g09-activity-center-retirement-<RUN_ID>` from the pinned program-start branch (contains only this plan) unless an explicitly accepted predecessor must be present. Dependent goals start from the prior **accepted integration commit** and record this exception. Never base on a rejected/dirty historical implementation tip.
2. Keep one active writer per goal; parent owns Git and the authoritative program ledger. Create the goal's execution directory and a `M0-A` docs baseline commit before editing production source. Accepted milestones each get meaningful Conventional Commit-style subjects including `G09/<milestone>` and one normal non-force push after commit. If push fails, preserve local commits and proceed; remote success must be recorded truthfully.
3. Parent integrates only reviewed, acceptance-gated production/documentation checkpoints into `codex/closeout-integration-<RUN_ID>` in dependency order (normally `git cherry-pick -x`). Do not import failed/unaccepted source. A rejected goal may contribute its **documentation-only** stop report/complete future pair, not an unsafe patch.
4. Role defaults verified from current `.codex`: orchestrator `gpt-6-astra/xhigh`; planner/reviewer `gpt-6-astra/max`; implementer `gpt-6-astra/xhigh`. These are configured role preferences, not proof of actual backend. If supported and selected for a narrow bounded implementation, `gpt-6.1-sol/xhigh` may be used deliberately, with honest attribution. No silent `.codex` configuration edits.
5. Roles contribute `roles/planner/`, `roles/implementer/`, `roles/reviewer/` and `roles/orchestrator/` Markdown under this goal's execution directory. New actionable recommendations get complete role-authored `roles/<role>/follow-on/<id>/GOAL.md` and `KICKSTART.md`; parent deduplicates into a canonical `follow-on/<id>/` library. Do not fabricate findings to fill quotas.
6. Do not ask the user to manually test between milestones; use supported app/native automation if available. If unavailable, log the gap and reserve a **single final manual-review checklist** for the program handoff.

## 9. Completed outcome definition

Selected G06 owner lifetime behavior has targeted meaningful receipts; no unsupported global retirement guarantee is declared. Any remaining native gap has a precise independent test plan.

**Required closure record:** real branch/base, all milestone dispositions, exact changed files, accepted and rejected interfaces, focused test counts and command receipts, actual native proof or `NOT_RUN`, model/role provenance, reviewer disposition, commits/pushes, output/follow-on paths, source safety constraints, and exact next action. No PR merge/CI success may be claimed without host-confirmed evidence.

## 10. Execution progress — mutable (Codex updates here)

- **Current state:** `IN_PROGRESS` — M1 proof checkpoint ready for independent review; no acceptance claim.
- **Run ID and branch:** `20261029-r01`, `codex/closeout-g09-activity-center-retirement-20261029-r01`.
- **Actual master/source base:** master `4a1283e6edd2c245f562a07dbe3dcc6a61061eed`; independent bootstrap `02d0ac64d51655f43d6e958ea551f5bf658ee48d`; authoritative integration `9b58d06ac1199a9864d3c7bf79d5e7aa080bb060` remains separate.
- **Current milestone:** `M1-A`, exact two-owner proof and one native pre-read handle case executed.
- **Active role and requested/configured/resolved model:** Implementer; requested Astra/xhigh, configured Luna/max unchanged, actual backend UNVERIFIED.
- **Last known commit / push:** parent committed/pushed contract `691645e357d94f240a544996f5a58b532955a345`; current test checkpoint uncommitted by Implementer.
- **Next action:** parent checkpoints exact test/evidence diff and requests independent review, then owns disposition/integration.
- **Blocker and scope:** no reproduced production defect. Test-only strengthening; all 12 selected tests pass. No global native retirement guarantee.

| Milestone | State | Real artifact, source commit, verification and owner | Next exact action |
|---|---|---|---|
| `M0-A` | Complete contract | Exact retained Duplicate Finder and cancellable Screenshot owner contract at `691645e3` | Preserve contract |
| `M1-A` | IN_PROGRESS | Implementer test checkpoint, 12/12 pass, production unchanged | Independent review |
| `M1-B` | NO_CHANGE_REVALIDATED evidence | No production correction warranted by selected proof | Parent disposition after review |
| `M2-A` | IN_PROGRESS | Actual native source-handle baseline/OS32/post-join/warm restart PASS; other native categories NOT_RUN | Review exact native claim |
| `M3-A` | IN_PROGRESS | Owner matrix and cleanup in execution VERIFICATION | Parent canonical closure |

### Append-only execution events

- Planning package created 2026-10-29; no goal code, test, branch, PR or merge was executed by generating these documents.

- Runtime2026-10-30 M0-A IN_PROGRESS: independent codex/closeout-g09-activity-center-retirement-20261029-r01 frombootstrap02d; unchanged fetchedmaster4a, authoritativeintegration9b58 and acceptedsourceb9 separate. Read-onlyowner/nativeproof planner next; no source/tests/native.

- 2026-10-30 M1 Implementer: strengthened existing test modules only. Exact serial library run passed 12/12, exit0, 3.648s wrapper/0.732s tests after 211.037s preparation. Native pre-read handle fixture passed and was removed; no owned build/test processes remain. Preparation numeric exit null is preserved; no test/compile failures or correction reruns. Production unchanged; checkpoint awaits parent commit and independent review. Details: [G09 verification](../../../execution/multi-launcher-reliability-closeout-20261029-20261029-r01/goals/G09/VERIFICATION.md).
