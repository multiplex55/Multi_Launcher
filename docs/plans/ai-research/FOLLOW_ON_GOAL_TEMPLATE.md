# Follow-On Goal Authoring Template — Full Engineering Contract

**This file is an authoring template, not an executable implementation goal.** During either research goal, instantiate one complete `GOAL.md` plus companion `KICKSTART.md` for each actionable recommendation. Replace the instructions below with actual source-backed content. Do not leave bracketed fields, generic TODOs, or a requirement for the user to rediscover the architecture.

A finding that is still speculative must produce a **bounded research-spike goal**, with a concrete decision and evidence deliverable, rather than a pretend production fix. The generated files default to `PROPOSED_NOT_AUTHORIZED`; their creation does not authorize executing them.

---

# Goal <stable-ID> — <Specific outcome>

## 0. Identity, provenance, and readiness

Record:

- Package ID, recommendation/finding IDs, source research goal and canonical directory.
- Type: correctness fix, behavior-preserving refactor, feature, performance experiment, or research spike.
- State: `PROPOSED_NOT_AUTHORIZED`.
- Readiness: `READY_FOR_IMPLEMENTATION_REVIEW` or `NEEDS_RESEARCH_SPIKE`.
- Priority, user value, severity where applicable, confidence, implementation complexity, regression risk, and validation burden as separately explained judgments.
- Research source SHA and paths/symbols. This is evidence provenance, not an instruction to reset a later implementation to an old SHA.
- Original author role/model, contributors, reviewer and date; actual identity unknown remains unknown.
- Links to source finding, decision record, competing role proposal and audit/search handoff.
- Exact companion `KICKSTART.md` path.

## 1. Objective and user-visible outcome

Explain the concrete change and why it matters. State the current behavior and intended behavior with representative examples. For an internal refactor, state what must remain observably identical and what engineering property improves.

Specify whether the benefit is:

- A proven source defect/inconsistency removed.
- An anticipated user-efficiency or maintenance benefit.
- A measurable performance hypothesis requiring a later comparison.
- A speculative opportunity that must first pass an investigation gate.

Do not turn a static inference into a reproduced runtime failure. Do not promise a percentage improvement without an actual baseline and experiment.

## 2. Current architecture and evidence

Describe the real architectural owner, concrete entry points, relevant callers, state/data types, and existing guards. Cite pinned source paths and symbols/ranges. Explain why the current owner is the appropriate place to change the behavior.

Include counterevidence, existing partial implementations, and overlap with older/deferred plans. State why this package is new work, a narrow amendment to existing work, or a replacement for an obsolete plan. Do not duplicate an existing engine or entire framework just to match a proposed design.

A table should identify:

| Owner/path/symbol | Current responsibility | Proposed change | Relevant callers/contracts |
|---|---|---|---|
| Fill with actual source | Explain | Explain | Link |

If exact ownership is unresolved, make bounded source discovery the explicit first research milestone. Do not use vague “find relevant files” instructions in a package labeled implementation-ready.

## 3. Scope, non-goals, and dependencies

List allowed production/documentation/configuration owners and adjacent changes genuinely necessary for compatibility. Identify files/interfaces that must not be changed.

State prerequisites as capabilities and accepted commits/branches where actually known. For independent future work, begin from the then-current verified master. For dependent work, identify the required accepted predecessor or approved integrated tip. Never inherit unaccepted patches or use a moving branch name as proof of prerequisite completion.

Do not silently reopen old queued goals, exhausted repair attempts, global editor configuration, or unrelated native campaigns. Explicitly describe how missing prerequisites affect only this package and which analysis can still proceed.

## 4. Proposed design in implementer-level detail

Provide the substantive design, not only a wish list:

1. **Ownership and API:** which component owns policy/state, proposed public interface shapes, inputs/outputs, lifetimes and identity. Illustrative signatures may clarify contracts, but private helper names remain implementation choices.
2. **Data flow:** producer → validation → transformation → consumer; identify where existing flows are reused and where behavior changes.
3. **State transitions:** admission, pending/running, replacement, cancellation, terminal result, retirement and cleanup where relevant.
4. **Algorithm:** steps, precedence, tie rules, complexity, bounds, failure exits and examples. Define mathematical quantities instead of vague “smart ranking.”
5. **Persistence:** schema/defaults/migrations, corruption handling, transactional boundaries and backward compatibility when applicable.
6. **UI/keyboard behavior:** exact user action, visible response, focus/selection, modal authority, confirmation and disabled-state reason where applicable.
7. **Concurrency:** synchronization/ownership, stale result fencing, thread affinity, callback/reentrancy restrictions, shutdown and resource release where applicable.
8. **Safety/privacy:** action eligibility, payload handling, file identity, destructive-operation approval, retention and sanitized diagnostics where applicable.
9. **Resource budget:** proposed limits and behavior at limits; separate source reasoning from measured runtime claims.
10. **Migration/rollback:** ordered transition from existing owner, temporary compatibility rules, when obsolete code can be removed, and safe reversal.

Use concrete adverse sequences. Examples: a new query arrives before an old result; settings disable a provider while its result is selected; destination identity changes after approval; a worker reports cancelled before resources retire. Only include examples relevant to the actual package.

For a research spike, replace a production design commitment with competing hypotheses, bounded evidence collection, explicit decision criteria, and deliverables. Its success may be “do not implement this approach.”

## 5. Alternatives, tradeoffs, and rejection conditions

Compare the smallest viable options: no change/documentation, local fix, narrow extraction/consolidation, and larger redesign only where justified. State why the selected option fits current architecture.

Explain risks, compatibility cost, maintenance burden, testability and expected value. Identify evidence that would make the recommendation wrong. Do not conceal uncertainty behind an implementation mandate.

## 6. Invariants and acceptance behavior

Enumerate concrete properties that must hold. Examples must be adapted to the package:

- Explicit command routes and literal payloads keep their current semantics.
- A selected action remains bound to the same stable identity until activation or invalidation.
- Cancellation does not imply retirement before the owner proves it.
- Unconfirmed or identity-mismatched file replacement is rejected without mutating user data.
- Disabled/unsupported capabilities cannot become executable through a new surface.
- Default persisted behavior remains compatible with older configurations.
- Bounded retained metadata does not leak raw private content.

Give observable pass/fail outcomes, not “works correctly.” Distinguish implementation acceptance from optional future measurement or human review. A failure cannot be relabeled a pass because the package is otherwise useful.

## 7. Fresh-branch and checkpoint execution contract

Before source changes, locate repository and inspect current state. Create one new idiomatic branch from the correctly resolved current base; verify name collision, preserve dirty work, and record source/base/baseline. An interrupted authorized instance resumes its existing branch after identity checks instead of creating another baseline.

Specify M0-A and the fewest coherent implementation checkpoints that preserve ownership and review. Fill a concrete table:

| Checkpoint | Objective | Real owners/paths | Required change and outputs | Focused acceptance | Commit subject |
|---|---|---|---|---|---|
| M0-A | Safe bootstrap | Task docs/Git state | New branch and progress record | Identity, clean task scope | Exact subject |
| M1-A | Package-specific | Actual source | Explicit tasks | Actual invariants | Exact subject |
| Remaining | Only as needed | Actual source | Explicit tasks | Actual invariants | Exact subject |

For each substantial checkpoint, provide a separate implementer-ready handoff: objective, owner, facts, tasks, invariants, non-goals, dependencies, tests, verification and done criteria. Do not compress a broad feature into a single vague milestone.

Commit coherent accepted increments frequently and attempt one normal push after each. Preserve local commits when push fails; never force-push, amend away historical failures, or auto-merge master. Do not require a full suite before a documentation or pure metadata checkpoint.

## 8. Targeted future verification plan — not executed during research

The package must contain a concrete future verification matrix:

| Behavior/risk | Test owner/path/case or exact discovery step | Fixture/setup | Expected observation | Verification level | Bound/cleanup |
|---|---|---|---|---|---|
| Fill for each meaningful invariant | Source-backed | Safe synthetic data | Specific result | Unit / integration / native / experiment | Proportionate |

Explain precisely what needs to be added or changed and which existing tests should remain intact. Validate case discovery before calling a filtered run a pass. Identify package/target/features from current source when possible, and require version/target revalidation before future execution. Do not invent unsupported CLI flags or quote a guessed test name as existing.

Separate compile/build, selected test execution, native interaction and cleanup budgets. Do not run a repository-wide suite unless a concrete affected boundary cannot be covered narrowly; document that decision. Retain the user's preference for focused execution and no unnecessary TOML/global configuration changes.

For performance work, define baseline/candidate identity, workload, cache preparation, metric, noise limits and interpretation; no broad optimization matrix. For UI work, specify only the native interactions that must be proven and supported automation capabilities. If automation is unavailable, do not invent input APIs or disguise a unit test as native proof. Record the precise remaining acceptance gap and leave a bounded human-review handoff when permitted by the task.

For documentation/research follow-ons, explicitly say no Cargo/native/Clippy is required. For actual Rust implementation, include appropriate scoped first-party lint/review at the end, excluding unrelated vendored cleanup. Do not make the current research run execute these future checks.

## 9. Failure handling, rollback, and bounded continuation

State the limited correction policy appropriate to this package and what evidence must be preserved. Differentiate:

- Routine localized correction within approved scope.
- A real missing capability or unsafe architectural premise.
- An optional experiment that can remain inconclusive.
- An exhausted mandatory acceptance boundary.

Continue unaffected authorized work where dependencies permit. Never fabricate passing acceptance, silently renew a consumed budget, or repeat an unavailable model/tool indefinitely. When a genuine stop occurs, preserve partial work, log exact next action and publish the Markdown report when safe. Do not promise background continuation after the session ends.

## 10. Model assignments and role folders

Recommend actual supported model IDs/effort for planner, implementer, reviewer and parent if relevant, using MODEL_ASSIGNMENTS.md as dated guidance. Explain why this particular package needs that setup. Include available-model fallback and truthful review independence.

Define `roles/<role>/ROLE_RECORD.md` and role analysis/review locations within the task's docs directory. Read-only roles return content for the authorized writer. Do not change global role configs or permissions implicitly.

## 11. Progress and stall reporting

Give the exact task documentation path and mutable goal-progress section. Require updates at real state transitions with artifacts, last known checkpoint, current issue, next action and actual push visibility.

Use a task-specific `stalls/<checkpoint>-<UTC>.md` report for genuine Blocked/Paused/Deferred states. Include diagnostic/command, expected vs actual result, current branch/worktree, preserved changes, ownership/cleanup facts, budget, evidence and smallest safe resume instruction. Do not put secrets or private payloads in repository docs.

A report is valid even when implementation is not accepted; label the commit accordingly. Do not commit through an unresolved Git conflict or unsafe index. Preserve external/local evidence and publish at the first safe point.

## 12. Final delivery and authorization boundary

Specify the required final files, code/docs changed, validation evidence, remaining limitations, branch/base/checkpoint/final commit, push status and reviewer disposition. Do not claim full acceptance from a partial code commit or a benchmark prototype.

Preparing this goal and kickstart does not execute it. Opening/merging a PR, deploying, releasing, performing destructive user-data operations or changing external accounts requires the appropriate separate authorization.

## 13. Execution progress — mutable

Initialize the generated package with truthful state `PROPOSED_NOT_AUTHORIZED`; no tests/commits complete. Include a milestone table and next action: user selects/authorizes this package, then revalidate current source and bootstrap safely.

---

## Author's completion check

Before promoting a role proposal to canonical follow-on work, verify that all template instructions were replaced with substantive package-specific content, paths and evidence. Check that the Goal and Kickstart agree on scope, dependencies, branch base, models, limits and completion. Validate links and ID mapping. A full-looking document with generic prose is not an implementation-ready plan.
