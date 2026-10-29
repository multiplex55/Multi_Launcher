# M3-A — two review-ready PRs; external merge approval remains

Observed/fetched master **`59156d96adcc73939c7171487a0cb582e3286587`**, local master identical, 2026-10-29 21:39:02 UTC. **Neither PR is merged.** This is the authorized contract's accurate blocked/review-ready fallback, not a claim that master was integrated. Final direct approval is needed because automatic approval review rejected the first normal merge.

| PR | Actual head / base | State | Review / host checks | Actual merge SHA / next action |
|---|---|---|---|---|
| [#1456 query fix](https://github.com/multiplex55/Multi_Launcher/pull/1456) | `codex/integrate-query-escape-20261029-r01` / `master`; head `ed25f9dcc32f9bb74d2277d25d6fb62e292636c6` | OPEN, clean host mergeability, merge BLOCKED_EXTERNAL | Separate integration review ACCEPTED; zero GitHub reviews, statuses or check runs reported | No actual merge SHA; direct approval of normal master merge is the remaining gate |
| [#1457 research docs](https://github.com/multiplex55/Multi_Launcher/pull/1457) | `codex/integrate-research-docs-20261029-r01` / PR1 branch at `ed25f9dc`; checked candidate head `303b75b00e161f95abeeb0047c34470a92690b05` | OPEN, clean, STACKED_ON_UNMERGED_PR1; 186 host paths, all docs | Separate review ACCEPTED; metadata PASS; zero GitHub reviews, statuses or check runs reported | No actual merge SHA; after PR1 merge retarget to master, verify docs-only/host gates, then normally merge |

This table is the real pre-closing-receipt snapshot. The final administrative commit advances only PR2's handoff/progress receipts; its exact resulting head/push is reported by Git and the final user response. No recursive SHA commit or third PR is needed.

## Source preservation and implementation scope

PR1 contains exactly the verified correction/test blobs in `src/common/query.rs`, `src/plugin.rs`, `src/plugins/snippets.rs`, plus nine original evidence files and integration receipts. Same baseline and exact Git blobs justify reusing historical **11/11 focused Nextest PASS**, five new tests, scoped library Clippy PASS and touched-file formatting PASS. These were not rerun in this integration. The original warnings are outside the source patch. A plain source-import whitespace check passed; supplied immutable contract hard breaks were preserved and disclosed at M0, with the remaining whitespace classes checked separately. No full-suite or native acceptance is implied.

PR2 changes docs only relative to its actual PR1 base. The [manifest](INTEGRATION_MANIFEST.md) and [171-file blob map](receipts/integration-imports.json) retain the complete original research tree (133 files including three mutable planning ledgers), admission decision (15) and Diff/final queue (23). Audit is already a research ancestor, so no duplicate import. Stale query/admission ledger snapshots and all user model-configuration changes were excluded. Original source pins, frozen results, datasets, raw/canonical proposals, role attributions and historical no-merge clauses remain unchanged.

Both original studies remain RESEARCH_COMPLETE_WITH_LIMITATIONS. The catalog remains **three complete / SEARCH-F001 DEFERRED_DEPENDENCY, not implemented**. SEARCH-F002 stays H0 NO_CHANGE/CONTRACT_NOT_ACCEPTED; AUD-F002 stays H0 with native limits. The new Diff failure-information Goal/Kickstart remains PROPOSED_NOT_AUTHORIZED and unexecuted. The historical final ledger/G04 is exact from the latest verified Diff source; new integration facts are recorded here rather than rewriting that history.

## Actual new verification and missing integration

One 60-second-bounded standard-library metadata pass: **1497 assertions PASS**, 171 exact imports, 137 Markdown files, 924 local links, 63 anchors, 34 JSON files and 14 JSONL files/4200 records. Both 300-case corpus versions and 1800 original plus 1800 corrective result records are preserved. Runner **2.547s/60s**, exit0, output drained, no termination needed. Zero corrections/rechecks. No prototype/scoring, old study checker, Cargo/rustc/Nextest/Clippy/rustfmt, application/native/benchmark/provider/dependency or live-data operation ran. Separate PR1/PR2 reviews were accepted without corrections. Role configuration/dispatch is recorded; serving identity/effective effort remain unverified.

Final local Git/host checks confirm master still has the three old source blobs and lacks the owned query-proof root, completed research root, both decision roots and final four-package ledger. Both integration branches have the corrected three blobs; PR2 inherits them solely from its base. Master contains neither this correction nor these completed research/follow-on results. [Pre-closing reconciliation](receipts/final-reconciliation-before-closing.json), [PR1 state](PR1_STATE.md) and [PR2 state](PR2_STATE.md) preserve actual observations and limitations.

At preparation HEAD/upstream both `303b75b00e161f95abeeb0047c34470a92690b05`, branch `codex/integrate-research-docs-20261029-r01`, sole worktree clean before these receipts. All prior checkpoints were normally pushed. No conflicts, merges, history rewrites, force pushes, branch deletions, configuration changes or third PR occurred. No child/process/fixture cleanup is pending. The closing receipt gets one normal push and final worktree/upstream/host check.

## Exact remaining action

Approve **normal merge of #1456 into master, then retarget #1457 to master, recheck its docs-only diff and normal host gates, and normally merge #1457**. The risk-bearing action is updating the protected/default master branch. Automatic approval review rejected #1456 because it cited the earlier no-automatic-merge instruction and did not recognize later authorization for that exact merge. Its [unchanged diagnostic](stalls/G03-M1-B-20261029T212244Z.md) is preserved. No alternate route or bypass was tried.

Do not execute the proposed Diff fix or deferred metadata, open a third PR, delete branches or start a broad test campaign during this continuation. If approval is not supplied, both concrete PRs remain review-ready in the safe stacked topology.
