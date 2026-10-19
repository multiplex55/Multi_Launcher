# Jump Mode Execution

- Authority: CODEX_GOAL_JUMP_MODE.md (approved M00–M15), CODEX_KICKSTART_JUMP_MODE.md, current AGENTS.md.
- Source branch: master; starting SHA: 8f94b003a12853735d1cb4689c6ec4e53c4ace8e.
- Task branch: feat/jump-mode-2026-10-19-c7e42 (created from checked-out HEAD).
- Initial working tree: clean, including tracked task-input documents. No user changes copied/discarded.
- Environment: Windows PowerShell; rustc/cargo 1.99.0; Nextest 0.9.135; egui/eframe 0.36.2 with local vendor patches.
- Git branch creation needed sandbox escalation because .git is read-only in the normal sandbox; authorized creation succeeded.
- Current milestone: M01 baseline accepted; M05 native query proof next.
- Latest GUI evidence: accepted baseline jump_m01_20261019T232227Z_fc7bc04b; curated 00_baseline_dashboard.png.
- Latest checkpoint: 7a537081946e2cd49fe634c7a7b5d846bec84c89 — feat(jump): [M05-B] paint clipped hints and focus feedback.
- Push status: all checkpoints through M05-B (7a537081) pushed successfully to origin; upstream configured.

| Milestone | State | Checkpoint / verification | Evidence or dependency |
|---|---|---|---|
| M00 | complete | Branch from recorded HEAD; verified source map | Clean initial tree; existing fixture isolation seam |
| M01 | complete | M01-B production semantic fixture checks + accepted real baseline | 232227Z_fc7bc04b; visual review + owned cleanup PASS |
| M02 | complete | M02-A 3 tests + M02-B ownership regression passed | Real general apply/publication/rollback covered |
| M03 | complete | 10/10 targeted Nextest tests; scoped fmt/diff | Typed IDs, frozen mapping, prefix-free matching |
| M04 | complete | 24/24 all-Jump Nextest; scoped fmt/diff clean | Copied responses, multipass retention, one-shot intents |
| M05 | in_progress | M05-A input + M05-B painter; 33/33 all-Jump tests | Native query-only proof remains pending |
| M06 | pending | Grid integration | M05 |
| M07 | pending | Dashboard integration | M05 |
| M08 | pending | Native keyboard acceptance | M01, M06–M07 |
| M09 | pending | Phase 1 review | M02–M08 |
| M10 | pending | Modal ownership and Settings | M09 |
| M11 | pending | Notes/Todo/Timer/Paste Sequence | M10 |
| M12 | pending | Complex controls and MkMacro | M10–M11 |
| M13 | pending | Extended native evidence | M08, M10–M12 |
| M14 | pending | Focused reliability and review | M09, M13 |
| M15 | pending | Final smoke and handoff | All prior outcomes |

## Verified source ownership

- `src/main.rs::main`: derives `AppDataRoot` from cwd-relative settings.json and acquires `SingleInstanceGuard` before persistent reads. Existing cwd isolation can preserve production identity semantics without adding a bypass.
- `src/main.rs::spawn_gui`: creates `LauncherApp`, installs optional QA framebuffer request and startup services, runs native Glow renderer.
- `src/platform/app_data.rs::AppDataRoot`: records existing cwd-relative resolution; normalizes directory identity for single-instance locking.
- `src/gui/mod.rs::LauncherApp`: owns query, logical selected result and `handle_key`; current action/navigation tests live alongside it.
- `src/gui/render.rs`: owns root frame phases, query editor/autofocus, keyboard routing and result painting. True query focus gates several existing keyboard flows; Jump Mode must account for this explicitly.
- `src/gui/render.rs::root_grid_cell_response` and the list selectable responses provide native focusable card targets; `RootListGeometry::result_generation()` owns stale-result invalidation. The results loop calls `focus_input()` each visible frame without a panel: this must yield while a jump-selected non-query control owns focus. `note:open:{slug}` distinguishes notes with duplicate titles.
- `src/gui/render.rs`: multipass rendering guards side effects with `first_ui_pass`; existing action sheet, OCR and snippet prompt have higher input ownership. Capture leader before query rendering while respecting those owners. Settings/confirmation use ordinary egui windows; explicit surface ownership is needed, not panel-stack inference alone.
- `src/actions/mod.rs::Action`: label/description/action/args. `ActionIdLookup` is tied to its immutable catalog; IDs use action strings. A stale result index alone is insufficient for deferred targeting.
- `src/dashboard/dashboard.rs::DashboardContext`, `WidgetActivation`; `src/dashboard/widgets/mod.rs::Widget`, `WidgetAction`: widgets return existing action payloads, dashboard owns slot identity. Registration must preserve this route.
- `src/settings/model.rs::Settings`: serde defaults and explicit Rust Default; existing `settings_editor/render.rs` controls need a small opt-in setting.
- `src/gui/qa_framebuffer.rs`: existing gated ROOT capture, startup/F11 triggers, maximum 16 captures per request, exclusive fresh output directory. ROOT captures CPU framebuffer completion before GPU presentation; no claim of presented pixels.
- `dev/run_owned_process.ps1`: verified PID/creation-time tree tracking and bounded cleanup. Runner currently lacks cwd/env overrides, so reuse its ownership pattern or extend narrowly.
- Notes fixture must explicitly use `ML_NOTES_DIR`; safe enabled-plugin allowlist and empty index paths prevent loading live user sources.

## Execution boundaries

Parent owns branch, ledger, commits and one best-effort push per checkpoint. Planner is read-only. One implementer owns each bounded source milestone; reviewer is read-only at meaningful integration boundaries. No PR, merge, force push, unrelated refactor or live-data testing.

## Next action

M05-B painter committed/pushed. Sole fixture implementer owns M01-B concrete scheduling/tag repair, production semantic checks and one owned baseline; parent reviews actual PNG/content before acceptance. Native query-only Jump proof follows. M01-B fixture root-cause handoff is ready; populated dashboard baseline remains incomplete until semantic and rendered-content proof.



## M01 implementation observations

- Initial fixture-only attempt exposed a nested helper-package dependency-resolution problem: it did not inherit root vendor patches/lock and attempted unavailable registry access. Correcting ownership to a development-only root-package fixture binary, reusing existing dependencies. This is a harness build issue, not a GUI environment result.
- Parent scaffold review identified PowerShell function-scope lifecycle reporting and required `process_lifecycle.json`/scenario status corrections; being repaired before capture acceptance.
- No GUI baseline claimed passed yet.
- Actual baseline run: `dev/jump_mode/evidence/jump_m01_20261019T210335Z_cd3ec49b/summary.md`. Real 900×560 ROOT framebuffer captured; process 2128 closed gracefully, exit 0, verified owned tree gone. Visual review rejected fixture contents (hotkey conflict and empty pinned results); final baseline scenario FAIL. Capture provenance and cleanup remain valid, original PNG retained.
- M01 review P2 (inventory/log errors could skip root cleanup) repaired; targeted read-only rereview confirms resolved. Fixture-only 2/2 Rust unit tests and typed generation/verify passed; dashboard baseline remains incomplete until later-frame readiness and content are proven.

- M01-A latest fixture-only verification: jump_m01_20261019T211636Z_8316f826, 2/2 generator tests plus typed validation PASS; dashboard correctly BLOCKED (not requested). Fixture rustfmt, PowerShell parser and git diff --check pass. Root cargo fmt --all -- --check reports pre-existing formatting outside this change; fixture file corrected. Reviewer cleanup finding resolved.

## M02 outcome
- Added experimental_jump_mode_enabled (serde/Rust false), Settings Launcher Window toggle/help and existing committed save/apply runtime gate/accessor/setter.
- cargo nextest run --lib -E 'test(jump_mode)': exit 0, 3 passed / 5808 skipped. Touched Rust rustfmt and diff checks pass. Initial private-field visibility compiler feedback corrected using a narrow setter; editor test accounts for existing screenshot config normalization without changing production behavior.
- M01 content investigation: generated Markdown/notes engine/query/dashboard keys match actual loader. Tag fixture uses unsupported hyphens (parsed as jump); fix to underscores later. Before adding readiness infrastructure, semantic fixture verify must call production note loader/query and typed slot-config parsing. Empty results cause remains unproven.
## M04/M05 verified integration handoff

- Phase 1 suppresses unsupported surfaces using `any_panel_open()` plus confirmation, Action Sheet, macro pending prompt, and `ParameterPromptUi::owns_input()` (currently QA-gated; expose read-only generally).
- Use root viewport plus `InputState::focused`; `Popup::is_any_open(ctx)` persists across frames, unlike current-pass-only `Context::any_popup_open()`. Previous-frame `top_modal_layer()` and interaction allowance also help filter registration.
- Capture first-pass Jump input after second snippet-prompt ownership recheck/action-sheet blocker and before CentralPanel/query TextEdit. Latch higher-priority ownership across dismissal frame; cancel on hidden-root return and focus loss. Revalidate before applying intents if ownership changes later that frame.
- Root frame diagnostic seam for M14: `root_list_test_app`, `root_list_action`, `run_root_list_frame` in render.rs. Before M05 hooks, record narrow 100-action, 3-column 960x640 CPU baseline with 9 warmups/20 measured frames, no timing assertions. Reuse identical fixture/profile later for disabled/idle/active comparison; no historical broad perf campaign.

## M03 outcome
- Pure src/gui/jump_mode.rs: typed semantic/surface/generation identities, metadata, immutable hint map, explicit matcher/session outcomes and stale-population reconciliation. Shallowest-rightmost leaf expansion preserves short prefix-free uppercase A–Z codes. No action or UI dependency.
- cargo nextest run --lib -E 'test(gui::jump_mode)': exit 0, 10 passed / 5811 filtered. Scoped format/diff clean; vendor warnings only in test compile. One duplicate filtered invocation queued after tool handle loss reused Cargo output; no extra suite run.
- Count / longest / mean hint lengths: 0/0/0; 1/1/1; 2/1/1; 26/1/1; 27/2/1.074074; 52/2/1.538462; 100/2/1.77; 1000/3/2.337.
## M05 input handoff constraints

- M04 queue protocol: after begin_frame, capture input/advance navigation epoch/queue intent BEFORE begin_ui_pass and scope creation. Queueing during/after a pass returns false; no second propagation path. This prevents old scope epochs and dropped same-frame intents.
- Consume keys/text on every UI pass while state transitions happen once. Force-discard tests must prove leader/cancel/resolve events cannot reappear in TextEdit on a second pass. Latch first-pass ownership/capture through modal dismissal and hint resolution.
- Require a fresh post-resolution activation: consume same-frame Enter/Space, and held/repeated leader/completion/activation keys must not leak after exit before release. No arbitrary delays.
- Overlay never owns keyboard focus; cancellation normally preserves original field/caret without an unsafe focus resurrection. Focus loss/hidden root/owner change cancels.
- M05 minimal real GUI screenshot can use main query registration; full dashboard/grid native evidence belongs at M08. Ensure existing QA F11 capture remains usable for active-overlay screenshots (feature-gated QA interception may precede Jump input); no production screenshot shortcut bypass.
- M01 baseline must be completed through semantic fixture validation (production loader and exact queries) then actual readiness/target-state observation, not guessed sleep. Use underscore tags; hyphens collapse to tag `jump` in current parser.
### M05 insertion correction from current-source inspection

Root menus/buttons render before the originally proposed post-snippet/pre-query seam. Vendored egui treats focused Space as a click even with Shift. Therefore eligible leader/session key sanitation must occur in the early visible UI hook before root menu widgets, with existing modal/prompt/OCR/Screen Draw ownership latched and preserved. Later pre-query handling alone is insufficient. Ordinary root Escape hide is after query, so consumed active Escape can prevent it without moving that handler.

Safe actual-root test seam: `forced_discard_dispatches_the_root_action_and_history_once` already uses internal `radial edit`, `test_skip_history_persistence`, `test_activation_trace`, usage counts and forced multipass. Use `query:jump-safe-marker` for a minimal safe eventual-action case; do not deliberately activate `root_list_action()` synthetic app payloads.

## M04 outcome
- Added app-owned egui bridge in jump_mode/ui.rs, copied eligible target metadata/geometry, per-pass collectors, completed-pass retention and exact target/viewport/navigation-epoch focus directives. No callbacks/actions are stored or invoked.
- DashboardContext has a public opaque optional scope, slot-scoped across header/body; all six repository constructors migrated. Configuration generation guards slot reloads. Root hidden/disabled/ownership guards clear navigation. Parameter-prompt ownership accessor is generally available.
- Bridge is the single runtime enabled-state owner; App gate delegates to it. Ambiguous distinct non-root surfaces fail closed; root dashboard/results may coexist. Disabled, hover-only, clipped, blocked and wrong-viewport targets excluded. Queueing after pass start returns false; intent remains single-path/one-shot.
- cargo nextest run --lib -E 'test(jump_mode)': exit 0, 24 passed / 5808 skipped. Scoped rustfmt and diff checks pass, no project/private-interface warnings. Test-fixture mutability/capture, modal rejection classification and selectable-label setup were corrected through focused feedback; no assertions weakened.

## M02-B persistence correction / M05-A baseline
- Parent found src/settings_save.rs define_general_settings_fields! lacks experimental_jump_mode_enabled. Real general save would drop editor toggle even though serialization/mapping/direct runtime tests passed. Reopened M02; single active writer repairing whitelist and focused apply/publication/owned rollback test before input hooks. No broad persistence refactor.
- M05-A pre-hook CPU baseline completed on b151ff00660976aefd4d12b780f9fcefd7aa6dbb, test profile, 100 actions / grid3 / 960x640 / 9 warmups / 20 samples. Raw output: dev/jump_mode/evidence/cpu-baseline/m05-a-root-list-baseline.log. Diagnostic source remains uncommitted for M05-A; M02-B commit will contain only its repair/tests/docs.

- M02-B repair verified: cargo nextest run --lib -E 'test(jump_mode_general_save_acknowledges_and_rolls_back_owned_setting)' exit 0, 1 passed / 5833 skipped. Added experimental flag to GENERAL_SETTINGS_FIELDS, covering candidate apply, committed overlay and owned rollback; unrelated theme preserved. Checkpoint excludes M05-A diagnostic source.

## M01-B bounded investigation
- Read-only planner identified a concrete empty-cache mechanism: fixture inherits reduce_dashboard_work_when_unfocused=true. An unfocused initial frame resets last_refresh without populating pinned results; default interval is 45 seconds. Existing receipt proves later focus only, so exact execution in rejected run remains unproven.
- Pending fixture-only repair: set reduce_dashboard_work_when_unfocused=false; replace hyphenated tags with underscore tags; verify production load_notes/NotePlugin exact queries and typed PinnedQueryResultsConfig.
- Pending readiness proof: narrowly gated QA observation of actual per-slot cached and visible note identities, then existing retained ROOT capture after expected cards render. Loaded files alone are insufficient. No refresh scheduler change, shortened interval, or guessed sleep.
## M05-A scoped review (in progress)
- Read-only review identified modified Enter bypassing generic shortcut handling and reaching root's modifier-agnostic Enter activation. Repair must capture activation keys before shortcut bypass while Jump is active/resolved.
- Review also identified held captured keys surviving owner loss when release occurred in a blocked frame. Repair clears/reconciles guards at the ownership boundary while retaining current-frame capture across discarded passes.
- Both findings assigned to the sole input implementer together with exact regression cases. Neither is considered resolved until scoped checks pass.
## M05-A outcome
- Early visible-root input interception precedes menus/widgets. Once-per-frame transitions plus per-pass capture preserve query/caret across discarded passes; valid second leader/Escape cancel, Backspace edits prefix, letters normalize without Key/Text duplication, and paste/IME stay isolated while active.
- Hint completion queues one focus intent only. Active, completed and cancellation frames suppress same-frame modified/unmodified Enter/Space; held key guards require release, and owner loss clears them. Focus-loss transition captures formerly active input without taking input from focused modal owners.
- Actual query response is the minimal real registered target. Full grid/dashboard controls and painter remain later checkpoints.
- Final cargo nextest run --lib -E 'test(jump_mode)': exit 0, 29 passed / 5809 filtered. Scoped rustfmt and git diff --check pass. Initial test failures were sizing/registry setup plus a genuine focus-loss text leak; repaired at respective owners without relaxing eligibility. Review findings (modified activation, held-key poison, cancellation variant) resolved and covered in final passing filter.
## M05-B painter outcome
- Exact frozen hint IDs resolve against the revalidated current-pass registry. Badges use control layers/clipped geometry and accepted-prefix styling; static active/invalid/no-target feedback does not alter layout or request continuous repaint.
- Real focus plus the exact completed response is required for a destination outline. Painting never requests focus or executes handlers.
- Both visible root exits finish collection, validate/reconcile and paint before QA capture. Shared reconciliation retains the completion callback as final registry owner. Focus ID is read once per paint rather than locking memory per target.
- cargo nextest run --lib -E 'test(jump_mode)': exit 0, 33 passed / 5809 filtered. All four new model tests pass; touched-file rustfmt --check and diff check pass. Native screenshots are not yet claimed.
## M01-B execution scope clarification
Production plugin.rs search_plugin_with_ticket invokes NotePlugin.search synchronously; no missing async publication readiness has been established. First repair the verified fixture scheduling/tag defects and prove production semantic queries, then one actual baseline capture and parent visual-content review. Add further readiness instrumentation only if that run exposes a concrete unmet state. Existing capture alone remains insufficient for content PASS.
## P2 read-only integration handoff
- M06: register actual grid/list Responses before context-menu attachment with TargetKind::Action (Select). TargetId::root_result(result_generation, index) distinguishes duplicate payloads/args without ActionIdLookup. Set existing selected index, retain query keyboard owner/caret, one repaint for selected styling, no activation/deferred action on resolution. Fresh Enter uses existing root handler/activate_action. Capture result scope generation AFTER query processing because results can change during the frame. Existing search/command-host/universal-action invalidation owners advance generation; geometry-only rebuild must not.
- M06 narrow proof: list/grid actual-root hint selection plus later exact-once safe query action, full Action/args identity, stale generation, clipped/virtualized rows, and forced discard. Existing query-only registry-count assumption will need intentional migration.
- M07: reuse slot-scoped DashboardContext jump scope. Small opt-in Response helper requests native focus; original clicked/WidgetAction or direct mutation branches stay owners. Stable native egui IDs are also required for reordered dynamic rows (TodoEntry.id, timer ID, temp path; action/args plus occurrence for query/history rows).
- M07 headers/body keys must differ. Root dashboard idle lacks results-loop unconditional focus_input; avoid broad suppression of explicit query focus. Guard direct Refresh/Copy/Append mutations against repeated discarded passes, while collecting every pass. Native fixtures must never execute destructive/media/clipboard controls.
- M07 exact files: recent_commands, pinned_query_results, query_list (bookmarks equivalent), todo, calendar, tempfiles, active_timers, scratchpad, now_playing, recycle_bin, system_actions/system_status headers. Todo inline complex widgets, Scratchpad multiline, Calendar date/event navigation remain later control coverage unless explicitly integrated.
## M01-B accepted baseline / future test constraint
- Fixed fixture-only focus throttling/tags and validated actual production note loading, exact list/search IDs (32 each), four bucket tags (8 each), typed pinned-query config and complete hotkey route. CtrlAltF10 passes; old F12 co-fire conflict is rejected headlessly.
- Canonical notes AND template paths are strict profile subpaths before any environment update/plugin construction. ML_NOTE_TEMPLATES_DIR is necessary because NotePlugin construction refreshes templates even when disabled; verifier and launched app now use the isolated templates directory.
- Warning run jump_m01_20261019T232004Z_28069510 marked FAIL, original pixels/receipt/lifecycle retained. Fresh jump_m01_20261019T232227Z_fc7bc04b: production semantics PASS, 3 fixture tests PASS, actual 900x560 populated dashboard visually accepted by parent, no warning. PID23360 gracefully exited0, owned_tree_gone, no forced cleanup. Automated runner originally BLOCKED content; explicit baseline_visual_review.json records parent acceptance and scenarios/manifest now PASS.
- Curated fixture-only PNG is docs/plans/jump_mode_review/00_baseline_dashboard.png (171568 bytes); raw logs/evidence remain ignored.
- NEW USER CONSTRAINT: disable Enable Next Shortcut and Enable Cancel Shortcut for all future test profiles. Generator now writes paste_sequence.next_hotkey=None and cancel_hotkey=None; verifier rejects either enabled, README records it. Existing baseline predates this steering and remains immutable. Production defaults unchanged. Fixture-only follow-up jump_m01_20261019T232417Z_6468abe4 PASS: 3 tests, generate/production verify exit0, persisted Next/Cancel null confirmed.