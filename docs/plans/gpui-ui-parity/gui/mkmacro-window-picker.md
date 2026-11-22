# MkMacro Window Picker

G19 owns `mkmacro-window-picker`. Macro Active Window Hotkey Choose Window or a current [Action Editor](mkmacro-action-editor.md) matcher field stages the exact typed request. The retained picker owns current list/search, selection, preview and host. The macro draft owns the persisted matcher.

| Criterion | Source-backed behavior |
| --- | --- |
| MK-WINDOW-PICKER-01 | Browse and filter current title/exe/class/path, Refresh and Choose selected. Windows Pick From Screen reads current pointer/window identity. Preview editable criteria uses current exe basename, literal title/class or valid title regex. Require at least one active nonblank criterion. |
| MK-WINDOW-PICKER-02 | Use Target applies once through exact retained picker and initiating draft. Verify macro ID, step/field path, generation and original matcher. Close/Cancel/Back preserves the matcher. Only typed matcher criteria become draft content. |
| MK-WINDOW-PICKER-03 | Preserve stable request and allowed host across Dock/Pop Out/inherited opening. Refresh retires old selection. Reject stale or invalid owner. This picker selects criteria without activating the window or invoking a macro. |

Select `macro_hotkey_picker_inherits_retained_parent_and_rejects_retired_origin`, `picker_confirmation_rejects_changed_original_matcher`, `picker_confirmation_targets_stable_macro_id_after_selection_changes`, `window_picker_routes_root_nested_and_image_matchers`, `refresh_replaces_snapshot_and_invalidates_selection`, `validation_and_handle_never_persist` and `screen_poll_cancel_and_failures_are_non_destructive`. Existing fake window candidates support isolated presentation and cancel. Native and GPUI remain pending.
