# OpenCode Goal: Restore Reliable Launcher Query-to-Results Keyboard Navigation

**Repository:** `multiplex55/Multi_Launcher`  
**Execution environment:** OpenCode **Build** agent, operating on the existing local checkout  
**Companion execution document:** `OPENCODE_KICKSTART_LAUNCHER_QUERY_NAVIGATION.md`  
**Scope:** Main launcher query input and its search-result list/grid only  
**Priority:** High — frequent-workflow regression  
**Execution:** One focused linear branch, minimal commits, targeted testing only

## Objective

Restore the ability to navigate the main launcher's search results immediately after typing a query, without pressing Tab or manually transferring focus.

Primary scenario:

1. Open Multi Launcher.
2. Type `note list`.
3. Press NumPad Down (2).
4. The first search result should immediately become selected.
5. Continue pressing NumPad Up/Down to navigate the results.
6. Press Enter to execute the selected result.

The query input should retain its normal editing behavior. Selecting results must not require changing focus to a separate list widget.

## Required Behavior

### A. Arrow and NumPad navigation

- NumPad 2 / Down: Select the first result, then move downward.
- NumPad 8 / Up: Move upward.
- NumPad 4 / Left: Move left when grid layout is enabled.
- NumPad 6 / Right: Move right when grid layout is enabled.
- Standard keyboard arrow keys must retain equivalent result-navigation behavior.
- Support Num Lock on and off where Windows provides the appropriate physical or logical key events.
- Rapid presses and key repeats should not be silently lost.
- Do not incorrectly interpret top-row number keys as NumPad navigation.
- Left/Right must preserve ordinary cursor editing and existing list-mode behavior when grid navigation is not applicable.

### B. Restore Page Up / Page Down

- Page Down selects the first result when nothing is selected.
- Subsequent Page Down presses advance by the configured `page_jump` value.
- Page Up moves backward by the same amount.
- Navigation clamps to valid result indexes.
- The selected result automatically scrolls into view.
- These shortcuts must work directly from the focused launcher query without requiring Tab.
- Avoid repeated scroll requests, animation loops, and the previously fixed bottom-edge flickering.

### C. Focus and keyboard ownership

- Search-result navigation must work while the main query input owns focus.
- Do not require a separate result-list focus state.
- Preserve query caret, text selection, typing, autocomplete, and ordinary editing behavior.
- Preserve Ctrl+Up/Down query-history shortcuts.
- Preserve Jump Mode and its focus-only hint behavior.
- Do not intercept input belonging to an active modal, utility editor, popup, or other legitimate keyboard owner.
- Opening, closing, docking, or popping out a utility must not leave the main query in a stale navigation state.
- Enter should activate the actual selected result according to existing rules.

## Investigation Scope

Inspect the relevant ownership and event-routing paths in the **currently checked-out** source:

- `src/gui/render.rs`
- `src/gui/numpad_navigation.rs`
- `src/gui/mod.rs`
- Existing focused query/result-selection tests

Use repository search (for example, `rg`) to find the current definitions, call sites, and tests. Follow input from the native Windows or egui key event through the focused query `TextEdit`, keyboard-owner checks, and `LauncherApp::handle_key()`.

Investigate whether input events are consumed before navigation, physical NumPad state is sampled too late, or query-focus conditions incorrectly suppress result navigation. These are **hypotheses**, not established causes; verify them against code and evidence.

**Correct the actual cause.** Do not apply a broad focus workaround that simply redirects all keyboard input into the result list.

## Constraints

- Minimal production code changes.
- No unrelated dialogs, search-ranking changes, or plugin behavior changes.
- No new global keyboard hooks.
- No egui upgrade, GUI rewrite, or large architectural refactoring.
- Preserve normal query input and all previously supported shortcuts.
- Keep the existing selection and grid/list navigation model wherever possible.
- No unrelated formatting or cleanup.
- Work locally using the existing checkout; do not create auxiliary clones or worktrees.
- Do not depend on Codex session memory or Codex-specific orchestration features. Derive the current status from Git, on-disk repository instructions, and the two goal documents.

## Targeted Verification Only

Add or update a small set of focused regression tests covering:

1. Focused query → NumPad Down → first result selected without Tab.
2. Repeated Up/Down navigation, including rapid key events.
3. NumPad behavior with Num Lock on/off and ordinary number-key preservation.
4. Page Up/Down with first selection, configured jump distance, and boundary clamping.
5. List versus grid directional behavior.
6. Query focus after an ordinary utility opens/closes or is popped out.
7. Non-query keyboard owners retaining their input.
8. Ctrl+Up/Down history and ordinary query text editing remaining functional.

At least one regression test must exercise the **actual root query `TextEdit` and keyboard-routing path**, rather than only invoking `handle_key()` directly.

Discover the real test names/filters before running tests. Run only the relevant focused tests and narrowly required compilation commands. If the Windows-specific behavior cannot be exercised in the current environment, report which cases remain unverified; never claim that a simulated test proves unexecuted native interaction.

**Do not run:**

- Full Cargo Nextest suite
- Full integration-test campaign
- Full Clippy campaign
- Release builds
- Broad native UI automation campaigns
- Unrelated regression tests

A narrowly targeted compile/test command is sufficient.

## Acceptance Criteria

The fix is accepted when:

- `note list` results are navigable immediately from the query with NumPad Down.
- Arrow and NumPad navigation is consistent with the active result layout.
- Page Up and Page Down work directly from the query.
- Selected rows scroll into view correctly without flickering.
- Query typing, caret behavior, autocomplete, and query history are preserved.
- Existing modal and utility keyboard ownership remains correct.
- The focused regression tests pass.

Clearly distinguish **automated verification**, **manually observed behavior**, and **not yet verified** in the final report.

## Git and Delivery

Create one fresh branch from the **exact final tip of the preceding completed Goal**, preserving the project's linear ancestry. The last verified Productivity Toolkit tip was `ec91fb29ba55f026d31ae0337e531f8b22ba18a3`; this is a historical reference, **not permission to assume it is today's correct predecessor**. Verify the relevant current predecessor records and Git history before branching.

Suggested branch: `fix/launcher-query-result-navigation`.

- First inspect `git status`, the current branch and HEAD, and the recent history. Preserve all existing user changes.
- If this goal's branch already exists and is clearly the correct continuation, resume it rather than creating another branch. Otherwise, create **one** fresh linear child branch from the verified predecessor.
- If the predecessor cannot be verified safely, do not invent a parent or change branches; report the specific blocker.
- No worktrees, auxiliary clones, unrelated merges, resets, or force pushes.
- Make one focused fix commit, or a minimal additional test/remediation commit if necessary.
- Attempt a normal push after each commit. Respect OpenCode's actual tool-permission prompts; if push is denied or fails, retain the local commit and report why.
- Do not merge into `master`.

## OpenCode Execution Expectations

- Use the **Build** agent, not Plan-only mode, for the actual implementation.
- Read and honor the repository `AGENTS.md` and applicable nested instructions. Existing repository skill files may be used if relevant, but no skill is required to complete this goal.
- Treat this Goal and its companion Kickstart as the scoped task. Do not silently expand it or start a follow-on feature.
- Run local inspection, editing, tests, and Git commands through tools available to the current OpenCode environment, within configured permissions.
- If the session is compacted, resumed, or interrupted, reconstruct progress from the working tree, commits, test results, and these documents instead of relying on an assumed previous chat state.
- If a permission prompt cannot be resolved, an essential predecessor is ambiguous, or environmental tooling blocks testing, report the blocker rather than bypassing safeguards.

## Final Handoff

Report concisely:

- Root cause identified (and any remaining hypotheses).
- Exact behavior corrected.
- Files modified.
- Focused test commands and outcomes, including any native behaviors not verified.
- Remaining limitations or blockers, if any.
- Branch name and final commit SHA.
- Push result (remote updated, failed, or not attempted, with reason).

**Stop after this goal.** Do not merge or begin unrelated work.
