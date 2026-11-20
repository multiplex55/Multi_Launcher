# OpenCode Kickstart: Fix Main Launcher Query Keyboard Navigation

**Agent:** OpenCode **Build** (`build`)  
**Companion goal:** `OPENCODE_GOAL_LAUNCHER_QUERY_NAVIGATION.md`  
**Execution:** One small, targeted regression fix; continue through final verification and handoff

Execute the companion Goal. **Do not stop after writing a plan or investigating the issue**; implement and test the fix if the repository and tools permit it.

## 1. Load the authoritative task and inspect the checkout

1. Confirm that the active OpenCode workspace is the intended local `multiplex55/Multi_Launcher` checkout. Do not clone it elsewhere or create a worktree.
2. Locate and read `OPENCODE_GOAL_LAUNCHER_QUERY_NAVIGATION.md`, this Kickstart, the root `AGENTS.md`, and any relevant nested repository instructions. If the companion file is not at the repository root, locate it by exact filename using repository search.
3. Read relevant existing progress/preceding-goal records where present. Inspect `git status`, current branch and HEAD, recent commit history, and the exact final commit of the preceding completed Goal.
4. Verify the predecessor instead of blindly branching from the historical reference `ec91fb29ba55f026d31ae0337e531f8b22ba18a3`.
5. Preserve existing user changes. If already on the correct branch for this goal, safely resume it. Otherwise create one fresh **linear child** branch from the verified predecessor, suggested name `fix/launcher-query-result-navigation`.

If you cannot establish a safe branch point or would overwrite user changes, **stop before modifying Git state** and report the blocker. Do not modify `master`, use worktrees, reset, force-push, or perform unrelated merges.

## 2. Reproduce and identify the defect

Use `rg` and available code-reading tools to trace the current launcher query keyboard pipeline, particularly:

- Main query `TextEdit` event handling.
- Physical NumPad detection and key routing.
- `query_navigation_allowed` and `query_workflow_allowed`.
- `query_has_focus` and `launcher_query_keyboard_enabled`.
- Page Up/Down event delivery.
- Result selection, scroll-to-selection behavior, and focused query/result tests.

Use `note list` as the primary reproduction scenario. Determine why NumPad Down or standard arrows sometimes require Tab to reach results, and why Page Up/Down may fail directly from the query.

Distinguish **confirmed causes** from hypotheses. Check native/egui routing, event consumption, keyboard ownership, and focus behavior before deciding on a fix. Do not invent observations that the current environment cannot reproduce.

## 3. Implement the smallest correct fix

Restore reliable direct navigation from the focused main query while preserving:

- Search text editing, caret movement, selection, and ordinary number-key input.
- Ctrl+Up/Down query history and autocomplete.
- List/grid navigation semantics, including physical NumPad and standard arrow behavior.
- Existing modal, utility, and Jump Mode keyboard ownership.
- Page Up/Down configured jump distance and clamping.
- Result scrolling without bottom-edge flicker.

Keep the changes focused on the routing or selection defect. Avoid a keyboard-system rewrite, egui upgrade, global keyboard hooks, or unrelated cleanup.

## 4. Run narrowly scoped verification

1. Discover actual existing test names and filters before executing tests.
2. Add/update focused regressions, including **at least one test through the actual root query `TextEdit` event path**, not just a direct `handle_key()` call.
3. Verify the cases required by the Goal: NumPad and standard arrows, Num Lock event handling where testable, Page Up/Down, list/grid behavior, input ownership, and query editing/history preservation.
4. Run only focused tests and the minimum required compilation. Fix only failures attributable to this change.
5. Record failures that are unrelated or environment-dependent rather than expanding the campaign.

**Prohibited:** full Nextest suite, full Clippy, release builds, broad native UI automation, and unrelated regression tests.

Do not assert native Windows UI acceptance from tests that never exercised the corresponding interaction. If necessary, describe any remaining manual checks in the handoff.

## 5. Commit, push, and finish

Once the relevant tests pass, or verification is blocked in a clearly documented way:

1. Review `git diff` and `git status` for unintended changes. Do not stage unrelated pre-existing edits.
2. Make one focused fix commit, with a minimal additional test/remediation commit only if necessary. Do not commit unverified changes as a successful fix without disclosing their verification status.
3. Attempt a normal `git push` after each commit, honoring OpenCode permission prompts. If a push is rejected or blocked, keep the local commit and report the failure; do not force-push.
4. Provide a concise handoff: confirmed root cause, change summary, modified files, focused test commands/results, unverified cases, branch, final SHA, and push status.

Do not merge into `master`.

## OpenCode session behavior

- This is an **implementation** task. Use **Build** rather than staying in Plan mode.
- A short diagnosis/sequence is fine; do not generate a second large plan in place of the requested fix.
- Continue through analysis, code changes, focused tests, and commit/push without asking for approval at routine milestones. Honor any actual approval prompt required by OpenCode.
- If context is compacted or the session resumes, re-read the Goal, Git state, and current code. Do not assume previous Codex chat context is available.
- Do not require Codex configuration files, an MCP server, or a particular coding model to complete ordinary local repository operations.

**Stop after this goal. Do not start another feature, broad cleanup, or follow-on investigation.**
