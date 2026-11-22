# Favorite Snippets

G23 owns `dashboard-widget-snippets_favorites`. The `SnippetsFavoritesWidget::settings_ui` callback owns this slot's configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-SNIP-FAV-01 | Preserve count and original order of applicable favorites, without exposing hidden snippet contents. Dispatch the exact full wire and args under the current accepted favorite. Copy and run remain explicit. |

Use inert synthetic favorites or a fake sink. Select `snippet_run_action_round_trips_alias_bytes_and_rejects_invalid_encoding` for the alias contract.

Native and GPUI remain pending.
