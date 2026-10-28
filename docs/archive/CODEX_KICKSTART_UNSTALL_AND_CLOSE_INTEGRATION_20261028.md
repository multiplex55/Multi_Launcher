# Codex Kickstart — Unstall the Multi Launcher Integration Queue

**Queue:** `multi-launcher-queue-20261025-eedc19d2`  
**Repository:** `multiplex55/Multi_Launcher`  
**Task:** Execute a finite, safe recovery, then complete an evidence-backed *partial integration handoff* if mandatory goals cannot be completed. This is not permission to mislabel incomplete goals as completed.  
**Status baseline:** GitHub read-only snapshot on 2026-10-28. The live local queue ledger, Git and process state take precedence over this snapshot after reconciliation.

## 1. Mandatory branch and authority

The existing integration branch is **the one and only branch for integration work**:

`codex/queue-integration-pre-goal17-20261027`

Last observed pushed HEAD: `d4ba24b0000d35bc83f23eb99a28f0c021df949d` (`docs(queue): record blocked G09/G12 M2-A and require stall reports`).  
Last accepted integration *code* checkpoint: `2a7535086f00359c36037c32cdd3a26f95b9c9bd`.  
Original accepted carry: `ceff1f17fb09545d8b9a6010aae9eca7892a7ea7`.  
`master` was observed at `bbc2ca1ef247c53eeae9f2ce7ba69f381483318d` and is **not** the integration working branch.

**Execute this kickstart, rather than drafting another plan or repeating a diagnostic-only report.** This request explicitly authorizes the narrow actions in Sections 4–6, superseding older instructions *only* where they would force the integration effort to terminate indefinitely because optional future goal-completion work is deferred. All repository instructions, functional safety requirements, honest testing/reporting rules, and mandatory feature dependencies remain binding. No automatic merge to `master`.

First, read the existing `AGENTS.md`, `docs/plans/CODEX_KICKSTART_MULTI_GOAL_QUEUE.md`, the authorized recovery/deferral amendments, and the durable **external** `QUEUE_STATE.md`. Read the committed `docs/plans/execution/multi-launcher-queue-20261025-eedc19d2/INTEGRATION_STATUS.md` and its two `stalls/G09-M2-A-20261028T154609Z.md` and `stalls/G12-M2-A-20261028T154609Z.md` records, plus `integration-pre17/SOURCE_COMMIT_MANIFEST.json`, `integration-pre17/BLOCKED_RESUME.md`, and the relevant Goal 09/10/12/17 contracts and raw receipts.

Reconcile the local worktrees, local/upstream HEADs, staged/unstaged/untracked content, merge/cherry-pick/rebase state, active writers and PID-plus-creation-time ownership. A newer legitimate integration-branch descendant must be adopted after verification; **never reset it back to the observational SHA**. Use the already owning worktree if safe:

`C:/Users/Jay/.codex/worktrees/queue-integration-pre-goal17-20261027/Multi_Launcher`

Do not start or commit integration changes on focused Goal 09 or Goal 12 branches, `master`, a detached HEAD or another newly created integration branch. Preserve existing focused worktrees, their uncommitted patches, evidence and exhausted-attempt history. If the intended branch is checked out elsewhere, resolve its actual owner and safely operate there; never force checkout, stash, reset, discard, rebase, force-push or overwrite user changes. Do not launch duplicate owned builds or kill unrelated processes.

## 2. What is accepted and what is not

The integrated source tree contains reviewed partial checkpoints from Goals 06, 09, 11, 12, 14, 15 and 16, plus integration fixes, on top of completed Goals 01–03 and accepted Goal 04 M1-A/M1-B. *Presence in the integration tree is not full goal acceptance.*

- **Goal 09/M2-A:** Deferred. Final 95-input candidate was neither compiled nor tested, and is preserved in the separate `codex/large-csv-completion-queue-20261028` worktree. Reviewer identified three unresolved safety boundaries: identity-free stage/root path cleanup, in-place ancestor reparse redirection, and lost cleanup ownership if receipt creation and exact deletion both fail. Also investigate the recorded `GetTransactionId` signature and admission-slot issues only insofar as necessary. The previous renewed 2/2 correction rounds are exhausted; preserve every failed receipt.
- **Goal 12/M2-A:** Deferred. There is evidence of **27 distinct passing ordinary focused tests** (26 previously retained + one successful rerun), but there is **no** current-source native editor/output acceptance and no final Clippy. The available Computer Use drag interface lacked a secondary mouse-button argument needed for crop; prior native run used older source and timed out. Do not rerun already passing tests merely to create activity.
- **Goal 10:** Not accepted/not implemented, requires fully accepted Goal 09.
- **Goal 17:** Not eligible until Goals 09, 10 and 12 are fully accepted in the verified integrated tree.
- Other unfinished or deferred Goals stay labeled by their actual checkpoint: Goal 04 editor, Goal 05, Goal 06 M2-B, Goals 07–08, Goal 11 M2-A, Goal 13, and follow-on Goal 14/15/16 work. Do not import their unaccepted patches wholesale.

## 3. First deliverable — state and ownership reconciliation

Before any further implementation, update the existing external ledger with the actual current integration branch, verified HEAD, last accepted integrated source tree, current writer/process state, source-binding hashes, next action, and attempt counters. Update the existing tracked `INTEGRATION_STATUS.md` only when new factual state warrants it; the permanent `AGENTS.md` stall-reporting policy has already been pushed. **Do not create duplicate status files or restate an unchanged blocker in repeated commits.**

The next step must be execution, not another prompt/kickstart. Use the configured Planner/Implementer/Reviewer roles where available. One writer per shared source/target directory. Corrections and scoped checks must have finite predeclared deadlines and checked cleanup. Prefer exact relevant cases and source-backed requirements; do not run an automatic all-suite Nextest campaign.

## 4. One final, bounded Goal 09 safety-design opportunity (explicit new authority)

I authorize **one new, narrowly scoped design-and-implementation recovery window**, independent of and additional to the exhausted *historical* 2/2 rounds, consisting of **at most one initial candidate plus one focused correction**. This authorization is limited to the three specific Goal 09 safety findings and necessary directly supporting code/tests. Record actual new attempts and resource limits before each run. No retries of unchanged old candidates and no new general-purpose transaction framework, speculative abstraction layer, or unbounded experiment matrix.

The Planner and Reviewer should first select the **smallest maintainable design that can prove the original publication-safety contract on Windows**. Prefer established owner/handle-based capabilities already available in this codebase; do not treat a path-only recheck, advisory lock or successful TxF capability probe as sufficient evidence. Reject unsafe replacement rather than overwriting a changed/unverifiable destination. If declining replacement fails an original mandatory requirement, record it as a limitation and **do not** declare Goal 09 complete. Specifically establish:

1. Stage/root cleanup only destroys objects with verified identity or leaves a durable owned recovery record; never delete a possibly substituted file by bare path.
2. Confirmed replacement cannot be redirected via in-place ancestor reparse mutation during publication, or the operation fails closed with a source-backed proof of why an untrusted target cannot be overwritten.
3. If receipt persistence and exact cleanup both fail, cleanup ownership remains durable/recoverable across worker shutdown; no orphan is silently discarded.

Resolve any immediately related signatures/admission deadlocks, preserve quota/source/preview cancellation behavior, and create focused tests for changed identity/reparse/receipt failures and a normal successful publication path. The final input binding must **compile** and its precisely selected tests must actually execute; an isolated OS TxF probe is not acceptance of the production owner. Require bounded verification and independent review when available. If any mandatory safety criterion remains unproven after this window, **stop Goal 09 work**, preserve its focused patch and reports, and record it **Deferred — incomplete**, without importing unaccepted changes into the integration branch. No third correction, no weakened assertions, no fabricated pass.

Even if the source boundary passes, do **not** call Goal 09 complete before its original full-file M2-A and >500 MiB/native/lifecycle M2-B acceptance, review and end-only Clippy pass. If those gates cannot be completed within the original authorized budgets, defer it factually.

## 5. Goal 12: native feasibility, not another test-repair loop

Reuse the 27 already-supported passing ordinary test cases unless changed inputs invalidate them. Inspect the actual supported native GUI/testing interfaces and existing Windows owned UI harness for a permitted means of performing the required secondary-button drag, output, zoom/crop, undo/redo, save and copy checks. Authorize **one bounded capability/feasibility assessment**; if a supported native interaction method is found, execute the original focused native acceptance under its existing time/resource limits and record the results. Do not modify production code solely to accommodate an inadequate test tool. Do not claim that ordinary unit tests replace native acceptance.

If the required native acceptance cannot be performed with the available authorized interface, or it fails without remaining correction authority, **defer Goal 12** with its 27-case proof retained and native/Clippy outstanding. Do not solicit intermediate manual testing from the user, silently accept an unsupported proxy, or relabel it as completed. Keep its preserved candidate worktree and patch intact.

## 6. Mandatory forward progress when goals remain incomplete

This section is an explicit **partial-queue closeout authorization**, not a relaxation of mandatory Goal 09/12/10/17 acceptance or a claim that all 17 goals are finished.

If Goal 09 and/or Goal 12 cannot be completed in the bounded windows above:

1. Mark Goal 09 and/or Goal 12 `Deferred — incomplete`, with exact checkpoint, missing mandatory evidence and immutable stall report. Keep Goal 10 `Skipped — missing Goal 09 prerequisite` whenever Goal 09 is unaccepted. Keep Goal 17 `Skipped — missing prerequisites` whenever any of Goals 09/10/12 is unaccepted. **Do not create Goal 17's branch, claim its functionality, or fabricate final commits.**
2. **Continue safely with integration of the already accepted source checkpoint groups** on `codex/queue-integration-pre-goal17-20261027`. Do not cherry-pick whole unfinished branches, unverified patches or uncommitted candidate files. Inspect whether the existing partial integrated features are properly gated and preserve all legacy behavior; fix only integration-caused regressions within finite correction budgets. If a partial feature cannot be made safe/compatible, report it and withhold the integration handoff rather than ship a known unsafe state.
3. Verify the actual integrated code tree with a targeted compilation, selected cross-feature focused tests/native checks justified by integration, independent/scoped review where available and **end-only first-party Clippy**. No blanket all-suite requirement; ensure selected cases are nonzero, failures are preserved, ownership/cleanup is proven and the verified exact tree remains unchanged after the final gate. Test coverage for a partial checkpoint does not establish its whole-Goal acceptance.
4. Record an integration **partial outcome**, with a complete Goal 01–17 matrix (`Completed`, `Partially integrated`, `Deferred`, `Skipped`, `Not started`), included source/integration commit mapping, excluded patches, exact verification results and residual risks. Create/update `INTEGRATION_STATUS.md` and one new timestamped `stalls/*.md` report for each *new* stop or material change (never repeat reports for unchanged conditions). Keep the external `QUEUE_STATE.md` operationally authoritative.
5. Commit coherent reviewed integration-only corrections and documentation to the **same integration branch**, and push normally after each accepted checkpoint. Propose a PR into `master` **only if** the actual integrated tree passes applicable integration safety/build/tests/Clippy gates; label the PR as **partial accepted-checkpoint integration**, not all 17 goals complete. Do not merge it into `master` automatically. If any mandatory integration gate fails, leave the branch unmerged with a final durable stop report and next action instead.
6. Produce a concise final user-visible report with the exact integration branch, HEAD/upstream/push result, completed versus deferred versus skipped goals, tests/native/Clippy outcomes, whether a PR was prepared, and the next standalone recovery needed. Do not keep re-entering the same exhausted goals in an infinite loop.

**Critical distinction:** A partial-queue closeout is allowed; **unsafe writes, invented acceptance or automatic master merge are not**. Keep each incomplete Goal resumable in its own preserved worktree under a future, separately authorized contract.

## 7. Immediate action and termination

Start now with Section 1 reconciliation, then Section 4 bounded Goal 09 recovery, Section 5 Goal 12 native feasibility, and Section 6 partial integration handoff if prerequisites remain blocked. Progress autonomously through valid next steps without routine approval. Stop only for a real safety/permission/capability conflict, an exhausted finite correction window or a completed partial integration handoff. On a genuine stop, leave an updated **pushed, human-readable Markdown report** under `docs/plans/execution/multi-launcher-queue-20261025-eedc19d2/stalls/` whenever Git safety permits, with an exact safe restart point. Do not promise work continues after the Codex session ends.
