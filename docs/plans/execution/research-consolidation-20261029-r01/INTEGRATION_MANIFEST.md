# Integration manifest — completed records, unchanged content

Run `20261029-r01`; branch `codex/integrate-research-docs-20261029-r01`, **STACKED_ON_UNMERGED_PR1**. Actual base branch `codex/integrate-query-escape-20261029-r01`, published base `ed25f9dcc32f9bb74d2277d25d6fb62e292636c6`. Master remains `59156d96adcc73939c7171487a0cb582e3286587`. PR1 #1456 merge was blocked by automatic approval review, not a test failure; do not imply master integration.

M2-A `6f8de6cfa4d1e8bc82c2eabac41fb43299402049` is committed and pushed. M2-B adds the exact decision/final-queue whitelist. The [machine manifest](receipts/integration-imports.json) enumerates every imported file, source SHA, mode and Git blob; no whole source branch is merged.

| Group | Actual fetched source | Files | Imported paths |
|---|---|---:|---|
| research | `cc94dbf4f98f155bcdef81165f647cbafb472eb0` | 133 | `docs/research/multi-launcher-research-20261028-r01/`, `docs/plans/ai-research/01_GOAL_STATIC_ARCHITECTURE_AUDIT.md`, `docs/plans/ai-research/02_GOAL_SEARCH_ACTION_DISCOVERY.md`, `docs/plans/ai-research/ORCHESTRATOR_GOAL.md` |
| admission | `cc6ed8ad91711c834631074a2cb1c96ec22fd6ae` | 15 | `docs/implementation/SEARCH-F002-discovery-admission-consumer-proof/` |
| diff_and_final_queue | `124f929dbcc272396a329c0adf380d223ece07b9` | 23 | `docs/research/diff-mutation-authority-r01/`, `docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md`, `docs/plans/execution/research-follow-ons-20261029/stalls/G04-prerequisite-20261029T120120Z.md` |

Every selected source blob is retained exactly. Research root tree parity was established at M2-A; both M2-B source/index comparisons (`git diff --cached --quiet <source> -- <whitelist>`) returned zero. The final queue file is exclusively the actual SOURCE_DIFF version; the original query/admission interim snapshots remain only in original history. Source branch advancement to 124f929d adds the invoked integration contract/model settings; it does not change these selected result paths.

## Faithful outcomes

- Original Goals 01 and 02 retain **RESEARCH_COMPLETE_WITH_LIMITATIONS**, complete role/review/provenance, raw and canonical follow-on packages, 300-query v1/v2 dataset and all saved initial/corrective results. No scoring or source experiment is rerun. Original historical no-merge clauses and future authorization labels are preserved as evidence of their earlier scope.
- AUD-F001 is completed/verified/pushed in the historical ledger. PR1 carries its exact code/evidence; its actual merge remains blocked and is recorded in this integration's receipts, not by rewriting the historical ledger.
- SEARCH-F002 retains **H0 NO_CHANGE**, rejected `SEARCH-F002-consumer-v1`, canonical LF SHA-256 `8e26d00204f9e1ed703ef17b1cc1d79f1d99089f3e9a39842f2204ddba603013`, reviewed closing commit `012ed8afde88e89d360c3218e6cced130a5112fc` and later provenance receipt. No accepted consumer is fabricated.
- AUD-F002 retains **H0** best-effort path/metadata authority, unchanged native code, source/abstract/native distinctions and resolved separate review. The failure-information Goal/Kickstart remains **PROPOSED_NOT_AUTHORIZED**, unexecuted and outside the four-package catalog authority.
- SEARCH-F001 remains **DEFERRED_DEPENDENCY**, not implemented. Its G04 report is a deliberate failed-prerequisite disposition, not a failed compilation or lack of generic catalog selection.

## Administrative scope and omissions

Additional changes are confined to this execution root and **section 9 of the active integration Goal**, as the contract expressly requires its own mutable progress to be updated at every milestone. That single existing Goal path is an intentional administrative addition to PR2's illustrative path list; its immutable prefix and Kickstart remain exact. No historical studies or source templates are edited for this reporting purpose.

Do not separately import the audit branch, stale sibling ledgers, SOURCE_DIFF configuration changes, unrelated ai-research templates, or any production search/Diff feature. Existing GitHub/source-commit evidence links remain pinned to their historical meaning. No Cargo/Nextest/Clippy, app/native/benchmark, old checker/prototype, dependency or user-data operation occurred in M2-A/B.

M2-C owns the single bounded local-link/structured-data/source-scope metadata pass and independent docs review. Its receipt is not yet claimed. PR2 will be opened against the actual PR1 branch; approval/merge requires PR1 master merge, retarget, docs-only revalidation and normal host gates. No approval bypass or third PR is authorized.
