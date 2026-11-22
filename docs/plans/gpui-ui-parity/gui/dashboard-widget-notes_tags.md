# Notes Tags

G23 owns `dashboard-widget-notes_tags`. The `NotesTagsWidget::settings_ui` callback owns this slot's configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-NOTES-TAGS-01 | Preserve count, optional totals, current Note cache, tag frequency order and the exact tag query. Activating a tag fills the Note list query without editing a note. |

Use generated tagged Notes. Select `note_cache_publishes_alias_title_body_tag_and_backlink_changes_once`, `note_list_supports_hash_and_at_tags` and `note_tag_lists_tags_and_drills_into_list`.

Native and GPUI remain pending.
