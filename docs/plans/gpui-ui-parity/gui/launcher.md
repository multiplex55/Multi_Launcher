# Launcher

G05 owns `launcher`, the root query and result workflow. The source baseline is `98bbc0ccc22a297e8eafae7e844c2f7e45854856`. G00-C exercised one egui route on an identified artifact. The launcher remains unimplemented in GPUI.

## Routes and ownership

`src/main.rs::spawn_gui` creates the egui root and binds it to the native invocation and foreground services. `gui/render.rs` owns the root query, result activation, dashboard admission and menu controls. `gui/search.rs` owns query and result state transitions. Reuse the typed command bus, existing ranking and plugin operations, and the command outcome contract. The GPUI shell from G01 proves event-loop and input feasibility. It has no result model or action dispatch.

`launcher-result-context-menu`, `launcher-action-confirmation`, `universal-action-sheet`, `jump-mode-overlay`, `quick-help` and `command-help` are connected surfaces. G05 owns the route, payload, focus and dismissal contract for each. Utility targets, the Dashboard and authorship/settings destinations use the surface owners in [GUI inventory](../GUI_INVENTORY.md).

## Parity criteria

| Criterion | Source-backed behavior |
| --- | --- |
| LAUNCH-01 | Initial launch and configured invocation give the root the expected visibility and query focus. Preserve toggle, hide, reopen, foreground capture and restoration policy. A transient modal, action sheet or exclusive overlay owns its keyboard input. Independently hosted utility windows do not acquire root keyboard authority. |
| LAUNCH-02 | Search is editable with selection/caret and ordinary text shortcuts. A query edit updates the intended session exactly once. Keep the empty dashboard/list behavior, busy and empty-result states, and existing invalidation and stale-result rules. Root query can remain eligible with an ordinary utility open when the accepted focus session permits it. |
| LAUNCH-03 | Preserve ordinary list and grid modes, ranking, metadata, result labels/descriptions, selected state, scrolling and action identity. Large result sets use bounded visible rows. Filtering or scrolling does not retarget a context action. |
| LAUNCH-04 | Up/Down/Page keys move selection. Left/Right in grid mode navigate results. Unmodified physical NumPad2/4/6/8 retain their navigation meaning across Num Lock. The paired digit immediately following an accepted keypad event is consumed while ordinary digits and modified editor events survive. List-mode keypad left/right return to query or enter results. |
| LAUNCH-05 | Ctrl+Up/Down traverse query history only with accepted query focus. Preserve the draft and stop boundaries. Tab accepts the current completion under existing conditions. Offer suggestions only when enabled, available and outside dashboard mode. |
| LAUNCH-06 | Enter, pointer selection and the current context actions dispatch the same typed payload and apply query/results/history/visibility/focus/restore/caret and feedback policies. Preserve generic confirmation, its owner generation, and exactly-once action effects. |
| LAUNCH-07 | The result context menu shows action groups and disabled reasons from the universal action catalog. Add to Radial distinguishes primary and secondary actions, explains unavailable bindings and can save the non-empty live query. The action sheet keeps its own search, selection and keyboard ownership while open. |
| LAUNCH-08 | File → Apps offers Edit Apps, Edit Plugins and Edit Radial Menus. File also routes Pin List, Activity and Close Application. Settings opens or requests close of Edit Settings. Help includes Command List, Linking Guide with the existing filter, Quick Help Overlay, Open Toast Log externally and View Toast Log. Pinned panel controls focus or unpin the exact panel. |
| LAUNCH-09 | Preserve visible focus, Jump Mode navigation and popup precedence, handling of Alt+Space, ordinary versus pinned utility closing, topmost policy and resize/scale behavior. Keep framework widget memory isolated from semantic preferences. |

## Source and verification

Use `gui/query_navigation.rs`, `native_keyboard.rs`, `gui/render.rs`, `gui/actions.rs`, `gui/command_host.rs` and `help_window.rs` for implementation anchors. The G00-C receipt in [validation log](../VALIDATION_LOG.md) records Search text input, Down/Enter, opening Hash Generator and a persisted popout change in a scratch root. Other states need baseline evidence at G05 and their named owner checkpoints.

Current focused library tests include `keyboard_navigation_is_consistent_between_grid_and_list_modes` and `root_list_viewport_builds_bounded_rows_with_absolute_ids_and_selection`. Add the affected target in `focus_visibility`, `gui_visibility` or grouped `pure` query autocomplete coverage only when its contract changes. Native evidence must exercise global invocation, editability, physical numpad, window focus, dismissal and handoffs through both selected frontends. Compare representative list/grid and compact layouts under equivalent settings. G05 owns the exact component mapping and the remaining control inventory before the port.
