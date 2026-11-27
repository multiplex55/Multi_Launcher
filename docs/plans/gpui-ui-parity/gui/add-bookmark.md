# Add Bookmark

G06 owns `add-bookmark`. Bare `bm` and exact `bm add` open the typed bookmark draft. A direct URL argument belongs to the separate storage action.

| Criterion | Source-backed behavior |
| --- | --- |
| BOOKMARK-FORM-01 | Preserve fresh URL and alias, blank validation, normalized atomic add/update, feedback and intact draft on failure. Accepted save refreshes Search. Draft and Cancel do not launch a browser. |

Select `typed_simple_dialogs_preserve_interactive_lifecycle`, `every_simple_dialog_routes_through_the_typed_host` and `add_with_alias_updates_existing_entry_in_one_transaction`.

`G06-EGUI-BOOKMARK-DRAFTS` verifies the native egui `bm add` draft, URL editing and Cancel with the existing bookmark bytes preserved. GPUI now presents the retained URL and Alias form from `bm` and `bm add`. Guarded Save uses stock normalization, preserves the authored draft on a refused write, and refreshes affected Search. A typed URL uses the same canonical owner without the form. Matched Windows authoring and Cancel replay remain open. [Bookmark checkpoint](../CHECKPOINTS.md#p004-canonical-bookmark-authoring)
