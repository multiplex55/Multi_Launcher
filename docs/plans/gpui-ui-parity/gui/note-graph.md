# Note Graph

G12 owns `note-graph`. The Notes plugin accepts `note graph` or `notes graph` with optional tag and root slug, and emits `note:graph_dialog`. `note_graph_dialog.rs` owns the root utility window, camera and selection. `NoteGraphEngine` owns graph and layout, fed by the versioned dashboard notes snapshot. `Settings.note_graph` owns the saved graph options.

| Criterion | Source-backed behavior |
| --- | --- |
| NOTE-GRAPH-01 | Search title, alias and slug, show up to eight suggestions, and select or center the exact node. Preserve Global/Local, root prefill, Local Depth 1–3 and include/exclude tag controls. Preserve Any/All, Orphans only and Only tagged options. |
| NOTE-GRAPH-02 | Preserve pause/reset simulation, render cap and truncation, labels and zoom threshold, repulsion, link distance and iteration settings. The camera, selection, scope, pause and Details are session state. |
| NOTE-GRAPH-03 | Canvas wheel zoom, background pan, node drag/pin, click selection, double-click and keyboard Open use the current graph identity. Details shows tags and outgoing/backlinks with center/open actions and reachable controls on bounded scroll. Open routes to the full [Note Editor](note-editor.md). |
| NOTE-GRAPH-04 | Preserve remembered dimensions and saved graph options. Route tags and root can override saved defaults for the new session. The current graph rebuild key covers the notes version and normalized graph filter. The Any-tag and Only tagged prefilters can remain stale until that key changes. |

Select `note_graph_returns_dialog_action`, `note_graph_tag_prefilter_parses_hash_and_at`, `note_graph_dialog_action_opens_dialog`, `open_with_args_queues_prefilter_and_root_mode`, `search_ranks_slug_and_alias_matches`, `local_scope_depth_traversal_limits_nodes`, `long_details_scroll_keeps_actions_reachable_and_state_stable` and `settings_deserialize_backward_compatible_for_note_graph_fields`. `G12-EGUI-GRAPH-ASSETS` covers actual tag/root prefill, two-node Details navigation and opening the exact linked note with unchanged sources. Canvas gestures, other options and GPUI remain pending.
