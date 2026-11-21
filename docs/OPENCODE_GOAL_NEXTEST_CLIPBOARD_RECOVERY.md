# OpenCode Goal: Nextest Failure Recovery and Clipboard Modify Regression

**Repository:** `multiplex55/Multi_Launcher`  
**Agent:** OpenCode Build  
**Model:** GPT  
**Priority:** High  
**Execution:** Autonomous, iterative, narrowly scoped  
**Branch:** Create a new branch from the current HEAD  
**Completion requirement:** All runnable tests pass, and Clipboard Modify works correctly.

---

## 1. Objective

Resolve the failures identified in the latest full Cargo Nextest run and repair a separate regression affecting the Clipboard Modify plugin.

Keep iterating through diagnosis, focused fixes, testing, and verification until the failures have been resolved.

**This is a bug-fixing and regression-recovery task, not a feature-development task.**

Make the smallest changes required to restore correct behavior. Avoid broad refactoring, architectural redesign, unrelated cleanup, dependency upgrades, or changes that could destabilize existing functionality.

### Starting failure summary

The latest full test execution reported:

```text
9253 tests run
8795 passed
458 failed
67 skipped

456 ABORT: Windows stack overflow (0xC00000FD)
2 FAIL: Ordinary test failures
```

Most failed tests terminate with:

```text
Recursion too deep; the stack overflowed.
(os error 1001)
```

The failures are heavily concentrated in GUI-related tests but also occur across other modules, including command dispatch, note operations, OCR, settings persistence, and integration tests.

**Do not assume that 456 stack overflows represent 456 independent bugs.**

Investigate whether they share a common cause, particularly test-thread stack size, large GUI fixtures, stack-allocated application state, recursive initialization, or a recent regression affecting shared test setup.

The two ordinary test failures must also be resolved.

Separately, users are experiencing a live Clipboard Modify regression:

```text
Permanent("The clipboard could not be accessed.")
```

This must be investigated and corrected independently of the test failures.

---

## 2. Start Conditions — Git Setup

Before making any code changes:

1. Read `AGENTS.md` and applicable project instructions.
2. Confirm the existing repository directory.
3. Identify the currently checked-out branch.
4. Record the exact current HEAD SHA.
5. Inspect `git status` for uncommitted or untracked changes.
6. Fetch the latest remote refs without changing the current branch's HEAD.
7. Create a **new branch directly from the original current HEAD**.

Suggested name:

`fix/nextest-stack-overflows-and-clipboard`

If the branch name already exists, choose a unique suffix rather than resetting or overwriting another branch.

### Git restrictions

- Use the existing checkout.
- Do not create worktrees or additional clones.
- Do not branch from `master` unless it is the currently checked-out branch.
- Do not reset, force-push, squash, or rebase unrelated work.
- Do not discard uncommitted changes.
- If existing local modifications cannot be safely isolated, stop and report the conflict rather than overwriting them.
- Commit small, logically grouped changes.
- Attempt a normal push after each successful checkpoint.
- Do not merge into `master`.

Record the starting branch and SHA in the final handoff.

---

## 3. Phase A — Diagnose the 456 Stack Overflow Aborts

**Priority: Highest**

### A1. Establish a reproducible baseline

Inspect the existing test configuration and representative failing tests.

Start with a small selection that covers different modules:

```text
gui::render::tests::keyboard_navigation_is_consistent_between_grid_and_list_modes

gui::render::tests::root_list_viewport_builds_bounded_rows_with_absolute_ids_and_selection

gui::actions::tests::specialized_actions_ignore_query_override_before_dispatch

gui::notes_dialog::tests::quick_notes_wrap_links_preserves_open_panel_and_view_mode

settings_save::tests::hidden_settings_editor_completes_and_a_second_save_uses_the_new_baseline

hide_after_run_not_for_calc_copy
```

Run tests individually to identify whether the same failure occurs without other tests running concurrently.

Record:

- Exact failure signature
- Whether the failure is deterministic
- Stack configuration
- Test profile
- Rust/Cargo/Nextest versions
- Relevant environment variables
- Whether the failure occurs during application construction, rendering, execution, or teardown

Do not immediately modify hundreds of tests.

### A2. Investigate test-stack requirements

Specifically evaluate whether the tests exceed the default Windows test-thread stack size.

As a diagnostic, compare normal execution with a larger Rust thread stack.

For PowerShell:

```powershell
$env:RUST_MIN_STACK = "16777216"
```

This provides a 16 MiB minimum stack for threads where Rust's stack-size setting applies.

Test representative failures under both configurations.

If needed, test a larger bounded stack size to determine whether the issue is insufficient stack capacity or genuine unbounded recursion.

**A larger stack making a test pass is diagnostic evidence, not automatically a complete fix.**

Determine whether the underlying cause is:

- Legitimately large application or GUI test fixtures
- Excessive stack allocation
- Accidental recursion
- Shared test initialization
- Unexpected test-runner or build-profile behavior
- Another independently verified cause

Do not blindly increase stack limits or add arbitrary recursion guards.

### A3. Implement the minimum correction

Prefer a localized solution that addresses the shared cause.

Examples of acceptable solutions, if supported by evidence:

- Correcting a shared test fixture that creates unnecessarily large stack allocations.
- Moving demonstrably oversized fixture storage to the heap.
- Correcting unintended recursive initialization.
- Adjusting an explicitly documented test-runner stack requirement when that is genuinely appropriate.
- Fixing a small shared helper causing the repeated failures.

Do not rewrite `LauncherApp` or restructure the GUI architecture merely to address test-stack failures.

Do not modify production application behavior to accommodate an incorrectly configured test environment.

### A4. Validate the shared fix

After applying the correction:

1. Rerun the initial representative failing tests.
2. Run a targeted selection across the major affected GUI modules.
3. Verify that unrelated tests still pass.
4. Compare normal stack settings against any required override.
5. Determine whether the fix resolves the broader stack-overflow cluster.

If a documented test environment setting is genuinely required, make the supported test command explicit. Prefer a solution that allows the ordinary repository test workflow to succeed without undocumented shell configuration.

Do not proceed to hundreds of individual test edits until the shared-cause investigation is complete.

---

## 4. Phase B — Resolve the Two Ordinary Test Failures

These tests failed normally rather than aborting with stack overflows.

### B1. Calculation Pad host migration

```text
gui::calculation_pad_dialog::tests::
studio_usability_calculation_host_migration_preserves_cached_session_and_pending_decision
```

Investigate:

- Cached Calculation Pad session preservation
- Docked versus detached host transitions
- Pending confirmation or decision state
- Any recent changes affecting host migration
- Whether the existing test expectation still describes the intended behavior

Correct the smallest verified defect.

Do not weaken session-preservation guarantees simply to make the assertion pass.

### B2. Openable panel tracking

```text
gui::tests::tracked_openable_panels_count_as_any_panel_open
```

Investigate:

- Panel-open tracking
- Newly introduced utility panels
- Detached versus docked state
- Whether the panel inventory has changed
- Whether test fixtures or tracking logic need updating

If the test is outdated because a valid new panel was added, update the assertion or fixture to reflect the intended contract.

If production tracking is incorrect, fix the implementation.

Do not merely remove an assertion or ignore a panel to produce a passing result.

### B3. Verify both tests

Run each test individually.

Confirm the corrected behavior using targeted related tests.

Only change test expectations when supported by a clearly established intended behavior.

---

## 5. Phase C — Fix Clipboard Modify Regression

**Priority: High — affects live application functionality**

### C1. Reproduce the issue

Users report that opening or using Clipboard Modify produces:

```text
Permanent("The clipboard could not be accessed.")
```

Expected behavior:

1. Copy ordinary text in Windows.
2. Open Multi Launcher.
3. Launch Clipboard Modify.
4. The copied text should load successfully.
5. Apply a simple transformation.
6. Preview and commit the transformed text.
7. Verify the clipboard contains the intended result.
8. Verify Undo restores the previous clipboard content.

The operation should not fail permanently when the Windows clipboard is accessible.

### C2. Inspect relevant implementation

Start with:

```text
src/platform/protected_clipboard.rs
src/clipboard_modify/clipboard.rs
src/gui/clipboard_modify_dialog.rs
src/clipboard_modify/coordinator.rs
```

Follow the actual call chain through Clipboard Modify and its clipboard backend.

The current source includes these important boundaries:

- `read_clipboard()` in the protected clipboard layer
- Windows Runtime clipboard access through `Clipboard::GetContent()`
- WinRT apartment initialization
- Error classification into Busy, Unavailable and other states
- Clipboard Modify's conversion of protected clipboard errors into `ClipboardError::Permanent`
- Bounded retry behavior in `ClipboardService`

The displayed error is consistent with `ClipboardError::Unavailable` being converted into a permanent Clipboard Modify error.

**Investigate the exact failing Windows API operation rather than assuming the clipboard is genuinely unavailable.**

### C3. Identify the root cause

Determine whether the failure is caused by:

- Incorrect WinRT initialization or threading assumptions
- Windows clipboard contention
- Incorrect error classification
- A transient condition treated as permanent
- Protected clipboard access interacting incorrectly with ordinary text
- A recent change to clipboard privacy handling
- Clipboard access occurring on an inappropriate thread
- Another verified cause

Capture useful error classifications or HRESULT codes where appropriate, without logging clipboard contents.

### C4. Correct the regression

Implement the smallest safe fix that restores ordinary text clipboard access.

Required behavior:

- Ordinary text loads reliably.
- Transient clipboard contention is handled with bounded retries where appropriate.
- Permanent failures remain correctly classified.
- Non-text clipboard data is not mistaken for ordinary text.
- Clipboard Modify never silently overwrites clipboard contents when a read fails.
- Undo and conflict-confirmation behavior remain correct.
- Protected/private clipboard items remain private.
- Password Generator privacy protections are preserved.
- Clipboard History does not accidentally persist protected clipboard contents.
- No infinite retries, busy loops, or UI-thread blocking.

If considering a fallback clipboard-reading mechanism, ensure it does not bypass private-item detection or leak protected data.

Avoid broad clipboard subsystem changes unless the existing shared implementation is conclusively responsible.

### C5. Add focused regression coverage

Add or adjust narrowly scoped tests for:

1. Reading ordinary Windows clipboard text.
2. Correct classification of clipboard access errors.
3. Bounded retries for transient failures.
4. Handling genuinely inaccessible clipboard state.
5. Preserving source data after failed reads.
6. Correct preview, commit and Undo behavior.
7. Protected clipboard and password privacy guarantees.

Use deterministic mock backends for failure classification and retry tests.

When an interactive Windows desktop is available, perform a real clipboard smoke test using disposable, non-sensitive text.

Do not report native functionality as verified if only mocks were exercised.

---

## 6. Phase D — Iterative Verification

After Phases A–C:

### D1. Run targeted tests first

Run the focused tests associated with the changes.

Avoid repeatedly running the entire 9,000-plus-test suite after every small edit.

Fix failures in the smallest possible related group.

### D2. Run the complete Nextest suite

Once the focused tests pass, execute:

```powershell
cargo nextest run --no-fail-fast
```

If compilation resource pressure prevents running it, use justified bounded build/test concurrency, and record the configuration.

Do not confuse compilation problems with test failures.

### D3. Analyze remaining failures

If any tests still fail:

1. Capture the exact failing test names and error messages.
2. Group failures by root cause.
3. Determine whether they relate to the current fixes.
4. Reproduce the relevant test individually.
5. Implement the smallest correct change.
6. Rerun the focused tests.
7. Repeat full-suite verification when the remaining group is resolved.

Continue iterating until the complete suite passes or a clearly documented blocker prevents safe completion.

**Do not stop simply because the original 456 stack overflows have disappeared if other tests remain failing.**

However, if solving a remaining failure requires a substantial architectural redesign, unrelated feature implementation, or destructive change, stop that part of the work and report the blocker instead of expanding the task without authorization.

### D4. Test integrity requirements

Do not achieve a passing result by:

- Deleting tests.
- Adding `#[ignore]` to failing tests.
- Broadening test filters to exclude failures.
- Suppressing panics or errors without correcting their cause.
- Weakening assertions without evidence.
- Adding arbitrary sleeps or excessively long timeouts.
- Increasing retries merely to conceal deterministic failures.
- Altering production behavior solely to satisfy incorrect tests.
- Silently changing the test environment between comparison runs.

The 67 originally skipped tests are not failures. Preserve existing legitimate skips and do not add new skips to hide problems.

---

## 7. Scope Boundaries

### Allowed

- Shared test-harness corrections.
- Minimal GUI fixture and initialization corrections.
- Fixes directly related to failing assertions.
- Clipboard Modify regression corrections.
- Focused test updates.
- Small related documentation or configuration changes required to make verification repeatable.

### Not allowed

- New application features.
- Major GUI refactoring.
- egui/eframe migrations or version upgrades.
- Broad dependency updates.
- Rewriting the clipboard architecture.
- Large performance optimization efforts.
- Unrelated formatting or Clippy cleanup.
- Unrelated changes to plugins or settings.
- Changes to normal application behavior without a demonstrated regression.

Keep all modifications directly traceable to the supplied failures or clipboard regression.

---

## 8. Checkpoints and Git Workflow

Use small, reviewable checkpoints.

### M0 — Baseline and classification

- Record starting Git state.
- Create the new branch.
- Reproduce representative failures.
- Identify shared versus independent causes.
- Record findings.

### M1 — Stack-overflow recovery

- Resolve the shared stack-overflow cause.
- Pass representative affected tests.
- Confirm broader related coverage.
- Commit and push.

### M2 — Ordinary assertion fixes

- Resolve both specifically named test failures.
- Run relevant focused tests.
- Commit and push.

### M3 — Clipboard Modify recovery

- Repair the live clipboard regression.
- Pass focused unit tests.
- Perform native Windows verification if available.
- Commit and push.

### M4 — Full-suite verification

- Run complete Nextest.
- Address remaining failures.
- Repeat targeted/full verification as appropriate.
- Commit and push any final minimal corrections.

Checkpoint order may be adjusted if dependencies require it, but preserve the same narrow scope.

Do not commit unrelated generated build artifacts or logs containing sensitive data.

If pushing fails, retain the local commits and report the failure. Do not force-push.

---

## 9. Progress Reporting

Maintain a concise progress record in:

`docs/plans/execution/standalone/nextest-clipboard-recovery/STATUS.md`

Record:

- Starting branch and SHA
- Current checkpoint
- Confirmed root causes
- Files changed
- Test commands executed
- Passing/failing counts
- Remaining work
- Blockers
- Commit SHAs
- Push status

Keep the document factual and brief.

Do not create a large planning framework or numerous additional documentation files.

---

## 10. Stop Conditions

### Successful completion

Declare the goal **COMPLETE** only when all of the following are true:

1. The original stack-overflow failures have been resolved.
2. Both ordinary test failures pass.
3. The full Nextest suite completes with zero `FAIL` and zero `ABORT` results.
4. No tests were newly ignored or excluded to obtain that result.
5. Clipboard Modify successfully reads ordinary clipboard text.
6. Clipboard Modify transformations, commit and Undo work.
7. Protected clipboard and history privacy guarantees remain intact.
8. Focused regression coverage passes.
9. All task changes have been committed.
10. The final branch has been pushed successfully.

If actual native Clipboard Modify verification is unavailable, explicitly mark it unverified rather than claiming full acceptance.

### Blocked completion

Declare the goal **BLOCKED**, not complete, if:

- A required failure cannot be reproduced because of an environmental limitation.
- Windows-native validation is essential but unavailable.
- A remaining failure requires major unrelated architectural work.
- The appropriate fix cannot be established without risking existing functionality.
- Required dependencies, permissions, or tooling are unavailable.

Report the blocker, the evidence collected, and the smallest recommended next action.

Do not fabricate passing results or silently omit unresolved failures.

### Prohibited stopping behavior

Do not stop merely because:

- The first representative test passes.
- The stack-overflow count decreases.
- The two ordinary failures are fixed.
- Clipboard Modify passes mock tests but still fails in the application.
- A checkpoint is completed while other in-scope failures remain.

Continue through the established checkpoints until the completion or blocked conditions are reached.

---

## 11. Final Handoff

When finished, report:

**Git**
- Original branch and HEAD
- New working branch
- Final commit SHA
- Push status

**Root causes**
- Shared cause of the stack overflows
- Cause of each ordinary assertion failure
- Cause of Clipboard Modify's permanent access error

**Changes**
- Files modified
- Brief explanation of each fix
- Why the changes are narrowly scoped

**Verification**
- Original Nextest results: 8,795 passed / 458 failed / 67 skipped
- Final complete Nextest results
- Test environment and stack settings used
- Clipboard Modify verification results
- Remaining failures or limitations

**Status**
- COMPLETE or BLOCKED
- Any specific outstanding issues

Do not merge into `master`.

Do not begin any unrelated work after completion.

**Execute autonomously through implementation and verification, not merely investigation or planning.**
