# Plugin Editor

G06 owns `plugin-editor`. File → Apps → Edit Plugins and accepted Settings search open the retained Plugin Editor. It can dock or pop out with the same session.

| Criterion | Source-backed behavior |
| --- | --- |
| PLUGIN-EDITOR-01 | Preserve current provider and capability filters, exact origin/epoch and catalog revision, and selection acknowledgement. Checkbox changes automatically queue the exact selection slice. Reject stale candidates without affecting unrelated settings. |

Select `newer_committed_plugin_selection_invalidates_staged_candidate`, `plugin_selection_intent_preserves_latest_other_settings_and_plugin_settings` and `hosted_plugin_native_filter_rejects_discarded_input_and_keeps_draft`.

Native and GPUI remain pending.
