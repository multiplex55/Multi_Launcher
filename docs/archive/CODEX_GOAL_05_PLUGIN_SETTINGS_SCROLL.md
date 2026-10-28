# Codex Goal 05 — Keep Plugin Settings usable with long descriptions

**Companion execution prompt:** [CODEX_KICKSTART_05_PLUGIN_SETTINGS_SCROLL.md](CODEX_KICKSTART_05_PLUGIN_SETTINGS_SCROLL.md)  
**Requested coverage:** Explicit Plugin Settings horizontal-scrolling request  
**Program position:** 05 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `fix/plugin-settings-scroll` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish plugin-settings-scroll branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
6. **Attempt the initial upstream push once.** After the baseline commit exists, perform one normal non-force push to this task branch when an appropriate remote and platform approvals permit, for example `git push -u origin HEAD` when `origin` is verified as the intended repository. If denied/unavailable, record the real reason and keep working locally. Do not retry in a loop, bypass protections, or require the user to resolve ordinary push failure.
7. **Verify the bootstrap and remain on this branch.** Record branch, original base, baseline SHA, pre-existing work and initial push result in the execution ledger. A ledger created by the baseline commit can record its resulting SHA immediately afterward and include that update in the next legitimate checkpoint; do not create recursive commits merely to record their own hashes. Do not silently return to the prior branch.

### M0-A acceptance and commit contract

**Objective:** A unique active task branch with its own committed baseline and an honest initial push result.

**Code ownership:** Git state and an optional task execution ledger only; no application code or test execution yet.

**Tasks:** Complete steps 1–7 above, check the dependency contract below read-only, and keep existing user work untouched.

**Behavior after this checkpoint:** All later feature work and verification happen on the new branch and are measured against the new baseline commit.

**Edge cases:** Missing remote/upstream, denied fetch/push, existing branch name, dirty checkout, stale local integration tip, missing prerequisite or unsafe transition.

**Regression constraints:** No user change staged/lost, no destructive Git command, no force push/history rewrite, and no implementation on `master`, `main` or the old feature branch.

**Verification:** Inspect branch/HEAD, baseline commit, worktree/staged diff, base provenance and actual push output. No application build is needed to prove bootstrap.

**Acceptance:** Unique branch is active; baseline commit exists on it; original base and baseline SHA are recorded; push was attempted or its policy/access unavailability is recorded; unrelated changes are preserved. If branch/baseline cannot be established safely, stop with a precise blocker. Push failure alone is nonblocking.

**Commit boundary:** The first commit is the M0-A baseline. Every later checkpoint stays on this new branch; normal best-effort push follows each commit.


## A. Goal and desired outcome

The Plugin Settings window remains within the available launcher viewport even when a plugin has a very long description. Its plugin list scrolls horizontally and vertically, all descriptions and capability controls remain reachable, and existing enable/save/search-navigation behavior is preserved.

### Dependency and branch-base contract

**Required predecessor capabilities:** None.

**Recommended earlier work:** [01 — Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md), [02 — Faster isolated tests and efficient GUI reference coverage](CODEX_GOAL_02_TEST_EFFICIENCY.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[src/plugin_editor.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/plugin_editor.rs)** — `PluginEditor::ui, especially window/list construction around lines 216–271`. The current window contains a filter and a vertical-only ScrollArea capped at 200 logical points. Each plugin checkbox/name and its description share a horizontal row. Long description width can determine the content width; there is no explicit viewport-derived width bound.
- **[src/plugin_editor.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/plugin_editor.rs)** — `PluginEditor::save_settings; navigation_control closure; plugin_editor_control_id`. Plugin and capability changes use the existing asynchronous settings-save path. The editor disables changes while a save is pending. Settings Search navigation validates plugin identity, epoch and catalog revision before observing the actual checkbox response.
- **[src/gui/render.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/render.rs)** — `settings_search_cross_owner_plugin_editor_focuses_current_capability_and_rejects_stale_ambiguous_rows`. An existing focused GUI test covers current-plugin capability focus, stale navigation rejection, filtering and close/reopen behavior. This is relevant regression coverage for changes to the scroll container.

## C. Approved functional requirements

1. Add horizontal scrolling to the existing plugin-list region while retaining vertical scrolling and the filter above that region. Full descriptions must remain accessible; do not substitute truncation for horizontal scrolling.
2. Use a practical default width, resize behavior and available-viewport width bound so a long row does not force the whole dialog off-screen. Preserve a usable height and the existing capped vertical list behavior unless a small necessary sizing adjustment is justified.
3. Preserve plugin order, case-insensitive filtering, enabled-plugin/capability semantics, settings-save pending state, persistence and normal close behavior.
4. Keep stable response identities and Settings Search/Jump Mode registration. Revealing a requested checkbox must scroll the relevant axes sufficiently to make that actual control visible.
5. Respect the effective UI scale and available viewport, including opening with previously remembered oversized window geometry. No new user preference or persisted layout format is required.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- Plugin metadata redesign, new plugin capabilities, settings architecture refactoring, changed descriptions, new command behavior, or broader settings-window redesign.
- A new scroll helper framework or a new test harness for this surgical layout fix.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Fix the owner in PluginEditor::ui. Replace the list's vertical-only scrolling with a two-axis scroll region and give the containing window/list an effective finite viewport width, using existing egui window sizing patterns.
2. Keep the filter, pending-save feedback and control-response observation in their existing ownership. Check how the current egui version handles horizontal content measurement and auto-shrink before selecting the minimal combination of width/scroll options.
3. Reuse the existing settings-navigation test and native GUI harness. A small visual fix does not require a new fixture framework, a full suite or repeated release builds.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Bound the dialog and make every plugin row reachable

**Objective:** Complete the horizontal-scroll fix and prove that plugin controls remain usable.

**Code ownership:**

- src/plugin_editor.rs
- src/gui/render.rs — existing focused regression only, if adaptation is genuinely needed

**Implementation tasks:**

1. Read the existing window sizing and ScrollArea behavior in the checked-out egui version; reproduce the long-description expansion with a disposable plugin profile.
2. Constrain the available window/list width and enable both scroll axes for the plugin list without moving the filter into the scrolling row contents.
3. Preserve stable widget/control IDs and navigation observation. Ensure programmatic reveal of a capability checkbox works after horizontal or vertical scrolling.
4. Launch the changed application under a disposable profile, inspect narrow and ordinary widths, scroll both axes, toggle one plugin/capability and close/reopen. Capture the visible horizontal scroll and reachable controls.
5. Run the existing relevant Settings Search navigation test with a bounded targeted selection if the touched geometry changes that path; inspect the diff and commit the coherent fix.

**Behavioral requirements:**

- Long descriptions stay inside the dialog's scroll viewport and can be read by horizontal scrolling.
- Filter, checkboxes, capability controls and close remain usable at the tested widths/scales.

**Edge cases:**

- Very long unbroken descriptions, many capabilities, no filter matches, a small available viewport, raised UI scale and saved oversized geometry.
- A settings-navigation target that begins outside the horizontally or vertically visible region; a pending save while the list is scrolled.

**Regression constraints:**

- No plugin enable/capability value changes merely from resize or scrolling.
- Existing filtering, async saves, settings-search owner validation and Jump target identity remain intact.

**Verification:**

- One targeted compile/build of the necessary application/test targets, reusing the selected profile and cache.
- One existing focused settings-navigation test selection when relevant; one native session with narrow width, ordinary width and a raised-scale view. Verify actual horizontal movement and checkbox interaction rather than only scrollbar presence.

**Acceptance criteria:**

- The long-description fixture no longer grows the dialog beyond the available width.
- Horizontal and vertical scrolling expose all tested content; filtering, toggling, save feedback and reopening preserve values.
- Required native evidence and relevant existing regression results are recorded; no unrelated files or settings changed.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `fix(plugins): [M1-A] constrain settings width and enable horizontal scrolling` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Use the configured development build mode once; allow normal Windows compilation time and log build/test stages separately. Do not put a short GUI timeout around compilation.
- After compilation, cap the focused test execution at 3 minutes and the native session at 5 minutes. Permit at most two focused remediation attempts; if native execution is unavailable, report that unverified behavior explicitly.
- Inspect existing control values before/after scrolling and use disposable settings with Paste Sequence next_hotkey and cancel_hotkey disabled before app construction. No full-suite, release rebuild or broad Clippy cleanup is justified by this fix.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| Adding a horizontal ScrollArea without constraining its parent can leave the original width expansion unchanged. | Verify the actual window and clipping rectangles on a narrow viewport, and test a long unbroken description. |
| A nested scroll/layout change could disturb Settings Search reveal or Jump control registration. | Retain the existing response IDs/observers and exercise the existing focused navigation regression. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- Plugin Settings has usable horizontal and vertical scrolling within viewport bounds.
- The identified enable/save/filter/navigation behavior is preserved with focused evidence.
- The change remains a single small implementation checkpoint after the mandatory branch baseline.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- Describe the exact sizing/scroll change and link the narrow-window evidence.
- Report the selected regression outcome and any environment limitation; identify the checkpoint commit and actual push result.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
