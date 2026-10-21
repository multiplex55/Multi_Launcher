# Jump Mode expanded coverage execution

Authority: `multi_launcher_jump_mode_full_coverage_plan.md`, current `AGENTS.md`, and the active kickoff attachment. This compact ledger replaces chronological working notes; checkpoint history remains in Git.

Started 2026-10-20 from clean selected branch `jump`, base `0cb3dabf148dc720ed560626c32ef596b5e8ff9a`. Continue that branch and the existing Jump implementation. Parent owns Git/ledger; configured implementer is the sole source writer; configured planner/reviewer supplied bounded handoffs and source reviews.

## Status

| Milestone | Status | Outcome |
|---|---|---|
| M0 | complete | Read plan/instructions/roles and inspected current checkout. |
| M1 | complete | Typed owners/lifetimes, shared adapters and safe fixture bridge; 32 owner and 5 adapter tests passed; independent review accepted. |
| M2 File Search | complete | Form/results/lifecycle and exact popup correction; eight focused cases and fixture validation passed. Native workflow unavailable. |
| M3 Snippets | complete | Rows, revealed editors and exclusive removal/prompt/preview; focused lifecycle/privacy evidence passed. |
| M4 Clipboard | complete | Opaque rows, editor, explicit copy and checked atomic persistence/removal; focused cases passed. |
| M5 Dashboard | complete | Pinned/history and simple widget gaps; actual composition/reorder evidence passed. |
| M6 JSON/Regex/XML | complete | Actual forms/rows, native viewport binding and child/popup ownership; D had eight unique passes plus four adjacent owner passes. |
| M7 Mouse/Gestures | verification_gap | Implementation and review complete; one extra Jump-off Cancel pointer fixture remains stopped. |
| M8 Rename/Replace | verification_gap | Implementation and review complete; Rename Apply-target fixture remains stopped; actual-app foreign-foreground evidence is source-only. |
| M9 Designers | verification_gap | Implementation and review complete; A/B five unique passes each, C eight, D three; B retains unrelated error-baseline fixture failure. |
| M10 | complete | Exactly two curated selections passed 12/12 + 10/10; docs/evidence review clean; existing fixture-only run passed 7/7. |
| M11-A | verification_gap | Original full run failed (6588 passed, 20 failed, 48 skipped). All 12 task-related newly exposed cases now pass after focused correction; three exhausted gaps and five baseline findings remain. |
| M11-B | in_progress | Final classification and source diff inspected; repair commit and handoff ledger being finalized. |
| M11-C | pending | Clippy last, after normal source/docs/format/tests/commits. |

A verification_gap is not a claim that the failed fixture passed, nor proof of a product defect. Exact limitations below remain part of the handoff.

## Architectural boundaries

- Reuse `JumpModeUi`, `JumpModeScope`, the shared registry/focus adapters and typed `NavigationOwner`/`NavigationWindows`; no replacement hint engine, extra hook or ambient context manager.
- Register actual currently visible, enabled, clipped-positive Responses. Unknown foreground layers, inactive/wrong viewports, stale rows, pending pickers and capture/recording fail closed. Per-open child lifetimes retire old authority immediately.
- Hint completion focuses/selects only. A later fresh Enter/Space/typing/native action activates; held/opening-frame input and owner-transition passes are discarded. Native admission remains independent of default-off Jump registration.
- Exact Combo anchor/native Response/layer/viewport provenance is shared; semantic anchor and popup namespaces match. Legitimate sizing evidence is separate from strict Jump eligibility.
- Native Regex/XML and deferred Radial use existing engines bound to their actual viewports. ROOT cannot reset/complete a child session. Radial retains its existing single shared lock and explicit callback-local UI-pass plumbing.
- Stable semantic/native identities use opaque session/entity/row authority, not payload or positional index. Existing common generation reconciliation preserves active text/slider focus when identity remains valid.
- Clipboard/snippet/file/source/query payloads stay out of hint IDs, badges and QA metadata. Existing explicit file plans, receipts and confirmation execution remain authoritative.
- No production default, persisted schema, dependency, AGENTS or configured-role changes. No synthesized canvas dragging or native picker automation.

## All eight areas: implemented controls and individual limits

| Area | Implemented visible controls | Exclusions / evidence boundary |
|---|---|---|
| File Search | Query/root/refinement; Filename/Content, modes/filters/sort/folds; Search/Cancel; actual filename and content-match rows; enabled Open/Copy/diagnostics. Opaque search/view identity survives reorder. | Export menu options retain native menu access; native picker internals, passive headings and truncation notices have no hints. Native workflow denied before interaction. |
| Snippets | Saved rows/filter/alias/tags; revealed body and prompt-field forms; Add/Save/Cancel/Test/Preview; exclusive removal, exact prompt and Preview Return. | Concealed body/defaults, passive preview and sequence-preflight Jump targets excluded. Hint does not copy or submit. |
| Clipboard History | Opaque selection/Edit/Copy/Remove/Close; multiline editor/Save/Cancel; exclusive fresh Confirm/Cancel; checked atomic snapshot persistence. | Payload never enters target metadata. Context-menu contents remain native. Tests use fake clipboard, not system writes. |
| Dashboard gaps | Actual List/Grid Pinned Commands; Command History filter/pin/action/missing Unpin; Todo status/tag/priority/done; Scratchpad multiline; Calendar month/date/event responses. | Reuse already-covered controls. Passive summaries, picker/event-editor foreign owners excluded. No runtime schema changes. |
| JSON/Regex/XML | Source/pattern/search fields, flags/tabs/tree/result/case/preset/history/reference/replacement/XPath/schema forms; enabled Copy/Save/Close and actual navigation; exact native/embedded viewport return, Combo and XML overwrite child. | Passive diagnostics/output and native picker/titlebar-only controls have no fabricated targets. Registration inventory does not prove every conditional branch interactively. |
| Mouse Settings/Gestures | Actual switches/sliders/appearance/reset/Apply/Cancel and exact selectors; gesture/rename/delete/record-management/binding forms; separate live Gesture Settings with Save and ignored-title rows/picker anchor. | Active local/global recording, coordinate capture, path/canvas drags, color-picker and OS-picker internals excluded. Live-versus-Save behavior preserved. Extra off-state Cancel fixture gap below. |
| Batch Rename/Search & Replace | Rename rules/form, visible Include/Select rows, all five sort headers, safety/recovery folds, Preview/Cancel/Apply/receipt Undo; Replace search/filter/sort/inspection, file/match approvals/diff/receipt/recovery and typed Apply/Restore child Confirm/Cancel. | Existing worker/frozen-plan/receipt authority unchanged; no actual destructive execution in new UI fixtures. Native picker/passive file text excluded. Rename Apply-target fixture and actual-app foreground limits below. |
| Dashboard Editor/Radial Designer | Dashboard grid/options/slot selection/ID/widget/swap/split/remove/reload/save/reset; all 36 actual property renderers receive optional current context and real Responses. Radial tree/search/toolbar/main inspectors, exact staged Properties/Add/dirty-close/delete/resize children; inline ActionBinding fields/results/Test/staged actions and Appearance gallery/forms. | Canvas dragging, hover/pointer geometry, embedded Radial fallback and unsupported native/foreign/color-popup internals excluded. New Radial drivers call retained viewport_ui; actual existing deferred-close fixture separately runs callback. Neither proves OS focus. |

The 36 Dashboard property renderers are weather_site, plugin_home, command_history, recent_commands, frequent_commands, gesture_cheat_sheet, gesture_recent, todo, layouts, recent_notes, pinned_commands, pinned_query_results, query_list, active_timers, clipboard_snippets, clipboard_recent, browser_tabs, calendar, process_list, window_list, windows_overview, system_actions, system_controls, system_status, now_playing, snippets_favorites, scratchpad, notes_recent, notes_tags, notes_graph, todo_focus, quick_tools, recycle_bin, tempfiles, volume and stopwatch. Diagnostics/context_links/gesture_health have no actual property renderer. This is a source inventory, not universal interaction proof.

## Retained bounded verification gaps

Each fixture remains meaningful and unchanged after the permitted two substantive repairs and exact reruns. No test was deleted, ignored or weakened to obtain green results. Do not deliberately repair/rerun these again. These retained failures first arose in QA-feature runs; the mandatory default-feature full run also included and reproduced all three. Feature scope and actual results are reported separately.

1. `jump_mode_mouse_settings_native_click_remains_available_when_jump_is_disabled`: empty registry, current Window, enabled checkbox pointer toggle and unchanged committed settings are proved. The fixture then resolves its Cancel fullrect center to Background after scrolling, before clicking Cancel. Jump-off Cancel remains unverified; no product defect established. Separate Jump-enabled Cancel/popup cases pass.
2. `jump_mode_batch_rename_confirmation_apply_hint_requires_fresh_cancel`: correct owner, accepted completed scan, Ready preview/journal and no operations are established, but current Apply target is missing before hint completion. Its actual enabled/clipped Apply Response was not captured. Held-opening Enter/child Cancel are not reached; do not assert clipping caused the failure. Real Apply/Undo/Restore were not executed. Other row/native-ID/frozen-plan and Undo/Replace-child Cancel cases pass. Another recognized foreground while Rename child is pending is source-reviewed only, without actual LauncherApp integration proof.
3. `jump_mode_radial_tree_foreign_foreground_focus_blocks_native_shortcuts`: actual foreign Foreground focus is established, but last_error baseline was captured before warmup. First focus frame requests an unloaded font catalog with client=None and records ServiceClosed before Ctrl+S. Source review confirms foreign focus makes native admission false and skips shortcut dispatch; this failure is not evidence of Save leakage. The pending_request assertion is not reached. Separate M9-D settled-catalog unknown-foreground case passes without erasing this retained failure.

## Verification policy and results

Implementation used one focused QA-feature Nextest slice per checkpoint, at most one justified adjacent shared-owner slice, with exact bounded repair reruns. No full suite or Clippy during M0-M9. All new application fixtures use disposable isolated workspaces, inert plugin configuration and Paste Sequence next_hotkey=None/cancel_hotkey=None before application construction; production defaults remain unchanged. Actual child-viewport observations are captured within the child pass; FullOutput is consumed before assertions. No blanket error muting substitutes for safe setup.

M10 used exactly two selections with `cargo nextest run --lib --features qa-framebuffer-capture -E ...`:

- **12 passed, 6017 filtered**: actual expanded root ownership; File Search form; Snippets privacy/prompt; Clipboard fresh removal; opaque Pinned rows; JSON/Regex/XML viewport return; Gesture binding/popup; Rename sort; Replace approval child; all three designer ownership cases.
- **10 passed, 6019 filtered**: actual query capture/discard; root list/grid full action; exact Combo later activation; transition discarded passes; disabled/owner-loss behavior; existing Notes, Todo, Timer, Paste Sequence and MkMacro identity/activation.

The exact two M10 filter expressions were:

```text
# Selection 1
cargo nextest run --lib --features qa-framebuffer-capture -E 'test(actual_root_navigation_owner_follows_expanded_dialogs) | test(jump_mode_file_search_form_focus_types_then_enter_starts_one_search) | test(jump_mode_snippets_privacy_pending_removal_and_prompt_have_exact_owners) | test(jump_mode_clipboard_actions_removal_has_an_exclusive_fresh_confirmation) | test(jump_mode_dashboard_commands_pinned_rows_are_opaque_stable_and_focus_only) | test(jump_mode_tool_viewports_root_json_regex_xml_return_isolate_child_state) | test(jump_mode_mouse_gestures_binding_child_and_category_popup_are_exact) | test(jump_mode_file_operations_integration_sort_headers_are_focus_only_and_current) | test(jump_mode_replace_approvals_focus_only_then_open_an_exclusive_cancel_child) | test(jump_mode_designer_ownership)'
# Selection 2
cargo nextest run --lib --features qa-framebuffer-capture -E 'test(jump_mode_actual_query_capture_preserves_text_and_survives_discarded_pass) | test(jump_mode_root_result_hints_select_without_activation_in_list_and_grid_then_fresh_enter_activates_full_action) | test(combo_popup_uses_exact_foreground_owner_and_later_native_option_activation) | test(owner_transition_blocks_all_discarded_passes_then_stabilizes) | test(jump_mode_disabled_and_owner_loss_preserve_normal_input_ownership) | test(jump_mode_notes_dialog_escape_preserves_draft_and_fresh_enter_cancels_edit) | test(jump_mode_todo_dialog_add_hint_then_fresh_enter_adds_once_and_escape_preserves_draft) | test(jump_mode_timer_dialog_cancel_focuses_then_uses_the_existing_button) | test(jump_mode_paste_sequence_selects_saved_row_and_keeps_nested_confirmation_safe) | test(jump_mode_mkmacro_macro_and_step_targets_keep_native_identity_after_reorder)'
```
The three stopped fixtures were deliberately absent from those selections. Source review found no new tasks/hooks or registration-driven repaint loop; registration follows visible Responses. This is not a CPU benchmark.

Existing `pwsh -NoProfile -ExecutionPolicy Bypass -File dev/jump_mode/run.ps1 -FixturesOnly` passed **7/7 fixture unit tests plus generation/typed verifier**, run [jump_m01_baseline_20261021T183503Z_4f830f92](../../dev/jump_mode/evidence/jump_m01_baseline_20261021T183503Z_4f830f92/summary.md). Parent inspected summary/manifest: PASS, complete, cleanup.method="not launched", PID null, post_cleanup_state=not_started. GUI remains blocked because this invocation requested no launch; it is not eight-area interactive proof.

Native budget used **2/12 sessions, 0/24 images**. [Diagnostic](../../dev/jump_mode/evidence/jump_m13_phase2_file-search_20261020T225218Z_f11a5e7f/summary.md) and [retry](../../dev/jump_mode/evidence/jump_m13_phase2_file-search_20261020T225308Z_c2d414de/summary.md) failed at host process inventory Access denied before workflow interaction. Both owned PIDs were later absent; receipts accurately retain cleanup_uncertain. Later native area outcomes are not_run_environment. No further launch, fabricated screenshot or OS-focus claim.

M11-A:

- Explicit task-changed Rust-file formatting check passed for **80 files**, skip_children=true. Only `src/gui/jump_mode/ui.rs` required task-local formatting. Baseline `src/bin/radial_acceptance/native.rs`, `src/lib.rs`, `src/paste_sequence/playback.rs` and vendor source remain untouched.
- **`cargo check --bin multi_launcher` passed**, default dev profile, 1m22s.
- **`cargo nextest run --workspace --no-fail-fast` failed**, run 6e498ca9-7f74-40ea-a8ef-caf098b3dc45: 6608 selected across 71 binaries, **6588 passed, 20 failed, 48 skipped**, exit 1. Default features; no QA/all-features/dev/perf flags. Compilation/linking took 49m03s. The final hotfolder test was forcibly aborted after 279.274s following source-confirmed unchanged baseline deadlock, exact owned test-path/PID/start-time/session verification; this was not natural completion. No Cargo/Nextest or unrelated process was killed. No full repaired rerun has been started.

The 20 failures comprised: all three retained gaps; Clipboard browse; ambiguous nested-popup expectation; two TreeSearch undo fixtures; eight Radial Properties/Inspector interaction cases; Paste three-step acceptance; Hotfolder capability; Recovery canonical reset; XML gate; and the aborted Hotfolder lease test.

Detailed original output was tool-truncated and no retained Nextest/JUnit session artifact existed. One necessary **15-case default-feature lib diagnostic selection** reproduced all unresolved failures (0 passed/15 failed, exit 100), excluding the three exhausted fixtures and source-confirmed XML/hung-test baselines. Complete diagnostics are in `target/productivity_expansion/m11_unresolved_failures.log` (generated local artifact, not committed evidence).

Source-confirmed baseline findings, unchanged from starting HEAD:

- XML gate expects SchemaPolicy while engine intentionally rejects document() as XPath; external-resource rejection remains intact. Earlier c885f881f changed the error variant without migrating integration expectation.
- Hotfolder lease fixture installs an exited fake worker but holds its channel receiver, then finish_lease queues a reply-bearing request and waits forever. No consumer can reply or disconnect. Only the exact owned final test process was terminated to let the original suite finish.
- Hotfolder capability fixture supplies only watched-root availability, omits Copy/Move destination availability and expects Supported despite explicit Unavailable validation.
- Paste three-step acceptance expects progress after final settlement, but unchanged playback explicitly clears its session on Completed. The fixture incorrectly expects Some((3,3)) instead of None after final settlement; direct controller coverage already expects no session. No prompt-rendering path is involved.
- Recovery test asks canonical_reset for HotFolderRules although no reset form exists; catalog/reset contract is inconsistent, and later non-resettable journals also lack forms. Do not conceal this by skipping eligible stores.

Task-related repair triage is in progress. Confirmed test migrations: Clipboard's blanket no-copy label assertion predates its explicit safe Copy action; nested popups now win by exact depth while same-depth ambiguity remains blocked; old timed TreeSearch driver still supplies ROOT viewport input. Reviewer also found a real Properties ordering defect: explicit binding intents mutate a local draft after its earlier snapshot was persisted, losing Pin/Save/Set changes; preserve behavioral assertions and correct intent/state commit order at the owner boundary.
M11-A first repair checkpoint: the four confirmed fixture migrations passed **4/4 default-feature lib cases**, run a6f7df0d-2e64-40da-8117-addc6ecf6215, 5984 filtered, exit 0. Nested depth precedence plus same-depth exclusion, both actual child-viewport TreeSearch undo cases, and explicit safe opaque Copy controls retain behavioral/privacy assertions. Scoped formatting/diff checks passed. Full log: target/productivity_expansion/m11_repair_known_fixtures.log. Properties production ordering and eight related interaction cases were handled by the subsequent checkpoint; no second full run.

M11-A2: reviewer accepted the production ordering correction at the existing owner-authorized intent boundary. Pin/Save query/Set command now reduce the local Properties draft before its single visible-pass writeback; disabled shadow rollback and all binding/pending assertions remain. Retained helper observations use current actual child/parent ownership and geometry; Skins observes clicks inside the same timed 1000x1800 child input path with Jump off. Primary exact-eight default lib selection passed seven; exact Skins rerun **7740bdba-910f-452b-b957-ab4d0411463d passed 1/1 (5987 filtered)** after recollecting the actual enabled Menus control after Cancel. Eight unique cases pass, plus four prior fixture repairs = **12 corrected cases verified**. Scoped rustfmt/diff checks passed. Logs: target/productivity_expansion/m11_radial_repair.log and its exact-rerun output. Generated root clipboard_modifiers.json matched default IDs/schema and was absent initially; only that test artifact was removed. No production defaults or unrelated baseline fixtures were changed.

The five baseline findings and three exhausted fixture gaps remain recorded; no full repaired rerun is needed to re-establish those known failures or reproduce the deterministic hang. The twelve directly affected corrected cases received focused verification; the original full run is not relabeled green.

M11-C remains pending. Finish ordinary source/docs/format/tests/commits first, then `cargo clippy --workspace --all-targets -- -D warnings` on maintained default targets. Record actual failures, including known unchanged Pinboard never_loop baseline findings, rather than cleaning unrelated source or blanket allowing lints. After the last lint result, no more source/docs/format/tests; report late lint outcome directly in final handoff.

## Checkpoint commit index

All listed commits were retained locally and successfully pushed normally to `origin/jump`. No branch switch, reset, rebase, force-push or imported historical source.

| Checkpoint | Commits |
|---|---|
| M0/M1 | `520bd40c`, `3c3b8e91`, `8648b2d0`, `a1181df6` |
| M2 | `b91740f3`, `5ffdf9a5`, `251f27e5`; popup correction `61ac46c9` |
| M3 | `f4ad295e`, `788cf705`, `c5b32aef` |
| M4 | `84a78537`, `68b9838b`, `5443c706` |
| M5 | `d0f27e1f`, `4e488312`, `f2573f76` |
| M6 | `84db28d1`, bounded-investigation docs `848aa8bb`, `0ea42baf`, `91c2660c`, `5f3985f4` |
| M7 | `6979475b`, `acd39ed8`, review correction `784b1d27`, `4792f526`; collapsed-close correction `5d51d322` |
| M8 | `2df67bc9`, `6160e96f`, `fc4a287b`, `80b70a8f`, `69405cc6` |
| M9 | `7b060b2c`, `c7cd3d52`, `09ed8de3`, `1d7c1010` |
| M10 | `e643c0f0`, `04f272ad` |
| M11-A fixture repair | `ab32aa62`; Properties repair pending commit |

No universal coverage or all-green claim. Final checkpoint state must reflect the actual full-suite/lint outcome and the explicit limits above.
