# G05 GOAL — G16 Hot Folder: Safe Size, Age and Literal-Content Conditions

> **Program:** `multi-launcher-reliability-closeout-20261029`  
> **Disposition at plan creation:** `NOT_STARTED / NOT_EXECUTED`  
> **Priority:** P1 / unattended automation  
> **Lane:** bounded implementation  
> **Prerequisite:** None, but fail-closed condition guarantee is mandatory  
> **Companion:** [KICKSTART.md](KICKSTART.md)  
> **Historical planning-time master:** `56223fdc123396f4c41c48e0542506d912c6c634` (October 29, 2026; verify current upstream at runtime; **never reset to this SHA**).

## 1. Goal and user-visible benefit

Turn the already persisted v2 optional Hot Folder conditions into a bounded and safe evaluator, visible editor controls and a side-effect-free Test Rule workflow; preserve the fail-closed behavior until all required evidence exists.

**Scope of user authorization when this package is launched:** execute only the milestones and bounded continuation rules in this Goal; this isn't permission to import unaccepted old queue branches, broaden into refactors, run a full test campaign, or merge into `master` automatically. This is a new action program, not the prior Astra research-only program.

## 2. Source-grounded initial observations

- Current master models `HotFolderConditions` with optional size-byte range, modified-age seconds range and literal content Contains/DoesNotContain, including a bounded literal length.
- Current G16 feature intentionally fails closed when optional conditions are configured but unimplemented. This is a safety guard, not a bug to bypass.
- Hot Folder handles unattended file mutations and external actions; eligibility, cancellation, settled-file evidence and rule consistency must be bound to the actual ready-candidate lease.

The above is a **planning-time source observation**, not proof of runtime behavior or an assertion that all functions remain identical after this program starts. The worker must re-read the named owners at its actual task base and classify any drift before editing. Important links are pinned to the merged master for provenance:

- [`src/hotfolder/model.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/hotfolder/model.rs) — `HOT_FOLDER_RULES_FORMAT_VERSION=2`, `HotFolderConditions`, `evaluate_metadata`, `FILE_CONDITIONS_NOT_EVALUATED`
- [`src/hotfolder/runtime.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/hotfolder/runtime.rs) — watcher ingress, settling, ready candidate leases
- [`src/hotfolder/execution.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/hotfolder/execution.rs) — `evaluate_rule_conditions`, action admission, receipts, loop control
- [`src/hotfolder/capability.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/hotfolder/capability.rs) — saved action identity/eligibility checks
- [`src/hotfolder/store.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/hotfolder/store.rs) — v1/v2 persistence
- [PR #1455](https://github.com/multiplex55/Multi_Launcher/pull/1455) — G16 M1-A model/fail-closed behavior only

## 3. Exact ownership, permitted changes and exclusions

- `src/hotfolder/model.rs` conditions/metadata predicate semantics (reuse existing types).
- `src/hotfolder/runtime.rs`, `execution.rs`, `capability.rs` for ready candidate evidence, staged content read and snapshot revalidation; `store.rs` only for compatibility-preserving v2 updates.
- Actual Hot Folder GUI rule editor/preview location, discovered before editing; avoid inventing a file path when owner is in GUI module hierarchy.

**Non-goals / changes to refuse:**

- No new arbitrary scripting grammar, regex content scanning, remote action authorization, external cloud automation, or broad watcher redesign.
- Do not change the existing macro/launcher action completion authority except a demonstrated necessary fence.

New files may be created under the goal-specific execution directory `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G05/` and in tightly scoped production owners above only. Do not edit `.codex` role/configuration files, historical research reports, unrelated worktrees, vendored crates, snapshots or independent user content. A previously uncommitted or blocked candidate is **evidence only**, never an implementation source to cherry-pick wholesale.

## 4. Required behavior and preserved invariants

- Existing rules without new conditions behave exactly as before, including v1 migration/defaults.
- Content DoesNotContain must never succeed on partial/unreadable/changed input.
- All conditions evaluate under the same rule revision, candidate identity, settled-file evidence and action authorization used by the effect; no UI preview can grant execution permission.
- No execution from Test Rule; no new watcher polling when no rules/conditions are enabled.

If a required invariant has a concrete counterexample that the stated milestone cannot fix without substantial adjacent work, **stop that checkpoint**, preserve its evidence and author an explicit follow-on Goal/Kickstart. Continue the next independent program goal. An accepted subset may be integrated only if default behavior is safe, the subset is independent and the acceptance scope is truthful.

## 5. Milestones and bounded handoffs

| Checkpoint | Owner and outcome | Acceptance / intended commit |
|---|---|---|
| `M0-A` | Safety contract and existing-state check | Scoped work + tangible artifact; `docs(closeout): [G05/M0-A] ...` or `fix/feat` when source changed |
| `M1-A` | Metadata predicates and stale evidence | Scoped work + tangible artifact; `docs(closeout): [G05/M1-A] ...` or `fix/feat` when source changed |
| `M1-B` | Bounded literal scan | Scoped work + tangible artifact; `docs(closeout): [G05/M1-B] ...` or `fix/feat` when source changed |
| `M1-C` | Rule editor and Test Rule | Scoped work + tangible artifact; `docs(closeout): [G05/M1-C] ...` or `fix/feat` when source changed |
| `M2-A` | Narrow tests and final admission review | Scoped work + tangible artifact; `docs(closeout): [G05/M2-A] ...` or `fix/feat` when source changed |
| `M3-A` | Controlled enablement and closure | Scoped work + tangible artifact; `docs(closeout): [G05/M3-A] ...` or `fix/feat` when source changed |

### M0-A — Safety contract and existing-state check

**Objective and detailed handoff.** Verify current v1/v2 round trips, all dispatch paths, current fail-closed gate, file identity/lease proof and editor/preview. Record exact predicate semantics and rejection priority: invalid > changed source/rule > unavailable resource > no-match > match.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-A — Metadata predicates and stale evidence

**Objective and detailed handoff.** Use existing `evaluate_metadata`, captured current time and stable settled-file snapshot for size/modified age. Revalidate rule revision, candidate lease, root containment, source identity and actions before any admitted side effect. A changed or unreadable file never becomes a match.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-B — Bounded literal scan

**Objective and detailed handoff.** Implement a streaming/bounded content matcher with explicit byte budget, cancellation, time/size ceilings and NUL/binary/encoding policy. Never log literals, contents, watched paths beyond existing safe diagnostics or user secrets. Define DoesNotContain only after a complete admissible scan; truncated or read-error input must be UNKNOWN/refused, not success.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-C — Rule editor and Test Rule

**Objective and detailed handoff.** Expose size range, modified age and literal mode/input in existing rule authoring UI with validation. Test Rule uses the *same pure predicate* over a user-selected/synthetic fixture without invoking actions, moving, renaming, deleting, or consuming a production candidate lease; show match/no-match/unknown reason with sanitized diagnostics.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M2-A — Narrow tests and final admission review

**Objective and detailed handoff.** Use owned fixtures for metadata edges, age clock skew, empty literal, partial UTF-8/binary, unreadable/read error, changed evidence, cancellation and unsupported resource budgets. Verify old rules unchanged and v2 invalid fields quarantine/fail closed. One scoped review of unattended execution authority.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M3-A — Controlled enablement and closure

**Objective and detailed handoff.** Only remove the particular `FILE_CONDITIONS_NOT_EVALUATED` refusal once evaluator, editor and Test Rule acceptance are complete. Otherwise retain the fail-closed guard, document remaining work and stop safely.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### Reviewer handoff rule

For each completed production milestone, the parent must provide a reviewer: actual diff from the correct base, original objective, named callers and existing defenses, new tests and selection inventory, first meaningful failure if any, exact run/receipt, stop/accept criteria and non-goals. The reviewer should return **severity-ranked substantive findings only** and identify stale callers, duplicate state, false positive test passes, unsafe activation, migrations or ownership violations. One substantive correction pass is allowed before a recorded stop/spike; do not loop indefinitely.

## 6. Focused acceptance / negative regression tests

- Boundary inclusive min/max and clock skew/future modification time; invalid range or literal fails safely.
- Literal across chunk boundary and file > permitted budget; binary/UTF-8 policy, unreadable, truncated and cancelled yields unknown/refusal.
- Stale candidate/source swap; edited/disabled rule or changed action fingerprint before commit; no side effect.
- v1/v2 serialization preservation, valid rule editor tests, side-effect-free Test Rule fixture; no real unattended action to user folders.

Test execution is **not automatic simply because it is listed here**; first identify the actual Cargo package, present test names and test-target support. Select a nonzero meaningful subset. Prefer `cargo nextest list` and `cargo nextest run` scoped to named modules or exact test cases; if a test command selects zero, that is not PASS. Default allowance for one changed source checkpoint is one selected preparation, one selected run and at most two correction reruns **only when required**. The program-wide budgets in [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md) control timings, native proof and documentation-only checks. Never weaken tests, delete failures or claim native acceptance from pure raster/source tests.

## 7. Decision gates: block only unsafe scope, not the whole queue

- If new conditions cannot be bound to settled candidate evidence, leave all configured conditions fail closed and create a smaller proof spike; no optimistic execution.
- No recursive scan of arbitrary folders or indefinite blocking on content readers; bounded queues and idle overhead must be preserved.

Classify each checkpoint as `PENDING`, `IN_PROGRESS`, `ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `DEFERRED_DEPENDENCY`, `DEFERRED_WITH_DESIGN`, `NATIVE_PROOF_DEFERRED`, `FAILED_VERIFICATION`, or `NO_CHANGE_REVALIDATED`. The label must match the documented facts; a `PARTIAL_ACCEPTED` status is not feature-wide completion. On a true stop, create a durable per-goal `stalls/<checkpoint>-UTC.md` with actual source/ref, blocker, attempted one-shot fallback, unrun commands, ownership/cleanup, accepted files and exact next step; then return control to the parent to schedule the next independent goal.

## 8. Branch, commit, push, integration and model rules

1. Parent verifies actual `origin/master` and the program's accepted integration ancestry. Each **new** Goal receives its own branch `codex/closeout-g05-hotfolder-condition-evaluator-<RUN_ID>` from the pinned program-start branch (contains only this plan) unless an explicitly accepted predecessor must be present. Dependent goals start from the prior **accepted integration commit** and record this exception. Never base on a rejected/dirty historical implementation tip.
2. Keep one active writer per goal; parent owns Git and the authoritative program ledger. Create the goal's execution directory and a `M0-A` docs baseline commit before editing production source. Accepted milestones each get meaningful Conventional Commit-style subjects including `G05/<milestone>` and one normal non-force push after commit. If push fails, preserve local commits and proceed; remote success must be recorded truthfully.
3. Parent integrates only reviewed, acceptance-gated production/documentation checkpoints into `codex/closeout-integration-<RUN_ID>` in dependency order (normally `git cherry-pick -x`). Do not import failed/unaccepted source. A rejected goal may contribute its **documentation-only** stop report/complete future pair, not an unsafe patch.
4. Role defaults verified from current `.codex`: orchestrator `gpt-6-astra/xhigh`; planner/reviewer `gpt-6-astra/max`; implementer `gpt-6-astra/xhigh`. These are configured role preferences, not proof of actual backend. If supported and selected for a narrow bounded implementation, `gpt-6.1-sol/xhigh` may be used deliberately, with honest attribution. No silent `.codex` configuration edits.
5. Roles contribute `roles/planner/`, `roles/implementer/`, `roles/reviewer/` and `roles/orchestrator/` Markdown under this goal's execution directory. New actionable recommendations get complete role-authored `roles/<role>/follow-on/<id>/GOAL.md` and `KICKSTART.md`; parent deduplicates into a canonical `follow-on/<id>/` library. Do not fabricate findings to fill quotas.
6. Do not ask the user to manually test between milestones; use supported app/native automation if available. If unavailable, log the gap and reserve a **single final manual-review checklist** for the program handoff.

## 9. Completed outcome definition

Size/age/literal conditions and Test Rule are implemented with accepted safety predicates and focused proof, or the initial fail-closed behavior remains with a precise deferred design package.

**Required closure record:** real branch/base, all milestone dispositions, exact changed files, accepted and rejected interfaces, focused test counts and command receipts, actual native proof or `NOT_RUN`, model/role provenance, reviewer disposition, commits/pushes, output/follow-on paths, source safety constraints, and exact next action. No PR merge/CI success may be claimed without host-confirmed evidence.

## 10. Execution progress — mutable (Codex updates here)

- **Current state:** `IN_PROGRESS`; M0-A safety contract/source planning.
- **Run ID and branch:** 20261029-r01 / codex/closeout-g05-hotfolder-condition-evaluator-20261029-r01.
- **Actual master/source base:** fetched master4a1283e6; bootstrap02d0ac64; integration5873cb8a.
- **Current milestone:** M1-A verified19/19; independent checkpoint review pending.
- **Active role and requested/configured/resolved model:** parent checkpoint; implementer requested Astra xhigh/configured Luna max/backend UNVERIFIED.
- **Last known commit / push:** contract c009b533 pushed; M1-A source checkpoint next.
- **Next action:** independent M1-A review; then fresh M1-B packet against checkpointed interfaces. All condition guards retained.
- **Blocker and scope:** none reported.

| Milestone | State | Real artifact, source commit, verification and owner | Next exact action |
|---|---|---|---|
| `M0-A` | ACCEPTED_COMPLETE | safety contract c009b533; source-only design | Preserve binding/gates |
| `M1-A` | IN_PROGRESS | exact corrected19/19; source checkpoint/review next | No reader/GUI/native enablement |
| `M1-B` | PENDING | — | Bounded literal scan |
| `M1-C` | PENDING | — | Rule editor and Test Rule |
| `M2-A` | PENDING | — | Narrow tests and final admission review |
| `M3-A` | PENDING | — | Controlled enablement and closure |

### Append-only execution events

- Planning package created 2026-10-29; no goal code, test, branch, PR or merge was executed by generating these documents.

- Runtime2026-10-30: prior goals dispositions closed sequentially throughG04; G05 independent frombootstrap. Master4a1283e6 unchanged after fetch, integration5873cb8a authoritative. Conditions fail-closed safety remains until actual evidence. Planner next; no source/test/native work at baseline.

- Scope clarification2026-10-29session/2026-10-30UTC: necessary later narrow existing file_operations HotFolder ownership handoff and actual GUI dispatch binding are authorized by parent to avoid copied-evidence/path-reopen mismatch. No duplicate FFI/reader/framework/externalauthority redesign. M1-A constrained to existing model/execution/minimalvisibility/shared semantics, all condition gates retained. Planner safety contract/packet persisted; source implementation pending.

- Runtime M1-A2026-10-30: corrected19/19 focused tests; two approved source modules only. All configured conditions still refused. Initial4fixture failures preserved; representable subsecond timestamps/real WrongServiceLease correction, no production weakening. Prep381.358s, incremental88.824s, finalrun4.742s within caps; cleanup complete. Parent source checkpoint then independent review, no M1-B work yet.
