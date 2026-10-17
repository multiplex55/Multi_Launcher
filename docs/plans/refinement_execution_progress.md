# Refinement execution ledger

Approved contract: `multi_launcher_refinement_codex_master_plan.md` and attached kickoff.
Execution branch: `big-change-cleanup-2`; initial HEAD `d0853d79`; initial worktree clean.
One writer; sequential checkpoint commits; best-effort normal push per commit.
Evidence/fixtures: ignored `target/refinement-evidence/` (owned fixtures only).

| Milestone | Status | Checkpoints / evidence |
| --- | --- | --- |
| M0 | complete | M0-A: source/manifest/status inspection; Nextest 0.9.135; rustc 1.99.0, x86_64-pc-windows-msvc; commit `513e89a4ca0292017b348735ce33cb896f25cd12` |
| M1 | complete | M1-A implemented in `src/gui/render.rs`: removed launcher scope strip, Ctrl+1–5 routing, scoped empty-state message and new-session scope reset; query/results layout now follows visible query geometry. `cargo nextest run --lib launcher_query_frame_has_no_scope_strip_or_ctrl_number_scope_changes`: 1 passed, 5,687 skipped. `rustfmt --edition 2024 src/gui/render.rs` and `git diff --check` passed. M1-A commit `178fb3b6`. M1-B source and scoped verification complete: unified Action-only provider/search flow, retired scope/category state and module removed; 11 launcher/provider tests and 1 radial ordering test passed (details below). M1-C README/Quick Help cleanup complete; dashboard compatibility target 9/9 passed. |
| M2 | in_progress | M2-A `e7950691` pushed: input coverage/startup/native event delivery complete; 9 focused tests passed (run `204a0662-cf09-4013-8184-e9f8a906ee90`). M2-B guarded lifecycle complete: 19/19 focused tests passed (run `4cff0574-77d8-4bbc-98f0-ccb5ac991d8d`); M2-C native evidence pending. |
| M3 | pending | Ordered-step layout |
| M4 | pending | Playback settings / atomic hotkey routes |
| M5 | pending | Cross-app playback / physical paste / tooltip |
| M6 | pending | Search and Replace layout |
| M7 | pending | Cargo binary target selection |
| M8 | pending | Integration, one full Nextest, ledger closure, Clippy last |

## Owner / direct test map

- Search: `src/gui/{render,search,mod}.rs`, Action-only plugin result boundary, Omni Search and radial actions; inline search/render tests. Retired `src/search_scope.rs` removed by M1-B.
- Capture: `src/gui/pinboard*.rs`, `src/gui/mkmacro_dialog/{visual_capture_workflow,visual_overlay,visual_overlay_windows}.rs`; inline overlay tests.
- Paste: `src/paste_sequence.rs`, `src/paste_sequence/playback{,/windows}.rs`, `src/gui/paste_sequence_dialog.rs`; inline persistence/playback/dialog tests. Existing schema v1 uses snake_case Automatic / ManualAdvance.
- Settings/routes: `src/settings/model.rs`, `src/settings_editor/`, `src/hotkey/{validation,reconfiguration,launcher_invocation}.rs`, `src/main.rs`, shared `src/mkmacro/input/`; staged settings and prepared reconfiguration own rollback.
- Replace: `src/gui/search_replace_dialog.rs`, domain/executor owners unchanged; inline dialog and disposable file tests.
- Cargo: root manifest currently autodiscovers binaries, no default feature list; explicit benches and grouped tests. Patched vendor dependencies are not assumed workspace members.

## Verification policy

No baseline build, full Nextest or Clippy. M0 used read-only inspection only.
M0–M7: directly affected focused tests and minimum compile checks.
M8-B: one full `cargo nextest run --workspace`, at most one repaired rerun.
M8-D only: maintained-target Clippy with `-D warnings`; final successful lint is the last automated verification.

## Commit index

- M0-A: `513e89a4ca0292017b348735ce33cb896f25cd12`.
- M1-A: `178fb3b6` (pushed).
- M1-B: `b27937cc` (pushed).
- M1-C: `7e9e0a32` (pushed).
- M2-A: `e7950691` (pushed).

## Limitations / unresolved issues

Native GUI evidence has not yet been attempted. No implementation blockers identified.
Initial M0-A push was rejected by automatic approval review as an untrusted remote. Parent later revalidated the intended configured remote/upstream and supplied the explicit AGENTS.md push authorization; normal push succeeded through M1-C `7e9e0a32`. M0/M1 commits are pushed.

## M1-B scoped evidence

Removed launcher `SearchScope`, effective-scope/filter/cache identity, result categories and category-only plugin APIs. Plugins return `Action`; scoring uses `(Action, f32)`. Omni keeps source order, first-source deduplication and independent include-provider settings. Provider pending/revision/catalog-version checks, literal query policy, enabled prefix matching and `fs` Global/Directory ownership remain intact. Main/settings reload callers use the existing Action-only reload API. Scope-only tests were removed; eager search/watch oracles and radial fixtures were migrated. M1-A's frame test now proves query/results/Ctrl+1-5/layout behavior without retired state.

- Launcher/provider command: `cargo nextest run --lib -E 'test(unified_launcher_and_read_only_snapshot_keep_order) | test(launcher_query_frame_has_no_scope_strip_or_ctrl_number_scope_changes) | test(read_only_search_uses_launcher_boundary_without_mutating_root_state) | test(deferred_snapshot_matches_ordinary_g_prefix_provider_selection) | test(track_c2a_oracle_covers_provider_precedence_and_snapshot_outcomes) | test(track_c3a_search_contract_matches_independent_fst_and_linear_references) | test(omni_results_keep_catalog_order_and_first_source_dedup) | test(provider_actions_match_on_synchronous_and_snapshot_paths) | test(query_prefix_respects_enabled_plugins_and_search_capability) | test(literal_qr_payload_survives_exact_sync_and_snapshot_search) | test(cs_prefix_uses_isolated_snippets_provider)'`: **11 passed, 5,672 skipped**; run `fb45a624-3249-4968-a564-edd3c8852e25`.
- Adjacent radial command: `cargo nextest run --lib deferred_execute_first_uses_the_current_shared_search_order`: **1 passed, 5,682 skipped**; run `2bca522b-5aec-4dec-9985-638fc1fbe0da`.
- Initial focused compile failed on duplicate ticketed helper and stale snippets/watch fixture API calls (E0592, E0599, E0061); corrected before the successful rerun above. No failing test assertions. Final compilation emitted existing vendor warnings only (`epaint`, `egui`, `image`); no vendor source changed.
- Touched Rust files formatted with `rustfmt --edition 2024 --config skip_children=true`; touched/cumulative diff inspected and `git diff --check` passed. Source/tests/benches search found no retired launcher scope/category/API references; unrelated radial-editor local `search_scope` and file-search domain scopes remain. No full suite, Clippy, extra check/build or native desktop test was run for this checkpoint.

## M1-C compatibility closure

README and Quick Help now describe unified search and existing plugin prefixes. The targeted stale-reference search across source, tests, benches and README found no retired scope/category API or shortcut guidance. File Search Global/Directory documentation remains intact. M1-B's passing provider/snapshot and radial tests supply ordering/prefix evidence; `cargo nextest run --test dashboard_config` passed 9/9 (run `9bf5a3d5-62c8-4882-be24-84bad29160ab`; compile 5m25s, tests 0.779s), including `should_show_dashboard_when_empty_query` and `activation_applies_query_override_first`. No owned native screenshot has been captured for M1.

## M2-A input coverage and startup

Existing rectangle shields intentionally use one-alpha layered hit-testable windows. They previously discarded native mouse messages while polling reconstructed edges, losing rapid Down/Up; controller and native show also sampled separate startup baselines. M2-A gives all monitor shields one ordered native input stream, makes rectangle visual surfaces transparent above shields, and acknowledges every actual shield rectangle, visibility, layered/nontransparent/noactivate/topmost style and complete count before establishing the startup baseline and arming selection. Signed physical monitor bounds, including monitors outside constrained selection regions, remain unchanged. Creation/acknowledgement failure closes partial resources without publishing an active picker. No new hook or BlockInput.

Focused command: `cargo nextest run --lib -E 'test(rectangle_shield_) | test(rectangle_startup_baseline_is_taken_after_coverage_acknowledgement) | test(only_rectangle_picker_uses_per_monitor_input_shields) | test(shield_plan_preserves_negative_origins_mixed_sizes_and_offsets) | test(held_launch_click_is_consumed_before_picker_arms) | test(rectangle_picker_full_lifecycle_records_visuals_frames_tooltips_and_cleanup) | test(picker_visual_is_transparent_above_exclusive_shields)'`: **9 passed, 5,678 skipped**; run `204a0662-cf09-4013-8184-e9f8a906ee90`, compilation 4m17s, test run 0.082s. The three native reducer/acknowledgement tests cover rapid queued Down/Move/Up, held startup release, and invisible/mismatched/transparent/not-topmost rejection; the controller test covers baseline changes during show. Existing lifecycle/geometry/style tests passed. Only existing vendor warnings were emitted. Source files formatted with `rustfmt --edition 2024 --config skip_children=true`; touched diff inspected and `git diff --check` passed. No full suite, Clippy, extra check/build, native desktop fixture or screenshot was run for A.

M2-B must complete release-safe cancellation/replacement/shutdown, capture-loss/monitor-change handling, and the startup-held external-capture release/new shield Down before first poll edge. Native SetCapture assists continuation; per-monitor hit-testable coverage remains required regardless of nonactivating/background capture semantics. Real underlying-window click-through remains unverified until M2-C; deterministic acknowledgements are not native click-through evidence.

## M2-B guarded terminal lifecycle

Rectangle terminal events and replacements now wait for the native owner's held-transaction cleanup acknowledgement. Native close witnesses queued Down, preserves shields while held, pumps through the physical release boundary (including unacknowledged partial-show input), releases capture and destroys resources before publishing completion/cancellation/error. Successful capture uses `close_after_release`, avoiding polling a later unrelated Down. Fresh shield Down after a foreign launch release arms a new transaction rather than disappearing behind the held startup baseline. Targeted cancellation recognizes queued replacement IDs; delayed passive previews receive their full lifetime from actual show time.

Capture-loss and display-change signals invalidate the active picker. Capture recovery gets one bounded attempt; display coverage repair installs/validates missing physical monitors before retaining old shields through release. Failed repair reports an error rather than asserting coverage over an uncovered monitor. A foreign launch release and capture-loss physical-up fallback are explicitly distinguished from consuming an owned native Up. No competing hook, BlockInput, forced focus, or new cancellation sleep was added. Cooperative shutdown uses the existing owner cadence while pending release.

`SharedVisualOverlayController::poll_app_exit` retains the native service and JoinHandle until release-aware shutdown completes. `LauncherApp::guard_overlay_app_exit` runs in root `logic` and `logic_only`, defers eframe Close with CancelClose while that owner remains live, and completes Close when cleanup finishes; the existing quit hotkey routes through the same Close request. No detached cleanup worker is treated as process-exit evidence. Readiness failures also clear pointer/desktop state.

Focused command: `cargo nextest run --lib -E 'test(rectangle_guard_) | test(rectangle_shield_) | test(rectangle_startup_baseline_is_taken_after_coverage_acknowledgement) | test(held_launch_click_is_consumed_before_picker_arms) | test(rectangle_picker_full_lifecycle_records_visuals_frames_tooltips_and_cleanup) | test(escape_before_and_during_drag_closes_once_and_emits_one_cancellation) | test(repeated_escape_cancel_shutdown_poll_and_drop_do_not_duplicate_cleanup_or_late_events) | test(service_worker_cancels_on_escape_and_never_times_out_interactive_picker)'`: **19 passed, 5,678 skipped**; run `4cff0574-77d8-4bbc-98f0-ccb5ac991d8d`, compilation 3m12s, test run 0.136s. New guard tests cover held Escape, delayed replacement and queued targeted cancel, passive expiry rebasing, failure/readiness cleanup, and cooperative owner shutdown. Native reducer tests directly cover signal reduction, interrupted release waiting, partial-install physical release boundary, and the fresh Down after foreign launch release. Existing lifecycle/startup/rapid-message tests passed. Only existing vendor warnings were emitted. Three owned compile sessions were deliberately canceled for concrete source corrections before any tests ran: `20977` (queued replacement targeted-cancel race), `72078` (reviewer passive-expiry finding), `25134` (partial-install physical release boundary). These exit-1 interruptions are not failing test assertions and are not passing results. Authoritative final focused session `42903` completed successfully with the result above.

Independent review found one P2: a queued passive preview's deadline began at request time. Corrected across all passive states with a fake-clock regression proving the full duration after delayed show; no additional independent findings. All four touched Rust files formatted using `rustfmt --edition 2024 --config skip_children=true`; final changed-file inventory/diff inspected and `git diff --check` passed. No extra compile check, adjacent test slice, full suite, Clippy, or native UI fixture was run for B. Actual native physical input/screenshot proof remains M2-C; these deterministic tests do not establish real click-through suppression on an interactive desktop.
