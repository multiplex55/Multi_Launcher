# Note Settings

G12 owns `plugin-settings-note` in the common Settings host. Feature effects stay with the existing runtime owner.

| Criterion | Source-backed behavior |
| --- | --- |
| NOTE-PLUGIN-SETTINGS-01 | Use the dedicated Notes section in the common Settings host. Preserve external opener configuration and Backlinks, Aliases and Templates. Keep current malformed JSON protected. The generic provider callback is skipped on the production route. |

Select `settings_search_cross_owner_inline_notes_focus_and_query_cancellation_preserve_editor_state`, `settings_search_delivery_preserves_malformed_notes_value` and `note_plugin_settings_survive_round_trip`. Do not launch an external editor in passive checks.

Native and GPUI remain pending.
