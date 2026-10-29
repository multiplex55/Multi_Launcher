# Integration intake — 20261029-r01

Observed 2026-10-29. Authority: user-invoked integration Kickstart, complete Goal/Kickstart, current AGENTS.md. Merge policy **MERGE_WHEN_GATES_PASS**. Seven milestones, one active branch/writer, parent owns all Git/PR operations. No source research or future feature implementation is authorized here.

| Pin | Fetched branch | Actual SHA | Commit date |
|---|---|---|---|
| CURRENT_MASTER | `master` | `59156d96adcc73939c7171487a0cb582e3286587` | 2026-10-28T16:08:37-04:00 |
| SOURCE_QUERY | `codex/query-terminal-escape` | `2db6c8a8e3c7b9c6de8c1f2c1dccd3ca0c2a77c5` | 2026-10-29T06:15:46-04:00 |
| SOURCE_AUDIT | `codex/research-static-audit-20261028-r01` | `3b79d73f12a791b8b077ed0bf7d6631bd885b68c` | 2026-10-28T18:17:41-04:00 |
| SOURCE_RESEARCH | `codex/research-search-discovery-20261028-r01` | `cc94dbf4f98f155bcdef81165f647cbafb472eb0` | 2026-10-28T21:15:50-04:00 |
| SOURCE_ADMISSION | `codex/discovery-admission-decision` | `cc6ed8ad91711c834631074a2cb1c96ec22fd6ae` | 2026-10-29T06:43:19-04:00 |
| SOURCE_DIFF | `codex/research-diff-mutation-authority-r01` | `124f929dbcc272396a329c0adf380d223ece07b9` | 2026-10-29T17:09:01-04:00 |

Origin is `https://github.com/multiplex55/Multi_Launcher.git`. One normal `git fetch origin` succeeded. Local master already equals fetched origin/master; no fast-forward update was necessary. Initial checkout was clean, sole worktree `G:/Repos/rust/Multi_Launcher`, branch `codex/research-diff-mutation-authority-r01` at SOURCE_DIFF. No merge/rebase/cherry-pick/revert/sequencer/index lock existed. No integration branch/run/PR already existed; GitHub open PR inventory was empty and latest 30 records contained no matching integration PR. [Host receipt](receipts/host-intake.json) records actual metadata and limitations.

`SOURCE_AUDIT` is an ancestor of `SOURCE_RESEARCH` (`git merge-base --is-ancestor` exit 0). Do not import the audit separately. SOURCE_RESEARCH has 133 changed paths against current master, all to be narrowed by the explicit research whitelist at M2-A. SOURCE_QUERY has exactly three changed Rust paths plus its own evidence and an obsolete queue snapshot; exclude that snapshot. Admission's old queue snapshot is also excluded. The authoritative final queue/G04 comes only from SOURCE_DIFF.

## Drift and exclusions

SOURCE_DIFF advanced from `52eec0137a3dc0571580ef8da4c01962481d0848` to `124f929dbcc272396a329c0adf380d223ece07b9`. The only additions are the invoked integration Goal/Kickstart; modifications are four `.codex` configuration/role files. The earlier completed Diff result and final queue remain preserved. Copy the integration contract pair at M0-A; exclude all configuration changes from both PRs. Preserve the source branch and user commits intact. Other four source tips and master equal the initial observations.

PR 1 branch is `codex/integrate-query-escape-20261029-r01`, created directly from CURRENT_MASTER with `--no-track`. It changes the exact three whitelisted Rust files, original fix evidence, this integration contract and first-phase receipts. PR 2 is `codex/integrate-research-docs-20261029-r01`: start from fetched post-PR1 master if merged, otherwise from the published PR1 tip and target that branch as stacked. Its diff must contain docs only against its actual base.

## Runtime and role attribution

At invocation the user-authored SOURCE_DIFF configuration requested parent/default Sol medium, planner/reviewer Sol high and implementer Luna max. The clean master-based integration branch instead contains its original Astra xhigh parent/implementer and Astra max planner/reviewer files. These differences are branch content, not edits by this integration; source configuration changes are deliberately not imported. No model settings or permissions are changed. Tool dispatch/configuration will be recorded for the separate reviewer; actual serving model/effective effort are unverified.

The existing explicit seven-milestone contract removes the need for another planning agent. Integration is predominantly parent-owned Git path imports; parent is the sole writer. Use one separate configured reviewer sequentially for PR1 and PR2. An unavailable role gets one recovery and an honestly labeled parent/available-role fallback. No repeated agent queue.

`gh` is absent. The installed GitHub connector can create/inspect/merge PRs, list complete changed paths and read checks/rules; use it. Merge commits are enabled. The visible active master ruleset prohibits deletion/non-fast-forward only. Legacy protection detail returns 403 through the integration; branch metadata reports legacy protection disabled with no required contexts. No bypass or setting change is used; check each real PR's host status and normal merge permission at its gate. Empty CI lists mean no checks reported.

## Verification scope

Read-only Git/source/receipt inspection, exact blob parity, bounded docs metadata checks and real GitHub state. Reuse original query proof only if current baseline and all three source blobs are exact. No automatic Cargo/Nextest/Clippy/prototype/native/application rerun. PR2 gets one standard-library metadata pass, 60s bound and at most one localized correction/recheck. No third PR solely for final merge receipts.

M0-A whitespace receipt: ordinary staged `git diff --check` reported Markdown two-space hard breaks in the supplied contract pair and progress. Removed the newly authored progress trailing spaces. The nine original immutable contract/Kickstart hard breaks remain byte-faithful; no contract body rewrite is permitted. Per-command `git -c core.whitespace=-blank-at-eol diff --cached --check` checks the remaining whitespace classes without changing repository configuration. This is a disclosed preserved-input exception, not a silent plain-check PASS.
