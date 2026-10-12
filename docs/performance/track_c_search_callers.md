# C2-A production search caller inventory

Read-only planner c0_recon verified this inventory at C2-A base6efe54007057edc5172e38886b3602c46f40bdef/current working-tree inspection. Lines are navigation hints and may shift under test-only observation/fixture edits. No source edits or execution were performed by this audit. Read-only means launcher query/results/selection are not replaced; provider calls may have internal effects.

## Shared boundaries

| File / owner | Call lines | Role |
|---|---:|---|
| gui/search.rs::LauncherApp::search | 366,384 | Calls read-only owner, then installs root state |
| Same | 382 | Sole production without_providers caller, suppresses failed-provider reentry |
| gui/search.rs::LauncherApp::search_read_only | 818 | Compatibility wrapper returns outcome.actions |
| gui/universal_action_catalog.rs::LauncherApp::build_universal_action_catalog_snapshot | 903 | Queries crop/ss/fav/mkmacro/note/cs/cb for authoring snapshot; outer authoring_catalog_demand cache, carries capacity deferral |
| gui/radial_actions.rs::LauncherApp::prepare_radial | 365 | Sole production search_read_only caller for DynamicSource::LauncherQuery |
| gui/radial_actions.rs::LauncherApp::complete_authoring_provider_search | 1389 | Snapshot boundary after cancellation/retired-request rejection; checks provider revision and before/after/current catalogs; pending/stale Test retries, Preview remains pending; rechecks before execution/fallback |
| gui/radial_actions.rs::LauncherApp::complete_deferred_radial_search | 1754 | Snapshot boundary after cancellation/preparation identity; stale/pending replies remain pending; stable first result freezes, stable empty becomes original-query fallback; envelope/attempt correlated |

Internal composition into search_read_only_outcome_from_scored_plugins: search_read_only_outcome at search.rs668(capacity)/686(ordinary); without_providers708; with_plugin_snapshot735. No production direct boundary callers in radial_editor/action_editor.rs: previously observed calls at4212/4271 are in mod tests beginning2026.

## Direct mutable LauncherApp::search callers

All paths retain the existing cached-query early return and established replacement/provider-state/selection/root-generation behavior. Paths do not independently implement deferred-provider acceptance unless noted above.

| File / enclosing symbol | Call lines | Trigger |
|---|---:|---|
| gui/search.rs::maybe_run_note_search_debounce | 322 | Accepted note debounce |
| gui/search.rs::request_background_query_refresh | 409 | Immediate unless OCR defers |
| gui/render.rs::handle_query_text_changed | 1233 | Input/history query; note queries debounce |
| gui/render.rs::handle_macro_launcher_command | 1641 | Macro query, optional single-result activation |
| gui/render.rs::poll_macro_launcher_query | 1703 | Broker query, optional first-result activation |
| gui/render.rs::render_root_frame | 1880,1954,2292,2892 | Provider-generation refresh, pending query, periodic net refresh, result-menu refresh |
| gui/mod.rs::LauncherApp::new | 2548 | Initial search |
| gui/mod.rs::maybe_refresh_timer_list | 2573 | Timer/alarm refresh |
| gui/mod.rs::maybe_refresh_stopwatch_list | 2587 | Stopwatch refresh |
| gui/mod.rs::accept_suggestion | 2608 | Autocomplete query |
| gui/mod.rs::open_note_tags | 3976 | note tags query |
| gui/mod.rs::filter_notes_by_tag | 4020 | Tag query |
| gui/mod.rs::delete_note | 4117 | Affected note-list refresh |
| gui/command_host.rs::apply_command_outcome_with_root_policy | 1077 | Central search flag, including recursive query execution |
| gui/actions.rs::drain_clipboard_modify_immediate | 233 | Failed immediate operation restores query under root policy |
| gui/actions.rs::handle_clipboard_modify_gui_event | 269 | Configuration reload affecting cm query |
| gui/actions.rs::refresh_clipboard_modify_catalog | 302 | Catalog refresh affecting cm query |
| gui/universal_action_executor.rs::execute_universal_ui_intent | 600 | Successful clipboard-entry removal |
| gui/color_pick.rs::poll_color_pick | 152 | Accepted nonsuperseded color query |
| gui/note_mutation.rs::refresh_after_note_mutation | 149 | Committed mutation |
| gui/note_panel.rs::NotePanel::finish_save | 3890 | Successful panel save |
| gui/notes_dialog.rs::TemplateManagerState::ui | 722,755 | Template save/delete |
| gui/notes_dialog.rs::NotesDialog::save_note_draft | 1147 | New/existing/append save |
| gui/alias_dialog.rs::AliasDialog::ui | 47 | Folder alias save |
| gui/bookmark_alias_dialog.rs::BookmarkAliasDialog::ui | 46,62 | Bookmark alias save/remove |
| gui/add_bookmark_dialog.rs::AddBookmarkDialog::ui | 57 | Bookmark add |
| gui/tempfile_alias_dialog.rs::TempfileAliasDialog::ui | 43 | Tempfile alias save |
| gui/tempfile_dialog.rs::TempfileDialog::ui | 66 | Tempfile operation |
| gui/clipboard_dialog.rs::ClipboardDialog::save | 41 | Clipboard history save |
| gui/fav_dialog.rs::FavDialog::save | 119 | Favorite save |
| gui/macro_dialog.rs::MacroDialog::save | 100 | Macro save |
| gui/mouse_gestures_dialog.rs::MgGesturesDialog::save | 634 | Gesture save |
| gui/snippet_dialog.rs::SnippetDialog::finish_success | 691 | Snippet operation |
| gui/shell_cmd_dialog.rs::ShellCmdDialog::save | 59 | Shell-command save |
| gui/todo_dialog.rs::TodoDialog::save | 64 | Todo save |
| gui/todo_view_dialog.rs::TodoViewDialog::save | 205 | Todo edit |
| plugin_editor.rs::PluginEditor::save_settings | 168 | Settings/plugin reload |

Paths above are relative to src/. render_root_frame1880 compares plugins.search_generation with last_plugin_search_generation, invalidates and refreshes unless OCR defers.

## Indirect pinned/history/recursive routes

CommandHistoryWidget::render (dashboard/widgets/command_history.rs539) emits query_override; render_root_frame2583–2585 passes it to activate_action. activate_action (gui/actions.rs113) parses/dispatches; dispatch_command_invocation_with_history (gui/command_host.rs953) handles overrides/outcomes. commands/handlers/launcher_query.rs::handle_query37 maps Set to CommandOutcome::query and ExecuteFirst adds activation; commands/outcome.rs::CommandOutcome::query85 sets query/search flag. apply_command_outcome_with_root_policy applies query1056, searches1077, activates first1082 through recursion guard; allow_radial_queryexec_activation starts1148. universal_action_executor dispatch_universal_primary_command488 / dispatch_radial_primary510 carry captured history query and scoped radial recursion depth. execute_universal_ui_intent640 recomputes persisted pins through resolve_pin_action: current catalog/plugin/snapshot resolution, not an additional launcher search call.