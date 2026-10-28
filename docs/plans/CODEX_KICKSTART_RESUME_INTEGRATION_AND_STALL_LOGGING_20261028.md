Codex Kickstart — Resume Existing Queue Integration and Persist Stalled-Goal Reports
Repository: multiplex55/Multi_Launcher
Local repository previously reported: G:\Repos\rust\Multi_Launcher
Queue ID: multi-launcher-queue-20261025-eedc19d2
Task: Resume the existing pre-Goal-17 integration on its existing branch, safely repair the specifically recorded blockers, and establish durable Markdown stall reporting for this and future Codex Goals.

Execute this instruction; do not only produce a new plan or summary. This is an explicit, narrow recovery and documentation authorization. Apply existing repository permissions, AGENTS.md, the original Goal/Kickstart requirements, and the existing queue's preservation rules. This is not authorization to restart the 17-goal queue, erase partial work, weaken acceptance, or merge into master.

1. Mandatory integration branch — no substitutions
The ONLY branch for the resumed pre-Goal-17 integration task is:

codex/queue-integration-pre-goal17-20261027
Last GitHub-verified remote state on October 28, 2026:

origin/codex/queue-integration-pre-goal17-20261027
    b4a13294a756aef8da86ae30789cb26c42b4d2b6
    (documentation-only: Update continue.md)

Last integrated implementation checkpoint:
    2a7535086f00359c36037c32cdd3a26f95b9c9bd
    (Goal 16/M1-A integrated; direct parent of b4a1329)

Original accepted carry anchor, NOT the restart tip:
    ceff1f17fb09545d8b9a6010aae9eca7892a7ea7

master, NOT the working integration branch:
    bbc2ca1ef247c53eeae9f381483318d
First perform read-only reconciliation. Fetch normally when safe; verify branch identity, Git ancestry, local/upstream SHA, worktree ownership, clean/dirty/staged changes, CHERRY_PICK_HEAD/MERGE_HEAD/rebase state, push status, external ledger, and owned process identities. Treat the remote SHA above as an observation, not a command to reset or discard newer legitimate work. If the same branch has subsequently advanced, verify it is a genuine descendant and use the verified latest safe state.

If the integration branch is already checked out in an existing worktree, use that worktree after confirming it is safe. Do not move the work to a different task branch.
If the current checkout is on another branch, find the integration branch's owning worktree, or safely check out the existing integration branch in a clean worktree if Git allows it. Do not create a new integration branch, alternate queue baseline, detached-HEAD implementation, or work on a focused Goal branch by mistake.
If switching would disturb unrelated local or partially applied work, stop before mutation, preserve everything, and record an exact stall report as Section 5 permits.
All integration-specific fixes, reviews, commit records, stall reports, and integration-state documentation belong on codex/queue-integration-pre-goal17-20261027. Before each integration commit and push, recheck and record git branch --show-current, the current HEAD, the staged path list, and the actual destination/upstream.
No git reset --hard, force checkout, force push, rebase, squash, stash, wholesale cherry-pick of unaccepted work, deletion of worktrees, or automatic merge to master.
Do not let the old docs/plans/continue.md change the branch, reauthorize already exhausted Goal 04 repair rounds, or select a stale baseline. This kickstart expressly governs the current integration resumption.

2. Read the real recovery state, not just GitHub
Read and reconcile these existing local artifacts (use the durable ledger to locate the real paths):

External QUEUE_STATE.md (the one operational queue ledger; update it, do not create another competing ledger).
integration-pre17/SOURCE_COMMIT_MANIFEST.json and integration-pre17/BLOCKED_RESUME.md.
Goal 09's and Goal 12's DEFERRED_HANDOFF.json files, their preserved exact uncommitted patch manifests, executed-input hashes, and failed/positive receipts.
The original Goal 09, Goal 10, Goal 12, and Goal 17 contracts, together with the queue's approved deferral/integration amendments.
AGENTS.md, relevant scoped instructions, and the Git worktree state (including any owner still writing to the worktree or using the target directory).
Do not accidentally re-enter Goal 04 M2-A or restart earlier accepted checkpoints. Retain its deferred editor status; accepted Goal 04 M1-A/M1-B is already in the integrated ancestry.

Record a concise reconciliation in the external ledger: integration branch/HEAD, last verified integrated code commit, full list of included accepted checkpoint groups, excluded or incomplete work, active blocker, source bindings and next action. The external ledger remains authoritative for in-flight commands and facts unavailable at commit time.

3. Existing accepted integration vs outstanding work
The latest preserved integration report says the integration branch contains reviewed/verified partial checkpoint groups from Goals 06, 09, 11, 12, 14, 15, and 16, plus focused integration corrections. Their presence does not establish full completion of those Goals. Goals 01–03 are already complete.

The two immediate unresolved mandatory boundaries are:

A. Goal 09 — destination identity at publication. The path-based publishing owner must safely bind an intended replacement to the previously confirmed destination identity. Do not accept a path-only comparison that could overwrite a changed/replaced file. Inspect the current integration implementation and the preserved seven-file focused patch read-only first. Correct the actual owner and establish focused evidence for stable identity, changed destination/repointing, rejection without unintended overwrite, bounded cleanup and source/preview ownership. Preserve any accepted Goal 09 source/test evidence and its remaining full-file/native acceptance; do not declare FINAL09 until all mandatory Goal 09 criteria pass.

B. Goal 12 — nine E0600 test-compile errors and native acceptance. Newly added test assertions treated unit-returning screenshot editor undo/redo operations as booleans. Inspect the actual APIs and preserved four-file patch. Correct the test contract to observe meaningful editor state/undo-redo behavior without changing the production API merely to satisfy a mistaken assertion or weakening tests. Rebuild only the affected test target(s), prove nonzero focused test discovery and actual passing results, and complete Goal 12's required native editor/output acceptance and documentation. The previous default application build succeeded in 195.822422 s, but its separate QA test build failed after 397.996201 s with nine E0600 errors and ran zero tests; do not relabel that as passing QA or native evidence.

Explicit new authorization: Up to two additional focused correction rounds per named outstanding boundary (Goal 09 destination identity, Goal 12 test-contract compilation), only after the read-only reconciliation above. These rounds are new, specifically authorized limits, not a claim the old exhausted attempts were unused. Count each attempted correction, preserve raw outcomes, apply existing finite build/test/native time and cleanup limits, and stop the affected boundary if the renewed allowance is exhausted. Other blocked or deferred boundaries receive no automatic budget renewal. Initial execution of still-required, previously unattempted mandatory native checks is allowed under the original Goal's stated budget; failures are not passing evidence.

Do not rerun previously accepted work without a concrete input change invalidating its evidence. Do not broaden into all-suite Nextest or global editor/tooling reconfiguration.

Integration rule: Any reviewed change actually admitted to the integration code must be committed and pushed on the existing integration branch, after its own targeted acceptance. The unfinished focused Goal 09/12 worktrees remain preserved; neither focused branch is a substitute integration starting point. Do not cherry-pick an entire unfinished branch, and never import unaccepted code just to advance the queue.

4. Safe order after repairing the two blockers
Reconcile the existing integration branch and manifest; preserve accepted integrations and local partial branches. Begin permanent stall-reporting policy in Section 5 before launching another potentially blocking check.
Complete Goal 09 mandatory acceptance on the integration state, including the destination identity, remaining full-file behavior, resource safety and the Goal's required native/large-file checks. If it cannot pass within bounds, preserve a report and do not begin dependent Goal 10 or 17.
Complete Goal 12's actual test and native acceptance on the integration state; preserve all failed receipts and provide its required end-of-Goal lint/review. If it cannot pass, do not begin Goal 17.
Once Goal 09 is accepted, execute Goal 10 under its original contract and safety budgets, including correct inherited Goal 09 APIs and acceptance. If the original contract demands a separate per-Goal feature branch, that branch must be created from the verified integration tip, and only its accepted result may subsequently be brought back and verified on the named integration branch. Never call that separate branch the integration branch. Do not begin a new branch merely to resume the integration itself.
Goal 17 may start only when Goals 09, 10, and 12 are all demonstrably accepted and included in the verified integration tree. Follow its original branch requirement if any, creating that Goal 17 branch from the exact accepted integrated tip; integrate its verified changes back only under a separately recorded safe handoff. Do not manufacture a missing prerequisite or call Goal 17 completed because a baseline exists.
Keep unfinished Goals 04 editor work, 05, 06 M2-B, 07, 08, 11 M2-A, 13, 14/15/16 follow-on work and any other incomplete requirements explicitly Deferred or Incomplete, unless separately authorized and genuinely verified. Do not merge partial unaccepted edits wholesale.
When the requested integration scope actually passes its gates: perform scoped cross-feature tests, required native checks, review, end-only first-party Clippy, exact-tree verification, normal commit/push, and a factual integration handoff. Produce a PR proposal to master when ready; do not merge master automatically.
If any of the above is inconsistent with the current authoritative ledger, stop and report the exact conflict rather than guessing. No claim of full 17-goal completion unless all mandatory goals are actually complete; a closed queue with deferred/skipped goals is a partial outcome.

5. NEW PERMANENT RULE — tracked Markdown reports for every stalled Codex Goal
User requirement: From this task forward, every substantial Codex Goal, Kickstart, integration phase or queue checkpoint that reaches a real Blocked, Deferred due to failed mandatory acceptance, Paused due to session/resource interruption, Exhausted repair budget, or other genuine stop must produce a human-readable Markdown stopping report inside docs/. This is additional to, not a replacement for, the external QUEUE_STATE.md and raw receipts.

Make this behavior survive future Codex sessions by adding a small, narrowly scoped Codex stalled-goal reporting subsection to AGENTS.md in the integration branch. Keep all existing AGENTS.md content/instructions intact, avoid duplicate guidance if an equivalent rule exists, and include the paths and reporting fields below. This single minimal policy change is specifically authorized; do not perform unrelated AGENTS cleanup.

For this queue, use these tracked repository locations:

docs/plans/execution/multi-launcher-queue-20261025-eedc19d2/
    INTEGRATION_STATUS.md
    stalls/
        G09-<checkpoint>-<UTC-timestamp>.md
        G12-<checkpoint>-<UTC-timestamp>.md
        GXX-<checkpoint>-<UTC-timestamp>.md
        INTEGRATION-<phase>-<UTC-timestamp>.md
For a future standalone Codex Goal without a queue ID:

docs/plans/execution/standalone/<goal-slug>/stalls/<UTC-timestamp>.md
Use filename-safe UTC timestamps such as 20261028T153000Z. Create one concise report per actual stopping event, not repeated duplicate reports for an unchanged state. Keep earlier reports immutable when their evidence is historical; write a fresh report or append a clearly dated resolution update when material facts change. INTEGRATION_STATUS.md is a compact human-readable progress index (branch/last verified checkpoint/active blockers/links to stall reports), not a replacement for the external queue ledger.

Every stalled-goal report must record:

Queue/goal/checkpoint identifier, report time and truthful state (Blocked, Deferred, or Paused, never Completed for unaccepted work).
Exact working branch, local HEAD, upstream/remote HEAD (or unknown), base/accepted carry SHA; confirm which worktree actually owns the branch.
Last fully accepted checkpoint and commit, verified behavior, and a specific list of mandatory acceptance checks still missing.
The precise failure: owned command/test/fixture, expected vs actual, relevant diagnostic/error code, exit status or unknown, elapsed/deadline, cleanup/process-drain disposition, and evidence links/paths. Distinguish successful compilation from executed tests and from native/UI acceptance.
Previous attempts, remaining correction budget, hypotheses distinguished from proven causes, and blockers on dependencies.
Tracked and untracked file state, preserved partial patches/worktree, any CHERRY_PICK_HEAD/merge state, and explicit no-discard safety confirmation.
Minimal next action to resume safely, required authorization when exhausted, and consequences for downstream Goals.
Any unpublished report/push failure stated explicitly; no statement that GitHub contains a file before a successful push.
Reporting and committing rules:

Write the report and update the external ledger before stopping whenever writing is safe. If there are no source changes, a documentation-only stall commit is valid; it must be clearly labeled docs(queue): record blocked <goal>/<checkpoint> and must not be represented as an accepted implementation checkpoint.
Stage only AGENTS.md (if implementing this policy), INTEGRATION_STATUS.md, and the specific new stall report(s). Always inspect the staged diff and current branch before committing; make one normal push to the verified integration branch when policy and Git state allow it. Keep the underlying unaccepted code and original failed receipts untouched.
If the active worktree is in an unresolved cherry-pick/merge, its index is unsafe, or reporting there would overwrite user work, do not commit through the conflict and do not switch branches destructively. First write the exact report to the existing external ledger/evidence directory; materialize the corresponding tracked Markdown report and doc-only commit on the intended integration branch at the first safe reconciliation point. Disclose any report that could not be committed/pushed.
Never store secrets, token values, user private payloads, raw clipboard content, or uncontrolled dumps in these reports. Reference restricted artifacts by safe path and hash, without copying sensitive contents.
Carry the same AGENTS.md rule into future accepted goal branches through ordinary ancestry; it must not require this particular chat to be remembered.
Immediate historical work: In the current integration branch, add a concise Markdown stall report for the present Goal 09/12 integration stopping event based only on the existing records. Label old outcomes as historical and do not invent missing command results. Update INTEGRATION_STATUS.md with the accepted anchor, current working branch/HEAD, dependency blockers and the precise next recovery step. Commit/push these documentation records normally if safe. Subsequent new stalls must also be reported before exiting.

6. Acceptance and final report
Provide progress only at actual state transitions. For each accepted integration checkpoint, show the working branch, original-source SHA(s), applied integration SHA, pass/fail verification evidence, source-safety outcomes, reviewer disposition, process cleanup and push result. For each stop, link the tracked stall report and external raw evidence, identify whether the report was actually pushed, and give the exact next instruction.

Final response must answer:

Which branch was used for all integration work? It must be codex/queue-integration-pre-goal17-20261027 (except explicitly distinct per-Goal feature branches created later under the recorded contract).
Which integrated checkpoints were accepted, and which are still partial/deferred/skipped?
Are Goal 09, Goal 10 and Goal 12 mandatory prerequisites actually accepted in the integrated tree, so that Goal 17 is eligible?
What scoped tests/native checks/Clippy passed or failed, on which commit/tree?
Where are INTEGRATION_STATUS.md and each newly created stalls/*.md report? Were they committed and pushed to the integration branch?
What is the exact current integration HEAD, upstream, remaining blocker, and next safe action? Has a PR been prepared (not automatically merged) for master?
Start now: Reconcile the existing integration branch and local owned worktree; read the ledger and integration stopping receipts; establish AGENTS.md stall-reporting policy and the initial repo-tracked integration/stall status safely; then apply only the two named repair allowances and continue through dependency-safe acceptance. Stop on an unresolved mandatory gate with a pushed Markdown report wherever Git safety permits.