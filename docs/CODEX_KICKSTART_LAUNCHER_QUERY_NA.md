# CODEX_KICKSTART_LAUNCHER_QUERY_NAVIGATION.md

## Kickstart: Fix Main Launcher Query Keyboard Navigation

Execute the companion **CODEX_GOAL_LAUNCHER_QUERY_NAVIGATION.md** as one small, targeted regression fix.

### 1. Start from the correct source

Read `AGENTS.md` and the companion Goal.

Inspect the current checkout, confirm its Git state, and identify the exact final commit of the preceding Goal.

Create a fresh linear child branch, suggested name:

`fix/launcher-query-result-navigation`

Preserve existing user changes. Do not use worktrees or modify `master`.

### 2. Reproduce and identify the defect

Investigate the existing launcher query keyboard pipeline, particularly:

- Main query `TextEdit` event handling.
- Physical NumPad detection and key routing.
- `query_navigation_allowed` and `query_workflow_allowed`.
- `query_has_focus` and `launcher_query_keyboard_enabled`.
- Page Up/Down event delivery.
- Result selection and scrolling.

Use `note list` as the primary reproduction scenario.

Determine why the user may need Tab before arrows work, and why Page Up/Down sometimes fail to move the results.

Distinguish confirmed causes from hypotheses.

### 3. Implement the smallest correct fix

Restore reliable direct navigation from the focused query while preserving:

- Search text editing and caret movement.
- Ordinary number-key input.
- Query history and autocomplete.
- List/grid navigation semantics.
- Existing modal, utility and Jump Mode ownership.
- Result scrolling without flicker.

Avoid expanding this task into a keyboard-system rewrite.

### 4. Run narrowly scoped verification

Add focused tests that exercise actual root query input routing and the affected selection behavior.

Run only those tests and directly related existing tests.

Do not run the full Nextest suite, full Clippy, a release build or an unrelated acceptance campaign.

Fix only failures caused by this change. If an unrelated or environmental issue prevents verification, record it and continue with the safe, independently verifiable work.

### 5. Commit and finish

Once the focused tests pass:

1. Review the small diff for unintended changes.
2. Commit the targeted fix.
3. Attempt a normal push.
4. Report the root cause, changes, test results and final SHA.

Do not merge to `master`.

**Stop after this goal. Do not start another feature, broad cleanup or follow-on investigation.**
