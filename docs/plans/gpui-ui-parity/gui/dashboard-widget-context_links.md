# Context Links

G23 owns `dashboard-widget-context_links`. `ContextLinksWidget` renders this registered slot from the current dashboard state.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-CONTEXT-01 | Combine current Note and Todo tags, show counts and retain bounded stable tag ordering. Actions fill only the exact Note or Todo tag query. There is no settings callback. |

Use synthetic tagged records. Select `note_list_supports_hash_and_at_tags` and `list_filters_by_tags_and_text` for downstream query semantics.

Native and GPUI remain pending.
