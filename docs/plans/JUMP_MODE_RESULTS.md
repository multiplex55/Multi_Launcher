# Jump Mode Results

Implementation is in progress. M00–M11 are complete; M12 complex controls/MkMacro is next. M12–M15 remain pending. Phase 1 native evidence is accepted; native Settings/dialog/complex-control evidence remains pending.

- Branch: feat/jump-mode-2026-10-19-c7e42.
- Starting SHA: 8f94b003a12853735d1cb4689c6ec4e53c4ace8e.
- Latest completed checkpoint: 1ce05985c4bbd668f0d63b80bc319ef28e2e5f39 (M11-A).
- All checkpoint commits through M11-A were pushed normally to origin; no local-only task commits.
- Current ownership, exact historical commands, failures and next action: [execution ledger](JUMP_MODE_EXECUTION.md).
- This report consolidates accepted results. Earlier failed evidence remains unchanged in the ignored raw evidence directory.

## Behavior and architecture

The persisted experimental flag defaults off and appears under Settings → Launcher Window & Appearance. The general-settings save coordinator owns committed publication/rollback; editing the checkbox remains a draft until successful Save. Cancel preserves the committed runtime setting.

Focused Shift+Space starts/cancels navigation. Hints are deterministic, uppercase, prefix-free A–Z and case-insensitive. The map is frozen for a session. Escape cancels and Backspace edits the prefix. Hint completion focuses/selects only; later distinct native interaction activates. Search text/caret survive the leader and captured hint input. Same-frame activation is suppressed, held-key guards require release, and native focus loss cancels safely.

One app-owned bridge holds copied semantic/native IDs, geometry, clips, epochs and generations. It collects real enabled actionable Responses in the current viewport, rejects clipped/disabled/ambiguous/stale entries, reconciles at pass/frame completion and paints a clipped overlay without a continuous repaint loop. It retains no widget callbacks or Responses.

Root result IDs use current result generation and full result index, preserving equal-looking actions with different arguments. Selecting a list/grid card updates the existing logical selection and leaves native focus with the query; later Enter uses the existing full Action/args handler. Dashboard controls use slot/row semantic and native IDs so configuration reorder cannot redirect completed focus.

Root/Settings(instance)/Notes(instance)/Todo(instance)/Timer(instance)/PasteSequence(instance)/Blocked ownership follows actual retained ordinary-window order and focused response layer. Opening/closing/raising a window invalidates navigation and blocks the transition frame across discarded passes. Supported windows use the same input, registry and painter while root editing/autofocus/navigation/Enter yield. Dialog targets require the exact kind and instance; scoped child confirmations retain their parent identity. Existing container Escape/Note Find behavior remains with its owner. Unsupported windows/popups fail closed.

| Component | Main source ownership |
|---|---|
| Setting/default/save/editor | src/settings/model.rs; src/settings_save.rs; src/settings_editor/state.rs and render.rs |
| Pure typed hints/session | src/gui/jump_mode.rs |
| Registry, focus intent, keyboard lifetime | src/gui/jump_mode/ui.rs |
| Overlay | src/gui/jump_mode/paint.rs |
| Root host/query/list/grid | src/gui/render.rs; LauncherApp in src/gui/mod.rs |
| Dashboard controls/native slot identity | src/dashboard/dashboard.rs and widgets modules |
| Fixture/native runner | dev/jump_mode/fixture_generator/src/main.rs; dev/jump_mode/run.ps1 |
| Opt-in observation/framebuffer | src/gui/qa_framebuffer.rs; existing ROOT framebuffer path |

## Coverage

| Surface/control | Current status and evidence |
|---|---|
| Root query | Supported + verified headless/native: leader/hint/focus, text/caret, ordinary typing afterward |
| Root list/grid, note cards | Supported + verified headless/native: selection before action, full payload, duplicate-title exact slug, stale generation |
| Dashboard discrete controls | Supported + verified representative native FillQuery; full registered inventory below |
| Settings basic controls | Supported + verified real egui widgets/headless; native screenshots pending M13 |
| Notes/Todo dialogs | Supported basic controls; real egui/headless behavior verified; native evidence pending M13 |
| Timer/Paste Sequence dialogs | Basic controls supported+verified headless; native evidence follows M13 |
| Combo/slider/DragValue/radio/multiline adapters | Pending M12; currently unregistered |
| MkMacro representative controls | Pending M12; native proof pending M13 |
| Unsupported windows/popups | Background/root suppression verified; supported nested controls still require M11/M12 |
| Feature disabled | Supported + verified headless/native catalog18 |
| Performance/Clippy/final smoke | Baseline recorded; M14/M15 remain pending |

Dashboard registration covers RecentCommands rows; PinnedQueryResults/QueryList rows and Refresh; Todo summary/row Open; Calendar Open/Add; Temp Add/ClearAll/Refresh/row Open/Clear; Timer New/Pause/Cancel; Scratchpad Copy/AppendTimestamp/Refresh; NowPlaying controls; Recycle Clean/Refresh; SystemActions rows/Refresh; SystemStatus Refresh. Native automation activates only safe fixture note FillQuery actions. PinnedCommands/CommandHistory and complex Todo/Scratchpad/Calendar controls remain unregistered.

Settings registration covers all eight top-level section toggles, Expand/Collapse all, Save/Cancel, direct built-in checkboxes/single-line fields and simple buttons in Hotkeys, Paste Sequences, Launcher, Search, Actions/Feedback, Dashboard and Radial. The Plugins section header is supported; delegated plugin/Note editor controls and complex controls remain unregistered. Disabled pending-save footer controls are excluded. The section target is the actual collapse-toggle Response, with existing persistent section identity.

## Milestones

| Milestone | Status | Accepted verification/checkpoint |
|---|---|---|
| M00 | complete | Unique branch/base/initial clean state and ownership ledger |
| M01 | complete | Production fixture semantics; actual populated baseline and owned cleanup |
| M02 | complete | Default-off/editor/runtime gate plus general-save ownership regression |
| M03 | complete | 10 pure engine tests |
| M04 | complete | 24 registry/focus tests |
| M05 | complete | 33 input/painter tests; actual native query proof |
| M06 | complete | 38 Jump tests including real list/grid selection/Enter |
| M07 | complete | 43 Jump tests; actual Dashboard reload/reorder native-ID regression |
| M08 | complete | All15 Phase1 catalog cases through focused Dashboard/Grid/Disabled runs |
| M09 | complete | Bounded core/evidence review;8 painter tests and post-fix native Dashboard01–05 |
| M10 | complete | M10-A63 tests; M10-B67 tests and scoped review; native evidence reserved M13 |
| M11 | complete | Notes/Todo and Timer/Paste; repaired final83tests + scoped review clean |
| M12 | pending | Complex controls/MkMacro |
| M13 | pending | Extended native screenshots and control semantics |
| M14 | pending | Focused reliability, after measurements, Clippy and review |
| M15 | pending | Final representative native smoke, consolidated final status/handoff |

## Verification

Latest combined command on repaired M11-B source, exit0, **83 passed / 5827 skipped**:

~~~powershell
cargo nextest run --lib --no-fail-fast --cargo-profile fast-dev --features "dev-smoke-binaries qa-framebuffer-capture" -E 'test(jump_mode) | test(actual_root_navigation_owner_follows_settings_and_foreign_window_layers) | test(opening_settings_on_a_discarded_root_pass_cancels_background_navigation) | test(settings_owner_blocks_root_autofocus_and_enter_activation) | test(settings_escape_closes_when_jump_mode_owns_root_navigation) | test(note_find_shortcut_and_escape_leave_dirty_note_open_until_the_next_escape) | test(note_find_shortcut_uses_focused_or_top_note_and_preserves_ctrl_w_close) | test(settings_shell_tests) | test(todo_row_widget_ids_stay_bound_to_entry_when_filter_moves_another_row_to_zero) | test(enter_adds_todo_with_filter_focus) | test(enter_adds_todo_with_tags) | test(ordinary_enter_cannot_activate_root_results_behind_timer_or_paste_dialogs) | test(open_timer_resets_timer_fields_and_arms_duration_focus) | test(open_alarm_clears_pending_timer_focus_and_keeps_alarm_title) | test(paste_saved_row_native_identity_survives_removal_of_an_earlier_row)'
~~~

The real Settings test verifies focus before action, distinct Enter/Space/text, disabled footer exclusion, draft/runtime isolation and Cancel restoration. One actual run_ui/request_discard verifies final keyboard/pointer checkbox state, section state, text and caret. Existing general-save/runtime tests cover committed apply/publication/rollback. Scoped rustfmt and diff checks pass. Warnings were existing vendored epaint/egui/image warnings. Scoped independent review has no remaining concrete finding. A source-local replay allegation was retracted after tracing egui's real event-clearing lifecycle; unnecessary rollback code was removed.

Earlier scoped accepted checks include M09 paint/parent-clip8 passed/5877 skipped; M08 processed Tab/Enter2 passed/5879 skipped and observer3 passed/5877 skipped; fixture tests5/5 plus production note/search/tag/zero-result semantics and PowerShell parser/reader/predicate checks. Exact historical commands/results and repaired failures are in the execution ledger. No full repository suite, final Clippy or final performance qualification is claimed.

Notes/Todo basic controls now use the same owner/registry. Notes uses visible slug-bound rows, management/search controls and edit Save/Cancel; multiline remains M12. Todo covers draft/filter/tag fields, Add, row completion/tag editing/Remove and clear-confirmation controls; priority DragValue remains M12. Completion only focuses; fresh native Enter performs the existing action. Exact-instance, popup, draft preservation, immediate-next-frame non-Add safety and feature-off behavior are covered. The earlier exact Todo row-identity/legacy run passed2/2; both cases are included in the current83-test union. Native dialog proof remains pending M13.


Timer basic targets cover Duration/Time, Name, Play, Start and Cancel; Sound combo follows M12. Native Timer/Alarm title changes invalidate the old instance. Paste covers saved rows, New/Remove Saved/Playback Settings, name/hotkey/playback choices, simple step-management buttons and footer controls with their existing enablement. Source/advanced editors remain unregistered pending M12. Saved rows have durable semantic and explicit native IDs. Remove Saved confirmation owns only Confirm/Keep; opening and closing discard stale targets immediately. Production action handlers are unchanged; tests use only inert row loading, Keep and cancellation, never timer start/sound or paste execution. Focused5/5 initially passed; scoped review then found a same-pass confirmation handoff defect. After repair, its strengthened exact test passed1/1 and the final union passed83/83 (run e1cc9e15-a854-49d1-af06-7861b7b3ba35). Bounded reviewer closed the transition defect with no new finding. Native evidence remains M13.
## Actual native evidence

All raw paths are under G:/Repos/rust/Multi_Launcher/dev/jump_mode/evidence/. Each accepted run retains summary.md, scenarios.json, events.jsonl, manifest, PNG/receipt/hash bindings and process_lifecycle.json. Parent/reviewer rechecked identities, frame/pass and owned cleanup.

| Accepted run | Evidence/result |
|---|---|
| jump_m01_20261019T232227Z_fc7bc04b | [Populated baseline summary](../../dev/jump_mode/evidence/jump_m01_20261019T232227Z_fc7bc04b/summary.md); parent visual acceptance; PID23360 graceful exit0/tree gone |
| jump_m05_query_probe_20261020T000051Z_b5d58182 | [Query proof](../../dev/jump_mode/evidence/jump_m05_query_probe_20261020T000051Z_b5d58182/summary.md); real ShiftSpace→hintA→query focus→X typing; activation0; PID40556 graceful exit0/tree gone |
| jump_m08_phase1_enabled_20261020T014934Z_c5af8d06_dashboard | [Dashboard01–05](../../dev/jump_mode/evidence/jump_m08_phase1_enabled_20261020T014934Z_c5af8d06_dashboard/summary.md); YA focuses Note006/count0 then distinct Enter exact FillQuery once; PID28440 graceful exit0/tree gone |
| jump_m08_phase1_enabled_20261020T020713Z_48084286_grid | [Grid06–14](../../dev/jump_mode/evidence/jump_m08_phase1_enabled_20261020T020713Z_48084286_grid/summary.md);32 cards/dense hints; exact duplicate-title slug001; stale Enter blocked; query/cancel/unsupported-window safety; PID20588 graceful exit0/tree gone |
| jump_m08_phase1_disabled_20261020T020802Z_db3b2dd1 | [Disabled18](../../dev/jump_mode/evidence/jump_m08_phase1_disabled_20261020T020802Z_db3b2dd1/summary.md); processed native Tab, ordinary ShiftSpace/text,0 targets/actions; PID20440 graceful exit0/tree gone |
| jump_m08_phase1_enabled_20261020T021959Z_abcd5152_dashboard | [Post-placement Dashboard01–05](../../dev/jump_mode/evidence/jump_m08_phase1_enabled_20261020T021959Z_abcd5152_dashboard/summary.md); Refresh labels readable; count0→focus→Enter exact action1; PID23372 graceful exit0/tree gone |

Phase1 cases passed across focused runs; **no whole-aggregate PASS** is claimed. Earlier rejected aggregates, readiness/driver/fixture attempts and their cleanup remain immutable FAIL. The stale test uses a production-verified zero-result query: generation3→4 clears selection, processed Enter does not open a note, restored list generation5 allows exact001 reselect/open. No file watcher/cache behavior was changed.

The ROOT framebuffer records CPU UI completion before GPU presentation; it does not prove presented-window pixels. Structured observations bind screenshots to actual native input, owned PID/build/fixture and retained frame/pass.

Curated fixture-only images (G:/Repos/rust/Multi_Launcher/docs/plans/jump_mode_review/) total **1319518 bytes**, below8MiB:

- [Baseline dashboard](jump_mode_review/00_baseline_dashboard.png),171568bytes.
- [Query hints](jump_mode_review/00_query_hints.png),178133bytes.
- [Dashboard hints after placement repair](jump_mode_review/02_dashboard_hints_active.png),568604bytes; SHA256 f6f455043e592a57de8c24626ba363d0286d559f51e1a79caed5fb6e4b6504af.
- [Dense note grid hints](jump_mode_review/07_note_grid_hints_active.png),401213bytes; SHA25684595d5fe6a7c72ffc9af2dc012d01a0a57a33a4e968534c88a1c5d4601405f2.

## Harness and testing constraints

[Runner README](../../dev/jump_mode/README.md) documents Baseline/QueryProbe, Phase1, Phase1Enabled with Dashboard/Grid filters and Phase1Disabled. Each owned run has bounded commands/steps/deadline, readiness/foreground/PID checks, native keyboard events, environment/window/DPI metadata and retained receipts. It closes gracefully then performs bounded identity-verified cleanup. It never kills by executable name. Aggregate Phase1+FixturesOnly explicitly rejects before children; child fixture-only modes are supported.

Fresh CWD/AppDataRoot and canonical profile notes/template paths isolate production data. Fixture-only focus throttling is disabled; normal production defaults are unchanged. Fixture semantics are checked through production loaders and hotkey validation.

Per user instruction, **every future profile disables Enable Next Shortcut and Enable Cancel Shortcut** (both optional Paste Sequence hotkeys None/null); the verifier rejects either enabled. The accepted early baseline predates this instruction and remains immutable. Native normal/error toasts stay enabled, so unrelated failures remain visible. Shared future root-list and Settings headless fixtures also set both shortcuts None.

M13 preparation identified required isolated stores for later dialogs: todo.json; empty alarms.json; paste_sequences.json plus empty snippets.json/clipboard_history.json; mkmacros.json/assets/templates and legacy macros.json. Constructors may run before plugin filtering. MkMacro recorder polling has no disabled representation; future native fixtures must use a validated unsent recorder chord and no macro hotkeys. No playback, recording, clipboard mutation, sound or system action is authorized in automation.

## Performance and remaining work

Pre-input CPU baseline at b151ff00660976aefd4d12b780f9fcefd7aa6dbb: test profile,100 inert actions,3-column960×640 root fixture,9 warmups/20 samples, timed frame work only (output disposal excluded). Mean159.54µs, median163.1µs, nearest-rank p95 213.9µs, range97.7–228.5µs. Raw log: G:/Repos/rust/Multi_Launcher/dev/jump_mode/evidence/cpu-baseline/m05-a-root-list-baseline.log. Identical disabled/idle/active after measurements remain M14 work; no performance parity claim yet.

Notes/Todo, Timer/Paste, complex controls/MkMacro, native Phase2 evidence, Clippy/reliability/performance and final smoke/handoff remain required. Existing untracked clipboard_modifiers.json is preserved, unread and excluded from task commits. No PR, merge or history rewrite has been performed. Optional manual-review guidance will be finalized only after M15.
