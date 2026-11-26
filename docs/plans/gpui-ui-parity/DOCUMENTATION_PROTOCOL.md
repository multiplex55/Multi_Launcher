# Documentation and progress protocol

## One fact, one authoritative record

Use the supplied documents as an execution ledger. Avoid copying full checklists into every goal. Link source facts, GUI records, evidence and decisions by stable ID.

| Record | Required maintained content |
| --- | --- |
| README | Active goal/checkpoint, next action, links, last checkpoint |
| REQUIREMENTS | User scope, controlling clarifications, explicit changes |
| SOURCE_BASELINE | Planning reference and actual execution baseline; source/env differences |
| GUI_INVENTORY | Stable GUI IDs, parent/children, routes, modes, owner goal, discovery status |
| PARITY_MATRIX | Control/behavior requirement IDs, implementation and evidence, gaps |
| CORE_CONTRACTS | Real authoritative owners, contracts, adapter obligations and extraction map |
| FRONTEND_SELECTION | Parse/default/saved choice, existing-instance response, switch and recovery |
| DESIGN_SYSTEM | Recorded visual decisions, density, text/theme scales and visual acceptance |
| COMPONENT_MAPPING | Kit mappings, custom components, accessibility/input ownership |
| GOAL_REGISTER | Goal dependencies, ownership, checkpoint IDs and actual stage |
| EXECUTION_STRATEGY | Required implementation methodology, infrastructure priorities, batching, and verification cadence |
| PROGRESS | Correct counts, denominators, current work, pending native checks and revalidation |
| VALIDATION_STRATEGY | Relevant targets/filters, native requirements, broader-run triggers |
| VALIDATION_LOG | Exact commands, selected tests/counts, timings, binary/commit and real result |
| PERFORMANCE | Equivalent baseline/candidate measurements, practical budgets, differences |
| KNOWN_ISSUES | Baseline defects vs migration regressions; evidence and owner |
| DECISIONS | Rationale, alternatives where material, implications, date/owner |
| CHECKPOINTS | Git state, work completed, evidence applicability, exact continuation |
| BLOCKERS | Proven blocker, attempts, available independent work, smallest next step |
| FINAL_ACCEPTANCE | Every original criterion against final candidate and actual delivered state |
| gui/<id>.md | Complete individual GUI spec and acceptance |
| goals/<id>.md | Goal-specific implementation packets and actual checkpoint state |
| reviews/<id>.md | Bounded reviewer snapshot, findings, remedies and disposition |
| evidence/<id>.md | Reproducible scenario and safe evidence references |

Per-GUI records and evidence/review logs are created when their contents are known. Templates are available; do not create hundreds of empty instances. The initial top-level files contain real policy and preparation state, not fabricated execution receipts.

## Status model

Use stages Not started, Specified, In implementation, Implemented, Validation pending, Accepted. Track blocker separately so the last achieved stage is not lost. A goal may contain accepted GUI records and a blocked native criterion; it is not fully accepted.

Record source candidates separately from confirmed inventory entries. Count GUIs only after consistent granularity is decided: parent surfaces, independently operable children, and modes must have explicit relationships. Do not count one Rust file as one GUI.

Count implemented and accepted GUI IDs; report pending required checks, known regressions, blocked work and revalidation. Percentages require a denominator and an explanation that coverage is not effort. If discovery increases scope, update it openly.

## Evidence and invalidation

Every acceptance assertion links the relevant criterion, scenario/test, actual result, binary/commit identity and review decision. Group checks when one real scenario proves several criteria. Do not duplicate expensive scenarios for bookkeeping.

Later core/focus/component/theme/toolkit/persistence changes require impact analysis. Reopen affected criteria; retain evidence that remains applicable. At final reconciliation, explain whether the recorded scenario ran on the final candidate or why intervening changes do not invalidate it.

## Session and commit boundaries

At a meaningful checkpoint record exact next action, current branch/HEAD, current working-tree state, changed files, partial work, tests actually run, pending checks, dependencies and blockers. Commit natural coherent changes with stage tags. Record one push attempt/result; local progress remains valid after push failure.

Do not create endless commits to store a commit's own hash in itself. A tracked checkpoint can identify its predecessor and label; resolve its actual resulting SHA in the next record/final report.

Genuine stops require the current repository AGENTS stall-report location and fields. Link that report from BLOCKERS; do not create a competing operational ledger.
