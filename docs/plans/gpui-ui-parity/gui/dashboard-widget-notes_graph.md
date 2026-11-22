# Notes Graph Widget

G23 owns `dashboard-widget-notes_graph`. The `NotesGraphWidget::settings_ui` callback owns this slot's configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-NOTES-GRAPH-01 | Preserve bounded graph, current note version, focus-controlled layout and exact slug links. The shared graph display is passive. Test changing the configured node cap against the cached link list. |

Use two generated linked Notes. Select `cached_graph_refreshes_only_when_notes_version_changes`.

Native and GPUI remain pending.
