# Plugin Editor

G06 owns `plugin-editor`. File → Apps → Edit Plugins and accepted Settings search open the retained Plugin Editor. It can dock or pop out with the same session.

| Criterion | Source-backed behavior |
| --- | --- |
| PLUGIN-EDITOR-01 | Preserve current provider and capability filters, exact origin/epoch and catalog revision, and selection acknowledgement. Checkbox changes automatically queue the exact selection slice. Reject stale candidates without affecting unrelated settings. |

Select `newer_committed_plugin_selection_invalidates_staged_candidate`, `plugin_selection_intent_preserves_latest_other_settings_and_plugin_settings` and `hosted_plugin_native_filter_rejects_discarded_input_and_keeps_draft`.

`G06-EGUI-PLUGIN-APP-EDITORS` verifies the File menu route and retained editor. `G06-EGUI-PLUGIN-SELECTION-GPUI-SEARCH` verifies exact Settings Search navigation, a committed Hash Generator plugin change, the editor's own filter, and GPUI Search consuming the saved selection. `G06-EGUI-HASH-CAPABILITY-GPUI-SEARCH` verifies a separate capability edit with Hash still enabled. GPUI Search honored it and the dedicated Hash shortcut remained usable. A GPUI Plugin Editor remains pending.
