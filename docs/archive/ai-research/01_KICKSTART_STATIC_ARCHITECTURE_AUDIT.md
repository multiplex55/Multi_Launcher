# Kickstart 01 — Execute the Static Architecture Audit

Read [01_GOAL_STATIC_ARCHITECTURE_AUDIT.md](01_GOAL_STATIC_ARCHITECTURE_AUDIT.md) and [ORCHESTRATOR_GOAL.md](ORCHESTRATOR_GOAL.md) completely. Execute the first research goal; do not begin search research concurrently and do not modify production code.

## 1. Confirm source, branch, and prior progress

Use the program's actual new-master `MASTER_START`. If no program exists, perform the orchestrator bootstrap rather than inventing a different baseline. If this goal already has a branch, worktree, or checkpoint record, verify identity and resume the first unfinished checkpoint.

For a new goal, create exactly one fresh `codex/research-static-audit-<RUN_ID>` branch from the verified master. Preserve unrelated user changes using a safe isolated worktree. Do not create a new branch to escape an ordinary research uncertainty. Do not reset, stash, force-push, overwrite prior plans, or operate on master.

Resolve `GOAL_ROOT=docs/research/<PROGRAM_ID>/01-static-audit`. Record source SHA, actual branch/worktree, relevant instructions, role/model availability and this goal's coverage approach in M0-A. Commit/push the coherent baseline normally.

## 2. Delegate sequentially and persist every role's contribution

Use planner → research implementer → reviewer, one active child at a time. Recommended models are Astra max for planning/review and Astra xhigh for research authorship. Follow MODEL_ASSIGNMENTS.md for actual configuration and fallback.

Each role contributes under `GOAL_ROOT/roles/<role>/`: model/effort provenance, substantive analysis, decisions or objections, and full proposed Goal/Kickstart pairs for genuinely new actionable recommendations. Read-only roles return Markdown for the parent to persist. Do not claim they directly wrote files or that a parent self-check is independent review.

On one capacity failure, retire completed owned children if supported, try once, then use the authorized parent/available-role fallback. Do not repeat dispatch loops. The goal is the audit deliverable, not perfect agent orchestration.

## 3. Produce a real source-grounded audit

Execute the milestone sequence from the Goal:

- M1-A: source owner/dependency inventory and risk-weighted coverage matrix.
- M1-B: query/command/activation and input/focus/Jump ownership traces.
- M2-A: worker lifecycle/cancellation, persistence/file identity/recovery and privacy analysis.
- M2-B: change-cost/performance hypotheses, alternatives, findings and priorities.
- M3-A: complete follow-on work library and search-specific handoff.
- M3-B: substantive review, correction, limitations and closure.

Inspect actual owners, callers and guards; do not merely paraphrase file names. Build end-to-end traces and state/sequence diagrams where they clarify important behavior. Use source citations pinned to the analyzed commit. Separate observed code from inferred consequences and unverified Windows behavior.

Do not assume a previous blocked implementation remains a defect in current master. Check whether the code was merged, excluded, moved, fixed or superseded. Preserve sound existing abstractions and prefer narrow fixes over unnecessary framework replacements.

No bug quota. No artificial certainty. No performance percentages derived from static file size. A no-change conclusion with evidence is valid.

## 4. Turn recommendations into complete future contracts

For every selected actionable recommendation, fill the supplied follow-on templates using real owners and precise behavior. Store the canonical pair in `GOAL_ROOT/follow-on/AUD-F###-<slug>/` and map it from the finding/recommendation index.

Include:

- The concrete problem, affected user workflow, evidence and confidence.
- Current architecture and likely source files/symbols/callers.
- Proposed design, algorithm/state transitions, migration and compatibility.
- Invariants, resource/cancellation/file-safety rules, non-goals and dependencies.
- New branch policy, small checkpoints, progress logging and safe rollback.
- Focused future tests with expected observations, native proof only where necessary, and what must not be retested broadly.
- Proven versus speculative benefit, unresolved questions and model assignment.

Uncertain findings get research-spike Goal/Kickstart pairs that produce a decision, not unsupported production fixes. Duplicate role proposals point to the canonical package after consolidation; preserve attribution and substantive dissent.

Do not execute any future package now. Do not recursively create unlimited programs from speculative ideas.

## 5. Research-only verification and checkpoint publication

Do not run Cargo/rustc/nextest/Clippy, the launcher, native input, profiler or build harness. Do not install tooling, alter global configuration, or inspect live clipboard/history data. Simple static parsing and local document/ID checks are enough.

Update this goal's mutable progress section at actual transitions: completed output links, known previous commit/push, current limit and exact next action. Update the program index consistently. Commit each coherent checkpoint before changing topic, then attempt one ordinary push. Continue locally when a push is denied; record visibility honestly.

A substantive review gets one focused correction pass. Further disagreement becomes a qualified finding, alternative, or narrow future spike. Remove or narrow false claims; do not stall the whole audit on an answer unavailable from static evidence.

## 6. Handoff and stop behavior

Finish SOURCE_MAP, COVERAGE_MATRIX, all scoped audit reports, FINDINGS, recommendations, complete follow-on library, and SEARCH_HANDOFF. The handoff must explain the exact search/input/async/persistence constraints Goal 02 should preserve and where evidence is missing.

Mark research Complete or Complete with limitations only when the core required content exists. Missing optional runtime proof is not a build failure; missing core audit content is Partial and must be labeled so. Record any safe partial handoff explicitly rather than declaring a comprehensive review.

If a genuine Git/permission/data-safety issue blocks writing, preserve state and create a timestamped Markdown stop report under the program stalls directory when safe. Before a session boundary, record the runnable next step and commit coherent documentation; do not imply background activity.

The parent records this goal's closing commit, retires writers, and starts Goal 02 on its own fresh branch from that commit. Do not start Goal 02 yourself unless acting as the program parent. Do not reopen completed audit sections for optional improvements.
