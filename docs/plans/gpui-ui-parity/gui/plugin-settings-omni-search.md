# Omni Search Plugin Settings

G05 owns `plugin-settings-omni-search` in the common Settings host. Feature effects stay with the existing runtime owner.

| Criterion | Source-backed behavior |
| --- | --- |
| OMNI-SETTINGS-01 | Preserve the six default-enabled source groups for apps, notes, todos, calendar, folders and bookmarks. The callback owns a draft with existing malformed-input behavior. Save and provider reload apply the six flags to actual search. |

Select `settings_ui_reseeds_defaults_for_malformed_json` and `omni_results_keep_catalog_order_and_first_source_dedup`. Use synthetic catalog contents.

Native and GPUI remain pending.
