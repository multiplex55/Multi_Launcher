# Network Plugin Settings

G24 owns `plugin-settings-network`. The built-in callback exposes Network refresh interval and rate units through the common retained Settings host. The provider reads local counters.

| Criterion | Source-backed behavior |
| --- | --- |
| NETWORK-SETTINGS-01 | Preserve the 0.1–60 second refresh range, one-second default and Auto/B/s/kB/s/MB/s. Keep live provider application and save mapping for the current settings. Opening settings does not configure an interface. |

Select `search_returns_actions` and `search_no_panic_with_empty_history` in the network plugin, and `settings_search_catalog_covers_builtin_and_dedicated_control_inventory`. Native and GPUI remain pending.
