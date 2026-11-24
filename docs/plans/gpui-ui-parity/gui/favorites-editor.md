# Favorites Editor

G06 owns `favorites-editor`. Bare `fav`, exact Add and Add to Favorites open the current browse/form owner. Prefill resolves an exact command and arguments before a label match.

| Criterion | Source-backed behavior |
| --- | --- |
| FAVORITES-EDITOR-01 | Preserve Label, Command, Args and provider choices without running them. Save validates and persists the checked draft before publication. Remove is immediate. Cancel, malformed data and pinned reopen preserve the applicable draft and last-good state. |

Select `favorites_pinned_open_loads_once_and_preserves_draft_until_close`, `invalid_reload_and_commit_keep_dialog_last_good_and_read_only` and `prefilled_add_preserves_legacy_favorite_fields`. `G06-EGUI-FAVORITES-SHELL-DRAFTS` verifies prefill and Cancel.

The initial egui baseline verified prefill and Cancel. The first checked Save and GPUI draft route are recorded below.

### Shared prefilled draft and checked save

Extract the renderer-neutral prefill and checked save used by Add to Favorites, then adopt it in the existing egui path before the GPUI companion. Resolve the first exact command and optional Args match, then first ASCII case-insensitive label, then append. A prefilled edit carries its original index and entry, and a prefilled append carries the original lookup key. The canonical `update_favs` operation checks that target against the latest data under its existing repair fence and transaction. A changed or moved entry and a newly matching append target report the stale draft. An accepted save preserves unrelated current entries, nonblank payload whitespace and the existing publication. Blank arguments retain their current normalization. The manual New/Edit/Remove paths retain their existing behavior and controls.

Review and type-check the adopted path and the cases for exact arguments, a latest unrelated entry, stale refusal and missing or malformed data. Run their targeted combination before the Windows Add to Favorites Save. Reuse the canonical result through the separate [GPUI List/Grid popup checkpoint](launcher-result-context-menu.md#first-native-result-context-menu-checkpoint). If implementation changes repair or publication ordering, verify that boundary before dependent source proceeds.

The shared draft and egui Save route passed scoped source review and the combined tests. The draft exposes label, command and arguments to both frontends and retains the captured origin inside the library. Windows verified the GPUI context-result draft, Cancel, Save, and egui reopening the exact canonical row. The remaining Favorites routes and authoring controls are still in scope for G06. [Batch evidence](../VALIDATION_LOG.md#g05-c-and-g06-quick-help-and-favorites-batch)

### Favorites browse from Search

After the Universal Action Sheet source freezes, route bare `fav` and exact `fav add` to the existing GPUI Favorites owner. The benchmark egui provider emits `fav:dialog:` for both. Reuse the canonical result construction and empty-label typed command, following the current Favorites plugin enablement and Search capability rules. Use a stateless installed provider for these routes and advertise the supported commands through the existing cache and Help collection.

Use one asynchronous canonical read path for browse and prefill. Browse opens without a draft and never writes. Preserve the current request identity, last-good rows, errors, active draft and acknowledged saves. Cancel during prefill retains the browse load. An ordinary Close retires a pending browse load. A fresh open reads the current file and Escape restores live Search focus and scroll.

Queue focused cases for exact query forms, command admission, provider gates and load receipts with the Action Sheet tests. In Windows use the owned Favorites-enabled fixture to activate both queries, observe the earlier saved row, and verify the exact Search query, focus and unchanged store hash. Later Favorites authoring evidence covers named Add, manual Add/Edit/Remove and provider choices.

The focused tests and Windows replay passed. GPUI opened the same saved row from `fav`, `fav add` and Action Sheet Execute. Reinvoking from Search reused the panel, and Escape returned to the current Search. Explicit egui showed the exact row from the same store with no Favorites write. [Batch evidence](../VALIDATION_LOG.md#g05-c-and-g06-action-sheet-and-favorites-browse-batch)

### Favorites authoring packet

1. Add checked manual append, exact-case named draft and exact existing-row Save and Remove to `plugins/fav.rs` through `update_favs`. Keep the repair permit, transaction order, atomic save and publication sequence. Manual Add permits duplicate labels. A retry targets the captured index and original row under the transaction, preserving unrelated entries. Move the real egui `FavDialog` paths to these operations using the path supplied to `ui_from`. Preserve stale draft and last-good read-only feedback and the checked result prefill.
2. Extract the provider picker operations used by the real egui form. Preserve category order, folder and bookmark lookup, label/description/command filtering, query and Args resolution, and unchanged Label on a pick. Choosing or saving author text does not execute it.
3. Route the named Favorites draft through installed Search and mount named drafts, Add Fav and exact-row Edit in the retained GPUI form. Use one canonical read, one typed draft and the existing acknowledged Save and restart ownership. Keep the context and action sheet prefilled paths on that same owner.
4. Add immediate exact-row Remove through the same write lane. Retain last-good rows and disable unsafe mutations when the store load or repair fence fails. Reopen against canonical data after recovery.
5. Give the GPUI form real Category, Filter and choice controls for the installed providers. Share authoring resolution with egui, preserve nested popup Escape and keep the provider list's authoring scope independent of launcher Search availability.

Type-check the five increments with `fast-dev` and run one focused Nextest and one linked Windows replay. Verify write and repair tests before native mutation. If the packet needs to change the repair permit, lock order, atomic save or publication, verify that shared transaction before dependent renderer code. Use owned rows with the same label and different Args for the stale and exact Remove checks. Replay native Add, named drafts, Edit, Remove, provider fill with no execution, and malformed read-only behavior, then reopen the exact saved store in explicit egui. Wider providers, a separate Data Recovery UI, and pinned presentation remain individual criteria.

The focused tests and corrected native replay passed. GPUI selected named drafts by exact case, preserved the other same-label row during Edit and Remove, canceled a blank Add, and picked and saved `hash:open` without running it. Explicit egui reopened the same four canonical entries. Native malformed Save retained the draft and last-good rows, and restoring and reopening the owned store restored authoring. The separate Favorites layout child resolved the original native stack overflow. [Batch evidence](../VALIDATION_LOG.md#g06-favorites-authoring-batch)

### Compact pane and saved pin packet

1. Derive the GPUI Favorites pane width and body budget from the actual logical client area and theme metrics. Keep Add Fav and Close reachable outside one retained scrollable Browse body. Wrap captions so the current row and Edit and Remove controls remain reachable. Keep last-good rows and long diagnostics usable at compact sizes. Launcher and Hash sizing retain their existing owners.
2. Use the same bounded pane for the form. Keep Label, Command, Args, Category, Filter and choices in the retained form body and Save and Cancel in the reachable footer. A fresh draft gets its own initial scroll position. Retain the current draft and scroll through ordinary rerender and resize. Keep nested Category Escape, exact renderer inputs and Save acknowledgement.
3. Read saved `Settings::pinned_panels` membership for `Panel::FavDialog` at GPUI startup and passively mount the retained Favorites owner once through the canonical load. Search keeps focus, root scroll stays put and no Settings write occurs. A pinned Browse resists ordinary Close and Escape. Cancel still leaves Browse mounted, and nested popup Escape still owns its event. Deliberate `fav` activation reveals the same owner and can reload a stale or unavailable pinned store after the active draft is canceled. Recovery guidance also gives a restart route for profiles with Favorites Search disabled.

Queue focused viewport, scroll transition, once-only mount and read, pinned dismiss and reload cases with `favorites_pinned_open_loads_once_and_preserves_draft_until_close`, `pinned_panel_prevents_close` and the relevant current receipts. Replay with owned long rows, both Paste Sequence shortcuts disabled, and pinned and ordinary profiles. Verify startup Search focus, reveal, a compact last row and both footers, draft and picker retention across a visibility transition, ordinary Close, and exact Settings and Favorites. The toolbar focus-or-unpin action needs the shared active-panel policy and existing field-scoped Settings write. Wider provider installation and Data Recovery keep separate owners.
