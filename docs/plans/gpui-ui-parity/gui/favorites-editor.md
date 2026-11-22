# Favorites Editor

G06 owns `favorites-editor`. Bare `fav`, exact Add and Add to Favorites open the current browse/form owner. Prefill resolves an exact command and arguments before a label match.

| Criterion | Source-backed behavior |
| --- | --- |
| FAVORITES-EDITOR-01 | Preserve Label, Command, Args and provider choices without running them. Save validates and persists the checked draft before publication. Remove is immediate. Cancel, malformed data and pinned reopen preserve the applicable draft and last-good state. |

Select `favorites_pinned_open_loads_once_and_preserves_draft_until_close`, `invalid_reload_and_commit_keep_dialog_last_good_and_read_only` and `prefilled_add_preserves_legacy_favorite_fields`. `G06-EGUI-FAVORITES-SHELL-DRAFTS` verifies prefill and Cancel.

Save and GPUI remain pending.
