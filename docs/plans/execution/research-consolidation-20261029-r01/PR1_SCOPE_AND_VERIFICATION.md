# PR 1 — exact query-fix integration

Run `20261029-r01`; source/master baseline `59156d96adcc73939c7171487a0cb582e3286587`. M0-A `762ce52d5aabe52fc6006dff30430a63d97cccc6` was committed and pushed. Branch `codex/integrate-query-escape-20261029-r01`, intended base `master`.

## Change and parity

The scanner already records a backslash in `RawToken.raw`. At pending-escape EOF, the original source appends that same character again. The imported correction removes that raw append while retaining decoded-value behavior. It adds five meaningful regression tests covering raw/decoded tokens and filters, synchronous/snapshot ActionFilters transport, matching-prefix Literal input, and pure snippet reconstruction/no-tag preservation. No parser grammar or provider policy changes.

Only three Rust paths are imported: `src/common/query.rs`, `src/plugin.rs`, `src/plugins/snippets.rs`. The [parity receipt](receipts/pr1-source-parity.json) confirms their exact Git blob equality to source `2db6c8a8e3c7b9c6de8c1f2c1dccd3ca0c2a77c5`, and all nine files under `docs/plans/execution/standalone/preserve-terminal-query-escape/` are exact too. Current master equals that source's original base; no intervening production change requires reconciliation. This is a tree transplant, not a claim that the original source commit is an ancestor of the integration commit.

The source branch's obsolete `research-follow-ons-20261029/INTEGRATION_STATUS.md` is absent from the index and deliberately excluded. PR 2 will import the final ledger from actual SOURCE_DIFF. No manifest, dependency, config, native/app, source-research or unrelated code change belongs to PR 1.

## Reused historical proof and actual new checks

Read the complete [original verification](../standalone/preserve-terminal-query-escape/VERIFICATION.md) and [original independent review](../standalone/preserve-terminal-query-escape/roles/reviewer/REVIEW.md).

| Evidence | Actual status |
|---|---|
| Original selected-target compile/list | PASS, 434.594s/1200s; exact 11-case inventory |
| Original focused Nextest run | **11/11 PASS**, six existing plus five new; test execution 0.102s, process 2.266s/120s |
| Original scoped library Clippy | PASS, 90.266s/600s; unchanged-code warnings documented, no broad lint claim |
| Original touched-file rustfmt | PASS, edition 2024 with skip_children |
| Original independent implementation review | ACCEPTED, no substantive correction |
| New integration parity/source check | PASS: three Rust blobs and nine owned evidence files exact; same original baseline; obsolete ledger excluded |
| New staged `git diff --check` after whitelisted import | PASS, exit 0 |
| New Cargo/Nextest/Clippy/rustfmt/native/app/prototype runs | **NOT_RUN**; exact-source/same-baseline reuse is expressly required by this integration contract |
| New independent integration review | **ACCEPTED**, no substantive findings or corrections; [review](roles/reviewer/PR1_REVIEW.md) and [provenance](roles/reviewer/ROLE_RECORD.md) |

The prior ignored raw logs are historical local artifacts identified by hashes in the retained verification; this integration does not fabricate new raw output or reinterpret those runs as executed on its branch. No full-suite, native or application acceptance is claimed. Inert manager/pure snippet fixtures remain exact; no new application fixture was constructed.

M0-A's immutable supplied contract pair contains nine intentional Markdown hard-break lines. Ordinary cumulative whitespace inspection discloses those preserved lines; the actual source import's staged plain check is clean. A per-command `core.whitespace=-blank-at-eol` check is available for cumulative docs without changing config. No immutable contract body is rewritten to hide that input formatting.

Parent owns Git/PR/merge. A separate read-only reviewer checks the candidate source and evidence once; corrections, if any, must stay within the narrow import. No production source is authored anew by the parent.
