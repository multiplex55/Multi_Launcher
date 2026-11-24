# Universal Action Sheet

G05 owns `universal-action-sheet`. Exact Ctrl+Enter from Search opens the current selected or sole result's action sheet when its input owner permits it.

| Criterion | Source-backed behavior |
| --- | --- |
| ACTION-SHEET-01 | Preserve independent filter, available and disabled reasons, exact keyboard selection with unavailable actions skipped, and pointer/keyboard origin. Accept closes and restores focus before typed dispatch. Escape only dismisses and preserves the query. |

Select `production_action_sheet_router_consumes_exact_ctrl_enter_and_preserves_tab`, `action_sheet_escape_preserves_query_and_does_not_activate_result` and `sheet_selection_preserves_keyboard_and_mouse_activation_sources`.

`G05-EGUI-ACTION-SHEET-HELP` verifies the actual Ctrl+Enter route, independent filter and Escape with Search focus. `G05-EGUI-ACTION-SHEET-EXECUTE` verifies the keyboard-filtered Execute action opens one Hash Generator and closing it restores the exact launcher query. GPUI remains pending.

## First native GPUI action sheet checkpoint

Begin after the Quick Help and Favorites verification batch. Move the exact Ctrl+Enter binding from the foundation probe to live Launcher Search and keep the probe button and Hash's separate Calculate binding. Use `LauncherSearchResults::enter_target` for selected-or-sole admission. Capture the exact query, full Action, absolute row, result generation, live visibility revision and return focus. Use the existing universal resolver and registry for the Action Sheet surface. The first real destinations are installed typed primary commands and the Favorites draft companion.

Give the Kit dialog its own focus, filter and available-choice selection. Preserve effective labels, short labels, descriptions, registry order and disabled reasons for admitted actions. The filter searches labels, description and action ID. Plain Up, Down, Enter and Escape follow the egui contract, modified variants do not reach underlying Search, and Tab remains available for traversal. A nested input popup retains its own Escape. Close and restore live Search before deferred dispatch, and revalidate the exact request before it can execute once. Make query changes, hide, menu and dialog ownership, and restart retire stale requests. Preserve the current Search value, caret and selection during filtering and cancellation.

Type-check the app and test configurations with `fast-dev`. Add focused receipt, focus, modifier and navigation tests and include the existing egui router, Escape and source selectors. Native replay on the later linked artifact uses an owned Hash and Help profile with an absent Favorites store and both Paste Sequence shortcuts disabled. Exercise Favorites filter and Escape, a real Favorites draft and Cancel, then the filtered Execute action opening one Hash Generator. Verify one hash action, retained launcher query and scoped Calculate and Escape behavior. Keep unavailable destinations and geometry in their existing parity scope.

The first GPUI Action Sheet is implemented and type-checked. The modal uses the shared filter and selection logic. Its enabled destinations use the existing installed Search dispatcher and checked Favorites draft owner. The native hide path retires its owned dialog and asynchronous Hash completion preserves the modal's focus. Batch tests and Windows replay remain pending, including actual modifier and Tab ownership, hide and reopen, and both deferred destinations.
