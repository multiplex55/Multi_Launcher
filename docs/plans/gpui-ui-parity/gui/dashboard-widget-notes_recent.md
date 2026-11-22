# Notes Recent

G23 owns `dashboard-widget-notes_recent`. The `NotesRecentWidget::settings_ui` callback owns this slot's configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-NOTES-RECENT-01 | Preserve count, snippet/tags, current cache order, exact slug and alias/title display. Keep its Panel open behavior. Existing byte-indexed snippet truncation needs Unicode verification even with snippet display off. |

Use one synthetic short Note. Select `note_cache_versioned_snapshot_pairs_revision_and_notes_during_publication` and `jump_mode_notes_rows_use_visible_slug_identity_and_fresh_enter_opens_exact_note`.

Native and GPUI remain pending.
