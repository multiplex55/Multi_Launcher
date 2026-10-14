# Surface coverage ledger

Source inventory; this is not completed visual/native qualification. Status.md remains the single execution authority. Shared runtime inheritance does not by itself complete a surface's visual review. Later writers inspect actual current owners; add missing reachable surfaces when discovered. Group checkpoints contain at most three related surfaces and preserve all behavior/domain-specific content.

M5-B groups: B1 Quick Notes/note panel; B2 Clipboard/Modify; B3 MKMacro; B4 MultiManager; B5 File Search/Preview; B6 Radial editor chrome; B7 Mouse settings/gesture forms; B8 Regex surrounding controls. M5-C groups action authoring, snippets/favorites, aliases/bookmarks/files, ordinary utilities, task/time, messages/data, Diff and image-tool chrome. Settings/theme editor are M5-A. All local visual work remains pending unless explicitly marked below.

| Presentation owner | Group | Status / evidence |
|---|---|---|
| `src/gui/actions.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/add_action_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/add_bookmark_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/alias_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/bookmark_alias_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/brightness_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/calendar_event_details.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/calendar_event_editor.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/calendar_popover.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/clipboard_dialog.rs` | M5-B2 | pending surface polish/review; runtime inheritance available |
| `src/gui/clipboard_modify_dialog.rs` | M5-B2 | pending surface polish/review; runtime inheritance available |
| `src/gui/confirmation_modal.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/convert_panel.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/cpu_list_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/crop_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/dashboard_editor_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/data_recovery_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/diff_dialog/mod.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/fav_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/file_search_dialog.rs` | M5-B5 | pending surface polish/review; runtime inheritance available |
| `src/gui/file_search_preview_dialog.rs` | M5-B5 | pending surface polish/review; runtime inheritance available |
| `src/gui/image_crop_editor.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/image_panel.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/json_utility_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/macro_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/mkmacro_dialog/mod.rs` | M5-B3 | pending surface polish/review; runtime inheritance available |
| `src/gui/mouse_gesture_settings_dialog.rs` | M5-B7 | pending surface polish/review; runtime inheritance available |
| `src/gui/mouse_gestures_dialog.rs` | M5-B7 | pending surface polish/review; runtime inheritance available |
| `src/gui/mouse_settings_dialog.rs` | M5-B7 | pending surface polish/review; runtime inheritance available |
| `src/gui/multi_manager_actions.rs` | M5-B4 | pending surface polish/review; runtime inheritance available |
| `src/gui/note_graph_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/note_panel.rs` | M5-B1 | pending surface polish/review; runtime inheritance available |
| `src/gui/notes_dialog.rs` | M5-B1 | pending surface polish/review; runtime inheritance available |
| `src/gui/qr_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/radial_actions.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/radial_editor/mod.rs` | M5-B6 | pending surface polish/review; runtime inheritance available |
| `src/gui/regex_tester_dialog.rs` | M5-B8 | pending surface polish/review; runtime inheritance available |
| `src/gui/screen_draw_toolbar.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/screenshot_editor.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/shell_cmd_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/snippet_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/snippet_prompt_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/tempfile_alias_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/tempfile_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/theme_settings_dialog.rs` | M3-C/M5-A | pending surface polish/review; runtime inheritance available |
| `src/gui/timer_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/toast_log_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/todo_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/todo_view_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/unused_assets_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/gui/volume_dialog.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/help_window.rs` | M5-C | pending surface polish/review; runtime inheritance available |
| `src/multi_manager/ui.rs` | M5-B4 | pending surface polish/review; runtime inheritance available |
| `src/settings_editor/render.rs` | M5-A | pending surface polish/review; runtime inheritance available |

Plugin settings_ui owners also require local presentation review alongside their surface group: browser_tabs, network, history, mkmacro, mouse_gestures, file_search, omni_search, virtual_desktop, note, diff, clipboard_modify, color_picker, screenshot, shell, clipboard and stopwatch. Preserve mappings/ranges and native control behavior.

Intentional domain ownership: native radial compositor/skins; Screen Draw canvas; coordinate effects; sampling/capture overlays; image pixels/selection/annotations; regex source/capture highlights; diff semantics; Markdown syntax/callouts/code/images. Theme surrounding chrome only, never globally recolor these meaningful outputs or alter transparency/focus/hook transport. QR/Regex immediate viewports, Radial deferred viewport, macro prompts, Screen Draw toolbar and radial-placement recovery must inherit Context changes without parent geometry reset. Native capture/input/DPI qualification remains separate.

## M5-B1 implementer handoff — after M4-C/M5-A

Owner notes_dialog.rs + note_panel.rs; existing shared helpers/runtime only. Standardize browse/edit/template headings, search/form spacing, compact toolbar, metadata/backlinks and confirmation chrome. Keep text roles aligned with measure_notes_geometry, controls/footer reachable and constrained-width wrapping without enlarging windows. Map Name-required and missing/ambiguous wiki-link warnings to semantic statuses with explicit text/meaning. Preserve ordinary hyperlink behavior. Leave Markdown/code/tasks/images/custom font size with existing renderers.

Invariants: slug-based window/text/scroll IDs, menu-owner retention, query projection/NotesGeometry keys/virtualization, split/outline allocation, save/discard/external-open and analysis reuse. No storage/parsing/mutation/cache-policy/native changes. Reuse actual landed helpers, no note-specific theme state.

Target quick_notes_browsing_virtualizes_rows_and_reuses_geometry; quick_notes_geometry_follows_consumer_font_activation; markdown_title_change_keeps_note_window_rect; split_view_allows_narrow_dialog_without_forcing_pane_minimums; discard_prompt_actions_have_explicit_transitions; markdown_fragments_keep_distinct_stable_ids_and_prepare_shared_cache. Add only missing changed footer/toolbar reachability protection. Locked/offline fast-dev targeted Nextest, explicit FullOutput disposal, scoped fmt/diff/review, appropriate compile. No benchmark/full suite/native campaign. Record actual visual inspection/manual gaps. Complete when both surfaces use shared chrome/status and editing/geometry contracts pass.
