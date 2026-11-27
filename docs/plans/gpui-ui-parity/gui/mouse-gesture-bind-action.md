# Bind Mouse Gesture Action

G21 owns `mouse-gesture-bind-action`. `mg add` opens the binding editor from the root. Add Binding or Edit opens the same child for the exact retained gesture and binding. The directory route reveals the already published binding.

| Criterion | Source-backed behavior |
| --- | --- |
| GESTURE-BIND-01 | Preserve the private label, kind, action, query, arguments and enabled draft. Support Execute, Set Query, Set Query and Show, Set Query and Execute, and Toggle Launcher with the appropriate fields. Browsing and selecting an action update only the draft. |
| GESTURE-BIND-02 | Save checks required inputs, activation release, current retained host, parent session, child instance, stable row tokens and expected source. Reject stale replacement or deletion. Cancel leaves storage intact. Failed save restores the prior committed database. |

Select `every_dialog_variant_routes_through_the_typed_host`, `root_binding_on_empty_database_can_save_without_pending_rename`, `hosted_binding_uses_stable_tokens_and_rejects_replaced_source`, `native_mouse_binding_inherits_parent_and_keeps_its_session` and `jump_mode_mouse_gestures_binding_native_ids_follow_reorder`. `G21-EGUI-GESTURE-AUTHORING` verifies the actual child opens and Cancel returns to a gesture with no bindings. Save and GPUI remain pending.

## Checked GPUI Set Query binding

The retained gesture editor can save an exact Set Query binding while retaining other bindings and their enabled or unsupported state. The current receipt follows the shared command and reveal owners. The separate child host, other binding kinds, and native release and focus replay remain open. [Mouse Gestures checkpoint](../CHECKPOINTS.md#p018-checked-ordinary-mouse-gestures)
