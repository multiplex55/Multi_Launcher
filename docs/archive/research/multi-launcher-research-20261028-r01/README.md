# Multi Launcher sequential research — 20261028-r01

Two sequential research goals, one pinned application source: `59156d96adcc73939c7171487a0cb582e3286587`. Both goals are **RESEARCH_COMPLETE_WITH_LIMITATIONS**, with full packages, separate reviews, parent corrections and scoped final checks complete. The final branch, `codex/research-search-discovery-20261028-r01`, begins at Goal 01's exact closing documentation commit `3b79d73f12a791b8b077ed0bf7d6631bd885b68c` and contains both goals. [Status](PROGRAM_STATUS.md) is the execution ledger; [baseline](SOURCE_BASELINE.md) records source and model provenance.

## Start here

1. [Handoff](HANDOFF.md): findings, actual execution, limitations and ready-to-copy future requests.
2. [Canonical catalog](FOLLOW_ON_CATALOG.md) and [recommended next work](NEXT_IMPLEMENTATION_WAVE.md): four complete proposed packages and one hard dependency. No future goal is executed automatically.
3. [Decisions/limitations](DECISIONS_AND_LIMITATIONS.md): evidence boundaries, model disclosure, frozen study and review scope.
4. [Draft PR description](PR_SUMMARY.md): research-only summary; no PR opened or merged.

## Goal 01 — static architecture audit

[Goal index](01-static-audit/README.md), [source map](01-static-audit/SOURCE_MAP.md) and [coverage matrix](01-static-audit/COVERAGE_MATRIX.md) establish actual owners and risk-weighted depth: 11 bounded deep, 32 targeted and 62 survey buckets over 790 first-party Rust files. Inventory presence is not deep review.

Read [architecture/data flows](01-static-audit/ARCHITECTURE_AND_DATA_FLOWS.md), [input/UI ownership](01-static-audit/INPUT_AND_UI_OWNERSHIP.md), [lifecycle](01-static-audit/LIFECYCLE_AND_CONCURRENCY.md) and [persistence/file safety](01-static-audit/PERSISTENCE_AND_FILE_SAFETY.md) for the contracts and counterguards. [Findings](01-static-audit/FINDINGS.md), [decisions](01-static-audit/DECISION_RECORDS.md), [change cost](01-static-audit/BOUNDARIES_AND_CHANGE_COST.md) and [recommendations](01-static-audit/RECOMMENDATIONS.md) explain the two finite proposals and no-action outcomes.

[Canonical packages](01-static-audit/FOLLOW_ON_INDEX.md), [corrected search handoff](01-static-audit/SEARCH_HANDOFF.md) and [review/limits](01-static-audit/REVIEW_AND_LIMITATIONS.md) close the audit. Planner, author, reviewer and parent contributions remain in the goal's `roles/` tree. These artifacts are frozen on the second branch.

## Goal 02 — search and action discovery

[Goal index](02-search-discovery/README.md), [current architecture](02-search-discovery/CURRENT_SEARCH_ARCHITECTURE.md) and [catalog](02-search-discovery/COMMAND_AND_CAPABILITY_CATALOG.md) distinguish current routing/actions from proposed discovery. [Grammar](02-search-discovery/INTENT_TAXONOMY_AND_GRAMMAR.md), [ranking](02-search-discovery/RANKING_SPECIFICATION.md), [runtime](02-search-discovery/RUNTIME_AND_INTEGRATION_DESIGN.md), [keyboard UX](02-search-discovery/KEYBOARD_AND_RESULTS_UX.md), [safety](02-search-discovery/PRIVACY_SAFETY_AND_COMPATIBILITY.md) and [alternatives](02-search-discovery/ALTERNATIVES_AND_EXTERNAL_REFERENCES.md) contain the detailed design.

The [frozen protocol](02-search-discovery/evaluation/EVALUATION_PROTOCOL.md), [actual execution record](02-search-discovery/evaluation/M2A_EXECUTION.md), [results](02-search-discovery/evaluation/RESULTS.md) and [worked examples](02-search-discovery/EVALUATED_EXAMPLES.md) preserve the synthetic 300-case study, all original outputs and one source-backed label correction. P2 is only a conditional reference; no production search change is recommended.

[Recommendations](02-search-discovery/RECOMMENDATIONS.md), [follow-on index](02-search-discovery/FOLLOW_ON_INDEX.md), [canonical design amendments](02-search-discovery/CANONICAL_DESIGN_AMENDMENTS.md) and [cross-goal reconciliation](02-search-discovery/cross-goal/SEARCH_AUDIT_RECONCILIATION.md) resolve ownership/dependencies without rewriting frozen evidence. [Separate review and limitations](02-search-discovery/REVIEW_AND_LIMITATIONS.md) preserve the accepted conclusions and one bounded parent correction; no future implementation is authorized.

All future pairs are **PROPOSED_NOT_AUTHORIZED**. This program changes research artifacts and mutable plan progress only; it runs no Rust/application/native test or application benchmark.
