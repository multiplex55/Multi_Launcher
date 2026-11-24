# Favorites Editor

G06 owns `favorites-editor`. Bare `fav`, exact Add and Add to Favorites open the current browse/form owner. Prefill resolves an exact command and arguments before a label match.

| Criterion | Source-backed behavior |
| --- | --- |
| FAVORITES-EDITOR-01 | Preserve Label, Command, Args and provider choices without running them. Save validates and persists the checked draft before publication. Remove is immediate. Cancel, malformed data and pinned reopen preserve the applicable draft and last-good state. |

Select `favorites_pinned_open_loads_once_and_preserves_draft_until_close`, `invalid_reload_and_commit_keep_dialog_last_good_and_read_only` and `prefilled_add_preserves_legacy_favorite_fields`. `G06-EGUI-FAVORITES-SHELL-DRAFTS` verifies prefill and Cancel.

Save and GPUI remain pending.

### Shared prefilled draft and checked save

Extract the renderer-neutral prefill and checked save used by Add to Favorites, then adopt it in the existing egui path before the GPUI companion. Resolve the first exact command and optional Args match, then first ASCII case-insensitive label, then append. A prefilled edit carries its original index and entry, and a prefilled append carries the original lookup key. The canonical `update_favs` operation checks that target against the latest data under its existing repair fence and transaction. A changed or moved entry and a newly matching append target report the stale draft. An accepted save preserves unrelated current entries, nonblank payload whitespace and the existing publication. Blank arguments retain their current normalization. The manual New/Edit/Remove paths retain their existing behavior and controls.

Review and type-check the adopted path and the cases for exact arguments, a latest unrelated entry, stale refusal and missing or malformed data. Run their targeted combination before the Windows Add to Favorites Save. Reuse the canonical result through the separate [GPUI List/Grid popup checkpoint](launcher-result-context-menu.md#first-native-result-context-menu-checkpoint). If implementation changes repair or publication ordering, verify that boundary before dependent source proceeds.

The shared draft and egui Save route passed the `fast-dev` library check and scoped source review. The draft exposes label, command and arguments to both frontends and retains the captured origin inside the library. The exact-argument, latest-entry, stale and malformed-store tests and egui Save replay remain queued for the batch. [Checkpoint ledger](../CHECKPOINTS.md#resume-after-agent-configuration-update)
