# Plugin Editor

G06 owns `plugin-editor`. File → Apps → Edit Plugins and accepted Settings search open the retained Plugin Editor. It can dock or pop out with the same session.

| Criterion | Source-backed behavior |
| --- | --- |
| PLUGIN-EDITOR-01 | Preserve current provider and capability filters, exact origin/epoch and catalog revision, and selection acknowledgement. Checkbox changes automatically queue the exact selection slice. Reject stale candidates without affecting unrelated settings. |

Select `newer_committed_plugin_selection_invalidates_staged_candidate`, `plugin_selection_intent_preserves_latest_other_settings_and_plugin_settings` and `hosted_plugin_native_filter_rejects_discarded_input_and_keeps_draft`.

`G06-EGUI-PLUGIN-APP-EDITORS` verifies the File menu route and retained editor. `G06-EGUI-PLUGIN-SELECTION-GPUI-SEARCH` verifies exact Settings Search navigation, a committed Hash Generator plugin change, the editor's own filter, and GPUI Search consuming the saved selection. `G06-EGUI-HASH-CAPABILITY-GPUI-SEARCH` verifies a separate capability edit with Hash still enabled. GPUI Search honored it and the dedicated Hash shortcut remained usable. The retained GPUI Search controls are implemented with focused and Windows verification pending.

## First GPUI Search capability checkpoint

Open a retained docked editor from File → Apps → Edit Plugins. Project installed GPUI providers and their Search capability from the existing catalog. Keep a filter and current provider identity. The installed catalog can be a subset of the full application, so global membership continues to use the full plugin editor and catalog owner. The Search control includes Google's explicit `g ` route even when Google is outside the ordinary membership list.

Use the canonical Settings writer to check the selected provider's current effective Search value. An omitted entry inherits Search. An explicit empty entry disables it. Save only the selected provider's capability change and preserve other providers, sibling capabilities, opaque Settings values and the exact global membership setting. The egui editor needs the same sparse-map semantics on display and mutation so reading one committed GPUI change cannot materialize unrelated entries.

Allow one pending change across the editor. Keep the restart permit until the registered worker and matching acknowledgement settle. Apply the committed map and refresh the current Search projection on success. Report stale input and store errors through the retained editor. Rebuilding Search must retire stale results and actions while preserving the direct Hash shortcut.

Use scoped fast-dev checks, one focused Nextest batch and one Windows artifact with both Paste Sequence shortcuts unset. Exercise the actual menu, filter, disable and re-enable a provider, explicit Google capability refusal, direct Hash shortcut and matched egui readback. Complete catalog membership editing, Settings Search routing, the native popout and provider-specific controls remain tracked by PLUGIN-EDITOR-01.
