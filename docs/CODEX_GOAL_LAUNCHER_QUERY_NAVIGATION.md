# CODEX_GOAL_LAUNCHER_QUERY_NAVIGATION.md

## Goal: Restore Reliable Launcher Query-to-Results Keyboard Navigation

**Repository:** `multiplex55/Multi_Launcher`  
**Scope:** Main launcher query input and its search-result list/grid only  
**Priority:** High — frequent-workflow regression  
**Execution:** One focused branch, minimal commits, targeted testing only

### Objective

Restore the ability to navigate the main launcher's search results immediately after typing a query, without pressing Tab or manually transferring focus.

For example:

1. Open Multi Launcher.
2. Type `note list`.
3. Press NumPad Down (2).
4. The first search result should immediately become selected.
5. Continue pressing NumPad Up/Down to navigate the results.
6. Press Enter to execute the selected result.

The query input should retain its normal editing behavior. Selecting results must not require changing focus to a separate list widget.

### Required Behavior

**A. Arrow and NumPad navigation**

- NumPad 2 / Down: Select the first result, then move downward.
- NumPad 8 / Up: Move upward.
- NumPad 4 / Left: Move left when grid layout is enabled.
- NumPad 6 / Right: Move right when grid layout is enabled.
- Standard keyboard arrow keys must retain equivalent result-navigation behavior.
- Support Num Lock on and off where Windows provides the appropriate physical or logical key events.
- Rapid presses and key repeats should not be silently lost.
- Do not incorrectly interpret top-row number keys as NumPad navigation.
- Left/Right must preserve ordinary cursor editing and existing list-mode behavior when grid navigation is not applicable.

**B. Restore Page Up / Page Down**

- Page Down selects the first result when nothing is selected.
- Subsequent Page Down presses advance by the configured `page_jump` value.
- Page Up moves backward by the same amount.
- Navigation clamps to valid result indexes.
- The selected result automatically scrolls into view.
- These shortcuts must work directly from the focused launcher query without requiring Tab.
- Avoid repeated scroll requests, animation loops, and the previously fixed bottom-edge flickering.

**C. Focus and keyboard ownership**

- Search-result navigation must work while the main query input owns focus.
- Do not require a separate result-list focus state.
- Preserve query caret, text selection, typing, autocomplete and ordinary editing behavior.
- Preserve Ctrl+Up/Down query-history shortcuts.
- Preserve Jump Mode and its focus-only hint behavior.
- Do not intercept input belonging to an active modal, utility editor, popup or other legitimate keyboard owner.
- Opening, closing, docking or popping out a utility must not leave the main query in a stale navigation state.
- Enter should activate the actual selected result according to existing rules.

### Investigation Scope

Inspect the relevant ownership and event-routing paths:

- `src/gui/render.rs`
- `src/gui/numpad_navigation.rs`
- `src/gui/mod.rs`
- Existing focused query/result-selection tests

Trace input from the native Windows or egui key event through the focused query TextEdit, keyboard-owner checks and `LauncherApp::handle_key()`.

Investigate whether input events are consumed before navigation, physical NumPad state is sampled too late, or query-focus conditions incorrectly suppress result navigation.

**Correct the actual cause.** Do not apply a broad focus workaround that simply redirects all keyboard input into the result list.

### Constraints

- Minimal production code changes.
- No unrelated dialogs, search-ranking changes or plugin behavior changes.
- No new global keyboard hooks.
- No egui upgrade, GUI rewrite or large architectural refactoring.
- Preserve normal query input and all previously supported shortcuts.
- Keep the existing selection and grid/list navigation model wherever possible.
- No unrelated formatting or cleanup.

### Targeted Verification Only

Add or update a small set of focused regression tests covering:

1. Focused query → NumPad Down → first result selected without Tab.
2. Repeated Up/Down navigation, including rapid key events.
3. NumPad behavior with Num Lock on/off and ordinary number-key preservation.
4. Page Up/Down with first selection, configured jump distance and boundary clamping.
5. List versus grid directional behavior.
6. Query focus after an ordinary utility opens/closes or is popped out.
7. Non-query keyboard owners retaining their input.
8. Ctrl+Up/Down history and ordinary query text editing remaining functional.

At least one regression test must exercise the actual root query TextEdit and keyboard-routing path, rather than only calling `handle_key()` directly.

Run only the relevant focused tests. Discover the actual test filters before running them.

**Do not run:**
- Full Cargo Nextest suite
- Full integration-test campaign
- Full Clippy campaign
- Release builds
- Broad native UI automation campaigns
- Unrelated regression tests

A narrowly targeted compile/test command is sufficient.

### Acceptance Criteria

The fix is accepted when:

- `note list` results are navigable immediately from the query with NumPad Down.
- Arrow and NumPad navigation is consistent with the active result layout.
- Page Up and Page Down work directly from the query.
- Selected rows scroll into view correctly without flickering.
- Query typing, caret behavior, autocomplete and query history are preserved.
- Existing modal and utility keyboard ownership remains correct.
- The focused regression tests pass.

### Git and Delivery

Create one fresh branch from the exact final tip of the preceding completed Goal, preserving the project's linear ancestry. The last verified Productivity Toolkit tip was `ec91fb29ba55f026d31ae0337e531f8b22ba18a3`; verify current predecessor records before branching.

Use the existing checkout only. No worktrees, auxiliary clones, unrelated merges, resets or force pushes.

Make one focused fix commit, or a minimal additional test/remediation commit if necessary. Attempt a normal push after each commit.

Do not merge into `master`.

### Final Handoff

Report:

- Root cause identified.
- Exact behavior corrected.
- Files modified.
- Focused test commands and outcomes.
- Remaining known limitations, if any.
- Branch name and final commit SHA.

Keep the final report concise.
