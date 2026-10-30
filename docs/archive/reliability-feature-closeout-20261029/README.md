# Multi Launcher — Sequential reliability and feature closeout package

**Deliverable:** one mega Goal, one orchestrator Kickstart, **11 individually executable Goals + 11 companion Kickstarts**, and reusable policy/templates. All are Markdown and designed to be copied under `docs/plans/reliability-feature-closeout-20261029/` in the repository. **No source was modified by creating this package.**

**Source at preparation:** merged `master` `56223fdc123396f4c41c48e0542506d912c6c634` (2026-10-29). The Kickstart verifies the actual new master and worktree before execution.

## Start here

1. [MEGA_GOAL.md](MEGA_GOAL.md) — authoritative scope, 11-goal sequencing, dependencies, Git/PR rules, model orchestration, stop handling and acceptance.
2. [ORCHESTRATOR_KICKSTART.md](ORCHESTRATOR_KICKSTART.md) — one entrypoint; execute each child sequentially, do not run concurrent source writers.
3. [SOURCE_AND_RISK_BASELINE.md](SOURCE_AND_RISK_BASELINE.md) — current-source evidence versus historical research conclusions.
4. [VERIFICATION_POLICY.md](VERIFICATION_POLICY.md) — deliberately bounded test, native proof, clean-up, safety/privacy budgets.
5. [PROGRESS_AND_STALL_TEMPLATES.md](PROGRESS_AND_STALL_TEMPLATES.md) — per-goal and parent ledgers, roles, stalls, follow-on packages.
6. [FOLLOW_ON_GOAL_TEMPLATE.md](FOLLOW_ON_GOAL_TEMPLATE.md) and [FOLLOW_ON_KICKSTART_TEMPLATE.md](FOLLOW_ON_KICKSTART_TEMPLATE.md) — fully specified future tasks for newly discovered actionable issues.

## Individual Goal/Kickstart pairs

| Goal | Scope | Contracts |
|---|---|---|
| G00 | Source Baseline, Stale Continuation Cleanup and Execution Ledger | [Goal](goals/G00-baseline-and-continuation/GOAL.md) · [Kickstart](goals/G00-baseline-and-continuation/KICKSTART.md) |
| G01 | Folder Diff: Preserve Destination Read Errors and Operation Feedback | [Goal](goals/G01-diff-failure-information/GOAL.md) · [Kickstart](goals/G01-diff-failure-information/KICKSTART.md) |
| G02 | G09 Table Utility: Full-File Publication Safety Foundation | [Goal](goals/G02-table-publication-safety/GOAL.md) · [Kickstart](goals/G02-table-publication-safety/KICKSTART.md) |
| G03 | G09 Table Utility: Conditional Full-File Operations and UX Closure | [Goal](goals/G03-table-full-file-operations/GOAL.md) · [Kickstart](goals/G03-table-full-file-operations/KICKSTART.md) |
| G04 | G12 Screenshot Redaction: Native Crop, Output and Interaction Acceptance | [Goal](goals/G04-screenshot-native-redaction/GOAL.md) · [Kickstart](goals/G04-screenshot-native-redaction/KICKSTART.md) |
| G05 | G16 Hot Folder: Safe Size, Age and Literal-Content Conditions | [Goal](goals/G05-hotfolder-condition-evaluator/GOAL.md) · [Kickstart](goals/G05-hotfolder-condition-evaluator/KICKSTART.md) |
| G06 | G11 Jump Mode: Finish Customization UI and Focus Regression Coverage | [Goal](goals/G06-jump-settings-and-focus/GOAL.md) · [Kickstart](goals/G06-jump-settings-and-focus/KICKSTART.md) |
| G07 | G14 ZIP: Bounded Extraction Writer and Safe UI Integration | [Goal](goals/G07-zip-extraction/GOAL.md) · [Kickstart](goals/G07-zip-extraction/KICKSTART.md) |
| G08 | G15 MkMacro: Bounded Run History and Conditional Safe Recipes | [Goal](goals/G08-mkmacro-run-history/GOAL.md) · [Kickstart](goals/G08-mkmacro-run-history/KICKSTART.md) |
| G09 | G06 Activity Center: Source-Grounded Native Retirement Acceptance | [Goal](goals/G09-activity-center-retirement/GOAL.md) · [Kickstart](goals/G09-activity-center-retirement/KICKSTART.md) |
| G10 | Residual Defects, Deferred Goals and Final Integration Closeout | [Goal](goals/G10-residual-defect-triage-integration/GOAL.md) · [Kickstart](goals/G10-residual-defect-triage-integration/KICKSTART.md) |

## Safe launch

Extract this ZIP at the root of `Multi_Launcher`, preserving the `docs/` folder. In Codex, supply the following one-line entry:

```text
Read docs/plans/reliability-feature-closeout-20261029/ORCHESTRATOR_KICKSTART.md and MEGA_GOAL.md completely and EXECUTE G00→G10 strictly sequentially, following all sibling Goal/Kickstart pairs, role/model policies, scoped tests, evidence and safe-stop rules. Start from verified current master, create a new branch for every goal, commit/push accepted checkpoints, integrate only reviewed safe work, publish durable status and follow-on Goal/Kickstart files, and prepare one final reviewable PR without merging master automatically. Never run parallel source goals or ask for manual checks mid-program.
```

The user can inspect progress after launch in `docs/plans/execution/multi-launcher-reliability-closeout-20261029-<RUN_ID>/PROGRAM_STATUS.md` and the mutable section of each child `GOAL.md`. No old Codex hidden state or the previous model/CI environment is necessary to understand what a task accomplished.

## Design caution

Not every historical feature is feasible to complete safely in the same session. The program explicitly distinguishes a complete implementation from accepted bounded partial work and **native proof unavailable**. It publishes finishable next-step packages instead of silently implementing incomplete/unsafe functionality. Search metadata/ranker/UI remains deliberately on hold from Astra's H0 decision. The new Diff info fix is first priority, and the previous `AUD-F001` tokenizer correction is **already merged**.
