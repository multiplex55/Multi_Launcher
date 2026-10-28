# Codex Kickstart — EXECUTE Existing Pre-Goal-17 Integration Recovery

**Repository:** `multiplex55/Multi_Launcher`  
**Queue:** `multi-launcher-queue-20261025-eedc19d2`  
**Local repository previously reported:** `G:\Repos\rust\Multi_Launcher`  
**Authoritative integration branch:** `codex/queue-integration-pre-goal17-20261027`  
**Task:** Perform the already-authorized recovery, resolve the specific remaining acceptance boundaries, and establish permanent, GitHub-visible Markdown reporting for future stalled Codex goals. **Execute the work. Do not stop after giving another diagnosis, producing a new plan, or rereading `continue.md`.**

## 1. Exact starting point and authority

The *latest GitHub-verified* integration branch HEAD at the time this instruction was written is:

- `0d08386ab1c78288273d190ed57f496ad74db301` — documentation-only `Update continue.md`.
- Its parent `478b08b9b797e2504ace03846f5556212c7c8a18` added the detailed integration-recovery kickstart.
- **Last integrated code commit:** `2a7535086f00359c36037c32cdd3a26f95b9c9bd` — selected Goal 16/M1-A integration.
- Accepted shared ancestry includes `ceff1f17fb09545d8b9a6010aae9eca7892a7ea7` (`CARRY04`); **it is not the restart tip**.
- `master` was `bbc2ca1ef247c53eeae9f2ce7ba69f381483318d`; **do not modify or merge to master**.

These are observations, *not commands to reset*. Reconcile the current local branch, upstream, worktree, external ledger, and any newer verified commits. Continue from a newer legitimate descendant if present.

Read and obey the existing, already-committed authority:

1. `docs/plans/CODEX_KICKSTART_RESUME_INTEGRATION_AND_STALL_LOGGING_20261028.md` — **full contract for this recovery and for durable stall reporting**.
2. The durable external `QUEUE_STATE.md` for this exact queue, plus `integration-pre17/SOURCE_COMMIT_MANIFEST.json` and `integration-pre17/BLOCKED_RESUME.md` in its recorded evidence root.
3. Goal 09 and Goal 12 `DEFERRED_HANDOFF.json` and their exact preserved patch manifests, source bindings, prior attempted commands, and raw receipts.
4. The original Goals 09, 10, 12, and 17 and their companion kickstarts, plus applicable `AGENTS.md` and actual runner configuration.

**This is an execution prompt for the existing authorization, not a new repair-budget renewal.** The existing kickstart already authorizes up to **two additional focused correction rounds each** for (A) Goal 09's destination-identity publication boundary and (B) Goal 12's erroneous unit-returning undo/redo assertions. Count any rounds already used against those allowances. Do not reset them by rereading this file or starting another session. Goal 04's earlier exhausted editor repair allowance is **not** renewed.

## 2. Mandatory branch/worktree guard — before any mutation

Use **only** `codex/queue-integration-pre-goal17-20261027` for the integration work. The initial reconciled HEAD should be the latest verified safe descendant of `0d08386a`, not `master`, a focused Goal 09/12 branch, or `CARRY04`.

Before any code edit, commit, or build:

- Read-only: inspect `git status -sb`, exact local/upstream HEADs, remotes, index/staged/unstaged/untracked paths, `git worktree list --porcelain`, and any `CHERRY_PICK_HEAD`, `MERGE_HEAD`, or rebase/sequencer state.
- Locate the **existing integration-branch-owning worktree**. If this branch is already checked out in a different worktree, use that safe worktree rather than switching another one or creating a parallel integration branch. If safe checkout of the existing branch is required, preserve any unrelated user work and never force a switch.
- Reconcile any active queue-owned job by recorded process ID *and creation identity*, command owner/lease, actual result, and finite deadline. Do not launch a duplicate build/test/agent while another owner is working. Never kill arbitrary processes by name.
- Compare local tracked changes against the exact preserved Goal 09/12 patches and the accepted integration manifest. Do not conflate incomplete focused-branch edits with accepted integration code.
- Record the chosen worktree path, active branch, current local HEAD, upstream HEAD, last accepted integration checkpoint and next task in the **existing external** `QUEUE_STATE.md`.

If the current integration branch has an unresolved cherry-pick/merge, unsafe index, unaccounted concurrent writer, or conflicting user-owned edits, **do not reset, stash, force-checkout, abort, or overwrite anything**. Preserve the state and follow the stall-reporting escape path in Section 3. Do not report a fabricated successful recovery.

For *every* integration commit and push, recheck the current branch, upstream, staged diff, and task-owned files. Keep the integration commit history as normal additive commits. Do not create `integration-v2` or an alternate branch just because an earlier check failed.

## 3. First actionable checkpoint: permanent tracked stall reporting

**Do this before the next potentially blocking verification campaign**, once the integration worktree is safe. This is a required concrete deliverable, not optional planning:

1. Add one minimal, durable subsection to `AGENTS.md`: when any substantial Codex Goal, queue checkpoint, or integration task ends **Blocked**, **Deferred due to failed mandatory acceptance**, **Paused**, **repair-budget exhausted**, or at another genuine stopping condition, write a human-readable Markdown stopping record under `docs/plans/execution/`. For this queue, use the folder below; for standalone future tasks use `docs/plans/execution/standalone/<goal-slug>/stalls/<UTC-timestamp>.md`.
2. Create/update:
   - `docs/plans/execution/multi-launcher-queue-20261025-eedc19d2/INTEGRATION_STATUS.md`
   - `docs/plans/execution/multi-launcher-queue-20261025-eedc19d2/stalls/G09-<checkpoint>-<timestamp-UTC>.md`
   - `docs/plans/execution/multi-launcher-queue-20261025-eedc19d2/stalls/G12-<checkpoint>-<timestamp-UTC>.md`
3. Ground initial historical reports **only** in `QUEUE_STATE.md`, `BLOCKED_RESUME.md`, recorded raw receipts, and preserved patches. Distinguish the historical stop from current live state. Do not invent a failed test result or falsely say local work is absent because GitHub does not show it.
4. Each report must identify actual branch/worktree/HEAD/upstream, last verified checkpoint, error/expected-vs-actual, exact scoped command and exit status (or unknown), deadlines, tests actually executed, native results, owned process cleanup, unchanged/missing acceptance gates, original evidence location, consumed/remaining correction rounds, protected uncommitted files, and **the smallest safe next action**. Link the report from `INTEGRATION_STATUS.md`.
5. Keep the existing external `QUEUE_STATE.md` as the **one operational ledger**; tracked Markdown reports are a searchable human-readable index and stopping history, not a competing authority. Do not embed secrets, clipboard payloads, or private logs.
6. Once the index is safe, stage **only** the `AGENTS.md` policy addition, `INTEGRATION_STATUS.md`, and these reports. Review the exact staged diff, make one clearly labeled **documentation-only** commit, and normal-push it to **this integration branch**. Do **not** call the documentation commit an accepted feature checkpoint.

If the Git index is unsafe or an active conflicting operation makes a commit inappropriate, write the report in the external evidence/ledger first, leave the source unchanged, and materialize/push the tracked record at the **first safe integration-branch checkpoint**. State plainly if the GitHub-visible record could not yet be pushed. **Every subsequent genuine stop must likewise produce a new timestamped Markdown report, even when no code checkpoint was accepted.** Do not create duplicate reports simply for repeated no-change inspections.

## 4. Execute Goal 09 recovery on the accepted integration tree

The still-open Goal 09 requirement is *destination identity when publishing a file result*. A path-only comparison is inadequate because a destination can be replaced or repointed after approval. The previous accepted file-backed source/page checkpoints remain retained, but full Goal 09 acceptance does not exist.

- Inspect the integration code's actual publication owner and the saved seven-file focused patch **read-only first**; determine the legitimate write/approval and Windows destination-identity boundary. Do not assume the patch itself is already accepted.
- Use one bounded planner packet, one source writer, and a focused reviewer. Correct the actual unsafe ownership/identity boundary without weakening user approval or altering legacy compact-file behavior.
- Prove approved unchanged identity succeeds; a replaced/repointed/changed destination fails closed **without overwriting another file**; ownership, cancellation, temp-file cleanup, and previous-source/preview state stay correct. Cover race-sensitive identity behavior with realistic Windows-backed evidence where required by the goal.
- Complete the original Goal 09 remaining required functionality and evidence: full-file filter/sort/streaming export, representative **>500 MiB** file behavior, bounded quota/cancellation/error cleanup, and native GUI acceptance. Do not treat the previously passed 16 MiB/220,000-row scenario as the >500 MiB gate.
- Reuse previously accepted evidence only when source identity and affected behavior remain unchanged. Run only relevant focused tests and originally mandated native checks with finite deadlines. Preserve failures; do not broaden into an unrelated full Nextest suite.
- Commit/push only *accepted, reviewed* changes to the **integration branch** and record exact SHA, passing verification, and remaining gates. If the named two-round correction allowance is exhausted, write/push its Markdown stall report, mark Goal 09 incomplete, and do not start dependent Goal 10 or Goal 17.

## 5. Execute Goal 12 recovery on the accepted integration tree

The recorded failing QA build reported **nine Rust `E0600` errors** because new assertions treated screenshot-editor `undo`/`redo` methods returning `()` as though they returned `bool`. The earlier production application build succeeded, but **zero QA tests ran** in that failed build.

- Inspect the *actual* current undo/redo API and the preserved four-file incomplete focused patch. Repair the **test contract** to assert observable editor state, history, render/output behavior, and correct undo/redo effects. Do not change the production API merely to make a false Boolean assertion compile or replace meaningful assertions with trivial checks.
- Build only affected test targets, confirm a nonzero inventory, execute selected tests, and retain the actual compile/run outcomes. Verify the native Windows editor, redaction/output/export/help flow and all original mandatory Goal 12 acceptance checks under bounded deadlines and approved source handling.
- Retain earlier M1-A/M1-B passes only where valid; do not mislabel them as M2-A, native, final lint, or full completion. Have implementation and changed tests reviewed, then perform the appropriate **end-only first-party Clippy** on the verified final tree.
- Commit/push accepted changes on the **integration branch**. If the specific two-round test-contract allowance is exhausted with a mandatory failure remaining, preserve the partial patch, write/push the Markdown stall report, and keep Goal 12 incomplete. Do not start Goal 17.

If Goal 09 is blocked, continue independent Goal 12 recovery *only when source/process isolation and the existing queue amendments permit it*, but keep Goal 10 and Goal 17 gated. Do not treat safe integration of partial prior checkpoints as a final Goal.

## 6. Dependency-safe continuation and clean handoff

- **Goal 10 requires fully accepted Goal 09.** Do not bootstrap or accept Goal 10 before Goal 09's mandatory gates pass.
- **Goal 17 requires fully accepted Goals 09, 10, and 12**, all verifiably present in the accepted integrated tree. A baseline, partially imported code, or documentation-only repair does not establish these prerequisites.
- If a separate feature branch is **actually required** by a Goal's original contract, create that Goal-specific branch from the verified integration tip, verify and commit its work there, then integrate its accepted content back into `codex/queue-integration-pre-goal17-20261027` with checks. Such a feature branch must **never** become a substitute integration branch.
- Preserve Goals 04 editor, 05, 06 M2-B, 07, 08, 11 M2-A, 13, and incomplete follow-ons for Goals 14–16 as honestly **Deferred/Incomplete**, unless independently authorized and verified. Do not silently cherry-pick unfinished code, relabel incomplete work as complete, or erase deferred state.
- Once requirements permit, run *focused cross-feature integration verification*, applicable native checks, review, final first-party Clippy, exact-tree verification and normal pushes. Prepare a factual integration handoff/PR proposal; **never auto-merge or push to `master`**.

## 7. No-progress loops, bounded repairs, and final report

Make a real action on the first safe step: **branch reconciliation → durable stall-report policy and initial pushed reports → Goal 09 scoped repair → Goal 12 scoped repair → accepted handoff**. Do not use an execution turn merely to re-summarize the same problem, write another recovery kickstart, or wait for routine approval. Continue autonomously until the authorized work completes or a genuine safety/mandatory-acceptance blocker requires stopping.

Work within the **already granted** two focused correction rounds per named boundary (minus any rounds actually consumed). Preserve original failures and exact repair-count accounting; do not silently extend deadlines or manufacture new attempt budgets. Track job admission, compile vs test execution, real native results, ownership/cleanup, reviewer mode, and Git push outcome separately.

Before an unavoidable stop, save an exact resume point to the external ledger and a timestamped Markdown stall report. If Git is safe, commit/push the report on the **integration branch** even if no implementation passed; if unsafe, leave the external report and state that a repo push is pending. Do not imply continuing in the background without an active Codex session.

Report concisely:

- Integration worktree and **current actual** branch/HEAD/upstream, plus which original checkpoint SHAs and new integration SHAs are included.
- `AGENTS.md` reporting rule and **actual pushed URLs** to `INTEGRATION_STATUS.md` and stall reports (or why not pushed).
- Goal 09, Goal 12, and Goal 10/17 prerequisite statuses; distinguish verified completion from deferred/unverified.
- Exact selected commands and compilation/test/native/Clippy outcomes, source restoration and cleanup, reviewer disposition, and remaining correction-round count.
- The smallest remaining blocker and exact safe next action, or the accepted integration handoff/PR proposal.

**Do not modify `master`, rewrite history, discard user changes, create a second integration branch, or terminate without a durable status record. START EXECUTION NOW.**
