# Stopwatch Plugin Settings

G23 owns `plugin-settings-stopwatch`. Settings search or Plugins → Stopwatch exposes the built-in callback. The dashboard Stopwatch widget and launcher result commands have distinct owners.

| Criterion | Source-backed behavior |
| --- | --- |
| STOPWATCH-SETTINGS-01 | Preserve retained precision 0–9 and refresh interval 0–5 seconds, current defaults and live capability. Editing calls the plugin's settings application path. Settings owns durable persistence. |

Select `settings_search_catalog_covers_builtin_and_dedicated_control_inventory`, `settings_search_cross_owner_builtin_provider_focuses_its_actual_response` and `stopwatch_paused_tracks_live_pause_state_and_missing_entries`. Native and GPUI remain pending.
