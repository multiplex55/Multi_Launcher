# Clipboard and Snippets

G23 owns `dashboard-widget-clipboard_snippets`. The `ClipboardSnippetsWidget::settings_ui` callback edits this slot's draft configuration through the Dashboard Editor. Runtime actions require the accepted current slot and dashboard pass.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-CLIP-SNIP-01 | Retain independent counts and optional system snapshot. Use current stored rows and exact alias or clipboard identity. Keep hidden snippet bodies out of displayed text, tooltip and action wire. Explicit activation owns clipboard or validated prompt effects. |

Select `hidden_snippet_dashboard_preview_and_tooltip_do_not_expose_body`, `plain_and_prompted_snippet_rows_use_alias_identity_without_template_text` and `clipboard_history_action_preserves_index_and_clipboard_identity`. Use synthetic stores and a fake clipboard. Check history reorder before acceptance.
