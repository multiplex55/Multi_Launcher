# Multi Launcher — Two-Goal AI Research Package

**Prepared:** 2026-10-28  
**Purpose:** substantial AI reasoning, static source analysis, architecture and future-work design without a Windows Rust build campaign.

## Start here

Place this package's `docs/plans/ai-research/` directory in your repository, or let the orchestrator locate the supplied files and copy only this plan set after creating the first research branch. Start from your intended new master; the orchestrator discovers the actual latest source and does not reuse the old integration branch.

Give Codex this instruction:

```text
Read docs/plans/ai-research/ORCHESTRATOR_KICKSTART.md and its linked
ORCHESTRATOR_GOAL.md completely, then execute the two-goal research program.

Run Goal 01 Static Architecture Audit first, then Goal 02 Intelligent Search
and Action Discovery. Never run the two goals concurrently. Use one active
child role at a time and the documented fallback for agent limits.

Start Goal 01 from my actual new master on a fresh research branch. Start
Goal 02 on its own fresh branch from Goal 01's recorded closing documentation
commit so the final branch contains both goals' deliverables.

Do not modify production source or run Cargo, nextest, Clippy, native UI tests,
or application benchmarks. Produce the detailed docs, small isolated search
prototype/evaluation where available, and complete follow-on Goal/Kickstart
packages required by the contracts.

Record progress in each goal Markdown and the tracked program status. Commit
and normally push coherent checkpoints frequently. Preserve role/model
contributions and publish exact Markdown resume reports for genuine stops.

Do not let optional tools, unavailable models, child capacity, or unmeasured
runtime questions stop unrelated research: use the documented fallback,
qualified conclusion or bounded follow-on research spike. Do not invent
results, bypass permissions, or discard work.

Finish the research handoff and prioritized future-work catalog. Do not execute
the generated future goals or merge master automatically.
```

This launches the research program when submitted to a capable execution session. Saving a file alone does not start Codex. No work has been scheduled or started by this package's creation.

## Files included

| File | Purpose |
|---|---|
| [ORCHESTRATOR_GOAL.md](ORCHESTRATOR_GOAL.md) | Program contract, branch chain, sequential roles, fallback, progress, follow-on completeness |
| [ORCHESTRATOR_KICKSTART.md](ORCHESTRATOR_KICKSTART.md) | One launch prompt that facilitates both goals |
| [01_GOAL_STATIC_ARCHITECTURE_AUDIT.md](01_GOAL_STATIC_ARCHITECTURE_AUDIT.md) | Detailed static audit scope, evidence, findings, milestones and follow-on packages |
| [01_KICKSTART_STATIC_ARCHITECTURE_AUDIT.md](01_KICKSTART_STATIC_ARCHITECTURE_AUDIT.md) | Execution instructions for the first goal |
| [02_GOAL_SEARCH_ACTION_DISCOVERY.md](02_GOAL_SEARCH_ACTION_DISCOVERY.md) | Search architecture, catalog, ranking, UX, safety, synthetic evaluation and future implementation plans |
| [02_KICKSTART_SEARCH_ACTION_DISCOVERY.md](02_KICKSTART_SEARCH_ACTION_DISCOVERY.md) | Execution instructions for the second goal |
| [MODEL_ASSIGNMENTS.md](MODEL_ASSIGNMENTS.md) | Quality-first model recommendations, observed existing configs, safe selection/fallback guidance |
| [FOLLOW_ON_GOAL_TEMPLATE.md](FOLLOW_ON_GOAL_TEMPLATE.md) | Full engineering specification required for each actionable recommendation |
| [FOLLOW_ON_KICKSTART_TEMPLATE.md](FOLLOW_ON_KICKSTART_TEMPLATE.md) | Companion execution contract for each future goal |
| [FINDINGS_AND_PROGRESS_TEMPLATES.md](FINDINGS_AND_PROGRESS_TEMPLATES.md) | Source evidence, role records, recommendation mapping, mutable progress and stop reporting |

This README is the eleventh Markdown file. The two main goals plus their two kickstarts and the orchestrator goal are the five requested core documents. The extra orchestrator kickstart and supporting guides make the package launchable and consistent.

## Recommended models

For the thought-heavy configuration requested:

| Role | Model | Effort |
|---|---|---|
| Orchestrator | GPT-6 Astra | xhigh |
| Planner | GPT-6 Astra | max |
| Research implementer/author | GPT-6 Astra | xhigh |
| Reviewer | GPT-6 Astra | max |

The deliberate lower-cost option is GPT-6.1 Sol xhigh for the research author, while retaining Astra for planning and review. Model names/effort are current recommendations, not a guarantee of account/client availability. The model guide includes official references and the actual repository role configuration inspected during preparation. No `.codex` configuration is changed by this package.

## How the branch chain works

```text
Actual new master at program start
└─ codex/research-static-audit-<RUN_ID>
   ├─ baseline, source map, audit, findings and full follow-on plans
   └─ codex/research-search-discovery-<RUN_ID>
      └─ search design, prototype, evaluation, full follow-ons and program handoff
```

Each goal gets a fresh branch and frequent checkpoint commits. Goal 02 carries Goal 01's documentation forward; the program does not create another set of independent sibling branches requiring a later integration effort. No production source change or automatic master merge is authorized.

## Where results will appear

Paths below are **future execution outputs**, not already completed research:

```text
docs/research/multi-launcher-research-<RUN_ID>/
├─ README.md
├─ PROGRAM_STATUS.md
├─ SOURCE_BASELINE.md
├─ 01-static-audit/
│  ├─ SOURCE_MAP.md, COVERAGE_MATRIX.md, detailed audit reports
│  ├─ FINDINGS.md, findings/, RECOMMENDATIONS.md
│  ├─ roles/{planner,implementer,reviewer,orchestrator}/
│  │  ├─ ROLE_RECORD.md and substantive analysis/review
│  │  └─ follow-on/<candidate-id>/{GOAL.md,KICKSTART.md}
│  ├─ follow-on/AUD-F###-<slug>/{GOAL.md,KICKSTART.md}
│  ├─ FOLLOW_ON_INDEX.md
│  └─ SEARCH_HANDOFF.md
├─ 02-search-discovery/
│  ├─ architecture, ranking, UX, safety and decision Markdown
│  ├─ catalog/, prototype/, evaluation/
│  ├─ roles/{planner,implementer,reviewer,orchestrator}/
│  ├─ follow-on/SEARCH-F###-<slug>/{GOAL.md,KICKSTART.md}
│  ├─ FOLLOW_ON_INDEX.md
│  └─ cross-goal/
├─ FOLLOW_ON_CATALOG.md
├─ NEXT_IMPLEMENTATION_WAVE.md
├─ DECISIONS_AND_LIMITATIONS.md
├─ HANDOFF.md
├─ PR_SUMMARY.md
└─ stalls/<goal>-<checkpoint>-<UTC>.md
```

Each main Goal file also has an `Execution progress — mutable` section. The executor fills branches, milestones, actual output links, known commits/pushes, limitations and next actions there. The program status handles orchestration, while the goal file records detailed milestone state.

## What “do not stall” means here

Ordinary research limitations do not halt the queue. Missing optional prototype runs become NOT_RUN results with reproducible code. Unproved runtime questions become precise future investigations. Agent thread limits use sequential retirement and a labeled fallback. Push failure leaves local commits and an honest visibility record.

This does not promise uninterrupted execution through unavailable permissions, a broken repository or session termination. Real safety/authority interruptions leave a recoverable Markdown state. No fabricated results, unsafe shortcuts or background-work claims are permitted.

## What each follow-on contains

Every actionable recommendation gets a complete Goal/Kickstart pair describing what should change, why, exactly which owners/callers are affected, proposed data/control flow and interfaces, migration and rollback, compatibility, small commit milestones, future focused testing, confidence/benefit, model assignments and progress reporting.

Role proposals are preserved; duplicate recommendations are consolidated into one canonical package. Speculative opportunities receive bounded research-spike pairs. Already-covered or rejected ideas get explicit dispositions rather than fake implementation tasks. Future packages remain proposed until selected for execution.

## What this package does not claim

The audit, search experiments and findings have not been executed merely by creating these contracts. The package contains no fabricated research result, no claim that PR #1455 has merged, no model configuration change, and no production implementation. Source/model navigation facts are preparation-time observations; the execution baseline is resolved from the user's actual new master.
