# Universal Action Sheet

G05 owns `universal-action-sheet`. Exact Ctrl+Enter from Search opens the current selected or sole result's action sheet when its input owner permits it.

| Criterion | Source-backed behavior |
| --- | --- |
| ACTION-SHEET-01 | Preserve independent filter, available and disabled reasons, exact keyboard selection with unavailable actions skipped, and pointer/keyboard origin. Accept closes and restores focus before typed dispatch. Escape only dismisses and preserves the query. |

Select `production_action_sheet_router_consumes_exact_ctrl_enter_and_preserves_tab`, `action_sheet_escape_preserves_query_and_does_not_activate_result` and `sheet_selection_preserves_keyboard_and_mouse_activation_sources`.

`G05-EGUI-ACTION-SHEET-HELP` verifies the actual Ctrl+Enter route, independent filter and Escape with Search focus. `G05-EGUI-ACTION-SHEET-EXECUTE` verifies the keyboard-filtered Execute action opens one Hash Generator and closing it restores the exact launcher query.

## First native GPUI action sheet checkpoint

Begin after the Quick Help and Favorites verification batch. Move the exact Ctrl+Enter binding from the foundation probe to live Launcher Search and keep the probe button and Hash's separate Calculate binding. Use `LauncherSearchResults::enter_target` for selected-or-sole admission. Capture the exact query, full Action, absolute row, result generation, live visibility revision and return focus. Use the existing universal resolver and registry for the Action Sheet surface. The first real destinations are installed typed primary commands and the Favorites draft companion.

Give the Kit dialog its own focus, filter and available-choice selection. Preserve effective labels, short labels, descriptions, registry order and disabled reasons for admitted actions. The filter searches labels, description and action ID. Plain Up, Down, Enter and Escape follow the egui contract, modified variants do not reach underlying Search, and Tab remains available for traversal. A nested input popup retains its own Escape. Close and restore live Search before deferred dispatch, and revalidate the exact request before it can execute once. Make query changes, hide, menu and dialog ownership, and restart retire stale requests. Preserve the current Search value, caret and selection during filtering and cancellation.

Type-check the app and test configurations with `fast-dev`. Add focused receipt, focus, modifier and navigation tests and include the existing egui router, Escape and source selectors. Native replay on the later linked artifact uses an owned Hash and Help profile with an absent Favorites store and both Paste Sequence shortcuts disabled. Exercise Favorites filter and Escape, a real Favorites draft and Cancel, then the filtered Execute action opening one Hash Generator. Verify one hash action, retained launcher query and scoped Calculate and Escape behavior. Keep unavailable destinations and geometry in their existing parity scope.

The first GPUI Action Sheet passed focused Rust and Windows replay for Ctrl+Enter, independent filtering, Shift+Enter, Escape, Shift+Tab to Close and both deferred destinations. A nested filter popup owned the first Escape. Minimize and restore retired the sheet and allowed a fresh open. The native global hotkey, modal title and accurate active focus in UI Automation, wider destinations and geometry remain open. Hash Escape is tracked in G04. [Batch evidence](../VALIDATION_LOG.md#g05-c-and-g06-action-sheet-and-favorites-browse-batch)

Execute admits checked saved Windows shortcuts, supported documents, literal HTTP(S) and verified local folders. Deferred dispatch uses the ordinary Search activation path after rechecking the sheet receipt. Registry and focused tests passed. Matched Windows replay is pending. [Checkpoint](../CHECKPOINTS.md#g05-checked-saved-associations-in-action-sheet-execute)
