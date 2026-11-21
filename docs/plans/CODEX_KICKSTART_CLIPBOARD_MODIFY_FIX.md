# Codex Kickstart — Fix Clipboard Modify on the Current Branch

You are executing this task in **Codex Desktop** inside the user's existing Multi_Launcher checkout. Implement the fix; do not stop at another plan or a diagnostic-only report.

Read the entire companion [CODEX_GOAL_CLIPBOARD_MODIFY_FIX.md](CODEX_GOAL_CLIPBOARD_MODIFY_FIX.md). It is the feature contract. The Goal and this Kickstart already exist in the repository at execution time; locate them in their current directory, including with `rg --files` if needed. Do not ask the user to upload source, recreate these files, or supply earlier conversations.

## 1. User-authorized overrides — apply before any implementation

For this task, the user explicitly overrides all generic instructions that require a fresh goal/category branch:

- **Do not create a new branch. Do not switch branches. Continue from the current branch and current HEAD.**
- **Do not create a Git worktree, clone, secondary checkout, or per-agent/per-milestone branch.** Use the existing checkout throughout.
- **Do not manufacture an empty M0/bootstrap commit.** Capture the existing start HEAD and make small meaningful implementation commits.
- **Commit frequently and attempt a normal push after each coherent checkpoint, on this same branch.** Preserve user work and history. No force push, reset, automatic stash, rebase, squash, or amend.
- **Clipboard Modify is the only feature being repaired.** Inspect working Snippets and native clipboard helpers as references; do not redesign them or expand into other plugins.
- **No full test suite.** Use focused Modify/platform tests and necessary compilation. No broad regression or lint-cleanup campaign.
- **If native Windows verification cannot be performed, finish the code, available checks, review, commits, pushes, and PR anyway.** Give the user the exact final manual check. Unavailable native tools are not permission to abandon implementation.
- **At the end, create or update a PR from the current branch to `master` and resolve conflicts.** You may merge freshly fetched `master` into the current task branch to do this. Preserve both sides' intended behavior, commit, and push. Do not merge the PR or push directly to master.
- Use **Codex**, not OpenCode. Do not invoke OpenCode commands, orchestration, or old goal queues.

These are explicit task instructions, not optional preferences. Apply them when a historical template or default branching/native-acceptance procedure conflicts. Respect actual tool permissions and preserve unrelated work. Do not rewrite `AGENTS.md` or agent configuration just to encode this one task.

## 2. M0-A — inspect the existing checkout, capture the baseline, then proceed

Read the applicable repository and ancestor/nested `AGENTS.md` instructions, the complete Goal, and relevant Codex role configuration. Keep this task's overrides in effect.

Before editing application code, inspect:

~~~powershell
git rev-parse --show-toplevel
git status -sb
git branch --show-current
git rev-parse HEAD
git remote -v
git rev-parse --abbrev-ref --symbolic-full-name '@{upstream}'
git diff --name-status
git diff --cached --name-status
~~~

A missing upstream is a condition to record, not an automatic implementation blocker. Record `task_branch`, `start_head`, upstream, pre-existing staged/unstaged/untracked work, and the actual Goal/Kickstart paths. Preserve unrelated changes; do not stage them accidentally.

The planning review found `fix/nextest-stack-overflows-and-clipboard` at `39efd5587082f15c081b3be26d886644a41c9c19`, with last code commit `004950b`. This is **not** a command to select that branch or reset to that commit. If the checkout has advanced, use its actual current state.

If the checkout is detached, already has an unresolved unrelated merge, or is on `master` itself, identify the exact conflict with the existing-branch PR requirement. Do not invent a branch or discard work. Otherwise proceed without seeking confirmation.

M0-A is a baseline inspection, not a new-branch checkpoint. No empty commit, separate queue, new plan, or full test run is required.

## 3. Investigate the right boundary without repeating the old stall

The user's reported error is:

`Permanent("The clipboard could not be accessed.")`

Use the Goal's source findings as a head start, then confirm them in the live checkout.

### Primary investigation

- `src/clipboard_modify/clipboard.rs`: `ArboardClipboardBackend::read_text`, `map_protected_read`, `ClipboardService`.
- `src/platform/protected_clipboard.rs`: `read_clipboard_for_modify`, Windows reader, error mapping, worker/deadline logic, private marker, `copy_secret`.
- `src/gui/clipboard_modify_dialog.rs`: asynchronous Load/Reload and guarded mutation state.
- Inspect coordinator/runtime, command handler, and root completion polling only as the actual call trace requires.

Review approximately the latest thirty commits and targeted file history. Explicitly inspect commit `979b112582416a0595e5f4567c4f799f3645938b` and parent `d51a629621001f7b490585cbc7ce8a143f56fcde` using `git show`, not by switching the checkout. That commit moved Modify reads from arboard to protected WinRT. It is a strong investigation boundary, not a confirmed first-bad commit; file history matters because unrelated commits separate it from HEAD.

### Compare Snippets accurately

Plain snippet copying uses `actions::clipboard::set_text` and arboard. Modify already uses arboard for writing. Snippet `sys.clipboard` capture uses the protected brief reader and can discard errors through `.ok()`.

Therefore, working snippet copying supports retaining the writer. It does not prove that a separate “snippets read engine” is available. Inspect the actual operation instead of duplicating a presumed engine.

For native Windows patterns, consult:

- `src/image_compare/clipboard.rs::load_windows_clipboard` for open/close and global-memory guards.
- `src/gui/note_clipboard.rs::windows_clipboard_formats` for format enumeration and correct end/error distinction.
- Existing `windows` 0.58 and arboard dependencies in `Cargo.toml`.

These are references, not new refactor targets.

### Use evidence, then implement

Read existing stage/outcome/HRESULT diagnostics or reproduce with a harmless fixture when feasible. Record the exact native failure if observed. The generic message does not prove access denial; **MTA by itself is not an established cause**.

If logs or native tooling are unavailable, do not ask the user to provide them before proceeding. Use the actual source transition, platform contracts, and focused regression coverage to implement the best supported repair. Historical native-stall reports do not impose a disposable-desktop requirement on this task.

## 4. Use the available Codex agents, with one writer

Use the repository's configured Codex planner/implementer/reviewer roles if the active harness exposes them. Do not change model/config settings or require unavailable role APIs.

- The parent owns scope, sequencing, Git, build/test serialization, and final delivery.
- A planner may perform a bounded read-only check of the reader strategy and hand back a concise implementation packet. This Goal already exists; do not spend a separate phase rewriting it.
- An implementer owns one coherent checkpoint at a time in the same checkout.
- A reviewer independently inspects the finished diff and directly affected invariants, read-only.

Parallel read-only investigation is useful. Overlapping writes, multiple build/test/native processes, competing Git operations, worktrees, and separate agent branches are prohibited. If subagents are unavailable, perform the same responsibilities sequentially yourself and continue.

## 5. Execute the Goal's checkpoints

### M1-A — fix the reader

Choose the smallest complete repair. Correct an identifiable WinRT defect if evidence supports it. Otherwise prefer the Goal's approved native ordinary-text reader for Modify while retaining the existing protected publisher and service boundary. You do not need another user approval for that choice.

For a native implementation:

1. Keep OS calls in the bounded worker, off the GUI thread.
2. Inspect the private marker and retrieve text during one open clipboard scope.
3. Treat marker absence as normal, and marker inspection failure as failure. Do not propagate “format unavailable” as generic access denial.
4. Return a protected-item outcome without reading text when the marker is present.
5. Read `CF_UNICODETEXT` with checked allocation bounds and RAII; preserve empty text, CRLF, and Unicode; reject malformed data safely.
6. Preserve the deadline, worker admission limit, late-result rejection, and eventual cleanup. A wrapper timeout does not cancel a synchronous native call.
7. Add a focused interoperability test using the actual WinRT protected publisher/test seam. A native-only marker fixture does not prove the bridge works.
8. Keep the current writer unless it is part of the demonstrated defect. No blind arboard fallback, privacy bypass, larger-timeout-only fix, synthetic focus workaround, or broad framework.
9. Migrate/remove only reader code made obsolete by this repair; preserve shared callers and protected-copy behavior.

Add or adapt meaningful tests at the production boundary. Inspect the diff, commit a coherent `[M1-A]` increment, and attempt a normal push.

### M1-B — retain guarded Modify behavior

Trace Load, Reload, Apply, Copy Result, immediate commands, overwrite confirmation, and Undo through the repaired path. Preserve:

- session/source/mutation revisions and source authority;
- reset and preview/draft state;
- root repaint and delayed Undo visibility fencing;
- conflict preflight and Undo retention on failure;
- recovery after a later successful read.

Correct only concrete integration defects. Reuse existing relevant tests. If no integration change is required, record that finding; do not create artificial code or empty commits. Commit/push any coherent `[M1-B]` changes before moving on.

### M2-A — focused checks and review

Run the smallest relevant checks during iteration. Discover actual test names. An appropriate final scope at the reviewed baseline is:

~~~powershell
cargo nextest list -p multi_launcher --lib -E 'test(clipboard_modify) | test(protected_clipboard)'
cargo nextest run -p multi_launcher --lib -E 'test(clipboard_modify) | test(protected_clipboard)'
~~~

Do not count zero selected tests as a pass. Use equivalent narrow Cargo tests if Nextest is unavailable. Retain the existing Windows Nextest wrapper; do not start runner maintenance.

Compile the affected Windows target when available. Use a single development build for a native smoke test; do not add redundant check/build/release cycles. Report host/target accurately; Linux-only tests do not verify the Windows implementation.

Follow the Goal's approximate 100-second scoped test-execution budget after compilation, with one build/test process at a time. Do not repeatedly terminate a legitimate first compilation just to satisfy the test-runtime target. No full suite, blanket `--workspace` test campaign, or unrelated Clippy cleanup.

Review the cumulative task diff and fix substantive findings. Rerun only checks affected by those corrections. Commit/push meaningful tests, fixes, and concise execution notes.

## 6. Native verification: attempt a small check, then use the authorized fallback if necessary

When the connected Windows environment and tools allow it, use a known current candidate and harmless fixtures:

1. Copy `ml modify alpha`; open Clipboard Modify; confirm it loads.
2. Copy `ml modify beta`; Reload; confirm the new source.
3. Preview uppercase; Apply; expect `ML MODIFY BETA` on the actual clipboard.
4. Undo, including `cm undo` if configured; expect `ml modify beta`.
5. Cover Copy Result or immediate commands if their path changed, and the protected-publisher/native-reader interoperability check if applicable.

Use the existing test-profile conventions. Separate app data is not an isolated Windows clipboard. Avoid unrelated clipboard contents and personal data in logs/screenshots, and do not terminate unrelated launcher instances.

If the environment cannot perform native/UI verification, **continue through the final review, commits, pushes, conflict handling, and PR**. State the exact limitation. The user has already agreed to final verification; do not ask for intermediate commands or screenshots and do not produce another documentation-only stall.

A native check that actually fails is different: investigate it, correct the relevant defect, and rerun that small check. Do not call a known failure “unverified.” Keep working while a viable scoped correction remains.

## 7. Commit and push on the original branch throughout

Use the project's active checkpoint commit cadence and the Goal's task-specific boundaries.

Before every commit:

- verify the branch still equals `task_branch`;
- inspect current/staged diffs and stage only intended task files;
- preserve unrelated user work;
- record the checkpoint and verification honestly.

Suggested subjects include:

- `fix(clipboard-modify): [M1-A] restore protected ordinary text reads`
- `fix(clipboard-modify): [M1-B] preserve guarded operations after reader repair`
- `test(clipboard-modify): [M2-A] cover access and recovery behavior`

Use only subjects supported by actual changes. Do not delay every commit until a large final batch.

After each checkpoint, attempt one normal push to the matching upstream. If none exists and the intended repository is confirmed, `git push -u origin HEAD` publishes the existing branch. Never force push.

On push failure, retain local commits and continue implementation. Inspect non-fast-forward remote movement separately from authentication/network failure; reconcile only understood changes through a safe same-branch merge. Do not reset, rebase, pull blindly, discard concurrent work, or enter repeated retry loops.

## 8. M3-A — resolve master integration and create the PR

The user has authorized this step. Do not ask whether to open the PR after finishing the fix.

1. Safely fetch the intended remote; record the current `origin/master` SHA.
2. Stay on `task_branch`. If that master commit is not already an ancestor of HEAD, merge it **into this branch** using the Goal's reviewable, history-preserving integration procedure.
3. Resolve normal conflicts autonomously by preserving intended behavior from both sides. Do not choose blanket “ours” or “theirs,” rewrite history, or create another branch/worktree.
4. Preserve pre-existing edits. If integration would overwrite them and they cannot be safely retained, report that specific limitation instead of auto-stashing or discarding them.
5. Confirm no unresolved index entries, inspect resolutions and `git diff --check`, and run only checks affected by the merge. Commit the resolution/merge and push normally.
6. Open a PR with **base `master` and head the existing `task_branch`**, or update an already-open PR with that exact pair. Use the authenticated GitHub connector or `gh`; for a multiline CLI body, use a temporary file with `--body-file`.
7. Verify the PR's URL, head SHA, and actual conflict/mergeability state after the final push. Unknown/calculating is not conflict-free. Use a bounded refresh; handle a newly moved master with only the necessary same-branch reconciliation.
8. Leave the PR open for the user's review. **Do not merge it, push directly to master, change CI policy, or bypass branch protection.**

At the planning snapshot, this branch was 22 commits ahead and 52 behind master and already contained other work. Preserve that history. Review the new fix from `start_head` separately from the full PR comparison and disclose inherited changes in the PR body. Do not create a fresh “clean PR” branch to remove them.

The PR body must contain the problem, cause/evidence, repair, protected-item and state guarantees, actual focused checks, native verification status, integration-base SHA, and any inherited scope. Native verification assigned to the user is not a reason to withhold the PR.

If remote delivery is unavailable, finish all safe local work, preserve the commits, and report the exact delivery blocker and pending PR details. Never invent a URL or claim a successful push.

## 9. Completion and final response

Do not stop after analysis, the first test failure, a normal conflict, or an unavailable native test. Fix substantive task defects and finish the authorized delivery. Stop once the scoped work is complete; do not expand into optional improvements.

Start the final response with the appropriate clear status:

- **Done — Clipboard Modify fixed; native Windows verification passed.**
- **Done — Clipboard Modify implementation complete; ready for your final Windows verification.**
- **Blocked — [specific unresolved feature or integration/delivery blocker].**

Then report concisely:

1. The actual cause or best-supported diagnosis, and what changed.
2. Relevant files and any material remaining uncertainty.
3. Exact focused checks run/results; native checks performed or pending.
4. Original branch, start HEAD, checkpoint SHAs, final tip, and actual push state.
5. PR URL, head SHA, master SHA used for conflict checking, and actual mergeability status.
6. Any inherited PR scope or external delivery limitation.
7. If needed, the short fixture-based final Windows check and expected outputs.

Use “unfixable” only if the evidence supports that conclusion after viable approaches have been assessed. Missing native tools, missing logs, or a blocked push do not establish that the feature cannot be repaired. If a genuine blocker remains, preserve work and give a precise evidence-based report under the repository's applicable conventions.

**Begin with the read-only M0-A inspection, then implement the Goal on the current branch. Do not create a branch.**

