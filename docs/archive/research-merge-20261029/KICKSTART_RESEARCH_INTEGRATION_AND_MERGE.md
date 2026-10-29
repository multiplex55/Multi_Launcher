# KICKSTART — Execute the two-PR Multi Launcher research/follow-on integration

**Read this file and [GOAL_RESEARCH_INTEGRATION_AND_MERGE.md](GOAL_RESEARCH_INTEGRATION_AND_MERGE.md) in full, then execute the Goal's seven milestones in order.** This Kickstart is an execution instruction for **integration and merging**, not a request for another plan. The user will complete one manual review at the end where repository approval rules permit. Never ask for routine confirmation between milestones, perform unbounded test campaigns, or stop the entire task because one PR cannot merge immediately.

**Repository:** `multiplex55/Multi_Launcher`, Windows local worktree normally `G:\Repos\rust\Multi_Launcher`.  
**Goal directory:** `docs/plans/research-merge-20261029/`.  
**Historical baseline (verification only):** `59156d96adcc73939c7171487a0cb582e3286587`.  
**Default action policy:** `MERGE_WHEN_GATES_PASS` — create both PRs; merge them **sequentially** when current user invocation authorizes this work and all host-required reviews/checks/permissions are satisfied. No admin bypass, force merge, stale-branch assumptions or private permission changes. On an external merge block, continue to create the stacked docs PR and finish with exact next actions rather than asking for manual intervention mid-plan.

## 0. Resolve current reality before doing anything

1. Read repository `AGENTS.md`, any path-relevant nested instructions, this Kickstart and entire Goal. The Goal's `Execution progress — mutable` section is the primary ledger; continue an existing matching run rather than starting another. Historical research Goals and their no-merge clauses applied to their earlier tasks; the active contract is **this** specific integration.
2. Verify Git remote identity, current branch, local/remote master, existing PRs (open or merged), worktree/index state, ongoing Git operation markers, source branches and SHA ancestry. Use a clean isolated worktree if user changes already exist. Do not stash/reset/discard other work.
3. Perform **one normal fetch** of `origin` (including named source branches, or fetch those refs explicitly under a narrow refspec) and safe fast-forward of local master only if allowed. Never rewrite a diverged local master to make its name match the remote. Treat actual fetched `origin/master` as `CURRENT_MASTER`.
4. Re-resolve, record and compare these inputs. The given SHAs are **initial observations, not mandatory current heads**:

   | Source purpose | Branch | Earlier observed tip |
   |---|---|---|
   | Query fix | `codex/query-terminal-escape` | `2db6c8a8e3c7b9c6de8c1f2c1dccd3ca0c2a77c5` |
   | Original two-goal research | `codex/research-search-discovery-20261028-r01` | `cc94dbf4f98f155bcdef81165f647cbafb472eb0` |
   | Separate audit ancestor | `codex/research-static-audit-20261028-r01` | `3b79d73f12a791b8b077ed0bf7d6631bd885b68c` |
   | Admission decision | `codex/discovery-admission-decision` | `cc6ed8ad91711c834631074a2cb1c96ec22fd6ae` |
   | Diff decision and final queue | `codex/research-diff-mutation-authority-r01` | `52eec0137a3dc0571580ef8da4c01962481d0848` |

5. Confirm the search-research tip already contains the audit closing commit, and the final Diff tip contains the most recent authoritative `INTEGRATION_STATUS.md`. Compare all relevant trees/changed paths with actual current master. Don't infer ancestry from branch name or a prior chat.
6. Record `RUN_ID`, current remote source SHAs, initial current-master SHA, source/outcome map, runtime/agent settings observed, and whether a matching PR already exists. Put artifacts under `docs/plans/execution/research-consolidation-<RUN_ID>/`.

Useful read-only commands to adapt to the live environment (not blindly execute against an unsafe worktree):

```powershell
git remote -v
git status --short --branch
git worktree list
git branch -a -vv
git fetch origin
git rev-parse origin/master
git log -1 --format='%H %s' origin/master
git merge-base --is-ancestor origin/codex/research-static-audit-20261028-r01 origin/codex/research-search-discovery-20261028-r01
```

GitHub CLI, if installed and authenticated, can manage PRs:

```powershell
gh auth status
gh pr list --repo multiplex55/Multi_Launcher --state all --limit 30
```

If `gh` is unavailable, **still finish branches and PR body files**. Use an authorized GitHub connector/CLI if available, or leave an exact new-PR comparison link and state `PR_CREATION_BLOCKED` without inventing a PR number. No repeated authentication loops.

## 1. M0-A — Fresh query-integration branch and checkpoint

After safely resolving current master, create/resume `codex/integrate-query-escape-<RUN_ID>` from that exact current master. If local master is safe to update, fast-forward it first; otherwise create from `origin/master` in an isolated worktree. The parent/orchestrator alone owns git and PR operations; use one active implementation writer and one separate reviewer when available.

Copy/persist this Goal and Kickstart into the required repository path if the user installed the package elsewhere. Initialize `INTAKE_AND_PROVENANCE.md`, `BRANCH_MAP.md`, and `STATUS.md`. Log the exact source SHAs and initial source branch relationships, expected PR 1/PR 2 scopes and the `MERGE_WHEN_GATES_PASS` rule. Update **the Goal itself** (mutable progress section only) and commit/push this coherent documentation baseline.

**No production change during M0-A.** If the fresh branch cannot be safely created due to genuine Git/user-work conflict, write a precise stop report and do not risk the worktree.

## 2. M1-A — Import the tested terminal-escape correction

Read the original implementation proof at:

```text
docs/plans/execution/standalone/preserve-terminal-query-escape/VERIFICATION.md
```

on `SOURCE_QUERY`, plus the original independent reviewer record. Confirm **11/11 selected tests passed**, five are new, scoped library Clippy passed, and only three Rust paths were affected. The prior test evidence is historical but reusable **only** for identical code/baseline.

On the new clean PR 1 integration branch, import **whitelisted files**, not a full merge of the original branch. Conceptual Git commands, after validation:

```powershell
# Substitute a freshly verified SHA for <SOURCE_QUERY_SHA>.
git restore --source=<SOURCE_QUERY_SHA> -- src/common/query.rs src/plugin.rs src/plugins/snippets.rs docs/plans/execution/standalone/preserve-terminal-query-escape/

git diff --check
git diff --stat
git diff -- src/common/query.rs src/plugin.rs src/plugins/snippets.rs
```

**Explicitly exclude** the query branch's earlier copy of `docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md`. The authoritative version will come from the final Diff branch in PR 2.

Validate the exact three Rust blob IDs versus `SOURCE_QUERY`; if current master has advanced and restoring would overwrite newer work, stop and inspect the specific divergence, preserving current behavior and running only the narrow necessary tests. No full suite. When parity and original verification are sound, **do not recompile the same Rust application merely to recreate old receipts**. Record `PR1_SCOPE_AND_VERIFICATION.md`, including actual checks and which original evidence is being reused, and obtain a single substantive independent review.

Commit `fix(query): [M1-A] integrate verified terminal-escape correction`, record source provenance in the commit body, push once. If the fix is already in master, document the evidence-backed no-op and omit an empty PR; continue documentation consolidation rather than manufacturing code changes.

## 3. M1-B — Open PR 1 and merge it when permitted

Prepare `PR1_BODY.md` with source parity, the exact three Rust paths, five new regression tests, original **11/11 Nextest PASS**, scoped Clippy, focused review and no broad test claim. Check the actual PR diff. Create or resume a PR:

```powershell
# Only when gh authentication and remote state were verified:
gh pr create --repo multiplex55/Multi_Launcher --base master --head <ACTUAL_PR1_BRANCH> --title "fix(query): preserve terminal escapes in raw search tokens" --body-file <ABSOLUTE_OR_WORKTREE_PATH_TO_PR1_BODY>
```

Do not invent the PR number. Record the real URL/base/head, `gh pr view` evidence, host checks/reviews and GitHub status. If host-required checks/approvals and scope review are satisfied and the user is executing this merge Kickstart, use the normal repository-supported PR merge (prefer merge commit; **never** `--admin` or force). Record actual merge SHA and refetch `origin/master`.

If a required approval, review, check, GitHub permission or PR creation is genuinely unavailable, **do not pause the whole goal**. Save accurate `PR1_STATE.md` and `BLOCKED_EXTERNAL` report, then proceed with a **stacked** documentation branch based on PR 1's published integration HEAD. Do not pretend that PR 1 merged.

## 4. M2-A — Import both completed research goals, without duplicate audit import

Create/resume fresh `codex/integrate-research-docs-<RUN_ID>`:

- If PR 1 **merged**, base it on the *new* fetched `origin/master`.
- If PR 1 **not merged**, base it on PR 1's **published integration branch tip**. PR 2 will then target that branch, explicitly stacked, not master.

Import exactly these source-research paths after confirming no collision with current master:

```powershell
git restore --source=<SOURCE_RESEARCH_SHA> -- docs/research/multi-launcher-research-20261028-r01/ docs/plans/ai-research/01_GOAL_STATIC_ARCHITECTURE_AUDIT.md docs/plans/ai-research/02_GOAL_SEARCH_ACTION_DISCOVERY.md docs/plans/ai-research/ORCHESTRATOR_GOAL.md
```

Do not import/merge the separate audit branch; it is already included. Do not overwrite unrelated original templates in `docs/plans/ai-research/`, modify experiment results or run the Python prototype again.

Verify the research status remains `RESEARCH_COMPLETE_WITH_LIMITATIONS` for **both** goals. Preserve the evaluation's 300 synthetic cases, all versioned labels and 1,800 preserved predictions, all raw/canonical follow-on packages, source maps, reviewer records and model-attribution caveats. Commit/push M2-A.

## 5. M2-B — Import both decisions and the **final** four-package ledger

From `SOURCE_ADMISSION`, import only:

```text
docs/implementation/SEARCH-F002-discovery-admission-consumer-proof/
```

From `SOURCE_DIFF`, import only:

```text
docs/research/diff-mutation-authority-r01/
docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md
docs/plans/execution/research-follow-ons-20261029/stalls/G04-prerequisite-20261029T120120Z.md
```

The **last Diff branch** version of `INTEGRATION_STATUS.md` is authoritative. Require the following exact *substantive* dispositions (wording may reflect actual source):

| Package | Must remain true |
|---|---|
| AUD-F001 | Verified Rust correction completed and pushed; merge disposition separately recorded here |
| SEARCH-F002 | H0 NO_CHANGE completed; consumer contract `CONTRACT_NOT_ACCEPTED` |
| AUD-F002 | H0 best-effort mutation authority research completed, no native code change |
| SEARCH-F001 | `DEFERRED_DEPENDENCY`, **not implemented**; don't build unused metadata |

Retain the separate Diff failure-information Goal/Kickstart as **new proposed-only future work**; do not execute it. Preserve the prerequisite G04 report; it was a deliberate dependency decision, not a failed compilation.

Write `INTEGRATION_MANIFEST.md` with exact imported files/source SHAs and any intentional omission. Commit/push M2-B once, after reviewer scope check if useful. Do not pull any whole source branch into the docs integration tip.

## 6. M2-C — Docs-only verification, PR 2 creation and merge

Run **only bounded, inexpensive document checks**: check relative links and key references, parse affected JSON/JSONL using a standard-library tool where appropriate, check source tree presence/hashes, `git diff --check`, and inspect the path diff against the **actual PR 2 base**. Do **not** rerun the synthetic scoring experiment or Cargo/Nextest/Clippy. One metadata check and at most one localized correction/recheck; record precise results.

The diff for PR 2 **must contain only paths under `docs/`** relative to its actual base. Reject production Rust, config and manifests. Reviewer checks that historical records were preserved, authoritative status is final, SEARCH-F001 deferred, the new Diff Goal is proposed only, both research goals complete, and PR 2 does not accidentally include unmerged PR 1 code.

Prepare `PR2_BODY.md` and open/resume PR 2:

```powershell
# If PR 1 was merged, base PR 2 on master.
# Otherwise, base PR 2 on the verified PR 1 integration branch.
gh pr create --repo multiplex55/Multi_Launcher --base <ACTUAL_DOCS_PR_BASE> --head <ACTUAL_PR2_BRANCH> --title "docs(research): integrate completed studies and follow-on decisions" --body-file <ABSOLUTE_OR_WORKTREE_PATH_TO_PR2_BODY>
```

If PR 1 merged, host checks/review pass, and the PR diff is docs-only against updated master, merge normally. If PR 2 is **stacked** on unmerged PR 1, leave it stacked until PR 1 merges; after that retarget to master, re-check its master diff for docs-only scope, and merge under the same gates. Do not execute a `gh pr merge --admin` and do not merge code accidentally via an incorrectly based documentation PR.

If host permissions or required approvals block it, preserve a review-ready PR and record its exact URL/required action. No false CI claims: zero check runs means **no checks reported**.

## 7. M3-A — Reconcile the actual final master and produce the final report

If both PRs merged, fetch master once and verify the corrected raw-token code plus completed research, admission and Diff documentation and final queue ledger are present. If either PR is pending, state precisely what master does **not** yet contain.

Report this table to the user, with actual GitHub data only:

| PR | Head/base | Number/URL | Actual status | Checks/review | Merge SHA or exact next action |
|---|---|---|---|---|---|
| 1 — query fix | ... | ... | ... | ... | ... |
| 2 — research docs | ... | ... | ... | ... | ... |

Report source path scope, reused historic 11/11 and Clippy result, actual new checks or `NOT_RUN`, relevant link validation, retained 3-complete/1-deferred catalog, new proposed Diff follow-on, worktree/upstream status and last actual master SHA. Never fabricate final commit/merge SHA or claim a PR appeared in GitHub because a local `gh` command was intended.

**Stop after this integration.** Do not automatically authorize the proposed Diff failure-information fix, unblocked search discovery, another full test campaign, a new PR, unrelated merges, or branch deletion.

---

## 8. Non-stalling checkpoints and exact recovery

- **After every coherent milestone:** update the Goal's mutable progress, write one concise linked receipt, inspect staged file scope, commit locally, and attempt one normal push. A push failure does not invalidate a local commit; mark its remote availability accurately.
- **One writer at a time.** Planner/reviewer may perform read-only bounded reviews, but do not spawn concurrent branch writers or a large agent queue. Use the current configured Astra roles or a documented available model. Do not edit model configs.
- **If a child tool/model is unavailable:** one recovery attempt, then parent or available authorized role finishes the bounded checkpoint; mark review independence honestly.
- **If a PR cannot merge:** save the PR or pushed branch, host error, required external approval/check, and resume instructions. Continue PR 2 as stacked if safe. An external gate is not permission to override protection.
- **If a real Git conflict or dirty-worktree safety issue occurs:** preserve work and record `docs/plans/execution/research-consolidation-<RUN_ID>/stalls/<MILESTONE>-<UTC>.md`; no force reset, destructive stash or lossy conflict resolution. If the affected import can be safely skipped with explicit limitation and the remaining documentation is independently complete, continue; otherwise stop the unsafe operation only.
- **If an outdated source branch conflicts with updated master:** use reviewed path-level reconciliation and a narrowly justified affected-test check. Never blindly overwrite updated source or repeatedly rebuild until something passes.
- **Do not require the user to perform a manual check mid-plan.** At the end, give the actual PR state and one clear action if protected-branch approval remains necessary.

## Completion criteria

- Both original research goals, the three finished follow-on results, the one deferred dependency and the new proposed-only Diff improvement are faithfully represented.
- One narrow code-fix PR and one docs-only research PR are opened and merged **if host gates permit**, otherwise preserved as review-ready/stacked with an exact next step.
- No unnecessary Rust build/test campaign, no accidental source changes on docs PR, no search UI/ranker/metadata implementation, no force update/branch deletion, and no invented verification/merge success.
- The final answer distinguishes `MERGED` from `READY`, and records all actual PR links, branch tips, master SHA and limitations.
