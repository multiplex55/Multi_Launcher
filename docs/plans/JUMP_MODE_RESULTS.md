# Jump Mode Results

M00–M15 are complete. Jump Mode is implemented, disabled by default, and verified across the root, Settings and representative dialogs. Final native root/Settings/Notes/disabled smoke and owned-process cleanup passed, including a smaller owned window and corrected Settings header badges. The 109-test focused union and final 9-test paint/Settings closure pass. Repository-wide formatting/Clippy retain the unrelated baseline failures documented below.

- Branch: feat/jump-mode-2026-10-19-c7e42.
- Starting SHA: 8f94b003a12853735d1cb4689c6ec4e53c4ace8e.
- Final Rust and runner source checkpoint: ff7124054174a52bc785b2485764fad4b54e6bcc (M15-A). Final documentation/image commit is identifiable as the branch HEAD after handoff.
- All source checkpoint commits through M15-A were pushed normally to origin; final documentation/image checkpoint receives one normal best-effort push. Final push outcome is recorded in the handoff.
- Current ownership, exact historical commands, failures and next action: [execution ledger](JUMP_MODE_EXECUTION.md).
- This report consolidates accepted results. Earlier failed evidence remains unchanged in the ignored raw evidence directory.

## Behavior and architecture

The persisted experimental flag defaults off and appears under Settings → Launcher Window & Appearance. The general-settings save coordinator owns committed publication/rollback; editing the checkbox remains a draft until successful Save. Cancel preserves the committed runtime setting.

Focused Shift+Space starts/cancels navigation. Hints are deterministic, uppercase, prefix-free A–Z and case-insensitive. The map is frozen for a session. Escape cancels and Backspace edits the prefix. Hint completion focuses/selects only; later distinct native interaction activates. Search text/caret survive the leader and captured hint input. Same-frame activation is suppressed, held-key guards require release, and native focus loss cancels safely.

One app-owned bridge holds copied semantic/native IDs, geometry, clips, epochs and generations. It collects real enabled actionable Responses in the current viewport, rejects clipped/disabled/ambiguous/stale entries, reconciles at pass/frame completion and paints a clipped overlay without a continuous repaint loop. It retains no widget callbacks or Responses.

Root result IDs use current result generation and full result index, preserving equal-looking actions with different arguments. Selecting a list/grid card updates the existing logical selection and leaves native focus with the query; later Enter uses the existing full Action/args handler. Dashboard controls use slot/row semantic and native IDs so configuration reorder cannot redirect completed focus.

Root/Settings(instance)/Notes(instance)/Todo(instance)/Timer(instance)/PasteSequence(instance)/MkMacro(instance)/Blocked ownership follows actual retained ordinary-window order and focused response layer. Opening/closing/raising a window invalidates navigation and blocks the transition frame across discarded passes. Supported windows use the same input, registry and painter while root editing/autofocus/navigation/Enter yield. Dialog targets require the exact kind and instance; scoped child confirmations retain their parent identity. Existing container Escape/Note Find behavior remains with its owner. Unsupported windows/popups fail closed.

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

The complete changed-source inventory groups into: settings model/save/editor state/mapping/render/save; `gui/jump_mode.rs` and its `ui.rs`/`paint.rs`; LauncherApp/actions/command_host/root render/QA framebuffer; dashboard/dashboard performance fixture/widget context and each discrete widget adapter; Notes/Todo/Timer/Paste dialogs; MkMacro jump adapter/list/properties/table/toolbar/parameter prompt; Paste/Snippet invocation-context accessors; and two narrow vendored egui fixes (`memory/mod.rs` focus-lock initialization and `widgets/drag_value.rs` keyboard clamp/integer step). Build/harness support changes are Cargo.toml, .gitignore, fixture_generator, runner/README, testing policy in AGENTS.md, execution/results and curated PNGs/index. No production dependency was added for Jump Mode.
## Coverage

| Surface/control | Current status and evidence |
|---|---|
| Root query | Supported + verified headless/native: leader/hint/focus, text/caret, ordinary typing afterward |
| Root list/grid, note cards | Supported + verified headless/native: selection before action, full payload, duplicate-title exact slug, stale generation |
| Dashboard discrete controls | Supported + verified representative native FillQuery; full registered inventory below |
| Settings basic controls | Supported headless; native checkbox/slider/numeric/Cancel verified (20ea511d); compact-header placement repaired and visually verified in final M15 run |
| Notes/Todo dialogs | Supported + verified headless/native: Notes focus/newline/Cancel (665061ad); Todo safe Add once (d9288141) |
| Timer/Paste Sequence dialogs | Supported + verified headless/native: Timer text/Sound/Cancel (cb635122), Paste empty context/manual row load/Keep/Cancel (e086058e) |
| Complex control adapters | Representative sliders/numeric/multiline/combos supported+verified headless; native Settings slider/numeric and Timer Sound combo verified; radio adapter tested without inventing product UI |
| MkMacro representative controls | Supported + verified headless/native macro/step selection and actual table focus (5f0e4761); later Enter opens blocked editor |
| Unsupported windows/popups | Background/root suppression verified; recognized combos and Todo/Paste inline confirmations supported headless; other nested owners blocked |
| Feature disabled | Supported + verified headless/native catalog18 |
| Performance/Clippy/final smoke | Current visible CPU comparison and focused checks complete; unrelated baseline Clippy/format failures recorded; M15 native smoke complete |

Dashboard registration covers RecentCommands rows; PinnedQueryResults/QueryList rows and Refresh; Todo summary/row Open; Calendar Open/Add; Temp Add/ClearAll/Refresh/row Open/Clear; Timer New/Pause/Cancel; Scratchpad Copy/AppendTimestamp/Refresh; NowPlaying controls; Recycle Clean/Refresh; SystemActions rows/Refresh; SystemStatus Refresh. Native automation activates only safe fixture note FillQuery actions. PinnedCommands/CommandHistory and complex Todo/Scratchpad/Calendar controls remain unregistered.

Settings registration covers all eight top-level section toggles, Expand/Collapse all, Save/Cancel, direct built-in checkboxes/single-line fields and simple buttons in Hotkeys, Paste Sequences, Launcher, Search, Actions/Feedback, Dashboard and Radial. The Plugins section header is supported; delegated plugin/Note settings controls and unlisted complex controls remain unregistered. Disabled pending-save footer controls are excluded. The section target is the actual collapse-toggle Response, with existing persistent section identity.

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
| M12 | complete | M12-A89tests; M12-B96tests + scoped review closed3P2s |
| M13 | complete | Six native surfaces + disabled recheck; shared Escape/context-owner repairs; scoped checks and curated images |
| M14 | complete | 109focused tests; compact badge repair; current CPU comparison; source review; default check; baseline lint/format exceptions below |
| M15 | complete | Final native root/Settings/Notes/disabled smoke; owned resize; compact-badge visual closure; evidence/handoff |

## Verification

Latest combined command on accepted M12-B source, exit0, **96 passed / 5827 skipped**:

~~~powershell
cargo nextest run --lib --no-fail-fast --cargo-profile fast-dev --features "dev-smoke-binaries qa-framebuffer-capture" -E 'test(jump_mode) | test(actual_root_navigation_owner_follows_settings_and_foreign_window_layers) | test(opening_settings_on_a_discarded_root_pass_cancels_background_navigation) | test(settings_owner_blocks_root_autofocus_and_enter_activation) | test(settings_escape_closes_when_jump_mode_owns_root_navigation) | test(note_find_shortcut_and_escape_leave_dirty_note_open_until_the_next_escape) | test(note_find_shortcut_uses_focused_or_top_note_and_preserves_ctrl_w_close) | test(settings_shell_tests) | test(todo_row_widget_ids_stay_bound_to_entry_when_filter_moves_another_row_to_zero) | test(enter_adds_todo_with_filter_focus) | test(enter_adds_todo_with_tags) | test(ordinary_enter_cannot_activate_root_results_behind_timer_or_paste_dialogs) | test(open_timer_resets_timer_fields_and_arms_duration_focus) | test(open_alarm_clears_pending_timer_focus_and_keeps_alarm_title) | test(paste_saved_row_native_identity_survives_removal_of_an_earlier_row)'
~~~

The real Settings test verifies focus before action, distinct Enter/Space/text, disabled footer exclusion, draft/runtime isolation and Cancel restoration. One actual run_ui/request_discard verifies final keyboard/pointer checkbox state, section state, text and caret. Existing general-save/runtime tests cover committed apply/publication/rollback. Scoped rustfmt and diff checks pass. Warnings were existing vendored epaint/egui/image warnings. Scoped independent review has no remaining concrete finding. A source-local replay allegation was retracted after tracing egui's real event-clearing lifecycle; unnecessary rollback code was removed.

Earlier scoped accepted checks include M09 paint/parent-clip8 passed/5877 skipped; M08 processed Tab/Enter2 passed/5879 skipped and observer3 passed/5877 skipped; fixture tests5/5 plus production note/search/tag/zero-result semantics and PowerShell parser/reader/predicate checks. Exact historical commands/results and repaired failures are in the execution ledger. No full repository suite was run. Final scoped Clippy/performance outcomes, including unrelated repository-check failures, are recorded in M14 below.

Notes/Todo basic controls now use the same owner/registry. Notes uses visible slug-bound rows, management/search controls and edit Save/Cancel; its draft multiline editor is now registered in M12-A. Todo covers draft/filter/tag fields, Add, row completion/tag editing/Remove and clear-confirmation controls; priority DragValue is now registered and verified with actual u8 arrow/bounds tests in M12-A. Completion only focuses; fresh native Enter performs the existing action. Exact-instance, popup, draft preservation, immediate-next-frame non-Add safety and feature-off behavior are covered. The earlier exact Todo row-identity/legacy run passed2/2; both cases are included in the current89-test union. Accepted M13 native runs below now cover representative Notes/Todo behavior.


Timer basic targets cover Duration/Time, Name, Play, Start and Cancel; Sound combo is now registered in M12-A. Native Timer/Alarm title changes invalidate the old instance. Paste covers saved rows, New/Remove Saved/Playback Settings, name/hotkey/playback choices, simple step-management buttons and footer controls with their existing enablement. Source/advanced Paste editors remain unregistered. Saved rows have durable semantic and explicit native IDs. Remove Saved confirmation owns only Confirm/Keep; opening and closing discard stale targets immediately. Production action handlers are unchanged; tests use only inert row loading, Keep and cancellation, never timer start/sound or paste execution. Focused5/5 initially passed; scoped review then found a same-pass confirmation handoff defect. After repair, its strengthened exact test passed1/1 and the final union passed83/83 (run e1cc9e15-a854-49d1-af06-7861b7b3ba35). Bounded reviewer closed the transition defect with no new finding. Accepted representative M13 native evidence is indexed below.

M12-A adds distinct thumb/numeric targets to the five existing Settings float rows: Toast duration0.1–5, Query/List scale0.5–5, Fuzzy/Usage weight0–5. Debug logging and Timer Sound expose their actual combo anchors and visible options. Notes draft multiline uses native caret/input; Todo priority uses native integer editing. Radio focus/Space behavior is verified at the adapter level; no new Settings radio UI was added.

Recognized native combos require exactly one current owned anchor, actual foreground popup layer and focus in the parent or that popup. The invisible sizing pass and owner-transition passes expose no eligible targets; only narrow anchor provenance survives until the actual popup is interactive. Unknown/foreign/stale popups remain blocked. Existing inline Todo/Paste confirmations retain their separate child scopes. Real foreign Foreground TextEdit regression proves its leader/text remain untouched with the combo still open.

Actual native-widget tests cover focus without mutation, later numeric/radio/text actions, multiline typing/newline, combo option focus/activation/Escape, first and repeated slider arrows and continuous Todo priority arrows at0/255. A final focused3/3 run passed, then the literal combined filter passed89/89. Early integer/no-op, range, first-arrow focus and fixture-kinetic-scroll failures are documented in the execution ledger. Narrow vendored fixes clamp keyboard increments, give zero-decimal keyboard input a meaningful signed step without changing pointer sensitivity, and allow native key filters on newly focused widgets. One-shot focus seeds native editor/slider filters; exact owned TextEntry identity preserves focus across legitimate save generations. Scoped review closed both popup and numeric/focus findings; no high/medium finding remains. M13 subsequently verified actual Settings slider/numeric and Timer combo behavior; see native evidence below.
## M13-A native fixture and observation foundation

Every newly generated profile now seeds isolated typed Todo, alarm, clipboard/history, snippet, legacy macro, Paste Sequence and MkMacro stores before constructors, regardless of plugin allowlist. Production loaders, MkMacroStore save/reopen, semantic validation and an in-memory recorder conflict probe verify the fixture. Macros and steps are disabled, no macro hotkey is bound, and the recorder chord is an unsent Ctrl+Alt+Shift+F24. Strict fixture/safety manifests reject unknown fields. Next/Cancel remain None and normal/error toasts remain enabled.

An explicit ROOT/F11 Phase2-Paste QA request must match the fixture hash, identity, schema, surface and isolation/safety flags before the existing paste:open route can choose an empty invocation context. The choice happens before OS clipboard capture; absent/default requests keep the existing production capture path. Only clipboard-presence booleans are exposed. No clipboard mutation, playback, recording or native input occurred during this checkpoint.

The retained observer adds structured active owner, surface/instance/parent, semantic target and generation facts, plus named read-only dialog snapshots. Existing ROOT/query behavior remains intact. Todo reports the production target generation; MkMacro reports logical selection and actual expected-table focus independently, preserving row focus truth. Both review findings were repaired and their regressions passed.

Verification: fixture target2/2 (5skipped), new QA/context tests5/5 (5923skipped), existing ROOT/query/activation compatibility6/6 (5922skipped), all exit0. Exact commands/run IDs and normal compiler repairs are in the execution ledger. Scoped rustfmt/diff checks pass; unrelated playback formatting churn was removed. These checks establish the foundation; M13-B subsequently completed the native workflows, control transitions, screenshots and cleanup evidence indexed below.

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

Curated fixture-only images (G:/Repos/rust/Multi_Launcher/docs/plans/jump_mode_review/) total **2744955 bytes**, below8MiB:

- [Baseline dashboard](jump_mode_review/00_baseline_dashboard.png),171568bytes.
- [Query hints](jump_mode_review/00_query_hints.png),178133bytes.
- [Dashboard hints after placement repair](jump_mode_review/02_dashboard_hints_active.png),568604bytes; SHA256 f6f455043e592a57de8c24626ba363d0286d559f51e1a79caed5fb6e4b6504af.
- [Dense note grid hints](jump_mode_review/07_note_grid_hints_active.png),401213bytes; SHA25684595d5fe6a7c72ffc9af2dc012d01a0a57a33a4e968534c88a1c5d4601405f2.

M13 native runs accepted so far:

- [Settings summary](../../dev/jump_mode/evidence/jump_m13_phase2_settings_20261020T093421Z_20ea511d/summary.md): exact opener once; checkbox draft false/runtime true; slider1.0→1.05; numeric1.05→1.1; inactive exact focused numeric capture; later Up→1.2; Cancel runtime1.0/Jump true. PID32812 graceful exit0/tree gone. Three receipt PID/fixture/frame/pass/PNG hash bindings independently checked; selected pixels viewed.
- [Notes summary](../../dev/jump_mode/evidence/jump_m13_phase2_notes_20261020T093507Z_665061ad/summary.md): exact note:dialog opener once; inactive content-editor focus before typing;18 characters and native newline, then Cancel leaves32notes. PID14600 graceful exit0/tree gone. Two receipt bindings independently checked and selected pixels viewed.
- [Todo summary](../../dev/jump_mode/evidence/jump_m13_phase2_todo_20261020T093921Z_d9288141/summary.md): map-derived New Todo focus, typing and later Enter adds exactly one isolated entry; PID36528 graceful exit0/tree gone. Parent independently checked both receipt PID/fixture/frame/pass/PNG hash bindings and actual17 pixels. The narrow Todo window clips the row list, so the added ID is proven by captured state rather than a visible row.

Curated selected-state additions: [Settings numeric focus](jump_mode_review/17_settings_numeric_selected.png),328310bytes/SHA2564af2cb39cfb3aaf029f76a1c15113ffa4120a8d314e81752230fe46685071196; [Notes content focus](jump_mode_review/17_notes_content_selected.png),381682bytes/SHA2561cb4b880073f2e3d3349fbd2b4dbeaee82d3ec1eaf1eb8ecd74440ccd7725be5. Total remains below8MiB. Failed fixture/Settings/Todo attempts are retained unchanged, with exact causes/cleanup in the ledger. No whole Phase2 aggregate PASS is claimed.

## Harness and testing constraints

[Runner README](../../dev/jump_mode/README.md) documents Baseline/QueryProbe, Phase1, Phase1Enabled with Dashboard/Grid filters and Phase1Disabled. Each owned run has bounded commands/steps/deadline, readiness/foreground/PID checks, native keyboard events, environment/window/DPI metadata and retained receipts. It closes gracefully then performs bounded identity-verified cleanup. It never kills by executable name. Aggregate Phase1+FixturesOnly explicitly rejects before children; child fixture-only modes are supported.

Fresh CWD/AppDataRoot and canonical profile notes/template paths isolate production data. Fixture-only focus throttling is disabled; normal production defaults are unchanged. Fixture semantics are checked through production loaders and hotkey validation.

Per user instruction, **every future profile disables Enable Next Shortcut and Enable Cancel Shortcut** (both optional Paste Sequence hotkeys None/null); the verifier rejects either enabled. The accepted early baseline predates this instruction and remains immutable. Native normal/error toasts stay enabled, so unrelated failures remain visible. Shared future root-list and Settings headless fixtures also set both shortcuts None.

M13 preparation identified required isolated stores for later dialogs: todo.json; empty alarms.json; paste_sequences.json plus empty snippets.json/clipboard_history.json; mkmacros.json/assets/templates and legacy macros.json. Constructors may run before plugin filtering. MkMacro recorder polling has no disabled representation; future native fixtures must use a validated unsent recorder chord and no macro hotkeys. No playback, recording, clipboard mutation, sound or system action is authorized in automation.

## Performance and remaining work

Historical M05 CPU log at b151ff00660976aefd4d12b780f9fcefd7aa6dbb is INVALID as a visible-grid timing baseline: M14 diagnosed the shared setup rendered zero rows/targets because the root/query owner remained hidden. Its nominal configuration was test profile,100 actions,3-column960×640,9 warmups/20 samples, but the old timings do not establish visible rendering cost or before/after parity. The raw log is preserved at G:/Repos/rust/Multi_Launcher/dev/jump_mode/evidence/cpu-baseline/m05-a-root-list-baseline.log. The corrected contemporary comparison passed (Nextest6f649f0c,1passed/5935skipped), with raw samples at G:/Repos/rust/Multi_Launcher/dev/jump_mode/evidence/cpu-comparison/m14-root-list-disabled-idle-active-test-profile-raw.log. No historical rerun or performance claim from hidden samples.

Compact badge source repair, focused reliability/performance and lint review are complete. Final representative native smoke and visual acceptance passed; final handoff is below. Existing untracked clipboard_modifiers.json is preserved, unread and excluded from task commits. No PR, merge or history rewrite has been performed. Optional post-completion manual review is listed at the end.

M12-B source is frozen after96/96 targeted tests and scoped formatting/diff checks. MkMacro targets cover visible durable macro/step rows, fold buttons, Refresh checks/+Action and representative Name/Description/Enabled/Folder/playback properties. Hinting only selects/focuses; a later native Enter opens the existing safe step editor or action catalog. Row identity survives reorder and duplicate names; clipped rows are excluded. Active recording, action editor, search/package/confirmation/key capture/picker children and foreign windows block underlying navigation. Unsafe toolbar controls remain unregistered. Explicit Select handoff tracks table native focus separately from the semantic row, and exact owned Slider continuity preserves repeated arrow editing across draft generations. M13 subsequently verified exact macro/step selection, native table focus and later editor Enter; all three M12 reviewer findings remain closed.
M13 Timer native [cb635122 summary](../../dev/jump_mode/evidence/jump_m13_phase2_timer_20261020T101148Z_cb635122/summary.md) passed after fixing shared Escape ownership: an open native popup handles Escape before its containing dialog. Actual Sound option hint then fresh Enter changes index0→1 with activation1/timers0/alarms0; native Escape dismisses the popup, Duration/Name accept normal typing, and Cancel closes without Start/Play. Both receipt PID/fixture/frame/pass/PNG hashes independently checked, actual17 pixels viewed; PID26028 graceful exit0/owned tree gone. Focused Escape regression3/3 passed (607480c3); earlier popup-readiness test failures and two native Timer failures remain immutable in the ledger.

M13-B final accepted additions:

- [Paste Sequence](../../dev/jump_mode/evidence/jump_m13_phase2_paste_20261020T102403Z_e086058e/summary.md): actual typed `paste:open` once, context present/clipboard-text absent, manual saved row loads0→1 step, Keep/Cancel and store/history hashes unchanged. PID33836 graceful exit0/tree gone.
- [MkMacro](../../dev/jump_mode/evidence/jump_m13_phase2_mkmacro_20261020T102454Z_5f0e4761/summary.md): selected macro1/step1 and actual native table focus, unchanged revision and inactive runtime/recorder at selected capture; later Enter opens actual editor with Blocked owner/zero background targets. Post-editor runtime/revision are not directly exposed while Blocked. PID25140 graceful exit0/tree gone.
- [Disabled recheck](../../dev/jump_mode/evidence/jump_m08_phase1_disabled_20261020T102531Z_97af62fd/summary.md): ordinary Shift+Space/X text, query ` x`, zero targets/actions; PID6832 graceful exit0/tree gone.

Parent independently checked every accepted retained receipt's PID/fixture/frame/pass/PNG hash bindings and representative selected pixels. Curated Paste/MkMacro images are byte-identical fixture screenshots, bringing eight PNGs to2744955bytes. Actual metadata is1800×1000 at scale1; non-1×DPI/multiple-monitor operation is not claimed. Dense32-target evidence was reviewed earlier; the compact-header collision is an explicit M14 repair and active-session resize/final smaller-window visual closure follows in M14/M15.

The failed Paste run6f8d9115 exposed duplicate initialization: the approved empty context was overwritten by `focus_panel`→`ensure_open`, which read clipboard context. Selection now belongs to one `ensure_open` path; the typed opener no longer opens twice. A full typed-action regression forbids clipboard capture before the OS getter, asserts zero capture calls, and preserves the empty context (2/2 scoped tests, ec49eabf). No paste ran and no clipboard mutation occurred; agents did not inspect/log clipboard contents. The first post-fix ordinary-sandbox attempt1937e223 failed process inventory with access denied and retains `cleanup_uncertain`; exact PID7724 was later absent. Its lifecycle proof is not upgraded. The accepted escalated retry has verified owned-tree cleanup. All future native runs use the established escalated runner route.
M14 current CPU comparison: source f8375cc3dedd7f74bcced246c316afa001e8ee24+dirty/M14source, Cargo test profile (unoptimized+debuginfo),100actions/3columns/960×640,9warmups and20samples per state; output disposal outside the timed frame. All cases submitted39 row widgets; enabled cases registered33 eligible root actions plus query (34total), excluding clipped rows. Disabled registered0targets.

| Current state | Mean CPU frame (ms) | Median (ms) | Range (ms) |
|---|---:|---:|---:|
| Disabled | 3.182 | 3.155 | 3.004–3.729 |
| Enabled idle | 3.397 | 3.397 | 3.245–3.577 |
| Active hints | 4.833 | 4.452 | 3.669–7.996 |

Idle mean increment was0.216ms (~6.8%) in this small local debug sample. Active samples measure no-input UI frames including overlay painting, not keypress latency or GPU presentation. Source inspection found no continuous Jump-specific repaint, retained callbacks, per-widget subscriptions or unbounded registry. This is a contemporary disabled/idle/active comparison; a valid pre-feature visible-grid baseline is unavailable. No release-performance or exact historical-parity claim.

## M14 final combined verification

Final focused union after four behavior-preserving Clippy let-chain fixes: Nextest42b36e2e-1223-42b0-8165-6788e6503c27, **109passed/5827skipped**, exit0. Exact command:

~~~powershell
cargo nextest run --lib --no-fail-fast --cargo-profile fast-dev --features "dev-smoke-binaries qa-framebuffer-capture" --status-level fail --final-status-level fail -E 'test(jump_mode) | test(actual_root_navigation_owner_follows_settings_and_foreign_window_layers) | test(opening_settings_on_a_discarded_root_pass_cancels_background_navigation) | test(settings_owner_blocks_root_autofocus_and_enter_activation) | test(settings_escape_closes_when_jump_mode_owns_root_navigation) | test(note_find_shortcut_and_escape_leave_dirty_note_open_until_the_next_escape) | test(note_find_shortcut_uses_focused_or_top_note_and_preserves_ctrl_w_close) | test(settings_shell_tests) | test(todo_row_widget_ids_stay_bound_to_entry_when_filter_moves_another_row_to_zero) | test(enter_adds_todo_with_filter_focus) | test(enter_adds_todo_with_tags) | test(ordinary_enter_cannot_activate_root_results_behind_timer_or_paste_dialogs) | test(open_timer_resets_timer_fields_and_arms_duration_focus) | test(open_alarm_clears_pending_timer_focus_and_keeps_alarm_title) | test(paste_saved_row_native_identity_survives_removal_of_an_earlier_row) | test(compact_settings_arrow_badges_clear_the_actual_adjacent_heading) | test(wide_card_badges_stay_in_the_target_and_clear_its_label) | test(phase2_fixture_capability_requires_exact_surface_identity_and_safe_allowlist) | test(phase2_request_is_root_f11_and_empty_context_is_paste_only) | test(phase2_paste_context_suppression_is_explicit_and_scoped_to_qa_request) | test(todo_dialog_observation_reports_registry_target_generation) | test(mkmacro_observation_separates_selected_step_from_actual_table_focus) | test(settings_observation_separates_draft_weight_from_effective_runtime_weight) | test(timer_dialog_observation_reports_actual_sound_index) | test(qa_observation_reports_the_actual_sound_selection) | test(typed_paste_open_keeps_bound_empty_context_through_panel_ensure_open) | test(timer_popup_escape_closes_popup_before_parent_and_jump_escape_cancels_first) | test(launcher_escape_defers_to_file_search_and_native_popup) | test(native_combo_escape_closes_without_changing_the_selected_value)'
~~~


M14 final checks and source release: `cargo check --bin multi_launcher` PASS; `cargo clippy --all-targets --features "dev-smoke-binaries qa-framebuffer-capture"` FAIL exit1 solely on two denied pre-existing Pinboard test-helper `never_loop` lints (src/pinboard/live.rs:2313,2346). That file is unchanged from starting8f94. Four feature-authored collapsible-if warnings were fixed; final Clippy no longer reports them. `cargo fmt --check` FAIL on untouched baseline formatting in src/bin/radial_acceptance/native.rs, src/lib.rs, src/paste_sequence/playback.rs. `rustfmt --edition 2024 --config skip_children=true --check src/gui/jump_mode/paint.rs src/gui/jump_mode/ui.rs src/gui/render.rs src/settings_editor/render.rs src/gui/paste_sequence_dialog.rs` PASS and `git diff --check` PASS. No unrelated lint/format fixes, no full suite. Final read-only review had no concrete high/medium finding; parent inspected subsequent legacy-placement guard and four syntactic let-chain changes. Source frozen/released. M14 complete with explicit unrelated repository-check exceptions; subsequent M15 native visual acceptance is recorded below.
## M15 final native smoke

Frozen source checkpoint774fd160, runner M15 working changes; each command uses `pwsh -NoProfile -ExecutionPolicy Bypass -File dev/jump_mode/run.ps1` plus the arguments below. Every native command used the established escalated execution route. FixtureOnly preflight98ccf72c passed7/7 tests and typed verification without launching GUI.

| Arguments | Actual evidence | Result |
|---|---|---|
| `-Scenario Phase1Enabled -Screen FinalSmoke` | [Root smoke summary](G:/Repos/rust/Multi_Launcher/dev/jump_mode/evidence/jump_m15_final_smoke_20261020T113712Z_4c58c867/summary.md) | PASS; catalog05 FillQuery count1,09 exact note001 count2; same owned HWND/client1800×1000→1527×844,DPI96,receipt PNGs match;PID35132 graceful0/treegone |
| `-Scenario Phase2 -Surface Settings` | [Settings functional run](G:/Repos/rust/Multi_Launcher/dev/jump_mode/evidence/jump_m13_phase2_settings_20261020T113805Z_6684e9b2/summary.md) | Functional PASS/PID12684 graceful0/treegone; visual closure failed: header overlap gone but several assigned compact hints hidden. Retained unchanged; subsequently closed by final12e41b61 run below |
| `-Scenario Phase2 -Surface Notes` | [Notes smoke summary](G:/Repos/rust/Multi_Launcher/dev/jump_mode/evidence/jump_m13_phase2_notes_20261020T113852Z_3f65bb1a/summary.md) | PASS; actual content editor focus/edit/cancel,32notes preserved;PID21992 graceful0/treegone |
| `-Scenario Phase1Disabled` | [Disabled smoke summary](G:/Repos/rust/Multi_Launcher/dev/jump_mode/evidence/jump_m08_phase1_disabled_20261020T113933Z_e40459d2/summary.md) | PASS; ordinary query ` x`,disabled/inactive,0targets/0actions;PID18144 graceful0/treegone |

Parent independently checked every retained capture's PID/fixture/frame/pass/PNG hash bindings and representative Root/Notes/Settings pixels. FinalSmoke claims only catalog05/09; prior full Phase1 evidence remains the earlier focused catalog runs. This first M15 Settings run exposed a genuine visibility defect despite functional PASS. The real clip-min25 regression, source repair and fresh final Settings pixels below subsequently closed it.

M15 narrow visual repair adds one candidate after the actual Settings heading, preserving native target IDs and the existing clipping/occlusion rules. The real x=25 clip regression and affected paint/Settings tests passed 9/9 (3b5d03b6,5927skipped), with scoped formatting/diff checks. Exact command is in the execution ledger. Fresh Settings15/16 pixels passed in final12e41b61 below; the earlier functional run remains unchanged.

## Final acceptance and limitations

The final Settings run [12e41b61 summary](G:/Repos/rust/Multi_Launcher/dev/jump_mode/evidence/jump_m13_phase2_settings_20261020T114315Z_12e41b61/summary.md) passed against source ff712405. PID31600 exited gracefully with code0; owned-tree cleanup is verified. Parent checked all three receipt PID/fixture/frame/pass/PNG-hash bindings: active15 frame/pass149, active16 frame/pass1336, inactive17 frame/pass1669. Actual15/16 pixels show section badges beside readable titles; a final independent read-only inspection also confirmed boxed Hotkeys C in16. Paste D, Launcher E and Search F are visible. This closes the earlier hidden-header defect; no further source changes or native reruns were made.

Final exact source verification is the 109-test union (42b36e2e,109passed/5827skipped), followed by the affected paint/actual-Settings closure (3b5d03b6,9passed/5927skipped). M14 default `cargo check --bin multi_launcher` passed; the final native Settings runner rebuilt `cargo build --profile fast-dev --features dev-smoke-binaries,qa-framebuffer-capture --bin multi_launcher` successfully on ff712405. The runner parser, fixture7/7, scoped Rust formatting and diff checks pass. Full `cargo fmt --check` and `cargo clippy --all-targets --features "dev-smoke-binaries qa-framebuffer-capture"` remain unsuccessful only for the explicitly identified untouched baseline formatting and two Pinboard never_loop errors. Four feature-authored Clippy warnings were corrected. No full repository suite was run.

There are no unimplemented required milestones. Supported surfaces/control inventories are above; unsupported advanced editors and foreign owners deliberately block background targets. Native Debug Logging combo selection was not exercised because that fixture control was clipped; native Timer Sound demonstrates the supported combo path. Actual native captures used 96 DPI/scale1.0; other DPI scales and multiple monitors remain untested. ROOT captures prove CPU UI completion before GPU presentation, not presented desktop pixels. The timing sample is local, debug-profile and contemporary; the hidden historical baseline is invalid.

The failed M13 Paste run read clipboard context through a duplicate initialization path; that owner defect is fixed and the full typed path has a no-getter regression. No paste execution or clipboard mutation occurred, and no clipboard contents were inspected/logged by agents. One failed sandbox run retains cleanup_uncertain even though its parent PID was later absent; later accepted runs have complete tree proof. These historical facts remain visible in the ledger rather than being relabeled as passes.

The unrelated pre-existing untracked clipboard_modifiers.json remains untouched, unread and excluded. Raw evidence stays ignored/local. No PR, merge, force-push, history rewrite or production dependency was added.

## Final screenshot index

Ten fixture-only curated PNGs total 3,426,581 bytes (<8MiB) in [the review directory](G:/Repos/rust/Multi_Launcher/docs/plans/jump_mode_review/README.md). All are byte-identical copies of retained, receipt-bound captures.

- [Final active Settings hints](G:/Repos/rust/Multi_Launcher/docs/plans/jump_mode_review/16_settings_hints_final.png):1800×1000,340691bytes,SHA25681ce7295d280c920f5757ede56a4496cc7a8ab0ae85f74d004e84da924416308; final12e41b61 capture16.
- [Final resized exact note opened](G:/Repos/rust/Multi_Launcher/docs/plans/jump_mode_review/09_resized_note_opened_final.png):1527×844,340935bytes,SHA2567e4acff17f20ad3887302a7c13069eb441cd21cea84ff58aea85fdd725dfdb0b;4c58c867 capture09.
- [Dashboard hints](G:/Repos/rust/Multi_Launcher/docs/plans/jump_mode_review/02_dashboard_hints_active.png) and [dense note-grid hints](G:/Repos/rust/Multi_Launcher/docs/plans/jump_mode_review/07_note_grid_hints_active.png).
- [Settings numeric focus](G:/Repos/rust/Multi_Launcher/docs/plans/jump_mode_review/17_settings_numeric_selected.png), [Notes content focus](G:/Repos/rust/Multi_Launcher/docs/plans/jump_mode_review/17_notes_content_selected.png), [Paste sequence loaded](G:/Repos/rust/Multi_Launcher/docs/plans/jump_mode_review/17_paste_sequence_loaded.png), and [MkMacro step selected](G:/Repos/rust/Multi_Launcher/docs/plans/jump_mode_review/17_mkmacro_step_selected.png).
- [Baseline dashboard](G:/Repos/rust/Multi_Launcher/docs/plans/jump_mode_review/00_baseline_dashboard.png) and [query hints](G:/Repos/rust/Multi_Launcher/docs/plans/jump_mode_review/00_query_hints.png).

## Optional review after completion

When convenient, inspect the final Settings/dense-grid screenshots, enable Experimental Jump Mode under Settings → Launcher Window & Appearance and Save, then try Shift+Space → hint → later Enter plus Escape/second-leader cancellation in your preferred layout. A separate check on non-1×DPI or multiple monitors would cover the environment not available in this run. This is optional post-handoff review, not a missing implementation gate. All future tests must keep Enable Next Shortcut and Enable Cancel Shortcut disabled as recorded in AGENTS.md.
