# Surface coverage ledger

Source inventory; this is not completed visual/native qualification. Status.md remains the single execution authority. Shared runtime inheritance does not by itself complete a surface's visual review. Later writers inspect actual current owners; add missing reachable surfaces when discovered. Group checkpoints contain at most three related surfaces and preserve all behavior/domain-specific content.

M5-B groups: B1 Quick Notes/note panel; B2 Clipboard/Modify; B3 MKMacro; B4 MultiManager; B5 File Search/Preview; B6 Radial editor chrome; B7 Mouse settings/gesture forms; B8 Regex surrounding controls. M5-C groups action authoring, snippets/favorites, aliases/bookmarks/files, ordinary utilities, task/time, messages/data, Diff and image-tool chrome. Settings/theme editor are M5-A. All local visual work remains pending unless explicitly marked below.

| Presentation owner | Group | Status / evidence |
|---|---|---|
| `src/gui/render.rs` | M4-A/B | M4-A/B accepted; Dark/Light852x460 ROOT framebuffer inspected, receipts/m4b-root-visual-review.md; physical focus/IME/DPI gaps remain |
| `src/actions_editor.rs` | M5-C1 | pending App Editor presentation; src/gui/actions.rs domain dispatch remains unchanged |
| `src/gui/add_action_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/add_bookmark_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/alias_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/bookmark_alias_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/brightness_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/calendar_event_details.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/calendar_event_editor.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/calendar_popover.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/clipboard_dialog.rs` | M5-B2 | accepted semantic chrome/bounded previews/actions;15scoped contracts across B2 chain; populated History002 framebuffer inspected; native input/close gaps explicit |
| `src/gui/clipboard_modify_dialog.rs` | M5-B2 | accepted semantic sections/status/footer/native constrained scrolling; scoped layout/state/persistence contracts and review pass; native visual/input coverage unobserved |
| `src/gui/confirmation_modal.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/convert_panel.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/cpu_list_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/crop_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/dashboard_editor_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/data_recovery_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/diff_dialog/mod.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/fav_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/file_search_dialog.rs` | M5-B5 | accepted wrapped chrome/font-aware highlight/table metrics;14distinctcontracts/review pass; initialDark002 framebuffer inspected, populatednative/input gaps explicit |
| `src/gui/file_search_preview_dialog.rs` | M5-B5 | accepted wrapped header/status/measured native rows; scoped line-map/payload/row-stride contracts pass; native selected-preview capture/input gap explicit |
| `src/gui/image_crop_editor.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/image_panel.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/json_utility_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/macro_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/mkmacro_dialog/mod.rs` | M5-B3 | main toolbar/properties/sidebar/diagnostics accepted;9contracts and initial main framebuffer pass; child chrome M5-C pending and native interaction gaps explicit |
| `src/gui/mouse_gesture_settings_dialog.rs` | M5-B7 | pending surface polish/review; runtime inheritance available |
| `src/gui/mouse_gestures_dialog.rs` | M5-B7 | pending surface polish/review; runtime inheritance available |
| `src/gui/mouse_settings_dialog.rs` | M5-B7 | pending surface polish/review; runtime inheritance available |
| `src/gui/multi_manager_actions.rs` | M5-B4 | inspected behavior owner unchanged; presentation remains src/multi_manager/ui.rs |
| `src/gui/note_graph_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/note_panel.rs` | M5-B1 | implemented/source reviewed; strict window/save/discard/narrow tests pass; native after-image gap |
| `src/gui/notes_dialog.rs` | M5-B1 | implemented/source reviewed;13 targeted contracts plus Notes002 app-native after-image atda0ca08c; receipts/m5b1-notes-presentation.md |
| `src/gui/qr_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/radial_actions.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/radial_editor/mod.rs` | M5-B6 | accepted surrounding chrome/shared native actions,7targetedcontracts/review pass; authoredcanvas/skins unchanged; deferrednativeDesigner visual/input gap explicit |
| `src/gui/regex_tester_dialog.rs` | M5-B8 | pending surface polish/review; runtime inheritance available |
| `src/gui/screen_draw_toolbar.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/screenshot_editor.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/shell_cmd_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/snippet_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/snippet_prompt_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/tempfile_alias_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/tempfile_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/theme_settings_dialog.rs` | M3-C/M5-A | M5-A complete at94b4724d;21contracts and Dark/Light852x460 rendered footer reviewed; physical native gaps remain |
| `src/gui/timer_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/toast_log_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/todo_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/todo_view_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/unused_assets_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/volume_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/help_window.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/multi_manager/ui.rs` | M5-B4 | accepted shared main/settings chrome;9targeted contracts/review pass; Dark main/Light Settings initial framebuffers inspected, native input/scroll/close gaps explicit |
| `src/settings_editor/render.rs` | M5-A | complete at94b4724d; section identity/collapse/footer and mapping tests, Dark852x460 rendered review |

Plugin settings_ui owners also require local presentation review alongside their surface group: browser_tabs, network, history, mkmacro, mouse_gestures, file_search, omni_search, virtual_desktop, note, diff, clipboard_modify, color_picker, screenshot, shell, clipboard and stopwatch. Preserve mappings/ranges and native control behavior.

Intentional domain ownership: native radial compositor/skins; Screen Draw canvas; coordinate effects; sampling/capture overlays; image pixels/selection/annotations; regex source/capture highlights; diff semantics; Markdown syntax/callouts/code/images. Theme surrounding chrome only, never globally recolor these meaningful outputs or alter transparency/focus/hook transport. QR/Regex immediate viewports, Radial deferred viewport, macro prompts, Screen Draw toolbar and radial-placement recovery must inherit Context changes without parent geometry reset. Native capture/input/DPI qualification remains separate.

## M5-B1 implementer handoff — after M4-C/M5-A

Owner notes_dialog.rs + note_panel.rs; existing shared helpers/runtime only. Standardize browse/edit/template headings, search/form spacing, compact toolbar, metadata/backlinks and confirmation chrome. Keep text roles aligned with measure_notes_geometry, controls/footer reachable and constrained-width wrapping without enlarging windows. Map Name-required and missing/ambiguous wiki-link warnings to semantic statuses with explicit text/meaning. Preserve ordinary hyperlink behavior. Leave Markdown/code/tasks/images/custom font size with existing renderers.

Invariants: slug-based window/text/scroll IDs, menu-owner retention, query projection/NotesGeometry keys/virtualization, split/outline allocation, save/discard/external-open and analysis reuse. No storage/parsing/mutation/cache-policy/native changes. Reuse actual landed helpers, no note-specific theme state.

Target quick_notes_browsing_virtualizes_rows_and_reuses_geometry; quick_notes_geometry_follows_consumer_font_activation; markdown_title_change_keeps_note_window_rect; split_view_allows_narrow_dialog_without_forcing_pane_minimums; discard_prompt_actions_have_explicit_transitions; markdown_fragments_keep_distinct_stable_ids_and_prepare_shared_cache. Add only missing changed footer/toolbar reachability protection. Locked/offline fast-dev targeted Nextest, explicit FullOutput disposal, scoped fmt/diff/review, appropriate compile. No benchmark/full suite/native campaign. Record actual visual inspection/manual gaps. Complete when both surfaces use shared chrome/status and editing/geometry contracts pass.

Recovery-surface follow-up: retained invalid-fixture Dark001 defaults400x220 capture crowds recovery banner/header. Inspect constrained fallback recovery layout during planned M5-C; data-preservation rejection behaved correctly.

### Theme editor pre-shell observation

Existing immutable M4-B QA binary C95E, isolated run-m4b-theme-before-001, ROOT852x460 capture saved and visually inspected. Color sections extend below visible bounds and Apply/Save/Cancel footer is not visible. M5-A already owns adaptive scrolling body and reserved footer correction; this is retained adverse evidence, not final acceptance. Capture: target/egui-modernization/native-themed/run-m4b-theme-before-001/framebuffer-output/framebuffer.png with source/build/process receipt. Fixture PID31952 exited after normal Alt-F4; no source or settings save action performed. Native screenshot/focus coverage remains distinct from framebuffer pixels.

### General Settings and Quick Notes pre-shell observations

Same immutable M4-B C95E QA binary, separate isolated startup pin fixtures, ROOT852x460 saved receipts with no failure; processes35324/26904 closed normally and exit confirmed. General Settings default640x320 shell has reachable Save and scrolling sections; native accessibility lists all seven sections/control states. Preserve default sizing while improving actual metric reservation/alignment in M5-A. Quick Notes default narrow shell has reachable toolbar/Search, but one-line title/slug/backlink metadata clips at its right boundary; M5-B1 owns readable narrow hierarchy with matching measurement/paint geometry. No Save, note mutation, or native click action was performed. These are pre-change framebuffer/accessibility observations, not final visual/native acceptance.

- Settings: target/egui-modernization/native-themed/run-m4b-settings-before-001/framebuffer-output/framebuffer.png, SHA256417cd0550cac69c25e81566d0da19776695a35364c0db2b963dd42a3245a837e.
- Notes: target/egui-modernization/native-themed/run-m4b-notes-before-001/framebuffer-output/framebuffer.png, SHA2568efe1102331e3deed556dc11f88ced2d589fc7fdb8ee8afcb135920fd5edc310.

### M5-A accepted rendered states

Settings/theme-editor local shell work complete with21 targeted contracts, reviewed source and Dark/Light theme+DarkSettings852x460 after-images; see receipts/m5a-settings-theme-shells.md for source/build/capture identities. Theme footer nowvisible comparedwithretainedpre-shellclipping. Physicalfocus/hotkey/DPI remains separate QA gap. Notes/plugin work remains pending.

### M5-B1 implementation and qualification

Notes chrome/header/edit-footer implementation and shared ThemeRuntime scroll-padding ownership correction accepted by source review;13distinct targeted contracts pass004/005chain. At checkpoint commit the native after-image was unobserved due Escape/approval restriction; later app-native follow-up below supersedes that visual gap. Originalfixture27204 was retired; no normal UI-close proof. See receipts/m5b1-notes-presentation.md. Continue safe B2 source work; retain visual/manual follow-up.

### Notes app-native visual follow-up

At da0ca08c, immutable65EE QA binary produced Notes002 and Theme001852x460 images without Computer Use; inspected readable stacked metadata and reachablethemefooter. Original interruption retained separately. See receipts/m5b1-notes-presentation.md. Native focus/hotkey/normal-close gaps remain; all fixture processes retired after saved receipts. User permanently forbids Computer Use for this goal.

M5-B2 also updates Clipboard/Modify settings_ui labeled rows with unchanged ranges/serialization. App-native History001 exposed the pinned load bypass; transition-only State::ensure_open and LauncherApp delegation fixed it, repeated drafts/pinned close retained. History002 confirms populated saved entries; raw001 and all earlier failures preserved. See receipts/m5b2-clipboard-presentation.md. No Computer Use.

M5-B3 covers main only. Remaining MKMacro presentation review in M5-C includes action/condition/call editors, action/launcher/window pickers, parameter prompts/package/recording review, image/OCR surrounding controls and runtime-inspector/outline chrome; authored images/canvas/native overlays remain domain exceptions. Review scaled table row fit (current body row22) and pinned-panel menu display labels (currently enum Debug names) during that existing chrome pass; these are not completed coverage or performance-backlog work.
