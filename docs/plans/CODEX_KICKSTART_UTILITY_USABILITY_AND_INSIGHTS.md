# Codex Kickstart — Utility Usability and Visual Insights

Execute `CODEX_GOAL_UTILITY_USABILITY_AND_INSIGHTS.md` autonomously in the current repository. Implement the complete approved Goal, verify the changed boundaries, commit frequent checkpoints, attempt best-effort pushes and deliver one final report. Do not stop after individual milestones to request permission to continue.

The Goal specifies what to build and the acceptance contract. This Kickstart specifies execution discipline. Read the full Goal; this document is not a replacement for its requirements, source findings, defaults or verification cases.

## 1. Mandatory first execution: exact predecessor → fresh child → M0-A → push

**Before editing application code, running implementation builds/tests or beginning M1, create this Goal's own unique branch and baseline in the single existing checkout. The Goal and Kickstart share one task branch.**

1. Read `AGENTS.md`, any applicable local instructions, the entire Goal and this Kickstart. Perform read-only Git checks: `git status -sb`, current branch/HEAD, remotes/upstream, dirty/untracked files and the queue/orchestrator/execution records of the previous completed Goal/category.
2. Resolve and record `parent_branch` and `parent_tip_sha`. For a continuing sequence, the required base is the **exact final commit of the immediately preceding completed Goal/category branch**, including a newer local-only tip when its push failed. Verify the actual commit and current records; safely fetch a remote predecessor when necessary. Do not hard-code an older conversation SHA or assume the previous Studio and Utility Polish report is still the latest handoff.
3. **Only the first task of a genuinely new chain** defaults to the up-to-date primary integration branch, normally master/main. Being currently checked out on master, or seeing a merge of the previous Goal, is not permission to restart the chain there. Check for subsequent user/category work that would otherwise be omitted. If records conflict, an ancestor is unavailable, or an exact-base transition would lose work, report the precise blocker rather than silently choosing an older base or importing commits.
4. Choose a unique idiomatic lowercase branch, preferably `feat/utility-usability-and-insights` with a unique suffix when needed. Protect existing work: never automatically reset, hard-pull, stash, force-checkout, rebase, merge, overwrite files or stage unrelated changes. A documented safe current-HEAD initial base is allowed only for the first task without a predecessor and without losing/committing user work.
5. Create and switch directly from the verified parent SHA, for example `git switch -c feat/utility-usability-and-insights <parent_tip_sha>`. Verify initial HEAD equals that SHA. Do not implement on the predecessor or master/main.
6. Make **this branch's own M0-A baseline commit**, preferably a concise task ledger or an intentional empty baseline commit. Record `baseline_sha` separately and verify that `git rev-parse <baseline_sha>^` equals `parent_tip_sha`. The predecessor's final commit is not this Goal's new baseline.
7. Attempt one normal initial upstream push, such as `git push -u origin HEAD`, if permission/approval permits. Record actual success, failure or denial. If pushing is unavailable, retain the commit and **continue safe local work** without retry loops, force push, approval bypasses or repeated requests for user intervention.
8. Perform all subsequent edits, builds, tests and checkpoints on this same branch. **Never use Git worktrees, `git worktree add/move/prune`, auxiliary checkouts, secondary clones, agent-specific branches or checkpoint branches.** Do not merge/rebase/cherry-pick intermediate category branches merely to advance execution.
9. At completion record the actual `final_tip_sha` after the last commit. The next authorized Goal must create a new branch **directly from that exact final tip**, not from master/main or an older common ancestor. If that commit is local-only, another machine must obtain it before continuing. The last branch carries the cumulative chain; integrate only at a separately approved final boundary.

Do not advance past M0-A until parentage, branch identity, baseline, push outcome and user-work protection are documented. Keep a single task ledger under current repository conventions, preferably `docs/plans/execution/standalone/utility-usability-and-insights/STATUS.md`.

## 2. Validate the implementation packet without expanding it

After the Git gate, inspect the relevant current source and tests. The Goal's anchors guide this inspection; adapt moved files while preserving the approved behavior. Do not invent missing symbols, assume an old document describes current code exactly, or treat earlier goals' native acceptance campaigns as this Goal's gates.

There are **17 required outcomes**:

| Area | Scope identifiers |
|---|---|
| File Search | FS1 Clear results; FS2 literal filename-only contains mode. |
| Window presentation | H1 child inheritance; H2 remembered presentation across reopen/restart. |
| Launcher grid | L1 compact descriptions and bounded cards. |
| Diagram Studio | D1 Colors placement; D2 menu/action separation; D3 endpoint labels; D4 pinned properties/arrangement layout; D5 connector creation/reconnection. |
| Programmer Inspector | I1 bit-label file export/import. |
| Dashboard Editor | B1 identifiable preview slots. |
| Recording | R1 zero/unlimited; R2 1–1,000 seconds, 25-second steps and exact typing. |
| Folder Size | F1 retained-data treemap; F2 independent view visibility in the existing host. |
| Calculation Pad | C1 separate optional Dependency Explorer. |

Use the Goal's explicit defaults. In particular:

- An accepted open from a popped-out ordinary parent requests a popped-out ordinary child; direct/root opening uses remembered preference. Focus/promote an existing child, never duplicate or discard its session. Do not turn menus, native file pickers, confirmations or overlays into ordinary detached tools.
- Remembered presentation is separate from live host retirement. Do not persist the internal `retire_to_docked` fallback as the user's choice. Preserve correct owner/host-generation/pass checks and independently owned settings writes.
- Shorten the inline description, not the user's stored data. Width constraints are mandatory even for a single long word; cache measurement and actual card painting must agree.
- Filename contains is an additional literal leaf-name mode, not a new name for path-fallback Ranked Substring or Fuzzy. Clear must invalidate active results, not just empty a vector while a worker keeps publishing.
- Colors belongs directly under Suggestions inside the Mermaid source column. Top menus are separate from immediate actions. Visual arrangement tools go above the canvas; properties/actions go in the right collapsible independently scrolling sidebar.
- Connector endpoint labels use owned object text plus IDs. Existing route-point dragging and connected-object movement are not the requested endpoint-reassignment behavior. Preserve primitive double-click text editing.
- Label import/export exchanges bounded label definitions only. No implicit numeric mode change, Pad edit, comparison change or profile-manager expansion.
- Recording `0` means no automatic time stop, not unlimited memory/disk or bypassed target/error safeguards. Widen cumulative GIF timestamps and handle bounded per-frame delay chunks; do not merely change 120 to 1,000.
- The treemap uses the same accepted scan tree and dialog as the table. Selection, drill-down, resizing and showing the chart must not trigger another scan. Both table and chart can be hidden individually, with a reachable way to restore them.
- Dependency Explorer is a separate companion dialog using cached evaluation and stable line identities. Do not replace/enlarge the normal Pad or add a second evaluator.

## 3. Use the configured agents and one implementation lane

Follow the actual `.codex` configuration and repository agent instructions; do not change model selections or require tools/roles that are unavailable.

Use the planner for bounded validation of source ownership, dependencies and checkpoint scope; the implementer for sequential coherent changes; the reviewer for one cumulative independent review; and the orchestrator for acceptance, commits, pushes and reporting. Bounded read-only reasoning/review may overlap when safe. **One source writer and one Cargo/build/test/native lane at a time**, all in the same checkout. No parallel implementations in separate working directories.

Have each implementation packet identify its Goal checkpoint, required behavior, allowed files, directly affected tests and preserved invariants. Do not delegate a vague “make the whole application better” task. Return lane ownership explicitly before another agent edits or builds.

## 4. Execute all milestones without intermediate approval stops

Recommended sequence:

| Milestone | Checkpoints and delivery |
|---|---|
| M0 | M0-A: exact-parent branch and baseline, push outcome and inventory. |
| M1 | M1-A: root descriptions/card geometry. M1-B: Dashboard slot labels. |
| M2 | M2-A: safe results clearing. M2-B: filename-only matching. |
| M3 | M3-A: remembered presentation/persistence. M3-B: custom host coverage and child origin. |
| M4 | M4-A: Mermaid menu/colors layout. M4-B: Visual toolbar/sidebar. M4-C: text-bearing endpoints. M4-D: interactive creation/reconnection. |
| M5 | M5-A: bit-label codec, file UI and guarded replacement. |
| M6 | M6-A: duration/timeline/encoders. M6-B: exact duration input and UI. |
| M7 | M7-A: pure retained-tree treemap. M7-B: linked UI/visibility. |
| M8 | M8-A: cached dependency read model. M8-B: companion UI/navigation. |
| M9 | M9-A: cumulative review, focused final checks, documentation and handoff. |

Independent work may be reordered for implementation locality or build efficiency after M0-A. Record the order and preserve real dependencies: shared presentation before new companion-host integration, pure models before their UI, and layout before final gesture acceptance. Ordinary milestones do not create new branches.

Use the project's active checkpoint commit cadence and the task-specific boundaries in the Goal. Commit each verified coherent increment with its M-label, inspect the staged diff and attempt an ordinary push. Retain successful local commits on push failure and continue. Use small same-branch remediation commits rather than rewriting history or restarting milestones.

Do not pause after a UI polish checkpoint, after the shared host work, or after any other intermediate milestone merely to ask whether to continue. Complete every independent approved requirement, even when another item has an external native-verification limitation. Do not add optional recommendations from earlier conversations to this scope.

## 5. Verification: focused, bounded and truthful

The Goal's checkpoint packets and Section H are the validation contract. Use domain tests, rendered interaction tests, persistence races and targeted host integration cases appropriate to the change. Do not substitute a test of moving a connected object for a test of actually changing a connector endpoint. Do not substitute final-value output for proof that a duration remained unlimited on no-frame/idle paths.

Use the existing fast-dev profile and owned-process tooling. Compile relevant test targets separately, then run targeted `cargo nextest` selections with a **100-second execution deadline per batch**; compilation is outside that execution cap. Read the actual script interface before invoking it. Use `--no-fail-fast` for a small useful batch, and rerun failures individually or with their immediate neighbors. Split a larger focused batch rather than bypassing the time budget.

Long recording tests must use injected clocks and bounded encoder fixtures, including elapsed time beyond 120, 655.35 and 1,000 seconds. Do not wait for a real 1,000-second clip. For treemap navigation use a scan-service spy or equivalent to prove no new scan. For Dependency Explorer prove source/history/dirty state are unchanged by browsing/navigation. For presentation prove cancelled close and stale settings writes do not erase the preference.

Use at most two focused remediation/retest rounds per concrete issue before reassessing the cause and recording a precise blocker. This is a bound on blind repetition, not permission to mark an unfixed blocking defect complete. Continue independent features where safe.

Avoid full-suite runs, repeated release builds, broad benchmarks, new screenshot-diff infrastructure and inherited historical validation campaigns. Run one final scoped first-party Clippy pass; fix introduced warnings and report unrelated baseline findings without vendored cleanup. Build one current runnable executable for consolidated native smoke after source review/remediation.

Launch with a disposable task-owned profile and safe fixture settings. Use supported window observation/input/screenshot tools where available; do not guess pointer coordinates, bypass unavailable geometry, or terminate the user's unrelated processes. Capture evidence for the Goal's grouped native flows. If capture/input fails, allow one justified refreshed attempt or evidence-backed correction, then document the limitation rather than retrying identical operations across every tool. Clean up only task-owned processes and drain runner output.

**Never claim a test/native interaction passed unless it was executed.** Distinguish implemented, automated-verified, native-observed and unverified. Environmental native limits yield LIMITED native acceptance, not fabricated success and not an indefinite implementation pause. Do not ask the user to run intermediate commands or screenshots; their manual review happens after delivery.

## 6. Final review and stop boundary

Obtain one cumulative independent review of this Goal's diff. Concentrate on settings ownership/presentation lifetime, accepted effects, request cancellation, canvas transactions, file bounds, encoder timing, scan accounting and stable-row navigation. Resolve relevant findings with focused remediation and reviewer confirmation; do not launch repeated broad review campaigns.

Check all 17 requirement IDs against the acceptance contract, update only the directly relevant help/docs and record actual results. Preserve pre-existing user work and unrelated warnings. If anything is genuinely blocked, explain the failing boundary, evidence, attempted fixes and smallest next decision; do not conceal it behind a general “complete.”

After the actual final commit, report:

- Completed milestones/checkpoints and a compact per-requirement implementation/automated/native status matrix.
- Important design decisions and affected files.
- Actual commands/selectors, build/test/lint outcomes and native evidence locations.
- Reviewer findings/resolution, unrelated baseline issues, remaining feature blockers and environmental limits.
- `parent_branch`, `parent_tip_sha`, current task branch, `baseline_sha`, significant checkpoint SHAs, **exact `final_tip_sha`**, upstream/push state and checkout cleanliness. State that unrelated user work was preserved.
- A simple grouped manual checklist for final user review.
- The explicit next-task instruction: **create the next fresh Goal/category branch directly from this final tip, not master/main; no worktrees, helper clones or intermediate merge/rebase/cherry-pick is used merely to advance the chain.**

Stop when the approved scope and bounded verification/reporting are finished. Do not start another feature, rewrite unrelated code, expand into a general performance project or merge into master without a separate authorized integration instruction.
