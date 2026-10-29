# EXECUTE — Sequential Multi Launcher Reliability and Feature Closeout

**This is the one entrypoint.** Read sibling [MEGA_GOAL.md](MEGA_GOAL.md), [SOURCE_AND_RISK_BASELINE.md](SOURCE_AND_RISK_BASELINE.md), [VERIFICATION_POLICY.md](VERIFICATION_POLICY.md), [PROGRESS_AND_STALL_TEMPLATES.md](PROGRESS_AND_STALL_TEMPLATES.md), all eleven child `GOAL.md` and `KICKSTART.md` contracts (or read the next pair fully before that child starts) and the current repository `AGENTS.md`. **Execute the plan; do not merely create another plan.** Do not ask user to perform manual verification between goals.

## 0. Immediate mandate and non-goals

The user authorizes a phased implementation-oriented closeout of the completed Astra research findings and known incomplete accepted foundations, beginning with the source-supported Folder Diff failure-information fix. Complete what can be safely proven, and produce detailed, actionable future Goal/Kickstart plans for the rest. The current research explicitly rejects implementation of unused intelligent search metadata/ranker/UI. The historic terminal query escape is already integrated: do not reimplement it.

Never import old unsafe/dirty queue patches wholesale, silently increase a test budget, force-push/overwrite user edits, install dependencies, run a full Rust suite, create workspace features, or auto-merge to master. **After the program, create a review-ready PR and stop for the user's final review**; no manual check is needed during the run.

## 1. Source/permission/model bootstrapping

1. Locate `G:/Repos/rust/Multi_Launcher` if available, else identify the actual connected checkout. Read current `AGENTS.md`, `.codex/config.toml`, planner/implementer/reviewer role files, manifest, worktrees and Git state. This project has only one permitted source of current runtime truth: actual verified checked-out Git tree, not the historical SHA below.
2. Fetch/pull safely from `origin/master` without discarding user changes. Planning-time merge HEAD `56223fdc123396f4c41c48e0542506d912c6c634` after [PR #1456](https://github.com/multiplex55/Multi_Launcher/pull/1456) and [PR #1457](https://github.com/multiplex55/Multi_Launcher/pull/1457) is an evidence pin, not a reset instruction. Verify current remote/local master, stale `continue.md` and documented source owners. Record user changes and ownership; use an isolated clean worktree when needed.
3. If matching program index/branch exists, verify its identity and ancestry and resume the first unfinished milestone **instead of starting another run**. If not, allocate a unique run ID `20261029-r01` or a subsequent unused suffix. Create `codex/closeout-integration-<RUN_ID>` from verified new master; add/commit this plan package and a skeleton `docs/plans/execution/multi-launcher-reliability-closeout-20261029-<RUN_ID>/` with source, role and branch records. This exact committed state is `PROGRAM_BOOTSTRAP_SHA`.
4. Observe configured agent pins (`gpt-6-astra` xhigh orchestrator/implementer and max planner/reviewer at planning-time master); record requested/observed/resolved separately. Do not modify global `.codex` files. Use a supported explicit Sol xhigh alternative for narrow authorship only when deliberate/available and documented. **Only one active child source writer; no parallel Goals.**
5. Parent alone owns Git, upstream pushes, task schedule, authoritative program status and final PR; children author bounded research or source and can return their content for the parent to persist with accurate attribution.

## 2. Launch loop: one Goal at a time

For each G00→G10 in the exact order below, perform this complete control loop:

```text
Read the complete Goal and Kickstart, current source and dependencies
→ Verify hard gate (no optimistic prerequisite completion)
→ Create fresh goal branch from PROGRAM_BOOTSTRAP_SHA; if a hard accepted
  dependency is required, use the exact ACCEPTED integration tip and record it
→ Create goal docs/progress/roles/follow-on directory; M0-A docs commit + push
→ Planner (read-only) returns one implementer-ready milestone handoff
→ Implementer does only the bounded milestone, tests and durable Markdown
→ Reviewer challenges the exact changed owner and focused evidence
→ One correction pass if necessary; no endless tests or reviews
→ Parent records TRUE accepted/partial/deferred state, commits, normal-pushes
→ Parent imports ONLY accepted reviewed source (and safe docs) to integration
→ Update authoritative program ledger and last accepted SHA; commit/push
→ Next independent Goal. Never simultaneously run a second Goal.
```

**Fixed order:**

- **G00 — Source Baseline, Stale Continuation Cleanup and Execution Ledger:** [Goal](goals/G00-baseline-and-continuation/GOAL.md) · [Kickstart](goals/G00-baseline-and-continuation/KICKSTART.md). Dependency: None; must complete before production work.
- **G01 — Folder Diff: Preserve Destination Read Errors and Operation Feedback:** [Goal](goals/G01-diff-failure-information/GOAL.md) · [Kickstart](goals/G01-diff-failure-information/KICKSTART.md). Dependency: None; primary source-supported bugfix.
- **G02 — G09 Table Utility: Full-File Publication Safety Foundation:** [Goal](goals/G02-table-publication-safety/GOAL.md) · [Kickstart](goals/G02-table-publication-safety/KICKSTART.md). Dependency: None; G03 depends on this goal reaching ACCEPTED_COMPLETE.
- **G03 — G09 Table Utility: Conditional Full-File Operations and UX Closure:** [Goal](goals/G03-table-full-file-operations/GOAL.md) · [Kickstart](goals/G03-table-full-file-operations/KICKSTART.md). Dependency: G02 accepted safety foundation, otherwise DEFERRED_DEPENDENCY without source changes.
- **G04 — G12 Screenshot Redaction: Native Crop, Output and Interaction Acceptance:** [Goal](goals/G04-screenshot-native-redaction/GOAL.md) · [Kickstart](goals/G04-screenshot-native-redaction/KICKSTART.md). Dependency: No Table dependency; native proof required for end-to-end completion.
- **G05 — G16 Hot Folder: Safe Size, Age and Literal-Content Conditions:** [Goal](goals/G05-hotfolder-condition-evaluator/GOAL.md) · [Kickstart](goals/G05-hotfolder-condition-evaluator/KICKSTART.md). Dependency: None, but fail-closed condition guarantee is mandatory.
- **G06 — G11 Jump Mode: Finish Customization UI and Focus Regression Coverage:** [Goal](goals/G06-jump-settings-and-focus/GOAL.md) · [Kickstart](goals/G06-jump-settings-and-focus/KICKSTART.md). Dependency: None.
- **G07 — G14 ZIP: Bounded Extraction Writer and Safe UI Integration:** [Goal](goals/G07-zip-extraction/GOAL.md) · [Kickstart](goals/G07-zip-extraction/KICKSTART.md). Dependency: A safe writer and exact output contract must precede extraction UI enablement.
- **G08 — G15 MkMacro: Bounded Run History and Conditional Safe Recipes:** [Goal](goals/G08-mkmacro-run-history/GOAL.md) · [Kickstart](goals/G08-mkmacro-run-history/KICKSTART.md). Dependency: Recipes require accepted history and explicit policy; do not auto-run after mere display success.
- **G09 — G06 Activity Center: Source-Grounded Native Retirement Acceptance:** [Goal](goals/G09-activity-center-retirement/GOAL.md) · [Kickstart](goals/G09-activity-center-retirement/KICKSTART.md). Dependency: Native proof when owner retirement is claimed; do not infer from terminal UI.
- **G10 — Residual Defects, Deferred Goals and Final Integration Closeout:** [Goal](goals/G10-residual-defect-triage-integration/GOAL.md) · [Kickstart](goals/G10-residual-defect-triage-integration/KICKSTART.md). Dependency: Follows individual goal dispositions, never requires all goals to pass.

A goal that is `DEFERRED_WITH_DESIGN`, `NO_CHANGE_REVALIDATED`, or `NATIVE_PROOF_DEFERRED` has completed its *decision/documentation* duties but **not** falsely completed the intended implementation. Do not ask for approval to resume irrelevant work mid-run. Skip only its dependent tasks; keep executing independent goals.

## 3. Worktree/branch discipline and checkpoint cadence

- Before each branch: verify current integration accepted HEAD, source master pin, branch names, worktrees, operations and dirty state. Never remove another person's changes. New branch naming `codex/closeout-gXX-<slug>-<RUN_ID>`; report actual ancestor and any intentional dependent tip. `master` is never the active source editing branch.
- At G00 commit the program package and source baseline. On every goal commit/push a genuine M0 docs record, then small independent source checkpoints (M1-A, M1-B, etc.). No commit should claim to contain its own SHA; record it in the next event/parent ledger. A push failure preserves the local commit and does not stop the next milestone; one push attempt per checkpoint, no force.
- Goal branches may be independent sibling descendants of the program bootstrap. The parent alone performs selected `cherry-pick -x` into the cumulative integration branch. For G03 and G10, whose accepted predecessors matter, cherry-pick only commits **after** their accepted base to avoid replaying old ancestors. Never import a whole goal branch just because `git diff` shows unaccepted work.
- After accepted imports, inspect exact changed-file list, code tests/receipts, and `cargo` artifacts only if relevant. On a minor conflict owned by two accepted workstreams, one bounded adjustment/reviewer pass. On material conflict, preserve both and publish a follow-on pair instead of force-merging.
- For each checkpoint update both `goals/GXX-.../GOAL.md` mutable progress and parent `PROGRAM_STATUS.md`. Store per-role records and required future packages. Parent's integration ledger is the single source of *current program state*; dated frozen research reports remain immutable history.

## 4. Verification strategy — lots of thought, not a giant build campaign

- Read-only source/architecture planning with exact files, call sites and minimal current tests. Deep model reasoning is welcome; do not pre-implement an entire module in planning or run Cargo just to create a handoff.
- For a changed owner run **one nonzero focused test selection** and an optional single scoped lint as spelled out in each Goal. Default initial selected build/list upper bound 20 min, already-built test execution 180 s, one needed incremental correction or scoped lint 10 min. One or two affected reruns only for actual failures. Do not run a full Nextest 6,000+ test suite, blanket `cargo clippy --all`, native automated suite, perf harness or clean rebuild without a specific cross-owner necessity.
- Native input/output: app automation if supported; must really execute secondary mouse button, Windows publication, actual reaper teardown, or other exact native contract when asserted. If tools cannot perform it, record a missing proof and create a complete narrow acceptance follow-on, leaving unsafe features blocked. No user manual test until final handoff, and no sham success.
- Documentation-only acceptance uses cheap source/Markdown/JSON/link validation and staged-path inspection. Previously completed test receipts can be reused only for identical source+relevant manifest and exact behavioral scope; otherwise run the meaningful subset once.
- No private logs/clipboard content/macro variables/user files are to be captured or persisted in Markdown. Use synthetic fixtures and bounded task-owned folders.

## 5. Blockers, review limits and designed non-stalling behavior

- If capacity/model dispatch fails: record the actual error, retire completed owned agent if supported, retry once; then use available parent or supported agent fallback with truthful independence label. Never repeatedly spawn substitute chains.
- If a native driver lacks a required mouse operation or resource observation, stop *that native checkpoint only*, report `NATIVE_PROOF_DEFERRED`, do not cherry-pick an unsafe M2-A patch, and proceed to the next independent goal.
- If compiler/test reaches its bounded limit or a source owner contradicts the plan, preserve the first error and accepted safe checkpoint; no silent test weakening/timeout renewal. One factual stall report with exact safe restart is sufficient. A suspended process must be owned/cleaned up by the task before leaving it.
- If GitHub approval/push/PR is externally blocked, preserve commits and execute unrelated permitted local work. At final stage report exact PR state and user action; do not bypass protection, rewrite history or auto-merge.
- For unresolved distinct findings, each model/role creates its own complete actionable Goal/Kickstart pair in role's `follow-on/` directory; parent consolidates duplicates under canonical `follow-on/` and updates `FOLLOW_ON_CATALOG.md`. No recursive uncontrolled work queue or arbitrary new project spun up by research alone.

## 6. Final integration, review and user handoff

At G10, reconcile exact source and documentation import manifests, test/proof and final state for all original incomplete feature families. Review previously deferred G04/G05/G07/G08/G13 and prerequisites of original G10/G17; do not revive automatically. A source-backed bug outside the current acceptance set becomes a narrowly bounded future pair, not opportunistic source change.

Perform one reviewer pass over the cumulative accepted integration diff; one targeted cross-owner regression if source ownership actually overlaps. Produce and push final `PROGRAM_STATUS.md`, `ACCEPTED_CHECKPOINTS.md`, `FOLLOW_ON_CATALOG.md`, `RISKS_AND_DECISIONS.md`, `PR_SUMMARY.md`, `FINAL_HANDOFF.md` and a **single user manual-review checklist** for features actually included. Open a PR to *then-current master* only if permitted; if not, provide exact GitHub comparison and manual PR instructions. **Do not merge** unless user separately authorizes a specific PR and host permits it.

Final answer must report actual new branch tips, PR URL/state, accepted vs deferred work, source/test/native evidence, recorded models, unfinished Goal/Kickstart pairs with priorities, dependency gates, and whether the stale `continue.md` was actually fixed. Never promise future asynchronous completion or monitoring.

## 7. Your first action now

Read [G00 Goal](goals/G00-baseline-and-continuation/GOAL.md) and [Kickstart](goals/G00-baseline-and-continuation/KICKSTART.md) completely; verify master/source and bootstrap. Begin **G00 only**, then follow the fixed loop. Do not ask the user to select one goal or repeat constraints: the package contains the sequence and bounded authority already.
