# Launcher Geometry and Query History Execution Ledger

This document is the durable execution ledger for the launcher geometry and
query-history initiative. Keep it current as each milestone moves through
`pending`, `in_progress`, `complete`, or `blocked`. A milestone is `complete`
only after its acceptance criteria, required verification, and commit have all
succeeded.

## Initiative baseline

- Branch: `visibility-and-history`
- Upstream: `origin/visibility-and-history`
- Starting HEAD: `8349fe17826fc35f5eab98347c05ece5f9d301f0`
- Initial working tree: clean (`git status --short --branch` reported only the
  branch/upstream line)
- Source of truth: the checkout at the starting HEAD, the feature request, and
  `AGENTS.md`
- Planned implementation commits:
  1. `fix(gui): preserve launcher geometry during restore`
  2. `feat(gui): add launcher query history navigation`

## Status summary

| Milestone | Status | Dependency | Commit |
| --- | --- | --- | --- |
| 0. Persist this execution ledger | `complete` | None | `bef14309` |
| 1. Preserve launcher geometry during restore | `complete` | Milestone 0 complete | `8c8bc31d` |
| 2. Add launcher query-history navigation | `complete` | Milestone 1 complete | `72ebbb77` |
| Integration verification | `complete` | Milestones 1 and 2 complete | Not applicable |
| Independent review and remediation | `in_progress` | Integration verification passes | Pending if remediation is required |
| Final verification | `pending` | Review has no unresolved substantive findings | Not applicable |

## Baseline architecture and inventory

### Visibility and restoration

- `src/visibility.rs`
  - `apply_visibility` currently combines configured placement, visibility,
    unminimizing, focus, and repaint behavior.
  - The hidden branch moves the viewport to the configured offscreen position.
  - `handle_visibility_trigger` owns immediate hotkey and queued visibility
    application.
- `src/gui/mod.rs`
  - `LauncherApp::new` applies the startup visibility and placement behavior.
  - `LauncherApp::handle_key` owns ordinary result navigation.
- `src/gui/render.rs`
  - The `restore_flag` path calls `apply_visibility(true, ...)` and then
    `force_restore_and_foreground`; this is the geometry regression boundary.
  - `last_visible` distinguishes genuine visibility transitions in the render
    loop.
- `src/gui/actions.rs`
  - `LauncherInteractionSnapshot` and
    `restore_for_new_launcher_interaction` centrally identify newly opened
    launcher-owned UI and request restoration.
- `src/main.rs`
  - Global hotkey visibility changes route through
    `handle_visibility_trigger`.

The baseline has exactly seven `apply_visibility` call sites, all of which must
receive an explicit policy in Milestone 1:

1. `src/visibility.rs` immediate hotkey visibility application.
2. `src/visibility.rs` previously queued visibility application.
3. `src/gui/mod.rs` launcher startup initialization.
4. `src/gui/render.rs` `restore_flag` restoration.
5. `src/gui/render.rs` `last_visible` visibility transition.
6. `tests/follow_mouse.rs` direct visibility behavior test.
7. `tests/domain_cases/offscreen.rs` direct hidden/offscreen behavior test.

Relevant existing visibility and lifecycle coverage includes:

- `tests/follow_mouse.rs`
- `tests/gui_visibility.rs`
- `tests/trigger_visibility.rs`
- `tests/focus_visibility.rs`
- `tests/domain_cases/offscreen.rs::offscreen_position_when_hidden`
- `src/gui/actions.rs::typed_dialog_preserves_query_launcher_interactivity_restore_and_history_exemption`
- `src/gui/actions.rs::typed_simple_dialogs_preserve_interactive_lifecycle`

### Query input and history

- `src/history.rs`
  - `HistoryEntry` retains the original `query` and a lowercased search helper.
  - The bounded in-memory `VecDeque` stores newest entries at the front.
  - `with_history` provides read-only access without requiring keyboard routing
    to read `history.json`.
- `src/gui/render.rs`
  - Owns the central launcher `TextEdit`, `query_response.changed()` handling,
    focus eligibility, autocomplete routing, note-search debounce interaction,
    and arrow/page/numeric keyboard routing.
- `src/gui/mod.rs`
  - Owns `self.query`, selection/autocomplete state, cursor/focus flags, and
    `handle_key` result navigation.
- `src/gui/actions.rs`
  - `LauncherApp::activate_action` is the central action boundary and history
    traversal reset seam.
- `src/gui/search.rs`
  - Owns search/autocomplete updates and note-search debounce helpers.
- `src/help_window.rs`
  - Owns the launcher Quick Help `Hotkeys` section.

Relevant existing query/history coverage includes:

- `tests/history.rs`
- `tests/query_autocomplete.rs`
- `tests/domain_cases/selection.rs`
- `src/gui/mod.rs::handle_key_grid_navigation_arrows_and_numpad`
- `src/gui/mod.rs::handle_key_list_mode_remains_compatible`
- `src/gui/mod.rs::arrow_page_tab_navigation_requires_query_focus`
- `src/gui/mod.rs::launcher_query_is_not_refocused_while_file_search_remains_open`
- `src/gui/search.rs::note_search_debounce_gate_only_fires_after_delay`
- Existing render/action tests for launcher query activation, search refresh,
  focus restoration, and programmatic macro queries.

## Initiative invariants

### Geometry and visibility

- Static Position and Static Size are placement policies for startup and a
  genuine hidden-to-visible show, not permanent anchors during a visible
  launcher session.
- Follow Mouse is recalculated only for a real show, not launcher-owned
  restoration while already visible.
- An internal restore may make the viewport visible, unminimize it, focus it,
  repaint it, and restore native foreground ownership, but it must not emit
  `OuterPosition` or `InnerSize` commands.
- Startup, hotkey/queued shows, and `last_visible` genuine transitions continue
  to apply configured placement.
- Hidden/offscreen behavior remains unchanged.
- `restore_flag`, `LauncherInteractionSnapshot`,
  `restore_for_new_launcher_interaction`, and
  `force_restore_and_foreground` remain the central lifecycle/focus mechanisms.
- No launcher panel, dialog, command, or plugin receives a geometry-specific
  exception.
- No new polling, worker, channel, filesystem access, window enumeration,
  plugin iteration, periodic repaint, or per-frame allocation is introduced.

### Query history

- Only exact Ctrl-only `Up` and `Down`, while the main launcher query owns
  keyboard focus, perform history traversal.
- Bare arrows and all existing page, horizontal, numeric, Enter, Tab, and Escape
  behavior remain unchanged.
- Ctrl+Up/Down must not also move result selection in the same frame.
- Traversal snapshots the existing in-memory history only on the first Ctrl+Up
  of a session; there is no per-frame clone or lock acquisition.
- The snapshot is newest-first, filters only trim-empty entries, and deduplicates
  by exact query-string equality while keeping the newest occurrence.
- Recalled text, including case, arguments, and meaningful whitespace, is not
  normalized.
- Traversal does not wrap. Ctrl+Down while inactive is a no-op. Moving newer
  past the newest entry restores the exact draft, including an empty draft, and
  ends the session.
- Manual edits, action execution, autocomplete, pending query replacement,
  macro query injection, and other observable programmatic query divergence
  invalidate stale traversal state.
- Applying recalled text reuses the existing query-change/search path, clears
  stale selection/autocomplete state, retains query focus, moves the cursor to
  the end, and preserves note-search debounce behavior.
- No persistence schema/file, background task, watcher, cache, or new top-level
  integration-test binary is added.

### Compatibility and scope

- Existing command parsing, typed command-bus semantics, launcher visibility,
  focus, hide, queued show, startup, panel/dialog behavior, fuzzy/plugin search,
  dashboard behavior, and persistence schemas remain compatible except for the
  explicitly requested geometry and history behavior.
- Meaningful tests are migrated around the new contract, not deleted, disabled,
  or weakened.
- No unrelated cleanup or architectural expansion is included.

## Milestone 0 — Persist the execution ledger

**Status:** `complete`

**Objective:** Preserve the reviewed implementation plan and baseline evidence
in the repository so sequential implementation, verification, commits, review,
and remediation can proceed without reconstructing context.

**Owned file:** `docs/launcher-geometry-history-plan.md`

**Acceptance criteria:**

- [x] Starting branch, HEAD, upstream, and initially clean state are recorded.
- [x] Baseline visibility, interaction, query, history, and test ownership are
  recorded.
- [x] All seven baseline `apply_visibility` call sites are inventoried.
- [x] Initiative invariants and non-goals are explicit.
- [x] Milestones 1 and 2 have ordered objectives, dependencies, affected areas,
  acceptance criteria, and verification commands.
- [x] Integration, review/remediation, final verification, commit, and actual
  result records are present.
- [x] The ledger-only diff passes whitespace validation and has been inspected.
- [x] The orchestrator commits this milestone separately; its hash is recorded
  in the final closeout update.

**Actual verification:**

- `git status --short --branch`: passed; branch/upstream are unchanged and this
  ledger is the sole untracked path (`?? docs/launcher-geometry-history-plan.md`).
- `git diff --no-index --check -- NUL docs/launcher-geometry-history-plan.md`:
  no whitespace errors (exit 1 is the expected no-index result for a new file;
  Git also reported the repository's normal LF-to-CRLF checkout warning).
- Full-file and generated no-index diff inspection: passed; the ledger contains
  only the accepted plan and baseline evidence.
- Tests: not run; Milestone 0 changes documentation only and broad tests are
  intentionally outside its scope.
- Commit: `docs(gui): plan launcher geometry and history navigation` (hash to be
  recorded in the final closeout update)

## Milestone 1 — Preserve launcher geometry during restore

**Status:** `complete`

**Planned commit:** `fix(gui): preserve launcher geometry during restore`

**Dependency:** Milestone 0 must be complete and committed.

**Objective:** Separate choosing configured geometry for a real launcher show
from restoring/focusing an existing visible viewport, and apply that distinction
at every visibility boundary without changing lifecycle ownership.

**Architectural intent:**

- Add a typed policy at the shared visibility boundary:

  ```rust
  pub enum VisiblePlacementPolicy {
      ApplyConfiguredPlacement,
      PreserveCurrentGeometry,
  }
  ```

- Pass the policy explicitly to `apply_visibility`; do not use an opaque
  boolean or a default argument.
- Keep common visible commands (`Visible(true)`, `Minimized(false)`, `Focus`,
  repaint) and hidden/offscreen behavior centralized.
- Assign `ApplyConfiguredPlacement` to startup, immediate hotkey show, queued
  show, and a genuine `last_visible` hidden-to-visible transition.
- Assign `PreserveCurrentGeometry` to the render-time `restore_flag` path.

**Likely affected areas:**

- `src/visibility.rs`
- `src/gui/mod.rs`
- `src/gui/render.rs`
- `tests/follow_mouse.rs`
- `tests/domain_cases/offscreen.rs`
- Existing visibility/focus/trigger and typed dialog lifecycle tests

**Required behavior and acceptance criteria:**

- [x] `VisiblePlacementPolicy` makes placement versus restoration explicit.
- [x] All seven baseline `apply_visibility` call sites pass an intentional
  policy; a post-change `rg` audit finds no ambiguous caller.
- [x] `PreserveCurrentGeometry` emits no `OuterPosition` or `InnerSize` even
  when Static Position, Static Size, and/or Follow Mouse are configured.
- [x] Geometry-preserving restore still emits `Visible(true)`,
  `Minimized(false)`, `Focus`, and repaint behavior.
- [x] Startup and genuine hidden-to-visible show continue to apply Static
  Position and Static Size.
- [x] Real-show Follow Mouse placement remains correct.
- [x] Cursor lookup failure remains safe: no move and no panic.
- [x] Immediate hotkey and previously queued shows use configured placement.
- [x] Render `last_visible` true transitions use configured placement.
- [x] The `restore_flag` path preserves geometry and retains
  `force_restore_and_foreground`.
- [x] `restore_flag`, `LauncherInteractionSnapshot`, and
  `restore_for_new_launcher_interaction` retain their lifecycle roles.
- [x] Offscreen hide behavior remains unchanged.
- [x] Typed-dialog lifecycle tests continue to prove restoration is requested;
  they are not changed merely to expect `restore_flag == false`.
- [x] No plugin-specific exceptions, polling, background work, persistence, or
  duplicate visibility implementation is added.
- [x] Existing legitimate visibility, focus, trigger, and interaction behavior
  remains covered and passing.

**Focused test additions/migrations:**

- Extend existing visibility tests to prove preserve mode ignores configured
  static position and size while retaining visible/unminimize/focus commands.
- Prove preserve mode ignores Follow Mouse.
- Retain configured static and Follow Mouse behavior in apply mode.
- Retain cursor-failure and hidden/offscreen coverage.
- Retain both typed-dialog interaction lifecycle tests.

**Required verification:**

- `cargo nextest run --test follow_mouse`
- `cargo nextest run --test gui_visibility`
- `cargo nextest run --test trigger_visibility`
- `cargo nextest run --test focus_visibility`
- `cargo nextest run offscreen_position_when_hidden`
- `cargo nextest run typed_dialog_preserves_query_launcher_interactivity_restore_and_history_exemption`
- `cargo nextest run typed_simple_dialogs_preserve_interactive_lifecycle`
- `cargo fmt --all --check`
- `cargo check`
- `git diff --check`
- `rg -n "apply_visibility\\(" src tests` and inspect all callers/policies
- Audit the diff for plugin-specific geometry handling and added background,
  persistence, or duplicated visibility paths.

**Actual verification:**

- `cargo nextest run --test follow_mouse --test gui_visibility --test
  trigger_visibility --test focus_visibility`: passed; 9 tests run, 9 passed,
  0 skipped.
- `cargo nextest run offscreen_position_when_hidden`: passed; 1 test run,
  1 passed, 3,282 filtered/skipped.
- `cargo nextest run
  typed_dialog_preserves_query_launcher_interactivity_restore_and_history_exemption`:
  passed; 1 test run, 1 passed, 3,282 filtered/skipped.
- `cargo nextest run typed_simple_dialogs_preserve_interactive_lifecycle`:
  passed; 1 test run, 1 passed, 3,282 filtered/skipped.
- `cargo fmt --all --check`: passed.
- `cargo check`: passed.
- `git diff --check`: passed with only the repository's normal LF-to-CRLF
  checkout warnings.
- `rg -n "apply_visibility\\(" src tests`: inspected; all production and test
  callers pass an explicit `VisiblePlacementPolicy`. Startup, immediate and
  queued hotkey handling, and render-loop visibility transitions apply
  configured placement; `restore_flag` preserves current geometry.
- Architecture/diff audit: passed for Milestone 1 scope; no plugin-specific
  handling, polling, background work, persistence, or duplicate visibility
  path was added. `restore_flag`, interaction snapshots, native foreground
  restoration, focus/unminimize/repaint, and offscreen hiding remain intact.
- Commit subject: `fix(gui): preserve launcher geometry during restore` (hash
  to be recorded in the final closeout update)

## Milestone 2 — Add launcher query-history navigation

**Status:** `complete`

**Planned commit:** `feat(gui): add launcher query history navigation`

**Dependency:** Milestone 1 must be complete and committed.

**Objective:** Add shell-style Ctrl+Up/Ctrl+Down navigation through executed
launcher queries using the existing in-memory history and a small transient,
independently testable state machine, without changing ordinary navigation.

**Architectural intent:**

- Add `src/gui/query_history.rs` with a pure transient
  `QueryHistoryNavigator` that owns the newest-first snapshot, cursor, exact
  draft, and expected current query.
- Keep persistence and global history ownership in `src/history.rs`; lazily
  obtain one snapshot with `history::with_history` on the first Ctrl+Up only.
- Integrate exact Ctrl-only shortcut matching beside the main query `TextEdit`
  in `src/gui/render.rs`, before/gating ordinary arrow navigation.
- Add one default-initialized, non-serialized navigator field to `LauncherApp`.
- Reset navigation at `LauncherApp::activate_action` and centrally synchronize
  expected versus actual query text so programmatic mutations cannot leave a
  stale traversal.
- Reuse existing search, autocomplete reset, selection reset, note debounce,
  cursor-end, and focus mechanisms rather than duplicating them.

**Likely affected areas:**

- New `src/gui/query_history.rs`
- `src/gui/mod.rs`
- `src/gui/render.rs`
- `src/gui/actions.rs`
- `src/help_window.rs`
- Existing unit tests in GUI modules plus `tests/history.rs` and existing query,
  autocomplete, focus, selection, and debounce coverage

**Required behavior and acceptance criteria:**

- [x] First Ctrl+Up recalls the newest usable executed query.
- [x] Repeated Ctrl+Up moves newest-to-oldest through unique entries and stops
  at the oldest without wrapping.
- [x] Ctrl+Down moves newer; crossing past the newest recalled entry restores
  the exact original draft and ends traversal.
- [x] Empty drafts restore correctly; Ctrl+Down while inactive is a no-op.
- [x] Empty and whitespace-only history entries are skipped.
- [x] Exact duplicate query strings are collapsed, preserving only the newest
  occurrence and preserving the original text/case.
- [x] An empty/unusable snapshot does not mutate the query or leave invalid
  active state.
- [x] Manual editing abandons traversal; the edited query becomes the next
  traversal's draft.
- [x] Action execution resets traversal and forces the next traversal to take a
  fresh snapshot that can include newly recorded history.
- [x] Autocomplete, pending query replacement, macro query injection, and other
  observable programmatic divergence invalidate traversal.
- [x] Shortcuts activate only with the main query focused and exact Ctrl-only
  Up/Down; they do not fire in child dialogs or unrelated text fields.
- [x] Handled history shortcuts do not also move normal result selection.
- [x] Bare Up/Down and existing PageUp/PageDown, left/right, numeric, Enter,
  Tab, and Escape behavior remain unchanged.
- [x] Applying recalled text clears result selection and autocomplete state,
  preserves/focuses the main input, places the cursor at the end, and refreshes
  results through the existing search path.
- [x] Note-search recall preserves the existing debounce policy.
- [x] Quick Help documents Ctrl+Up and Ctrl+Down query-history navigation.
- [x] No history file read, persistence/schema, background worker/task/watcher,
  additional cache, per-frame history clone/deduplication, or new top-level
  integration test binary is introduced.

**Focused test additions/migrations:**

- Pure navigator tests: first/repeated older, oldest boundary, newer movement,
  exact/empty draft restoration, inactive newer, empty history, blank filtering,
  exact duplicate filtering, case preservation, reset, and divergence followed
  by a fresh traversal.
- Shortcut/routing tests: focus eligibility, exact modifiers, event handling,
  result-selection isolation, recalled-query application, cursor/focus state,
  search refresh, and note debounce.
- Retain grid/list `handle_key`, main query focus routing, history persistence,
  autocomplete, and launcher-query behavior coverage.

**Required verification:**

- `cargo nextest run query_history`
- `cargo nextest run --test history`
- Target the existing `handle_key` grid/list navigation tests.
- Target launcher query focus/keyboard-routing tests.
- Target launcher-query application/search tests.
- Target accept-suggestion/autocomplete tests.
- Target note-search debounce tests.
- `cargo nextest run --test query_autocomplete`
- `cargo fmt --all --check`
- `cargo check`
- `git diff --check`
- Audit query-history state ownership and all shortcut handling.
- Audit for disk access, added persistence/background work, per-frame history
  cloning, and new top-level integration-test binaries.

**Actual verification:**

- `cargo nextest run query_history`: passed; 12 tests run, 12 passed,
  3,282 skipped. This includes seven pure navigator tests, four new GUI
  routing/application tests, and the existing typed Todo history test.
- `cargo nextest run --test history --test query_autocomplete`: passed; 6
  tests run, 6 passed, 0 skipped.
- `cargo nextest run handle_key`: passed; 2 tests run, 2 passed, 3,292
  skipped, covering existing list/grid arrow and numpad navigation.
- `cargo nextest run launcher_query`: passed; 9 tests run, 9 passed, 3,285
  skipped.
- `cargo nextest run query_focus`: passed; 3 tests run, 3 passed, 3,291
  skipped.
- `cargo nextest run tab_cycles_through_suggestions`: passed; 1 test run, 1
  passed, 3,293 skipped.
- `cargo nextest run note_search_debounce`: passed; 2 tests run, 2 passed,
  3,292 skipped.
- `cargo fmt --all --check`: passed.
- `cargo check`: passed.
- `git diff --check`: passed with only the repository's normal LF-to-CRLF
  checkout warnings. `git diff --no-index --check -- NUL
  src/gui/query_history.rs` likewise reported no whitespace errors; exit 1 is
  the expected no-index result for the new file.
- Routing/performance/architecture audits: passed for Milestone 2 scope. The
  navigator is transient and centralized; its newest-first snapshot is created
  lazily through the sole GUI `history::with_history` call on first Ctrl+Up.
  Inactive synchronization performs only an `Option` check. No disk access,
  persistence/schema change, cache, worker, watcher, polling path, or new
  integration-test binary was added. Ctrl-only history routing gates ordinary
  result arrows and remains tied to main-query focus.
- Commit subject: `feat(gui): add launcher query history navigation` (hash to
  be recorded in the final closeout update)

## Integration verification gate

**Status:** `complete`

Begins only after Milestones 1 and 2 are complete and committed.

**Acceptance criteria:**

- [x] Inspect the cumulative diff against the original request and this ledger.
- [x] Search every `apply_visibility` call and confirm its policy is explicit and
  correct.
- [x] Search for abandoned/duplicate visibility implementations and
  plugin-specific geometry workarounds.
- [x] Confirm launcher-owned restore cannot reapply static position, static size,
  or Follow Mouse placement.
- [x] Confirm real startup/show placement and hidden/offscreen behavior remain.
- [x] Confirm query-history state is transient and centralized.
- [x] Confirm no history disk access, persistence/schema change, worker, polling,
  watcher, cache, per-frame history cloning, or unnecessary integration-test
  binary was introduced.
- [x] Confirm all intended milestone changes are committed and no unrelated
  changes are included.

**Required verification:**

- `cargo fmt --all --check`
- `cargo check`
- `git diff --check`
- `cargo nextest run --no-fail-fast`
- Run the repository's established Clippy command only if branch/repository
  tooling requires it; do not invent a stricter unrelated policy.

**Actual verification:**

- Cumulative diff and architecture audit: passed; ten planned files differ from
  baseline, every visibility policy is explicit, restore cannot reach configured
  placement, history snapshotting is lazy/in-memory, and no new persistence,
  background path, or integration-test binary exists.
- `cargo fmt --all --check`: passed.
- `cargo check`: passed.
- `git diff --check`: passed with only the repository's normal LF-to-CRLF
  checkout warning for the ledger update.
- `cargo nextest run --no-fail-fast`: passed; 3,287 tests run, 3,287 passed,
  7 skipped.
- Clippy: not required by repository tooling for this initiative.
- Git status: milestone commits present; only the in-progress ledger update is
  uncommitted pending review and final closeout.

## Independent review and remediation gate

**Status:** `in_progress`

After integration verification passes, a high-reasoning reviewer who did not
perform the primary implementation must inspect the original request, this
ledger, cumulative branch diff, relevant surrounding code, and tests.

**Required review focus:**

- Geometry: remaining restore paths that can reapply placement/size/Follow
  Mouse; loss of focus/foreground or real-show behavior; duplicated viewport
  logic; plugin-specific exceptions.
- Query history: double-handled Ctrl arrows, incorrect focus/modifier routing,
  stale traversal/snapshot, wrong order/deduplication/draft boundaries, text
  mutation, note debounce bypass, unnecessary recomputation, disk/background
  work, and idle-frame overhead.
- General: correctness, regressions, incomplete migrations, weak ownership,
  missing or misleading tests, unnecessary complexity, and unrelated changes.

**Acceptance criteria:**

- [ ] Reviewer returns concrete findings ordered by severity.
- [ ] No unresolved substantive defect remains.
- [ ] Valid findings are remediated by an implementation agent, verified with
  affected targeted tests, and committed separately when materially distinct.
- [ ] Review is repeated when remediation warrants it.
- [ ] Full integration verification is rerun after behavior/shared-code
  remediation.

**Actual review/remediation:**

- Reviewer/task: independent review completed; remediation remains in progress.
- Findings: event routing combined frame-level modifiers with aggregate
  `key_pressed` state, which could misclassify batched key events; routing tests
  did not exercise event consumption; and the activation-reset test changed the
  current query after activation, allowing divergence synchronization to mask a
  missing central reset.
- Remediation: exact Ctrl-only history routing now inspects the modifiers on the
  corresponding pressed `egui::Event::Key` and removes that exact event in
  place. Focus and bare-arrow behavior remain unchanged, and handled history
  arrows cannot remain visible to ordinary result navigation. Regression tests
  exercise this production helper with differing frame/event modifiers and
  verify consumption. The activation test retains the same recalled query so a
  fresh post-activation snapshot depends on the central reset.
- Remediation commits: pending.
- Targeted reruns: `cargo nextest run query_history` passed; 13 tests run, 13
  passed, 3,282 skipped.
- Full-suite rerun: pending because remediation changes shared GUI routing.

## Final verification gate

**Status:** `pending`

**Acceptance criteria:**

- [ ] Milestones 0, 1, and 2 are `complete` with commit hashes recorded.
- [ ] All milestone and initiative acceptance criteria are satisfied.
- [ ] No required migration or substantive review finding remains.
- [ ] `cargo fmt --all --check` passes.
- [ ] `cargo check` passes.
- [ ] `git diff --check` passes.
- [ ] A complete final `cargo nextest run --no-fail-fast` passes; isolated reruns
  do not substitute for the final complete run.
- [ ] All intended changes are committed on `visibility-and-history`.
- [ ] `git status` confirms the expected clean working tree.

**Actual final verification:**

- Final HEAD: pending
- `cargo fmt --all --check`: pending
- `cargo check`: pending
- `git diff --check`: pending
- `cargo nextest run --no-fail-fast`: pending (record pass/skip/fail counts)
- Git status/cleanliness: pending
- Known remaining issues: pending

## Commit record

| Milestone/purpose | Hash | Subject | Verification summary |
| --- | --- | --- | --- |
| 0. Execution ledger | `bef14309` | `docs(gui): plan launcher geometry and history navigation` | Ledger inspection and whitespace checks passed |
| 1. Geometry-preserving restore | `8c8bc31d` | `fix(gui): preserve launcher geometry during restore` | Focused geometry, visibility, lifecycle, format, check, and diff checks passed |
| 2. Query-history navigation | `72ebbb77` | `feat(gui): add launcher query history navigation` | Navigator, routing, history, autocomplete, focus, debounce, format, check, and diff checks passed |
| Review remediation, if needed | Not applicable yet | Pending | Pending |

## Manual smoke-check record

These checks are optional when this environment can launch and reliably
interact with the Windows GUI. Never mark them passed unless actually run.

- Static Position visible-session preservation and hidden/show reapplication:
  not run.
- Static Size visible-session preservation and hidden/show reapplication: not
  run.
- Follow Mouse visible-session preservation and hidden/show recalculation: not
  run.
- Ctrl+Up/Ctrl+Down draft restoration, edit invalidation, and bare-arrow result
  navigation: not run.

## Non-goals

- Persisting manually dragged/resized launcher geometry.
- Replacing offscreen hiding or redesigning global hotkeys.
- Replacing typed command dispatch or creating a generalized history editor.
- Fuzzy history search, configurable history shortcuts, new persistence, or
  cross-process history synchronization.
- Plugin-specific geometry behavior or settings.
- Broad egui input, launcher performance, or unrelated architecture refactors.
