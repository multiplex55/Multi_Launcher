# G05 KICKSTART — G16 Hot Folder: Safe Size, Age and Literal-Content Conditions

**Binding task contract:** [GOAL.md](GOAL.md), overarching [MEGA_GOAL.md](../../MEGA_GOAL.md), [ORCHESTRATOR_KICKSTART](../../ORCHESTRATOR_KICKSTART.md), and [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md). If run under the orchestrator, **do not create a new independent program or ask for this already supplied task scope**.

## Dispatch packet

**Desired result:** Turn the already persisted v2 optional Hot Folder conditions into a bounded and safe evaluator, visible editor controls and a side-effect-free Test Rule workflow; preserve the fail-closed behavior until all required evidence exists.

**Hard gate:** None, but fail-closed condition guarantee is mandatory. Before doing any source work, read the current Goal mutable progress and the authoritative `PROGRAM_STATUS.md` and check this gate. If unmet, write `DEFERRED_DEPENDENCY` with exact missing proof; do not do the blocked implementation, and return to the parent so the next independent goal proceeds.

**Current-state references:**

- [`src/hotfolder/model.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/hotfolder/model.rs) — `HOT_FOLDER_RULES_FORMAT_VERSION=2`, `HotFolderConditions`, `evaluate_metadata`, `FILE_CONDITIONS_NOT_EVALUATED`
- [`src/hotfolder/runtime.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/hotfolder/runtime.rs) — watcher ingress, settling, ready candidate leases
- [`src/hotfolder/execution.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/hotfolder/execution.rs) — `evaluate_rule_conditions`, action admission, receipts, loop control
- [`src/hotfolder/capability.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/hotfolder/capability.rs) — saved action identity/eligibility checks
- [`src/hotfolder/store.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/hotfolder/store.rs) — v1/v2 persistence
- [PR #1455](https://github.com/multiplex55/Multi_Launcher/pull/1455) — G16 M1-A model/fail-closed behavior only

## Execute sequentially

1. **Safe start:** discover current repository/worktree/AGENTS instructions; fetch and verify `origin/master`; check user changes, other worktrees and active Git operations. Resume an existing matching task branch only when its program identity and dependency ancestry are valid. Otherwise let parent create fresh named branch from program-start base or accepted dependency. Historical SHA `56223fdc123396f4c41c48e0542506d912c6c634` is planning evidence, never a reset instruction.
2. **Role assignment:** parent orchestrator owns Git, progress and conflicts. Have planner (configured Astra/max) inspect real owners and produce one approved bounded milestone handoff. Have exactly one implementer/author (configured Astra/xhigh, optionally Sol/xhigh if explicitly supported and attributed) execute that handoff. Reviewer (configured Astra/max) examines the stable diff and focused proof. One active child at a time; do not require a second worker to start another goal.
3. **Create durable task state:** write `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G05/` with `SOURCE_REVALIDATION.md`, `PROGRESS.md`, `VERIFICATION.md`, `RISKS.md`, role folders and `follow-on/`. Copy or link this exact Goal/Kickstart. Initialize the mutable progress in GOAL.md; docs baseline commit/push before source work.
4. **Execute checkpoints in order.** Do not enter a subsequent checkpoint until the previous relevant source evidence is accepted or a truthful defer/no-change boundary has been documented:

   - **M0-A — Safety contract and existing-state check:** Verify current v1/v2 round trips, all dispatch paths, current fail-closed gate, file identity/lease proof and editor/preview. Record exact predicate semantics and rejection priority: invalid > changed source/rule > unavailable resource > no-match > match.
   - **M1-A — Metadata predicates and stale evidence:** Use existing `evaluate_metadata`, captured current time and stable settled-file snapshot for size/modified age. Revalidate rule revision, candidate lease, root containment, source identity and actions before any admitted side effect. A changed or unreadable file never becomes a match.
   - **M1-B — Bounded literal scan:** Implement a streaming/bounded content matcher with explicit byte budget, cancellation, time/size ceilings and NUL/binary/encoding policy. Never log literals, contents, watched paths beyond existing safe diagnostics or user secrets. Define DoesNotContain only after a complete admissible scan; truncated or read-error input must be UNKNOWN/refused, not success.
   - **M1-C — Rule editor and Test Rule:** Expose size range, modified age and literal mode/input in existing rule authoring UI with validation. Test Rule uses the *same pure predicate* over a user-selected/synthetic fixture without invoking actions, moving, renaming, deleting, or consuming a production candidate lease; show match/no-match/unknown reason with sanitized diagnostics.
   - **M2-A — Narrow tests and final admission review:** Use owned fixtures for metadata edges, age clock skew, empty literal, partial UTF-8/binary, unreadable/read error, changed evidence, cancellation and unsupported resource budgets. Verify old rules unchanged and v2 invalid fields quarantine/fail closed. One scoped review of unattended execution authority.
   - **M3-A — Controlled enablement and closure:** Only remove the particular `FILE_CONDITIONS_NOT_EVALUATED` refusal once evaluator, editor and Test Rule acceptance are complete. Otherwise retain the fail-closed guard, document remaining work and stop safely.

5. **Verification:** choose actual nonzero targeted tests matched to changed modules; respect per-checkpoint compilation/execution caps in VERIFICATION_POLICY. No default full nextest/Clippy/native campaign. Source-only claims require inspection, not invented passing tests. Native-proof-specific user flows need supported automation, real effects in owned fixtures and actual results; otherwise mark that piece `NATIVE_PROOF_DEFERRED`.
6. **Review and publication:** review the exact cumulative source diff and changed callers. Apply at most one substantive correction cycle, rerun only the affected checks, and preserve failures. Commit coherent accepted checkpoints; attempt one normal push after each. Report real task/parent/head/upstream and push outcome. Parent may import **accepted** changes only; do not independently merge to master or start another Goal.
7. **Follow-on package duty:** for each new *source-supported* actionable issue, write a fully scoped Goal/Kickstart pair in the discovering role's folder and parent canonical library. Include confidence, counterevidence, chosen existing owner, exact scope/callers, migrations/defaults, stage boundaries, focused tests/native proof, dependencies, model assignments, small commits, rollbacks, and defer criteria. Do not execute generated follow-ons or invent an unbounded queue.
8. **Stop handling:** if hard proof/tool/permission/native scope fails, save the source/condition, actual command/error, exact resume step and a Markdown report. Return a `DEFERRED_*` state and let the parent proceed to the next **independent** goal. Never claim future background work or hand off an unfinished source mutation as accepted.

## Must-preserve checklist

- Existing rules without new conditions behave exactly as before, including v1 migration/defaults.
- Content DoesNotContain must never succeed on partial/unreadable/changed input.
- All conditions evaluate under the same rule revision, candidate identity, settled-file evidence and action authorization used by the effect; no UI preview can grant execution permission.
- No execution from Test Rule; no new watcher polling when no rules/conditions are enabled.

## Exact test intent

- Boundary inclusive min/max and clock skew/future modification time; invalid range or literal fails safely.
- Literal across chunk boundary and file > permitted budget; binary/UTF-8 policy, unreadable, truncated and cancelled yields unknown/refusal.
- Stale candidate/source swap; edited/disabled rule or changed action fingerprint before commit; no side effect.
- v1/v2 serialization preservation, valid rule editor tests, side-effect-free Test Rule fixture; no real unattended action to user folders.

## Done report to orchestrator

Include `G05` outcome (`ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `NO_CHANGE_REVALIDATED` or precise defer), source/master/branch/closing SHA/push, diff and owners, tests actually run + nonzero counts, native evidence vs `NOT_RUN`, reviewer and model receipts, unresolved issues, created follow-on Goal/Kickstart paths and integration eligibility. **Never claim the presence of a Kickstart itself proves implementation acceptance.**
