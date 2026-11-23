# Launcher Result Context Menu

G05 owns `launcher-result-context-menu`. The root list and grid attach the same semantic popup to the exact rendered row.

| Criterion | Source-backed behavior |
| --- | --- |
| CONTEXT-MENU-01 | Resolve current universal actions with disabled reasons and retained Action/query context. Closing the menu precedes deferred typed dispatch. Preserve exact row ownership when scrolling, changing layouts or replacing a result. Radial authoring and live-query actions use separate typed routes. |

Select `root_list_context_menu_owner_stays_with_its_absolute_row_offscreen`, `root_grid_popup_owner_retains_complete_row_without_retargeting` and `context_menu_resolves_semantic_actions_with_list_grid_parity`.

`G05-EGUI-LIST-CONTEXT-EXACT-FAVORITE` verified List right-click targeting while a different row was selected, cancellation with Search and selection retained, and the Beta Favorites draft and save through the real form. `G05-EGUI-GRID-CONTEXT-EXACT-DRAFT` verified the corresponding visible Grid route, exact row 06 radial submenu and Favorites draft, and cancel without a store write. Offscreen popup retention and native GPUI remain pending. [Validation log](../VALIDATION_LOG.md#g05-result-context-menu-native-baseline)
