# OpenCode Goal — Launcher Query ↔ Results NumPad Navigation

**Suggested filename:** `OPENCODE_GOAL_LAUNCHER_QUERY_ROUNDTRIP.md`

**Repository:** `multiplex55/Multi_Launcher`  
**Reference commit:** `67587a6527d27bedf8fcded0f27f741a6a25f91f`  
**Reference branch:** `fix/launcher-query-result-navigation`  
**Priority:** High  
**Scope:** Main launcher query input and its search-result list/grid only

## 1. Objective

Fix the remaining inconsistent keyboard navigation between the main launcher query field and the search results displayed directly underneath it.

The previous fix improved physical NumPad recognition and result selection, but users still report needing to press Tab before they can navigate results.

The core requirement is:

**The user must be able to navigate freely between the main query input and its results using only the NumPad, without Tab, regardless of the search query or active list/grid layout.**

This is a narrowly scoped regression fix. Do not expand it into a general keyboard-navigation overhaul.

## 2. Reproduction

### Primary: `mm`

1. Open Multi Launcher.
2. Type `mm` into the main query.
3. Leave keyboard focus in the query input.
4. Press NumPad 2 (Down).
5. Expected: The first `mm` result immediately becomes selected.
6. Continue pressing NumPad 2/8 to move through the results.
7. From the first selected result, press NumPad 8 to return to the query-input state.
8. Press NumPad 2 again to reenter the results.

No Tab should be required at any point.

### Additional scenarios

Repeat with:

- `note list`
- `fileops`
- `downloads`
- A query with one result
- A query with many results
- A query with zero results
- A generic non-plugin query
- Grid layout enabled and disabled

**The query must not determine whether keyboard navigation works.**

Use actual `mm` plugin results in at least one focused regression test. Do not rely exclusively on a synthetic `note list` provider.

## 3. Required behavior

### A. Query → results

- NumPad Down selects the first result if nothing is selected.
- Subsequent Down presses move through the results.
- NumPad Up moves toward the first result.
- Every accepted key press should produce exactly one intended navigation step.
- Rapid presses and repeats should not silently disappear.
- Navigation must work immediately after typing a query.
- The user must not need to Tab into the result list.
- The selected result must be visually highlighted and scrolled into view.

### B. Results → query

- When the first result is selected, pressing NumPad Up again returns to the query-input state.
- Returning to the query clears the result selection and makes the query immediately editable.
- Pressing NumPad Down again selects the first result.
- This round trip must be repeatable indefinitely.
- If a result button has actual egui keyboard focus because the user previously pressed Tab, NumPad navigation must still provide a path back to the main query.
- The query's existing text, caret and selection must remain intact during navigation.

Prefer retaining the query TextEdit as the primary text-entry owner while using the existing `selected` result state for navigation. Do not force focus through every virtualized result button unless current code evidence shows that this is necessary.

### C. Left/Right behavior

For grid layout:

- NumPad Left/Right move between result columns.
- Existing row/column boundaries remain respected.
- Entering and leaving the result selection must not require Tab.

For list layout:

- NumPad Right may enter the results from the query.
- NumPad Left returns from a selected result to the query.
- These must not interfere with ordinary query typing.
- Standard Left/Right arrow keys retain their existing query-caret editing behavior when applicable.

Distinguish physical NumPad arrows from the main keyboard's arrow and number keys where the native event provides that information.

### D. Page Up / Page Down

Preserve the previous fix's Page Up/Down support.

- Work from the focused query without Tab.
- Navigate by the configured `page_jump` amount.
- Clamp to valid results.
- Scroll the selected result into view.
- Do not introduce repeated scrolling, bottom-edge flickering or unnecessary repainting.

### E. Keyboard ownership and event integrity

Investigate the existing routing in:

- `src/gui/query_navigation.rs`
- `src/gui/render.rs`
- `src/gui/mod.rs`
- The native physical-key mapping introduced by the previous fix

Specifically investigate:

1. Why `query_owned_focus` sometimes prevents navigation.
2. Whether input events are removed before later focus/ownership checks reject dispatch.
3. Whether list-mode and grid-mode handling produce different entry behavior.
4. Whether `selected` and egui widget focus can become inconsistent.
5. Whether result refreshes reset selection unexpectedly.
6. Whether keyboard ownership changes after Tab or UI transitions.

An event must not be consumed and silently discarded because the dispatch conditions differ from the conditions used to capture it.

Use one consistent, authoritative navigation-ownership decision for each accepted input event, or another equally narrow correction grounded in the actual cause.

Do not broadly bypass legitimate modal, popup, utility or Jump Mode keyboard ownership.

## 4. Preserve existing behavior

- Normal search text editing, caret movement and selection
- Top-row digit input, including 2/4/6/8
- Ctrl+Up/Down query history
- Autocomplete and Tab behavior
- Existing Enter result activation
- Jump Mode, including Shift+Space
- List/grid layout configuration
- Root query usability beside popped-out dialogs
- Existing result filtering, ranking, scrolling and virtualization
- Existing focus behavior of unrelated utility dialogs

No new global hotkeys, keyboard hooks, framework upgrades or unrelated refactoring.

Avoid additional changes to vendored egui/eframe unless the actual failure conclusively requires one.

## 5. Focused tests only

Add a small group of regression tests using the real root query rendering/event path.

Required checks:

1. `mm` → NumPad Down immediately selects result zero without Tab.
2. Repeated Down/Up navigation advances and returns to the query.
3. After returning to the query, Down reenters the list.
4. Num Lock on/off and physical NumPad distinction.
5. List-mode NumPad Left/Right behavior.
6. Grid-mode directional navigation.
7. Page Up/Down selection and scrolling.
8. Result selection and focus remain coherent across search changes.
9. Ctrl+Up/Down, autocomplete, ordinary editing and top-row digits remain functional.
10. Modal and popup ownership prevents inappropriate root navigation.

Include at least one test proving **both directions** of navigation through real widget focus and selected-result state.

A test that only calls `handle_key()` is insufficient.

Where practical, use one real Windows desktop smoke test for `mm` and `note list`. If native interaction is unavailable, report it honestly rather than marking it verified.

**Do not run:**

- Full Nextest suite
- Full Clippy campaign
- Release builds
- Broad GUI automation
- Unrelated integration tests
- Large refactoring or performance projects

Use only focused test filters and required compilation.

## 6. Acceptance criteria

The goal is complete when:

- Typing `mm`, then immediately pressing NumPad Down selects the first result.
- NumPad movement works regardless of query or list/grid mode.
- NumPad Up from the first result returns to the query.
- Repeated transitions require no Tab.
- The query remains editable and its text is preserved.
- All accepted keys are dispatched exactly once.
- No events silently disappear during a focus transition.
- Result scrolling remains stable.
- Existing keyboard ownership and editing behavior remain intact.
- Relevant focused tests pass.

## 7. Git and delivery

This is a fresh follow-up task to the previous navigation fix.

Use the existing checkout and verify the latest actual local/remote predecessor before branching. The last inspected fix tip is `67587a6527d27bedf8fcded0f27f741a6a25f91f`.

Suggested new branch:

`fix/launcher-query-numpad-roundtrip`

Create it directly from the verified preceding fix tip, not from `master`.

If it already exists and clearly represents this exact task, resume safely.

- No worktrees or auxiliary clones.
- No unrelated merges, rebases, resets or force pushes.
- Preserve all existing user changes.
- Keep implementation and tests to one or two small commits.
- Attempt a normal push after each commit.
- Do not merge to `master`.

## 8. Final report

Provide a concise handoff containing:

- Confirmed root cause
- Why the previous fix was insufficient
- Exact new behavior
- Files modified
- Focused test commands and outcomes
- Any unverified native behavior
- Branch name and final SHA
- Push status

**Stop after this fix. Do not start additional features.**
