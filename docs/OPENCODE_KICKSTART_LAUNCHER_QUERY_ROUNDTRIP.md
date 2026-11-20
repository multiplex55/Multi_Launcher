# OpenCode Kickstart — Fix Launcher Query/Results NumPad Round Trip

**Suggested filename:** `OPENCODE_KICKSTART_LAUNCHER_QUERY_ROUNDTRIP.md`

**Agent:** OpenCode Build\
**Companion:** `OPENCODE_GOAL_LAUNCHER_QUERY_ROUNDTRIP.md`\
**Scope:** One focused regression fix

## Task

Execute the companion Goal completely.

The user has verified a remaining regression in the main launcher:

> After typing a command such as `mm`, NumPad navigation does not reliably enter the results until Tab is pressed. Once navigation begins, returning to the query and reentering results using only the NumPad is inconsistent.

**Required outcome:** Reliable, repeatable query ↔ result-list navigation using the NumPad alone, independent of the entered query.

## 1. Inspect current source and Git state

Read `AGENTS.md`, the companion Goal, and applicable repository instructions.

Use the current local checkout.

Inspect Git status, current branch, HEAD and predecessor history.

The prior inspected fix commit is:

`67587a6527d27bedf8fcded0f27f741a6a25f91f`

Verify whether newer local work exists before creating a fresh child branch.

Suggested branch:

`fix/launcher-query-numpad-roundtrip`

Do not branch from master, use worktrees, or discard unrelated edits.

## 2. Identify the exact cause

Inspect:

- `src/gui/query_navigation.rs`
- `src/gui/render.rs`
- `src/gui/mod.rs`
- Related root query/result tests

Reproduce the `mm` list-mode workflow and compare it with `note list` and grid mode.

Pay special attention to:

- Query focus versus result selection
- Event consumption before final dispatch
- `query_owned_focus` versus `query_has_focus`
- Result-button focus following Tab
- NumPad Up returning to the query
- Physical keypad Left/Right in list mode
- Result refresh and selection persistence

Confirm whether the current implementation handles only selection movement without providing the intended two-way navigation.

## 3. Make the smallest targeted correction

Implement deterministic NumPad navigation from query → results → query.

Do not simply force Tab or add synthetic Tab events.

Preserve legitimate keyboard ownership and normal query editing.

Avoid introducing new global keyboard handling or modifying vendored egui unless evidence requires it.

## 4. Verify narrowly

First, add a failing focused test that reproduces the actual missing round trip.

Then fix the behavior.

Run only the directly relevant tests, including a real root-render regression for `mm`.

No full Nextest, full Clippy, release build or unrelated UI verification.

If a native Windows smoke test is available without a broad automation effort, exercise the real `mm` and `note list` query paths.

Do not treat synthetic events as proof of unexecuted native behavior.

## 5. Commit and finish

Review the changed files for unrelated edits.

Commit the focused fix and tests, attempt one ordinary push per checkpoint, and report:

- Root cause
- Changes
- Targeted test results
- Remaining limitations
- Final commit SHA and branch

Use OpenCode's actual permission workflow when required.

**Continue through implementation and testing. Do not stop after diagnosis. Stop after this specific goal is finished.**
