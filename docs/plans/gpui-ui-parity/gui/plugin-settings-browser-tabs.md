# Browser Tabs Plugin Settings

G24 owns `plugin-settings-browser-tabs`. Settings search or Plugins → Browser Tabs opens the built-in callback under the current Settings owner. The plugin stores its configuration under `browser_tabs`.

| Criterion | Source-backed behavior |
| --- | --- |
| BROWSER-TABS-SETTINGS-01 | Preserve Recalculate cache on each query, its default, warning text and currently applied cache policy. Keep configuration persistence in common Settings. Opening the callback performs no tab activation. |

Select `recalc_each_query_coalesces_many_consumer_filters_per_cache_epoch`, `populated_snapshot_filters_without_discovery` and `settings_search_catalog_covers_builtin_and_dedicated_control_inventory`. Native and GPUI remain pending.
