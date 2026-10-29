# GOAL — Integrate the completed Multi Launcher research and follow-on results through two controlled pull requests

**Program:** `research-followon-merge-20261029`  
**Task type:** integration, evidence preservation, PR creation, review, and sequential merge — **not** new research, feature development, or an application-wide test campaign.  
**Repository:** `multiplex55/Multi_Launcher`  
**Execution root:** `docs/plans/research-merge-20261029/`  
**Persistent receipts:** `docs/plans/execution/research-consolidation-<RUN_ID>/`  
**Companion:** [KICKSTART_RESEARCH_INTEGRATION_AND_MERGE.md](KICKSTART_RESEARCH_INTEGRATION_AND_MERGE.md)  
**Initial status:** `NOT_STARTED` — this plan has not modified the user's repository, opened a PR, or merged anything.

> This is a new, explicitly authorized merge-workflow contract when its companion Kickstart is invoked by the user. Historical research documents saying *do not open or merge automatically* governed the old **research** task; they are evidence, not instructions to abandon this new user-requested integration. Nevertheless, never bypass branch protection, required approvals, external checks, authorization limitations, or destructive-work safeguards. A missing permission or genuinely blocked merge means preserve ready PRs and report the precise gate; do not fabricate success.

---

## 1. Objective and final observable result

Consolidate the **completed** two-goal source-only research program, the **verified** terminal-query-escape Rust correction, and the **closed** SEARCH-F002 / AUD-F002 decision studies into the repository's `master` branch **with exactly two deliberately scoped PRs**, unless a source change or repository constraint makes a documented no-op/defer necessary.

**PR 1 — narrow production correction:** a fresh integration branch containing only the original, already-reviewed terminal-backslash tokenizer correction, its five focused regressions, and its source-specific verification/review documentation. Exclude an outdated copy of the four-package `INTEGRATION_STATUS.md` from this PR.

**PR 2 — completed research and final outcomes:** a new documentation integration branch containing both original research goals, their intact datasets/prototype/reviews and canonical follow-on contracts, the two subsequent decision-study document trees, and **one final authoritative** follow-on ledger plus its G04 dependency disposition. No production Rust, config, Cargo, tests, or application assets may change in this PR.

The two PRs must be created/reviewed/merged **in that order** where permissions and required gates allow. If PR 1 cannot be merged within the run, prepare PR 2 as a **stacked documentation-only PR whose base is PR 1's integration branch**. Do not misrepresent that as a master-ready or already-merged PR. When PR 1 eventually merges, retarget PR 2 to updated master, reconcile only necessary differences, reverify docs-only scope, and merge it under the same gates. This avoids requiring a human mid-plan merely to let documentation work proceed.

**Definition of success:** (a) PR 1 is merged or is an accurately blocked, review-ready PR; (b) PR 2 is merged or accurately ready/stacked with no code changes relative to its actual base; (c) retained source provenance and proposed/deferred outcomes are faithful; (d) PR numbers/URLs, merge SHAs when known, result of checks, and any exact next manual action are recorded. Never claim `master` contains work whose PR is still open.

### 1.1 Deliberate non-goals

- Do **not** implement the deferred SEARCH-F001 eleven-descriptor metadata catalog, P2 ranker, discovery UI, new plugin behavior, or any other production search feature.
- Do **not** execute the new proposed Diff error-information Goal/Kickstart; retain it as a future proposal only.
- Do **not** alter the selected AUD-F002 H0 best-effort file-mutation policy or SEARCH-F002 H0 no-change decision.
- Do **not** run a full Nextest suite, full Cargo build, GUI automation, Windows-native acceptance, benchmarking, or an additional general architecture audit.
- Do **not** merge the standalone static-audit branch independently: it is already an ancestor of the finished search-research branch.
- Do **not** squash, rewrite, delete, reset, or force-push historical research and implementation branches as part of cleanup.
- Do **not** upgrade dependencies, change `.codex` settings/agents, reformat the codebase, touch unrelated files, or revive the old 17-goal queue.

---

## 2. Pinned intake inventory — **verify fresh, never assume unchanged**

The following is a source-backed **initial snapshot**, not an instruction to reset an advanced `master` or overwrite a moved tip. Before doing any writes, fetch, inspect the actual remote, re-resolve all heads, inspect commits and compare trees, and record any drift in `INTAKE_AND_PROVENANCE.md`.

| Source branch | Snapshot tip (2026-10-29) | Relevant result | Integration rule |
|---|---|---|---|
| `master` | `59156d96adcc73939c7171487a0cb582e3286587` | Baseline after previous PR #1455 | Fetch/current `origin/master` is authority; no hard reset |
| `codex/query-terminal-escape` | `2db6c8a8e3c7b9c6de8c1f2c1dccd3ca0c2a77c5` | Corrected `RawToken.raw`, five new regressions, 11/11 selected Nextest, scoped Clippy and independent review PASS | Import **whitelisted source and owned execution evidence**, not branch's interim queue-status snapshot |
| `codex/research-static-audit-20261028-r01` | `3b79d73f12a791b8b077ed0bf7d6631bd885b68c` | Goal 01 completed with limitations | **Do not separately import/merge**; present in the research-search branch's ancestry |
| `codex/research-search-discovery-20261028-r01` | `cc94dbf4f98f155bcdef81165f647cbafb472eb0` | Both research goals complete; audited source maps, synthetic dataset/results/prototype, four complete canonical *proposed* Goal/Kickstart pairs | Import canonical research `docs/` tree and three mutable planning ledgers; **no source** |
| `codex/discovery-admission-decision` | `cc6ed8ad91711c834631074a2cb1c96ec22fd6ae` | SEARCH-F002 closed, H0 NO_CHANGE; 81 registered builtins inspected, consumer contract NOT_ACCEPTED, independent review, 858 static assertions | Import only unique decision-study documentation tree; no feature implementation |
| `codex/research-diff-mutation-authority-r01` | `52eec0137a3dc0571580ef8da4c01962481d0848` | AUD-F002 closed, H0 retain best-effort authority, independent review, 190 static assertions; new **unexecuted** Diff failure-information plan; final G04 prerequisite disposition | Import unique Diff documentation, **final** follow-on status and G04 report; no native change |

**Stable provenance links:** [terminal fix](https://github.com/multiplex55/Multi_Launcher/commit/2db6c8a8e3c7b9c6de8c1f2c1dccd3ca0c2a77c5), [completed research](https://github.com/multiplex55/Multi_Launcher/commit/cc94dbf4f98f155bcdef81165f647cbafb472eb0), [SEARCH-F002 decision](https://github.com/multiplex55/Multi_Launcher/commit/cc6ed8ad91711c834631074a2cb1c96ec22fd6ae), [AUD-F002 / final queue](https://github.com/multiplex55/Multi_Launcher/commit/52eec0137a3dc0571580ef8da4c01962481d0848).

### 2.1 Confirmed code and documentation boundaries at intake

The source fix branch differs from its historical master by **three Rust paths**, plus task-specific documents:

```text
src/common/query.rs
src/plugin.rs
src/plugins/snippets.rs
docs/plans/execution/standalone/preserve-terminal-query-escape/**
```

The temporary queue record under `docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md` on that branch is **outdated**, because it precedes the later SEARCH-F002 / AUD-F002 closeouts. It must **not** be imported to PR 1. Its historical content remains recoverable from the original branch.

Research source for PR 2 is the **final** search branch, which already carries Goal 01. Its source diff from the historical master was confined to documentation/research data. The unique follow-on roots are:

```text
docs/implementation/SEARCH-F002-discovery-admission-consumer-proof/**
docs/research/diff-mutation-authority-r01/**
docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md
docs/plans/execution/research-follow-ons-20261029/stalls/G04-prerequisite-20261029T120120Z.md
```

**Use the last AUD-F002 branch as the source of the authoritative final `INTEGRATION_STATUS.md`**: three packages completed/pushed, fourth conditional/deferred. Do not overwrite it with any earlier branch version. When status facts changed after those commits, create an *integration-specific* note rather than silently reauthoring original historical receipts.

### 2.2 Content-integrity requirements

- Preserve the complete `docs/research/multi-launcher-research-20261028-r01/` directory, especially the 300-query corpus, source-labeled v1/v2 correction, original ranking outputs, receipts, prototype, model attribution, raw proposals and final four-package catalog. Do **not** rerun the prototype, tune scores, relabel cases, filter negative results, or change verdicts.
- Preserve the `RESEARCH_COMPLETE_WITH_LIMITATIONS` outcome for Goals 01 and 02; research completion is **not** a production release.
- Preserve SEARCH-F002 **H0 NO_CHANGE** and `CONTRACT_NOT_ACCEPTED`; SEARCH-F001 remains deferred for unmet useful-consumer/compatibility prerequisite, not a failed compile or missing generic authorization.
- Preserve AUD-F002 **H0 retain best-effort path/metadata authority**; do not claim native file identity is guaranteed.
- Preserve the new Diff failure-information Goal/Kickstart as `PROPOSED_NOT_AUTHORIZED`. This work was **not** part of the four-package execution authority and must not be implemented in this merge program.
- Preserve complete terminal-fix evidence: the original correction was already compiled in the selected Nextest test target, with 11/11 focused tests and scoped Clippy PASS. No full-suite/native proof is claimed.
- Original/source commits, review dates, resolved/unverified model identities and old status logs are historical evidence. Do not recast them as proof of a new execution on an integration branch.

---

## 3. Branch topology and merge architecture

Resolve a unique `<RUN_ID>` (e.g. `20261029-r01`, increment only for an actually distinct run). At program start, create a **fresh** integration branch from the verified current `origin/master` after safe fetch/fast-forward resolution. Use one active writer and one active integration branch at a time.

Preferred path if PR 1 can merge:

```text
origin/master (observed CURRENT_MASTER)
  └─ codex/integrate-query-escape-<RUN_ID>
       ├─ M0-A: bounded integration baseline
       ├─ M1-A: exact source fix + existing verification evidence
       └─ PR 1: code fix -> master, review/checks -> MERGE

updated origin/master (after PR 1 merge)
  └─ codex/integrate-research-docs-<RUN_ID>
       ├─ M2-A: both research goals and immutable artifacts
       ├─ M2-B: two decision-study trees + final follow-on status
       └─ PR 2: docs only -> master, review/checks -> MERGE
```

**No-intermediate-approval fallback:** If PR 1 is blocked by a genuine external approval or GitHub permission, do **not** stop authoring PR 2. Instead create the second new branch from the **published PR 1 integration tip** and open PR 2 with **base = PR 1 branch**, explicitly labelled `STACKED_ON_UNMERGED_PR1`. Its visible PR diff must be documentation-only relative to that base. When PR 1 becomes merged, retarget to `master` and ensure the retargeted PR still changes only allowed docs; fetch and reconcile the updated master conservatively if needed. Never merge PR 2 into master while it implicitly brings a blocked/unapproved code change.

**Branch rules:** never write to `master` directly; no force push, reset, destructive stash, branch deletion, unrelated cherry-picks, history rewriting, or import by checking out a historical branch over a dirty user's workspace. Use a clean isolated Git worktree when necessary. If a matching integration run exists, **resume it rather than create a duplicate**. Log all current branch names and head SHAs before work.

---

## 4. Operating roles, model recommendations, and evidence policy

| Role | Recommended model / effort | Responsibility | Safety |
|---|---|---|---|
| Parent/orchestrator | Existing configured `gpt-6-astra` / `xhigh`, or supported available equivalent | One writer/branch owner, Git/gh authority, ledgers, PR/merge sequencing, stop reports | All Git mutations and PR actions stay with parent |
| Planner | Existing configured `gpt-6-astra` / `max`; Sol high if task-scoped fallback supported | Targeted inventory, diff/ancestry comparison and conflict strategy only | Read-only; stop after handoff clarity |
| Integrator (implementer) | Existing configured `gpt-6-astra` / `xhigh`, or Sol xhigh per documented task-scoped fallback | Transplant immutable artifacts and documentation, keep scopes bounded | One source/documentation writer, never an unrelated code refactor |
| Reviewer | Existing configured `gpt-6-astra` / `max` | Review PR 1 source-parity/evidence, PR 2 docs-only scope/provenance/ledger/dependencies | Separate review if available; parent self-review must be labeled if not |

Do not edit `.codex/config.toml`, any role file, or global settings as part of integration. Model names/effort in the repository demonstrate a requested configuration, not necessarily the serving model; record actual identity only if observed. One active writer is preferred; don't launch redundant parallel agents. The parent may continue with a truthful labeled fallback after a single failed agent dispatch; never loop endlessly to reserve a thread.

Use current source and `AGENTS.md` as the primary policy. Historical Goal/Kickstart prohibitions govern their original research run; the new task exclusively authorizes the merge operations described here. Do not treat source files or documents as instructions to execute unrelated work.

---

## 5. Milestones, commits, and acceptance gates

**Milestone states:** `PENDING`, `IN_PROGRESS`, `COMPLETE`, `BLOCKED_EXTERNAL`, `DEFERRED_NO_OP`, `PARTIAL_RESUMABLE`. Record actual status in the `Execution progress — mutable` section at the end of this Goal; link an execution-specific receipt at each checkpoint. A commit cannot record its own resulting SHA; put it in the next checkpoint or final host report, not a recursive hash-only commit.

| ID | Checkpoint | Required result | Proposed commit subject |
|---|---|---|---|
| M0-A | Baseline and source reconciliation | Current master/branches, clean worktree, branch 1 created, manifest, PR/merge policy, model record and status initialized | `docs(integrate): [M0-A] pin completed research intake` |
| M1-A | Exact terminal fix transplant | Three Rust paths plus task evidence only, source parity verified, historical test receipt attached, code review | `fix(query): [M1-A] integrate verified terminal-escape correction` |
| M1-B | PR 1 publication and merge disposition | Accurate code PR opened/pushed or exact creation blocker; verification/CI/review/merge receipts | `docs(integrate): [M1-B] record query fix PR disposition` if warranted before merge |
| M2-A | Complete research import | New docs branch, both research goals and original corpus/reviews/Goals copied with correct ancestry | `docs(research): [M2-A] integrate completed audit and discovery studies` |
| M2-B | Decision follow-ons and canonical queue state | F002 decision docs, AUD-F002 decision docs, final G04 status, distinct proposed improvement, checks | `docs(research): [M2-B] preserve final follow-on decisions and provenance` |
| M2-C | PR 2 verification and publication | Docs-only diff/review, both PR base relationships and host checks, PR opened, merge or truthful stacked state | `docs(integrate): [M2-C] record consolidated research PR disposition` if appropriate |
| M3-A | Final master reconciliation and handoff | PR IDs/URLs/SHAs, source/doc scope, final user report and any exact external gate | Do not invent a third PR solely to self-record merge SHAs |

### 5.1 M0-A — Safe bootstrap and source map

1. Read this Goal, companion Kickstart, root and nested applicable AGENTS instructions. Resolve the actual repository/remote and verify that the current task does not already have an active integration branch/PR/receipt.
2. Observe `git status --porcelain=v1 -uall`, worktrees, branch, local/remote heads, upstream/remote ownership, merge/rebase/cherry-pick state, and relevant `.git` operation markers. Never overwrite a dirty checkout; use an isolated clean worktree instead.
3. Fetch `origin` once, including `master` and the named source branch refs (or fetch the required refs explicitly if the configured refspec is narrow). Advance local `master` **only if fast-forward safe**; if local changes or divergence make that unsafe, create the new task branch directly from verified `origin/master` in an isolated worktree without touching user's local master. This honors the current-master baseline while preserving work.
4. Resolve the five source tips and `master` from remote, compare ancestry, changed paths and source-object hashes. If a source branch is missing, distinguish GitHub-pushed availability from a stale local ref. Do not silently substitute a similarly named branch or fallback to old ZIP artifacts.
5. Record `CURRENT_MASTER`, `SOURCE_QUERY`, `SOURCE_RESEARCH`, `SOURCE_ADMISSION`, `SOURCE_DIFF`, `SOURCE_AUDIT`, compare/verification URLs and exact commit dates in the intake manifest. The source audit tip must be an ancestor of the research branch; if not, inspect discrepancy before deduplicating.
6. Define merge-policy as `MERGE_WHEN_GATES_PASS`, interpreted narrowly: the **future execution of this kickstart** requests sequential PR merge only when GitHub authorizes it and scoped independent review/required checks pass. Do not use admin bypass. If the user explicitly requests PR-only mode instead, record `PREPARE_ONLY` with the same artifact acceptance gates. Do not ask mid-plan for routine permissions.
7. Create `codex/integrate-query-escape-<RUN_ID>` fresh from current master. Write/persist this pair and `docs/plans/execution/research-consolidation-<RUN_ID>/INTAKE_AND_PROVENANCE.md`, `STATUS.md` and `BRANCH_MAP.md`. Update mutable Goal status. Commit/push one normal M0-A checkpoint.

**Acceptance:** actual observed facts match the recorded manifest; no user changes displaced, no stale master silently used, branch ancestry explicit, GitHub visibility claims truthful, no production-source import yet.

### 5.2 M1-A — Transplant the proven fix, not its stale queue ledger

1. Inspect exact diff of `SOURCE_QUERY` versus its merge base and compare with current `master`. Verify whether the defect already has an equivalent fix on current master; if so, mark this source import a **documented code no-op** and focus PR 1 on evidence only or skip PR 1 if no material diff. Do not reverse a newer intentional change.
2. Import, using **explicit path whitelisting** (e.g. `git restore --source=<SOURCE_QUERY> -- <allowed paths>` after checking the base/targets), exactly:

   ```text
   src/common/query.rs
   src/plugin.rs
   src/plugins/snippets.rs
   docs/plans/execution/standalone/preserve-terminal-query-escape/
   ```

   Do not import `docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md` from this source branch. Keep the fix's original author and source-tip reference in a provenance receipt; avoid suggesting that the historical code commit itself became an ancestor if the new integration commit merely transplanted its tree.
3. Confirm the code-phase deltas remain the tokenizer EOF raw append fix, meaningful inline regressions and existing affected tests. Rejected widening includes query grammar, Literal policy, snippet persistence, command execution, plugin ABI, settings, Cargo, vendored code and autoformatting churn.
4. Establish **byte/content parity** with the already-verified fix using Git blob comparison of the three Rust paths (or a justified equivalent). If current master and source baseline are unchanged and imported source blobs are exact, reuse documented evidence: 11/11 selected Nextest PASS, five new cases, scoped Clippy PASS, touched-file formatting PASS, separate review ACCEPTED. **No repeat Cargo compilation required.** If code differs or an updated master materially affects the covered path, one bounded affected-test selection may be necessary; document why and do not blindly reuse the prior receipt as current-tree test results. Never substitute an old receipt for a changed code path.
5. Run cheap `git diff --check`, inspect `git diff --stat`, `git diff -- src/` and document scope. Have reviewer inspect exact PR 1 candidate diff and evidence. Persist `PR1_SCOPE_AND_VERIFICATION.md` with test provenance and actual checks (or precise `NOT_RUN` reasons).
6. Commit M1-A with informative source SHA in body; attempt one ordinary push. Do not force-push or silently modify unrelated files.

**Acceptance:** only the three permitted Rust files and the owned fix/docs/receipts have changed relative to CURRENT_MASTER; original correction/test semantics retained; verifiable prior test receipt and separate review applicable or clearly bounded renewed evidence collected. No full suite.

### 5.3 M1-B — Create and resolve PR 1

1. Inspect stage/diff, remote branch, exact compare to current `origin/master`, current PR existence and availability of `gh`/GitHub permissions. A prior equivalent open/merged PR should be resumed/recognized, not duplicated.
2. Create PR 1 with a precise title such as `fix(query): preserve terminal escapes in raw search tokens`. Body must describe the single production behavior change, three files, five new regressions, already-recorded 11/11 focused tests, scoped Clippy, warnings not caused by patch, and explicit non-claims about broader acceptance. Attach links to original source commit and retained verification doc.
3. Verify GitHub PR diff/scope and source-head equality, review requirements, check results (when any), and separate reviewer disposition. An empty GitHub checks list means **no checks reported**, not `CI passed`. Do not treat unrun full tests as green. Correct a legitimate diff/PR issue once within the bounded scope, using one new coherent commit/push, not a rerun loop.
4. Under `MERGE_WHEN_GATES_PASS`, merge PR 1 only when review has no unresolved correctness issue, all **required** repository checks/approvals are satisfied, source/diff is intentional, and the host permits a normal merge with **no admin bypass**. Prefer repository-supported merge-commit strategy preserving the checkpoint commits; if unsupported, record the permitted strategy and provenance, never rebase/force against an existing user branch to manufacture it. After merge, fetch and record updated `origin/master` and actual PR merge SHA.
5. If PR 1 cannot merge because of external approval, branch protection, unavailable CLI, permission, pending required checks or review ambiguity: preserve its published PR/branch, mark `BLOCKED_EXTERNAL` with the precise gate; move ahead to PR 2 as stacked docs-only work. Do not claim master updated. If PR creation itself is unavailable, preserve pushed source branch plus an exact PR body and compare URL, then create PR 2 stacked from the verified integration branch tip where supported.

**Acceptance:** PR 1 genuinely merged into master, or a review-ready PR/explicit external blocker/next action is persisted; avoid fabricated PR IDs, green checks or merge SHAs.

### 5.4 M2-A — Create docs branch and preserve the entire two-goal research output

1. If PR 1 merged, safely fetch updated `origin/master` and create fresh `codex/integrate-research-docs-<RUN_ID>` from that exact SHA. Otherwise create a **fresh branch from the published PR 1 integration tip** and mark `STACKED_ON_UNMERGED_PR1` with proposed PR base = that integration branch. No independent code edits occur on the docs branch.
2. Import **only** these paths from verified `SOURCE_RESEARCH`, after checking they were not superseded on the new base:

   ```text
   docs/research/multi-launcher-research-20261028-r01/
   docs/plans/ai-research/01_GOAL_STATIC_ARCHITECTURE_AUDIT.md
   docs/plans/ai-research/02_GOAL_SEARCH_ACTION_DISCOVERY.md
   docs/plans/ai-research/ORCHESTRATOR_GOAL.md
   ```

   Do not wholesale overwrite `docs/plans/ai-research/` because it already contains immutable original Kickstarts/templates on master. Do not import static-audit branch separately. Do not modify/re-evaluate historical research findings or experiment data to fit a desired merge narrative.
3. Verify `SOURCE_AUDIT` is an ancestor of `SOURCE_RESEARCH` or document the exceptional difference and adopt a faithful path-level import without duplicate history. Check relevant research root tree/blob parity, the complete original follow-on catalog, reviewer records, 300 synthetic queries, original and corrected measurements, and `RESEARCH_COMPLETE_WITH_LIMITATIONS` status.
4. Commit M2-A and push once. Any report explaining integration should live under the **new** execution directory, not by mutating historical study receipts.

**Acceptance:** both research goals and every original artifact are navigable on docs branch, byte/content unchanged except genuinely necessary link normalization documented; no Rust/config/test change against the docs PR base.

### 5.5 M2-B — Preserve follow-on decision trees and authoritative final queue status

1. Import from verified `SOURCE_ADMISSION` **only**:

   ```text
   docs/implementation/SEARCH-F002-discovery-admission-consumer-proof/
   ```

   Preserve `H0 NO_CHANGE`, rejected `SEARCH-F002-consumer-v1` and the closing-reviewed `012ed8af...` receipt and source manifest. Do not import the older queue-status file from this branch.
2. Import from verified `SOURCE_DIFF` **only**:

   ```text
   docs/research/diff-mutation-authority-r01/
   docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md
   docs/plans/execution/research-follow-ons-20261029/stalls/G04-prerequisite-20261029T120120Z.md
   ```

   Preserve H0 Diff best-effort path/metadata decision, the new *proposed-only* Diff failure-information pair and reviewed source/verification receipts. The G04 report is a **conditional dependency disposition**, not an agent tool failure or reason to implement SEARCH-F001.
3. Reconcile the queue status by treating the Diff tip's final `INTEGRATION_STATUS.md` as authoritative for all four packages. The actual status is **AUD-F001 complete, SEARCH-F002 H0 research complete, AUD-F002 H0 research complete, SEARCH-F001 deferred/NOT_IMPLEMENTED**. Exclude stale earlier snapshots on sibling branches. Preserve historical original status texts in their original Git history; do not erase them.
4. Write `INTEGRATION_MANIFEST.md` with each imported tree/path and exact source tip. If an old document links to a branch-ref/historical SHA, leave that evidence link unless it is genuinely broken; do not falsely claim it points to a final merged `master` revision.
5. Commit M2-B and push once. Limit all changes to docs and execution receipts.

**Acceptance:** final record explains the complete 3+1 queue, SEARCH-F001 remains deferred, proposed Diff improvement isn't executed, both decision trees are intact and no unrelated patch is carried.

### 5.6 M2-C — Docs-only review, PR 2 and sequential merge

1. Identify PR 2's **actual** base. With PR 1 merged: `master` at the verified post-PR1 tip. With PR 1 blocked: the PR 1 integration branch (stacked). Never create a PR to master that includes PR 1's still-unapproved code and present it as documentation-only.
2. Run one lightweight consistency pass against the docs branch: Markdown local-link targets, known relative anchors when practical, status/ID agreement, source-tree presence/hashes and `.json`/`.jsonl` parseability when touched. Avoid re-evaluating the research corpus, running the Python ranking prototype, building Rust, or running tests. Preserve original binary/text format and UTF-8/LF experiment receipts.
3. Use `git diff --name-only <ACTUAL_PR2_BASE>...HEAD` and GitHub PR compare to enforce **every PR 2 changed path begins with `docs/`**. Explicitly reject `src/`, `tests/`, `crates/`, `.codex/`, `Cargo.toml`, `Cargo.lock`, `build.rs`, `.github/`, `AGENTS.md`, and production assets. Check the four package IDs and dispositions, original datasets and new proposed-only pair. Inspect accidental whitespace/line-ending churn.
4. Obtain one independent scoped review of the documentation inventory, canonical final ledger, source provenance, PR dependency and non-implementation policy. Allow one substantive correction pass; leave unresolvable external PR gate explicit rather than recheck forever.
5. Create PR 2 titled similar to `docs(research): integrate completed architecture/search studies and follow-on decisions`. Body must distinguish both research goals, the 300-query synthetic study, no production search/UI improvement, the fixed query defect carried only by PR 1, two H0 decision studies, and the new Diff proposal left unexecuted. List docs-only verification and the one final historical status source.
6. If PR 1 merged and GitHub required checks/approvals are satisfied, merge PR 2 normally to `master` under `MERGE_WHEN_GATES_PASS`. If stacked, leave it open against PR 1's branch until that code PR merges; then retarget and verify the new PR diff is docs-only before merging. Never force GitHub to accept PR 2 because it is "just docs".
7. Record PR 2 URL, base/head/commit, review/check result, actual merge SHA if available, or exact outstanding gate in an execution receipt. The ending Git commit cannot contain its own later merge SHA; finalize user-visible outcome from observed GitHub data.

**Acceptance:** second PR is genuinely docs-only against its actual base, accepted/merged if allowed, or safely review-ready and accurately blocked; the final queue record remains authoritative.

### 5.7 M3-A — Final repository state and handoff

1. If both PRs merged, fetch `origin/master` once and verify the exact three corrected Rust source blobs/semantics and all four document trees are present on `master`. Check final `INTEGRATION_STATUS.md` says `three complete; one deferred` and historical review outcomes are not misrepresented.
2. If PR 1 merged but PR 2 is pending: report master has **only the code fix**; do not claim research docs are integrated. If PR 1 is pending and PR 2 is stacked: report neither is merged and provide the exact approval/retarget instructions. If a PR was impossible to create: report pushed branch and exact compare path plus PR body location.
3. Return a succinct, explicit table of PR 1 and PR 2: branch/head, URL/number if real, base, `OPEN`/`MERGED`/`BLOCKED`, checks/review, merge SHA if present, and no-op/outstanding action. Include worktree state and whether final `master` is updated.
4. Do **not** open a third PR or edit `master` just to backfill SHAs in already merged status Markdown. Final merge receipts live in the user-facing handoff; write all pre-merge receipts before the corresponding PR merges.
5. Leave the old branches intact. Offer the proposed Diff failure-information implementation as **one separately authorized next task**, not part of these merges.

---

## 6. Narrow verification policy and time bounds

**No automatic Rust rebuild if original code blobs remain exact.** The original query-fix receipt records a 434.594-second selected build/list, 11/11 Nextest PASS, 90.266-second scoped Clippy PASS and touched-file rustfmt PASS; it was independently reviewed. Repeating that full compilation for an identical three-file tree on identical baseline adds little evidence and contradicts this merge's scope.

**PR 1 mandatory cheap checks:** `git diff --check`; allowed path inventory; exact source blob parity or a documented reason parity cannot be established; original focused proof; no unrelated code. If current master differs in a way that invalidates reuse, run only the relevant `common::query`, `terminal_escape`, plugin and snippet cases: a single selected-target compilation/discovery with a **20-minute bound**, one selected run with a **120-second bound**, and scoped Clippy only if modified code requires it (**10-minute bound**). Confirm nonzero selection. No second full campaign or unbounded test loop; record `NOT_RUN` honestly if the environment blocks necessary proof, leaving the PR not merge-ready if that proof is essential.

**PR 2 mandatory cheap checks:** docs-only diff and `git diff --check`; one sensible file/link/structured-data checker with a **60-second bound per invocation** and **one localized correction/check** if needed; inspect source paths/hashes once. No old research tool reruns, synthetic benchmark repetitions, native tests or Cargo invocations.

**CI and PR checks:** query the real status of **required** checks and required reviews via GitHub once per publication/merge gate. Missing checks are **unreported**, not silently passed. Pending checks may remain an external gate; do not poll indefinitely, invent completion, or bypass protection. GitHub/branch-host metadata is authoritative for merged/open state.

**Git and PR command failures:** at most one standard retry for a transient network action, no auth/protection mutation, no `--admin`, no force push. If a normal push fails, retain local commits and continue the other safe steps where possible. Never claim an unpublished branch or created PR exists remotely.

---

## 7. PR content contracts

### PR 1 intended scope

```text
src/common/query.rs
src/plugin.rs
src/plugins/snippets.rs
docs/plans/execution/standalone/preserve-terminal-query-escape/**
docs/plans/research-merge-20261029/GOAL_RESEARCH_INTEGRATION_AND_MERGE.md
docs/plans/research-merge-20261029/KICKSTART_RESEARCH_INTEGRATION_AND_MERGE.md
docs/plans/execution/research-consolidation-<RUN_ID>/**   # Only first-phase files/receipts
```

No `INTEGRATION_STATUS.md` import from the query branch; that file belongs to PR 2 and comes from the final Diff tip. No other Rust path. Review changes to `docs/plans/research-merge-20261029` only within the designated mutable execution section.

### PR 2 intended scope

```text
docs/research/multi-launcher-research-20261028-r01/**
docs/implementation/SEARCH-F002-discovery-admission-consumer-proof/**
docs/research/diff-mutation-authority-r01/**
docs/plans/ai-research/01_GOAL_STATIC_ARCHITECTURE_AUDIT.md
docs/plans/ai-research/02_GOAL_SEARCH_ACTION_DISCOVERY.md
docs/plans/ai-research/ORCHESTRATOR_GOAL.md
docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md
docs/plans/execution/research-follow-ons-20261029/stalls/G04-prerequisite-20261029T120120Z.md
docs/plans/execution/research-consolidation-<RUN_ID>/**   # Additional docs/receipts only
```

In the ideal two-PR sequence, PR 2 cannot change production Rust relative to updated master. In a stacked PR, evaluate against PR 1 branch (the correct base); evaluate again against updated master before actual merge.

### 7.1 Merge and history policy

Prefer two regular PR merge commits when supported, preserving separate evidence/checkpoint history. Squash/rebase behavior can depend on repository settings; don't change repo settings to force a strategy. If server permits only squash, preserve source SHA and explicit imported-path provenance, and report the resulting merge form. Merge commits from independent source branches are **not** necessary; staging verified path-level imports avoids the shared ledger collision. No force updates to an existing branch.

---

## 8. Review checklist, stop handling and reporting

The independent reviewer should address material risks, not cosmetic formatting:

1. PR 1 source equals the tested original patch, with explicit EOF bookkeeping only; tests are meaningful, no other grammar/mode change; evidence not mislabelled as newly rerun.
2. PR 2 source paths are documentation only, with complete two-goal research and original data. No prototype results or raw proposals rewritten.
3. Final queue status is the **Diff tip** version, not an older query/admission sibling snapshot. The G04 stop is a decision/prerequisite defer, not failed work.
4. SEARCH-F001 remains deferred; Search research P2 is conditional synthetic evidence, **not** an accepted shipping ranking/UI implementation.
5. Diff best-effort native status and the new proposed-only failure-information pair are intact; no stronger guarantees claimed.
6. Branch bases/PR targets are honest: PR 2 is not declared docs-only against `master` while it implicitly carries an unmerged PR 1.
7. No CI status or reviewer approval is fabricated, no unapproved protected-branch override, no production source drift on docs PR.

**When truly blocked**, create a short `stalls/<checkpoint>-<UTC-YYYYMMDDTHHMMSSZ>.md` under the task execution root, with: actual branch/HEAD/upstream/worktree; exact missing PR or permission gate; command/error/status; what is safely committed/pushed; current PR base; any unresolved conflict files; what was **not** run; and one smallest safe resume command/action. Don't use a stall file for an ordinary deferred goal or for a missing optional cosmetic check. Continue unrelated allowed documentation work if possible. No repeated stagnant reports.

**Final user report** must include: both PR results, actual master SHA, the query fix's scoped verification (historical or rerun distinguished), all archived research/decision trees, 3-complete/1-deferred catalogue, whether proposed Diff improvement remains unimplemented, a truthful no-full-test statement, any branch/permission blockers, and the recommended next action.

---

## 9. Execution progress — mutable

**State:** `IN_PROGRESS`
**RUN_ID:** `20261029-r01`
**Observed initial master:** `59156d96adcc73939c7171487a0cb582e3286587` (one normal fetch; local equals remote)
**PR 1:** `NOT_CREATED`
**PR 2:** `NOT_CREATED`
**Active branch/worktree:** `codex/integrate-query-escape-20261029-r01`; `G:/Repos/rust/Multi_Launcher`
**Current owner/role:** `/root`, sole integration writer and Git/PR owner; separate reviewer follows
**Last accepted checkpoint / commit / push:** M0-A acceptance complete for this publication; actual SHA/push recorded by next checkpoint
**Next action:** M1-A exact query-fix import and review
**Output index:** [STATUS](../execution/research-consolidation-20261029-r01/STATUS.md).

| Milestone | State | Evidence / next action |
|---|---|---|
| M0-A — baseline | COMPLETE | [Intake](../execution/research-consolidation-20261029-r01/INTAKE_AND_PROVENANCE.md), [branch map](../execution/research-consolidation-20261029-r01/BRANCH_MAP.md), [host receipt](../execution/research-consolidation-20261029-r01/receipts/host-intake.json); commit/push follows |
| M1-A — exact fix | PENDING | Import only exact three Rust files and owned evidence; separate review |
| M1-B — PR 1 | PENDING | Create, inspect gates and normally merge or record actual external block |
| M2-A — research docs | PENDING | Base on updated master or published PR1; import full two-goal whitelist |
| M2-B — decisions/queue | PENDING | Exact decision roots and final Diff ledger/G04 |
| M2-C — PR 2 | PENDING | One bounded metadata check, docs-only review, PR and normal merge gate |
| M3-A — final check | PENDING | Verify actual master/PR state and report |

### Material events

- 2026-10-29: user invoked the integration Kickstart. Full Goal/Kickstart and applicable root AGENTS read; no nested AGENTS found. Current master unchanged from historical baseline. SOURCE_DIFF advanced with contract/model config only; model changes excluded from imports. No matching integration run or PR exists. `gh` unavailable; GitHub connector available. No production change, test or prototype run in M0-A.
- Actual source SHAs and role/configuration caveats are recorded in intake. Normal merge is authorized only under host gates. No admin bypass, configuration edit, branch deletion or future-goal implementation.

### Acceptance sign-off

M0-A is accepted for publication from a safe, freshly verified baseline. Overall completion remains unproven until PR1/PR2 actual outcomes and master/worktree reconciliation are recorded. A later external gate must retain review-ready artifacts and exact next action; it does not authorize a bypass.
