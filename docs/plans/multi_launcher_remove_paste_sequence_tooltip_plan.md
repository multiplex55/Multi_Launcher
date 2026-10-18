# Multi Launcher — Remove Paste Sequence Floating Playback Tooltip

**Status:** Approved, narrow implementation task.  
**Source authority:** The current checked-out Multi Launcher repository; the plan was prepared from the user's October 18, 2026, 15:34:31 source snapshot. Verify only changed paths against the current checkout.  
**Scope:** Delete **only** the Paste Sequence floating playback tooltip feature and its configuration, code paths, tests, and current user documentation.  
**Not a repair:** The user confirms Paste-to-Advance, Manual Advance, and Automatic playback work. **Do not diagnose, modify, or reimplement them.**  
**User acceptance:** User will manually verify once Codex finishes. Do not use computer-control skills or live UI automation.

## 1. Exactly what must change

- Remove the Paste Sequence cursor-following/native floating progress tooltip entirely. No tooltip HWND/presenter, cursor sampling, overlay repaint/polling, failure toast, stale resource or dedicated progress snapshot remains associated with Paste Sequence.
- Remove **Settings → Paste Sequences → Show floating playback tooltip**, its persisted `tooltip_enabled` field, Save/Cancel draft wiring, runtime route-presentation value, dynamic help/hover summary and tooltip-specific tests.
- Remove current README/help statements promising a floating Paste Sequence tooltip.
- **Keep** the Paste Sequences dialog, including its run-state messages (`Ready`, `Running`, `Waiting for Ctrl+V`, `Waiting for Next`, `Completed`, `Cancelled`, `Failed`) and `step N of M` progress when applicable. That dialog is now the sole Paste Sequence progress display.
- **Keep unchanged** the Settings checkbox `Keep Paste Sequences dialog visible during playback`, its current default and persistence, Run/Run Once hide/show behavior, and all configured Next/Cancel/Escape controls.
- Preserve working repeat runs, existing physical Ctrl+V admission, step sequencing, clipboard overwrite/final-item retention, automatic/manual delays, preflight, window/target behavior, plugin disable, cancellation, exit cleanup and hotkey generation/route logic.
- Preserve every tooltip belonging to **Mouse Gestures, Radial, MkMacro, Notes, etc.** The native primitive `src/platform/passive_tooltip.rs` is *shared with Mouse Gestures* and **must not be deleted** or refactored. Its native tests must remain.

**Deliberate non-goals:** No new progress UI, animation, settings options, hotkey changes, window management changes, clipboard policy changes, input-hook changes, unrelated formatting or performance improvement.

## 2. Verified source map: focused edit locations

All entries below refer to the provided source snapshot; adjust only if the repository has advanced:

| Owner | Specific work |
|---|---|
| `src/paste_sequence/progress.rs` | Entire module is Paste Sequence's dedicated tooltip/presenter, cursor placement, per-run latching and tests. Delete the module after migrating its only necessary UI consumer. |
| `src/paste_sequence.rs` | Remove `pub(crate) mod progress;`. Remove `SequenceRoutePresentation.tooltip_enabled` and `Floating tooltip: …` from `summary()`, **keep** `physical_paste`, Next, Cancel, Starts, `escape_cancel_enabled`, `keep_dialog_visible_during_playback` and generation. |
| `src/paste_sequence/playback.rs` | Remove `SequencePlaybackController::progress()` and dedicated `sequence_progress_snapshot_is_started_only_and_never_contains_payload_or_title` test; **keep** `started_run()`, `status()`, `current_step_number()`, `needs_tick()`, `next_wake_after()` and all playback logic. |
| `src/gui/paste_sequence_dialog.rs` | `show_run_status` currently reads `app.sequence_playback.progress().map(...)`; replace with `app.sequence_playback.current_step_number()`. Preserve existing `run_status_text`, footer, the keep-visible preference and active-run/status tests. Replace incidental `.progress()` assertions with direct state/count assertions where they still matter. |
| `src/gui/mod.rs` | Remove only `LauncherApp.sequence_progress`, default construction and tooltip `clear()` calls. Do not alter `sequence_playback` ownership, plugin-disable cancellation, exit guards or the root GUI. |
| `src/gui/render.rs` | Remove `refresh_sequence_progress_tooltip()` and its call(s), associated tooltip-error message and tooltip-only 50 ms cursor-repaint scheduling. **Keep** the `sequence_playback.needs_tick()`/`tick()`/`next_wake_after()` loop, physical admission synchronization, `guard_overlay_app_exit` and app-close cancellation. |
| `src/gui/command_host.rs` | Remove tooltip-refresh calls in Start, Next, Cancel and physical input dispatch. **Keep** playback state transitions, `sync_physical_paste_admission()`, `schedule_sequence_playback_wake()` and its repaint scheduling, as well as all actual input/hotkey handlers. Fix only tooltip-only fields/assertions in existing tests. |
| `src/gui/watch.rs` | Keep the actual `PasteSequenceRoutes` generation-gated receipt and settings visibility preference; remove its call to refresh the tooltip and retire tooltip-specific presentation-test fields/assertions. |
| `src/settings/model.rs` | Remove `PasteSequencePreferences.tooltip_enabled` and default. Preserve serde defaults for missing current fields. Make loading preexisting settings JSON containing an unknown legacy `tooltip_enabled` key harmless; it must not be used/persisted as an active setting after the next explicit Save. |
| `src/settings_editor/render.rs` | Remove only the `Show floating playback tooltip` checkbox and its exact UI-test click/assertion. Preserve Escape and Keep Dialog Visible controls and staged Save/Cancel behavior. |
| `src/settings_editor.rs`, `src/settings_save.rs` | Remove tooltip-only setup/assertions from their tests. Keep relevant preference round-trip/rollback and dialog visibility assertions. |
| `src/main.rs` | Remove the tooltip field from `paste_sequence_route_presentation()` construction and corresponding fixture constructors/expectations. Preserve route validation, active/inactive help metadata, generation and full hotkey routing. |
| `README.md` / current in-app help | Remove the Paste Sequence floating-tooltip setting and the paragraph promising a passive playback overlay. Instead reference the **existing** dialog's Run status and `Keep Paste Sequences dialog visible during playback` setting, without adding new controls. |

**Shared-code boundary:** `src/platform/passive_tooltip.rs` and `src/mouse_gestures/overlay.rs` retain their existing implementation and tests. Do **not** remove `pub(crate) mod passive_tooltip;` from `src/platform/mod.rs`. Radial's `tooltip` subsystem and unrelated egui tooltips also remain untouched. Do not rewrite historical archived plans, changelogs or execution logs merely because they accurately document what existed previously.

## 3. Execution steps (two small checkpoints, no extra milestones)

### T1 — Remove the floating presenter and preserve dialog progress

1. Use `rg` to confirm direct consumers of `src/paste_sequence/progress.rs`, `SequenceProgressTooltip`, `PlaybackProgress`, `refresh_sequence_progress_tooltip`, `sequence_progress` and `SequencePlaybackController::progress()` in the current checkout. Confirm no non-Paste Sequence feature imports that module.
2. Switch the Paste Sequence dialog's `show_run_status` to `status()` plus `current_step_number()` **before** deleting the progress module; retain the exact message logic and progress layout.
3. Remove the progress module and public/internal module declaration, the playback `progress()` method and test that solely exercised the retired snapshot.
4. Remove `LauncherApp.sequence_progress`, its initialization, termination/plugin-disable cleanup and all `refresh_sequence_progress_tooltip()` callers (including physical-paste handler and route-watch receipt). Remove the presenter update method, its error toast, and its dedicated cursor wake timer.
5. **Do not delete adjacent scheduler calls** just because they were near the tooltip. Specifically preserve the playback tick in `src/gui/render.rs`, `schedule_sequence_playback_wake()` in command host, session admission/lease cancellation, and existing `guard_overlay_app_exit` behavior.
6. Repair existing tests that referenced tooltip-only data by deleting the *tooltip-specific assertion*, not the larger test. Where an assertion was proving actual playback state or privacy, continue to prove that requirement using the surviving controller/status/preview interface; do not weaken test coverage of physical Ctrl+V or Run→Complete→Run.

**Acceptance:** No Paste Sequence tooltip object/module/refresh method or timer remains; dialog status and counts compile and render using the controller's existing public internal methods; playback codepaths remain structurally unchanged.

**Suggested commit:** `refactor(paste): [T1] remove floating progress presenter` (only once code compiles and affected changes are coherent; combine with T2 into one commit if necessary to avoid a knowingly broken intermediate state).

### T2 — Remove setting, runtime metadata, references and tooltip-only tests

1. Remove persisted `tooltip_enabled` from `PasteSequencePreferences`, its default, staged Settings UI, acknowledgement/route presentation construction and help/hover summary. Keep all other Paste Sequence fields unmodified, especially `escape_cancel_enabled` and `keep_dialog_visible_during_playback`.
2. Remove just the tooltip-related fields, setup and assertions in `src/main.rs`, `src/gui/watch.rs`, `src/gui/command_host.rs`, `src/gui/paste_sequence_dialog.rs`, `src/settings/model.rs`, `src/settings_editor.rs`, `src/settings_editor/render.rs`, `src/settings_save.rs`. Preserve existing tests' non-tooltip behavior.
3. Confirm legacy `settings.json` with the previously persisted `paste_sequence.tooltip_enabled` still deserializes under the existing `#[serde(default)]` model with unknown fields ignored. A narrow existing compatibility-test fixture may retain the **legacy string literal solely as input data**; there must be **no live Rust field, UI switch or behavior** for it. Do not add a new migration system or automatically rewrite settings just because the app loaded them.
4. Update the small Paste Sequence section of `README.md` and any current user-facing help/tooltip summary to reference in-dialog status and the existing Keep Dialog Visible control. Historical plan/archive files should not be changed.
5. Run a **bounded dead-reference sweep** of maintained source/tests and current README: `sequence_progress`, `refresh_sequence_progress_tooltip`, `SequenceProgressTooltip`, `PlaybackProgress`, `tooltip_enabled` in Paste Sequence owners, `Show floating playback tooltip`, and `Floating tooltip:`. Any remaining hit must be classified as a different feature or an explicit legacy-settings compatibility fixture. Do not globally delete words like `tooltip` or `progress`.

**Acceptance:** The floating option cannot be toggled or displayed; no Paste Sequence runtime tooltip code or tests remain; previous Settings files load; dialog status, hotkey presentation, escape and keep-visible preference remain intact.

**Suggested commit:** `chore(paste): [T2] retire tooltip setting and references` (or one combined, clearly labeled commit if T1/T2 must land atomically).

## 4. Narrow verification budget — stop once this passes

Run **at most two focused Nextest invocations** against relevant existing tests, plus **one exact-failure rerun** if a concrete correction is required. Example filters from the source snapshot (adjust a changed test name, but do not broaden coverage):

```powershell
cargo nextest run --lib -E 'test(paste_preferences_legacy_and_partial_load_do_not_rewrite_settings) | test(paste_sequence_run_and_run_once_respect_visibility_preference_and_restart_all_modes) | test(paste_sequence_editor_status_has_live_counts_and_clear_terminal_failure_feedback) | test(paste_settings_navigation_controls_and_cancel_preserve_committed_state) | test(paste_settings_save_publishes_after_acknowledgement_and_reopens_committed_values)'

cargo nextest run --bin multi_launcher -E 'test(paste_route_presentation_tracks_omissions_reload_disable_and_clean_reenable) | test(main_hotkey_apply_publishes_runtime_before_applied_receipt)'
```

- These compile the affected library and main-binary test owners. Use one minimal `cargo check --bin multi_launcher` only if needed for a compilation gap; no extra unmotivated builds.
- Format **touched Rust files** consistently and run `git diff --check`; don't reformat vendor or unrelated modules. Review the final changed-file list.
- **Do not run a full Nextest suite, Clippy suite, benchmarks, desktop automation, screenshot capture, computer-control skills or a new UI/native test harness.** The user will manually verify the actual GUI after this pass.
- If a compile/test failure occurs, make one bounded correction and rerun just the failing subset. If it persists, record the precise issue rather than investigate indefinitely. No new tests dedicated to a feature being removed; only minimal compatibility adjustments in existing tests.
- Report which tests actually passed, which couldn't run, and where live Windows behavior was intentionally left to the user.

## 5. Existing agent orchestration and Git discipline

Read current `AGENTS.md`, `.codex/config.toml`, and actual `.codex/agents/{planner,implementer,reviewer}.toml`. Use **the configured named runners** rather than ad hoc agents: `planner` (read-only, GPT-6.1 Sol High) for one short owner/scope check, `implementer` (GPT-6 Luna Max, single source writer) for T1/T2, and `reviewer` (read-only, GPT-6.1 Sol High) for one final focused diff review. Parent/orchestrator manages tasks, commits, and bounded test results. A concrete reviewer finding may receive one narrow implementer correction; no repeated broad review loop.

Preserve the checked-out branch and unrelated dirty files. Never force-push, reset or rebase. Commit coherent changes and attempt **one normal push per commit** if configured and allowed; a rejected push never blocks the local work. No user input or approval needed while implementing.

## 6. Final handoff (then stop)

Provide: current branch/HEAD, commit hash(es) and push status, files removed/changed, confirmation the Settings checkbox is absent, confirmation the Paste Sequence dialog still gets status/counts from `current_step_number()`, exact focused checks/results, explicit statement that shared Mouse Gestures/Radial tooltips were not touched, and any honest unresolved issue. Return control for the user's one manual verification. **Do not propose a follow-on feature or expand this removal into an input/clipboard redesign.**
