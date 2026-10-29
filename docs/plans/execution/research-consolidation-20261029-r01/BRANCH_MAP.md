# Integration branch map — 20261029-r01

- Initial CURRENT_MASTER: `59156d96adcc73939c7171487a0cb582e3286587` (local and fetched remote equal).
- PR1: `codex/integrate-query-escape-20261029-r01` -> `master`; fresh from CURRENT_MASTER. M0-A adds contract/receipts only. M1-A imports exact query-fix paths, not its branch history or stale queue.
- PR2 planned: `codex/integrate-research-docs-20261029-r01`. If PR1 merges, create from its new fetched master. Otherwise stack on PR1's published tip and target the PR1 branch. Do not include unapproved code in a master-targeted docs PR.
- Original source branches remain intact. SOURCE_AUDIT is already included in SOURCE_RESEARCH; no separate audit import. No force updates, branch deletion, reset, rebase or configuration import.

See [intake](INTAKE_AND_PROVENANCE.md) for exact source pins/dates and [Goal](../../research-merge-20261029/GOAL_RESEARCH_INTEGRATION_AND_MERGE.md) for the primary progress ledger. Actual PR/base/head/merge identities are appended at their checkpoint, never guessed.
