# Universal Action Sheet

G05 owns `universal-action-sheet`. Exact Ctrl+Enter from Search opens the current selected or sole result's action sheet when its input owner permits it.

| Criterion | Source-backed behavior |
| --- | --- |
| ACTION-SHEET-01 | Preserve independent filter, available and disabled reasons, exact keyboard selection with unavailable actions skipped, and pointer/keyboard origin. Accept closes and restores focus before typed dispatch. Escape only dismisses and preserves the query. |

Select `production_action_sheet_router_consumes_exact_ctrl_enter_and_preserves_tab`, `action_sheet_escape_preserves_query_and_does_not_activate_result` and `sheet_selection_preserves_keyboard_and_mouse_activation_sources`.

`G05-EGUI-ACTION-SHEET-HELP` verifies the actual Ctrl+Enter route, independent filter and Escape with Search focus. Action acceptance and GPUI remain pending.
